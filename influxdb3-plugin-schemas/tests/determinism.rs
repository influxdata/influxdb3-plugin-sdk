//! Property: `to_canonical_json` is deterministic for any valid `Index`.
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
//! cargo nextest run -p influxdb3-plugin-schemas --test determinism
//! ```
//!
//! The seed value above is arbitrary — any 64-hex value works. Pin it in CI
//! and in the `.proptest-regressions` file that proptest emits on failure. We
//! intentionally do NOT try to seed from a `const` in this file: proptest
//! 1.x does not expose a supported public API for threading a `TestRng` into
//! the runner constructed by the `proptest!` macro, and pretending to seed in
//! code when we don't would be worse than being explicit about the env-var
//! mechanism.
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
    // Note: `arb_index` may emit duplicate (name, version) tuples. Because the
    // determinism test never re-parses the canonical output, the
    // `Index::validate` duplicate-rejection path is not exercised here — safe.
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
    proptest::collection::vec(arb_entry(), 0..=8).prop_map(|plugins| Index {
        index_schema_version: IndexSchemaVersion::new(1, 0),
        artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
        plugins,
    })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        rng_algorithm: RngAlgorithm::ChaCha,
        // No .proptest-regressions file. Reproducibility for this specific test
        // comes from PROPTEST_RNG_SEED (see module rustdoc). The test asserts
        // equality of two immediate calls on the same value, so any failure
        // reproduces from the seed alone — no persisted shrink state needed.
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn canonical_serialization_is_byte_deterministic(idx in arb_index()) {
        let a = idx.to_canonical_json().unwrap();
        let b = idx.to_canonical_json().unwrap();
        prop_assert_eq!(a, b);
    }
}
