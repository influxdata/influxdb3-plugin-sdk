//! Property: `Index` canonicalization respects equivalence.
//!
//! Asserts that equivalent JSON inputs (different formatting, key order,
//! whitespace) produce byte-identical canonical output.
//!
//! Strategy: generate one `Index`, serialize via TWO different routes
//! (canonical + a noise-perturbed JSON via `BTreeMap` key reordering),
//! parse both, re-canonicalize both, assert equal.
//!
//! ## Reproducibility
//!
//! See `index_canon_idempotent.rs` for the rationale on env-driven RNG
//! configuration and `parse_fixtures.rs` for the crate-root allow.

#![allow(unused_crate_dependencies)]

use std::collections::BTreeMap;

use influxdb3_plugin_schemas::{
    ArtifactHash, ArtifactsUrl, Dependencies, Description, Index, IndexEntry, IndexSchemaVersion,
    PluginName, TriggerType,
};
use proptest::prelude::*;
use proptest::test_runner::{Config as ProptestConfig, RngAlgorithm};

fn arb_name() -> impl Strategy<Value = PluginName> {
    proptest::string::string_regex("[a-z][a-z0-9-]{0,16}")
        .unwrap()
        .prop_filter_map("invalid name", |s| s.parse().ok())
}

fn arb_version() -> impl Strategy<Value = semver::Version> {
    (0u64..10, 0u64..20, 0u64..20).prop_map(|(a, b, c)| semver::Version::new(a, b, c))
}

fn arb_entry() -> impl Strategy<Value = IndexEntry> {
    (arb_name(), arb_version(), any::<bool>()).prop_map(|(name, version, yanked)| IndexEntry {
        name,
        version,
        description: Description::try_new("desc").unwrap(),
        triggers: vec![TriggerType::ProcessWrites],
        homepage: None,
        repository: None,
        documentation: None,
        dependencies: Dependencies {
            database_version: ">=3.0.0".parse().unwrap(),
            python: vec![],
        },
        hash: ArtifactHash::try_new(
            "sha256:0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap(),
        yanked,
    })
}

fn arb_index() -> impl Strategy<Value = Index> {
    proptest::collection::vec(arb_entry(), 0..=8).prop_map(|mut plugins| {
        // Dedupe by (name, version): Index::parse_json rejects
        // duplicates, so duplicates would corrupt this property's
        // shrunk counterexamples.
        let mut seen = std::collections::HashSet::new();
        plugins.retain(|e| seen.insert((e.name.clone(), e.version.clone())));
        Index {
            index_schema_version: IndexSchemaVersion::new(1, 0),
            artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
            plugins,
        }
    })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 200,
        rng_algorithm: RngAlgorithm::ChaCha,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn index_canon_respects_equivalence(idx in arb_index()) {
        let direct = idx.to_canonical_json().expect("direct canon");

        // Round-trip via serde_json::Value to perturb formatting.
        // Then convert to BTreeMap to also perturb key ORDER.
        let value: serde_json::Value = serde_json::from_str(&direct).expect("to value");
        let sorted: BTreeMap<String, serde_json::Value> =
            value.as_object().expect("top-level is object")
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
        let perturbed = serde_json::to_string_pretty(&sorted).expect("perturb");

        let reparsed = Index::parse_json(&perturbed).expect("re-parse perturbed");
        let canon = reparsed.to_canonical_json().expect("re-canon");
        prop_assert_eq!(direct, canon);
    }
}
