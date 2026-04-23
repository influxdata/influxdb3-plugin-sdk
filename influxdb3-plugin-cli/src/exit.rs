//! Exit-code contract per Spec 2 § S2-18.
//!
//! - `0` — success.
//! - `1` — runtime failure (`Result::Err` from [`crate::PluginConfig::run`]).
//! - `2` — usage error. clap emits this automatically on parse-time
//!   failures; command code returns the same code for argument-shape
//!   errors caught after parse.
//!
//! No SDK code calls `std::process::exit` per Spec 2 § S2-7 — `main.rs`
//! returns [`std::process::ExitCode`]; embedding hosts return through
//! `Result`.

use std::process::ExitCode;

/// Exit code 0 — operation completed successfully.
#[allow(dead_code)] // referenced by D34 cross-cutting harness
pub(crate) const SUCCESS: u8 = 0;

/// Exit code 1 — runtime failure (validation, I/O, immutability collision,
/// parse error, internal invariant break, etc.).
#[allow(dead_code)] // referenced by D34 cross-cutting harness
pub(crate) const RUNTIME_FAILURE: u8 = 1;

/// Exit code 2 — usage error (unknown flag, missing required arg, invalid
/// `--output` value, etc.). clap emits this on parse-time failures.
#[allow(dead_code)] // referenced by D34 cross-cutting harness
pub(crate) const USAGE_ERROR: u8 = 2;

/// Maps a `Result` to the SDK's success/runtime-failure exit codes.
///
/// Does not consume / inspect the `Err` payload; callers needing to
/// render the error to stderr handle that separately before exiting.
#[allow(dead_code)] // referenced by D34 cross-cutting harness
pub(crate) fn exit_code_from_result<T, E>(result: Result<T, E>) -> ExitCode {
    match result {
        Ok(_) => ExitCode::from(SUCCESS),
        Err(_) => ExitCode::from(RUNTIME_FAILURE),
    }
}
