//! Integration tests: walk fixture directories and assert expected parse outcomes
//! with per-fixture snapshots of the rendered error output.
//!
//! Integration-test files compile as their own crate and see every dep of the
//! parent crate's `[dependencies]` + `[dev-dependencies]`. The harness below
//! only needs `influxdb3-plugin-schemas` + `insta` + stdlib, so the rest trip
//! the workspace `unused_crate_dependencies = "deny"` lint. Allowing at the
//! crate-root level is the documented escape for integration tests.

#![allow(unused_crate_dependencies)]

use influxdb3_plugin_schemas::{Index, Manifest};
use std::fs;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixture_pairs(subdir: &str, ext: &str, name_prefix_filter: Option<&str>) -> Vec<(String, String)> {
    let dir = fixtures_dir().join(subdir);
    let mut out: Vec<(String, String)> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension().and_then(|e| e.to_str()) != Some(ext) {
                return None;
            }
            let name = path.file_name()?.to_string_lossy().into_owned();
            if let Some(prefix) = name_prefix_filter
                && !name.starts_with(prefix)
            {
                return None;
            }
            let contents = fs::read_to_string(&path).ok()?;
            Some((name, contents))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn all_valid_manifests_parse_and_fields_match_snapshot() {
    let pairs = fixture_pairs("valid", "toml", None);
    assert!(!pairs.is_empty(), "no valid manifest fixtures found");
    for (name, contents) in pairs {
        let manifest = Manifest::parse_toml(&contents)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        insta::assert_debug_snapshot!(format!("valid_manifest_{}", name), manifest);
    }
}

#[test]
fn all_valid_indexes_parse_and_fields_match_snapshot() {
    let pairs = fixture_pairs("valid", "json", None);
    assert!(!pairs.is_empty(), "no valid index fixtures found");
    for (name, contents) in pairs {
        let index = Index::parse_json(&contents)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        insta::assert_debug_snapshot!(format!("valid_index_{}", name), index);
    }
}

#[test]
fn all_invalid_manifests_report_expected_errors() {
    let pairs = fixture_pairs("invalid", "toml", Some("manifest_"));
    assert!(!pairs.is_empty(), "no invalid manifest fixtures found");
    for (name, contents) in pairs {
        let errors = Manifest::parse_toml(&contents)
            .expect_err(&format!("{name}: was expected to fail parse"));
        insta::assert_snapshot!(format!("invalid_manifest_{}", name), errors.to_string());
    }
}

#[test]
fn all_invalid_indexes_report_expected_errors() {
    let pairs = fixture_pairs("invalid", "json", Some("index_"));
    assert!(!pairs.is_empty(), "no invalid index fixtures found");
    for (name, contents) in pairs {
        let errors = Index::parse_json(&contents)
            .expect_err(&format!("{name}: was expected to fail parse"));
        insta::assert_snapshot!(format!("invalid_index_{}", name), errors.to_string());
    }
}
