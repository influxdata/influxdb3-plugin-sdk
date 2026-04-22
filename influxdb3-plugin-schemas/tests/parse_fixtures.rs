//! Integration tests: walk fixture directories and assert expected parse outcomes.
//!
//! Integration-test files compile as their own crate and see every dep of the
//! parent crate's `[dependencies]` + `[dev-dependencies]`. The harness below
//! only needs `influxdb3-plugin-schemas` + stdlib, so the rest trip the
//! workspace `unused_crate_dependencies = "deny"` lint. Allowing at the
//! crate-root level is the documented escape for integration tests.

#![allow(unused_crate_dependencies)]

use influxdb3_plugin_schemas::{Index, Manifest};
use std::fs;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn all_valid_manifests_parse() {
    let dir = fixtures_dir().join("valid");
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let contents = fs::read_to_string(&path).unwrap();
        Manifest::parse_toml(&contents).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    }
}

#[test]
fn all_valid_indexes_parse() {
    let dir = fixtures_dir().join("valid");
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let contents = fs::read_to_string(&path).unwrap();
        Index::parse_json(&contents).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    }
}

#[test]
fn all_invalid_manifests_fail_to_parse() {
    let dir = fixtures_dir().join("invalid");
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy();
        if !name.starts_with("manifest_")
            || path.extension().and_then(|e| e.to_str()) != Some("toml")
        {
            continue;
        }
        let contents = fs::read_to_string(&path).unwrap();
        assert!(
            Manifest::parse_toml(&contents).is_err(),
            "{} was expected to fail parse but succeeded",
            path.display()
        );
    }
}

#[test]
fn all_invalid_indexes_fail_to_parse() {
    let dir = fixtures_dir().join("invalid");
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy();
        if !name.starts_with("index_") || path.extension().and_then(|e| e.to_str()) != Some("json")
        {
            continue;
        }
        let contents = fs::read_to_string(&path).unwrap();
        assert!(
            Index::parse_json(&contents).is_err(),
            "{} was expected to fail parse but succeeded",
            path.display()
        );
    }
}
