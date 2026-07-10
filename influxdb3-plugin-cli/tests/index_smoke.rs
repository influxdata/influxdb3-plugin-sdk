//! Integration tests for `influxdb3-plugin search` and `influxdb3-plugin info`.
//!
//! Covers the CLI boundary for local, read-only index inspection: argument
//! parsing, JSON projection, human rendering, exit codes, failure envelopes,
//! and input immutability. Query semantics themselves live in
//! `influxdb3-plugin-schemas` and are only spot-checked here.

#![allow(unused_crate_dependencies)]

use std::path::Path;

mod common;
use common::{assert_absolute_json_path, cli_cmd};

const HASH_0: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
const HASH_1: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
const HASH_2: &str = "sha256:2222222222222222222222222222222222222222222222222222222222222222";
const HASH_3: &str = "sha256:3333333333333333333333333333333333333333333333333333333333333333";
const HASH_4: &str = "sha256:4444444444444444444444444444444444444444444444444444444444444444";
const HASH_5: &str = "sha256:5555555555555555555555555555555555555555555555555555555555555555";
const HASH_6: &str = "sha256:6666666666666666666666666666666666666666666666666666666666666666";

fn rich_index() -> serde_json::Value {
    serde_json::json!({
        "index_schema_version": "2.0",
        "artifacts_url": "https://plugins.example.com/artifacts",
        "plugins": [
            {
                "name": "alpha_writer",
                "version": "1.0.0",
                "published_at": "2026-04-29T18:45:12Z",
                "description": "Alpha writer plugin",
                "triggers": ["process_writes"],
                "dependencies": { "database_version": ">=3.0.0", "python": [] },
                "hash": HASH_0
            },
            {
                "name": "downsampler",
                "version": "1.0.0",
                "published_at": "2026-04-30T10:00:00Z",
                "description": "Downsample writes older version",
                "triggers": ["process_writes"],
                "dependencies": { "database_version": ">=3.0.0", "python": [] },
                "hash": HASH_1
            },
            {
                "name": "downsampler",
                "version": "1.2.0",
                "published_at": "2026-05-01T11:22:33Z",
                "description": "Downsample writes",
                "triggers": ["process_writes"],
                "homepage": "https://example.com/downsampler",
                "repository": "https://github.com/example/downsampler",
                "documentation": "https://docs.example.com/downsampler",
                "dependencies": {
                    "database_version": ">=3.0.0",
                    "python": ["requests>=2.31,<3"],
                    "plugins": [
                        {
                            "index_url": "https://plugins.example.com/index.json",
                            "name": "geo-lookup",
                            "version": ">=1.0.0, <2.0.0"
                        }
                    ]
                },
                "hash": HASH_2
            },
            {
                "name": "downsampler",
                "version": "2.0.0",
                "published_at": "2026-06-01T09:00:00Z",
                "description": "Yanked major downsampler",
                "triggers": ["process_writes"],
                "dependencies": { "database_version": ">=3.0.0", "python": [] },
                "hash": HASH_3,
                "yanked": true
            },
            {
                "name": "future_writer",
                "version": "2.0.0",
                "published_at": "2026-05-02T12:00:00Z",
                "description": "Future writer plugin",
                "triggers": ["process_writes"],
                "dependencies": { "database_version": ">=4.0.0", "python": [] },
                "hash": HASH_4
            },
            {
                "name": "http_auth",
                "version": "0.3.0",
                "published_at": "2026-05-03T12:00:00Z",
                "description": "Authenticate request plugins",
                "triggers": ["process_request"],
                "dependencies": { "database_version": ">=3.0.0", "python": [] },
                "hash": HASH_5
            },
            {
                "name": "legacy_rollup",
                "version": "0.9.0",
                "published_at": "2026-05-04T12:00:00Z",
                "description": "Legacy rollup job",
                "triggers": ["process_scheduled_call"],
                "dependencies": { "database_version": ">=3.0.0", "python": [] },
                "hash": HASH_6,
                "yanked": true
            }
        ]
    })
}

fn empty_index() -> serde_json::Value {
    serde_json::json!({
        "index_schema_version": "2.0",
        "artifacts_url": "https://plugins.example.com/artifacts",
        "plugins": []
    })
}

fn file_base_index() -> serde_json::Value {
    serde_json::json!({
        "index_schema_version": "2.0",
        "artifacts_url": "file:///tmp/reg",
        "plugins": [
            {
                "name": "alpha_writer",
                "version": "1.0.0",
                "published_at": "2026-04-29T18:45:12Z",
                "description": "Alpha writer plugin",
                "triggers": ["process_writes"],
                "dependencies": { "database_version": ">=3.0.0", "python": [] },
                "hash": HASH_0
            }
        ]
    })
}

fn write_index(path: &Path, value: serde_json::Value) {
    std::fs::write(path, serde_json::to_string_pretty(&value).unwrap()).unwrap();
}

fn write_rich_index(path: &Path) {
    write_index(path, rich_index());
}

fn index_path(td: &tempfile::TempDir) -> std::path::PathBuf {
    let dir = td.path().join("reg");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("index.json")
}

fn spawn_index_search(
    index_path: &Path,
    query: Option<&str>,
    extra: &[&str],
) -> assert_cmd::assert::Assert {
    let mut cmd = cli_cmd();
    cmd.arg("search");
    cmd.arg("--index").arg(index_path);
    if let Some(q) = query {
        cmd.arg(q);
    }
    for arg in extra {
        cmd.arg(arg);
    }
    cmd.assert()
}

fn spawn_index_info(index_path: &Path, name: &str, extra: &[&str]) -> assert_cmd::assert::Assert {
    let mut cmd = cli_cmd();
    cmd.arg("info");
    cmd.arg("--index").arg(index_path);
    cmd.arg(name);
    for arg in extra {
        cmd.arg(arg);
    }
    cmd.assert()
}

fn parse_stdout(output: &std::process::Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout must be valid JSON: {e}\n{stdout}"))
}

fn assert_json_success(assert: assert_cmd::assert::Assert) -> serde_json::Value {
    let assert = assert.success().stderr(predicates::str::is_empty());
    let doc = parse_stdout(assert.get_output());
    assert_eq!(doc["status"], "ok");
    doc
}

fn run_info_human(index_path: &Path, name: &str, extra: &[&str]) -> String {
    let mut full_extra = vec!["--output", "human"];
    full_extra.extend_from_slice(extra);
    let assert = spawn_index_info(index_path, name, &full_extra)
        .success()
        .stderr(predicates::str::is_empty());
    String::from_utf8(assert.get_output().stdout.clone()).expect("utf-8 stdout")
}

fn line_index_of(haystack: &str, prefix: &str) -> Option<usize> {
    haystack.lines().position(|line| line.starts_with(prefix))
}

fn assert_json_error(
    assert: assert_cmd::assert::Assert,
    exit_code: i32,
    error_code: &str,
) -> serde_json::Value {
    let assert = assert
        .failure()
        .code(exit_code)
        .stderr(predicates::str::is_empty());
    let doc = parse_stdout(assert.get_output());
    assert_eq!(doc["status"], "error");
    assert_eq!(doc["error"]["code"], error_code);
    doc
}

fn hit_names(doc: &serde_json::Value) -> Vec<String> {
    doc["result"]["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| hit["name"].as_str().unwrap().to_owned())
        .collect()
}

fn find_hit<'a>(doc: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    doc["result"]["hits"]
        .as_array()
        .unwrap()
        .iter()
        .find(|hit| hit["name"] == name)
        .unwrap_or_else(|| panic!("expected hit {name} in {doc:#}"))
}

#[test]
fn search_all_visible_plugins_json() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let doc = assert_json_success(spawn_index_search(&path, None, &["--output", "json"]));

    assert_eq!(
        hit_names(&doc),
        vec!["alpha_writer", "downsampler", "future_writer", "http_auth"]
    );
    assert_eq!(find_hit(&doc, "downsampler")["version"], "1.2.0");
    assert_eq!(
        find_hit(&doc, "downsampler")["published_at"],
        "2026-05-01T11:22:33Z"
    );
    assert_eq!(
        find_hit(&doc, "downsampler")["triggers"],
        serde_json::json!(["process_writes"])
    );
    assert_eq!(
        find_hit(&doc, "downsampler")["visibility"],
        serde_json::json!({"status": "visible"})
    );

    insta::assert_json_snapshot!("index_search_json", doc);
}

#[test]
fn search_text_filters_by_name_and_description() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let by_name = assert_json_success(spawn_index_search(
        &path,
        Some("http"),
        &["--output", "json"],
    ));
    assert_eq!(hit_names(&by_name), vec!["http_auth"]);

    let by_description = assert_json_success(spawn_index_search(
        &path,
        Some("Authenticate"),
        &["--output", "json"],
    ));
    assert_eq!(hit_names(&by_description), vec!["http_auth"]);
}

#[test]
fn search_trigger_filter_returns_only_matching_triggers() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let doc = assert_json_success(spawn_index_search(
        &path,
        None,
        &["--trigger-type", "process_request", "--output", "json"],
    ));

    assert_eq!(hit_names(&doc), vec!["http_auth"]);
    for hit in doc["result"]["hits"].as_array().unwrap() {
        assert!(
            hit["triggers"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == "process_request"),
            "hit must include requested trigger: {hit:#}"
        );
    }
}

#[test]
fn search_hides_and_includes_yanked_versions() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let default_doc = assert_json_success(spawn_index_search(&path, None, &["--output", "json"]));
    assert!(!hit_names(&default_doc).contains(&"legacy_rollup".to_owned()));
    assert_eq!(find_hit(&default_doc, "downsampler")["version"], "1.2.0");

    let included = assert_json_success(spawn_index_search(
        &path,
        None,
        &["--include-yanked", "--output", "json"],
    ));
    assert!(hit_names(&included).contains(&"legacy_rollup".to_owned()));
    assert_eq!(find_hit(&included, "downsampler")["version"], "2.0.0");
    assert_eq!(
        find_hit(&included, "legacy_rollup")["visibility"],
        serde_json::json!({
            "status": "hidden",
            "reasons": [{"kind": "yanked"}]
        })
    );
}

#[test]
fn search_hides_and_includes_incompatible_versions() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let default_doc = assert_json_success(spawn_index_search(
        &path,
        None,
        &["--database-version", "3.2.0", "--output", "json"],
    ));
    assert_eq!(
        hit_names(&default_doc),
        vec!["alpha_writer", "downsampler", "http_auth"]
    );
    assert!(!hit_names(&default_doc).contains(&"future_writer".to_owned()));

    let included = assert_json_success(spawn_index_search(
        &path,
        None,
        &[
            "--database-version",
            "3.2.0",
            "--include-incompatible",
            "--output",
            "json",
        ],
    ));
    let future = find_hit(&included, "future_writer");
    assert_eq!(
        future["visibility"],
        serde_json::json!({
            "status": "hidden",
            "reasons": [{
                "kind": "incompatible_database_version",
                "required": ">=4.0.0",
                "actual": "3.2.0"
            }]
        })
    );
}

#[test]
fn search_empty_index_and_zero_match_are_successful() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_index(&path, empty_index());

    let empty = assert_json_success(spawn_index_search(&path, None, &["--output", "json"]));
    assert_eq!(empty["result"]["hits"], serde_json::json!([]));

    write_rich_index(&path);
    spawn_index_search(&path, Some("does-not-match"), &["--output", "human"])
        .success()
        .stdout(predicates::str::contains("No matching plugins found."))
        .stderr(predicates::str::is_empty());
}

#[test]
fn info_by_name_selects_latest_visible_version_json() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let doc = assert_json_success(spawn_index_info(
        &path,
        "downsampler",
        &["--output", "json"],
    ));

    assert_eq!(doc["result"]["outcome"], "found");
    assert_eq!(doc["result"]["plugin"]["version"], "1.2.0");
    assert_eq!(
        doc["result"]["plugin"]["published_at"],
        "2026-05-01T11:22:33Z"
    );
    assert!(doc["result"]["plugin"].get("versions").is_none());
    assert_eq!(
        doc["result"]["plugin"]["dependencies"],
        serde_json::json!({
            "database_version": ">=3.0.0",
            "python": ["requests>=2.31,<3"],
            "plugins": [
                {
                    "index_url": "https://plugins.example.com/index.json",
                    "name": "geo-lookup",
                    "version": ">=1.0.0, <2.0.0"
                }
            ]
        })
    );
    assert_eq!(
        doc["result"]["plugin"]["homepage"],
        "https://example.com/downsampler"
    );
    assert_eq!(
        doc["result"]["plugin"]["artifact_url"]
            .as_str()
            .expect("artifact_url string"),
        "https://plugins.example.com/artifacts/downsampler-1.2.0.tar.gz"
    );

    insta::assert_json_snapshot!("index_info_found_json", doc);
}

#[test]
fn info_exact_version_returns_requested_version() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let doc = assert_json_success(spawn_index_info(
        &path,
        "downsampler",
        &["--version", "1.0.0", "--output", "json"],
    ));

    assert_eq!(doc["result"]["outcome"], "found");
    assert_eq!(doc["result"]["plugin"]["version"], "1.0.0");
}

#[test]
fn info_exact_hidden_versions_are_found_with_visibility_reasons() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let yanked = assert_json_success(spawn_index_info(
        &path,
        "legacy_rollup",
        &["--version", "0.9.0", "--output", "json"],
    ));
    assert_eq!(yanked["result"]["outcome"], "found");
    assert_eq!(
        yanked["result"]["plugin"]["visibility"],
        serde_json::json!({
            "status": "hidden",
            "reasons": [{"kind": "yanked"}]
        })
    );

    let incompatible = assert_json_success(spawn_index_info(
        &path,
        "future_writer",
        &[
            "--version",
            "2.0.0",
            "--database-version",
            "3.2.0",
            "--output",
            "json",
        ],
    ));
    assert_eq!(incompatible["result"]["outcome"], "found");
    assert_eq!(
        incompatible["result"]["plugin"]["visibility"],
        serde_json::json!({
            "status": "hidden",
            "reasons": [{
                "kind": "incompatible_database_version",
                "required": ">=4.0.0",
                "actual": "3.2.0"
            }]
        })
    );
}

#[test]
fn info_not_found_outcomes_are_successful_json() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let missing_name = assert_json_success(spawn_index_info(
        &path,
        "no_such_plugin",
        &["--output", "json"],
    ));
    assert_eq!(missing_name["result"]["outcome"], "not_found");
    assert_eq!(missing_name["result"]["name"], "no_such_plugin");
    assert_eq!(missing_name["result"]["version"], serde_json::Value::Null);

    insta::assert_json_snapshot!("index_info_not_found_json", missing_name);

    let missing_version = assert_json_success(spawn_index_info(
        &path,
        "downsampler",
        &["--version", "9.9.9", "--output", "json"],
    ));
    assert_eq!(missing_version["result"]["outcome"], "not_found");
    assert_eq!(missing_version["result"]["name"], "downsampler");
    assert_eq!(missing_version["result"]["version"], "9.9.9");
}

#[test]
fn info_filtered_out_is_successful_json() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let doc = assert_json_success(spawn_index_info(
        &path,
        "legacy_rollup",
        &["--output", "json"],
    ));
    assert_eq!(doc["result"]["outcome"], "filtered_out");
    assert_eq!(doc["result"]["name"], "legacy_rollup");
    assert_eq!(
        doc["result"]["reasons"],
        serde_json::json!([{"kind": "yanked"}])
    );

    insta::assert_json_snapshot!("index_info_filtered_out_json", doc);
}

#[test]
fn info_json_artifact_url_uses_exact_version_selection() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let doc = assert_json_success(spawn_index_info(
        &path,
        "downsampler",
        &["--version", "1.0.0", "--output", "json"],
    ));

    assert_eq!(
        doc["result"]["plugin"]["artifact_url"]
            .as_str()
            .expect("artifact_url string"),
        "https://plugins.example.com/artifacts/downsampler-1.0.0.tar.gz"
    );
}

#[test]
fn info_json_artifact_url_present_for_yanked_when_included() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let doc = assert_json_success(spawn_index_info(
        &path,
        "legacy_rollup",
        &["--include-yanked", "--output", "json"],
    ));

    assert_eq!(doc["result"]["outcome"], "found");
    assert_eq!(
        doc["result"]["plugin"]["artifact_url"]
            .as_str()
            .expect("artifact_url string"),
        "https://plugins.example.com/artifacts/legacy_rollup-0.9.0.tar.gz"
    );
}

#[test]
fn info_json_artifact_url_present_for_incompatible_when_included() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let doc = assert_json_success(spawn_index_info(
        &path,
        "future_writer",
        &[
            "--database-version",
            "3.2.0",
            "--include-incompatible",
            "--output",
            "json",
        ],
    ));

    assert_eq!(doc["result"]["outcome"], "found");
    assert_eq!(
        doc["result"]["plugin"]["artifact_url"]
            .as_str()
            .expect("artifact_url string"),
        "https://plugins.example.com/artifacts/future_writer-2.0.0.tar.gz"
    );
}

#[test]
fn info_json_no_artifact_url_when_not_found() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let doc = assert_json_success(spawn_index_info(
        &path,
        "no_such_plugin",
        &["--output", "json"],
    ));

    assert_eq!(doc["result"]["outcome"], "not_found");
    assert!(
        doc["result"].get("plugin").is_none(),
        "result.plugin must be absent for not_found"
    );
    assert!(
        doc["result"].get("artifact_url").is_none(),
        "no top-level artifact_url for not_found"
    );
}

#[test]
fn info_json_no_artifact_url_when_filtered_out() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let doc = assert_json_success(spawn_index_info(
        &path,
        "legacy_rollup",
        &["--output", "json"],
    ));

    assert_eq!(doc["result"]["outcome"], "filtered_out");
    assert!(
        doc["result"].get("plugin").is_none(),
        "result.plugin must be absent for filtered_out"
    );
    assert!(
        doc["result"].get("artifact_url").is_none(),
        "no top-level artifact_url for filtered_out"
    );
}

#[test]
fn info_json_artifact_url_uses_index_base_url_for_file_scheme() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_index(&path, file_base_index());

    let doc = assert_json_success(spawn_index_info(
        &path,
        "alpha_writer",
        &["--output", "json"],
    ));

    assert_eq!(
        doc["result"]["plugin"]["artifact_url"]
            .as_str()
            .expect("artifact_url string"),
        "file:///tmp/reg/alpha_writer-1.0.0.tar.gz"
    );
}

#[test]
fn info_human_found_and_not_found_write_stdout() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let found = spawn_index_info(&path, "downsampler", &["--output", "human"])
        .success()
        .stderr(predicates::str::is_empty());
    let stdout = String::from_utf8_lossy(&found.get_output().stdout);
    for expected in [
        "downsampler",
        "Downsample writes",
        "version:",
        "published_at:",
        "triggers:",
        "database:",
        "python:",
        "plugins: geo-lookup >=1.0.0, <2.0.0 (https://plugins.example.com/index.json)",
        "homepage:",
        "repository:",
        "documentation:",
        "hash:",
        "visibility: visible",
    ] {
        assert!(
            stdout.contains(expected),
            "human info output should contain {expected:?}; got:\n{stdout}"
        );
    }

    spawn_index_info(&path, "no_such_plugin", &["--output", "human"])
        .success()
        .stdout(predicates::str::contains("Plugin not found"))
        .stderr(predicates::str::is_empty());
}

#[test]
fn info_human_includes_artifact_url_line_for_visible_plugin() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let stdout = run_info_human(&path, "downsampler", &[]);
    assert!(
        stdout.contains(
            "artifact_url: https://plugins.example.com/artifacts/downsampler-1.2.0.tar.gz"
        ),
        "missing artifact_url line in human stdout:\n{stdout}"
    );
}

#[test]
fn info_human_artifact_url_positioned_after_optionals_before_hash() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let stdout = run_info_human(&path, "downsampler", &[]);

    let artifact = line_index_of(&stdout, "artifact_url:").expect("artifact_url line");
    let hash = line_index_of(&stdout, "hash:").expect("hash line");
    let homepage = line_index_of(&stdout, "homepage:").expect("homepage line");
    let repository = line_index_of(&stdout, "repository:").expect("repository line");
    let documentation = line_index_of(&stdout, "documentation:").expect("documentation line");

    assert!(
        homepage < artifact,
        "homepage must appear above artifact_url"
    );
    assert!(
        repository < artifact,
        "repository must appear above artifact_url"
    );
    assert!(
        documentation < artifact,
        "documentation must appear above artifact_url"
    );
    assert_eq!(
        artifact + 1,
        hash,
        "artifact_url must appear directly before hash"
    );
}

#[test]
fn info_human_artifact_url_present_when_optionals_absent() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let stdout = run_info_human(&path, "alpha_writer", &[]);

    assert!(
        stdout.contains(
            "artifact_url: https://plugins.example.com/artifacts/alpha_writer-1.0.0.tar.gz"
        ),
        "missing artifact_url for plugin without optionals:\n{stdout}"
    );
    assert!(!stdout.contains("homepage:"));
    assert!(!stdout.contains("repository:"));
    assert!(!stdout.contains("documentation:"));

    let artifact = line_index_of(&stdout, "artifact_url:").expect("artifact_url line");
    let hash = line_index_of(&stdout, "hash:").expect("hash line");
    assert_eq!(
        artifact + 1,
        hash,
        "artifact_url must be the line immediately before hash when no optionals exist"
    );
}

#[test]
fn info_human_artifact_url_uses_selected_version() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let stdout = run_info_human(&path, "downsampler", &["--version", "1.0.0"]);
    assert!(
        stdout.contains(
            "artifact_url: https://plugins.example.com/artifacts/downsampler-1.0.0.tar.gz"
        ),
        "human output must use selected version, not latest:\n{stdout}"
    );
}

#[test]
fn info_human_artifact_url_present_for_yanked_when_included() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let stdout = run_info_human(&path, "legacy_rollup", &["--include-yanked"]);
    assert!(
        stdout.contains(
            "artifact_url: https://plugins.example.com/artifacts/legacy_rollup-0.9.0.tar.gz"
        ),
        "missing artifact_url for yanked-and-included entry:\n{stdout}"
    );
}

#[test]
fn info_human_artifact_url_present_for_incompatible_when_included() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let stdout = run_info_human(
        &path,
        "future_writer",
        &["--database-version", "3.2.0", "--include-incompatible"],
    );
    assert!(
        stdout.contains(
            "artifact_url: https://plugins.example.com/artifacts/future_writer-2.0.0.tar.gz"
        ),
        "missing artifact_url for incompatible-and-included entry:\n{stdout}"
    );
}

#[test]
fn info_human_no_artifact_url_when_not_found() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let stdout = run_info_human(&path, "no_such_plugin", &[]);
    assert!(
        !stdout.contains("artifact_url"),
        "not-found human stdout must not contain artifact_url:\n{stdout}"
    );
}

#[test]
fn info_human_no_artifact_url_when_filtered_out() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let stdout = run_info_human(&path, "legacy_rollup", &[]);
    assert!(
        !stdout.contains("artifact_url"),
        "filtered-out human stdout must not contain artifact_url:\n{stdout}"
    );
}

#[test]
fn search_human_output_does_not_contain_artifact_url() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let assert = spawn_index_search(&path, None, &["--output", "human"])
        .success()
        .stderr(predicates::str::is_empty());
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(
        !stdout.contains("artifact_url"),
        "search human output must remain scan-friendly; no artifact_url:\n{stdout}"
    );
}

#[test]
fn index_read_and_parse_failures_emit_typed_json_errors() {
    let td = tempfile::tempdir().unwrap();
    let missing = td.path().join("missing.json");

    assert_json_error(
        spawn_index_search(&missing, None, &["--output", "json"]),
        1,
        "index::index_read_failed",
    );

    let path = index_path(&td);
    std::fs::write(&path, "not valid json").unwrap();
    let parse_doc = assert_json_error(
        spawn_index_info(&path, "downsampler", &["--output", "json"]),
        1,
        "index::index_parse_failed",
    );
    assert!(
        !parse_doc["error"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty(),
        "parse failure must include schema diagnostics: {parse_doc:#}"
    );
}

#[test]
fn index_json_error_absolutizes_relative_index_path() {
    let td = tempfile::tempdir().unwrap();
    let cwd = std::fs::canonicalize(td.path()).unwrap();

    let mut search = cli_cmd();
    let search_doc = assert_json_error(
        search
            .current_dir(&cwd)
            .arg("search")
            .arg("--index")
            .arg("./missing.json")
            .arg("--output")
            .arg("json")
            .assert(),
        1,
        "index::index_read_failed",
    );
    assert_index_read_path_is_absolute(&search_doc);

    let mut info = cli_cmd();
    let info_doc = assert_json_error(
        info.current_dir(&cwd)
            .arg("info")
            .arg("downsampler")
            .arg("--index")
            .arg("./missing.json")
            .arg("--output")
            .arg("json")
            .assert(),
        1,
        "index::index_read_failed",
    );
    assert_index_read_path_is_absolute(&info_doc);
}

fn assert_index_read_path_is_absolute(doc: &serde_json::Value) {
    let field = doc
        .pointer("/error/field")
        .and_then(|v| v.as_str())
        .expect("error.field missing");
    let path = doc
        .pointer("/error/details/path")
        .and_then(|v| v.as_str())
        .expect("error.details.path missing");
    assert_absolute_json_path(field, "error.field");
    assert_absolute_json_path(path, "error.details.path");
}

#[test]
fn usage_errors_emit_exit_two_json_envelopes() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    assert_json_error(
        spawn_index_search(&path, None, &["--trigger-type", "nope", "--output", "json"]),
        2,
        "usage::invalid_value",
    );

    let doc = assert_json_error(
        spawn_index_search(
            &path,
            None,
            &["--database-version", "nope", "--output", "json"],
        ),
        2,
        "usage::invalid_database_version",
    );
    assert_eq!(doc["error"]["details"]["value"], "nope");

    let doc = assert_json_error(
        spawn_index_info(
            &path,
            "downsampler",
            &["--version", "nope", "--output", "json"],
        ),
        2,
        "usage::value_validation",
    );
    assert_eq!(doc["error"]["field"], "--version");

    let doc = assert_json_error(
        spawn_index_info(&path, "7plugin", &["--output", "json"]),
        2,
        "usage::invalid_name",
    );
    assert!(
        doc["error"]["message"]
            .as_str()
            .unwrap()
            .contains("starting with a letter"),
        "invalid-name message should carry the schema rule: {doc:#}"
    );

    let doc = assert_json_error(
        spawn_index_info(&path, "downsampler@1.2.0", &["--output", "json"]),
        2,
        "usage::invalid_name",
    );
    assert_eq!(doc["status"], "error");
}

#[test]
fn invalid_arguments_fail_before_index_read() {
    let td = tempfile::tempdir().unwrap();
    let missing = td.path().join("missing.json");

    let doc = assert_json_error(
        spawn_index_info(
            &missing,
            "downsampler",
            &["--database-version", "nope", "--output", "json"],
        ),
        2,
        "usage::invalid_database_version",
    );
    assert!(
        !doc.to_string().contains("index_read_failed"),
        "usage validation must run before filesystem reads: {doc:#}"
    );
}

#[test]
fn json_output_never_contains_ansi_or_cli_unknown() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let success = {
        let mut cmd = cli_cmd();
        cmd.env("FORCE_COLOR", "1")
            .args(["search", "--output", "json"])
            .arg("--index")
            .arg(&path);
        cmd.assert().success()
    };
    assert!(
        !success
            .get_output()
            .stdout
            .windows(2)
            .any(|w| w == [0x1b, b'['])
    );
    let success_doc = parse_stdout(success.get_output());
    assert_eq!(success_doc["status"], "ok");

    let failure = {
        let mut cmd = cli_cmd();
        cmd.env("FORCE_COLOR", "1")
            .args(["search", "--database-version", "nope", "--output", "json"])
            .arg("--index")
            .arg(&path);
        cmd.assert()
            .failure()
            .code(2)
            .stderr(predicates::str::is_empty())
    };
    assert!(
        !failure
            .get_output()
            .stdout
            .windows(2)
            .any(|w| w == [0x1b, b'['])
    );
    let failure_doc = parse_stdout(failure.get_output());
    assert_eq!(failure_doc["status"], "error");
    assert_eq!(
        failure_doc["error"]["code"],
        "usage::invalid_database_version"
    );
    assert!(
        !failure_doc.to_string().contains(r#""cli::unknown""#),
        "representative index failure must use typed errors: {failure_doc:#}"
    );
}

#[test]
fn index_search_and_info_do_not_modify_input_index() {
    let td = tempfile::tempdir().unwrap();
    let path = index_path(&td);
    write_rich_index(&path);

    let before = std::fs::read_to_string(&path).unwrap();
    spawn_index_search(&path, Some("downsample"), &["--output", "json"]).success();
    spawn_index_info(&path, "downsampler", &["--output", "json"]).success();
    let after = std::fs::read_to_string(&path).unwrap();

    assert_eq!(before, after, "index inspection must be read-only");
}
