//! Locks the "authors fix everything in one pass" invariant: package's
//! failure path must emit each diagnostic, not just a count.

#![allow(unused_crate_dependencies)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn plugin() -> Command {
    Command::cargo_bin("influxdb3-plugin").expect("binary not built")
}

fn setup() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let reg = tmp.path().join("reg");
    plugin()
        .args(["new", "index", reg.to_str().unwrap()])
        .assert()
        .success();
    let dir = tmp.path().join("bad");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("manifest.toml"),
        r#"manifest_schema_version = "1.0"

[plugin]
name = "Bad Name"
version = "not-semver"
description = ""
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
fn package_human_failure_lists_each_diagnostic() {
    let tmp = setup();
    let reg = tmp.path().join("reg");
    let bad = tmp.path().join("bad");
    plugin()
        .args([
            "package",
            bad.to_str().unwrap(),
            "--index",
            reg.join("index.json").to_str().unwrap(),
            "--out",
            tmp.path().join("out").to_str().unwrap(),
            "--output",
            "human",
        ])
        .assert()
        .code(1)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("plugin.name"))
        .stderr(predicate::str::contains("plugin.version"))
        .stderr(predicate::str::contains("plugin.description"));
}

#[test]
fn package_json_failure_emits_error_envelope_on_stdout() {
    let tmp = setup();
    let reg = tmp.path().join("reg");
    let bad = tmp.path().join("bad");
    let assert = plugin()
        .args([
            "package",
            bad.to_str().unwrap(),
            "--index",
            reg.join("index.json").to_str().unwrap(),
            "--out",
            tmp.path().join("out").to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .code(1);
    // In JSON mode, errors are rendered as a JSON envelope on stdout.
    // stderr must be empty.
    let out = assert.get_output();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let doc: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must be valid JSON: {e}\n{stdout}"));
    assert_eq!(
        doc.get("status").and_then(|v| v.as_str()),
        Some("error"),
        "envelope status must be \"error\"; got:\n{stdout}"
    );
    assert!(
        out.stderr.is_empty(),
        "stderr MUST be empty in JSON-mode envelope dispatch, got: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// An invalid glob pattern in `manifest.toml`'s `exclude` list surfaces as a
/// top-level `package::invalid_exclude_pattern` error (NOT inside a
/// `diagnostics[]` array), because the CLI maps it directly via
/// `json_error_from_sdk` to a `CliError::runtime(je)`.
#[test]
fn package_invalid_exclude_pattern_reports_named_error() {
    let tmp = TempDir::new().unwrap();
    // Create a fresh index via `new index`.
    let reg = tmp.path().join("reg");
    plugin()
        .args(["new", "index", reg.to_str().unwrap()])
        .assert()
        .success();

    // Plugin dir with a valid entry point but an invalid exclude pattern.
    let dir = tmp.path().join("bad_exclude");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("manifest.toml"),
        "manifest_schema_version = \"1.2\"\n[plugin]\nname=\"p\"\nversion=\"0.1.0\"\n\
         description=\"x\"\ntriggers=[\"process_writes\"]\nexclude=[\"[z-a]\"]\n\
         [dependencies]\ndatabase_version=\">=3.0.0\"\n",
    )
    .unwrap();
    fs::write(
        dir.join("__init__.py"),
        "def process_writes(influxdb3_local, table_batches, args):\n    pass\n",
    )
    .unwrap();

    let assert = plugin()
        .args([
            "package",
            dir.to_str().unwrap(),
            "--index",
            reg.join("index.json").to_str().unwrap(),
            "--out",
            tmp.path().join("out").to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .code(1);

    let out = assert.get_output();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let doc: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must be valid JSON: {e}\n{stdout}"));
    assert_eq!(
        doc["error"]["code"], "package::invalid_exclude_pattern",
        "expected package::invalid_exclude_pattern, got:\n{stdout}"
    );
    // The offending pattern must appear in `field` or `details.pattern`.
    let field = doc["error"]["field"].as_str().unwrap_or("");
    let details_pattern = doc["error"]["details"]["pattern"].as_str().unwrap_or("");
    assert!(
        field == "[z-a]" || details_pattern == "[z-a]",
        "expected [z-a] in field or details.pattern, got:\n{stdout}"
    );
    assert!(
        out.stderr.is_empty(),
        "stderr MUST be empty in JSON-mode envelope dispatch, got: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
}
