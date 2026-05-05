//! Plugin-directory validation.
//!
//! Two plugin formats are supported:
//!
//! - **Multi-file** — a directory containing `__init__.py` (entry point) plus
//!   any number of helper modules. Detected when `__init__.py` exists as a
//!   regular file at the top level.
//! - **Single-file** — a directory containing exactly one `.py` file (no
//!   `__init__.py`). Detected when no `__init__.py` exists and there is
//!   exactly one regular `.py` file at the top level.
//!
//! Entry-point detection uses `symlink_metadata()` so symbolic links are
//! excluded (matching archive-collection semantics — archives store the link
//! target, not the link itself).
//!
//! Two check categories:
//!
//! - **Structural** (via [`Manifest::parse_toml`]): manifest well-formedness,
//!   required-file presence, name/version/trigger/URL/dep parseability,
//!   description length, non-empty triggers array, URL scheme allowlist.
//! - **Code / manifest cross-reference**: the entry point parses as valid
//!   Python 3, and each trigger declared in `manifest.plugin.triggers` has
//!   a top-level synchronous `def <trigger>(...)`. Indirect definitions
//!   (re-exports, module-level assignments, class methods) and `async def`
//!   are explicit non-matches.
//!
//! # Multi-error collection
//!
//! Structural parse failures short-circuit — cross-file checks cannot run
//! without a valid manifest. Cross-file failures accumulate into a
//! [`ValidationReport`] so multiple issues surface together.
//!
//! # Python parser
//!
//! `tree-sitter-python` — rationale for this pick over pyo3, shell-out, and
//! other Rust parsers lives in the core design-decisions doc.

use influxdb3_plugin_schemas::{IndexEntry, Manifest, TriggerType};
use std::collections::HashMap;
use std::path::Path;

use crate::{SdkError, ValidationError, ValidationReport};

/// Result of scanning the top level of a plugin directory for a Python entry
/// point. Crate-private — only used within `validate`.
enum DetectedEntryPoint {
    /// `__init__.py` exists as a regular file (multi-file plugin).
    MultiFile { contents: String },
    /// Exactly one `.py` file exists (no `__init__.py`) — single-file plugin.
    SingleFile { filename: String, contents: String },
}

/// Scans the top level of `dir` for a Python entry point.
///
/// Uses `symlink_metadata()` to exclude symbolic links (matching archive-
/// collection semantics). Returns `None` when an error is pushed to `report`.
fn detect_entry_point(dir: &Path, report: &mut ValidationReport) -> Option<DetectedEntryPoint> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            report.push(ValidationError::NoEntryPoint);
            return None;
        }
        Err(_) => {
            report.push(ValidationError::NoEntryPoint);
            return None;
        }
    };

    // Check for __init__.py first (multi-file takes priority).
    let init_path = dir.join("__init__.py");
    if let Ok(meta) = std::fs::symlink_metadata(&init_path)
        && meta.is_file()
    {
        match std::fs::read_to_string(&init_path) {
            Ok(contents) => {
                return Some(DetectedEntryPoint::MultiFile { contents });
            }
            Err(_) => {
                report.push(ValidationError::NoEntryPoint);
                return None;
            }
        }
    }

    // No __init__.py — scan for regular .py files.
    let mut py_files: Vec<String> = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if !name.ends_with(".py") {
            continue;
        }
        // Use symlink_metadata to exclude symlinks.
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        py_files.push(name.into_owned());
    }

    match py_files.len() {
        0 => {
            report.push(ValidationError::NoEntryPoint);
            None
        }
        1 => {
            let filename = py_files.into_iter().next().unwrap();
            let path = dir.join(&filename);
            match std::fs::read_to_string(&path) {
                Ok(contents) => Some(DetectedEntryPoint::SingleFile { filename, contents }),
                Err(_) => {
                    report.push(ValidationError::NoEntryPoint);
                    None
                }
            }
        }
        _ => {
            py_files.sort();
            report.push(ValidationError::AmbiguousEntryPoint { files: py_files });
            None
        }
    }
}

/// Validates a plugin directory.
///
/// Returns the parsed [`Manifest`] on success so downstream callers (e.g.
/// [`crate::package::package_plugin`]) don't re-parse. On failure:
/// - `SdkError::Io` — I/O error other than `NotFound` on required files.
/// - `SdkError::ValidationErrors` — structural or cross-file check failures.
///
/// Entry-point detection runs unconditionally (before the manifest check).
/// Missing `manifest.toml` surfaces as `ValidationError::MissingRequiredFile`.
/// Structural manifest parse failure is fail-fast: without a valid manifest,
/// the set of declared triggers is unknown and cross-file checks can't run.
/// Cross-file failures accumulate so multiple missing triggers or an
/// unparseable Python source come back together in one report.
pub fn plugin_dir(dir: &Path) -> Result<Manifest, SdkError> {
    let mut report = ValidationReport::new();

    // Entry-point detection runs unconditionally.
    let entry_point = detect_entry_point(dir, &mut report);

    let manifest_raw =
        read_optional_required(&dir.join("manifest.toml"), "manifest.toml", &mut report)?;

    // Without manifest contents, the trigger list is unknown, so cross-file
    // checks can't run. Surface the collected diagnostics now.
    let Some(manifest_raw) = manifest_raw else {
        report.into_result()?;
        unreachable!("non-empty report always returns Err");
    };
    let manifest = Manifest::parse_toml(&manifest_raw)?;

    if let Some(ep) = entry_point {
        let (entry_point_name, contents) = match ep {
            DetectedEntryPoint::MultiFile { contents } => ("__init__.py".to_owned(), contents),
            DetectedEntryPoint::SingleFile { filename, contents } => (filename, contents),
        };
        check_python_source(
            &contents,
            &manifest.plugin.triggers,
            &entry_point_name,
            &mut report,
        );
    }
    report.into_result()?;
    Ok(manifest)
}

/// Reads a required file, treating `NotFound` as a collectible validation
/// error rather than a runtime failure. Returns `Ok(None)` when the file is
/// missing (after recording the diagnostic), `Ok(Some(content))` when read
/// succeeded, and `Err(SdkError::Io)` for other I/O errors.
fn read_optional_required(
    path: &Path,
    label: &str,
    report: &mut ValidationReport,
) -> Result<Option<String>, SdkError> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            report.push(ValidationError::MissingRequiredFile { file: label.into() });
            Ok(None)
        }
        Err(source) => Err(SdkError::Io {
            source,
            path: Some(path.to_path_buf()),
        }),
    }
}

/// [`plugin_dir`] plus an index-relative uniqueness check.
///
/// Runs full [`plugin_dir`] validation first (short-circuits on structural
/// or cross-file failure). On success, compares the manifest's
/// `(name, version)` against every entry in `index.plugins[]`; a collision
/// surfaces as `SdkError::ValidationErrors` carrying a single
/// [`ValidationError::NameVersionConflict`].
///
/// Backs index-aware validation, letting uniqueness conflicts appear in the
/// same diagnostics array as other validation errors. The distinct
/// mutation-boundary check in `mutate_index::add_entry` returns
/// `SdkError::AlreadyPublished` instead.
pub fn plugin_dir_with_index(
    dir: &std::path::Path,
    index: &influxdb3_plugin_schemas::Index,
) -> Result<Manifest, SdkError> {
    let manifest = plugin_dir(dir)?;

    let probe_entry = IndexEntry::from_manifest(manifest.clone(), crate::hash::zero_hash());
    if let Err(err) = index.check_entry_insert(&probe_entry) {
        use influxdb3_plugin_schemas::IndexInsertError;
        // Surface a conflict only when the (canonical-name, version) pair
        // already exists in the index — matching the original inline check of
        // `canonical_match && version_match`. `CanonicalCollision` with a
        // *different* version is intentionally not flagged here; that stricter
        // spelling check runs at publish time in `mutate_index::add_entry`.
        let version_conflict = match &err {
            IndexInsertError::Duplicate { .. } => true,
            IndexInsertError::CanonicalCollision { existing, .. } => {
                existing.iter().any(|(_, v)| v == &probe_entry.version)
            }
            _ => false,
        };
        if version_conflict {
            let mut report = ValidationReport::new();
            report.push(ValidationError::NameVersionConflict {
                name: manifest.plugin.name.as_str().to_owned(),
                version: manifest.plugin.version.to_string(),
            });
            report.into_result()?;
            unreachable!("non-empty report always returns Err");
        }
    }

    Ok(manifest)
}

/// Parses `source` with tree-sitter-python and records findings into `report`.
fn check_python_source(
    source: &str,
    declared_triggers: &[TriggerType],
    entry_point: &str,
    report: &mut ValidationReport,
) {
    let mut parser = tree_sitter::Parser::new();
    let language = tree_sitter_python::LANGUAGE;
    parser
        .set_language(&language.into())
        .expect("tree-sitter-python grammar initializes");

    let Some(tree) = parser.parse(source, None) else {
        report.push(ValidationError::PythonParse {
            entry_point: entry_point.to_owned(),
            message: "tree-sitter produced no parse tree".into(),
        });
        return;
    };

    let root = tree.root_node();
    if root.has_error() {
        report.push(ValidationError::PythonParse {
            entry_point: entry_point.to_owned(),
            message: format_parse_error(root, source),
        });
        return;
    }

    // Recognize top-level `function_definition` and `decorated_definition`
    // (which wraps a `function_definition`). Decorators don't make a def
    // indirect; class methods, re-exports, and assignments do, so those
    // aren't collected here.
    let mut top_level_defs: HashMap<String, DefKind> = HashMap::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if let Some((name, kind)) = extract_top_level_def(&child, source) {
            top_level_defs.insert(name, kind);
        }
    }

    for trigger in declared_triggers {
        let expected = trigger.as_str();
        match top_level_defs.get(expected) {
            None => report.push(ValidationError::TriggerNotImplemented {
                trigger: *trigger,
                entry_point: entry_point.to_owned(),
            }),
            Some(DefKind::Async) => report.push(ValidationError::AsyncTriggerFn {
                trigger: *trigger,
                entry_point: entry_point.to_owned(),
            }),
            Some(DefKind::Sync) => {}
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum DefKind {
    Sync,
    Async,
}

/// If `node` is (or wraps) a top-level function definition, return its name
/// and sync/async kind. Returns `None` for class defs, imports, expressions,
/// assignments, and malformed defs caught by tree-sitter error recovery.
fn extract_top_level_def(node: &tree_sitter::Node<'_>, source: &str) -> Option<(String, DefKind)> {
    let function_def = match node.kind() {
        "function_definition" => *node,
        // `@foo\ndef bar():` parses as a `decorated_definition` wrapping a
        // `function_definition`; descend to the inner node.
        "decorated_definition" => {
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .find(|c| c.kind() == "function_definition")?
        }
        _ => return None,
    };
    let name_node = function_def.child_by_field_name("name")?;
    let name = source[name_node.byte_range()].to_owned();
    let kind = if is_async_function(&function_def) {
        DefKind::Async
    } else {
        DefKind::Sync
    };
    Some((name, kind))
}

/// tree-sitter-python 0.25 emits both `def` and `async def` as
/// `function_definition`; the async case has an `async` keyword child.
fn is_async_function(function_def: &tree_sitter::Node<'_>) -> bool {
    let mut cursor = function_def.walk();
    for child in function_def.children(&mut cursor) {
        if child.kind() == "async" {
            return true;
        }
    }
    false
}

/// Human-readable description of the first parse error in source order.
fn format_parse_error(root: tree_sitter::Node<'_>, source: &str) -> String {
    let Some(err_node) = find_first_error_or_missing(root) else {
        return "parse error (unknown location)".into();
    };
    let start = err_node.start_position();
    let snippet = source_snippet(source, &err_node);
    format!(
        "parse error at line {}, column {}: `{}`",
        start.row + 1,
        start.column + 1,
        snippet
    )
}

fn source_snippet(source: &str, node: &tree_sitter::Node<'_>) -> String {
    let range = node.byte_range();
    let end = range.end.min(range.start + 40);
    let slice = source.get(range.start..end).unwrap_or("");
    slice.replace('\n', "\\n")
}

/// Depth-first pre-order search for the earliest error/missing node.
/// Relies on tree-sitter's `children()` yielding in source order.
fn find_first_error_or_missing(root: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    if root.is_error() || root.is_missing() {
        return Some(root);
    }
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if let Some(found) = find_first_error_or_missing(child) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trigger_list(triggers: &[TriggerType]) -> Vec<TriggerType> {
        triggers.to_vec()
    }

    #[test]
    fn happy_path_sync_def_matches_trigger() {
        let src = "def process_writes(a, b, c):\n    pass\n";
        let mut report = ValidationReport::new();
        check_python_source(
            src,
            &trigger_list(&[TriggerType::ProcessWrites]),
            "__init__.py",
            &mut report,
        );
        assert!(report.is_empty(), "expected no errors, got {report:?}");
    }

    #[test]
    fn missing_trigger_def_reported() {
        let src = "def something_else():\n    pass\n";
        let mut report = ValidationReport::new();
        check_python_source(
            src,
            &trigger_list(&[TriggerType::ProcessWrites]),
            "__init__.py",
            &mut report,
        );
        assert_eq!(report.len(), 1);
        let err = report.into_result().unwrap_err();
        let SdkError::ValidationErrors(errs) = err else {
            panic!("expected ValidationErrors")
        };
        assert!(matches!(
            errs[0],
            ValidationError::TriggerNotImplemented {
                trigger: TriggerType::ProcessWrites,
                ..
            }
        ));
    }

    #[test]
    fn async_def_rejected_even_if_name_matches() {
        let src = "async def process_writes(a, b, c):\n    pass\n";
        let mut report = ValidationReport::new();
        check_python_source(
            src,
            &trigger_list(&[TriggerType::ProcessWrites]),
            "__init__.py",
            &mut report,
        );
        let err = report.into_result().unwrap_err();
        match err {
            SdkError::ValidationErrors(errs) => {
                assert_eq!(errs.len(), 1);
                assert!(matches!(errs[0], ValidationError::AsyncTriggerFn { .. }));
            }
            other => panic!("expected AsyncTriggerFn, got {other:?}"),
        }
    }

    #[test]
    fn class_method_with_matching_name_does_not_satisfy_trigger() {
        // Indirect definitions (class methods) don't count.
        let src = "class Foo:\n    def process_writes(self):\n        pass\n";
        let mut report = ValidationReport::new();
        check_python_source(
            src,
            &trigger_list(&[TriggerType::ProcessWrites]),
            "__init__.py",
            &mut report,
        );
        assert_eq!(report.len(), 1);
        let err = report.into_result().unwrap_err();
        let SdkError::ValidationErrors(errs) = err else {
            panic!()
        };
        assert!(matches!(
            errs[0],
            ValidationError::TriggerNotImplemented { .. }
        ));
    }

    #[test]
    fn module_level_assignment_does_not_satisfy_trigger() {
        // Module-level assignments don't count as trigger definitions.
        let src = "process_writes = lambda a, b, c: None\n";
        let mut report = ValidationReport::new();
        check_python_source(
            src,
            &trigger_list(&[TriggerType::ProcessWrites]),
            "__init__.py",
            &mut report,
        );
        assert_eq!(report.len(), 1);
        let err = report.into_result().unwrap_err();
        let SdkError::ValidationErrors(errs) = err else {
            panic!("expected ValidationErrors")
        };
        assert!(matches!(
            errs[0],
            ValidationError::TriggerNotImplemented {
                trigger: TriggerType::ProcessWrites,
                ..
            }
        ));
    }

    #[test]
    fn def_inside_triple_quoted_string_is_not_a_function() {
        // A triple-quoted string whose text looks like a def must not match —
        // the guard against any regex-based shortcut.
        let src = r#"
doc = """
def process_writes():
    pass
"""
"#;
        let mut report = ValidationReport::new();
        check_python_source(
            src,
            &trigger_list(&[TriggerType::ProcessWrites]),
            "__init__.py",
            &mut report,
        );
        assert_eq!(report.len(), 1);
        let err = report.into_result().unwrap_err();
        let SdkError::ValidationErrors(errs) = err else {
            panic!("expected ValidationErrors")
        };
        assert!(matches!(
            errs[0],
            ValidationError::TriggerNotImplemented {
                trigger: TriggerType::ProcessWrites,
                ..
            }
        ));
    }

    #[test]
    fn syntax_error_reported_as_python_parse() {
        let src = "def oops(:\n    pass\n";
        let mut report = ValidationReport::new();
        check_python_source(
            src,
            &trigger_list(&[TriggerType::ProcessWrites]),
            "__init__.py",
            &mut report,
        );
        let err = report.into_result().unwrap_err();
        let SdkError::ValidationErrors(errs) = err else {
            panic!()
        };
        assert_eq!(errs.len(), 1);
        assert!(matches!(errs[0], ValidationError::PythonParse { .. }));
    }

    #[test]
    fn multiple_missing_triggers_reported_together() {
        let src = "def unrelated():\n    pass\n";
        let mut report = ValidationReport::new();
        check_python_source(
            src,
            &trigger_list(&[
                TriggerType::ProcessWrites,
                TriggerType::ProcessScheduledCall,
                TriggerType::ProcessRequest,
            ]),
            "__init__.py",
            &mut report,
        );
        assert_eq!(report.len(), 3);
        let err = report.into_result().unwrap_err();
        let SdkError::ValidationErrors(errs) = err else {
            panic!("expected ValidationErrors")
        };
        let mut missing: Vec<TriggerType> = errs
            .iter()
            .map(|e| match e {
                ValidationError::TriggerNotImplemented { trigger, .. } => *trigger,
                other => panic!("expected TriggerNotImplemented, got {other:?}"),
            })
            .collect();
        missing.sort_by_key(|t| t.as_str().to_owned());
        assert_eq!(
            missing,
            vec![
                TriggerType::ProcessRequest,
                TriggerType::ProcessScheduledCall,
                TriggerType::ProcessWrites,
            ]
        );
    }

    #[test]
    fn multiple_triggers_accepted_when_all_implemented() {
        let src = r#"
def process_writes(a, b, c):
    pass

def process_scheduled_call(a, b, c):
    pass
"#;
        let mut report = ValidationReport::new();
        check_python_source(
            src,
            &trigger_list(&[
                TriggerType::ProcessWrites,
                TriggerType::ProcessScheduledCall,
            ]),
            "__init__.py",
            &mut report,
        );
        assert!(report.is_empty());
    }

    #[test]
    fn decorated_function_still_recognized() {
        let src = r#"
@staticmethod
def process_writes(a, b, c):
    pass
"#;
        let mut report = ValidationReport::new();
        check_python_source(
            src,
            &trigger_list(&[TriggerType::ProcessWrites]),
            "__init__.py",
            &mut report,
        );
        assert!(report.is_empty(), "got {report:?}");
    }

    /// Regression guard: for a source with multiple syntax errors, the
    /// reported parse-error position must be the earliest in source order.
    /// A prior stack-based `walk_preorder` impl produced pop-order semantics,
    /// not source order.
    #[test]
    fn first_parse_error_is_earliest_position() {
        // Three errors on lines 2, 4, 6: stray `:`, missing `)`, malformed class.
        let src = "\n\
                   def first(:\n\
                   \n\
                   x = (1 + 2\n\
                   \n\
                   class Bad[\n";
        let mut report = ValidationReport::new();
        check_python_source(
            src,
            &trigger_list(&[TriggerType::ProcessWrites]),
            "__init__.py",
            &mut report,
        );
        let err = report.into_result().unwrap_err();
        let SdkError::ValidationErrors(errs) = err else {
            panic!("expected ValidationErrors")
        };
        assert_eq!(errs.len(), 1);
        let ValidationError::PythonParse { message, .. } = &errs[0] else {
            panic!("expected PythonParse, got {:?}", errs[0])
        };
        // Earliest error is on line 2; later lines also contain errors.
        assert!(
            message.contains("line 2"),
            "expected error on line 2 (earliest position), got: {message}"
        );
    }

    fn build_index_with_one_entry(name: &str, version: &str) -> influxdb3_plugin_schemas::Index {
        let json = format!(
            r#"{{
                "index_schema_version": "2.0",
                "artifacts_url": "https://x.example/a",
                "plugins": [{{
                    "name": "{name}",
                    "version": "{version}",
                    "published_at": "2026-04-29T18:45:12Z",
                    "description": "seed",
                    "triggers": ["process_writes"],
                    "dependencies": {{ "database_version": ">=3.0.0", "python": [] }},
                    "hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                }}]
            }}"#
        );
        influxdb3_plugin_schemas::Index::parse_json(&json).expect("fixture parses")
    }

    fn write_plugin_with_name(dir: &std::path::Path, name: &str, version: &str) {
        std::fs::create_dir_all(dir).unwrap();
        let manifest = format!(
            r#"manifest_schema_version = "1.0"

[plugin]
name = "{name}"
version = "{version}"
description = "x"
triggers = ["process_writes"]

[dependencies]
database_version = ">=3.0.0"
"#
        );
        std::fs::write(dir.join("manifest.toml"), manifest).unwrap();
        std::fs::write(
            dir.join("__init__.py"),
            "def process_writes(a, b, c):\n    pass\n",
        )
        .unwrap();
    }

    fn assert_canonical_collision(manifest_name: &str, index_name: &str) {
        let td = tempfile::tempdir().unwrap();
        write_plugin_with_name(td.path(), manifest_name, "0.1.0");
        let index = build_index_with_one_entry(index_name, "0.1.0");

        let err =
            plugin_dir_with_index(td.path(), &index).expect_err("canonical collision must fail");
        let SdkError::ValidationErrors(errs) = err else {
            panic!("expected ValidationErrors, got {err:?}");
        };
        assert_eq!(errs.len(), 1);
        let ValidationError::NameVersionConflict { name, version } = &errs[0] else {
            panic!("expected NameVersionConflict, got {:?}", errs[0]);
        };
        assert_eq!(
            name, manifest_name,
            "diagnostic should pin the manifest's spelling"
        );
        assert_eq!(version, "0.1.0");
    }

    #[test]
    fn plugin_dir_with_index_collides_on_hyphen_underscore() {
        assert_canonical_collision("foo-bar", "foo_bar");
    }

    #[test]
    fn plugin_dir_with_index_collides_on_case() {
        assert_canonical_collision("Foo", "foo");
    }

    #[test]
    fn plugin_dir_with_index_collides_on_mixed_canonical() {
        assert_canonical_collision("Foo-Bar_Baz", "foo_bar_baz");
    }

    #[test]
    fn plugin_dir_with_index_no_collision_when_canonical_differs() {
        let td = tempfile::tempdir().unwrap();
        write_plugin_with_name(td.path(), "foo", "0.1.0");
        let index = build_index_with_one_entry("bar", "0.1.0");
        plugin_dir_with_index(td.path(), &index).expect("no collision");
    }

    #[test]
    fn plugin_dir_with_index_no_collision_when_version_differs() {
        let td = tempfile::tempdir().unwrap();
        write_plugin_with_name(td.path(), "foo-bar", "0.2.0");
        let index = build_index_with_one_entry("foo_bar", "0.1.0");
        plugin_dir_with_index(td.path(), &index).expect("different versions, no collision");
    }

    #[test]
    fn empty_dir_reports_missing_manifest_and_no_entry_point() {
        let td = tempfile::tempdir().unwrap();
        let err = plugin_dir(td.path()).expect_err("empty dir must fail");
        let SdkError::ValidationErrors(errs) = err else {
            panic!("expected ValidationErrors, got {err:?}");
        };
        assert_eq!(
            errs.len(),
            2,
            "expected exactly two diagnostics, got {errs:?}"
        );
        // First: NoEntryPoint (entry-point detection runs unconditionally first)
        assert!(
            matches!(errs[0], ValidationError::NoEntryPoint),
            "expected NoEntryPoint, got {:?}",
            errs[0]
        );
        // Second: MissingRequiredFile for manifest.toml
        assert!(
            matches!(
                &errs[1],
                ValidationError::MissingRequiredFile { file } if file == "manifest.toml"
            ),
            "expected MissingRequiredFile(manifest.toml), got {:?}",
            errs[1]
        );
    }
}
