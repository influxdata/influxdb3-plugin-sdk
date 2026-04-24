//! CLI-layer error classification.
//!
//! `anyhow::Error` is the single return type;
//! this type attaches semantic tags (`Usage`, `Silent`) so `main.rs` can
//! pick the right exit code and the right stderr discipline
//! without breaking the stable embedding surface.
//!
//! - [`CliError::Usage`] — the command was invoked incorrectly (bad
//!   `--name` value, flag/template mismatch, aliasing, malformed
//!   `<name>@<version>` target). Maps to exit code 2.
//! - [`CliError::Silent`] — the command failed, but stdout has already
//!   carried the primary signal (e.g. the `diagnostics` array in
//!   `validate --output json`). `main.rs` suppresses the stderr
//!   `eprintln!`; exit code stays 1.
//!
//! Plain `anyhow::Error` — no wrapper — is the runtime-failure default;
//! `main.rs` renders it on stderr and exits 1.

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// Command invoked incorrectly. Renders on stderr; exit 2.
    #[error(transparent)]
    Usage(anyhow::Error),

    /// Command failed but stdout already signaled it (e.g. `validate`
    /// JSON mode). Stderr silent; exit 1.
    ///
    /// Note: no `#[source]` — `thiserror` 2.x rejects
    /// `#[error(transparent)] + #[source]` at macro-expand time with
    /// `"transparent variant can't contain #[source]"`. `transparent`
    /// already wires `Error::source` to the single field.
    #[error(transparent)]
    Silent(anyhow::Error),
}

impl CliError {
    /// Converts a plain `anyhow!`-style message into `CliError::Usage`.
    /// Call sites: usage-class `bail!`/`anyhow!` points in the command
    /// modules. Shortens noise at each call site.
    pub(crate) fn usage(e: impl Into<anyhow::Error>) -> anyhow::Error {
        CliError::Usage(e.into()).into()
    }

    /// Wraps an already-constructed `anyhow::Error` as a silent failure.
    /// Used by `validate --output json` when the diagnostics document
    /// has been written and no stderr summary should follow.
    pub(crate) fn silent(e: impl Into<anyhow::Error>) -> anyhow::Error {
        CliError::Silent(e.into()).into()
    }
}

/// Classification an error maps to for exit-code and stderr handling.
///
/// Constructed from `anyhow::Error` via [`CliErrorKind::of`], which
/// downcasts to [`CliError`] and falls back to `Runtime`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliErrorKind {
    Runtime,
    Usage,
    Silent,
}

impl CliErrorKind {
    pub fn of(e: &anyhow::Error) -> Self {
        match e.downcast_ref::<CliError>() {
            Some(CliError::Usage(_)) => CliErrorKind::Usage,
            Some(CliError::Silent(_)) => CliErrorKind::Silent,
            None => CliErrorKind::Runtime,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

    #[test]
    fn usage_wraps_anyhow_and_is_downcastable() {
        let e: anyhow::Error = CliError::Usage(anyhow!("bad flag")).into();
        assert!(matches!(
            e.downcast_ref::<CliError>(),
            Some(CliError::Usage(_))
        ));
    }

    #[test]
    fn silent_wraps_anyhow_and_is_downcastable() {
        let e: anyhow::Error = CliError::Silent(anyhow!("quiet fail")).into();
        assert!(matches!(
            e.downcast_ref::<CliError>(),
            Some(CliError::Silent(_))
        ));
    }

    #[test]
    fn kind_of_plain_anyhow_is_runtime() {
        let e: anyhow::Error = anyhow!("plain runtime");
        assert_eq!(CliErrorKind::of(&e), CliErrorKind::Runtime);
    }

    #[test]
    fn kind_of_usage_is_usage() {
        let e: anyhow::Error = CliError::Usage(anyhow!("u")).into();
        assert_eq!(CliErrorKind::of(&e), CliErrorKind::Usage);
    }

    #[test]
    fn kind_of_silent_is_silent() {
        let e: anyhow::Error = CliError::Silent(anyhow!("s")).into();
        assert_eq!(CliErrorKind::of(&e), CliErrorKind::Silent);
    }

    #[test]
    fn display_of_usage_delegates_to_inner() {
        let e = CliError::Usage(anyhow!("bad --name \"X\""));
        assert_eq!(e.to_string(), "bad --name \"X\"");
    }
}
