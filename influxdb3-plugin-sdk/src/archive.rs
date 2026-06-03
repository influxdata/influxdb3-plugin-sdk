//! Canonical tar.gz archive construction.
//!
//! Given identical inputs and the same SDK version, [`canonical_tar_gz`]
//! produces byte-identical output on every machine and every run.
//!
//! # Canonicalization rules implemented
//!
//! 1. **Tar format**: `ustar` (via [`tar::Header::new_ustar`]). Explicitly not GNU.
//! 2. **Entry ordering**: sorted by archive path in lexicographic UTF-8 byte order.
//! 3. **mtime**: `0` on every entry.
//! 4. **UID / GID**: `0` / `0`. Owner and group name fields: empty strings.
//! 5. **File mode**: regular files → `0644`; exec-bit-on-disk files → `0755`.
//!    Directories are not included as separate entries (tar extraction
//!    auto-creates parents), so per-directory mode doesn't apply.
//! 6. **PAX extended headers**: none. Paths whose archive representation
//!    exceeds ustar's 255-byte split-path limit are rejected at package
//!    time with [`SdkError::PathTooLong`] (distinct from the catch-all
//!    [`SdkError::Archive`] variant so callers can pattern-match without
//!    string-scraping).
//! 7. **Gzip header timestamp**: `0`.
//! 8. **Original filename header**: omitted (FNAME flag not set).
//!
//! # File selection
//!
//! File selection (including `[plugin].exclude` filtering) is delegated to
//! [`crate::plugin_source_files`]; this module turns an already-selected file
//! set into canonical archive bytes.
//!
//! # Compression
//!
//! `flate2` with the `rust_backend` feature pins `miniz_oxide` as the gzip
//! encoder — different backends (system zlib, zlib-ng, cloudflare_zlib) produce
//! byte-different gzip streams at the same compression level and would break
//! determinism. Compression level is fixed at 6.
//!
//! # Cross-platform determinism caveat
//!
//! Unix filesystems report the exec bit; Windows has no Unix-style exec bit.
//! A `chmod +x` file packaged on Unix produces an archive with a 0755 entry
//! for that file; the same directory packaged on Windows produces 0644.
//! **Byte-identity across operating systems is not guaranteed for plugins
//! that carry executable files.** Plugins without exec files are
//! byte-identical across platforms.
//!
//! # Directory entries
//!
//! Directories are intentionally omitted — tar extraction creates parents
//! automatically. Consequence: extracted directory modes are umask-dependent
//! at `tar xf` time rather than pinned by the archive. The plugin-runtime
//! install path creates `plugin_dir/<name>/<version>/` directly via the DB
//! and does not rely on tar's extracted directory modes.

use flate2::{Compression, GzBuilder};
use influxdb3_plugin_schemas::PluginName;
use semver::Version;
use std::path::Path;

use crate::SdkError;

/// Packages `plugin_dir` into a canonical gzipped tar archive.
///
/// The archive's top-level directory is `{name}-{version}/`; the selected
/// source files (after `[plugin].exclude` filtering via
/// [`crate::plugin_source_files`]) are placed beneath it, preserving their
/// relative paths.
///
/// Returns the archive bytes. Consumers are expected to feed them into
/// [`crate::hash::sha256_of_bytes`] and write them out; this function
/// performs no file I/O on the output.
pub fn canonical_tar_gz(
    plugin_dir: &Path,
    name: &PluginName,
    version: &Version,
    exclude: &[String],
) -> Result<Vec<u8>, SdkError> {
    let files = crate::plugin_source_files::select(plugin_dir, exclude).map_err(SdkError::from)?;

    let archive_root = format!("{}-{}", name.as_str(), version);

    // Reject paths over ustar's 255-byte limit (100 name + 155 prefix, split
    // required at a `/`). `tar::Header::set_path` also errors here, but we
    // surface a domain-typed `SdkError::PathTooLong` earlier so callers
    // can pattern-match without string-scraping.
    const USTAR_PATH_LIMIT: usize = 255;
    for file in &files {
        let archive_path = format!("{}/{}", archive_root, file.normalized);
        if archive_path.len() > USTAR_PATH_LIMIT {
            return Err(SdkError::PathTooLong {
                archive_path,
                limit: USTAR_PATH_LIMIT,
            });
        }
    }

    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let gz = GzBuilder::new()
        .mtime(0) // canonical: gzip MTIME = 0
        .write(&mut buf, Compression::new(6));
    let mut tarball = tar::Builder::new(gz);

    for file in files {
        let archive_path = format!("{}/{}", archive_root, file.normalized);
        let data = std::fs::read(&file.absolute).map_err(|source| SdkError::Io {
            source,
            path: Some(file.absolute.clone()),
        })?;

        let mut header = tar::Header::new_ustar();
        header.set_size(data.len() as u64);
        header.set_mode(if file.is_exec { 0o755 } else { 0o644 });
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
        // `append_data` invokes `set_path` (invalidating any prior checksum)
        // then `set_cksum`, so we don't precompute the checksum.

        tarball
            .append_data(&mut header, &archive_path, std::io::Cursor::new(data))
            .map_err(|e| SdkError::Archive {
                message: format!("appending {archive_path:?}: {e}"),
            })?;
    }

    // Finalize tar first, then gz — both must finish for the bytes to be complete.
    let gz_encoder = tarball.into_inner().map_err(|e| SdkError::Archive {
        message: e.to_string(),
    })?;
    gz_encoder.finish().map_err(|e| SdkError::Archive {
        message: e.to_string(),
    })?;
    Ok(buf)
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
        let a = canonical_tar_gz(&dir, &name(), &version(), &[]).unwrap();
        let b = canonical_tar_gz(&dir, &name(), &version(), &[]).unwrap();
        assert_eq!(a, b, "same inputs must produce byte-identical output");
    }

    #[test]
    fn skips_paths_matched_by_manifest_exclude() {
        let td = tempfile::tempdir().unwrap();
        let dir = minimal_plugin_dir(td.path());
        fs::create_dir_all(dir.join("target")).unwrap();
        fs::write(dir.join("target/noise"), "ignore me").unwrap();
        fs::create_dir_all(dir.join("__pycache__")).unwrap();
        fs::write(dir.join("__pycache__/foo.pyc"), "junk").unwrap();
        fs::write(dir.join("compiled.pyc"), "also junk").unwrap();

        let exclude = vec![
            "target/".to_string(),
            "__pycache__/".to_string(),
            "*.pyc".to_string(),
        ];
        let bytes = canonical_tar_gz(&dir, &name(), &version(), &exclude).unwrap();
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
    fn no_exclude_keeps_formerly_hardcoded_paths() {
        // Behavior change: with no exclude, nothing is auto-removed — not the
        // old hard-coded `*.pyc`, `target/`, or `__pycache__/` patterns.
        let td = tempfile::tempdir().unwrap();
        let dir = minimal_plugin_dir(td.path());
        fs::write(dir.join("compiled.pyc"), "kept now").unwrap();
        fs::create_dir_all(dir.join("target")).unwrap();
        fs::write(dir.join("target/some_file"), "kept").unwrap();
        fs::create_dir_all(dir.join("__pycache__")).unwrap();
        fs::write(dir.join("__pycache__/cache.pyc"), "kept").unwrap();
        fs::create_dir_all(dir.join(".git")).unwrap();
        fs::write(dir.join(".git/config"), "[core]\n").unwrap();

        let bytes = canonical_tar_gz(&dir, &name(), &version(), &[]).unwrap();
        let listing = list_tar_paths(&bytes);
        assert!(
            listing.iter().any(|p| p.ends_with("compiled.pyc")),
            "no-exclude must keep root .pyc; got {listing:?}"
        );
        assert!(
            listing.iter().any(|p| p.contains("target/some_file")),
            "no-exclude must keep target/ files; got {listing:?}"
        );
        assert!(
            listing.iter().any(|p| p.contains("__pycache__/cache.pyc")),
            "no-exclude must keep __pycache__/ files; got {listing:?}"
        );
        assert!(
            listing.iter().any(|p| p.contains(".git/config")),
            "no-exclude must keep .git/ files; got {listing:?}"
        );
    }

    #[test]
    fn entries_sorted_by_archive_path() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("plugin");
        fs::create_dir_all(&dir).unwrap();
        // Write in out-of-order sequence; archive output must still be sorted.
        fs::write(
            dir.join("manifest.toml"),
            "manifest_schema_version = \"1.0\"\n\n[plugin]\nname = \"p\"\nversion = \"0.1.0\"\ndescription = \"x\"\ntriggers = [\"process_writes\"]\n\n[dependencies]\ndatabase_version = \">=3.0.0\"\n",
        )
        .unwrap();
        fs::write(dir.join("__init__.py"), "def process_writes():\n    pass\n").unwrap();
        fs::write(dir.join("zebra.py"), "# z\n").unwrap();
        fs::write(dir.join("alpha.py"), "# a\n").unwrap();

        let bytes = canonical_tar_gz(&dir, &name(), &version(), &[]).unwrap();
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
        let bytes = canonical_tar_gz(&dir, &name(), &version(), &[]).unwrap();
        let tar_bytes = gunzip(&bytes);
        // ustar magic at offset 257 ("ustar\0"), version "00" at 263.
        let magic = &tar_bytes[257..263];
        assert_eq!(magic, b"ustar\0", "expected ustar magic; got {magic:?}");
        let version = &tar_bytes[263..265];
        assert_eq!(version, b"00", "expected ustar version 00; got {version:?}");
    }

    #[test]
    fn every_entry_mtime_is_zero() {
        let td = tempfile::tempdir().unwrap();
        let dir = minimal_plugin_dir(td.path());
        let bytes = canonical_tar_gz(&dir, &name(), &version(), &[]).unwrap();
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
        let bytes = canonical_tar_gz(&dir, &name(), &version(), &[]).unwrap();
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
        let bytes = canonical_tar_gz(&dir, &name(), &version(), &[]).unwrap();
        let mut archive = tar::Archive::new(std::io::Cursor::new(gunzip(&bytes)));
        for entry in archive.entries_with_seek().unwrap() {
            let entry = entry.unwrap();
            let mode = entry.header().mode().unwrap();
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

        let bytes = canonical_tar_gz(&dir, &name(), &version(), &[]).unwrap();
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

        // Nested components keep each segment under the filesystem's
        // single-component limit but push the total relative path past
        // ustar's split-path limit. 51 bytes × 6 = 306 bytes relative.
        let component = "a".repeat(50);
        let mut nested = dir.clone();
        for _ in 0..6 {
            nested = nested.join(&component);
        }
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("leaf"), "data").unwrap();

        let err = canonical_tar_gz(&dir, &name(), &version(), &[]).unwrap_err();
        assert!(
            matches!(err, SdkError::PathTooLong { limit: 255, .. }),
            "expected SdkError::PathTooLong, got {err:?}"
        );
    }

    #[test]
    fn gzip_mtime_is_zero() {
        let td = tempfile::tempdir().unwrap();
        let dir = minimal_plugin_dir(td.path());
        let bytes = canonical_tar_gz(&dir, &name(), &version(), &[]).unwrap();
        // Gzip MTIME is bytes 4..8 (little-endian).
        let mtime = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        assert_eq!(mtime, 0, "expected gzip MTIME=0; got {mtime}");
    }

    #[test]
    fn gzip_fname_flag_is_not_set() {
        let td = tempfile::tempdir().unwrap();
        let dir = minimal_plugin_dir(td.path());
        let bytes = canonical_tar_gz(&dir, &name(), &version(), &[]).unwrap();
        // FLG byte is at offset 3. FNAME is bit 3 (0x08). MUST be clear.
        let flg = bytes[3];
        assert_eq!(flg & 0x08, 0, "expected FNAME bit clear; FLG={flg:08b}");
    }

    #[test]
    fn round_trip_archive_contents() {
        // Sanity check: the archive is extractable and carries expected files.
        let td = tempfile::tempdir().unwrap();
        let dir = minimal_plugin_dir(td.path());
        let bytes = canonical_tar_gz(&dir, &name(), &version(), &[]).unwrap();
        let listing = list_tar_paths(&bytes);
        assert!(listing.contains(&"p-0.1.0/manifest.toml".to_owned()));
        assert!(listing.contains(&"p-0.1.0/__init__.py".to_owned()));
    }

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
