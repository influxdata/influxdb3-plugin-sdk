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
    // Runtime failure path: `yank` against a nonexistent `--index` hits the
    // "failed to read --index" anyhow branch in commands/yank.rs, which
    // surfaces as CliError::Runtime (exit 1) with stderr content. Avoids
    // the silent JSON-mode validation path (S2-15) that `validate` would take.
    let tmp = TempDir::new().unwrap();
    plugin()
        .args([
            "yank",
            "downsampler@1.2.0",
            "--index",
            tmp.path().join("does-not-exist.json").to_str().unwrap(),
            "--out",
            tmp.path().join("out").to_str().unwrap(),
        ])
        .assert()
        .code(1)
        .stderr(predicates::str::contains("failed to read --index"));
}

#[test]
fn new_database_version_on_registry_template_exits_two() {
    let tmp = TempDir::new().unwrap();
    plugin()
        .args([
            "new",
            "registry",
            tmp.path().join("r").to_str().unwrap(),
            "--database-version",
            ">=3",
        ])
        .assert()
        .code(2)
        .stderr(predicates::str::contains(
            "--database-version is not supported",
        ));
}

#[test]
fn package_self_overwrite_exits_two() {
    let tmp = TempDir::new().unwrap();
    let reg = tmp.path().join("reg");
    plugin()
        .args(["new", "registry", reg.to_str().unwrap()])
        .assert()
        .success();
    let plug = tmp.path().join("p");
    plugin()
        .args(["new", "process_writes", plug.to_str().unwrap()])
        .assert()
        .success();
    plugin()
        .args([
            "package",
            plug.to_str().unwrap(),
            "--index",
            reg.join("index.json").to_str().unwrap(),
            "--out",
            reg.to_str().unwrap(),
        ])
        .assert()
        .code(2)
        .stderr(predicates::str::contains("S2-12"));
}

#[test]
fn yank_self_overwrite_exits_two() {
    let tmp = TempDir::new().unwrap();
    let reg = tmp.path().join("reg");
    plugin()
        .args(["new", "registry", reg.to_str().unwrap()])
        .assert()
        .success();
    // Empty index — `yank` will fail at entry lookup, but the S2-12
    // path-overlap check must fire FIRST and return 2.
    plugin()
        .args([
            "yank",
            "whatever@1.0.0",
            "--index",
            reg.join("index.json").to_str().unwrap(),
            "--out",
            reg.to_str().unwrap(),
        ])
        .assert()
        .code(2)
        .stderr(predicates::str::contains("S2-12"));
}

#[test]
fn yank_malformed_target_exits_two() {
    let tmp = TempDir::new().unwrap();
    let reg = tmp.path().join("reg");
    plugin()
        .args(["new", "registry", reg.to_str().unwrap()])
        .assert()
        .success();
    plugin()
        .args([
            "yank",
            "name:version",  // `:` not `@`
            "--index",
            reg.join("index.json").to_str().unwrap(),
            "--out",
            tmp.path().join("y").to_str().unwrap(),
        ])
        .assert()
        .code(2)
        .stderr(
            predicates::str::contains("name:version")
                .and(predicates::str::contains("<name>@<version>")),
        );
}
