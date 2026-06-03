//! Per-non-input perturbation tests for `canonical_tar_gz`.
//!
//! Canonical output must be invariant under perturbation of the listed
//! non-inputs. Each `#[test]` here picks one such item, varies it between
//! two invocations with otherwise-identical logical inputs, and asserts
//! the archive bytes are identical.
//!
//! See `archive_determinism.rs` for the in-process purity test (proptest-
//! generated inputs, two calls on same dir). That file catches a different
//! class of bug (iteration-order, hashmap-randomness) and does not overlap.

#![allow(unused_crate_dependencies)]

mod common;

use common::{VALID_INIT, VALID_MANIFEST};
use influxdb3_plugin_sdk::archive::canonical_tar_gz;
use semver::Version;
use std::fs;
use std::path::Path;

fn build(dir: &Path) -> Vec<u8> {
    let name = "p".parse().unwrap();
    let version = Version::new(0, 1, 0);
    canonical_tar_gz(dir, &name, &version, &[]).unwrap()
}

fn plant(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("manifest.toml"), VALID_MANIFEST).unwrap();
    fs::write(dir.join("__init__.py"), VALID_INIT).unwrap();
}

#[test]
fn invariant_under_absolute_path() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let da = a.path().join("plugin");
    let db = b.path().join("plugin");
    plant(&da);
    plant(&db);
    assert_eq!(
        build(&da),
        build(&db),
        "archive bytes must not depend on absolute path"
    );
}

#[test]
fn invariant_under_source_mtime() {
    use std::time::{Duration, SystemTime};
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let da = a.path().join("plugin");
    let db = b.path().join("plugin");
    plant(&da);
    plant(&db);

    // Skew mtimes in `b` by one hour into the future.
    let future = SystemTime::now() + Duration::from_secs(3600);
    filetime::set_file_mtime(db.join("manifest.toml"), future.into()).unwrap();
    filetime::set_file_mtime(db.join("__init__.py"), future.into()).unwrap();

    assert_eq!(
        build(&da),
        build(&db),
        "archive bytes must not depend on source mtime"
    );
}

#[test]
fn invariant_under_source_date_epoch_env_var() {
    // The canonicalization rules deliberately do NOT honor SOURCE_DATE_EPOCH;
    // every clock-bearing field is pinned to zero unconditionally. Verify that
    // perturbing the env var has no effect on output bytes.
    let td_a = tempfile::tempdir().unwrap();
    let td_b = tempfile::tempdir().unwrap();
    let da = td_a.path().join("plugin");
    let db = td_b.path().join("plugin");
    plant(&da);
    plant(&db);

    // SAFETY: single-threaded test under nextest's per-test isolation (nextest
    // spawns each test in its own process by default). env::set_var is
    // process-global; within one test process we serialize the reads.
    unsafe { std::env::set_var("SOURCE_DATE_EPOCH", "0") };
    let bytes_a = build(&da);
    unsafe { std::env::set_var("SOURCE_DATE_EPOCH", "9999999999") };
    let bytes_b = build(&db);
    unsafe { std::env::remove_var("SOURCE_DATE_EPOCH") };

    assert_eq!(
        bytes_a, bytes_b,
        "archive bytes must not depend on SOURCE_DATE_EPOCH"
    );
}

#[test]
fn invariant_under_locale() {
    // Perturb LC_ALL between runs. Relevant because any implementation that
    // reads locale-sensitive APIs (collation-order sort, case mapping) would
    // produce divergent output between C locale and UTF-8 locale.
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let da = a.path().join("plugin");
    let db = b.path().join("plugin");
    plant(&da);
    plant(&db);

    unsafe { std::env::set_var("LC_ALL", "C") };
    let bytes_a = build(&da);
    unsafe { std::env::set_var("LC_ALL", "en_US.UTF-8") };
    let bytes_b = build(&db);
    unsafe { std::env::remove_var("LC_ALL") };

    assert_eq!(bytes_a, bytes_b, "archive bytes must not depend on LC_ALL");
}

#[cfg(unix)]
#[test]
fn invariant_under_uid_gid_in_source() {
    use std::os::unix::fs::MetadataExt;
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let da = a.path().join("plugin");
    let db = b.path().join("plugin");
    plant(&da);
    plant(&db);

    // We can't easily chown without root. We can at least confirm both sides
    // have the same source UID (so variance *isn't* from an uncontrolled
    // source), and that output is byte-identical anyway. The
    // `uid_gid_and_names_canonical` test already pins UID=0 in output;
    // this test's scaffolding catches any future change where archive
    // code newly reads source UID.
    let uid_a = fs::metadata(da.join("manifest.toml")).unwrap().uid();
    let uid_b = fs::metadata(db.join("manifest.toml")).unwrap().uid();
    assert_eq!(
        uid_a, uid_b,
        "test requires both tempdirs to have same owner UID"
    );
    assert_eq!(build(&da), build(&db));
}

#[cfg(unix)]
#[test]
fn non_exec_file_mode_does_not_leak() {
    use std::os::unix::fs::PermissionsExt;
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let da = a.path().join("plugin");
    let db = b.path().join("plugin");
    plant(&da);
    plant(&db);

    // `a`: default mode (0644 typically).
    // `b`: explicit 0600 on manifest (still non-exec).
    // Both should produce 0644 in the archive per the canonicalization rule.
    fs::set_permissions(db.join("manifest.toml"), fs::Permissions::from_mode(0o600)).unwrap();

    assert_eq!(
        build(&da),
        build(&db),
        "non-exec source modes must all canonicalize to 0644"
    );
}
