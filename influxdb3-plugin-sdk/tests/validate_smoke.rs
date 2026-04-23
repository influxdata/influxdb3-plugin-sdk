//! Integration tests for `sdk::validate::plugin_dir` against fixture plugin
//! directories.
//!
//! Each fixture under `tests/fixtures/invalid_plugins/` is a plugin directory
//! that trips exactly one validation rule; the happy-path `valid_plugin/`
//! fixture verifies the positive case end-to-end.
//!
//! See `parse_fixtures.rs` (schemas crate) for the rationale behind the
//! crate-root allow.

#![allow(unused_crate_dependencies)]

use influxdb3_plugin_schemas::TriggerType;
use influxdb3_plugin_sdk::{SdkError, ValidationError, validate};
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn valid_plugin_passes() {
    validate::plugin_dir(&fixtures().join("valid_plugin"))
        .expect("valid_plugin fixture should pass validation");
}

#[test]
fn missing_init_reports_missing_required_file() {
    let err = validate::plugin_dir(&fixtures().join("invalid_plugins/missing_init")).unwrap_err();
    match err {
        SdkError::ValidationErrors(errs) => {
            assert_eq!(errs.len(), 1);
            assert!(matches!(
                errs[0],
                ValidationError::MissingRequiredFile { .. }
            ));
        }
        other => panic!("expected ValidationErrors(MissingRequiredFile), got {other:?}"),
    }
}

/// Spec 2 Validation groups `manifest.toml` and `__init__.py` under the same
/// "required files exist" rule. Both missing-file cases must surface as
/// `ValidationError::MissingRequiredFile`, not as a raw `SdkError::Io`.
#[test]
fn missing_manifest_reports_missing_required_file() {
    let err =
        validate::plugin_dir(&fixtures().join("invalid_plugins/missing_manifest")).unwrap_err();
    match err {
        SdkError::ValidationErrors(errs) => {
            assert_eq!(errs.len(), 1);
            match &errs[0] {
                ValidationError::MissingRequiredFile { file } => {
                    assert_eq!(file, "manifest.toml");
                }
                other => panic!("expected MissingRequiredFile(manifest.toml), got {other:?}"),
            }
        }
        other => panic!("expected ValidationErrors(MissingRequiredFile), got {other:?}"),
    }
}

#[test]
fn missing_trigger_impl_reports_trigger_not_implemented() {
    let err =
        validate::plugin_dir(&fixtures().join("invalid_plugins/missing_trigger_impl")).unwrap_err();
    match err {
        SdkError::ValidationErrors(errs) => {
            assert_eq!(errs.len(), 1);
            assert!(matches!(
                errs[0],
                ValidationError::TriggerNotImplemented {
                    trigger: TriggerType::ProcessWrites
                }
            ));
        }
        other => panic!("expected TriggerNotImplemented, got {other:?}"),
    }
}

#[test]
fn async_trigger_reports_async_trigger_fn() {
    let err = validate::plugin_dir(&fixtures().join("invalid_plugins/async_trigger")).unwrap_err();
    match err {
        SdkError::ValidationErrors(errs) => {
            assert_eq!(errs.len(), 1);
            assert!(matches!(
                errs[0],
                ValidationError::AsyncTriggerFn {
                    trigger: TriggerType::ProcessWrites
                }
            ));
        }
        other => panic!("expected AsyncTriggerFn, got {other:?}"),
    }
}

#[test]
fn bad_python_syntax_reports_python_parse() {
    let err =
        validate::plugin_dir(&fixtures().join("invalid_plugins/bad_python_syntax")).unwrap_err();
    match err {
        SdkError::ValidationErrors(errs) => {
            assert_eq!(errs.len(), 1);
            assert!(matches!(errs[0], ValidationError::PythonParse { .. }));
        }
        other => panic!("expected PythonParse, got {other:?}"),
    }
}
