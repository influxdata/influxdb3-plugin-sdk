//! Author-side packaging library for InfluxDB 3 plugins.
//!
//! Library surface wrapped by `influxdb3-plugin-cli`:
//!
//! - [`scaffold`] — generate a plugin or registry directory from a built-in template
//! - [`validate`] — structural + cross-file checks against a plugin directory
//! - [`archive`] — canonical tar.gz construction
//! - [`hash`] — SHA-256 of archive bytes
//! - [`mutate_index`] — add, yank, unyank entries in an existing index
//! - [`package`] — composes the above into a `plugin-dir → (archive, derived_index)` pipeline
//!
//! # Stability
//!
//! Internal crate. Consumers should go through `influxdb3-plugin-cli`'s public API.

// `proptest` is used only by the integration test `tests/archive_determinism.rs`;
// this guard keeps `unused_crate_dependencies` satisfied on the lib test build.
#[cfg(test)]
use filetime as _;
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
