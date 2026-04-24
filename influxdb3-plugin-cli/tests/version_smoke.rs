//! Integration tests for the `influxdb3-plugin --version` flag.
//!
//! Pins the output shape:
//! ```text
//! influxdb3-plugin <version> (<short-sha> <build-date>)
//! ```
//! and verifies the graceful-degradation `(unknown)` form when build-time
//! git/date metadata is unavailable.
//!
//! See `validate_smoke.rs` in the SDK crate for the rationale behind the
//! crate-root allow.

#![allow(unused_crate_dependencies)]

use assert_cmd::Command;
use predicates::Predicate as _;
use predicates::str::is_match;

const VERSION_RE: &str =
    r"^influxdb3-plugin \d+\.\d+\.\d+ (\([a-f0-9]{7} \d{4}-\d{2}-\d{2}\)|\(unknown\))\n$";

#[test]
fn version_output_shape_matches_spec() {
    let output = Command::cargo_bin("influxdb3-plugin")
        .expect("binary builds")
        .arg("--version")
        .output()
        .expect("spawning the binary succeeds");

    assert!(
        output.status.success(),
        "--version must exit 0, got {}",
        output.status
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    let predicate = is_match(VERSION_RE).expect("regex compiles");
    assert!(
        predicate.eval(&stdout),
        "version output {stdout:?} does not match regex {VERSION_RE}"
    );

    // Version portion MUST match the cli crate's CARGO_PKG_VERSION.
    let expected_prefix = format!("influxdb3-plugin {} ", env!("CARGO_PKG_VERSION"));
    assert!(
        stdout.starts_with(&expected_prefix),
        "expected version prefix {expected_prefix:?}, got {stdout:?}"
    );

    assert!(
        output.stderr.is_empty(),
        "--version must keep stderr empty, got {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// `--version` is exempt from `--output`. clap treats `--output` at the top
/// level as an unknown argument (the flag lives on subcommands), so the
/// spawn fails at parse time with exit 2 — and the stdout invariant for
/// `--version` (plain text, not JSON) still holds: stdout must NOT contain
/// a JSON document.
#[test]
fn version_output_flag_does_not_emit_json_on_stdout() {
    let output = Command::cargo_bin("influxdb3-plugin")
        .expect("binary builds")
        .args(["--version", "--output", "json"])
        .output()
        .expect("spawning the binary succeeds");

    if output.status.success() {
        // Some clap versions tolerate trailing args after `--version`. If so,
        // stdout must still be the plain-text `--version` shape.
        let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
        let predicate = is_match(VERSION_RE).expect("regex compiles");
        assert!(
            predicate.eval(&stdout),
            "stdout {stdout:?} must match plain-text version regex even with --output json"
        );
    } else {
        // clap rejected `--output` at the top level → exit 2 (usage error);
        // stdout must be empty (no partial JSON).
        assert_eq!(
            output.status.code(),
            Some(2),
            "usage error must exit 2 (S2-18); got {:?}",
            output.status.code()
        );
        let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
        assert!(
            !stdout.trim_start().starts_with('{'),
            "usage error must not emit JSON on stdout, got {stdout:?}"
        );
    }
}
