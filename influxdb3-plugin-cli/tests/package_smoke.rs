//! Integration tests for `influxdb3-plugin package`.
//!
//! Covers happy-path artifact + derived-index emission, duplicate
//! rejection, input-immutability, input/output non-overlap across
//! equivalence forms (same path, trailing slash, `.` segment, parent
//! traversal, symlink), and the data-tool JSON idiom.
//!
//! See `version_smoke.rs` for the rationale behind the crate-root allow.

#![allow(unused_crate_dependencies)]

use influxdb3_plugin_schemas::{Index, PublishedAt};
use rstest::rstest;
use std::io::Cursor;
use std::path::{Path, PathBuf};

mod common;
use common::{EMPTY_INDEX, SEEDED_INDEX, cli_cmd, write_valid_plugin};

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
    let derived =
        Index::parse_json(&std::fs::read_to_string(out_dir.join("index.json")).unwrap()).unwrap();
    assert_eq!(derived.plugins.len(), 1);
    let plugin = &derived.plugins[0];
    assert_eq!(plugin.name.as_str(), "downsampler");
    assert_eq!(plugin.version.to_string(), "1.2.0");
    PublishedAt::try_new(plugin.published_at.as_str()).unwrap();
    assert!(plugin.hash.as_str().starts_with("sha256:"));
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
        "index_schema_version": "2.0",
        "artifacts_url": "https://plugins.example.com/artifacts",
        "plugins": [
            {
                "name": "downsampler", "version": "1.0.0", "published_at": "2026-04-29T18:45:12Z",
                "description": "v1.0", "triggers": ["process_writes"],
                "dependencies": { "database_version": ">=3.0.0", "python": [] },
                "hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            },
            {
                "name": "downsampler", "version": "1.2.0", "published_at": "2026-04-30T00:00:00Z",
                "description": "v1.2", "triggers": ["process_writes"],
                "dependencies": { "database_version": ">=3.0.0", "python": [] },
                "hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
            },
            {
                "name": "other", "version": "9.9.9", "published_at": "2027-01-02T03:04:05Z",
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
        "output must list every existing version of `downsampler`, got: {stdout}"
    );
    assert!(
        !stdout.contains("9.9.9"),
        "output must NOT list versions of unrelated plugins, got: {stdout}"
    );
    assert!(
        stdout.contains("yank"),
        "output must direct the author to `yank`, got: {stdout}"
    );

    // No artifact / derived index written.
    assert!(!out_dir.join("downsampler-1.2.0.tar.gz").exists());
    // The check fires AFTER `--out` is created (canonicalize requires
    // existence), but the output files themselves must be absent.
    assert!(!out_dir.join("index.json").exists());
}

/// JSON-mode duplicate: the error envelope must carry the typed error
/// code `package::already_published` (not `cli::unknown`).
#[test]
fn package_duplicate_emits_typed_json_error_code() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    write_valid_plugin(&plugin_dir);
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    std::fs::write(&index_path, SEEDED_INDEX).unwrap();
    let out = td.path().join("build");

    let assert = spawn_package(&plugin_dir, &index_path, &out, &["--output", "json"])
        .failure()
        .code(1);
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let envelope: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must be valid JSON: {e}\n{stdout}"));
    assert_eq!(envelope["status"], "error");
    assert_eq!(
        envelope["error"]["code"], "package::already_published",
        "expected typed error code, got: {stdout}"
    );
}

/// JSON-mode canonical collision: the error envelope must carry the
/// typed error code `package::canonical_collision` (not `cli::unknown`).
#[test]
fn package_canonical_collision_emits_typed_json_error_code() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    write_plugin_named(&plugin_dir, "my_plugin", "1.0.0");
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    std::fs::write(&index_path, seeded_index_with("my-plugin", "1.0.0")).unwrap();
    let out = td.path().join("build");

    let assert = spawn_package(&plugin_dir, &index_path, &out, &["--output", "json"])
        .failure()
        .code(1);
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let envelope: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must be valid JSON: {e}\n{stdout}"));
    assert_eq!(envelope["status"], "error");
    assert_eq!(
        envelope["error"]["code"], "package::canonical_collision",
        "expected typed error code, got: {stdout}"
    );
}

// Canonical-form collision detection — hyphen/underscore and case
// differences collide under `Index::from_raw_json`'s canonical key
// (lowercase + `-` → `_`).

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
        "index_schema_version": "2.0",
        "artifacts_url": "https://plugins.example.com/artifacts",
        "plugins": [
            {
                "name": name, "version": version, "published_at": "2026-04-29T18:45:12Z",
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
    let derived: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("index.json")).unwrap())
            .unwrap();
    let seeded = derived["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["version"] == "1.0.0")
        .expect("seeded entry preserved");
    assert_eq!(seeded["published_at"], "2026-04-29T18:45:12Z");
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

#[test]
fn package_invalid_published_at_in_input_index_fails_before_outputs() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("p");
    write_valid_plugin(&plugin_dir);
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    let bad_index = SEEDED_INDEX.replace("2026-04-29T18:45:12Z", "2026-04-29T18:45:12.123Z");
    std::fs::write(&index_path, bad_index).unwrap();
    let out_dir = td.path().join("build");

    let assert = spawn_package(&plugin_dir, &index_path, &out_dir, &["--output", "json"])
        .failure()
        .code(1);

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let envelope: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(envelope["status"], "error");
    assert_eq!(envelope["error"]["code"], "package::index_parse_failed");
    assert_eq!(
        envelope["error"]["diagnostics"][0]["field"],
        "plugins[0].published_at"
    );
    assert!(!out_dir.join("index.json").exists());
    assert!(!out_dir.join("downsampler-1.2.0.tar.gz").exists());
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
    let published_at = result["new_entry_published_at"]
        .as_str()
        .unwrap()
        .to_owned();
    PublishedAt::try_new(&published_at).unwrap();
    result.insert(
        "new_entry_published_at".into(),
        "2026-04-29T18:45:12Z".into(),
    );

    insta::assert_json_snapshot!("package_success", envelope);
}

// ---------------------------------------------------------------------------
// Single-file plugin packaging
// ---------------------------------------------------------------------------

/// Returns the path to the SDK crate's test fixtures directory.
fn sdk_fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../influxdb3-plugin-sdk/tests/fixtures")
}

/// Packages the `valid_single_file_plugin` fixture and verifies:
/// - exit 0
/// - artifact tarball exists at expected name
/// - derived index.json exists and is parseable
/// - archive contains `single-file-plugin-0.1.0/manifest.toml` and
///   `single-file-plugin-0.1.0/my_plugin.py`
/// - archive does NOT contain any `__init__.py`
#[test]
fn package_single_file_plugin() {
    let td = tempfile::tempdir().unwrap();
    let fixture = sdk_fixtures().join("valid_single_file_plugin");
    let index_dir = td.path().join("reg");
    std::fs::create_dir_all(&index_dir).unwrap();
    let index_path = index_dir.join("index.json");
    write_empty_index(&index_path);
    let out_dir = td.path().join("build");

    spawn_package(&fixture, &index_path, &out_dir, &["--output", "json"]).success();

    // Artifact and derived index exist.
    let tarball_path = out_dir.join("single-file-plugin-0.1.0.tar.gz");
    assert!(
        tarball_path.exists(),
        "expected tarball at {}",
        tarball_path.display()
    );
    assert!(out_dir.join("index.json").exists());

    // Derived index round-trips and carries the new entry.
    let derived =
        Index::parse_json(&std::fs::read_to_string(out_dir.join("index.json")).unwrap()).unwrap();
    assert_eq!(derived.plugins.len(), 1);
    let plugin = &derived.plugins[0];
    assert_eq!(plugin.name.as_str(), "single-file-plugin");
    assert_eq!(plugin.version.to_string(), "0.1.0");

    // Inspect the archive contents.
    let tarball_bytes = std::fs::read(&tarball_path).unwrap();
    let gz = flate2::read::GzDecoder::new(Cursor::new(tarball_bytes));
    let mut archive = tar::Archive::new(gz);
    let paths: Vec<String> = archive
        .entries()
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path().unwrap().to_string_lossy().into_owned())
        .collect();

    assert!(
        paths.contains(&"single-file-plugin-0.1.0/manifest.toml".to_owned()),
        "archive must contain manifest.toml, got: {paths:?}"
    );
    assert!(
        paths.contains(&"single-file-plugin-0.1.0/my_plugin.py".to_owned()),
        "archive must contain my_plugin.py, got: {paths:?}"
    );
    assert!(
        !paths.iter().any(|p| p.contains("__init__.py")),
        "archive must NOT contain __init__.py, got: {paths:?}"
    );
}
