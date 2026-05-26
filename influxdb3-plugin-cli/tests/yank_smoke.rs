//! Integration tests for `influxdb3-plugin yank`.
//!
//! Covers happy-path yank/undo, idempotent paths (already-yanked /
//! already-not-yanked), missing-target failure, input immutability,
//! and input/output non-overlap.
//!
//! See `version_smoke.rs` for the rationale behind the crate-root allow.

#![allow(unused_crate_dependencies)]

use std::path::{Path, PathBuf};

mod common;
use common::{SEEDED_INDEX, cli_cmd};

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

/// Strip the per-machine `index_path` field inside the envelope's
/// `result` object so the snapshot is reproducible across hosts.
fn redact_index_path(envelope: &mut serde_json::Value) {
    envelope["result"]
        .as_object_mut()
        .expect("result is a JSON object")
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

fn read_published_at(index_path: &Path, name: &str, version: &str) -> String {
    let raw = std::fs::read_to_string(index_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let entries = v["plugins"].as_array().unwrap();
    let entry = entries
        .iter()
        .find(|e| e["name"] == name && e["version"] == version)
        .expect("entry must exist in derived index");
    entry["published_at"].as_str().unwrap().to_owned()
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
    let mut envelope: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(envelope["status"], "ok");
    assert_eq!(envelope["result"]["outcome"], "yanked");
    assert_eq!(envelope["result"]["name"], "downsampler");
    assert_eq!(envelope["result"]["version"], "1.2.0");
    assert_eq!(envelope["result"]["published_at"], "2026-04-29T18:45:12Z");

    assert!(
        read_yanked_flag(&out.join("index.json"), "downsampler", "1.2.0"),
        "derived index must reflect yanked=true"
    );
    assert_eq!(
        read_published_at(&out.join("index.json"), "downsampler", "1.2.0"),
        "2026-04-29T18:45:12Z",
        "derived index must preserve published_at"
    );

    redact_index_path(&mut envelope);
    insta::assert_json_snapshot!("yank_transitioned_json", envelope);
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
    let mut envelope: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(envelope["status"], "ok");
    assert_eq!(envelope["result"]["outcome"], "unyanked");
    assert_eq!(envelope["result"]["published_at"], "2026-04-29T18:45:12Z");

    assert!(
        !read_yanked_flag(&out.join("index.json"), "downsampler", "1.2.0"),
        "derived index must reflect yanked=false after --undo"
    );
    assert_eq!(
        read_published_at(&out.join("index.json"), "downsampler", "1.2.0"),
        "2026-04-29T18:45:12Z",
        "derived index must preserve published_at after --undo"
    );

    redact_index_path(&mut envelope);
    insta::assert_json_snapshot!("yank_undo_transitioned_json", envelope);
}

/// Idempotency: re-yanking an already-yanked entry exits 0 with the
/// `already_yanked` outcome.
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
    let mut envelope: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(envelope["status"], "ok");
    assert_eq!(envelope["result"]["outcome"], "already_yanked");

    redact_index_path(&mut envelope);
    insta::assert_json_snapshot!("yank_already_yanked_json", envelope);
}

/// Missing entry -> exit 1 + error envelope on stdout in JSON mode.
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
    let stdout = String::from_utf8_lossy(&out_bytes.stdout).into_owned();
    let doc: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must be valid JSON: {e}\n{stdout}"));
    assert_eq!(
        doc.get("status").and_then(|v| v.as_str()),
        Some("error"),
        "envelope status must be \"error\"; got:\n{stdout}"
    );
    assert!(
        stdout.contains("not present"),
        "output should reference the missing entry, got: {stdout}"
    );
}

#[test]
fn yank_invalid_published_at_in_input_index_fails_before_output() {
    let td = tempfile::tempdir().unwrap();
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    let bad_index = SEEDED_INDEX.replace("2026-04-29T18:45:12Z", "2026-04-29T18:45:12.123Z");
    write_index(&index_path, &bad_index);
    let out = td.path().join("build");

    let assert = spawn_yank(
        "downsampler@1.2.0",
        &index_path,
        &out,
        &["--output", "json"],
    )
    .failure()
    .code(1);

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let envelope: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(envelope["status"], "error");
    assert_eq!(envelope["error"]["code"], "yank::index_parse_failed");
    assert_eq!(
        envelope["error"]["diagnostics"][0]["field"],
        "plugins[0].published_at"
    );
    assert!(!out.join("index.json").exists(), "no output on failure");
}

/// Malformed `<name>@<version>` → exit 2 (usage error). Clap's
/// `ValueValidation` error kind surfaces the invalid
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
    // Under piped stdout, errors render as JSON envelopes on stdout.
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(
        stdout.contains("no-at-sign"),
        "output should echo the malformed argument value, got: {stdout}"
    );
    assert!(
        stdout.contains("<NAME@VERSION>"),
        "output should name the positional placeholder, got: {stdout}"
    );
    assert!(
        stdout.contains("no `@` separator")
            || stdout.contains("invalid plugin name")
            || stdout.contains("invalid SemVer version"),
        "output should include the FromStr failure detail, got: {stdout}"
    );
}

// PluginName rule inheritance — `yank` parses `<name>@<version>` through
// the same `PluginName::from_str` used by `package`, so the new Cargo
// rule applies transparently. Cover one accept + two reject paths.

/// `my_plugin` (underscore) parses cleanly through the clap value parser
/// under the new rule. The entry is not in the seeded index, so the
/// command fails downstream with exit 1 + "not present" — what matters
/// is that stderr does NOT carry a clap usage rejection for the name
/// itself.
#[test]
fn yank_parses_underscore_name() {
    let td = tempfile::tempdir().unwrap();
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    write_index(&index_path, SEEDED_INDEX);
    let out = td.path().join("build");

    let assert = spawn_yank("my_plugin@1.0.0", &index_path, &out, &[])
        .failure()
        .code(1);
    // Under piped stdout, errors render as JSON envelopes on stdout.
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(
        !stdout.contains("starting with a letter"),
        "parser rejected `my_plugin` -- new rule should accept it; stdout: {stdout}"
    );
    assert!(
        !stdout.contains("Windows reserved"),
        "output should not flag `my_plugin` as reserved; stdout: {stdout}"
    );
    assert!(
        stdout.contains("not present"),
        "downstream failure expected (entry absent); stdout: {stdout}"
    );
}

/// Regression narrowing: `7plugin` (digit-leading) was valid under the
/// old rule, rejected under the new one. Clap surfaces the regex on
/// stderr.
#[test]
fn yank_rejects_digit_leading_name_regression() {
    let td = tempfile::tempdir().unwrap();
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    write_index(&index_path, SEEDED_INDEX);
    let out = td.path().join("build");

    spawn_yank("7plugin@1.0.0", &index_path, &out, &[])
        .failure()
        .code(2)
        .stdout(predicates::str::contains("starting with a letter"));
}

#[test]
fn yank_rejects_reserved_name() {
    let td = tempfile::tempdir().unwrap();
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    write_index(&index_path, SEEDED_INDEX);
    let out = td.path().join("build");

    spawn_yank("con@1.0.0", &index_path, &out, &[])
        .failure()
        .code(2)
        .stdout(predicates::str::contains("Windows reserved"));
}

/// The input `--index` is byte-identical pre/post.
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

/// `--out == dirname(--index)` must be rejected (same contract as
/// `package`). One representative form here; `package_smoke.rs`
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
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(
        stdout.contains("usage::input_output_overlap"),
        "output should reference usage::input_output_overlap by identifier, got: {stdout}"
    );
    // Input index untouched.
    assert_eq!(std::fs::read_to_string(&index_path).unwrap(), SEEDED_INDEX);
}

/// Human-mode success output shortens the derived-index path to
/// CWD-relative form when the destination lives under the working
/// directory. Avoids leaking absolute machine paths in terminals,
/// demos, and CI logs.
#[test]
fn yank_human_mode_emits_cwd_relative_paths() {
    let td = tempfile::tempdir().unwrap();
    let cwd = std::fs::canonicalize(td.path()).unwrap();
    let index_dir = cwd.join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    write_index(&index_path, SEEDED_INDEX);

    let mut cmd = cli_cmd();
    cmd.current_dir(&cwd)
        .arg("yank")
        .arg("downsampler@1.2.0")
        .arg("--output")
        .arg("human")
        .arg("--index")
        .arg("reg/index.json")
        .arg("--out")
        .arg("build");
    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();

    let expected = PathBuf::from("build").join("index.json");
    assert!(
        stdout.contains(&format!("index: {}", expected.display())),
        "human output should print relative index path, got:\n{stdout}"
    );
    let cwd_str = cwd.display().to_string();
    assert!(
        !stdout.contains(&cwd_str),
        "human output must not leak the absolute CWD prefix {cwd_str:?}; got:\n{stdout}"
    );
}

/// JSON-mode payload keeps the absolute derived-index path so
/// programmatic consumers get unambiguous filesystem targets
/// regardless of caller CWD.
#[test]
fn yank_json_mode_keeps_absolute_paths() {
    let td = tempfile::tempdir().unwrap();
    let cwd = std::fs::canonicalize(td.path()).unwrap();
    let index_dir = cwd.join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    write_index(&index_path, SEEDED_INDEX);

    let mut cmd = cli_cmd();
    cmd.current_dir(&cwd)
        .arg("yank")
        .arg("downsampler@1.2.0")
        .arg("--output")
        .arg("json")
        .arg("--index")
        .arg("reg/index.json")
        .arg("--out")
        .arg("build");
    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let doc: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must be valid JSON: {e}\n{stdout}"));
    let index = doc
        .pointer("/result/index_path")
        .and_then(|v| v.as_str())
        .expect("result.index_path missing");
    assert!(
        Path::new(index).is_absolute(),
        "json index_path must be absolute, got {index:?}"
    );
}
