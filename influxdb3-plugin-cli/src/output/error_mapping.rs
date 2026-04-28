//! Maps SDK / clap errors into the wire-stable `JsonError` shape per
//! spec § 4.6 / § 4.5. CLI-owned, decoupled from internal Rust type
//! names so SDK refactors don't break the wire.

#[allow(unused_imports)]
use crate::output::json::JsonError;

/// Identifies the calling command so the error mapper can pick the
/// correct namespace for variants whose code dispatches by call site
/// (`SdkError::Io`, `SdkError::Archive`, `SdkError::PathOverlap`).
/// Spec § 4.6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorContext {
    Validate,
    Package,
    Yank,
    NewPlugin,
    NewRegistry,
    NewList,
    /// Top-level / clap-parse / pre-dispatch path. Used only for the
    /// `cli::unknown` safety fallback in main.rs.
    Cli,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_context_variants_are_distinct() {
        let all = [
            ErrorContext::Validate,
            ErrorContext::Package,
            ErrorContext::Yank,
            ErrorContext::NewPlugin,
            ErrorContext::NewRegistry,
            ErrorContext::NewList,
            ErrorContext::Cli,
        ];
        for (i, a) in all.iter().enumerate() {
            for (j, b) in all.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }
}
