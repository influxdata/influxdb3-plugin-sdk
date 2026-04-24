//! Integration tests for `influxdb3-plugin new`.
//!
//! Covers per-template happy paths, conflict behavior, invalid-name
//! handling, and the data-tool JSON idiom (single document on stdout for
//! success; empty stdout + stderr error on failure).
//!
//! See `version_smoke.rs` for the rationale behind the crate-root allow.

#![allow(unused_crate_dependencies)]

use std::path::Path;

mod common;
use common::cli_cmd;

/// Spawns `influxdb3-plugin` with `args` and an empty CWD-relative
/// environment so per-test invocations remain isolated.
fn spawn_new<P: AsRef<Path>>(target: P, extra_args: &[&str]) -> assert_cmd::assert::Assert {
    let mut cmd = cli_cmd();
    cmd.arg("new");
    for a in extra_args {
        cmd.arg(a);
    }
    cmd.arg(target.as_ref());
    cmd.assert()
}

#[test]
fn new_process_writes_happy_path_human_mode() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("hp");

    spawn_new(&target, &["process_writes"]).success();

    assert!(target.join("manifest.toml").exists());
    assert!(target.join("__init__.py").exists());
    assert!(target.join("README.md").exists());

    let manifest = std::fs::read_to_string(target.join("manifest.toml")).unwrap();
    assert!(manifest.contains("name = \"hp\""), "manifest: {manifest}");
    assert!(
        manifest.contains("triggers = [\"process_writes\"]"),
        "manifest: {manifest}"
    );
}

/// JSON-mode happy path emits a stable schema on stdout (data-tool idiom).
/// Snapshot the JSON shape minus the absolute-path field (which varies
/// per machine).
#[test]
fn new_process_writes_happy_path_json_mode() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("downsampler");

    let assert = spawn_new(&target, &["process_writes", "--output", "json"]).success();

    let out = assert.get_output();
    assert!(
        out.stderr.is_empty(),
        "stderr should be empty on success, got {:?}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = std::str::from_utf8(&out.stdout).unwrap();
    let mut payload: serde_json::Value = serde_json::from_str(stdout).expect("stdout is JSON");
    // Strip the absolute path before snapshotting so the snapshot is
    // machine-independent.
    payload
        .as_object_mut()
        .unwrap()
        .insert("target_dir".into(), "<TMPDIR>/downsampler".into());

    insta::assert_json_snapshot!("new_process_writes_json", payload);
}

/// Snapshot the JSON output for one scaffold template. `target` is the
/// path basename used to derive the plugin name (and the redacted
/// fragment in the snapshot's `target_dir`).
fn snapshot_new_template(template: &str, target: &str, snapshot_name: &str) {
    let td = tempfile::tempdir().unwrap();
    let target_path = td.path().join(target);
    let assert = spawn_new(&target_path, &[template, "--output", "json"]).success();
    let stdout = std::str::from_utf8(&assert.get_output().stdout).unwrap();
    let mut payload: serde_json::Value = serde_json::from_str(stdout).expect("stdout is JSON");
    let placeholder = format!("<TMPDIR>/{target}");
    payload
        .as_object_mut()
        .unwrap()
        .insert("target_dir".into(), placeholder.into());
    insta::assert_json_snapshot!(snapshot_name, payload);
}

/// Every per-template JSON output is a stable schema commitment. One
/// snapshot per template locks that contract. `process_writes` is covered
/// by `new_process_writes_happy_path_json_mode` above; this group covers
/// the remaining three.

#[test]
fn new_process_scheduled_call_json_snapshot() {
    snapshot_new_template(
        "process_scheduled_call",
        "downsampler",
        "new_process_scheduled_call_json",
    );
}

#[test]
fn new_process_request_json_snapshot() {
    snapshot_new_template("process_request", "downsampler", "new_process_request_json");
}

#[test]
fn new_registry_json_snapshot() {
    snapshot_new_template("registry", "reg", "new_registry_json");
}

#[test]
fn new_each_plugin_template_writes_matching_init() {
    for template in [
        "process_writes",
        "process_scheduled_call",
        "process_request",
    ] {
        let td = tempfile::tempdir().unwrap();
        let target = td.path().join("p");

        spawn_new(&target, &[template]).success();

        let init = std::fs::read_to_string(target.join("__init__.py")).unwrap();
        assert!(
            init.contains(&format!("def {template}(")),
            "expected `def {template}(` in {template} init, got:\n{init}"
        );
    }
}

#[test]
fn new_registry_happy_path_writes_file_url() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("reg");

    spawn_new(&target, &["registry"]).success();

    let index = std::fs::read_to_string(target.join("index.json")).unwrap();
    assert!(
        index.contains("\"artifacts_url\": \"file://"),
        "registry index should default artifacts_url to file://, got:\n{index}"
    );
}

/// Explicit `--artifacts-url` is written through verbatim (https / http
/// inclusive).
#[test]
fn new_registry_with_explicit_artifacts_url() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("reg");

    spawn_new(
        &target,
        &[
            "registry",
            "--artifacts-url",
            "https://plugins.example.com/artifacts",
        ],
    )
    .success();

    let index = std::fs::read_to_string(target.join("index.json")).unwrap();
    assert!(
        index.contains("\"artifacts_url\": \"https://plugins.example.com/artifacts\""),
        "explicit --artifacts-url should be written verbatim, got:\n{index}"
    );
}

/// `--database-version` overrides the default substitution baked into
/// the template.
#[test]
fn new_plugin_with_explicit_database_version() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("p");

    spawn_new(
        &target,
        &["process_writes", "--database-version", ">=3.5.0,<4.0.0"],
    )
    .success();

    let manifest = std::fs::read_to_string(target.join("manifest.toml")).unwrap();
    assert!(
        manifest.contains("database_version = \">=3.5.0,<4.0.0\""),
        "explicit --database-version should be written, got:\n{manifest}"
    );
}

/// `new` errors and writes nothing when any target file already exists
/// (writes full file set or nothing).
#[test]
fn new_errors_on_pre_existing_file() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("p");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("manifest.toml"), "pre-existing").unwrap();

    spawn_new(&target, &["process_writes"]).failure().code(1);

    assert_eq!(
        std::fs::read_to_string(target.join("manifest.toml")).unwrap(),
        "pre-existing",
        "pre-existing file should be preserved"
    );
    assert!(!target.join("__init__.py").exists());
}

/// Invalid path basename → exit 1 with stderr instructing `--name`.
/// (clap's exit-2 path applies only to argument-parse failures; an
/// invalid basename is a runtime-validation failure surfaced by the
/// command body.)
#[test]
fn new_rejects_invalid_basename_without_name_override() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("1bad");

    let assert = spawn_new(&target, &["process_writes"]).failure().code(1);

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.contains("--name"),
        "stderr should hint at --name, got: {stderr}"
    );
    assert!(!target.join("manifest.toml").exists());
}

/// Explicit `--name <bad>` also rejected, with a different message.
#[test]
fn new_rejects_invalid_explicit_name() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("ok");

    let assert = spawn_new(&target, &["process_writes", "--name", "1bad"])
        .failure()
        .code(2);

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.contains("1bad"),
        "stderr should name the bad value, got: {stderr}"
    );
    assert!(!target.join("manifest.toml").exists());
}

/// Plugin-template flags rejected with the registry template, and vice
/// versa. Surfaces nonsensical combinations at runtime rather than
/// silently ignoring them.
#[test]
fn new_rejects_artifacts_url_on_plugin_template() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("p");

    spawn_new(
        &target,
        &["process_writes", "--artifacts-url", "https://example.com/a"],
    )
    .failure()
    .code(2)
    .stderr(predicates::str::contains("--artifacts-url"));
}

#[test]
fn new_rejects_name_on_registry_template() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("r");

    spawn_new(&target, &["registry", "--name", "x"])
        .failure()
        .code(2)
        .stderr(predicates::str::contains("--name"));
}

/// Unknown template → clap parse error → exit code 2 (usage error), and
/// stderr points users at `new list` for template discovery.
#[test]
fn new_unknown_template_exits_two() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("p");

    cli_cmd()
        .args(["new", "garbage_template", target.to_str().unwrap()])
        .assert()
        .failure()
        .code(2)
        .stderr(predicates::str::contains("garbage_template"))
        .stderr(predicates::str::contains("new list"));
}

/// Data-tool failure path: stdout empty, error on stderr.
#[test]
fn new_failure_in_json_mode_keeps_stdout_empty() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("p");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("manifest.toml"), "pre-existing").unwrap();

    let assert = spawn_new(&target, &["process_writes", "--output", "json"])
        .failure()
        .code(1);

    let out = assert.get_output();
    assert!(
        out.stdout.is_empty(),
        "stdout MUST be empty on data-tool failure (S2-15), got: {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        !out.stderr.is_empty(),
        "stderr MUST carry the human-readable error (S2-15)"
    );
}

#[test]
fn new_conflict_error_mentions_path_once() {
    use assert_cmd::Command;
    use std::fs;
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("conflict");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("manifest.toml"), "pre-existing").unwrap();

    let output = Command::cargo_bin("influxdb3-plugin")
        .unwrap()
        .args([
            "new",
            "process_writes",
            dir.to_str().unwrap(),
            "--output",
            "human",
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(1),
        "conflict error should be a runtime failure per S2-18; got {:?}",
        output.status.code()
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    let occurrences = stderr.matches(dir.to_str().unwrap()).count();
    assert_eq!(
        occurrences, 1,
        "stderr should mention the conflicting path exactly once; was:\n{stderr}"
    );

    // After the Chunk 6 polish, "already exists" should appear exactly
    // once in the rendered error chain (anyhow's source-walk plus
    // `#[source]` no longer duplicates the inner io::Error's message).
    let phrase_occurrences = stderr.matches("already exists").count();
    assert_eq!(
        phrase_occurrences, 1,
        "phrase 'already exists' should appear exactly once; was:\n{stderr}"
    );
}

#[test]
fn new_list_human_mode_shows_templates() {
    // Explicit `--output human`: under the test harness stdout is piped,
    // so auto-detection would pick json (covered by the snapshot test
    // below).
    let assert = cli_cmd()
        .args(["new", "list", "--output", "human"])
        .assert()
        .success();
    let stdout = std::str::from_utf8(&assert.get_output().stdout).unwrap();

    assert!(stdout.contains("Template Name"), "stdout: {stdout}");
    assert!(stdout.contains("Short Name"), "stdout: {stdout}");
    // One row per built-in template (short names are the stable contract).
    for short in [
        "process_writes",
        "process_scheduled_call",
        "process_request",
        "registry",
    ] {
        assert!(stdout.contains(short), "missing `{short}` in:\n{stdout}");
    }
    // Descriptions are intentionally withheld from `list`; they appear
    // in `new <template> -h`.
    assert!(
        !stdout.contains("Plugin triggered by rows written"),
        "description leaked into list output:\n{stdout}"
    );
}

#[test]
fn new_list_json_mode_is_stable_schema() {
    let assert = cli_cmd()
        .arg("new")
        .arg("list")
        .arg("--output")
        .arg("json")
        .assert()
        .success();

    let out = assert.get_output();
    assert!(
        out.stderr.is_empty(),
        "stderr not empty: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = std::str::from_utf8(&out.stdout).unwrap();
    let payload: serde_json::Value = serde_json::from_str(stdout).expect("stdout is JSON");
    insta::assert_json_snapshot!("new_list_json", payload);
}

#[test]
fn new_help_does_not_enumerate_templates() {
    let assert = cli_cmd().arg("new").arg("--help").assert().success();
    let stdout = std::str::from_utf8(&assert.get_output().stdout).unwrap();

    // Scope the assertion to the `Commands:` block: `registry` also
    // appears in the after-help prose, which should not trip this check.
    let commands_block = stdout
        .split_once("Commands:")
        .and_then(|(_, after)| after.split_once("Options:"))
        .map(|(block, _)| block)
        .expect("help output should have a Commands section followed by Options");

    // Per-template subcommands are `hide = true`; users are funneled
    // through `new list`.
    assert!(
        commands_block.contains("list"),
        "commands:\n{commands_block}"
    );
    for short in [
        "process_writes",
        "process_scheduled_call",
        "process_request",
        "registry",
    ] {
        assert!(
            !commands_block.contains(short),
            "`{short}` should not appear in the Commands section of `new --help`:\n{commands_block}"
        );
    }
}

#[test]
fn new_process_writes_help_shows_template_flags_only() {
    let assert = cli_cmd()
        .arg("new")
        .arg("process_writes")
        .arg("-h")
        .assert()
        .success();
    let stdout = std::str::from_utf8(&assert.get_output().stdout).unwrap();

    assert!(
        stdout.contains("Plugin triggered by rows written to a database"),
        "stdout: {stdout}"
    );
    for needle in ["--output", "--force", "--name", "--database-version"] {
        assert!(stdout.contains(needle), "missing `{needle}`:\n{stdout}");
    }
    // Registry-only flags do not appear.
    assert!(
        !stdout.contains("--artifacts-url"),
        "registry flag leaked into plugin help:\n{stdout}"
    );
}

#[test]
fn new_process_writes_with_force_overwrites_existing_write_set() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("hp");
    std::fs::create_dir_all(&target).unwrap();
    // Pre-write every file in the template's write set; --force must
    // replace all of them, matching the SDK-inline coverage.
    std::fs::write(target.join("manifest.toml"), "pre-existing").unwrap();
    std::fs::write(target.join("__init__.py"), "pre-existing").unwrap();
    std::fs::write(target.join("README.md"), "pre-existing").unwrap();
    std::fs::write(target.join("notes.txt"), "keep me").unwrap();

    spawn_new(&target, &["process_writes", "--force"]).success();

    let manifest = std::fs::read_to_string(target.join("manifest.toml")).unwrap();
    assert!(manifest.contains("name = \"hp\""), "manifest: {manifest}");
    let init = std::fs::read_to_string(target.join("__init__.py")).unwrap();
    assert!(init.contains("def process_writes("), "init: {init}");
    let readme = std::fs::read_to_string(target.join("README.md")).unwrap();
    assert!(!readme.contains("pre-existing"), "readme: {readme}");
    // Unrelated file untouched.
    assert_eq!(
        std::fs::read_to_string(target.join("notes.txt")).unwrap(),
        "keep me"
    );
}

#[test]
fn new_process_writes_succeeds_when_only_unrelated_files_exist() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("hp");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("notes.txt"), "keep me").unwrap();

    // No `--force`; the unrelated file is not in the template's write
    // set, so the scaffold must succeed and leave it alone.
    spawn_new(&target, &["process_writes"]).success();

    assert!(target.join("manifest.toml").exists());
    assert_eq!(
        std::fs::read_to_string(target.join("notes.txt")).unwrap(),
        "keep me"
    );
}

#[test]
fn new_process_writes_without_force_fails_on_conflict() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("hp");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("manifest.toml"), "pre-existing").unwrap();

    spawn_new(&target, &["process_writes"]).code(1);

    // Content preserved — no partial scaffold.
    assert_eq!(
        std::fs::read_to_string(target.join("manifest.toml")).unwrap(),
        "pre-existing"
    );
    assert!(!target.join("__init__.py").exists());
    assert!(!target.join("README.md").exists());
}

#[test]
fn new_registry_with_force_overwrites_index() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("r");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("index.json"), "{}").unwrap();

    spawn_new(
        &target,
        &[
            "registry",
            "--force",
            "--artifacts-url",
            "https://x.example/",
        ],
    )
    .success();

    let raw = std::fs::read_to_string(target.join("index.json")).unwrap();
    assert!(raw.contains("https://x.example/"), "index: {raw}");
}

// -----------------------------------------------------------------------
// PluginName rule coverage — accept + reject paths under the new rule
// (`[a-zA-Z][a-zA-Z0-9_-]*`, case-preserving, rejects Windows reserved
// device names).
// -----------------------------------------------------------------------

#[test]
fn new_accepts_underscore_name_via_flag() {
    let td = tempfile::tempdir().unwrap();
    spawn_new(td.path(), &["process_writes", "--name", "my_plugin"]).success();
    let manifest = std::fs::read_to_string(td.path().join("manifest.toml")).unwrap();
    assert!(
        manifest.contains("name = \"my_plugin\""),
        "manifest should contain exact name, got: {manifest}"
    );
}

#[test]
fn new_accepts_mixed_case_name_via_flag() {
    let td = tempfile::tempdir().unwrap();
    spawn_new(td.path(), &["process_writes", "--name", "MyPlugin"]).success();
    let manifest = std::fs::read_to_string(td.path().join("manifest.toml")).unwrap();
    assert!(
        manifest.contains("name = \"MyPlugin\""),
        "manifest should preserve case, got: {manifest}"
    );
}

#[test]
fn new_accepts_mixed_case_basename() {
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("MyPlugin");
    spawn_new(&dir, &["process_writes"]).success();
    let manifest = std::fs::read_to_string(dir.join("manifest.toml")).unwrap();
    assert!(manifest.contains("name = \"MyPlugin\""));
}

/// Regression narrowing: `7plugin` (digit-leading) was valid under the
/// previous rule (which allowed a digit in the leading position). The
/// new Cargo rule requires a leading letter — verify the explicit
/// `--name` path rejects it with the new regex in the error message.
#[test]
fn new_rejects_digit_leading_name_regression() {
    let td = tempfile::tempdir().unwrap();
    // Anchor on the English rule rendering ("starting with a letter")
    // rather than the bracketed regex literal — durable against future
    // reformats of the error copy.
    spawn_new(td.path(), &["process_writes", "--name", "7plugin"])
        .code(2)
        .stderr(predicates::str::contains("starting with a letter"));
}

#[test]
fn new_rejects_reserved_device_name() {
    let td = tempfile::tempdir().unwrap();
    spawn_new(td.path(), &["process_writes", "--name", "con"])
        .code(2)
        .stderr(predicates::str::contains("Windows reserved"));
}

#[test]
fn new_rejects_reserved_device_name_case_insensitive() {
    let td = tempfile::tempdir().unwrap();
    spawn_new(td.path(), &["process_writes", "--name", "CON"])
        .code(2)
        .stderr(predicates::str::contains("Windows reserved"));
}

/// Basename-derived invalid name surfaces as a runtime failure (exit 1,
/// anyhow error — not a clap parse error), and the message instructs
/// `--name` as the remediation.
#[test]
fn new_rejects_invalid_basename_with_actionable_message() {
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("7plugin");
    spawn_new(&dir, &["process_writes"])
        .code(1)
        .stderr(predicates::str::contains("pass --name"));
}

#[test]
fn new_rejects_reserved_basename_with_actionable_message() {
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("con");
    spawn_new(&dir, &["process_writes"])
        .code(1)
        .stderr(predicates::str::contains("pass --name"));
}

#[test]
fn new_registry_help_shows_only_its_flags() {
    let assert = cli_cmd()
        .arg("new")
        .arg("registry")
        .arg("-h")
        .assert()
        .success();
    let stdout = std::str::from_utf8(&assert.get_output().stdout).unwrap();
    assert!(
        stdout.contains("Empty plugin registry directory"),
        "stdout: {stdout}"
    );
    for needle in ["--output", "--force", "--artifacts-url"] {
        assert!(stdout.contains(needle), "missing `{needle}`:\n{stdout}");
    }
    for absent in ["--name", "--database-version"] {
        assert!(
            !stdout.contains(absent),
            "plugin flag leaked into registry help:\n{stdout}"
        );
    }
}

/// `--force` only makes sense for commands that write files.
/// `new list` writes nothing, so the flag is deliberately not
/// declared on its `Args` — clap rejects it at parse time with
/// exit 2, the same contract as `--name` on the registry template.
#[test]
fn new_list_rejects_force_flag() {
    cli_cmd()
        .args(["new", "list", "--force"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicates::str::contains("--force"));
}

/// Discovery regression guard: a first-time user running `new --help`
/// must see both the scaffold form (`new <TEMPLATE> [PATH]`) and the
/// discovery form (`new list`) in the Usage block. Hiding per-template
/// subcommands from `Commands:` is correct, but the Usage line must
/// still teach the creation path or the help is a dead end.
#[test]
fn new_help_teaches_scaffold_and_list_forms() {
    let assert = cli_cmd().arg("new").arg("--help").assert().success();
    let stdout = std::str::from_utf8(&assert.get_output().stdout).unwrap();

    // Isolate clap's Usage block — from "Usage:" to the first blank line.
    let usage_block = stdout
        .split_once("Usage:")
        .and_then(|(_, after)| after.split_once("\n\n"))
        .map(|(block, _)| block)
        .expect("help output should have a Usage block ending in a blank line");

    // Scaffold form — uppercase `<TEMPLATE>` is clap's default rendering
    // for a positional with a `TEMPLATE` value name; accept either
    // casing so the test doesn't over-pin clap's placeholder styling.
    assert!(
        usage_block.contains("<TEMPLATE>") || usage_block.contains("<template>"),
        "Usage block should expose the scaffold form (`new <TEMPLATE> [PATH]`):\n{usage_block}"
    );
    // Discovery form — the literal `new list` must appear so users know
    // to run it before picking a template short-name.
    assert!(
        usage_block.contains("new list"),
        "Usage block should show the list-subcommand form:\n{usage_block}"
    );
}
