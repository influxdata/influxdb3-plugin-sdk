//! Integration tests for Spec 2 Reproducibility archive-canonicalization
//! rules, driven through the `influxdb3_plugin_sdk::archive::canonical_tar_gz`
//! public API. These complement the crate-internal inline tests in
//! `src/archive.rs` by exercising the function from an external test crate,
//! which catches drift in the public signature that inline tests cannot see.
//!
//! One test per rule, matching the plan's D23 enumeration.
//!
//! See `validate_smoke.rs` for the rationale behind the crate-root allow.

#![allow(unused_crate_dependencies)]

use influxdb3_plugin_sdk::archive::canonical_tar_gz;
use semver::Version;
use std::fs;

mod common;
use common::{VALID_INIT, VALID_MANIFEST, minimal_plugin_dir};

// ─── Fixture helpers ─────────────────────────────────────────────────────────

fn plugin_name() -> influxdb3_plugin_schemas::PluginName {
    "p".parse().unwrap()
}

fn plugin_version() -> Version {
    Version::new(0, 1, 0)
}

fn gunzip(bytes: &[u8]) -> Vec<u8> {
    use flate2::read::GzDecoder;
    let mut decoder = GzDecoder::new(bytes);
    let mut out = Vec::new();
    std::io::copy(&mut decoder, &mut out).unwrap();
    out
}

// ─── Rule 1: tar format is ustar ─────────────────────────────────────────────

#[test]
fn rule1_tar_format_is_ustar() {
    let td = tempfile::tempdir().unwrap();
    let dir = minimal_plugin_dir(td.path(), "plugin");
    let bytes = canonical_tar_gz(&dir, &plugin_name(), &plugin_version()).unwrap();
    let tar_bytes = gunzip(&bytes);
    // ustar magic at offset 257, version at offset 263.
    assert_eq!(&tar_bytes[257..263], b"ustar\0", "expected ustar magic");
    assert_eq!(&tar_bytes[263..265], b"00", "expected ustar version 00");
}

// ─── Rule 2: entries sorted by archive path ──────────────────────────────────

#[test]
fn rule2_entries_sorted_by_path() {
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("plugin");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("manifest.toml"),
        "manifest_schema_version = \"1.0\"\n\n[plugin]\nname = \"p\"\nversion = \"0.1.0\"\ndescription = \"x\"\ntriggers = [\"process_writes\"]\n\n[dependencies]\ndatabase_version = \">=3.0.0\"\n",
    )
    .unwrap();
    fs::write(dir.join("__init__.py"), "def process_writes():\n    pass\n").unwrap();
    fs::write(dir.join("zebra.py"), "# z\n").unwrap();
    fs::write(dir.join("alpha.py"), "# a\n").unwrap();

    let bytes = canonical_tar_gz(&dir, &plugin_name(), &plugin_version()).unwrap();
    let paths: Vec<String> = list_paths(&bytes);
    let without_root: Vec<String> = paths
        .iter()
        .map(|p| p.trim_start_matches("p-0.1.0/").to_owned())
        .collect();

    assert_eq!(
        without_root,
        vec!["__init__.py", "alpha.py", "manifest.toml", "zebra.py"],
    );
}

// ─── Rule 3: every entry's mtime = 0 ─────────────────────────────────────────

#[test]
fn rule3_every_entry_mtime_zero() {
    let td = tempfile::tempdir().unwrap();
    let dir = minimal_plugin_dir(td.path(), "plugin");
    let bytes = canonical_tar_gz(&dir, &plugin_name(), &plugin_version()).unwrap();
    let mut archive = tar::Archive::new(std::io::Cursor::new(gunzip(&bytes)));
    for entry in archive.entries_with_seek().unwrap() {
        let entry = entry.unwrap();
        assert_eq!(entry.header().mtime().unwrap(), 0);
    }
}

// ─── Rule 4: UID = 0, GID = 0, owner/group names empty ───────────────────────

#[test]
fn rule4_uid_gid_and_names_canonical() {
    let td = tempfile::tempdir().unwrap();
    let dir = minimal_plugin_dir(td.path(), "plugin");
    let bytes = canonical_tar_gz(&dir, &plugin_name(), &plugin_version()).unwrap();
    let mut archive = tar::Archive::new(std::io::Cursor::new(gunzip(&bytes)));
    for entry in archive.entries_with_seek().unwrap() {
        let entry = entry.unwrap();
        let h = entry.header();
        assert_eq!(h.uid().unwrap(), 0);
        assert_eq!(h.gid().unwrap(), 0);
        assert_eq!(h.username().unwrap().unwrap_or(""), "");
        assert_eq!(h.groupname().unwrap().unwrap_or(""), "");
    }
}

// ─── Rule 5: file mode canonicalization ──────────────────────────────────────

#[test]
fn rule5_non_exec_files_are_0644() {
    let td = tempfile::tempdir().unwrap();
    let dir = minimal_plugin_dir(td.path(), "plugin");
    let bytes = canonical_tar_gz(&dir, &plugin_name(), &plugin_version()).unwrap();
    let mut archive = tar::Archive::new(std::io::Cursor::new(gunzip(&bytes)));
    for entry in archive.entries_with_seek().unwrap() {
        let entry = entry.unwrap();
        if !entry.header().entry_type().is_file() {
            continue;
        }
        assert_eq!(entry.header().mode().unwrap(), 0o644);
    }
}

/// Spec 2 §Reproducibility rule 5 lists `directories → 0755`, but the
/// canonical archive is flat-files-only: no directory-entry records are
/// emitted. This test locks that stance.
///
/// If this test ever starts failing, the implementation has begun emitting
/// directory entries, and the reproducibility story must be reassessed
/// (traversal order of empty directories is a reproducibility hazard the
/// flat-files-only approach avoids).
#[test]
fn archive_contains_no_directory_entries() {
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("plugin");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("manifest.toml"), VALID_MANIFEST).unwrap();
    std::fs::write(dir.join("__init__.py"), VALID_INIT).unwrap();
    std::fs::create_dir_all(dir.join("subdir")).unwrap();
    std::fs::write(dir.join("subdir/child.py"), "# nested\n").unwrap();
    // Empty subdirectory - the real reproducibility hazard a future
    // "emit empty dirs" refactor would introduce. walkdir visits the
    // directory even though it has no children.
    std::fs::create_dir_all(dir.join("empty_subdir")).unwrap();

    let bytes = canonical_tar_gz(&dir, &plugin_name(), &plugin_version()).unwrap();
    let mut archive = tar::Archive::new(std::io::Cursor::new(gunzip(&bytes)));
    for entry in archive.entries_with_seek().unwrap() {
        let entry = entry.unwrap();
        let kind = entry.header().entry_type();
        assert!(
            kind.is_file(),
            "expected file-only entries, got {:?} for path {:?}",
            kind,
            entry.header().path().unwrap()
        );
    }
}

#[test]
#[cfg(unix)]
fn rule5_exec_files_are_0755() {
    use std::os::unix::fs::PermissionsExt;
    let td = tempfile::tempdir().unwrap();
    let dir = minimal_plugin_dir(td.path(), "plugin");
    let script = dir.join("run.sh");
    fs::write(&script, "#!/bin/sh\necho hi\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

    let bytes = canonical_tar_gz(&dir, &plugin_name(), &plugin_version()).unwrap();
    let mut archive = tar::Archive::new(std::io::Cursor::new(gunzip(&bytes)));
    let mut seen_exec = false;
    for entry in archive.entries_with_seek().unwrap() {
        let entry = entry.unwrap();
        if entry.header().path().unwrap().ends_with("run.sh") {
            assert_eq!(entry.header().mode().unwrap(), 0o755);
            seen_exec = true;
        }
    }
    assert!(seen_exec, "run.sh entry not found in archive");
}

// ─── Rule 6: path-overflow rejection (no PAX) ────────────────────────────────

#[test]
fn rule6_rejects_archive_path_over_ustar_limit() {
    use influxdb3_plugin_sdk::SdkError;

    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("plugin");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("manifest.toml"),
        "manifest_schema_version = \"1.0\"\n\n[plugin]\nname = \"p\"\nversion = \"0.1.0\"\ndescription = \"x\"\ntriggers = [\"process_writes\"]\n\n[dependencies]\ndatabase_version = \">=3.0.0\"\n",
    )
    .unwrap();
    fs::write(dir.join("__init__.py"), "def process_writes():\n    pass\n").unwrap();

    // Nested dirs: 6 × 50-byte components = 306 bytes relative path.
    // Stays under the filesystem's per-component limit (~255 bytes) while
    // exceeding ustar's combined 255-byte boundary.
    let component = "a".repeat(50);
    let mut nested = dir.clone();
    for _ in 0..6 {
        nested = nested.join(&component);
    }
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("leaf"), "data").unwrap();

    let err = canonical_tar_gz(&dir, &plugin_name(), &plugin_version()).unwrap_err();
    assert!(
        matches!(err, SdkError::Archive { .. }),
        "expected SdkError::Archive, got {err:?}"
    );
}

// ─── Rule 7: gzip header MTIME = 0 ───────────────────────────────────────────

#[test]
fn rule7_gzip_mtime_zero() {
    let td = tempfile::tempdir().unwrap();
    let dir = minimal_plugin_dir(td.path(), "plugin");
    let bytes = canonical_tar_gz(&dir, &plugin_name(), &plugin_version()).unwrap();
    // Bytes 4..8, little-endian, per RFC 1952.
    let mtime = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    assert_eq!(mtime, 0, "expected gzip MTIME=0; got {mtime}");
}

// ─── Rule 8: gzip FNAME flag clear ───────────────────────────────────────────

#[test]
fn rule8_gzip_fname_flag_clear() {
    let td = tempfile::tempdir().unwrap();
    let dir = minimal_plugin_dir(td.path(), "plugin");
    let bytes = canonical_tar_gz(&dir, &plugin_name(), &plugin_version()).unwrap();
    // FLG byte at offset 3; FNAME is bit 3 (0x08). MUST be clear.
    let flg = bytes[3];
    assert_eq!(flg & 0x08, 0, "expected FNAME bit clear; FLG={flg:08b}");
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn list_paths(bytes: &[u8]) -> Vec<String> {
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
