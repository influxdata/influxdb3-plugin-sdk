//! Locks the "stderr stays quiet" contract for `validate --output json`.
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
    // the diagnostic message starts with "plugin.name: plugin name ...".
    // The renderer must NOT prepend another "plugin.name:" on top of that.
    // The per-diagnostic lines land on stdout (see `render_human` in
    // `commands/validate.rs`); stderr carries only the anyhow summary.
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
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        !stdout.contains("plugin.name: plugin.name:"),
        "stdout duplicates the field prefix: {stdout}"
    );
    assert!(
        !stderr.contains("plugin.name: plugin.name:"),
        "stderr duplicates the field prefix: {stderr}"
    );
    // Sanity: the single occurrence is still present on stdout.
    assert!(
        stdout.contains("plugin.name: plugin name"),
        "stdout should contain the single field-prefixed message: {stdout}"
    );
}

/// Human-mode parity for index parse failures: the diagnostic line(s)
/// land on stdout (via `render_human`) and the anyhow summary lands on
/// stderr — symmetric with manifest-error rendering.
#[test]
fn validate_human_mode_index_failure_renders_diagnostics_on_stdout() {
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
        .code(1)
        .stderr(predicate::str::contains("validation failed"));
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    // The diagnostic body lands on stdout via render_human; the variant
    // tag is part of the stable contract. A regression that routed the
    // line to stderr (or dropped it) breaks this assertion.
    assert!(
        stdout.contains("[SchemaReported]"),
        "stdout should render the SchemaReported diagnostic line, got: {stdout}"
    );
}
