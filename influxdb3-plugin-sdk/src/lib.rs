//! Author-side packaging library for InfluxDB 3 plugins.
//!
//! The crate implements the library surface that `influxdb3-plugin-cli`
//! (Plan 3) will wrap in user-facing subcommands:
//!
//! - [`scaffold`] — generate a plugin directory or registry directory from a
//!   built-in template
//! - [`validate`] — structural + cross-file checks against a plugin directory
//! - [`archive`] — canonical tar.gz construction per Spec 2 Reproducibility
//! - [`hash`] — SHA-256 of archive bytes
//! - [`mutate_index`] — add, yank, unyank entries in an existing index
//! - [`package`] — composes the above into a single `plugin-dir → (archive,
//!   derived_index)` pipeline
//!
//! # Stability
//!
//! This crate is internal per the plugin SDK's Spec 2 Stability policy.
//! Consumers should go through `influxdb3-plugin-cli`'s public API.
//! Refactoring freedom is the goal.

// `proptest` is used only in the `tests/archive_determinism.rs` integration
// test, not in any inline `#[cfg(test)]` module. The lib crate's test target
// still sees it as a declared dev-dep, so this guard keeps
// `unused_crate_dependencies` satisfied on the lib test build. Same pattern
// as the schemas crate.
#[cfg(test)]
use proptest as _;

mod error;

pub mod archive;
pub mod hash;
pub mod mutate_index;
pub mod package;
pub mod scaffold;
pub mod validate;

pub use error::{SdkError, ValidationError, ValidationReport};
