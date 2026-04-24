//! Integration tests for `influxdb3-plugin yank`.
//!
//! Covers happy-path yank/undo, idempotent paths (already-yanked /
//! already-not-yanked), missing-target failure, S2-11 input
//! immutability, and S2-12 input/output non-overlap.
//!
//! See `version_smoke.rs` for the rationale behind the crate-root allow.

#![allow(unused_crate_dependencies)]

use std::path::Path;

mod common;
use common::{cli_cmd, SEEDED_INDEX};

fn write_index(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
}

fn spawn_yank(
    target: &str,
    index_path: &Path,
    out_dir: &Path,
    extra: &[&str],
) -> assert_cmd::assert::Assert {
    let mut cmd = cli_cmd();
    cmd.arg("yank").arg(target);
    cmd.arg("--index").arg(index_path);
    cmd.arg("--out").arg(out_dir);
    for a in extra {
        cmd.arg(a);
    }
    cmd.assert()
}

/// Strip the per-machine `index_path` field so the snapshot is
/// reproducible across hosts.
fn redact_index_path(payload: &mut serde_json::Value) {
    payload
        .as_object_mut()
        .expect("payload is a JSON object")
        .insert("index_path".into(), "<TMPDIR>/build/index.json".into());
}

fn read_yanked_flag(index_path: &Path, name: &str, version: &str) -> bool {
    let raw = std::fs::read_to_string(index_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let entries = v["plugins"].as_array().unwrap();
    let entry = entries
        .iter()
        .find(|e| e["name"] == name && e["version"] == version)
        .expect("entry must exist in derived index");
    entry["yanked"].as_bool().unwrap_or(false)
}

#[test]
fn yank_happy_path_sets_flag_and_emits_transitioned() {
    let td = tempfile::tempdir().unwrap();
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    write_index(&index_path, SEEDED_INDEX);
    let out = td.path().join("build");

    let assert = spawn_yank(
        "downsampler@1.2.0",
        &index_path,
        &out,
        &["--output", "json"],
    )
    .success();

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let mut payload: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["outcome"], "transitioned");
    assert_eq!(payload["target_state"], true);
    assert_eq!(payload["name"], "downsampler");
    assert_eq!(payload["version"], "1.2.0");

    assert!(
        read_yanked_flag(&out.join("index.json"), "downsampler", "1.2.0"),
        "derived index must reflect yanked=true"
    );

    redact_index_path(&mut payload);
    insta::assert_json_snapshot!("yank_transitioned_json", payload);
}

#[test]
fn yank_undo_clears_flag() {
    let td = tempfile::tempdir().unwrap();
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    let yanked = SEEDED_INDEX.replace(
        r#""hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000""#,
        r#""hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
      "yanked": true"#,
    );
    write_index(&index_path, &yanked);
    let out = td.path().join("build");

    let assert = spawn_yank(
        "downsampler@1.2.0",
        &index_path,
        &out,
        &["--undo", "--output", "json"],
    )
    .success();

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let mut payload: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["outcome"], "transitioned");
    assert_eq!(payload["target_state"], false);

    assert!(
        !read_yanked_flag(&out.join("index.json"), "downsampler", "1.2.0"),
        "derived index must reflect yanked=false after --undo"
    );

    redact_index_path(&mut payload);
    insta::assert_json_snapshot!("yank_undo_transitioned_json", payload);
}

/// Idempotency: re-yanking an already-yanked entry exits 0 with the
/// `already_in_desired_state` marker. Spec 2 § yank "Idempotent when the
/// target is already in the desired state."
#[test]
fn yank_already_yanked_is_no_op_with_marker() {
    let td = tempfile::tempdir().unwrap();
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    let yanked = SEEDED_INDEX.replace(
        r#""hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000""#,
        r#""hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
      "yanked": true"#,
    );
    write_index(&index_path, &yanked);
    let out = td.path().join("build");

    let assert = spawn_yank(
        "downsampler@1.2.0",
        &index_path,
        &out,
        &["--output", "json"],
    )
    .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let mut payload: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(payload["outcome"], "already_in_desired_state");
    assert_eq!(payload["target_state"], true);

    redact_index_path(&mut payload);
    insta::assert_json_snapshot!("yank_already_in_desired_state_json", payload);
}

/// Missing entry → exit 1 + stderr message.
#[test]
fn yank_missing_entry_exits_one() {
    let td = tempfile::tempdir().unwrap();
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    write_index(&index_path, SEEDED_INDEX);
    let out = td.path().join("build");

    let assert = spawn_yank("nope@9.9.9", &index_path, &out, &["--output", "json"])
        .failure()
        .code(1);
    let out_bytes = assert.get_output();
    assert!(
        out_bytes.stdout.is_empty(),
        "stdout must be empty on failure (data-tool idiom), got {:?}",
        String::from_utf8_lossy(&out_bytes.stdout)
    );
    let stderr = String::from_utf8_lossy(&out_bytes.stderr).into_owned();
    assert!(
        stderr.contains("not present"),
        "stderr should reference the missing entry, got: {stderr}"
    );
}

/// Malformed `<name>@<version>` → exit 2 (usage error per S2-18 /
/// spec F-1). Clap's `ValueValidation` error kind surfaces the invalid
/// value verbatim; the parser folds the `FromStr::Err` detail into the
/// `InvalidValue` field (clap's default renderer only emits
/// `InvalidValue`, silently discarding `Suggested`), so stderr echoes
/// both the offender and the expected shape on the error line.
#[test]
fn yank_malformed_target_exits_two() {
    let td = tempfile::tempdir().unwrap();
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    write_index(&index_path, SEEDED_INDEX);
    let out = td.path().join("build");

    let assert = spawn_yank("no-at-sign", &index_path, &out, &[])
        .failure()
        .code(2);
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert!(
        stderr.contains("no-at-sign"),
        "stderr should echo the malformed argument value, got: {stderr}"
    );
    // The FromStr detail ("no '@' separator" / "invalid plugin name" /
    // "invalid version") is now inside the InvalidValue field, which clap
    // renders as part of the error line.
    assert!(
        stderr.contains("<NAME@VERSION>"),
        "stderr should name the positional placeholder, got: {stderr}"
    );
    assert!(
        stderr.contains("no `@` separator")
            || stderr.contains("invalid plugin name")
            || stderr.contains("invalid SemVer version"),
        "stderr should include the FromStr failure detail, got: {stderr}"
    );
}

/// S2-11: the input `--index` is byte-identical pre/post.
#[test]
fn yank_does_not_modify_input_index() {
    let td = tempfile::tempdir().unwrap();
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    write_index(&index_path, SEEDED_INDEX);
    let out = td.path().join("build");

    let before = std::fs::read_to_string(&index_path).unwrap();
    spawn_yank("downsampler@1.2.0", &index_path, &out, &[]).success();
    let after = std::fs::read_to_string(&index_path).unwrap();
    assert_eq!(before, after, "input --index must be byte-identical");
}

/// S2-12: `--out == dirname(--index)` must be rejected (same contract
/// as `package`). One representative form here; `package_smoke.rs`
/// exercises the full equivalence matrix.
#[test]
fn yank_rejects_out_overlapping_index_dir() {
    let td = tempfile::tempdir().unwrap();
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    write_index(&index_path, SEEDED_INDEX);

    let assert = spawn_yank("downsampler@1.2.0", &index_path, &index_dir, &[])
        .failure()
        .code(2);
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    assert!(
        stderr.contains("S2-12"),
        "stderr should reference S2-12 by identifier, got: {stderr}"
    );
    // Input index untouched.
    assert_eq!(std::fs::read_to_string(&index_path).unwrap(), SEEDED_INDEX);
}
