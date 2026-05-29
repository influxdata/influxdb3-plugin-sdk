//! Plugin-directory validation — filesystem + Python-parser mechanism.
//!
//! This module owns the *mechanism* of validating a plugin directory on disk:
//! walking the top level, reading the entry-point file, and extracting
//! top-level Python definitions with `tree-sitter-python`. The *contract* —
//! what counts as a valid entry-point layout and a satisfied trigger — is the
//! pure surface in [`influxdb3_plugin_schemas::validate`]
//! ([`classify_entry_point`], [`check_triggers`]). This module feeds its
//! mechanical results into those pure checks.
//!
//! Two plugin formats are supported:
//!
//! - **Multi-file** — a directory containing `__init__.py` (entry point) plus
//!   any number of helper modules.
//! - **Single-file** — a directory containing exactly one `.py` file (no
//!   `__init__.py`).
//!
//! Entry-point detection uses `symlink_metadata()` so symbolic links are
//! excluded (matching archive-collection semantics — archives store the link
//! target, not the link itself).
//!
//! # Reference extractor
//!
//! [`extract_top_level_defs`] is the reference implementation of the
//! extraction rules documented on [`influxdb3_plugin_schemas::validate::TopLevelDef`].
//! It is `pub` so a consumer that accepts a `tree-sitter` dependency (e.g. the
//! future runtime) can reuse it directly instead of reimplementing the rules.
//! The shared [`TOP_LEVEL_DEF_CORPUS`](influxdb3_plugin_schemas::validate::TOP_LEVEL_DEF_CORPUS)
//! guards drift between extractors.
//!
//! # Multi-error collection
//!
//! Cross-file failures accumulate into a [`ValidationReport`] so multiple
//! issues surface together. A manifest parse failure stops cross-file checks
//! (the trigger set is unknown without a valid manifest) but its diagnostics
//! are merged with any entry-point diagnostic already collected, so both
//! surface in one pass.

use influxdb3_plugin_schemas::validate::{
    TopLevelDef, ValidatedPlugin, check_triggers, classify_entry_point,
};
use influxdb3_plugin_schemas::{Index, IndexEntry, Manifest, ValidationError};
use std::path::Path;

use crate::{ValidationFailure, ValidationReport};

/// A parser-level failure from [`extract_top_level_defs`].
///
/// Carries no caller-supplied metadata (e.g. the filename); the orchestrator
/// decorates it into [`ValidationError::PythonParse`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PythonParseError {
    pub message: String,
}

/// Extracts every top-level function definition from `source`, in source
/// order, using `tree-sitter-python`.
///
/// This is the reference implementation of the extraction rules on
/// [`TopLevelDef`]. It captures top-level `def`/`async def` and decorated
/// top-level functions; it does not capture class methods, nested defs,
/// guarded defs, re-exports, or assignments. It does **not** dedup — a
/// redefined name appears once per occurrence; last-occurrence-wins is
/// resolved by [`check_triggers`].
///
/// Returns [`PythonParseError`] when the source does not parse as valid
/// Python 3.
pub fn extract_top_level_defs(source: &str) -> Result<Vec<TopLevelDef>, PythonParseError> {
    let mut parser = tree_sitter::Parser::new();
    let language = tree_sitter_python::LANGUAGE;
    parser
        .set_language(&language.into())
        .expect("tree-sitter-python grammar initializes");

    let Some(tree) = parser.parse(source, None) else {
        return Err(PythonParseError {
            message: "tree-sitter produced no parse tree".into(),
        });
    };

    let root = tree.root_node();
    if root.has_error() {
        return Err(PythonParseError {
            message: format_parse_error(root, source),
        });
    }

    // Recognize top-level `function_definition` and `decorated_definition`
    // (which wraps a `function_definition`). Decorators don't make a def
    // indirect; class methods, re-exports, and assignments do, so those
    // aren't collected here.
    let mut defs: Vec<TopLevelDef> = Vec::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if let Some(def) = extract_top_level_def(&child, source) {
            defs.push(def);
        }
    }
    Ok(defs)
}

/// If `node` is (or wraps) a top-level function definition, return its name
/// and sync/async kind. Returns `None` for class defs, imports, expressions,
/// assignments, and malformed defs caught by tree-sitter error recovery.
fn extract_top_level_def(node: &tree_sitter::Node<'_>, source: &str) -> Option<TopLevelDef> {
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
    Some(TopLevelDef {
        name,
        is_async: is_async_function(&function_def),
    })
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

/// Lists the names of top-level regular files in `dir`.
///
/// Excludes symlinks (via `symlink_metadata`) and subdirectories (including a
/// directory named `foo.py`); does not recurse. Any `read_dir` failure
/// (including `NotFound`, permission, or other I/O) surfaces as
/// [`ValidationError::NoEntryPoint`] — preserving current behavior per the
/// design's non-goal. Per-entry read errors are skipped.
fn top_level_regular_file_names(dir: &Path) -> Result<Vec<String>, ValidationError> {
    let entries = std::fs::read_dir(dir).map_err(|_| ValidationError::NoEntryPoint)?;
    let mut names = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    Ok(names)
}

/// Reads a required file, treating `NotFound` as a collectible validation
/// error. Returns `Ok(Some(content))` on success, `Ok(None)` when missing
/// (after recording [`ValidationError::MissingRequiredFile`]), and
/// `Err(ValidationFailure::Io)` for other I/O errors.
fn read_required(
    path: &Path,
    label: &str,
    report: &mut ValidationReport,
) -> Result<Option<String>, ValidationFailure> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            report.push(ValidationError::MissingRequiredFile { file: label.into() });
            Ok(None)
        }
        Err(source) => Err(ValidationFailure::Io {
            source,
            path: Some(path.to_path_buf()),
        }),
    }
}

/// Validates a plugin directory.
///
/// Returns a [`ValidatedPlugin`] (parsed manifest + classified entry point)
/// on success. On failure:
/// - [`ValidationFailure::Io`] — I/O error other than `NotFound` on
///   `manifest.toml`.
/// - [`ValidationFailure::Invalid`] — structural or cross-file check failures.
///
/// Entry-point detection runs unconditionally (before the manifest check).
/// Missing `manifest.toml` surfaces as [`ValidationError::MissingRequiredFile`].
/// A manifest parse failure stops cross-file checks but its diagnostics are
/// merged with any entry-point diagnostic so both surface together. An
/// unreadable entry-point file after detection produces
/// [`ValidationError::NoEntryPoint`] (preserving current behavior).
pub fn plugin_dir(dir: &Path) -> Result<ValidatedPlugin, ValidationFailure> {
    let mut report = ValidationReport::new();

    // Step 1: entry-point detection runs unconditionally.
    let entry_point = match top_level_regular_file_names(dir) {
        Ok(names) => match classify_entry_point(&names) {
            Ok(ep) => Some(ep),
            Err(diag) => {
                report.push(diag);
                None
            }
        },
        Err(diag) => {
            report.push(diag);
            None
        }
    };

    // Step 2: read + parse manifest.toml.
    let Some(manifest_raw) =
        read_required(&dir.join("manifest.toml"), "manifest.toml", &mut report)?
    else {
        // Missing manifest: trigger set is unknown, so cross-file checks
        // can't run. Surface collected diagnostics now.
        report.into_result()?;
        unreachable!("non-empty report always returns Err");
    };
    let manifest = match Manifest::parse_toml(&manifest_raw) {
        Ok(manifest) => manifest,
        Err(schema_errors) => {
            // H2: merge manifest defects with any entry-point diagnostic so
            // both appear in one pass (do not use `?` — that would drop the
            // accumulated report).
            report.extend(
                schema_errors
                    .into_iter()
                    .map(ValidationError::SchemaReported),
            );
            report.into_result()?;
            unreachable!("non-empty report always returns Err");
        }
    };

    // Step 3: entry-point read + extraction + trigger checks.
    if let Some(ep) = &entry_point {
        let file_name = ep.file_name().to_owned();
        match std::fs::read_to_string(dir.join(&file_name)) {
            // C3: any read failure → NoEntryPoint (preserve current behavior).
            Err(_) => report.push(ValidationError::NoEntryPoint),
            Ok(source) => match extract_top_level_defs(&source) {
                Err(PythonParseError { message }) => report.push(ValidationError::PythonParse {
                    entry_point: file_name,
                    message,
                }),
                Ok(defs) => {
                    let diagnostics = check_triggers(&manifest.plugin.triggers, &defs, &file_name);
                    report.extend(diagnostics);
                }
            },
        }
    }

    // Step 4: success only when the report is empty.
    report.into_result()?;
    let entry_point = entry_point.expect("empty report implies a classified entry point");
    Ok(ValidatedPlugin::new(manifest, entry_point))
}

/// [`plugin_dir`] plus an index-relative uniqueness check.
///
/// Runs full [`plugin_dir`] validation first. On success, compares the
/// manifest's `(name, version)` against every entry in `index.plugins[]`; a
/// collision surfaces as [`ValidationFailure::Invalid`] carrying a single
/// [`ValidationError::NameVersionConflict`].
pub fn plugin_dir_with_index(
    dir: &Path,
    index: &Index,
) -> Result<ValidatedPlugin, ValidationFailure> {
    let validated = plugin_dir(dir)?;

    let probe_entry =
        IndexEntry::from_manifest(validated.manifest.clone(), crate::hash::zero_hash());
    if let Err(err) = index.check_entry_insert(&probe_entry) {
        use influxdb3_plugin_schemas::IndexInsertError;
        // Surface a conflict only when the (canonical-name, version) pair
        // already exists in the index. `CanonicalCollision` with a *different*
        // version is intentionally not flagged here; that stricter spelling
        // check runs at publish time in `mutate_index::add_entry`.
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
                name: validated.manifest.plugin.name.as_str().to_owned(),
                version: validated.manifest.plugin.version.to_string(),
            });
            report.into_result()?;
            unreachable!("non-empty report always returns Err");
        }
    }

    Ok(validated)
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
    use influxdb3_plugin_schemas::validate::{EntryPoint, Expectation, TOP_LEVEL_DEF_CORPUS};

    // -- extract_top_level_defs  F6-F15 -------------------------------------

    fn names(defs: &[TopLevelDef]) -> Vec<(String, bool)> {
        defs.iter().map(|d| (d.name.clone(), d.is_async)).collect()
    }

    #[test]
    fn f6_plain_def_captured_sync() {
        let defs = extract_top_level_defs("def foo(): pass").unwrap();
        assert_eq!(names(&defs), vec![("foo".into(), false)]);
    }

    #[test]
    fn f7_async_def_captured_async() {
        let defs = extract_top_level_defs("async def foo(): pass").unwrap();
        assert_eq!(names(&defs), vec![("foo".into(), true)]);
    }

    #[test]
    fn f8_decorated_def_captured() {
        let defs = extract_top_level_defs("@staticmethod\ndef foo(): pass").unwrap();
        assert_eq!(names(&defs), vec![("foo".into(), false)]);
    }

    #[test]
    fn f9_class_methods_not_captured() {
        let defs = extract_top_level_defs("class C:\n    def foo(self): pass").unwrap();
        assert!(defs.is_empty(), "got {defs:?}");
    }

    #[test]
    fn f10_nested_defs_not_captured() {
        let defs = extract_top_level_defs("def outer():\n    def inner(): pass").unwrap();
        assert_eq!(names(&defs), vec![("outer".into(), false)]);
    }

    #[test]
    fn f11_guarded_defs_not_top_level() {
        let defs = extract_top_level_defs("if True:\n    def foo(): pass").unwrap();
        assert!(defs.is_empty(), "got {defs:?}");
    }

    #[test]
    fn f12_reexport_and_assignment_not_captured() {
        assert!(
            extract_top_level_defs("from bar import foo")
                .unwrap()
                .is_empty()
        );
        assert!(extract_top_level_defs("foo = bar").unwrap().is_empty());
    }

    #[test]
    fn f13_redefinition_appears_per_occurrence_in_order() {
        let defs = extract_top_level_defs("def foo(): pass\nasync def foo(): pass").unwrap();
        assert_eq!(
            names(&defs),
            vec![("foo".into(), false), ("foo".into(), true)]
        );
    }

    #[test]
    fn f14_unparseable_source_is_parse_error() {
        let err = extract_top_level_defs("def foo(:").unwrap_err();
        assert!(err.message.contains("parse error"), "got {}", err.message);
    }

    #[test]
    fn f15_empty_source_is_empty() {
        assert!(extract_top_level_defs("").unwrap().is_empty());
    }

    #[test]
    fn def_inside_triple_quoted_string_is_not_a_function() {
        let src = "doc = \"\"\"\ndef process_writes():\n    pass\n\"\"\"\n";
        let defs = extract_top_level_defs(src).unwrap();
        assert!(defs.is_empty(), "got {defs:?}");
    }

    /// Regression guard: the reported parse-error position is the earliest in
    /// source order, not pop-order.
    #[test]
    fn first_parse_error_is_earliest_position() {
        let src = "\ndef first(:\n\nx = (1 + 2\n\nclass Bad[\n";
        let err = extract_top_level_defs(src).unwrap_err();
        assert!(
            err.message.contains("line 2"),
            "expected error on line 2 (earliest), got: {}",
            err.message
        );
    }

    // -- conformance corpus drift guard -------------------------------------

    #[test]
    fn sdk_extractor_satisfies_top_level_def_corpus() {
        for case in TOP_LEVEL_DEF_CORPUS {
            let result = extract_top_level_defs(case.source);
            match (&case.expected, result) {
                (Expectation::Defs(expected), Ok(got)) => {
                    let expected: Vec<(String, bool)> = expected
                        .iter()
                        .map(|e| (e.name.to_string(), e.is_async))
                        .collect();
                    assert_eq!(
                        names(&got),
                        expected,
                        "{}: extracted defs drifted",
                        case.label
                    );
                }
                (Expectation::ParseError, Err(_)) => { /* pass */ }
                (Expectation::Defs(_), Err(e)) => {
                    panic!(
                        "{}: expected defs, got parse error: {}",
                        case.label, e.message
                    )
                }
                (Expectation::ParseError, Ok(defs)) => panic!(
                    "{}: expected parse error, got {} def(s)",
                    case.label,
                    defs.len()
                ),
            }
        }
    }

    // -- orchestration: lister, required-file, multi-error ------------------

    fn write(dir: &Path, name: &str, contents: &str) {
        std::fs::write(dir.join(name), contents).unwrap();
    }

    const MINIMAL_MANIFEST: &str = "manifest_schema_version = \"1.0\"\n\
         [plugin]\n\
         name = \"test\"\n\
         version = \"0.1.0\"\n\
         description = \"x\"\n\
         triggers = [\"process_writes\"]\n\
         [dependencies]\n\
         database_version = \">=3.0.0\"\n";

    #[test]
    fn empty_dir_reports_missing_manifest_and_no_entry_point() {
        let td = tempfile::tempdir().unwrap();
        let err = plugin_dir(td.path()).expect_err("empty dir must fail");
        let ValidationFailure::Invalid(errs) = err else {
            panic!("expected Invalid, got {err:?}");
        };
        assert_eq!(errs.len(), 2, "got {errs:?}");
        // H1: entry-point detection runs first.
        assert!(
            matches!(errs[0], ValidationError::NoEntryPoint),
            "got {:?}",
            errs[0]
        );
        assert!(
            matches!(&errs[1], ValidationError::MissingRequiredFile { file } if file == "manifest.toml"),
            "got {:?}",
            errs[1]
        );
    }

    /// B4: a directory named `foo.py` is excluded; with no other entry point,
    /// classification yields `NoEntryPoint`.
    #[test]
    fn b4_directory_named_dot_py_excluded() {
        let td = tempfile::tempdir().unwrap();
        write(td.path(), "manifest.toml", MINIMAL_MANIFEST);
        std::fs::create_dir(td.path().join("foo.py")).unwrap();
        let err = plugin_dir(td.path()).expect_err("dir-named-.py must not count");
        let ValidationFailure::Invalid(errs) = err else {
            panic!("expected Invalid, got {err:?}");
        };
        assert!(
            errs.iter()
                .any(|e| matches!(e, ValidationError::NoEntryPoint))
        );
    }

    /// H2: an entry-point defect and a manifest defect surface together.
    #[test]
    fn h2_entry_point_and_manifest_defects_merged() {
        let td = tempfile::tempdir().unwrap();
        // Two .py files (no __init__.py) → AmbiguousEntryPoint.
        write(td.path(), "a.py", "def process_writes(a,b,c): pass\n");
        write(td.path(), "b.py", "def process_writes(a,b,c): pass\n");
        // Malformed manifest: empty description.
        write(
            td.path(),
            "manifest.toml",
            "manifest_schema_version = \"1.0\"\n\
             [plugin]\nname = \"test\"\nversion = \"0.1.0\"\ndescription = \"\"\ntriggers = [\"process_writes\"]\n\
             [dependencies]\ndatabase_version = \">=3.0.0\"\n",
        );
        let err = plugin_dir(td.path()).expect_err("both defects present");
        let ValidationFailure::Invalid(errs) = err else {
            panic!("expected Invalid, got {err:?}");
        };
        assert!(
            errs.iter()
                .any(|e| matches!(e, ValidationError::AmbiguousEntryPoint { .. })),
            "expected AmbiguousEntryPoint among {errs:?}"
        );
        assert!(
            errs.iter()
                .any(|e| matches!(e, ValidationError::SchemaReported(_))),
            "expected SchemaReported among {errs:?}"
        );
    }

    /// C3: an unreadable entry-point file after detection produces
    /// `NoEntryPoint`, not `Io`. Unix-only (uses permission bits).
    #[cfg(unix)]
    #[test]
    fn c3_unreadable_entry_point_is_no_entry_point() {
        use std::os::unix::fs::PermissionsExt;
        let td = tempfile::tempdir().unwrap();
        write(td.path(), "manifest.toml", MINIMAL_MANIFEST);
        let init = td.path().join("__init__.py");
        std::fs::write(&init, "def process_writes(a,b,c): pass\n").unwrap();
        std::fs::set_permissions(&init, std::fs::Permissions::from_mode(0o000)).unwrap();
        let result = plugin_dir(td.path());
        // Restore perms so tempdir cleanup succeeds.
        std::fs::set_permissions(&init, std::fs::Permissions::from_mode(0o644)).unwrap();
        let err = result.expect_err("unreadable entry point must fail");
        let ValidationFailure::Invalid(errs) = err else {
            panic!("expected Invalid (NoEntryPoint), got {err:?}");
        };
        assert!(
            errs.iter()
                .any(|e| matches!(e, ValidationError::NoEntryPoint)),
            "expected NoEntryPoint among {errs:?}"
        );
    }

    /// C2: an unreadable `manifest.toml` surfaces as `ValidationFailure::Io`.
    #[cfg(unix)]
    #[test]
    fn c2_unreadable_manifest_is_io() {
        use std::os::unix::fs::PermissionsExt;
        let td = tempfile::tempdir().unwrap();
        write(
            td.path(),
            "__init__.py",
            "def process_writes(a,b,c): pass\n",
        );
        let manifest = td.path().join("manifest.toml");
        std::fs::write(&manifest, MINIMAL_MANIFEST).unwrap();
        std::fs::set_permissions(&manifest, std::fs::Permissions::from_mode(0o000)).unwrap();
        let result = plugin_dir(td.path());
        std::fs::set_permissions(&manifest, std::fs::Permissions::from_mode(0o644)).unwrap();
        let err = result.expect_err("unreadable manifest must fail");
        assert!(
            matches!(err, ValidationFailure::Io { .. }),
            "expected Io, got {err:?}"
        );
    }

    // -- success payload ----------------------------------------------------

    #[test]
    fn success_multi_file_yields_entry_point_multi() {
        let td = tempfile::tempdir().unwrap();
        write(td.path(), "manifest.toml", MINIMAL_MANIFEST);
        write(
            td.path(),
            "__init__.py",
            "def process_writes(a,b,c): pass\n",
        );
        write(td.path(), "helper.py", "def helper(): pass\n");
        let validated = plugin_dir(td.path()).expect("valid multi-file plugin");
        assert_eq!(validated.entry_point, EntryPoint::Multi);
        assert_eq!(validated.manifest.plugin.name.as_str(), "test");
    }

    #[test]
    fn success_single_file_yields_entry_point_single() {
        let td = tempfile::tempdir().unwrap();
        write(td.path(), "manifest.toml", MINIMAL_MANIFEST);
        write(
            td.path(),
            "my_plugin.py",
            "def process_writes(a,b,c): pass\n",
        );
        let validated = plugin_dir(td.path()).expect("valid single-file plugin");
        assert_eq!(
            validated.entry_point,
            EntryPoint::Single {
                file_name: "my_plugin.py".into()
            }
        );
    }

    // -- plugin_dir_with_index uniqueness (G) -------------------------------

    fn build_index_with_one_entry(name: &str, version: &str) -> Index {
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
        Index::parse_json(&json).expect("fixture parses")
    }

    fn write_plugin_with_name(dir: &Path, name: &str, version: &str) {
        std::fs::create_dir_all(dir).unwrap();
        let manifest = format!(
            "manifest_schema_version = \"1.0\"\n\
             [plugin]\nname = \"{name}\"\nversion = \"{version}\"\ndescription = \"x\"\ntriggers = [\"process_writes\"]\n\
             [dependencies]\ndatabase_version = \">=3.0.0\"\n"
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
        let ValidationFailure::Invalid(errs) = err else {
            panic!("expected Invalid, got {err:?}");
        };
        assert_eq!(errs.len(), 1);
        let ValidationError::NameVersionConflict { name, version } = &errs[0] else {
            panic!("expected NameVersionConflict, got {:?}", errs[0]);
        };
        assert_eq!(name, manifest_name, "diagnostic pins the manifest spelling");
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
        let validated = plugin_dir_with_index(td.path(), &index).expect("no collision");
        assert_eq!(validated.manifest.plugin.name.as_str(), "foo");
    }

    #[test]
    fn plugin_dir_with_index_no_collision_when_version_differs() {
        let td = tempfile::tempdir().unwrap();
        write_plugin_with_name(td.path(), "foo-bar", "0.2.0");
        let index = build_index_with_one_entry("foo_bar", "0.1.0");
        plugin_dir_with_index(td.path(), &index).expect("different versions, no collision");
    }
}
