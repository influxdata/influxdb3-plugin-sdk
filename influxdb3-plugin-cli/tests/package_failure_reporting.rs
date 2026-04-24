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
        .args(["new", "registry", reg.to_str().unwrap()])
        .assert()
        .success();
    let dir = tmp.path().join("bad");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("manifest.toml"),
        r#"manifest_schema_version = "1.0"

[plugin]
name = "Bad_Name"
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
fn package_json_failure_keeps_stdout_empty_and_stderr_is_one_line() {
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
        .code(1)
        .stdout(predicate::str::is_empty());
    // `package`'s JSON failure path emits the human-readable error line
    // on stderr — singular. Enforce one meaningful line and no JSON escape
    // on stderr so the data-tool contract stays tight.
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    let lines: Vec<&str> = stderr
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    assert_eq!(lines.len(), 1, "expected 1 stderr line, got: {stderr:?}");
    assert!(
        !stderr.contains('{'),
        "stderr should not contain JSON, got: {stderr}"
    );
}
