//! Schema types for InfluxDB 3 plugin manifests and indexes.
//!
//! This crate defines the canonical Rust types for parsing and serializing
//! plugin manifests (`manifest.toml`), registry indexes (`index.json`), and
//! the `(index_url, name, version)` plugin-identity tuple that ties them
//! together.
//!
//! The crate is consumed by:
//! - `influxdb3-plugin-sdk` — the author-side packaging library
//! - `influxdb3-plugin-cli` — the `influxdb3-plugin` binary's CLI surface
//! - the future database runtime — for install-time manifest parsing and
//!   resolve-time index reads
//!
//! All three consumers depend on this crate through the published crates.io
//! version; schema evolution follows the rules in Spec 1's Schema Versioning
//! Strategy.
