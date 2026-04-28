//! CLI-layer error classification.
//!
//! Spec § 4.6 / § 4.7. Every error in JSON mode carries a structured
//! `JsonError` payload; `CliError` just adds the Usage-vs-Runtime
//! exit-code classification on top.

use crate::output::json::JsonError;

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("{}", _0.message)]
    Usage(Box<JsonError>),

    #[error("{}", _0.message)]
    Runtime(Box<JsonError>),
}

impl CliError {
    pub(crate) fn usage(je: JsonError) -> anyhow::Error {
        CliError::Usage(Box::new(je)).into()
    }

    pub(crate) fn runtime(je: JsonError) -> anyhow::Error {
        CliError::Runtime(Box::new(je)).into()
    }

    /// Temporary bridge: wraps a plain message string in a `cli::unknown`
    /// JsonError. Callers will be migrated to construct proper JsonError
    /// with specific wire codes in Chunks 5-8.
    pub(crate) fn usage_msg(msg: impl Into<String>) -> anyhow::Error {
        Self::usage(JsonError {
            code: "cli::unknown".into(),
            message: msg.into(),
            field: None,
            details: None,
            diagnostics: vec![],
            cause: vec![],
        })
    }

    /// Temporary bridge for runtime errors from plain messages.
    pub(crate) fn runtime_msg(msg: impl Into<String>) -> anyhow::Error {
        Self::runtime(JsonError {
            code: "cli::unknown".into(),
            message: msg.into(),
            field: None,
            details: None,
            diagnostics: vec![],
            cause: vec![],
        })
    }

    /// Runtime error whose output has already been written to stdout.
    /// `main.rs` detects this code and skips writing another envelope.
    /// Used by `validate --output json` where the diagnostics document
    /// is the primary output.
    pub(crate) fn runtime_silent(msg: impl Into<String>) -> anyhow::Error {
        Self::runtime(JsonError {
            code: "cli::output_already_written".into(),
            message: msg.into(),
            field: None,
            details: None,
            diagnostics: vec![],
            cause: vec![],
        })
    }

    pub fn json_error_of(e: &anyhow::Error) -> Option<&JsonError> {
        match e.downcast_ref::<CliError>() {
            Some(CliError::Usage(je) | CliError::Runtime(je)) => Some(je),
            None => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliErrorKind {
    Runtime,
    Usage,
}

impl CliErrorKind {
    pub fn of(e: &anyhow::Error) -> Self {
        match e.downcast_ref::<CliError>() {
            Some(CliError::Usage(_)) => CliErrorKind::Usage,
            Some(CliError::Runtime(_)) => CliErrorKind::Runtime,
            None => CliErrorKind::Runtime,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::json::JsonError;

    fn dummy_je(code: &str) -> JsonError {
        JsonError {
            code: code.into(),
            message: "m".into(),
            field: None,
            details: None,
            diagnostics: vec![],
            cause: vec![],
        }
    }

    #[test]
    fn usage_carries_json_error() {
        let e: anyhow::Error = CliError::usage(dummy_je("usage::missing_required_argument"));
        let kind = CliErrorKind::of(&e);
        assert_eq!(kind, CliErrorKind::Usage);
        let je = CliError::json_error_of(&e).expect("downcast yields JsonError");
        assert_eq!(je.code, "usage::missing_required_argument");
    }

    #[test]
    fn runtime_carries_json_error() {
        let e: anyhow::Error = CliError::runtime(dummy_je("package::canonical_collision"));
        assert_eq!(CliErrorKind::of(&e), CliErrorKind::Runtime);
        let je = CliError::json_error_of(&e).expect("downcast yields JsonError");
        assert_eq!(je.code, "package::canonical_collision");
    }

    #[test]
    fn plain_anyhow_is_runtime_with_no_json_error() {
        let e: anyhow::Error = anyhow::anyhow!("plain");
        assert_eq!(CliErrorKind::of(&e), CliErrorKind::Runtime);
        assert!(CliError::json_error_of(&e).is_none());
    }

    #[test]
    fn usage_msg_bridge_creates_cli_unknown() {
        let e = CliError::usage_msg("test message");
        let je = CliError::json_error_of(&e).unwrap();
        assert_eq!(je.code, "cli::unknown");
        assert_eq!(je.message, "test message");
    }
}
