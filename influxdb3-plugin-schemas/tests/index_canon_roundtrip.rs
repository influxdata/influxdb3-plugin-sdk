//! Property: `Index` canonical JSON round-trips.
//!
//! Asserts that `parse_json(canon(idx)) == idx` — parsing the canonical JSON of
//! an `Index` yields a value equal to the original.
//!
//! ## Reproducibility
//!
//! Reproducibility of proptest failures is achieved via the `PROPTEST_RNG_SEED`
//! and `PROPTEST_RNG_ALGORITHM` env vars, not a constant in this file. CI pins
//! both via shell env so the sequence of generated test cases is identical
//! across runs and machines:
//!
//! ```bash
//! PROPTEST_RNG_ALGORITHM=chacha \
//! PROPTEST_RNG_SEED=42cafe13374242cafe13374242cafe13374242cafe13374242cafe13374242ca \
//! cargo nextest run -p influxdb3-plugin-schemas --test index_canon_roundtrip
//! ```
//!
//! See `parse_fixtures.rs` for the rationale behind the crate-root allow.

#![allow(unused_crate_dependencies)]

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
        // Sort into canonical order (name asc, then version asc by SemVer
        // precedence) so that `parse_json(canon(idx)) == idx` holds — the
        // canonical JSON always emits entries in this order.
        plugins.sort_by(|a, b| {
            a.name
                .as_str()
                .cmp(b.name.as_str())
                .then_with(|| a.version.cmp_precedence(&b.version))
        });
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
    fn index_canon_roundtrip(idx in arb_index()) {
        let canon = idx.to_canonical_json().expect("canon");
        let parsed = Index::parse_json(&canon).expect("parse");
        prop_assert_eq!(idx, parsed);
    }
}
