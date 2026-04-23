//! Canonical tar.gz archive construction.
//!
//! Implements the Spec 2 Reproducibility rules for derived-artifact bytes.
//! Given identical inputs and the same SDK version, [`canonical_tar_gz`]
//! produces byte-identical output on every machine and every run.
//!
//! # Canonicalization rules implemented
//!
//! Per Spec 2 Reproducibility → "Archive canonicalization (tar.gz)":
//!
//! 1. **Tar format**: `ustar` (via [`tar::Header::new_ustar`]). Explicitly not
//!    GNU.
//! 2. **Entry ordering**: every entry sorted by archive path in lexicographic
//!    UTF-8 byte order.
//! 3. **mtime**: `0` on every entry.
//! 4. **UID / GID**: `0` / `0`. Owner and group name fields: empty strings.
//! 5. **File mode**: regular files → `0644`; exec-bit-on-disk files → `0755`.
//!    Directories are not included as separate entries (tar extraction
//!    auto-creates parents), so per-directory mode doesn't apply.
//! 6. **PAX extended headers**: none. Paths whose archive representation
//!    exceeds ustar's 255-byte split-path limit are rejected at package
//!    time with [`SdkError::Archive`].
//! 7. **Gzip header timestamp**: `0`.
//! 8. **Original filename header**: omitted (FNAME flag not set; no
//!    `GzBuilder::filename()` call).
//!
//! # File exclusion
//!
//! Per the plan's file-exclusion decision (v1 scope), these patterns are
//! skipped during walk: `target/`, `.git/`, `__pycache__/`, `*.pyc`. A more
//! configurable mechanism (`.pluginignore` or a manifest `plugin.files` list)
//! is explicitly post-v1.
//!
//! # Compression
//!
//! `flate2` with the `rust_backend` feature pins `miniz_oxide` as the gzip
//! encoder — different backends (system zlib, zlib-ng, cloudflare_zlib)
//! produce byte-different gzip streams at the same compression level and
//! would break determinism. Compression level is fixed at 6 via
//! [`flate2::Compression::new`].
//!
//! # Cross-platform determinism caveat
//!
//! Spec 2 Reproducibility lists "filesystem executable bit" as an input
//! contributing to output bytes. Unix filesystems report the exec bit;
//! Windows has no Unix-style exec bit. A plugin directory containing a
//! `chmod +x` file packaged on Unix produces an archive with a 0755 entry
//! for that file; the same directory packaged on Windows produces a 0644
//! entry. **Byte-identity across operating systems is not guaranteed for
//! plugins that carry executable files.** Plugins that bundle no exec
//! files are byte-identical across platforms.
//!
//! # Directory entries
//!
//! Directories are intentionally omitted from the archive — tar extraction
//! creates parent directories automatically when needed. Consequence:
//! extracted directory modes are umask-dependent at `tar xf` time rather
//! than pinned by the archive. Spec 2 Reproducibility's "directories →
//! 0755" rule refers to dir-entry mode IF emitted; this implementation
//! chooses not to emit them. Plugin-runtime install (Spec 4) creates
//! `plugin_dir/<name>/<version>/` directly via the DB and does not rely
//! on tar's extracted directory modes.

use flate2::{Compression, GzBuilder};
use influxdb3_plugin_schemas::PluginName;
use semver::Version;
use std::path::{Component, Path, PathBuf};

use crate::SdkError;

/// Joins a relative path's components with `/` for use as a tar entry path.
///
/// Tar archives canonically use `/` as the separator. On Windows,
/// `Path::display()` emits `\` between components, which would produce
/// malformed archive paths. This helper explicitly enumerates components
/// and joins them with `/`, producing correct output on every platform.
///
/// Assumes the input is a normalized relative path — no root, no parent
/// (`..`) components. The archive pipeline builds these from walkdir entries
/// stripped of the plugin-dir prefix, so this precondition holds.
fn to_archive_path(relative: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    for component in relative.components() {
        if let Component::Normal(os) = component {
            parts.push(os.to_string_lossy().into_owned());
        }
    }
    parts.join("/")
}

/// Packages `plugin_dir` into a canonical gzipped tar archive.
///
/// The archive's top-level directory is `{name}-{version}/`; all files under
/// `plugin_dir` are placed beneath it, preserving their relative paths.
///
/// Returns the archive bytes. Consumers are expected to feed them into
/// [`crate::hash::sha256_of_bytes`] and write them out; this function
/// performs no file I/O on the output.
pub fn canonical_tar_gz(
    plugin_dir: &Path,
    name: &PluginName,
    version: &Version,
) -> Result<Vec<u8>, SdkError> {
    // Collect + sort relative paths of files to include.
    let entries = collect_entries(plugin_dir)?;

    // Archive root prefix: `{name}-{version}/`.
    let archive_root = format!("{}-{}", name.as_str(), version);

    // Reject any archive path that exceeds ustar's 255-byte limit (100 name +
    // 155 prefix, with a split required at a '/'). tar::Header::set_path also
    // errors in this case but we surface a domain-typed error earlier.
    for entry in &entries {
        let archive_path = format!("{}/{}", archive_root, to_archive_path(&entry.relative));
        if archive_path.len() > 255 {
            return Err(SdkError::Archive {
                message: format!(
                    "archive path {archive_path:?} exceeds ustar's 255-byte limit; \
                     shorten file paths or the plugin name/version"
                ),
            });
        }
    }

    // Build the compressed tar in memory.
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let gz = GzBuilder::new()
        .mtime(0) // canonical: gzip MTIME = 0
        .write(&mut buf, Compression::new(6));
    let mut tarball = tar::Builder::new(gz);

    for entry in entries {
        let archive_path = format!("{}/{}", archive_root, to_archive_path(&entry.relative));
        let data = std::fs::read(&entry.absolute).map_err(|source| SdkError::Io {
            source,
            path: Some(entry.absolute.clone()),
        })?;

        let mut header = tar::Header::new_ustar();
        header.set_size(data.len() as u64);
        header.set_mode(if entry.is_exec { 0o755 } else { 0o644 });
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
        header.set_username("").map_err(|e| SdkError::Archive {
            message: e.to_string(),
        })?;
        header.set_groupname("").map_err(|e| SdkError::Archive {
            message: e.to_string(),
        })?;
        header.set_entry_type(tar::EntryType::Regular);
        // `append_data` calls `set_path` (which invalidates any prior
        // checksum) then `set_cksum` itself, so we don't precompute here.

        tarball
            .append_data(&mut header, &archive_path, std::io::Cursor::new(data))
            .map_err(|e| SdkError::Archive {
                message: format!("appending {archive_path:?}: {e}"),
            })?;
    }

    // Finalize the tar stream inside the gz encoder, then the gz encoder
    // itself. Both must finish for the bytes to be complete.
    let gz_encoder = tarball.into_inner().map_err(|e| SdkError::Archive {
        message: e.to_string(),
    })?;
    gz_encoder.finish().map_err(|e| SdkError::Archive {
        message: e.to_string(),
    })?;
    Ok(buf)
}

struct Entry {
    absolute: PathBuf,
    relative: PathBuf,
    is_exec: bool,
}

fn collect_entries(plugin_dir: &Path) -> Result<Vec<Entry>, SdkError> {
    let plugin_dir = std::fs::canonicalize(plugin_dir).map_err(|source| SdkError::Io {
        source,
        path: Some(plugin_dir.to_path_buf()),
    })?;

    let mut entries = Vec::new();
    // `follow_links(false)` makes walkdir report symlinks with their own
    // file_type (is_symlink, not is_file/is_dir). The filter below skips
    // anything that isn't a regular file, so symlinks are excluded from
    // the archive — plugins shouldn't rely on them.
    let walk = walkdir::WalkDir::new(&plugin_dir)
        .sort_by_file_name() // stable walk order (we re-sort by archive path below regardless)
        .follow_links(false);

    for result in walk {
        let entry = result.map_err(|e| SdkError::Archive {
            message: format!("walkdir error: {e}"),
        })?;
        let absolute = entry.path().to_path_buf();
        // Skip directories — we only archive files. Tar extraction
        // auto-creates parent directories.
        if entry.file_type().is_dir() {
            continue;
        }
        // Skip non-files (symlinks, sockets, etc.). Plugins are source code
        // directories; unusual file types are suspicious and excluded.
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = absolute
            .strip_prefix(&plugin_dir)
            .map_err(|e| SdkError::Archive {
                message: format!("path outside plugin_dir: {e}"),
            })?
            .to_path_buf();
        if should_exclude(&relative) {
            continue;
        }
        let is_exec = is_executable(&absolute).map_err(|source| SdkError::Io {
            source,
            path: Some(absolute.clone()),
        })?;
        entries.push(Entry {
            absolute,
            relative,
            is_exec,
        });
    }

    // Canonical order: lexicographic byte order on the archive path.
    // `as_encoded_bytes()` returns WTF-8 on Windows and UTF-8 on Unix; for
    // ASCII paths — which plugin files always are in practice — the two are
    // byte-identical. Sorting by relative paths (rather than full archive
    // paths) is equivalent because the `archive_root` prefix is identical
    // for every entry.
    entries.sort_by(|a, b| {
        a.relative
            .as_os_str()
            .as_encoded_bytes()
            .cmp(b.relative.as_os_str().as_encoded_bytes())
    });

    Ok(entries)
}

fn should_exclude(relative: &Path) -> bool {
    // Exclude: any path component named `target`, `.git`, or `__pycache__`;
    // any filename ending in `.pyc`. These match the author-dev-detritus
    // patterns called out in the plan.
    for component in relative.components() {
        if let Some("target" | ".git" | "__pycache__") = component.as_os_str().to_str() {
            return true;
        }
    }
    relative
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| ext == "pyc")
}

#[cfg(unix)]
fn is_executable(path: &Path) -> std::io::Result<bool> {
    use std::os::unix::fs::PermissionsExt;
    Ok(std::fs::metadata(path)?.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> std::io::Result<bool> {
    // Windows has no Unix-style exec bit. Every file ships as 0644.
    Ok(false)
}

#[cfg(test)]
mod archive_path_tests {
    use super::to_archive_path;
    use std::path::PathBuf;

    #[test]
    fn single_component_returns_component_string() {
        assert_eq!(
            to_archive_path(&PathBuf::from("manifest.toml")),
            "manifest.toml"
        );
    }

    #[test]
    fn nested_components_joined_with_forward_slash() {
        let p: PathBuf = ["a", "b", "c.py"].iter().collect();
        assert_eq!(to_archive_path(&p), "a/b/c.py");
    }

    #[test]
    fn empty_path_returns_empty_string() {
        assert_eq!(to_archive_path(&PathBuf::new()), "");
    }

    /// Even a path component containing a literal backslash byte (which is a
    /// valid filename byte on Unix) must produce a forward-slash-separated
    /// archive path. `to_archive_path` operates on normalized components and
    /// must not special-case any byte value.
    ///
    /// Unix-only: Windows parses `\` as a path separator, so the same input
    /// would split into different components and this test would spuriously
    /// fail against a correctly-behaving `to_archive_path`.
    #[cfg(unix)]
    #[test]
    fn backslash_byte_in_component_does_not_leak_into_archive_path() {
        // Unix: `PathBuf::from("sub\\leaf")` is one component whose name
        // contains a literal `\` byte.
        let p = PathBuf::from("sub\\leaf");
        let result = to_archive_path(&p);
        assert_eq!(result, "sub\\leaf", "single component preserved verbatim");
        assert!(!result.contains('/'), "single component must not introduce `/`");

        // Multi-component path with a backslash in one component: the component
        // separator is `/`, component content is preserved byte-for-byte.
        // On Unix, `PathBuf::from_iter` treats each string as a single
        // component and backslashes are ordinary filename bytes, so the middle
        // component's name is literally `sub\leaf` (8 bytes including the
        // backslash). `to_archive_path` joins the three components with `/`,
        // yielding the bytes `a/sub\leaf/c`.
        let p: PathBuf = ["a", "sub\\leaf", "c"].iter().collect();
        let result = to_archive_path(&p);
        assert_eq!(
            result, "a/sub\\leaf/c",
            "backslash byte inside a component is preserved; `/` only appears as component separator"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn minimal_plugin_dir(base: &Path) -> PathBuf {
        let dir = base.join("plugin");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("manifest.toml"),
            "manifest_schema_version = \"1.0\"\n\n[plugin]\nname = \"p\"\nversion = \"0.1.0\"\ndescription = \"x\"\ntriggers = [\"process_writes\"]\n\n[dependencies]\ndatabase_version = \">=3.0.0\"\n",
        )
        .unwrap();
        fs::write(
            dir.join("__init__.py"),
            "def process_writes(a, b, c):\n    pass\n",
        )
        .unwrap();
        dir
    }

    fn name() -> PluginName {
        "p".parse().unwrap()
    }

    fn version() -> Version {
        Version::new(0, 1, 0)
    }

    #[test]
    fn builds_deterministic_bytes_across_calls() {
        let td = tempfile::tempdir().unwrap();
        let dir = minimal_plugin_dir(td.path());
        let a = canonical_tar_gz(&dir, &name(), &version()).unwrap();
        let b = canonical_tar_gz(&dir, &name(), &version()).unwrap();
        assert_eq!(a, b, "same inputs must produce byte-identical output");
    }

    #[test]
    fn skips_excluded_paths() {
        let td = tempfile::tempdir().unwrap();
        let dir = minimal_plugin_dir(td.path());
        fs::create_dir_all(dir.join("target")).unwrap();
        fs::write(dir.join("target/noise"), "ignore me").unwrap();
        fs::create_dir_all(dir.join("__pycache__")).unwrap();
        fs::write(dir.join("__pycache__/foo.pyc"), "junk").unwrap();
        fs::write(dir.join("compiled.pyc"), "also junk").unwrap();

        let bytes = canonical_tar_gz(&dir, &name(), &version()).unwrap();
        let listing = list_tar_paths(&bytes);
        for entry in &listing {
            assert!(!entry.contains("/target/"), "unexpected target/: {entry}");
            assert!(
                !entry.contains("/__pycache__/"),
                "unexpected __pycache__/: {entry}"
            );
            assert!(!entry.ends_with(".pyc"), "unexpected .pyc: {entry}");
        }
    }

    #[test]
    fn entries_sorted_by_archive_path() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("plugin");
        fs::create_dir_all(&dir).unwrap();
        // Create in out-of-order sequence; the archive must still be sorted.
        fs::write(
            dir.join("manifest.toml"),
            "manifest_schema_version = \"1.0\"\n\n[plugin]\nname = \"p\"\nversion = \"0.1.0\"\ndescription = \"x\"\ntriggers = [\"process_writes\"]\n\n[dependencies]\ndatabase_version = \">=3.0.0\"\n",
        )
        .unwrap();
        fs::write(dir.join("__init__.py"), "def process_writes():\n    pass\n").unwrap();
        fs::write(dir.join("zebra.py"), "# z\n").unwrap();
        fs::write(dir.join("alpha.py"), "# a\n").unwrap();

        let bytes = canonical_tar_gz(&dir, &name(), &version()).unwrap();
        let listing = list_tar_paths(&bytes);
        let without_root: Vec<String> = listing
            .iter()
            .map(|p| p.trim_start_matches("p-0.1.0/").to_owned())
            .collect();

        assert_eq!(
            without_root,
            vec!["__init__.py", "alpha.py", "manifest.toml", "zebra.py"],
            "entries must be in lexicographic UTF-8 byte order"
        );
    }

    #[test]
    fn tar_format_is_ustar() {
        let td = tempfile::tempdir().unwrap();
        let dir = minimal_plugin_dir(td.path());
        let bytes = canonical_tar_gz(&dir, &name(), &version()).unwrap();
        let tar_bytes = gunzip(&bytes);
        // ustar magic is at offset 257 in the first header block: "ustar\0"
        // followed by version "00".
        let magic = &tar_bytes[257..263];
        assert_eq!(magic, b"ustar\0", "expected ustar magic; got {magic:?}");
        let version = &tar_bytes[263..265];
        assert_eq!(version, b"00", "expected ustar version 00; got {version:?}");
    }

    #[test]
    fn every_entry_mtime_is_zero() {
        let td = tempfile::tempdir().unwrap();
        let dir = minimal_plugin_dir(td.path());
        let bytes = canonical_tar_gz(&dir, &name(), &version()).unwrap();
        let mut archive = tar::Archive::new(std::io::Cursor::new(gunzip(&bytes)));
        for entry in archive.entries_with_seek().unwrap() {
            let entry = entry.unwrap();
            let mtime = entry.header().mtime().unwrap();
            assert_eq!(mtime, 0, "expected mtime=0; got {mtime}");
        }
    }

    #[test]
    fn every_entry_uid_gid_and_names_canonical() {
        let td = tempfile::tempdir().unwrap();
        let dir = minimal_plugin_dir(td.path());
        let bytes = canonical_tar_gz(&dir, &name(), &version()).unwrap();
        let mut archive = tar::Archive::new(std::io::Cursor::new(gunzip(&bytes)));
        for entry in archive.entries_with_seek().unwrap() {
            let entry = entry.unwrap();
            let h = entry.header();
            assert_eq!(h.uid().unwrap(), 0);
            assert_eq!(h.gid().unwrap(), 0);
            let username = h.username().unwrap().unwrap_or("");
            let groupname = h.groupname().unwrap().unwrap_or("");
            assert_eq!(username, "");
            assert_eq!(groupname, "");
        }
    }

    #[test]
    fn file_modes_are_0644_for_non_exec() {
        let td = tempfile::tempdir().unwrap();
        let dir = minimal_plugin_dir(td.path());
        let bytes = canonical_tar_gz(&dir, &name(), &version()).unwrap();
        let mut archive = tar::Archive::new(std::io::Cursor::new(gunzip(&bytes)));
        for entry in archive.entries_with_seek().unwrap() {
            let entry = entry.unwrap();
            let mode = entry.header().mode().unwrap();
            // Files written by `fs::write` have no exec bit on modern systems.
            assert_eq!(
                mode,
                0o644,
                "expected 0644; got {mode:o} for entry {:?}",
                entry.header().path().unwrap()
            );
        }
    }

    #[test]
    #[cfg(unix)]
    fn exec_bit_preserved_as_0755() {
        use std::os::unix::fs::PermissionsExt;
        let td = tempfile::tempdir().unwrap();
        let dir = minimal_plugin_dir(td.path());
        let script = dir.join("run.sh");
        fs::write(&script, "#!/bin/sh\necho hi\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

        let bytes = canonical_tar_gz(&dir, &name(), &version()).unwrap();
        let mut archive = tar::Archive::new(std::io::Cursor::new(gunzip(&bytes)));
        let mut seen_exec = false;
        for entry in archive.entries_with_seek().unwrap() {
            let entry = entry.unwrap();
            let path = entry.header().path().unwrap().to_path_buf();
            if path.ends_with("run.sh") {
                assert_eq!(entry.header().mode().unwrap(), 0o755);
                seen_exec = true;
            }
        }
        assert!(seen_exec, "exec entry not found");
    }

    #[test]
    fn rejects_path_over_ustar_limit() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("plugin");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("manifest.toml"),
            "manifest_schema_version = \"1.0\"\n\n[plugin]\nname = \"p\"\nversion = \"0.1.0\"\ndescription = \"x\"\ntriggers = [\"process_writes\"]\n\n[dependencies]\ndatabase_version = \">=3.0.0\"\n",
        )
        .unwrap();
        fs::write(dir.join("__init__.py"), "def process_writes():\n    pass\n").unwrap();

        // Force a long archive path via nested directories. Each component
        // stays under the filesystem's single-component limit (~255 bytes on
        // macOS/Linux), but the total relative path exceeds 255 bytes, which
        // pushes the archive path past ustar's split-path limit.
        // Component "a".repeat(50) + "/" = 51 bytes × 6 = 306 bytes relative.
        let component = "a".repeat(50);
        let mut nested = dir.clone();
        for _ in 0..6 {
            nested = nested.join(&component);
        }
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("leaf"), "data").unwrap();

        let err = canonical_tar_gz(&dir, &name(), &version()).unwrap_err();
        assert!(
            matches!(err, SdkError::Archive { .. }),
            "expected SdkError::Archive, got {err:?}"
        );
    }

    #[test]
    fn gzip_mtime_is_zero() {
        let td = tempfile::tempdir().unwrap();
        let dir = minimal_plugin_dir(td.path());
        let bytes = canonical_tar_gz(&dir, &name(), &version()).unwrap();
        // Gzip MTIME is bytes 4..8 (little-endian).
        let mtime = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        assert_eq!(mtime, 0, "expected gzip MTIME=0; got {mtime}");
    }

    #[test]
    fn gzip_fname_flag_is_not_set() {
        let td = tempfile::tempdir().unwrap();
        let dir = minimal_plugin_dir(td.path());
        let bytes = canonical_tar_gz(&dir, &name(), &version()).unwrap();
        // FLG byte is at offset 3. FNAME is bit 3 (0x08). MUST be clear.
        let flg = bytes[3];
        assert_eq!(flg & 0x08, 0, "expected FNAME bit clear; FLG={flg:08b}");
    }

    #[test]
    fn round_trip_archive_contents() {
        // Not a canonicalization rule per se — sanity check that the archive
        // we produce is actually extractable and contains the expected files.
        let td = tempfile::tempdir().unwrap();
        let dir = minimal_plugin_dir(td.path());
        let bytes = canonical_tar_gz(&dir, &name(), &version()).unwrap();
        let listing = list_tar_paths(&bytes);
        assert!(listing.contains(&"p-0.1.0/manifest.toml".to_owned()));
        assert!(listing.contains(&"p-0.1.0/__init__.py".to_owned()));
    }

    // Test helpers.

    fn gunzip(bytes: &[u8]) -> Vec<u8> {
        use flate2::read::GzDecoder;
        let mut decoder = GzDecoder::new(bytes);
        let mut out = Vec::new();
        std::io::copy(&mut decoder, &mut out).unwrap();
        out
    }

    fn list_tar_paths(bytes: &[u8]) -> Vec<String> {
        let tar_bytes = gunzip(bytes);
        let mut archive = tar::Archive::new(std::io::Cursor::new(tar_bytes));
        archive
            .entries_with_seek()
            .unwrap()
            .filter_map(|e| {
                e.ok()
                    .and_then(|e| e.header().path().ok().map(|p| p.display().to_string()))
            })
            .collect()
    }
}
