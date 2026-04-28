//! Integration tests for `influxdb3-plugin package`.
//!
//! Covers happy-path artifact + derived-index emission, duplicate
//! rejection, input-immutability, input/output non-overlap across
//! equivalence forms (same path, trailing slash, `.` segment, parent
//! traversal, symlink), and the data-tool JSON idiom.
//!
//! See `version_smoke.rs` for the rationale behind the crate-root allow.

#![allow(unused_crate_dependencies)]

use rstest::rstest;
use std::path::{Path, PathBuf};

mod common;
use common::{EMPTY_INDEX, cli_cmd, write_valid_plugin};

fn write_empty_index(path: &Path) {
    std::fs::write(path, EMPTY_INDEX).unwrap();
}

fn spawn_package(
    plugin_dir: &Path,
    index_path: &Path,
    out_dir: &Path,
    extra: &[&str],
) -> assert_cmd::assert::Assert {
    let mut cmd = cli_cmd();
    cmd.arg("package");
    for a in extra {
        cmd.arg(a);
    }
    cmd.arg(plugin_dir);
    cmd.arg("--index").arg(index_path);
    cmd.arg("--out").arg(out_dir);
    cmd.assert()
}

#[test]
fn package_happy_path_writes_artifact_and_derived_index() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    write_valid_plugin(&plugin_dir);
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    write_empty_index(&index_path);
    let out_dir = td.path().join("build");

    spawn_package(&plugin_dir, &index_path, &out_dir, &["--output", "json"]).success();

    assert!(out_dir.join("downsampler-1.2.0.tar.gz").exists());
    assert!(out_dir.join("index.json").exists());

    // Derived index round-trips via Index::parse_json + carries the new entry.
    let derived: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("index.json")).unwrap())
            .unwrap();
    let plugins = derived["plugins"].as_array().unwrap();
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0]["name"], "downsampler");
    assert_eq!(plugins[0]["version"], "1.2.0");
    assert!(plugins[0]["hash"].as_str().unwrap().starts_with("sha256:"));
}

/// The input `--index` file's bytes are byte-identical pre/post.
/// Hashing here is by string equality — the index file is small enough.
#[test]
fn package_does_not_modify_input_index() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    write_valid_plugin(&plugin_dir);
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    write_empty_index(&index_path);
    let out_dir = td.path().join("build");

    let before = std::fs::read_to_string(&index_path).unwrap();
    spawn_package(&plugin_dir, &index_path, &out_dir, &[]).success();
    let after = std::fs::read_to_string(&index_path).unwrap();

    assert_eq!(before, after, "input --index file must be byte-identical");
}

/// Duplicate `(name, version)` in the input index → exit 1, no outputs
/// created. The error message must enumerate every existing version of
/// the plugin and direct the author to either increment `plugin.version`
/// or run `yank`.
#[test]
fn package_rejects_duplicate_name_version() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    write_valid_plugin(&plugin_dir);
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    // Seed two prior versions of `downsampler` plus an unrelated entry
    // to confirm the payload only enumerates `downsampler`'s versions.
    let preexisting = serde_json::json!({
        "index_schema_version": "1.0",
        "artifacts_url": "https://plugins.example.com/artifacts",
        "plugins": [
            {
                "name": "downsampler", "version": "1.0.0",
                "description": "v1.0", "triggers": ["process_writes"],
                "dependencies": { "database_version": ">=3.0.0", "python": [] },
                "hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            },
            {
                "name": "downsampler", "version": "1.2.0",
                "description": "v1.2", "triggers": ["process_writes"],
                "dependencies": { "database_version": ">=3.0.0", "python": [] },
                "hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
            },
            {
                "name": "other", "version": "9.9.9",
                "description": "unrelated", "triggers": ["process_writes"],
                "dependencies": { "database_version": ">=3.0.0", "python": [] },
                "hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222"
            }
        ]
    });
    std::fs::write(
        &index_path,
        serde_json::to_string_pretty(&preexisting).unwrap(),
    )
    .unwrap();
    let out_dir = td.path().join("build");

    let assert = spawn_package(&plugin_dir, &index_path, &out_dir, &[])
        .failure()
        .code(1);

    // Payload contract: output must enumerate the existing versions of
    // `downsampler` AND direct the author to the actionable remediation.
    // The unrelated `other@9.9.9` must NOT appear. Under piped stdout,
    // errors render as JSON envelopes on stdout; version strings appear
    // inside the JSON message field with escaped quotes.
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(
        stdout.contains("1.0.0") && stdout.contains("1.2.0"),
        "output must list every existing version of `downsampler` (S2-2), got: {stdout}"
    );
    assert!(
        !stdout.contains("9.9.9"),
        "output must NOT list versions of unrelated plugins, got: {stdout}"
    );
    assert!(
        stdout.contains("yank"),
        "output must direct the author to `yank` per S2-2, got: {stdout}"
    );

    // No artifact / derived index written.
    assert!(!out_dir.join("downsampler-1.2.0.tar.gz").exists());
    // The check fires AFTER `--out` is created (canonicalize requires
    // existence), but the output files themselves must be absent.
    assert!(!out_dir.join("index.json").exists());
}

// -----------------------------------------------------------------------
// Canonical-form collision detection — hyphen/underscore and case
// differences collide under `Index::from_raw_json`'s canonical key
// (lowercase + `-` → `_`). The new `package` check must fire, reusing
// the existing S2-2 payload shape (list versions + direct to `yank`).
// -----------------------------------------------------------------------

/// Writes a plugin directory with a caller-chosen `name` and `version`.
/// Mirrors `write_valid_plugin` but parameterized so collision tests can
/// pick names that differ only by canonicalization.
fn write_plugin_named(dir: &Path, name: &str, version: &str) {
    std::fs::create_dir_all(dir).unwrap();
    let manifest = format!(
        r#"manifest_schema_version = "1.0"

[plugin]
name = "{name}"
version = "{version}"
description = "Test plugin."
triggers = ["process_writes"]

[dependencies]
database_version = ">=3.0.0"
"#
    );
    std::fs::write(dir.join("manifest.toml"), manifest).unwrap();
    std::fs::write(
        dir.join("__init__.py"),
        "def process_writes(a, b, c):\n    pass\n",
    )
    .unwrap();
}

/// Builds an `index.json` body with a single seeded entry at
/// `(name, version)`.
fn seeded_index_with(name: &str, version: &str) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "index_schema_version": "1.0",
        "artifacts_url": "https://plugins.example.com/artifacts",
        "plugins": [
            {
                "name": name, "version": version,
                "description": "seed", "triggers": ["process_writes"],
                "dependencies": { "database_version": ">=3.0.0", "python": [] },
                "hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            }
        ]
    }))
    .unwrap()
}

#[test]
fn package_rejects_hyphen_underscore_collision() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    write_plugin_named(&plugin_dir, "my_plugin", "1.0.0");
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    std::fs::write(&index_path, seeded_index_with("my-plugin", "1.0.0")).unwrap();
    let out_dir = td.path().join("build");

    let assert = spawn_package(&plugin_dir, &index_path, &out_dir, &[])
        .failure()
        .code(1);
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(
        stdout.contains("canonical collision"),
        "output must name the collision class, got: {stdout}"
    );
    assert!(
        stdout.contains("my_plugin"),
        "output must name the rejected spelling, got: {stdout}"
    );
    assert!(
        stdout.contains("my-plugin"),
        "output must name the existing spelling, got: {stdout}"
    );
    assert!(
        stdout.contains("Rename"),
        "output must direct the author to rename, got: {stdout}"
    );

    assert!(!out_dir.join("my_plugin-1.0.0.tar.gz").exists());
    assert!(!out_dir.join("index.json").exists());
}

#[test]
fn package_rejects_case_collision() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    write_plugin_named(&plugin_dir, "MyPlugin", "1.0.0");
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    std::fs::write(&index_path, seeded_index_with("myplugin", "1.0.0")).unwrap();
    let out_dir = td.path().join("build");

    let assert = spawn_package(&plugin_dir, &index_path, &out_dir, &[])
        .failure()
        .code(1);
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(
        stdout.contains("canonical collision"),
        "output must name the collision class, got: {stdout}"
    );
    assert!(
        stdout.contains("MyPlugin"),
        "output must name the rejected spelling, got: {stdout}"
    );
    assert!(
        stdout.contains("myplugin"),
        "output must name the existing spelling, got: {stdout}"
    );
    assert!(
        stdout.contains("Rename"),
        "output must direct the author to rename, got: {stdout}"
    );

    assert!(!out_dir.join("MyPlugin-1.0.0.tar.gz").exists());
    assert!(!out_dir.join("index.json").exists());
}

/// Canonical keying must NOT over-match on distinct versions of the same
/// canonical name — packaging `my_plugin@1.0.1` against an index seeded
/// with `my_plugin@1.0.0` succeeds.
#[test]
fn package_accepts_different_versions_of_same_canonical_name() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    write_plugin_named(&plugin_dir, "my_plugin", "1.0.1");
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    std::fs::write(&index_path, seeded_index_with("my_plugin", "1.0.0")).unwrap();
    let out_dir = td.path().join("build");

    spawn_package(&plugin_dir, &index_path, &out_dir, &[]).success();
    assert!(out_dir.join("my_plugin-1.0.1.tar.gz").exists());
    assert!(out_dir.join("index.json").exists());
}

/// Input/output non-overlap: every equivalence form for
/// `--out == dirname(--index)` must be rejected.
#[rstest]
#[case::same_path("eq")]
#[case::trailing_slash("trailing-slash")]
#[case::dot_segment("dot-segment")]
#[case::parent_traversal("parent-traversal")]
fn package_rejects_out_overlapping_index_dir(#[case] mode: &str) {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    write_valid_plugin(&plugin_dir);
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    write_empty_index(&index_path);

    // All variants below resolve to `index_dir` after canonicalization.
    let out_dir: PathBuf = match mode {
        "eq" => index_dir.clone(),
        "trailing-slash" => {
            let mut s = index_dir.as_os_str().to_owned();
            s.push("/");
            PathBuf::from(s)
        }
        "dot-segment" => index_dir.join("."),
        "parent-traversal" => index_dir.join("subdir").join(".."),
        _ => unreachable!(),
    };

    let assert = spawn_package(&plugin_dir, &index_path, &out_dir, &[])
        .failure()
        .code(2);
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(
        stdout.contains("S2-12"),
        "output should reference the S2-12 contract by identifier, got: {stdout}"
    );

    // Critical corollary: the input index file is unchanged.
    assert_eq!(
        std::fs::read_to_string(&index_path).unwrap(),
        EMPTY_INDEX,
        "input --index must be byte-identical even on rejection"
    );
}

/// Symlinked `--out` pointing at the index's directory is also rejected
/// (canonicalize resolves the symlink).
#[cfg(unix)]
#[test]
fn package_rejects_out_via_symlink_to_index_dir() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    write_valid_plugin(&plugin_dir);
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    write_empty_index(&index_path);
    let out_link = td.path().join("link-to-reg");
    std::os::unix::fs::symlink(&index_dir, &out_link).unwrap();

    let assert = spawn_package(&plugin_dir, &index_path, &out_link, &[])
        .failure()
        .code(2);
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    assert!(
        stdout.contains("S2-12"),
        "symlink rejection should reference S2-12, got: {stdout}"
    );
    assert_eq!(
        std::fs::read_to_string(&index_path).unwrap(),
        EMPTY_INDEX,
        "input --index must be byte-identical even on symlink rejection"
    );
}

/// JSON-mode failure path: errors render as JSON envelope on stdout;
/// stderr empty.
#[test]
fn package_failure_in_json_mode_emits_error_envelope() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    // Plugin dir intentionally missing -> MissingRequiredFile failure.
    std::fs::create_dir_all(&plugin_dir).unwrap();
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    write_empty_index(&index_path);
    let out_dir = td.path().join("build");

    let assert = spawn_package(&plugin_dir, &index_path, &out_dir, &["--output", "json"])
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

/// JSON success snapshot — strip the per-machine paths so the snapshot
/// is reproducible. Output is now an envelope:
/// `{"status":"ok","result":{...PackageOutput...}}`
#[test]
fn package_json_success_snapshot() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    write_valid_plugin(&plugin_dir);
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    write_empty_index(&index_path);
    let out_dir = td.path().join("build");

    let assert = spawn_package(&plugin_dir, &index_path, &out_dir, &["--output", "json"]).success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let mut envelope: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        envelope.get("status").and_then(|v| v.as_str()),
        Some("ok"),
        "envelope status must be \"ok\"; got:\n{stdout}"
    );
    let result = envelope
        .get_mut("result")
        .expect("envelope must have \"result\" key")
        .as_object_mut()
        .expect("result must be an object");
    result.insert(
        "artifact_path".into(),
        "<TMPDIR>/build/downsampler-1.2.0.tar.gz".into(),
    );
    result.insert("index_path".into(), "<TMPDIR>/build/index.json".into());
    // Hash depends on archive bytes; assert format then strip.
    let hash = result["hash"].as_str().unwrap().to_owned();
    assert!(
        hash.starts_with("sha256:") && hash.len() == "sha256:".len() + 64,
        "hash format unexpected: {hash}"
    );
    result.insert("hash".into(), "sha256:<64 hex>".into());

    insta::assert_json_snapshot!("package_success", envelope);
}
