//! Exit-code classification smoke — pins the mapping between
//! `CliError` variants and process exit codes (S2-18).

#![allow(unused_crate_dependencies, unused_imports)]

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn plugin() -> Command {
    Command::cargo_bin("influxdb3-plugin").expect("binary not built")
}

#[test]
fn success_exits_zero() {
    plugin().arg("--version").assert().code(0);
}

#[test]
fn clap_parse_error_exits_two() {
    plugin().arg("validate").arg("--nope").assert().code(2);
}

#[test]
fn missing_required_flag_exits_two() {
    plugin().arg("package").arg("some-dir").assert().code(2);
}

#[test]
fn plain_runtime_failure_exits_one() {
    // `validate` against a non-existent directory: I/O error → runtime fail.
    plugin()
        .arg("validate")
        .arg("/nonexistent/plugin/dir")
        .assert()
        .code(1);
}

#[test]
fn new_with_invalid_explicit_name_exits_two() {
    let tmp = TempDir::new().unwrap();
    plugin()
        .args([
            "new",
            "process_writes",
            tmp.path().join("ok-dir").to_str().unwrap(),
            "--name",
            "Bad_Name",
        ])
        .assert()
        .code(2);
}
