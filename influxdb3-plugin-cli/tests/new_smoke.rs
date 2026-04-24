//! Integration tests for `influxdb3-plugin new`.
//!
//! Covers per-template happy paths, conflict behavior, invalid-name
//! handling, and the data-tool JSON idiom (single document on stdout for
//! success; empty stdout + stderr error on failure).
//!
//! See `version_smoke.rs` for the rationale behind the crate-root allow.

#![allow(unused_crate_dependencies)]

use std::path::Path;

mod common;
use common::cli_cmd;

/// Spawns `influxdb3-plugin` with `args` and an empty CWD-relative
/// environment so per-test invocations remain isolated.
fn spawn_new<P: AsRef<Path>>(target: P, extra_args: &[&str]) -> assert_cmd::assert::Assert {
    let mut cmd = cli_cmd();
    cmd.arg("new");
    for a in extra_args {
        cmd.arg(a);
    }
    cmd.arg(target.as_ref());
    cmd.assert()
}

#[test]
fn new_process_writes_happy_path_human_mode() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("hp");

    spawn_new(&target, &["process_writes"]).success();

    assert!(target.join("manifest.toml").exists());
    assert!(target.join("__init__.py").exists());
    assert!(target.join("README.md").exists());

    let manifest = std::fs::read_to_string(target.join("manifest.toml")).unwrap();
    assert!(manifest.contains("name = \"hp\""), "manifest: {manifest}");
    assert!(
        manifest.contains("triggers = [\"process_writes\"]"),
        "manifest: {manifest}"
    );
}

/// JSON-mode happy path emits a stable schema on stdout (data-tool idiom).
/// Snapshot the JSON shape minus the absolute-path field (which varies
/// per machine).
#[test]
fn new_process_writes_happy_path_json_mode() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("downsampler");

    let assert = spawn_new(&target, &["process_writes", "--output", "json"]).success();

    let out = assert.get_output();
    assert!(
        out.stderr.is_empty(),
        "stderr should be empty on success, got {:?}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = std::str::from_utf8(&out.stdout).unwrap();
    let mut payload: serde_json::Value = serde_json::from_str(stdout).expect("stdout is JSON");
    // Strip the absolute path before snapshotting so the snapshot is
    // machine-independent.
    payload
        .as_object_mut()
        .unwrap()
        .insert("target_dir".into(), "<TMPDIR>/downsampler".into());

    insta::assert_json_snapshot!("new_process_writes_json", payload);
}

/// Snapshot the JSON output for one scaffold template. `target` is the
/// path basename used to derive the plugin name (and the redacted
/// fragment in the snapshot's `target_dir`).
fn snapshot_new_template(template: &str, target: &str, snapshot_name: &str) {
    let td = tempfile::tempdir().unwrap();
    let target_path = td.path().join(target);
    let assert = spawn_new(&target_path, &[template, "--output", "json"]).success();
    let stdout = std::str::from_utf8(&assert.get_output().stdout).unwrap();
    let mut payload: serde_json::Value = serde_json::from_str(stdout).expect("stdout is JSON");
    let placeholder = format!("<TMPDIR>/{target}");
    payload
        .as_object_mut()
        .unwrap()
        .insert("target_dir".into(), placeholder.into());
    insta::assert_json_snapshot!(snapshot_name, payload);
}

/// Spec 2 § S2-16: every per-template JSON output is a stable schema
/// commitment. One snapshot per template locks that contract.
/// `process_writes` is covered by `new_process_writes_happy_path_json_mode`
/// above; this group covers the remaining three.

#[test]
fn new_process_scheduled_call_json_snapshot() {
    snapshot_new_template(
        "process_scheduled_call",
        "downsampler",
        "new_process_scheduled_call_json",
    );
}

#[test]
fn new_process_request_json_snapshot() {
    snapshot_new_template("process_request", "downsampler", "new_process_request_json");
}

#[test]
fn new_registry_json_snapshot() {
    snapshot_new_template("registry", "reg", "new_registry_json");
}

#[test]
fn new_each_plugin_template_writes_matching_init() {
    for template in [
        "process_writes",
        "process_scheduled_call",
        "process_request",
    ] {
        let td = tempfile::tempdir().unwrap();
        let target = td.path().join("p");

        spawn_new(&target, &[template]).success();

        let init = std::fs::read_to_string(target.join("__init__.py")).unwrap();
        assert!(
            init.contains(&format!("def {template}(")),
            "expected `def {template}(` in {template} init, got:\n{init}"
        );
    }
}

#[test]
fn new_registry_happy_path_writes_file_url() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("reg");

    spawn_new(&target, &["registry"]).success();

    let index = std::fs::read_to_string(target.join("index.json")).unwrap();
    assert!(
        index.contains("\"artifacts_url\": \"file://"),
        "registry index should default artifacts_url to file://, got:\n{index}"
    );
}

/// Explicit `--artifacts-url` is written through verbatim (https / http
/// inclusive).
#[test]
fn new_registry_with_explicit_artifacts_url() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("reg");

    spawn_new(
        &target,
        &[
            "registry",
            "--artifacts-url",
            "https://plugins.example.com/artifacts",
        ],
    )
    .success();

    let index = std::fs::read_to_string(target.join("index.json")).unwrap();
    assert!(
        index.contains("\"artifacts_url\": \"https://plugins.example.com/artifacts\""),
        "explicit --artifacts-url should be written verbatim, got:\n{index}"
    );
}

/// `--database-version` overrides the default substitution baked into
/// the template.
#[test]
fn new_plugin_with_explicit_database_version() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("p");

    spawn_new(
        &target,
        &["process_writes", "--database-version", ">=3.5.0,<4.0.0"],
    )
    .success();

    let manifest = std::fs::read_to_string(target.join("manifest.toml")).unwrap();
    assert!(
        manifest.contains("database_version = \">=3.5.0,<4.0.0\""),
        "explicit --database-version should be written, got:\n{manifest}"
    );
}

/// `new` errors and writes nothing when any target file already exists
/// (Spec 2 § new "writes full file set or nothing").
#[test]
fn new_errors_on_pre_existing_file() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("p");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("manifest.toml"), "pre-existing").unwrap();

    spawn_new(&target, &["process_writes"]).failure().code(1);

    assert_eq!(
        std::fs::read_to_string(target.join("manifest.toml")).unwrap(),
        "pre-existing",
        "pre-existing file should be preserved"
    );
    assert!(!target.join("__init__.py").exists());
}

/// Invalid path basename → exit 1 with stderr instructing `--name`.
/// (clap's exit-2 path applies only to argument-parse failures; an
/// invalid basename is a runtime-validation failure surfaced by the
/// command body.)
#[test]
fn new_rejects_invalid_basename_without_name_override() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("BAD_NAME");

    let assert = spawn_new(&target, &["process_writes"]).failure().code(1);

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.contains("--name"),
        "stderr should hint at --name, got: {stderr}"
    );
    assert!(!target.join("manifest.toml").exists());
}

/// Explicit `--name <bad>` also rejected, with a different message.
#[test]
fn new_rejects_invalid_explicit_name() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("ok");

    let assert = spawn_new(&target, &["process_writes", "--name", "BAD_NAME"])
        .failure()
        .code(2);

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.contains("BAD_NAME"),
        "stderr should name the bad value, got: {stderr}"
    );
    assert!(!target.join("manifest.toml").exists());
}

/// Plugin-template flags rejected with the registry template, and vice
/// versa. Surfaces nonsensical combinations at runtime rather than
/// silently ignoring them.
#[test]
fn new_rejects_artifacts_url_on_plugin_template() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("p");

    spawn_new(
        &target,
        &["process_writes", "--artifacts-url", "https://example.com/a"],
    )
    .failure()
    .code(2);
}

#[test]
fn new_rejects_name_on_registry_template() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("r");

    spawn_new(&target, &["registry", "--name", "x"])
        .failure()
        .code(2);
}

/// Unknown template → clap parse error → exit code 2 (S2-18 usage error).
#[test]
fn new_unknown_template_exits_two() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("p");

    let assert = cli_cmd()
        .args(["new", "garbage_template", target.to_str().unwrap()])
        .assert()
        .failure();

    assert_eq!(
        assert.get_output().status.code(),
        Some(2),
        "clap usage errors must exit 2 per S2-18"
    );
}

/// Data-tool failure path: stdout empty, error on stderr (S2-15).
#[test]
fn new_failure_in_json_mode_keeps_stdout_empty() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("p");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("manifest.toml"), "pre-existing").unwrap();

    let assert = spawn_new(&target, &["process_writes", "--output", "json"])
        .failure()
        .code(1);

    let out = assert.get_output();
    assert!(
        out.stdout.is_empty(),
        "stdout MUST be empty on data-tool failure (S2-15), got: {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        !out.stderr.is_empty(),
        "stderr MUST carry the human-readable error (S2-15)"
    );
}

#[test]
fn new_conflict_error_mentions_path_once() {
    use assert_cmd::Command;
    use std::fs;
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("conflict");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("manifest.toml"), "pre-existing").unwrap();

    let output = Command::cargo_bin("influxdb3-plugin")
        .unwrap()
        .args([
            "new",
            "process_writes",
            dir.to_str().unwrap(),
            "--output",
            "human",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(1),
        "conflict error should be a runtime failure per S2-18; got {:?}",
        output.status.code()
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    let occurrences = stderr.matches(dir.to_str().unwrap()).count();
    assert_eq!(
        occurrences, 1,
        "stderr should mention the conflicting path exactly once; was:\n{stderr}"
    );

    // After the Chunk 6 polish, "already exists" should appear exactly
    // once in the rendered error chain (anyhow's source-walk plus
    // `#[source]` no longer duplicates the inner io::Error's message).
    let phrase_occurrences = stderr.matches("already exists").count();
    assert_eq!(
        phrase_occurrences, 1,
        "phrase 'already exists' should appear exactly once; was:\n{stderr}"
    );
}
