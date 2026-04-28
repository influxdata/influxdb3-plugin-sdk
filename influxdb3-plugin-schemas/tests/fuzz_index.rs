//! Bolero fuzz harness for `Index::parse_json`.
//!
//! Runs as a regular `#[test]` in PR CI. In M4 nightly invoked via
//! `cargo +nightly bolero test fuzz_index_parse --time 1800`.
//!
//! Target invariant: `Index::parse_json` must never panic on any
//! input bytes; invalid input must surface as `SchemaErrors`.

#![allow(unused_crate_dependencies)]

use influxdb3_plugin_schemas::Index;

#[test]
fn fuzz_index_parse() {
    bolero::check!()
        .with_type::<Vec<u8>>()
        .for_each(|input| {
            if let Ok(s) = std::str::from_utf8(input) {
                let _ = Index::parse_json(s);
            }
        });
}
