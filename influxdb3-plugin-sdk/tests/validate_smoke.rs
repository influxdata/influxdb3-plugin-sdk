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

mod common;
use common::empty_index;

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

/// `manifest.toml` and `__init__.py` share the same "required files exist"
/// rule. Both missing-file cases must surface as
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

/// `validate::plugin_dir_with_index` runs the same checks as `plugin_dir`
/// plus a uniqueness check against the supplied index. Collisions surface
/// as `ValidationError::NameVersionConflict` — the collectible variant the
/// CLI's `validate --index` flag uses to render uniqueness failures in the
/// same diagnostics array as other validation errors.
#[test]
fn validate_with_index_reports_name_version_conflict() {
    use influxdb3_plugin_schemas::{
        ArtifactHash, ArtifactsUrl, Dependencies, Description, Index, IndexEntry,
        IndexSchemaVersion, TriggerType,
    };

    let plugin_dir = fixtures().join("valid_plugin");
    // valid_plugin's manifest declares name="valid-plugin", version="0.1.0".
    // Construct an index that already contains that (name, version).
    let index = Index {
        index_schema_version: IndexSchemaVersion::new(1, 0),
        artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
        plugins: vec![IndexEntry {
            name: "valid-plugin".parse().unwrap(),
            version: semver::Version::new(0, 1, 0),
            description: Description::try_new("preexisting").unwrap(),
            triggers: vec![TriggerType::ProcessWrites],
            homepage: None,
            repository: None,
            documentation: None,
            dependencies: Dependencies {
                database_version: ">=3.0.0".parse().unwrap(),
                python: vec![],
            },
            hash: ArtifactHash::try_new(
                "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            )
            .unwrap(),
            yanked: false,
        }],
    };

    let err = validate::plugin_dir_with_index(&plugin_dir, &index).unwrap_err();
    match err {
        SdkError::ValidationErrors(errs) => {
            assert_eq!(errs.len(), 1);
            match &errs[0] {
                ValidationError::NameVersionConflict { name, version } => {
                    assert_eq!(name, "valid-plugin");
                    assert_eq!(version, "0.1.0");
                }
                other => panic!("expected NameVersionConflict, got {other:?}"),
            }
        }
        other => panic!("expected ValidationErrors, got {other:?}"),
    }
}

#[test]
fn validate_with_index_returns_manifest_when_no_collision() {
    let plugin_dir = fixtures().join("valid_plugin");
    let index = empty_index();

    let manifest =
        validate::plugin_dir_with_index(&plugin_dir, &index).expect("no collision; should pass");
    assert_eq!(manifest.plugin.name.as_str(), "valid-plugin");
}

#[test]
fn multi_cross_file_defects_collected_in_one_pass() {
    let err = validate::plugin_dir(&fixtures().join("invalid_plugins/multi_cross_file_defect"))
        .unwrap_err();

    let SdkError::ValidationErrors(errs) = err else {
        panic!("expected ValidationErrors, got {err:?}");
    };
    assert_eq!(
        errs.len(),
        2,
        "expected 2 errors, got {}: {:?}",
        errs.len(),
        errs
    );

    let async_found = errs.iter().any(|e| {
        matches!(
            e,
            ValidationError::AsyncTriggerFn {
                trigger: TriggerType::ProcessWrites
            }
        )
    });
    let missing_found = errs.iter().any(|e| {
        matches!(
            e,
            ValidationError::TriggerNotImplemented {
                trigger: TriggerType::ProcessScheduledCall
            }
        )
    });
    assert!(
        async_found,
        "expected AsyncTriggerFn(ProcessWrites) among {errs:?}"
    );
    assert!(
        missing_found,
        "expected TriggerNotImplemented(ProcessScheduledCall) among {errs:?}"
    );
}
