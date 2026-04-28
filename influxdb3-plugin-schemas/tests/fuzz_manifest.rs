//! Bolero fuzz harness for `Manifest::parse_toml`.
//!
//! Runs as a regular `#[test]` in PR CI (random property mode, very fast).
//! In the M4 nightly workflow, the same `#[test]` is invoked via
//! `cargo +nightly bolero test fuzz_manifest_parse --time 1800` to run
//! coverage-guided fuzzing under libfuzzer for 30 minutes.
//!
//! Target invariant: `Manifest::parse_toml` must never panic on any
//! input bytes. It is allowed (and expected) to return errors via
//! `SchemaErrors` for invalid input.

#![allow(unused_crate_dependencies)]

use influxdb3_plugin_schemas::Manifest;

#[test]
fn fuzz_manifest_parse() {
    bolero::check!()
        .with_type::<Vec<u8>>()
        .for_each(|input| {
            // parse_toml takes &str; handle non-UTF8 by skipping (errors
            // from invalid UTF-8 are not the parser's concern).
            if let Ok(s) = std::str::from_utf8(input) {
                let _ = Manifest::parse_toml(s);
            }
        });
}
