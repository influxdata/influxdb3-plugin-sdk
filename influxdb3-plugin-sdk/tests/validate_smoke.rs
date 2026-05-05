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

const MINIMAL_MANIFEST: &str = r#"manifest_schema_version = "1.0"
[plugin]
name = "test"
version = "0.1.0"
description = "x"
triggers = ["process_writes"]
[dependencies]
database_version = ">=3.0.0"
"#;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn valid_plugin_passes() {
    validate::plugin_dir(&fixtures().join("valid_plugin"))
        .expect("valid_plugin fixture should pass validation");
}

#[test]
fn missing_init_reports_no_entry_point() {
    let err = validate::plugin_dir(&fixtures().join("invalid_plugins/missing_init")).unwrap_err();
    match err {
        SdkError::ValidationErrors(errs) => {
            assert_eq!(errs.len(), 1);
            assert!(matches!(errs[0], ValidationError::NoEntryPoint));
        }
        other => panic!("expected ValidationErrors(NoEntryPoint), got {other:?}"),
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
                    trigger: TriggerType::ProcessWrites,
                    ..
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
                    trigger: TriggerType::ProcessWrites,
                    ..
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
        IndexSchemaVersion, PublishedAt, TriggerType,
    };

    let plugin_dir = fixtures().join("valid_plugin");
    // valid_plugin's manifest declares name="valid-plugin", version="0.1.0".
    // Construct an index that already contains that (name, version).
    let index = Index {
        index_schema_version: IndexSchemaVersion::CURRENT,
        artifacts_url: ArtifactsUrl::try_new("https://example.com/artifacts").unwrap(),
        plugins: vec![IndexEntry {
            name: "valid-plugin".parse().unwrap(),
            version: semver::Version::new(0, 1, 0),
            published_at: PublishedAt::try_new("2026-04-29T18:45:12Z").unwrap(),
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
                trigger: TriggerType::ProcessWrites,
                ..
            }
        )
    });
    let missing_found = errs.iter().any(|e| {
        matches!(
            e,
            ValidationError::TriggerNotImplemented {
                trigger: TriggerType::ProcessScheduledCall,
                ..
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

// ---------------------------------------------------------------------------
// Single-file plugin fixture-based tests
// ---------------------------------------------------------------------------

#[test]
fn valid_single_file_plugin_passes() {
    validate::plugin_dir(&fixtures().join("valid_single_file_plugin"))
        .expect("valid single-file plugin should pass validation");
}

#[test]
fn no_entry_point_reports_error() {
    let err = validate::plugin_dir(&fixtures().join("invalid_plugins/no_entry_point")).unwrap_err();
    let SdkError::ValidationErrors(errs) = err else {
        panic!("expected ValidationErrors")
    };
    assert_eq!(errs.len(), 1);
    assert!(matches!(errs[0], ValidationError::NoEntryPoint));
}

#[test]
fn ambiguous_entry_point_reports_sorted_files() {
    let err = validate::plugin_dir(&fixtures().join("invalid_plugins/ambiguous_entry_point"))
        .unwrap_err();
    let SdkError::ValidationErrors(errs) = err else {
        panic!("expected ValidationErrors")
    };
    assert_eq!(errs.len(), 1);
    match &errs[0] {
        ValidationError::AmbiguousEntryPoint { files } => {
            assert_eq!(files, &["bar.py", "foo.py"]);
        }
        other => panic!("expected AmbiguousEntryPoint, got {other:?}"),
    }
}

#[test]
fn single_file_missing_trigger_names_entry_point() {
    let err = validate::plugin_dir(&fixtures().join("invalid_plugins/single_file_missing_trigger"))
        .unwrap_err();
    let SdkError::ValidationErrors(errs) = err else {
        panic!("expected ValidationErrors")
    };
    assert_eq!(errs.len(), 1);
    match &errs[0] {
        ValidationError::TriggerNotImplemented {
            trigger,
            entry_point,
        } => {
            assert_eq!(*trigger, TriggerType::ProcessWrites);
            assert_eq!(entry_point, "my_plugin.py");
        }
        other => panic!("expected TriggerNotImplemented, got {other:?}"),
    }
}

#[test]
fn single_file_async_trigger_names_entry_point() {
    let err = validate::plugin_dir(&fixtures().join("invalid_plugins/single_file_async_trigger"))
        .unwrap_err();
    let SdkError::ValidationErrors(errs) = err else {
        panic!("expected ValidationErrors")
    };
    assert_eq!(errs.len(), 1);
    match &errs[0] {
        ValidationError::AsyncTriggerFn {
            trigger,
            entry_point,
        } => {
            assert_eq!(*trigger, TriggerType::ProcessWrites);
            assert_eq!(entry_point, "my_plugin.py");
        }
        other => panic!("expected AsyncTriggerFn, got {other:?}"),
    }
}

#[test]
fn single_file_bad_syntax_names_entry_point() {
    let err = validate::plugin_dir(&fixtures().join("invalid_plugins/single_file_bad_syntax"))
        .unwrap_err();
    let SdkError::ValidationErrors(errs) = err else {
        panic!("expected ValidationErrors")
    };
    assert_eq!(errs.len(), 1);
    match &errs[0] {
        ValidationError::PythonParse { entry_point, .. } => {
            assert_eq!(entry_point, "my_plugin.py");
        }
        other => panic!("expected PythonParse, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Edge-case tests (programmatic with tempdir)
// ---------------------------------------------------------------------------

#[test]
fn single_file_with_non_py_files_still_detected() {
    let td = tempfile::tempdir().unwrap();
    std::fs::write(td.path().join("manifest.toml"), MINIMAL_MANIFEST).unwrap();
    std::fs::write(
        td.path().join("my_plugin.py"),
        "def process_writes(a, b, c):\n    pass\n",
    )
    .unwrap();
    std::fs::write(td.path().join("README.md"), "# readme").unwrap();
    std::fs::write(td.path().join("data.json"), "{}").unwrap();
    validate::plugin_dir(td.path()).expect("non-py files should not affect detection");
}

#[test]
fn py_files_in_subdirectory_not_counted() {
    let td = tempfile::tempdir().unwrap();
    std::fs::write(td.path().join("manifest.toml"), MINIMAL_MANIFEST).unwrap();
    std::fs::create_dir(td.path().join("subdir")).unwrap();
    std::fs::write(
        td.path().join("subdir/plugin.py"),
        "def process_writes(a,b,c): pass\n",
    )
    .unwrap();
    let err = validate::plugin_dir(td.path()).unwrap_err();
    let SdkError::ValidationErrors(errs) = err else {
        panic!("expected ValidationErrors")
    };
    assert!(
        errs.iter()
            .any(|e| matches!(e, ValidationError::NoEntryPoint))
    );
}

#[test]
fn nonexistent_plugin_dir_returns_validation_errors() {
    let td = tempfile::tempdir().unwrap();
    let missing = td.path().join("does_not_exist");
    let err = validate::plugin_dir(&missing).unwrap_err();
    // A nonexistent directory surfaces as ValidationErrors (NoEntryPoint +
    // MissingRequiredFile) since both detect_entry_point and the manifest
    // read treat NotFound as collectible validation diagnostics.
    let SdkError::ValidationErrors(errs) = err else {
        panic!("expected ValidationErrors for missing dir, got {err:?}")
    };
    assert!(
        errs.iter()
            .any(|e| matches!(e, ValidationError::NoEntryPoint)),
        "expected NoEntryPoint among {errs:?}"
    );
}

// ---------------------------------------------------------------------------
// Symlink tests (Unix only)
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn symlinked_init_py_only_reports_no_entry_point() {
    let td = tempfile::tempdir().unwrap();
    std::fs::write(td.path().join("manifest.toml"), MINIMAL_MANIFEST).unwrap();
    let target = td.path().join("real_target.py");
    std::fs::write(&target, "def process_writes(a,b,c): pass\n").unwrap();
    std::os::unix::fs::symlink(&target, td.path().join("__init__.py")).unwrap();
    std::fs::remove_file(&target).unwrap();
    // __init__.py is a dangling symlink — not counted as regular file. No other .py files.
    let err = validate::plugin_dir(td.path()).unwrap_err();
    let SdkError::ValidationErrors(errs) = err else {
        panic!("expected ValidationErrors")
    };
    assert!(
        errs.iter()
            .any(|e| matches!(e, ValidationError::NoEntryPoint))
    );
}

#[cfg(unix)]
#[test]
fn symlinked_py_file_only_reports_no_entry_point() {
    let td = tempfile::tempdir().unwrap();
    std::fs::write(td.path().join("manifest.toml"), MINIMAL_MANIFEST).unwrap();
    let target = td.path().join("real_target.txt");
    std::fs::write(&target, "not python").unwrap();
    std::os::unix::fs::symlink(&target, td.path().join("plugin.py")).unwrap();
    // plugin.py is a symlink — not counted. No regular .py files.
    let err = validate::plugin_dir(td.path()).unwrap_err();
    let SdkError::ValidationErrors(errs) = err else {
        panic!("expected ValidationErrors")
    };
    assert!(
        errs.iter()
            .any(|e| matches!(e, ValidationError::NoEntryPoint))
    );
}

#[cfg(unix)]
#[test]
fn symlinked_py_alongside_real_py_counts_only_real() {
    let td = tempfile::tempdir().unwrap();
    let plugin_dir = td.path().join("plugin");
    std::fs::create_dir(&plugin_dir).unwrap();
    std::fs::write(plugin_dir.join("manifest.toml"), MINIMAL_MANIFEST).unwrap();
    std::fs::write(
        plugin_dir.join("real.py"),
        "def process_writes(a,b,c): pass\n",
    )
    .unwrap();
    // Put the symlink target outside the plugin directory so it's not counted.
    let target = td.path().join("somewhere.py");
    std::fs::write(&target, "").unwrap();
    std::os::unix::fs::symlink(&target, plugin_dir.join("link.py")).unwrap();
    // Only real.py is a regular file — link.py is a symlink, not counted.
    validate::plugin_dir(&plugin_dir).expect("should detect real.py as single-file entry point");
}
