//! Command implementations.
//!
//! Each submodule wraps one SDK function and adds the CLI-specific
//! plumbing — argument parsing, path resolution, output-mode rendering.
//! Per Spec 2 § Commands.

pub(crate) mod new;
pub(crate) mod package;
pub(crate) mod validate;
pub(crate) mod yank;
