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
}
