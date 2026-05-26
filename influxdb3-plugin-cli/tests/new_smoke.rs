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

/// Spawns `influxdb3-plugin new <args>` with no positional target from
/// the given working directory, exercising `[path]`'s default-to-`.`
/// behavior where `--name` derives from the cwd basename.
fn spawn_new_in<P: AsRef<Path>>(cwd: P, extra_args: &[&str]) -> assert_cmd::assert::Assert {
    let mut cmd = cli_cmd();
    cmd.current_dir(cwd.as_ref());
    cmd.arg("new");
    for a in extra_args {
        cmd.arg(a);
    }
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
    assert_eq!(
        payload.get("status").and_then(|v| v.as_str()),
        Some("ok"),
        "envelope status must be \"ok\"; got:\n{stdout}"
    );
    // Strip the absolute path inside `result` before snapshotting so the
    // snapshot is machine-independent.
    payload["result"]
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
    assert_eq!(
        payload.get("status").and_then(|v| v.as_str()),
        Some("ok"),
        "envelope status must be \"ok\"; got:\n{stdout}"
    );
    let placeholder = format!("<TMPDIR>/{target}");
    payload["result"]
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
fn new_index_json_snapshot() {
    snapshot_new_template("index", "reg", "new_index_json");
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
fn new_index_happy_path_writes_file_url() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("reg");

    spawn_new(&target, &["index"]).success();

    let index = std::fs::read_to_string(target.join("index.json")).unwrap();
    assert!(
        index.contains("\"artifacts_url\": \"file://"),
        "index should default artifacts_url to file://, got:\n{index}"
    );
}

/// Default `artifacts_url` on `new index` reflects the path the user
/// typed. On macOS the OS-level `/tmp` → `/private/tmp` symlink used to
/// leak into the index; `std::path::absolute` prevents that.
#[test]
fn new_index_default_artifacts_url_preserves_typed_path() {
    let td = tempfile::tempdir().unwrap();
    let real = td.path().join("real");
    std::fs::create_dir_all(&real).unwrap();
    let link = td.path().join("link");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real, &link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&real, &link).unwrap();
    let target = link.join("reg");

    spawn_new(&target, &["index"]).success();

    let index = std::fs::read_to_string(target.join("index.json")).unwrap();
    // The URL must reference the symlink path, not its target.
    let link_str = link.to_str().unwrap();
    assert!(
        index.contains(link_str),
        "index artifacts_url should preserve typed (symlink) \
         path {link_str:?}, got:\n{index}"
    );
}

/// Explicit `--artifacts-url` is written through verbatim (https / http
/// inclusive).
#[test]
fn new_index_with_explicit_artifacts_url() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("reg");

    spawn_new(
        &target,
        &[
            "index",
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

/// Invalid path basename -> exit 1 with error mentioning `--name`.
/// (clap's exit-2 path applies only to argument-parse failures; an
/// invalid basename is a runtime-validation failure surfaced by the
/// command body.)
/// Under piped stdout (assert_cmd default), errors render as JSON
/// envelopes on stdout.
#[test]
fn new_rejects_invalid_basename_without_name_override() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("1bad");

    let assert = spawn_new(&target, &["process_writes"]).failure().code(1);

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(
        stdout.contains("--name"),
        "output should hint at --name, got: {stdout}"
    );
    assert!(!target.join("manifest.toml").exists());
}

/// Explicit `--name <bad>` also rejected, with a different message.
/// Under piped stdout, errors render as JSON envelopes on stdout.
#[test]
fn new_rejects_invalid_explicit_name() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("ok");

    let assert = spawn_new(&target, &["process_writes", "--name", "1bad"])
        .failure()
        .code(2);

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(
        stdout.contains("1bad"),
        "output should name the bad value, got: {stdout}"
    );
    assert!(!target.join("manifest.toml").exists());
}

/// Plugin-template flags rejected with the index template, and vice
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
    .stdout(predicates::str::contains("--artifacts-url"));
}

#[test]
fn new_rejects_name_on_index_template() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("r");

    spawn_new(&target, &["index", "--name", "x"])
        .failure()
        .code(2)
        .stdout(predicates::str::contains("--name"));
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
        .stdout(predicates::str::contains("garbage_template"));
}

/// Data-tool failure path in JSON mode: error envelope on stdout,
/// stderr empty.
#[test]
fn new_failure_in_json_mode_emits_error_envelope() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("p");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("manifest.toml"), "pre-existing").unwrap();

    let assert = spawn_new(&target, &["process_writes", "--output", "json"])
        .failure()
        .code(1);

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
        "conflict error should be a runtime failure (exit 1); got {:?}",
        output.status.code()
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    let occurrences = stderr.matches(dir.to_str().unwrap()).count();
    assert_eq!(
        occurrences, 1,
        "stderr should mention the conflicting path exactly once; was:\n{stderr}"
    );

    // The error chain should not duplicate "already exists".
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
        "index",
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
    assert_eq!(
        payload.get("status").and_then(|v| v.as_str()),
        Some("ok"),
        "envelope status must be \"ok\"; got:\n{stdout}"
    );
    insta::assert_json_snapshot!("new_list_json", payload);
}

#[test]
fn new_help_does_not_enumerate_templates() {
    let assert = cli_cmd().arg("new").arg("--help").assert().success();
    let stdout = std::str::from_utf8(&assert.get_output().stdout).unwrap();

    // Scope the assertion to the `Commands:` block: `index` also
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
        "index",
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
        "index flag leaked into plugin help:\n{stdout}"
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
fn new_index_with_force_overwrites_index() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("r");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("index.json"), "{}").unwrap();

    spawn_new(
        &target,
        &["index", "--force", "--artifacts-url", "https://x.example/"],
    )
    .success();

    let raw = std::fs::read_to_string(target.join("index.json")).unwrap();
    assert!(raw.contains("https://x.example/"), "index: {raw}");
}

// PluginName rule coverage — accept + reject paths under the new rule
// (`[a-zA-Z][a-zA-Z0-9_-]*`, case-preserving, rejects Windows reserved
// device names).

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
        .stdout(predicates::str::contains("starting with a letter"));
}

#[test]
fn new_rejects_reserved_device_name() {
    let td = tempfile::tempdir().unwrap();
    spawn_new(td.path(), &["process_writes", "--name", "con"])
        .code(2)
        .stdout(predicates::str::contains("Windows reserved"));
}

#[test]
fn new_rejects_reserved_device_name_case_insensitive() {
    let td = tempfile::tempdir().unwrap();
    spawn_new(td.path(), &["process_writes", "--name", "CON"])
        .code(2)
        .stdout(predicates::str::contains("Windows reserved"));
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
        .stdout(predicates::str::contains("pass --name"));
}

#[test]
fn new_rejects_reserved_basename_with_actionable_message() {
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("con");
    spawn_new(&dir, &["process_writes"])
        .code(1)
        .stdout(predicates::str::contains("pass --name"));
}

#[test]
fn new_index_help_shows_only_its_flags() {
    let assert = cli_cmd()
        .arg("new")
        .arg("index")
        .arg("-h")
        .assert()
        .success();
    let stdout = std::str::from_utf8(&assert.get_output().stdout).unwrap();
    assert!(
        stdout.contains("Empty registry index file"),
        "stdout: {stdout}"
    );
    for needle in ["--output", "--force", "--artifacts-url"] {
        assert!(stdout.contains(needle), "missing `{needle}`:\n{stdout}");
    }
    for absent in ["--name", "--database-version"] {
        assert!(
            !stdout.contains(absent),
            "plugin flag leaked into index help:\n{stdout}"
        );
    }
}

/// `--force` only makes sense for commands that write files.
/// `new list` writes nothing, so the flag is deliberately not
/// declared on its `Args` — clap rejects it at parse time with
/// exit 2, the same contract as `--name` on the index template.
#[test]
fn new_list_rejects_force_flag() {
    cli_cmd()
        .args(["new", "list", "--force"])
        .assert()
        .failure()
        .code(2)
        .stdout(predicates::str::contains("--force"));
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

#[test]
fn new_plugin_omitted_path_uses_cwd_basename() {
    for template in [
        "process_writes",
        "process_scheduled_call",
        "process_request",
    ] {
        let td = tempfile::tempdir().unwrap();
        let working = td.path().join("downsampler-plugin");
        std::fs::create_dir_all(&working).unwrap();

        spawn_new_in(&working, &[template, "--output", "json"]).success();

        let manifest = std::fs::read_to_string(working.join("manifest.toml")).unwrap();
        assert!(
            manifest.contains("name = \"downsampler-plugin\""),
            "[{template}] manifest should derive name from cwd basename; got: {manifest}"
        );
        assert!(working.join("__init__.py").exists(), "[{template}]");
        assert!(working.join("README.md").exists(), "[{template}]");
    }
}

#[test]
fn new_plugin_literal_dot_path_matches_omitted_behavior() {
    let td = tempfile::tempdir().unwrap();
    let working = td.path().join("my-plugin");
    std::fs::create_dir_all(&working).unwrap();

    spawn_new_in(&working, &["process_writes", "."]).success();

    let manifest = std::fs::read_to_string(working.join("manifest.toml")).unwrap();
    assert!(
        manifest.contains("name = \"my-plugin\""),
        "literal `.` should behave identically to omitted path; got: {manifest}"
    );
}

#[test]
fn new_plugin_omitted_path_with_invalid_cwd_basename_errors_helpfully() {
    let td = tempfile::tempdir().unwrap();
    // `1bad` fails PluginName::from_str (digit-leading); matches the
    // invalid-name sentinel used elsewhere after the Cargo-rule alignment.
    let working = td.path().join("1bad");
    std::fs::create_dir_all(&working).unwrap();

    let assert = spawn_new_in(&working, &["process_writes"])
        .failure()
        .code(1);

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(
        stdout.contains("--name"),
        "output should hint at --name when cwd basename is invalid; got: {stdout}"
    );
    assert!(!working.join("manifest.toml").exists());
}

/// Isolates "explicit --name wins over cwd basename" — cwd basename here
/// is itself valid (`foo-cwd`), so the only thing being proven is that
/// --name takes precedence. Keeps this test distinct from the
/// invalid-cwd-basename case above.
#[test]
fn new_plugin_omitted_path_with_explicit_name_succeeds() {
    let td = tempfile::tempdir().unwrap();
    let working = td.path().join("foo-cwd");
    std::fs::create_dir_all(&working).unwrap();

    spawn_new_in(&working, &["process_writes", "--name", "good-name"]).success();

    let manifest = std::fs::read_to_string(working.join("manifest.toml")).unwrap();
    assert!(
        manifest.contains("name = \"good-name\""),
        "explicit --name should override cwd basename; got: {manifest}"
    );
}

#[test]
fn new_index_omitted_path_writes_index_in_cwd() {
    let td = tempfile::tempdir().unwrap();
    let working = td.path().join("reg");
    std::fs::create_dir_all(&working).unwrap();

    spawn_new_in(&working, &["index"]).success();

    assert!(working.join("index.json").exists());
}

/// Explicit `--database-version` must parse as a SemVer range. Invalid
/// ranges are usage errors (exit 2); no manifest is written.
#[test]
fn new_plugin_rejects_invalid_database_version() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("p");

    let assert = spawn_new(
        &target,
        &["process_writes", "--database-version", "not-a-range"],
    )
    .failure()
    .code(2);

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(
        stdout.contains("not-a-range"),
        "output should surface the rejected value verbatim; got: {stdout}"
    );
    assert!(!target.join("manifest.toml").exists());
    assert!(!target.join("__init__.py").exists());
    assert!(!target.join("README.md").exists());
}

/// Unsupported schemes and malformed URLs must be rejected at the CLI as
/// usage errors (exit 2), not written into an index that downstream
/// consumers (which accept only https/http/file) would reject.
#[test]
fn new_index_rejects_unsupported_artifacts_url_scheme() {
    for bad in [
        "ftp://example.com/artifacts",
        "s3://bucket/plugins",
        "not-a-url",
    ] {
        let td = tempfile::tempdir().unwrap();
        let target = td.path().join("reg");

        let assert = spawn_new(&target, &["index", "--artifacts-url", bad])
            .failure()
            .code(2);

        let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
        assert!(
            stdout.contains(bad),
            "output should surface the rejected value verbatim for {bad:?}; got: {stdout}"
        );
        assert!(
            !target.join("index.json").exists(),
            "no index.json should be written for rejected value {bad:?}"
        );
    }
}

/// Sibling canonical collision at new time: `my_plugin/` already exists,
/// `new process_writes .../my-plugin` is rejected (usage-error path,
/// exit 2 via CliError::usage).
#[test]
fn new_rejects_sibling_canonical_collision_hyphen_underscore() {
    let td = tempfile::tempdir().unwrap();
    std::fs::create_dir(td.path().join("my_plugin")).unwrap();
    let target = td.path().join("my-plugin");

    let assert = spawn_new(&target, &["process_writes"]).failure().code(2);

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(
        stdout.contains("my-plugin"),
        "output should name requested: {stdout}"
    );
    assert!(
        stdout.contains("my_plugin"),
        "output should name existing sibling: {stdout}"
    );
    assert!(
        !target.exists(),
        "no files/dirs written under target on rejection"
    );
}

/// Exercises the lowercase branch of canonicalization. `My-Plugin` and
/// `my_plugin` both canonicalize to `my_plugin`; both directory spellings
/// coexist on case-insensitive filesystems (APFS default on macOS).
#[test]
fn new_rejects_sibling_canonical_collision_case_and_separator() {
    let td = tempfile::tempdir().unwrap();
    std::fs::create_dir(td.path().join("My-Plugin")).unwrap();
    let target = td.path().join("my_plugin");

    let assert = spawn_new(&target, &["process_writes"]).failure().code(2);
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(stdout.contains("my_plugin"), "output: {stdout}");
    assert!(stdout.contains("My-Plugin"), "output: {stdout}");
    assert!(!target.exists());
}

#[test]
fn new_accepts_non_colliding_sibling() {
    let td = tempfile::tempdir().unwrap();
    std::fs::create_dir(td.path().join("downsampler")).unwrap();
    let target = td.path().join("my-plugin");

    spawn_new(&target, &["process_writes"]).success();

    assert!(target.join("manifest.toml").exists());
}

/// Sibling *directories* whose basenames fail `PluginName::from_str` are
/// skipped by the canonical scan. `.hidden` and `123-leading-digit` exercise
/// the two common rejection branches (dot-leading and digit-leading) of the
/// name rule. Non-directory siblings are separately filtered by `is_dir()`;
/// a later test can extend this fixture if that branch needs its own anchor.
#[test]
fn new_accepts_when_sibling_has_invalid_plugin_basename() {
    let td = tempfile::tempdir().unwrap();
    std::fs::create_dir(td.path().join(".hidden")).unwrap();
    std::fs::create_dir(td.path().join("123-leading-digit")).unwrap();
    let target = td.path().join("my-plugin");

    spawn_new(&target, &["process_writes"]).success();
    assert!(target.join("manifest.toml").exists());
}

/// The sibling scan uses the resolved plugin name (--name if supplied),
/// not the target basename.
#[test]
fn new_sibling_check_uses_resolved_name_not_basename() {
    let td = tempfile::tempdir().unwrap();
    std::fs::create_dir(td.path().join("my_plugin")).unwrap();
    let target = td.path().join("unrelated-dir");

    let assert = spawn_new(&target, &["process_writes", "--name", "my-plugin"])
        .failure()
        .code(2);

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(
        stdout.contains("my-plugin"),
        "output cites resolved name: {stdout}"
    );
    assert!(
        !stdout.contains("unrelated-dir"),
        "output must not cite the target basename: {stdout}"
    );
    assert!(!target.exists());
}

/// Paired asymmetry with `new_force_allows_overwrite_of_same_spelling_target`:
/// `--force` bypasses `check_no_existing` (same-spelling target) but NOT the
/// sibling canonical check. Rewriting either test silently rots the contract;
/// keep both in lockstep.
#[test]
fn new_force_does_not_bypass_sibling_canonical_collision() {
    let td = tempfile::tempdir().unwrap();
    std::fs::create_dir(td.path().join("my_plugin")).unwrap();
    let target = td.path().join("my-plugin");

    let assert = spawn_new(&target, &["process_writes", "--force"])
        .failure()
        .code(2);
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(
        stdout.contains("canonically collides"),
        "must reject with the canonical-collision error, not some other exit-2 path: {stdout}"
    );
    assert!(!target.exists(), "--force must not bypass canonical check");
}

/// Paired asymmetry with `new_force_does_not_bypass_sibling_canonical_collision`:
/// `--force` bypasses `check_no_existing` for same-spelling targets (the target
/// path itself is excluded from the sibling scan) but does NOT bypass the
/// sibling canonical check. Keep both in lockstep.
#[test]
fn new_force_allows_overwrite_of_same_spelling_target() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("my-plugin");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("manifest.toml"), "pre-existing").unwrap();

    spawn_new(&target, &["process_writes", "--force"]).success();

    let manifest = std::fs::read_to_string(target.join("manifest.toml")).unwrap();
    assert!(
        !manifest.contains("pre-existing"),
        "template should overwrite"
    );
    assert!(manifest.contains("name = \"my-plugin\""));
}

#[test]
fn new_succeeds_when_parent_dir_does_not_yet_exist() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("fresh-repo").join("my-plugin");

    spawn_new(&target, &["process_writes"]).success();

    assert!(target.join("manifest.toml").exists());
}

/// Unknown-flag errors on `new <template>` render the same `[OPTIONS] [PATH]`
/// usage shape as `--help`. Clap's default error rendering strips the
/// `[OPTIONS]` marker and the brackets around `[PATH]` whenever the
/// unknown flag appears *after* a consumed positional — the exact layout
/// users type (e.g. `new index /tmp/reg --name foo`). A per-template
/// `override_usage` forces help and error paths to agree.
#[test]
fn new_template_unknown_flag_usage_line_matches_help() {
    let td = tempfile::tempdir().unwrap();
    let target = td.path().join("x");
    let target_str = target.to_str().unwrap();

    for template in [
        "index",
        "process_writes",
        "process_scheduled_call",
        "process_request",
    ] {
        let mut cmd = cli_cmd();
        let assertion = cmd
            .arg("new")
            .arg(template)
            .arg(target_str)
            .arg("--totally-bogus-flag")
            .assert()
            .failure()
            .code(2);
        // Under piped stdout (assert_cmd default), clap errors are rendered
        // as JSON envelopes on stdout. The usage line is embedded in the
        // envelope's message field.
        let stdout = String::from_utf8_lossy(&assertion.get_output().stdout).to_string();
        let expected = format!("Usage: influxdb3-plugin new {template} [OPTIONS] [PATH]");
        assert!(
            stdout.contains(&expected),
            "template {template} parse-error output should contain {expected:?}, \
             got:\n{stdout}"
        );
        assert!(
            !stdout.contains(&format!("new {template} <PATH>")),
            "template {template} parse-error output still renders `<PATH>`, got:\n{stdout}"
        );
    }
}

/// Human-mode success output shortens the scaffold's target directory
/// to CWD-relative form when the target lives under the working
/// directory. Avoids leaking absolute machine paths in terminals,
/// demos, and CI logs.
#[test]
fn new_human_mode_emits_cwd_relative_paths() {
    let td = tempfile::tempdir().unwrap();
    let cwd = std::fs::canonicalize(td.path()).unwrap();
    let target = cwd.join("hp");

    let mut cmd = cli_cmd();
    cmd.current_dir(&cwd)
        .arg("new")
        .arg("process_writes")
        .arg(&target)
        .arg("--output")
        .arg("human");
    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();

    assert!(
        stdout.contains("Scaffolded plugin (process_writes template) at hp"),
        "human output should print relative target dir, got:\n{stdout}"
    );
    let cwd_str = cwd.display().to_string();
    assert!(
        !stdout.contains(&cwd_str),
        "human output must not leak the absolute CWD prefix {cwd_str:?}; got:\n{stdout}"
    );
}

/// JSON-mode payload keeps the absolute target directory so
/// programmatic consumers get unambiguous filesystem targets
/// regardless of caller CWD.
#[test]
fn new_json_mode_keeps_absolute_paths() {
    let td = tempfile::tempdir().unwrap();
    let cwd = std::fs::canonicalize(td.path()).unwrap();
    let target = cwd.join("hp");

    let mut cmd = cli_cmd();
    cmd.current_dir(&cwd)
        .arg("new")
        .arg("process_writes")
        .arg(&target)
        .arg("--output")
        .arg("json");
    let assert = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let doc: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must be valid JSON: {e}\n{stdout}"));
    let target_dir = doc
        .pointer("/result/target_dir")
        .and_then(|v| v.as_str())
        .expect("result.target_dir missing");
    assert!(
        Path::new(target_dir).is_absolute(),
        "json target_dir must be absolute, got {target_dir:?}"
    );
}
