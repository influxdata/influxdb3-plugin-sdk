//! Command implementations.
//!
//! Each submodule wraps one SDK function and adds the CLI-specific
//! plumbing — argument parsing, path resolution, output-mode rendering.
//! Per Spec 2 § Commands.

pub(crate) mod new;
