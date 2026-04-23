//! Property: `canonical_tar_gz` is byte-deterministic across two calls on the
//! same inputs — the S2-3 headline contract enforced at the full archive
//! composition layer.
//!
//! # Reproducibility
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
use std::path::{Path, PathBuf};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let base = std::env::temp_dir().join(format!(
            "influxdb3-plugin-sdk-proptest-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        Self(base)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A plugin-directory spec: a list of (relative path, contents) pairs.
/// Path components are short ASCII names; contents are byte vectors.
#[derive(Debug, Clone)]
struct PluginSpec {
    files: Vec<(Vec<String>, Vec<u8>)>,
}

fn arb_component() -> impl Strategy<Value = String> {
    // Short lowercase names, avoiding reserved exclusion tokens.
    proptest::string::string_regex("[a-z][a-z0-9]{0,6}")
        .unwrap()
        .prop_filter("excluded pattern", |s| {
            s != "target" && s != ".git" && s != "__pycache__"
        })
}

fn arb_path() -> impl Strategy<Value = Vec<String>> {
    proptest::collection::vec(arb_component(), 1..=3)
}

fn arb_contents() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(any::<u8>(), 0..=64)
}

fn arb_plugin_spec() -> impl Strategy<Value = PluginSpec> {
    proptest::collection::vec((arb_path(), arb_contents()), 0..=5).prop_map(|files| {
        // De-duplicate by joined path to avoid write collisions in the
        // generator.
        let mut seen = std::collections::HashSet::new();
        let files: Vec<_> = files
            .into_iter()
            .filter(|(components, _)| {
                let joined = components.join("/");
                seen.insert(joined) && !components.iter().any(|c| c.ends_with(".pyc"))
            })
            .collect();
        PluginSpec { files }
    })
}

fn materialize(spec: &PluginSpec, root: &Path) {
    // Every fixture must carry a valid manifest + __init__.py so it's a
    // realistic plugin-dir shape.
    fs::write(
        root.join("manifest.toml"),
        "manifest_schema_version = \"1.0\"\n\n\
         [plugin]\n\
         name = \"p\"\nversion = \"0.1.0\"\n\
         description = \"x\"\n\
         triggers = [\"process_writes\"]\n\n\
         [dependencies]\n\
         database_version = \">=3.0.0\"\n",
    )
    .unwrap();
    fs::write(
        root.join("__init__.py"),
        "def process_writes(a, b, c):\n    pass\n",
    )
    .unwrap();
    for (components, contents) in &spec.files {
        // Skip anything that would collide with the two required files above.
        let joined = components.join("/");
        if joined == "manifest.toml" || joined == "__init__.py" {
            continue;
        }
        let mut target = root.to_path_buf();
        for c in &components[..components.len() - 1] {
            target = target.join(c);
        }
        fs::create_dir_all(&target).unwrap();
        target = target.join(components.last().unwrap());
        fs::write(&target, contents).unwrap();
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
    fn canonical_tar_gz_is_byte_deterministic(spec in arb_plugin_spec()) {
        let td = TempDir::new("det");
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
