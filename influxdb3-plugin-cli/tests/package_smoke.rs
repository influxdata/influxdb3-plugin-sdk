//! Integration tests for `influxdb3-plugin package`.
//!
//! Covers happy-path artifact + derived-index emission, S2-2 duplicate
//! rejection, S2-11 input-immutability, S2-12 input/output non-overlap
//! across equivalence forms (same path, trailing slash, `.` segment,
//! parent traversal, symlink), and the data-tool JSON idiom.
//!
//! See `version_smoke.rs` for the rationale behind the crate-root allow.

#![allow(unused_crate_dependencies)]

use rstest::rstest;
use std::path::{Path, PathBuf};

mod common;
use common::{cli_cmd, write_valid_plugin, EMPTY_INDEX};

fn write_empty_index(path: &Path) {
    std::fs::write(path, EMPTY_INDEX).unwrap();
}

fn spawn_package(
    plugin_dir: &Path,
    index_path: &Path,
    out_dir: &Path,
    extra: &[&str],
) -> assert_cmd::assert::Assert {
    let mut cmd = cli_cmd();
    cmd.arg("package");
    for a in extra {
        cmd.arg(a);
    }
    cmd.arg(plugin_dir);
    cmd.arg("--index").arg(index_path);
    cmd.arg("--out").arg(out_dir);
    cmd.assert()
}

#[test]
fn package_happy_path_writes_artifact_and_derived_index() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    write_valid_plugin(&plugin_dir);
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    write_empty_index(&index_path);
    let out_dir = td.path().join("build");

    spawn_package(&plugin_dir, &index_path, &out_dir, &["--output", "json"]).success();

    assert!(out_dir.join("downsampler-1.2.0.tar.gz").exists());
    assert!(out_dir.join("index.json").exists());

    // Derived index round-trips via Index::parse_json + carries the new entry.
    let derived: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("index.json")).unwrap())
            .unwrap();
    let plugins = derived["plugins"].as_array().unwrap();
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0]["name"], "downsampler");
    assert_eq!(plugins[0]["version"], "1.2.0");
    assert!(plugins[0]["hash"].as_str().unwrap().starts_with("sha256:"));
}

/// S2-11: the input `--index` file's bytes are byte-identical pre/post.
/// Hashing here is by string equality — the index file is small enough.
#[test]
fn package_does_not_modify_input_index() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    write_valid_plugin(&plugin_dir);
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    write_empty_index(&index_path);
    let out_dir = td.path().join("build");

    let before = std::fs::read_to_string(&index_path).unwrap();
    spawn_package(&plugin_dir, &index_path, &out_dir, &[]).success();
    let after = std::fs::read_to_string(&index_path).unwrap();

    assert_eq!(before, after, "input --index file must be byte-identical");
}

/// S2-2: duplicate `(name, version)` in the input index → exit 1, no
/// outputs created. The error message must enumerate every existing
/// version of the plugin and direct the author to either increment
/// `plugin.version` or run `yank` (Spec 2 § S2-2 rejection-payload
/// contract).
#[test]
fn package_rejects_duplicate_name_version() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    write_valid_plugin(&plugin_dir);
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    // Seed two prior versions of `downsampler` plus an unrelated entry
    // to confirm the payload only enumerates `downsampler`'s versions.
    let preexisting = serde_json::json!({
        "index_schema_version": "1.0",
        "artifacts_url": "https://plugins.example.com/artifacts",
        "plugins": [
            {
                "name": "downsampler", "version": "1.0.0",
                "description": "v1.0", "triggers": ["process_writes"],
                "dependencies": { "database_version": ">=3.0.0", "python": [] },
                "hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            },
            {
                "name": "downsampler", "version": "1.2.0",
                "description": "v1.2", "triggers": ["process_writes"],
                "dependencies": { "database_version": ">=3.0.0", "python": [] },
                "hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
            },
            {
                "name": "other", "version": "9.9.9",
                "description": "unrelated", "triggers": ["process_writes"],
                "dependencies": { "database_version": ">=3.0.0", "python": [] },
                "hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222"
            }
        ]
    });
    std::fs::write(
        &index_path,
        serde_json::to_string_pretty(&preexisting).unwrap(),
    )
    .unwrap();
    let out_dir = td.path().join("build");

    let assert = spawn_package(&plugin_dir, &index_path, &out_dir, &[])
        .failure()
        .code(1);

    // S2-2 payload contract: stderr must enumerate the existing
    // versions of `downsampler` AND direct the author to the actionable
    // remediation. The unrelated `other@9.9.9` must NOT appear.
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert!(
        stderr.contains("\"1.0.0\"") && stderr.contains("\"1.2.0\""),
        "stderr must list every existing version of `downsampler` (S2-2), got: {stderr}"
    );
    assert!(
        !stderr.contains("9.9.9"),
        "stderr must NOT list versions of unrelated plugins, got: {stderr}"
    );
    assert!(
        stderr.contains("yank"),
        "stderr must direct the author to `yank` per S2-2, got: {stderr}"
    );

    // No artifact / derived index written.
    assert!(!out_dir.join("downsampler-1.2.0.tar.gz").exists());
    // The check fires AFTER `--out` is created (canonicalize requires
    // existence), but the output files themselves must be absent.
    assert!(!out_dir.join("index.json").exists());
}

/// S2-12 input/output non-overlap: every equivalence form for
/// `--out == dirname(--index)` must be rejected.
#[rstest]
#[case::same_path("eq")]
#[case::trailing_slash("trailing-slash")]
#[case::dot_segment("dot-segment")]
#[case::parent_traversal("parent-traversal")]
fn package_rejects_out_overlapping_index_dir(#[case] mode: &str) {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    write_valid_plugin(&plugin_dir);
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    write_empty_index(&index_path);

    // All variants below resolve to `index_dir` after canonicalization.
    let out_dir: PathBuf = match mode {
        "eq" => index_dir.clone(),
        "trailing-slash" => {
            let mut s = index_dir.as_os_str().to_owned();
            s.push("/");
            PathBuf::from(s)
        }
        "dot-segment" => index_dir.join("."),
        "parent-traversal" => index_dir.join("subdir").join(".."),
        _ => unreachable!(),
    };

    let assert = spawn_package(&plugin_dir, &index_path, &out_dir, &[])
        .failure()
        .code(2);
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert!(
        stderr.contains("S2-12") || stderr.contains("disjoint"),
        "stderr should reference the S2-12 contract, got: {stderr}"
    );

    // Critical S2-11 corollary: the input index file is unchanged.
    assert_eq!(
        std::fs::read_to_string(&index_path).unwrap(),
        EMPTY_INDEX,
        "input --index must be byte-identical even on rejection"
    );
}

/// Symlinked `--out` pointing at the index's directory is also rejected
/// (canonicalize resolves the symlink).
#[cfg(unix)]
#[test]
fn package_rejects_out_via_symlink_to_index_dir() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    write_valid_plugin(&plugin_dir);
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    write_empty_index(&index_path);
    let out_link = td.path().join("link-to-reg");
    std::os::unix::fs::symlink(&index_dir, &out_link).unwrap();

    spawn_package(&plugin_dir, &index_path, &out_link, &[])
        .failure()
        .code(2);
}

/// JSON-mode failure path: stdout MUST be empty (data-tool idiom);
/// stderr carries the human error.
#[test]
fn package_failure_in_json_mode_keeps_stdout_empty() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    // Plugin dir intentionally missing → MissingRequiredFile failure.
    std::fs::create_dir_all(&plugin_dir).unwrap();
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    write_empty_index(&index_path);
    let out_dir = td.path().join("build");

    let assert = spawn_package(&plugin_dir, &index_path, &out_dir, &["--output", "json"])
        .failure()
        .code(1);
    let out = assert.get_output();
    assert!(
        out.stdout.is_empty(),
        "stdout MUST be empty on data-tool failure (S2-15), got {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        !out.stderr.is_empty(),
        "stderr MUST carry the human error on failure (S2-15)"
    );
}

/// JSON success snapshot — strip the per-machine paths so the snapshot
/// is reproducible.
#[test]
fn package_json_success_snapshot() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    write_valid_plugin(&plugin_dir);
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    write_empty_index(&index_path);
    let out_dir = td.path().join("build");

    let assert = spawn_package(&plugin_dir, &index_path, &out_dir, &["--output", "json"]).success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let mut payload: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let obj = payload.as_object_mut().unwrap();
    obj.insert(
        "artifact_path".into(),
        "<TMPDIR>/build/downsampler-1.2.0.tar.gz".into(),
    );
    obj.insert("index_path".into(), "<TMPDIR>/build/index.json".into());
    // Hash depends on archive bytes; assert format then strip.
    let hash = obj["hash"].as_str().unwrap().to_owned();
    assert!(
        hash.starts_with("sha256:") && hash.len() == "sha256:".len() + 64,
        "hash format unexpected: {hash}"
    );
    obj.insert("hash".into(), "sha256:<64 hex>".into());

    insta::assert_json_snapshot!("package_success", payload);
}
