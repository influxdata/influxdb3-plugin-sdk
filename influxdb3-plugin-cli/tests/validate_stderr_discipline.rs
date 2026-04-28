//! Locks the stream-routing contract for `validate`.
//!
//! JSON mode: stdout carries the envelope, stderr stays quiet.
//! Human mode: stderr carries the error rendering via render_human_error,
//! stdout carries the success message only.
//!
//! See `version_smoke.rs` for the rationale behind the crate-root allow.

#![allow(unused_crate_dependencies)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

mod common;
use common::{VALID_INIT, VALID_MANIFEST};

fn plugin() -> Command {
    Command::cargo_bin("influxdb3-plugin").expect("binary not built")
}

fn scaffold_bad_plugin() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("bad");
    fs::create_dir_all(&dir).unwrap();
    // Minimal broken manifest: bad name pattern.
    fs::write(
        dir.join("manifest.toml"),
        r#"manifest_schema_version = "1.0"

[plugin]
name = "Bad Name"
version = "0.1.0"
description = "x"
triggers = ["process_writes"]

[dependencies]
database_version = ">=3.0.0"
"#,
    )
    .unwrap();
    fs::write(
        dir.join("__init__.py"),
        "def process_writes(influxdb3_local, table_batches, args):\n    pass\n",
    )
    .unwrap();
    tmp
}

#[test]
fn validate_json_failure_stderr_is_silent() {
    let tmp = scaffold_bad_plugin();
    let plugin_dir = tmp.path().join("bad");
    plugin()
        .args(["validate", plugin_dir.to_str().unwrap(), "--output", "json"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("\"diagnostics\""))
        .stderr(predicate::str::is_empty());
}

#[test]
fn validate_human_failure_still_writes_summary_on_stderr() {
    let tmp = scaffold_bad_plugin();
    let plugin_dir = tmp.path().join("bad");
    plugin()
        .args([
            "validate",
            plugin_dir.to_str().unwrap(),
            "--output",
            "human",
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("validation failed"));
}

#[test]
fn validate_human_diagnostics_do_not_duplicate_field_prefix() {
    // Construct a plugin whose manifest has a bad `plugin.name`, so
    // the diagnostic message starts with "plugin name ...".
    // The renderer must NOT prepend another "plugin.name:" on top of that.
    // In the new envelope flow, human-mode errors render to stderr via
    // render_human_error in main.rs.
    let tmp = scaffold_bad_plugin();
    let plugin_dir = tmp.path().join("bad");
    let assert = Command::cargo_bin("influxdb3-plugin")
        .unwrap()
        .args([
            "validate",
            plugin_dir.to_str().unwrap(),
            "--output",
            "human",
        ])
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    // The diagnostic output now renders to stderr through render_human_error.
    // Verify no double-prefix occurs.
    assert!(
        !stderr.contains("plugin.name: plugin.name:"),
        "stderr duplicates the field prefix: {stderr}"
    );
    // Sanity: the field-prefixed message is present on stderr.
    assert!(
        stderr.contains("plugin.name:"),
        "stderr should contain the field-prefixed message: {stderr}"
    );
}

/// Human-mode parity for index parse failures: the diagnostic renders
/// to stderr via render_human_error.
#[test]
fn validate_human_mode_index_failure_renders_diagnostics_on_stderr() {
    let tmp = TempDir::new().unwrap();
    let plugin_dir = tmp.path().join("p");
    fs::create_dir_all(&plugin_dir).unwrap();
    fs::write(plugin_dir.join("manifest.toml"), VALID_MANIFEST).unwrap();
    fs::write(plugin_dir.join("__init__.py"), VALID_INIT).unwrap();
    let index_path = tmp.path().join("bad.json");
    fs::write(&index_path, "not valid json").unwrap();

    let assert = plugin()
        .args([
            "validate",
            plugin_dir.to_str().unwrap(),
            "--index",
            index_path.to_str().unwrap(),
            "--output",
            "human",
        ])
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
    // In human mode, errors render to stderr via render_human_error.
    // The error should contain a recognizable token from the error code
    // or a meaningful diagnostic message.
    assert!(
        stderr.contains("validate") || stderr.contains("[validate::"),
        "stderr should contain a recognizable error token in human mode, got: {stderr}"
    );
}
