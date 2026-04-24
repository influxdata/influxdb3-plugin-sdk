//! Property: `canonical_tar_gz` is byte-pure across two calls on the *same*
//! materialized directory in the same process. This catches within-process
//! sources of non-determinism (fresh `HashMap`/`HashSet` `RandomState`, etc.).
//!
//! The full reproducibility contract — invariance under perturbation of the
//! listed non-inputs (abs path, source mtime, env vars, locale, UID/GID,
//! executable bit) — is covered in `reproducibility_perturbations.rs`, one
//! dedicated test per non-input.
//!
//! # Reproducibility of proptest failures
//!
//! Matches the `influxdb3-plugin-schemas` crate's `determinism.rs` pattern:
//! proptest's seed is pinned via `PROPTEST_RNG_SEED` and `PROPTEST_RNG_ALGORITHM`
//! env vars, not a constant in this file. CI pins both so the generated
//! test-case sequence is identical across runs and machines:
//!
//! ```bash
//! PROPTEST_RNG_ALGORITHM=chacha \
//! PROPTEST_RNG_SEED=42cafe13374242cafe13374242cafe13374242cafe13374242cafe13374242ca \
//! cargo nextest run -p influxdb3-plugin-sdk --test archive_determinism
//! ```
//!
//! See `validate_smoke.rs` for the rationale behind the crate-root allow.

#![allow(unused_crate_dependencies)]

use influxdb3_plugin_sdk::archive::canonical_tar_gz;
use proptest::prelude::*;
use proptest::test_runner::{Config as ProptestConfig, RngAlgorithm};
use semver::Version;
use std::fs;
use std::path::Path;

mod common;
use common::{VALID_INIT, VALID_MANIFEST};

/// A plugin-directory spec: a list of (flat filename, contents) pairs.
///
/// Flat-path shape avoids the class of fs::write panics that occur when a
/// generated path at `a/b` conflicts with a later generated file at `a`
/// (would require `a` to be both a file and a directory). Flat paths are
/// sufficient to exercise the determinism guarantee — the canonicalization
/// rules (sort order, per-entry header fields, gzip framing) are oblivious
/// to directory depth.
#[derive(Debug, Clone)]
struct PluginSpec {
    files: Vec<(String, Vec<u8>)>,
}

fn arb_filename() -> impl Strategy<Value = String> {
    // Short lowercase names with `.py` suffix. After the suffix, the output
    // can't collide with directory-exclusion tokens (`target`, `.git`,
    // `__pycache__` — which match path components, not filenames) or with
    // the required fixture files (`manifest.toml` / `__init__.py` —
    // different extension), so no post-map filters are needed.
    proptest::string::string_regex("[a-z][a-z0-9]{0,6}")
        .unwrap()
        .prop_map(|s| format!("{s}.py"))
}

fn arb_contents() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(any::<u8>(), 0..=64)
}

fn arb_plugin_spec() -> impl Strategy<Value = PluginSpec> {
    proptest::collection::vec((arb_filename(), arb_contents()), 0..=5).prop_map(|files| {
        // De-duplicate by filename: repeats would cause the second fs::write
        // to overwrite the first and change the final directory state
        // depending on Vec ordering.
        let mut seen = std::collections::HashSet::new();
        let files: Vec<_> = files
            .into_iter()
            .filter(|(name, _)| seen.insert(name.clone()))
            .collect();
        PluginSpec { files }
    })
}

fn materialize(spec: &PluginSpec, root: &Path) {
    // Every fixture must carry a valid manifest + __init__.py so it's a
    // realistic plugin-dir shape.
    fs::write(root.join("manifest.toml"), VALID_MANIFEST).unwrap();
    fs::write(root.join("__init__.py"), VALID_INIT).unwrap();
    for (name, contents) in &spec.files {
        fs::write(root.join(name), contents).unwrap();
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 100,
        rng_algorithm: RngAlgorithm::ChaCha,
        // No .proptest-regressions file. Same rationale as the schemas crate's
        // determinism.rs: the assertion is byte-equality of two immediate calls
        // on the same input, so failures reproduce from the seed alone.
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn canonical_tar_gz_is_byte_pure_across_two_calls(spec in arb_plugin_spec()) {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("plugin");
        fs::create_dir_all(&dir).unwrap();
        materialize(&spec, &dir);

        let name: influxdb3_plugin_schemas::PluginName = "p".parse().unwrap();
        let version = Version::new(0, 1, 0);
        let a = canonical_tar_gz(&dir, &name, &version).unwrap();
        let b = canonical_tar_gz(&dir, &name, &version).unwrap();
        prop_assert_eq!(a, b);
    }
}
