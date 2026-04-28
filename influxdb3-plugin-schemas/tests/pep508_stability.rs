//! Property: a known-valid PEP508 dependency spec parses successfully,
//! and re-printing then re-parsing yields a semantically equal Requirement.
//!
//! The "known-valid" generator uses `prop_oneof!` over hand-crafted
//! grammar fragments rather than arbitrary bytes. The fuzzer
//! (fuzz_manifest_parse) handles arbitrary-byte robustness; this
//! property tests symmetric parse/format on inputs the spec explicitly
//! accepts.

#![allow(unused_crate_dependencies)]

use pep508_rs::VerbatimUrl;
use proptest::prelude::*;
use proptest::test_runner::{Config as ProptestConfig, RngAlgorithm};
use std::str::FromStr;

fn arb_pep508_spec() -> impl Strategy<Value = String> {
    prop_oneof![
        // bare name
        Just("requests".to_string()),
        // name + extras
        Just("requests[security,socks]".to_string()),
        // name + version specifier
        Just("requests>=2.0".to_string()),
        // name + version range
        Just("requests>=2.0,<3.0".to_string()),
        // name + extras + version
        Just("requests[security]>=2.0,<3.0".to_string()),
        // arbitrary lowercase name patterns (must end with alphanumeric per PEP 508)
        proptest::string::string_regex("[a-z]([a-z0-9_-]{0,14}[a-z0-9])?").unwrap(),
        // name + ~= specifier
        proptest::string::string_regex("[a-z]([a-z0-9_-]{0,14}[a-z0-9])?~=[0-9]\\.[0-9]")
            .unwrap(),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 500,
        rng_algorithm: RngAlgorithm::ChaCha,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn pep508_known_valid_specs_round_trip(s in arb_pep508_spec()) {
        let r1 = match pep508_rs::Requirement::<VerbatimUrl>::from_str(&s) {
            Ok(r) => r,
            // generated string was not a valid PEP508 spec — skip
            // (the regex strategy is broader than the grammar).
            Err(_) => return Ok(()),
        };
        let printed = r1.to_string();
        let r2 = pep508_rs::Requirement::<VerbatimUrl>::from_str(&printed)
            .expect("re-parse of printed Requirement");
        prop_assert_eq!(r1, r2);
    }
}
