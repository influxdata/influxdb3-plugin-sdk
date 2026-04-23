//! Round-trip property: for every valid index fixture, canonicalizing the
//! parsed index should be idempotent — `canonical(parse(x)) ==
//! canonical(parse(canonical(parse(x))))`. This is a strictly stronger
//! property than comparing `parse(x) == parse(canonical(parse(x)))` because it
//! is immune to input ordering (if a fixture's `plugins[]` is not yet in
//! canonical order, the first `parse` preserves input order, the canonical
//! output sorts it, and structural equality on the parsed values would fail
//! even though the intent — "canonical form is stable" — holds).
//!
//! See `parse_fixtures.rs` for the rationale behind the crate-root allow.

#![allow(unused_crate_dependencies)]

use influxdb3_plugin_schemas::Index;
use std::fs;
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/valid")
}

#[test]
fn canonical_form_is_idempotent() {
    let mut processed = 0;
    for entry in fs::read_dir(fixtures()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let raw = fs::read_to_string(&path).unwrap();
        let once = Index::parse_json(&raw)
            .unwrap()
            .to_canonical_json()
            .unwrap();
        let twice = Index::parse_json(&once)
            .unwrap()
            .to_canonical_json()
            .unwrap();
        assert_eq!(once, twice, "canonical form changed on second pass for {}", path.display());
        processed += 1;
    }
    assert!(processed >= 1, "no valid index fixtures found at {}", fixtures().display());
}
