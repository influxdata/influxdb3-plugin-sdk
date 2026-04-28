//! Integration tests for `influxdb3-plugin validate`.
//!
//! Covers the validator-idiom contract (single JSON document on stdout
//! on BOTH pass and fail), the cross-file failure categories, the
//! `--index` uniqueness check, multi-error collection, and the
//! exit-code mapping.
//!
//! Fixtures are synthesized inline into per-test `tempfile::TempDir`s so
//! the suite is self-contained.
//!
//! See `version_smoke.rs` for the rationale behind the crate-root allow.

#![allow(unused_crate_dependencies)]

use std::path::Path;

mod common;
use common::{SEEDED_INDEX, VALID_INIT, VALID_MANIFEST, cli_cmd, write_valid_plugin};

fn spawn_validate<P: AsRef<Path>>(target: P, extra: &[&str]) -> assert_cmd::assert::Assert {
    let mut cmd = cli_cmd();
    cmd.arg("validate");
    for a in extra {
        cmd.arg(a);
    }
    cmd.arg(target.as_ref());
    cmd.assert()
}

#[test]
fn validate_happy_path_emits_empty_diagnostics_array() {
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("p");
    write_valid_plugin(&dir);

    let assert = spawn_validate(&dir, &["--output", "json"]).success();

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let payload: serde_json::Value =
        serde_json::from_str(&stdout).expect("validator stdout is JSON");
    assert_eq!(
        payload,
        serde_json::json!({ "diagnostics": [] }),
        "happy path must emit empty diagnostics array"
    );
    insta::assert_json_snapshot!("validate_happy_path_json", payload);
}

/// Empty plugin directory: BOTH `manifest.toml` and `__init__.py` are
/// missing. Spec says all validation errors are collected, so both
/// `MissingRequiredFile` diagnostics must surface in one run.
#[test]
fn validate_empty_plugin_dir_reports_both_missing_files_in_json() {
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("empty");
    std::fs::create_dir_all(&dir).unwrap();

    let assert = spawn_validate(&dir, &["--output", "json"])
        .failure()
        .code(1);

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let payload: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let diags = payload["diagnostics"].as_array().expect("array");
    assert_eq!(diags.len(), 2, "expected two diagnostics, got {payload}");
    let mut variants_and_fields: Vec<(&str, &str)> = diags
        .iter()
        .map(|d| (d["variant"].as_str().unwrap(), d["field"].as_str().unwrap()))
        .collect();
    variants_and_fields.sort();
    assert_eq!(
        variants_and_fields,
        vec![
            ("MissingRequiredFile", "__init__.py"),
            ("MissingRequiredFile", "manifest.toml"),
        ]
    );
    assert!(
        assert.get_output().stderr.is_empty(),
        "stderr must be empty in JSON mode"
    );
}

/// Validator idiom: failure path emits a single JSON document on STDOUT
/// (not stderr), and exits 1.
#[test]
fn validate_failure_emits_diagnostics_on_stdout_and_exits_one() {
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("p");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("manifest.toml"), VALID_MANIFEST).unwrap();
    // No __init__.py — should surface MissingRequiredFile.

    let assert = spawn_validate(&dir, &["--output", "json"])
        .failure()
        .code(1);

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let payload: serde_json::Value =
        serde_json::from_str(&stdout).expect("validator stdout is JSON even on failure");
    let diags = payload["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0]["variant"], "MissingRequiredFile");
    assert_eq!(diags[0]["field"], "__init__.py");
    insta::assert_json_snapshot!("validate_missing_init_json", payload);
}

#[test]
fn validate_collects_multiple_diagnostics_in_one_pass() {
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("p");
    std::fs::create_dir_all(&dir).unwrap();
    // Manifest with 3 distinct field-level defects: bad name, bad
    // version, bad URL scheme.
    let bad_manifest = r#"manifest_schema_version = "1.0"

[plugin]
name = "1bad"
version = "1.2"
description = "x"
triggers = ["process_writes"]
homepage = "ftp://bad"

[dependencies]
database_version = ">=3.0.0"
"#;
    std::fs::write(dir.join("manifest.toml"), bad_manifest).unwrap();
    std::fs::write(dir.join("__init__.py"), VALID_INIT).unwrap();

    let assert = spawn_validate(&dir, &["--output", "json"])
        .failure()
        .code(1);

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let payload: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let diags = payload["diagnostics"].as_array().unwrap();
    assert_eq!(
        diags.len(),
        3,
        "expected 3 diagnostics, got {}: {payload}",
        diags.len()
    );
    let variants: Vec<&str> = diags
        .iter()
        .map(|d| d["variant"].as_str().unwrap())
        .collect();
    assert!(
        variants.iter().all(|v| *v == "SchemaReported"),
        "all defects should surface as SchemaReported, got {variants:?}"
    );
    let fields: Vec<&str> = diags.iter().map(|d| d["field"].as_str().unwrap()).collect();
    assert!(
        fields.contains(&"plugin.name"),
        "missing plugin.name: {fields:?}"
    );
    assert!(
        fields.contains(&"plugin.version"),
        "missing plugin.version: {fields:?}"
    );
    assert!(
        fields.contains(&"plugin.homepage"),
        "missing plugin.homepage: {fields:?}"
    );
    insta::assert_json_snapshot!("validate_multi_defect_json", payload);
}

#[test]
fn validate_async_trigger_diagnostic_points_at_init() {
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("p");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("manifest.toml"), VALID_MANIFEST).unwrap();
    std::fs::write(
        dir.join("__init__.py"),
        "async def process_writes(a, b, c):\n    pass\n",
    )
    .unwrap();

    let assert = spawn_validate(&dir, &["--output", "json"])
        .failure()
        .code(1);
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let payload: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let diags = payload["diagnostics"].as_array().unwrap();
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0]["variant"], "AsyncTriggerFn");
    assert_eq!(diags[0]["field"], "__init__.py");
    insta::assert_json_snapshot!("validate_async_trigger_json", payload);
}

/// `validate --index <path>` runs the same checks plus a uniqueness
/// check against the supplied index. A `(name, version)` collision
/// surfaces as a `NameVersionConflict` diagnostic, NOT a runtime error
/// — same diagnostics array as other validation failures.
#[test]
fn validate_with_index_surfaces_uniqueness_collision() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    write_valid_plugin(&plugin_dir);

    let index = serde_json::json!({
        "index_schema_version": "1.0",
        "artifacts_url": "https://plugins.example.com/artifacts",
        "plugins": [{
            "name": "downsampler",
            "version": "1.2.0",
            "description": "preexisting",
            "triggers": ["process_writes"],
            "dependencies": { "database_version": ">=3.0.0", "python": [] },
            "hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
        }]
    });
    let index_path = td.path().join("index.json");
    std::fs::write(&index_path, serde_json::to_string_pretty(&index).unwrap()).unwrap();

    let assert = spawn_validate(
        &plugin_dir,
        &["--output", "json", "--index", index_path.to_str().unwrap()],
    )
    .failure()
    .code(1);

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let payload: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let diags = payload["diagnostics"].as_array().unwrap();
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0]["variant"], "NameVersionConflict");
    assert_eq!(diags[0]["field"], "downsampler@1.2.0");
    insta::assert_json_snapshot!("validate_name_version_conflict_json", payload);
}

/// Without `--index`, uniqueness is not checked even if a collision
/// would exist on disk.
#[test]
fn validate_without_index_flag_passes() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    write_valid_plugin(&plugin_dir);

    spawn_validate(&plugin_dir, &["--output", "json"]).success();
}

/// Proves `validate` does NOT auto-discover any index file from conventional
/// paths. We plant an index at the plugin-dir's parent that WOULD collide on
/// `(name, version)` if read; validation without `--index` must still succeed
/// because no implicit discovery occurs.
#[test]
fn validate_does_not_auto_discover_adjacent_index() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    write_valid_plugin(&plugin_dir);

    // SEEDED_INDEX carries a `(downsampler, 1.2.0)` entry that WOULD collide
    // with the plugin's (name, version) if validate auto-discovered it.
    std::fs::write(td.path().join("index.json"), SEEDED_INDEX).unwrap();
    std::fs::write(plugin_dir.join("index.json"), SEEDED_INDEX).unwrap();

    // Run validate without `--index`. Must succeed — no auto-discovery means
    // the planted indexes are invisible.
    spawn_validate(&plugin_dir, &["--output", "json"]).success();
}

/// `validate --index` must compare canonical name forms (lowercase,
/// hyphens replaced with underscores). `foo-bar` and `foo_bar` collide.
#[test]
fn validate_with_index_detects_hyphen_underscore_collision() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    let manifest = r#"manifest_schema_version = "1.0"

[plugin]
name = "foo-bar"
version = "0.1.0"
description = "x"
triggers = ["process_writes"]

[dependencies]
database_version = ">=3.0.0"
"#;
    std::fs::write(plugin_dir.join("manifest.toml"), manifest).unwrap();
    std::fs::write(plugin_dir.join("__init__.py"), VALID_INIT).unwrap();
    let index = serde_json::json!({
        "index_schema_version": "1.0",
        "artifacts_url": "https://x.example/a",
        "plugins": [{
            "name": "foo_bar",
            "version": "0.1.0",
            "description": "seed",
            "triggers": ["process_writes"],
            "dependencies": { "database_version": ">=3.0.0", "python": [] },
            "hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
        }]
    });
    let index_path = td.path().join("index.json");
    std::fs::write(&index_path, serde_json::to_string_pretty(&index).unwrap()).unwrap();

    let assert = spawn_validate(
        &plugin_dir,
        &["--output", "json", "--index", index_path.to_str().unwrap()],
    )
    .failure()
    .code(1);
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let payload: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let diags = payload["diagnostics"].as_array().unwrap();
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0]["variant"], "NameVersionConflict");
    let field = diags[0]["field"].as_str().unwrap();
    assert!(
        field.ends_with("@0.1.0"),
        "field should pin version: {field}"
    );
    assert!(assert.get_output().stderr.is_empty());
}

/// Sister case: case-only collision. `Foo` and `foo` share canonical form.
#[test]
fn validate_with_index_detects_case_collision() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    let manifest = r#"manifest_schema_version = "1.0"

[plugin]
name = "Foo"
version = "0.1.0"
description = "x"
triggers = ["process_writes"]

[dependencies]
database_version = ">=3.0.0"
"#;
    std::fs::write(plugin_dir.join("manifest.toml"), manifest).unwrap();
    std::fs::write(plugin_dir.join("__init__.py"), VALID_INIT).unwrap();
    let index = serde_json::json!({
        "index_schema_version": "1.0",
        "artifacts_url": "https://x.example/a",
        "plugins": [{
            "name": "foo",
            "version": "0.1.0",
            "description": "seed",
            "triggers": ["process_writes"],
            "dependencies": { "database_version": ">=3.0.0", "python": [] },
            "hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
        }]
    });
    let index_path = td.path().join("index.json");
    std::fs::write(&index_path, serde_json::to_string_pretty(&index).unwrap()).unwrap();

    let assert = spawn_validate(
        &plugin_dir,
        &["--output", "json", "--index", index_path.to_str().unwrap()],
    )
    .failure()
    .code(1);
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let payload: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let diags = payload["diagnostics"].as_array().unwrap();
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0]["variant"], "NameVersionConflict");
}

/// Multiline `plugin.description` must be rejected (one-line rule),
/// surfacing as a `SchemaReported` diagnostic at field `plugin.description`.
#[test]
fn validate_rejects_multiline_description() {
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("p");
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = r#"manifest_schema_version = "1.0"

[plugin]
name = "downsampler"
version = "1.2.0"
description = """
top
bottom
"""
triggers = ["process_writes"]

[dependencies]
database_version = ">=3.0.0"
"#;
    std::fs::write(dir.join("manifest.toml"), manifest).unwrap();
    std::fs::write(dir.join("__init__.py"), VALID_INIT).unwrap();

    let assert = spawn_validate(&dir, &["--output", "json"])
        .failure()
        .code(1);
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let payload: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let diags = payload["diagnostics"].as_array().expect("array");
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0]["variant"], "SchemaReported");
    assert_eq!(diags[0]["field"], "plugin.description");
    assert!(assert.get_output().stderr.is_empty());
}

/// Validator JSON-mode contract: a malformed `--index` file must
/// surface as a JSON document on stdout, with stderr empty.
#[test]
fn validate_with_malformed_index_emits_json_diagnostic() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    write_valid_plugin(&plugin_dir);
    let index_path = td.path().join("bad.json");
    std::fs::write(&index_path, "not valid json {{").unwrap();

    let assert = spawn_validate(
        &plugin_dir,
        &["--output", "json", "--index", index_path.to_str().unwrap()],
    )
    .failure()
    .code(1);

    let out = assert.get_output();
    assert!(
        out.stderr.is_empty(),
        "stderr MUST be empty in JSON mode, got {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let payload: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout must be one JSON document on parse failure");
    let diags = payload["diagnostics"].as_array().expect("array");
    assert!(
        !diags.is_empty(),
        "expected at least one diagnostic, got {payload}"
    );
    assert_eq!(diags[0]["variant"], "SchemaReported");
}

/// Index path that does not exist surfaces as a single
/// `IndexReadFailed` diagnostic on stdout in JSON mode.
#[test]
fn validate_with_unreadable_index_emits_json_diagnostic() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    write_valid_plugin(&plugin_dir);
    let missing = td.path().join("nope.json");

    let assert = spawn_validate(
        &plugin_dir,
        &["--output", "json", "--index", missing.to_str().unwrap()],
    )
    .failure()
    .code(1);

    let out = assert.get_output();
    assert!(out.stderr.is_empty(), "stderr empty: {:?}", out.stderr);
    let payload: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    let diags = payload["diagnostics"].as_array().unwrap();
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0]["variant"], "IndexReadFailed");
    assert_eq!(diags[0]["field"], missing.display().to_string());
}

/// Multi-error case: an index with two distinct schema defects (bad URL
/// scheme + non-SemVer version) surfaces as multiple `SchemaReported`
/// diagnostics in one document.
#[test]
fn validate_with_index_schema_errors_emits_all_diagnostics() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    write_valid_plugin(&plugin_dir);
    let bad_index = serde_json::json!({
        "index_schema_version": "1.0",
        "artifacts_url": "s3://nope",
        "plugins": [{
            "name": "downsampler",
            "version": "v1",
            "description": "seed",
            "triggers": ["process_writes"],
            "dependencies": { "database_version": ">=3.0.0", "python": [] },
            "hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
        }]
    });
    let index_path = td.path().join("bad-schema.json");
    std::fs::write(
        &index_path,
        serde_json::to_string_pretty(&bad_index).unwrap(),
    )
    .unwrap();

    let assert = spawn_validate(
        &plugin_dir,
        &["--output", "json", "--index", index_path.to_str().unwrap()],
    )
    .failure()
    .code(1);
    let out = assert.get_output();
    assert!(out.stderr.is_empty());
    let payload: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    let diags = payload["diagnostics"].as_array().unwrap();
    assert_eq!(
        diags.len(),
        2,
        "expected exactly two diagnostics, got {payload}"
    );
    assert!(
        diags.iter().all(|d| d["variant"] == "SchemaReported"),
        "all index schema errors should be SchemaReported, got {payload}"
    );
    let mut fields: Vec<&str> = diags.iter().map(|d| d["field"].as_str().unwrap()).collect();
    fields.sort();
    assert_eq!(fields, vec!["artifacts_url", "plugins[0].version"]);
}
