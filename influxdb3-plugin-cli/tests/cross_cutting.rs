//! Cross-cutting CLI invariants:
//!
//! - Every clap `env = "..."` binding starts with the `INFLUXDB3_PLUGIN_`
//!   prefix. v1 has no env-var-bound flags so the walk passes trivially
//!   today; the test locks the contract for any additive future flags.
//! - `clap` workspace pin meets the `>= 4.5.47` floor.
//! - `--output json` never emits ANSI escapes on stdout, even with
//!   `FORCE_COLOR=1`.
//! - `CI=true` (per-spawn env) forces json mode end-to-end.
//! - Observed exit codes are always in `{0, 1, 2}`.
//! - `--help` for top-level + each subcommand pinned via insta — the
//!   clap attribute surface is part of the cli's stable contract and
//!   snapshots catch silent drift.
//!
//! See `version_smoke.rs` for the rationale behind the crate-root allow.

#![allow(unused_crate_dependencies)]

use assert_cmd::Command;
use clap::CommandFactory;
use influxdb3_plugin_cli::PluginConfig;

const ENV_PREFIX: &str = "INFLUXDB3_PLUGIN_";

/// Recursively walk every command + subcommand and collect every
/// `arg.get_env()` value that does NOT start with [`ENV_PREFIX`].
fn collect_offending_envs(cmd: &clap::Command, into: &mut Vec<String>) {
    for arg in cmd.get_arguments() {
        if let Some(env) = arg.get_env() {
            let name = env.to_string_lossy().into_owned();
            if !name.starts_with(ENV_PREFIX) {
                into.push(format!("{} (arg `{}`)", name, arg.get_id().as_str()));
            }
        }
    }
    for sub in cmd.get_subcommands() {
        collect_offending_envs(sub, into);
    }
}

#[test]
fn every_env_var_binding_uses_influxdb3_plugin_prefix() {
    let mut offenders = Vec::new();
    collect_offending_envs(&PluginConfig::command(), &mut offenders);
    assert!(
        offenders.is_empty(),
        "every clap `env = ...` binding must start with `{ENV_PREFIX}`. \
         Offenders: {offenders:?}"
    );
}

/// Workspace pin for clap must be `>= 4.5.47`. Reads the workspace
/// root `Cargo.toml`, parses the `[workspace.dependencies]` clap entry,
/// asserts the version meets the floor.
#[test]
fn clap_workspace_pin_meets_floor() {
    const FLOOR: (u64, u64, u64) = (4, 5, 47);

    let workspace_toml = include_str!("../../Cargo.toml");
    let parsed: toml::Value = toml::from_str(workspace_toml).expect("workspace Cargo.toml is TOML");
    let clap_dep = parsed
        .get("workspace")
        .and_then(|w| w.get("dependencies"))
        .and_then(|d| d.get("clap"))
        .expect("workspace.dependencies.clap entry must exist");

    let version_str = match clap_dep {
        toml::Value::String(s) => s.clone(),
        toml::Value::Table(t) => t
            .get("version")
            .and_then(|v| v.as_str())
            .expect("clap dep table must carry version")
            .to_owned(),
        other => panic!("unexpected clap dep shape: {other:?}"),
    };

    let parts: Vec<u64> = version_str
        .trim_start_matches([' ', '=', '>', '^', '~'])
        .split('.')
        .map(|p| p.parse::<u64>().expect("version components are numeric"))
        .collect();
    assert!(
        parts.len() >= 3,
        "clap version {version_str:?} needs major.minor.patch"
    );
    let actual = (parts[0], parts[1], parts[2]);
    assert!(
        actual >= FLOOR,
        "workspace clap pin {version_str:?} ({actual:?}) is below floor {FLOOR:?}"
    );
}

/// Absolute rule: `--output json` on stdout NEVER emits ANSI escapes,
/// even when `FORCE_COLOR=1` is set in the spawn env. Exercised against
/// `validate` (which always emits a JSON document on stdout in the
/// validator idiom).
#[test]
fn json_stdout_emits_no_ansi_under_force_color() {
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("p");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("manifest.toml"),
        "manifest_schema_version = \"1.0\"\n\n\
         [plugin]\nname = \"p\"\nversion = \"1.0.0\"\n\
         description = \"x\"\ntriggers = [\"process_writes\"]\n\n\
         [dependencies]\ndatabase_version = \">=3.0.0\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("__init__.py"),
        "def process_writes(a, b, c):\n    pass\n",
    )
    .unwrap();

    let assert = Command::cargo_bin("influxdb3-plugin")
        .unwrap()
        .env("FORCE_COLOR", "1")
        .args(["validate", "--output", "json"])
        .arg(&dir)
        .assert()
        .success();
    let stdout = assert.get_output().stdout.clone();
    assert!(
        !stdout.windows(2).any(|w| w == [0x1b, b'[']),
        "stdout must not contain ANSI escape sequences in --output json mode \
         (absolute rule), got: {:?}",
        String::from_utf8_lossy(&stdout)
    );
}

/// End-to-end spot check: when `CI=true` is set (in addition to the non-TTY
/// piped stdout that `assert_cmd` already produces), stdout is a single valid
/// JSON document. This does NOT isolate `CI=true` as the sole trigger —
/// `assert_cmd`'s pipe already forces non-TTY → json. A PTY-based test that
/// would isolate `CI=true`'s independent effect is a deferred follow-up.
#[test]
fn ci_env_plus_pipe_yields_json_stdout() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("p");

    let assert = Command::cargo_bin("influxdb3-plugin")
        .unwrap()
        .env("CI", "true")
        .args(["new", "process_writes"])
        .arg(&target)
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let _: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout failed to parse as JSON: {e}\n{stdout}"));
}

/// Every observed exit code is in `{0, 1, 2}`. Spawns one command per
/// code and asserts each result.
#[test]
fn observed_exit_codes_are_in_documented_set() {
    let td = tempfile::tempdir().unwrap();

    // Code 0: --version (always succeeds).
    let zero = Command::cargo_bin("influxdb3-plugin")
        .unwrap()
        .arg("--version")
        .assert()
        .success();
    assert_eq!(zero.get_output().status.code(), Some(0));

    // Code 2: clap usage error (unknown subcommand).
    let two = Command::cargo_bin("influxdb3-plugin")
        .unwrap()
        .arg("garbage_subcommand")
        .assert()
        .failure();
    assert_eq!(two.get_output().status.code(), Some(2));

    // Code 1: runtime failure (yank against a nonexistent --index).
    let one = Command::cargo_bin("influxdb3-plugin")
        .unwrap()
        .args(["yank", "p@1.0.0"])
        .arg("--index")
        .arg(td.path().join("nonexistent.json"))
        .arg("--out")
        .arg(td.path().join("out"))
        .assert()
        .failure();
    assert_eq!(one.get_output().status.code(), Some(1));
}

/// Help-text snapshots for the top-level binary and each subcommand.
/// The clap attribute surface (arg names, env-var bindings, version
/// declaration) is part of the cli's stable contract; pinning `--help`
/// output catches silent drift in any externally-observable projection
/// of that surface.
#[test]
fn help_text_snapshots() {
    for (name, args) in [
        ("top", &["--help"][..]),
        ("new", &["new", "--help"]),
        ("validate", &["validate", "--help"]),
        ("package", &["package", "--help"]),
        ("yank", &["yank", "--help"]),
        ("search", &["search", "--help"]),
        ("info", &["info", "--help"]),
    ] {
        let assert = Command::cargo_bin("influxdb3-plugin")
            .unwrap()
            .args(args)
            .assert()
            .success();
        let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
        insta::assert_snapshot!(format!("help_{name}"), stdout);
    }
}

/// Asserts that `stdout` contains a valid JSON envelope with
/// `"status":"error"` and that stderr is empty — the envelope-mode
/// contract for clap parse failures in JSON mode.
fn assert_json_error_envelope(output: &std::process::Output) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.is_empty(),
        "stderr must be empty in JSON-mode envelope dispatch; got:\n{stderr}"
    );
    let doc: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must be valid JSON: {e}\n{stdout}"));
    assert_eq!(
        doc.get("status").and_then(|v| v.as_str()),
        Some("error"),
        "envelope status must be \"error\"; got:\n{stdout}"
    );
    assert!(
        doc.get("error").is_some(),
        "envelope must carry an \"error\" field; got:\n{stdout}"
    );
}

/// `--output json` usage errors must emit a JSON error envelope on
/// stdout and empty stderr. Applies to clap parse failures.
#[test]
fn json_mode_usage_error_emits_envelope_for_new() {
    let assert = Command::cargo_bin("influxdb3-plugin")
        .unwrap()
        .args(["new", "not_a_template", "--output", "json"])
        .assert()
        .failure();

    assert_eq!(assert.get_output().status.code(), Some(2));
    assert_json_error_envelope(assert.get_output());
}

#[test]
fn ci_env_triggers_json_envelope_for_usage_errors() {
    let assert = Command::cargo_bin("influxdb3-plugin")
        .unwrap()
        .env("CI", "true")
        .args(["new", "not_a_template"])
        .assert()
        .failure();

    assert_eq!(assert.get_output().status.code(), Some(2));
    assert_json_error_envelope(assert.get_output());
}

/// validate with an unknown flag — confirms main-level interception
/// applies to subcommands other than `new`.
#[test]
fn json_mode_validate_unknown_flag_emits_envelope() {
    let assert = Command::cargo_bin("influxdb3-plugin")
        .unwrap()
        .args(["validate", "--nope", "--output", "json"])
        .assert()
        .failure();
    assert_json_error_envelope(assert.get_output());
}

/// package with no positional — confirms the collapse covers the
/// missing-required class of clap error, not only unknown-value.
#[test]
fn json_mode_package_missing_required_emits_envelope() {
    let assert = Command::cargo_bin("influxdb3-plugin")
        .unwrap()
        .args(["package", "--output", "json"])
        .assert()
        .failure();
    assert_json_error_envelope(assert.get_output());
}

/// Human mode must keep clap's full multi-line diagnostic — including the
/// `For more information, try '--help'.` footer.
#[test]
fn explicit_human_mode_preserves_multi_line_clap_output() {
    let assert = Command::cargo_bin("influxdb3-plugin")
        .unwrap()
        .args(["new", "not_a_template", "--output", "human"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains("For more information"),
        "human mode must preserve clap's full diagnostic (with help footer); got:\n{stderr}"
    );
}

/// Safety guard: no representative production path should emit
/// `cli::unknown`. That code is the fallback for plain `anyhow::Error`
/// escaping the typed `CliError` wiring. If this fires, a call site is
/// returning a bare `anyhow!` instead of `CliError::runtime(JsonError)`.
#[test]
fn no_production_path_emits_cli_unknown() {
    let cases: &[&[&str]] = &[
        // Clap-detected usage failures (missing required args)
        &["package"],
        &["yank"],
        // Runtime failure from a missing input
        &["validate", "/path/that/definitely/does/not/exist/plugin"],
        // Success: new list
        &["new", "list", "--output", "json"],
    ];
    for argv in cases {
        let mut cmd = Command::cargo_bin("influxdb3-plugin").unwrap();
        cmd.args(*argv).env("CI", "true");
        let out = cmd.output().unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            !stdout.contains(r#""cli::unknown""#),
            "argv {argv:?} produced a cli::unknown envelope; \
             a typed CliError is missing somewhere. stdout:\n{stdout}"
        );
    }
}

/// Regression guard for the SDK-error mapping hardening gap: the central SDK
/// mapper must not keep a wildcard arm that turns future SDK variants into
/// `cli::unknown`. The fix should replace this with a typed/contextual code or
/// otherwise make new variants impossible to map silently.
#[test]
fn sdk_error_mapping_must_not_fallback_to_cli_unknown() {
    let source = include_str!("../src/output/error_mapping.rs");
    let body = source
        .split_once("pub(crate) fn json_error_from_sdk")
        .and_then(|(_, rest)| rest.split_once("\n}\n\n#[cfg(test)]"))
        .map(|(body, _)| body)
        .expect("json_error_from_sdk body should be locatable");

    assert!(
        !body.contains("\"cli::unknown\""),
        "json_error_from_sdk still contains a cli::unknown fallback; \
         future SDK variants can silently lose typed JSON error codes"
    );
}

/// Regression guard for the validate-smoke coverage gap: validate JSON-error
/// tests should assert the expected validate namespace/code, not permit
/// `cli::`. Allowing `cli::` would mask the same fallback this suite is meant
/// to catch.
#[test]
fn validate_smoke_tests_must_not_allow_cli_namespace_error_codes() {
    let source = include_str!("validate_smoke.rs");
    let forbidden_prefix = concat!("cli", "::");
    let forbidden = format!("starts_with({forbidden_prefix:?})");

    assert!(
        !source.contains(&forbidden),
        "validate_smoke.rs still permits cli:: error codes; tighten those \
         assertions before fixing the mapper"
    );
}
