//! Locks Spec 2 § S2-15 "stderr stays quiet" for `validate --output json`.
//!
//! See `version_smoke.rs` for the rationale behind the crate-root allow.

#![allow(unused_crate_dependencies)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

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
name = "Bad_Name"
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
        .args([
            "validate",
            plugin_dir.to_str().unwrap(),
            "--output",
            "json",
        ])
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
