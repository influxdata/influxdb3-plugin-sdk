//! Color smoke — `FORCE_COLOR=1` on human mode emits ANSI; `NO_COLOR=1`
//! on the same command path does not. Locks the color-precedence table
//! end-to-end.
//!
//! `assert_cmd` spawns without a real TTY, so the default (no env override)
//! path yields no color. These tests therefore probe the `FORCE_COLOR`
//! branch explicitly and pin the JSON-stdout absolute rule.
//!
//! Stream routing note: in human mode, `validate`
//! writes the diagnostics block to STDOUT and only the summary line to
//! STDERR (via the anyhow error `main.rs` prints with `eprintln!("{e:#}")`).
//! So `FORCE_COLOR=1` + `--output human` lands ANSI on stdout primarily;
//! stderr may or may not carry escapes depending on how anyhow formats the
//! chain. We check stdout for the ANSI presence/absence assertions.

#![allow(unused_crate_dependencies)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn plugin() -> Command {
    let mut c = Command::cargo_bin("influxdb3-plugin").expect("binary not built");
    c.env_remove("NO_COLOR")
        .env_remove("FORCE_COLOR")
        .env_remove("TERM");
    c
}

fn bad_plugin() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let d = tmp.path().join("p");
    fs::create_dir_all(&d).unwrap();
    fs::write(
        d.join("manifest.toml"),
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
        d.join("__init__.py"),
        "def process_writes(influxdb3_local, table_batches, args):\n    pass\n",
    )
    .unwrap();
    tmp
}

#[test]
fn force_color_emits_ansi_on_pipe() {
    // Human-mode diagnostics land on stdout (Task 4.1's stream routing).
    // With FORCE_COLOR=1 the palette is populated and ANSI escapes appear
    // on the diagnostics stream.
    let tmp = bad_plugin();
    plugin()
        .env("FORCE_COLOR", "1")
        .args([
            "validate",
            tmp.path().join("p").to_str().unwrap(),
            "--output",
            "human",
        ])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("\x1b["));
}

#[test]
fn no_color_suppresses_ansi_even_with_force() {
    // NO_COLOR precedes FORCE_COLOR per no-color.org. Neither stream may
    // carry ANSI.
    let tmp = bad_plugin();
    let assert = plugin()
        .env("NO_COLOR", "1")
        .env("FORCE_COLOR", "1")
        .args([
            "validate",
            tmp.path().join("p").to_str().unwrap(),
            "--output",
            "human",
        ])
        .assert()
        .code(1);
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        !stdout.contains("\x1b["),
        "NO_COLOR must suppress ANSI on stdout, got: {stdout}"
    );
    assert!(
        !stderr.contains("\x1b["),
        "NO_COLOR must suppress ANSI on stderr, got: {stderr}"
    );
}

#[test]
fn json_mode_never_colorizes_stdout() {
    // Absolute rule: JSON on stdout is byte-stable; FORCE_COLOR must not
    // override. stderr is stream-silent per the validator idiom so we only
    // pin the stdout side here.
    let tmp = bad_plugin();
    plugin()
        .env("FORCE_COLOR", "1")
        .args([
            "validate",
            tmp.path().join("p").to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("\x1b[").not());
}
