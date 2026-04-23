//! Cross-cutting CLI invariants from Spec 2:
//!
//! - **S2-9** — every clap `env = "..."` binding starts with the
//!   `INFLUXDB3_PLUGIN_` prefix. v1 has no env-var-bound flags so the
//!   walk passes trivially today; the test locks the contract for any
//!   additive future flags.
//! - **S2-6** — `clap` workspace pin meets the `>= 4.5.47` floor.
//! - **S2-17 absolute rule** — `--output json` never emits ANSI escapes
//!   on stdout, even with `FORCE_COLOR=1`.
//! - **S2-14** — `CI=true` (per-spawn env) forces json mode end-to-end.
//! - **S2-18** — observed exit codes are always in `{0, 1, 2}`.
//! - **Help-text snapshots** — `--help` for top-level + each subcommand
//!   pinned via insta. Spec 2 § Stability names the clap attribute
//!   surface as part of the cli's stable contract.
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
        "S2-9: every clap `env = ...` binding must start with `{ENV_PREFIX}`. \
         Offenders: {offenders:?}"
    );
}

/// S2-6: workspace pin for clap must be `>= 4.5.47`. Reads the workspace
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
        "S2-6: workspace clap pin {version_str:?} ({actual:?}) is below floor {FLOOR:?}"
    );
}

/// S2-17 absolute rule: `--output json` on stdout NEVER emits ANSI
/// escapes, even when `FORCE_COLOR=1` is set in the spawn env.
/// Exercised against `validate` (which always emits a JSON document on
/// stdout in the validator idiom).
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
         (S2-17 absolute rule), got: {:?}",
        String::from_utf8_lossy(&stdout)
    );
}

/// S2-14 e2e: `CI=true` (per-spawn env) forces json mode when no
/// explicit `--output` is given. We invoke `new` since it's the simplest
/// command that emits JSON on success.
#[test]
fn ci_env_forces_json_mode_e2e() {
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
    let _: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "CI=true must force json mode (S2-14); stdout failed to parse as JSON: {e}\n{stdout}"
        )
    });
}

/// S2-18: every observed exit code is in `{0, 1, 2}`. Spawns one
/// command per code and asserts each result.
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
/// Spec 2 § Stability names the clap attribute surface (arg names,
/// env-var bindings, version declaration) as part of the cli's stable
/// contract; pinning `--help` output catches silent drift in any
/// externally-observable projection of that surface.
#[test]
fn help_text_snapshots() {
    for (name, args) in [
        ("top", &["--help"][..]),
        ("new", &["new", "--help"]),
        ("validate", &["validate", "--help"]),
        ("package", &["package", "--help"]),
        ("yank", &["yank", "--help"]),
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
