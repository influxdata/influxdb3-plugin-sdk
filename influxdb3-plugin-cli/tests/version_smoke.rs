//! Integration tests for the `influxdb3-plugin --version` flag plus
//! grammar-coverage tests for `VERSION_RE` itself.
//!
//! Pins the output shape:
//! ```text
//! influxdb3-plugin <version>, revision <sha>
//! ```
//! the graceful-degradation `revision unknown` form when no SHA source
//! produces a value, and the regex's acceptance of the full SemVer
//! grammar (pre-release identifiers and build metadata).
//!
//! See `validate_smoke.rs` in the SDK crate for the rationale behind the
//! crate-root allow.

#![allow(unused_crate_dependencies)]

use assert_cmd::Command;
use predicates::Predicate as _;
use predicates::str::is_match;

const VERSION_RE: &str = r"^influxdb3-plugin \d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?, revision ([a-f0-9]{40}|unknown)\n$";

/// Exercises `VERSION_RE` directly so coverage of the full SemVer
/// grammar (pre-release identifiers + build metadata) and the SHA
/// grammar (40-char lowercase hex or literal `unknown`) does not
/// depend on the cli crate's current `Cargo.toml` version or on the
/// build environment producing a real SHA.
#[test]
fn version_re_accepts_full_semver_grammar() {
    let predicate = is_match(VERSION_RE).expect("regex compiles");

    // 40-char lowercase hex SHA used in positive cases below.
    let sha = "a5ed19d8e3f4c2b1a09f8e6d5c4b3a2918f7e6d5";

    // Stable form — primary positive case; guards against the regex
    // accidentally requiring a pre-release or build-metadata segment.
    assert!(
        predicate.eval(&format!("influxdb3-plugin 1.2.3, revision {sha}\n")),
        "stable MAJOR.MINOR.PATCH must match"
    );

    // Org tag convention is `vX.Y.Z-N.(alpha|beta|rc).N` — pin all three
    // channels so a single-channel grammar regression is caught.
    assert!(
        predicate.eval(&format!(
            "influxdb3-plugin 1.0.0-1.alpha.0, revision {sha}\n"
        )),
        "alpha channel pre-release must match"
    );
    assert!(
        predicate.eval(&format!(
            "influxdb3-plugin 1.0.0-1.beta.5, revision {sha}\n"
        )),
        "beta channel pre-release must match"
    );
    assert!(
        predicate.eval(&format!("influxdb3-plugin 3.9.0-1.rc.0, revision {sha}\n")),
        "rc channel pre-release must match"
    );

    // Single-segment pre-release — valid per SemVer though outside the
    // org's three-channel tag convention.
    assert!(
        predicate.eval(&format!("influxdb3-plugin 1.2.3-alpha, revision {sha}\n")),
        "single-segment pre-release must match"
    );

    // Build metadata.
    assert!(
        predicate.eval(&format!(
            "influxdb3-plugin 1.2.3+build.42, revision {sha}\n"
        )),
        "build metadata must match"
    );

    // Pre-release plus build metadata.
    assert!(
        predicate.eval(&format!(
            "influxdb3-plugin 1.2.3-rc.1+sha.deadbee, revision {sha}\n"
        )),
        "combined pre-release + build metadata must match"
    );

    // Graceful degradation: the literal `unknown` is the only non-hex
    // SHA accepted.
    assert!(
        predicate.eval("influxdb3-plugin 1.2.3, revision unknown\n"),
        "graceful-degradation `revision unknown` must match"
    );

    // Negative cases — version-segment.

    assert!(
        !predicate.eval(&format!("influxdb3-plugin 1.2, revision {sha}\n")),
        "two-segment version must NOT match (regex requires MAJOR.MINOR.PATCH)"
    );
    assert!(
        !predicate.eval(&format!("influxdb3-plugin 1.2.3-, revision {sha}\n")),
        "trailing hyphen with empty pre-release must NOT match"
    );
    assert!(
        !predicate.eval(&format!("influxdb3-plugin 1.2.3+, revision {sha}\n")),
        "trailing plus with empty build metadata must NOT match"
    );

    // Negative cases — SHA-segment. Pin the 40-char lowercase-hex
    // requirement; over-broadening would let short SHAs or uppercase
    // through.
    assert!(
        !predicate.eval("influxdb3-plugin 1.2.3, revision a5ed19d\n"),
        "7-char short SHA must NOT match (regex requires 40-char)"
    );
    assert!(
        !predicate.eval(&format!(
            "influxdb3-plugin 1.2.3, revision {}\n",
            sha.to_uppercase()
        )),
        "uppercase-hex SHA must NOT match (regex requires lowercase)"
    );
}

#[test]
fn version_output_shape_matches_pattern() {
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
    let expected_prefix = format!("influxdb3-plugin {}, revision ", env!("CARGO_PKG_VERSION"));
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
            "usage error must exit 2; got {:?}",
            output.status.code()
        );
        let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
        assert!(
            !stdout.trim_start().starts_with('{'),
            "usage error must not emit JSON on stdout, got {stdout:?}"
        );
    }
}
