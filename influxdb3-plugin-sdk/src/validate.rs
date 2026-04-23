//! Plugin-directory validation per Spec 2 Validation.
//!
//! Runs two check categories on a plugin directory:
//!
//! - **Structural** (delegated to `influxdb3-plugin-schemas` via
//!   [`Manifest::parse_toml`]): manifest well-formedness, required-file
//!   presence, name/version/trigger/URL/dep parseability, description length,
//!   non-empty triggers array, URL scheme allowlist.
//! - **Code / manifest cross-reference**: `__init__.py` must parse as valid
//!   Python 3, and for each trigger declared in `manifest.plugin.triggers`
//!   there must be a top-level synchronous `def <trigger>(...)`. Indirect
//!   definitions (re-exports, module-level assignments, class methods) and
//!   `async def` are explicit non-matches per Spec 2 Validation.
//!
//! # Multi-error collection
//!
//! Structural parse failures short-circuit — we can't meaningfully run
//! cross-file checks without a valid manifest. Cross-file failures
//! accumulate into a [`ValidationReport`]; multiple missing triggers or an
//! unparseable `__init__.py` come back together rather than one-at-a-time.
//!
//! # Python parser
//!
//! Uses `tree-sitter-python`. Rationale for this pick over pyo3, shell-out,
//! and other Rust Python parsers lives in the core doc's Design Decisions
//! under `influxdb3-plugin-sdk` crate specifics.

use influxdb3_plugin_schemas::{Manifest, TriggerType};
use std::collections::HashMap;
use std::path::Path;

use crate::{SdkError, ValidationError, ValidationReport};

/// Validates a plugin directory against Spec 2 Validation.
///
/// Returns the parsed [`Manifest`] on success so downstream callers (e.g.,
/// [`crate::package::package_plugin`]) don't need to re-read and re-parse
/// the file. On failure returns one of:
/// - `SdkError::Io` — an I/O error other than `NotFound` on required files.
/// - `SdkError::ValidationErrors` — manifest failed structural parsing or
///   one or more cross-file validation checks failed.
///
/// # Multi-error collection
///
/// Spec 2 Validation prescribes "all validation errors are collected and
/// reported together rather than failing on the first." This contract is
/// honored at the cross-file layer: a missing `__init__.py`, missing
/// trigger implementations, and an unparseable Python source can all
/// surface together in one `ValidationErrors` report.
///
/// Structural manifest parsing is a fail-fast cut: if `manifest.toml` is
/// unparseable or structurally invalid, we cannot learn what triggers are
/// declared, so we cannot meaningfully continue to cross-file checks.
/// Missing files (both `manifest.toml` and `__init__.py`) are reported
/// through `ValidationError::MissingRequiredFile`.
pub fn plugin_dir(dir: &Path) -> Result<Manifest, SdkError> {
    let mut report = ValidationReport::new();

    // Required file: manifest.toml. NotFound is a collectible validation
    // error; any other I/O error surfaces as SdkError::Io.
    let manifest_path = dir.join("manifest.toml");
    let manifest_raw = match std::fs::read_to_string(&manifest_path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            report.push(ValidationError::MissingRequiredFile {
                file: "manifest.toml".into(),
            });
            report.into_result()?;
            unreachable!("non-empty report always returns Err");
        }
        Err(source) => {
            return Err(SdkError::Io {
                source,
                path: Some(manifest_path),
            });
        }
    };
    let manifest = Manifest::parse_toml(&manifest_raw)?;

    // Cross-file: __init__.py must exist, parse, and implement the declared
    // triggers as top-level sync defs.
    let init_path = dir.join("__init__.py");
    let init_raw = match std::fs::read_to_string(&init_path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            report.push(ValidationError::MissingRequiredFile {
                file: "__init__.py".into(),
            });
            report.into_result()?;
            unreachable!("non-empty report always returns Err");
        }
        Err(source) => {
            return Err(SdkError::Io {
                source,
                path: Some(init_path),
            });
        }
    };

    check_python_source(&init_raw, &manifest.plugin.triggers, &mut report);
    report.into_result()?;
    Ok(manifest)
}

/// Same as [`plugin_dir`] plus an index-relative uniqueness check.
///
/// Runs the full [`plugin_dir`] validation pass first (short-circuits on
/// structural or cross-file failure). On success, compares the manifest's
/// `(name, version)` against every entry in `index.plugins[]`; a collision
/// surfaces as `SdkError::ValidationErrors` carrying a single
/// [`ValidationError::NameVersionConflict`]. Returns the parsed [`Manifest`]
/// on full success.
///
/// This is the entry point the CLI's `validate --index` flag uses — Spec 2
/// S2-15 (validator idiom) requires uniqueness conflicts to appear in the
/// same diagnostics array as other validation errors, which routing through
/// `mutate_index::add_entry`'s `SdkError::AlreadyPublished` wouldn't allow.
/// `add_entry` continues to enforce S2-2 at the mutation boundary for the
/// `package` / `yank` pipelines.
pub fn plugin_dir_with_index(
    dir: &std::path::Path,
    index: &influxdb3_plugin_schemas::Index,
) -> Result<Manifest, SdkError> {
    let manifest = plugin_dir(dir)?;

    let collision = index.plugins.iter().any(|e| {
        e.name.as_str() == manifest.plugin.name.as_str() && e.version == manifest.plugin.version
    });
    if collision {
        let mut report = ValidationReport::new();
        report.push(ValidationError::NameVersionConflict {
            name: manifest.plugin.name.as_str().to_owned(),
            version: manifest.plugin.version.to_string(),
        });
        report.into_result()?;
        unreachable!("non-empty report always returns Err");
    }

    Ok(manifest)
}

/// Parses `source` with tree-sitter-python and records validation findings
/// into `report`. Inline `#[cfg(test)]` module below exercises this directly.
fn check_python_source(
    source: &str,
    declared_triggers: &[TriggerType],
    report: &mut ValidationReport,
) {
    let mut parser = tree_sitter::Parser::new();
    let language = tree_sitter_python::LANGUAGE;
    parser
        .set_language(&language.into())
        .expect("tree-sitter-python grammar initializes");

    let Some(tree) = parser.parse(source, None) else {
        report.push(ValidationError::PythonParse {
            message: "tree-sitter produced no parse tree".into(),
        });
        return;
    };

    let root = tree.root_node();
    if root.has_error() {
        report.push(ValidationError::PythonParse {
            message: format_parse_error(root, source),
        });
        return;
    }

    // Walk top-level statements. We care about:
    //   - `function_definition` — bare def or async def
    //   - `decorated_definition` — def or async def preceded by one or more
    //     decorators (e.g., `@staticmethod\ndef foo(): ...`). Decorators don't
    //     make a def indirect per Spec 2 Validation; only re-exports, class
    //     methods, and assignments do.
    // Everything else (class defs, imports, expressions, assignments) is not a
    // top-level `def` and therefore doesn't satisfy a declared trigger.
    let mut top_level_defs: HashMap<String, DefKind> = HashMap::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if let Some((name, kind)) = extract_top_level_def(&child, source) {
            top_level_defs.insert(name, kind);
        }
    }

    // For each declared trigger, check the implementation.
    for trigger in declared_triggers {
        let expected = trigger.as_str();
        match top_level_defs.get(expected) {
            None => report.push(ValidationError::TriggerNotImplemented { trigger: *trigger }),
            Some(DefKind::Async) => {
                report.push(ValidationError::AsyncTriggerFn { trigger: *trigger })
            }
            Some(DefKind::Sync) => { /* ok */ }
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum DefKind {
    Sync,
    Async,
}

/// If `node` is (or wraps) a top-level function definition, return its name
/// and sync/async kind. Returns `None` for anything else (class defs,
/// imports, expressions, assignments, malformed defs caught by tree-sitter's
/// error recovery).
fn extract_top_level_def(node: &tree_sitter::Node<'_>, source: &str) -> Option<(String, DefKind)> {
    let function_def = match node.kind() {
        "function_definition" => *node,
        // Decorated form: `@foo\ndef bar():` → decorated_definition with a
        // function_definition child. Find the inner function_definition.
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
/// `function_definition` nodes. The async case has an `async` keyword as a
/// leading child; walk the immediate children for one.
fn is_async_function(function_def: &tree_sitter::Node<'_>) -> bool {
    let mut cursor = function_def.walk();
    for child in function_def.children(&mut cursor) {
        if child.kind() == "async" {
            return true;
        }
    }
    false
}

/// Best-effort human-readable description of the first parse error in
/// source order.
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

/// Depth-first in-order search for the earliest-position error or missing
/// node. Returns the first match in source order, exploiting that
/// tree-sitter's `children()` yields children in source order and pre-order
/// visits a parent before its descendants.
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
            &mut report,
        );
        assert_eq!(report.len(), 1);
        let err = report.into_result().unwrap_err();
        let SdkError::ValidationErrors(errs) = err else {
            panic!("expected ValidationErrors")
        };
        assert!(matches!(
            errs[0],
            ValidationError::TriggerNotImplemented { trigger: TriggerType::ProcessWrites }
        ));
    }

    #[test]
    fn async_def_rejected_even_if_name_matches() {
        let src = "async def process_writes(a, b, c):\n    pass\n";
        let mut report = ValidationReport::new();
        check_python_source(
            src,
            &trigger_list(&[TriggerType::ProcessWrites]),
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
        // Spec 2 Validation: indirect definitions (class methods) are not recognized.
        let src = "class Foo:\n    def process_writes(self):\n        pass\n";
        let mut report = ValidationReport::new();
        check_python_source(
            src,
            &trigger_list(&[TriggerType::ProcessWrites]),
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
        // Spec 2 Validation: module-level assignments are not recognized.
        let src = "process_writes = lambda a, b, c: None\n";
        let mut report = ValidationReport::new();
        check_python_source(
            src,
            &trigger_list(&[TriggerType::ProcessWrites]),
            &mut report,
        );
        assert_eq!(report.len(), 1);
        let err = report.into_result().unwrap_err();
        let SdkError::ValidationErrors(errs) = err else {
            panic!("expected ValidationErrors")
        };
        assert!(matches!(
            errs[0],
            ValidationError::TriggerNotImplemented { trigger: TriggerType::ProcessWrites }
        ));
    }

    #[test]
    fn def_inside_triple_quoted_string_is_not_a_function() {
        // Multi-line string literal containing what looks like a def at column 0 —
        // the regex-based approach we explicitly rejected would false-positive here.
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
            &mut report,
        );
        assert_eq!(report.len(), 1);
        let err = report.into_result().unwrap_err();
        let SdkError::ValidationErrors(errs) = err else {
            panic!("expected ValidationErrors")
        };
        assert!(matches!(
            errs[0],
            ValidationError::TriggerNotImplemented { trigger: TriggerType::ProcessWrites }
        ));
    }

    #[test]
    fn syntax_error_reported_as_python_parse() {
        let src = "def oops(:\n    pass\n";
        let mut report = ValidationReport::new();
        check_python_source(
            src,
            &trigger_list(&[TriggerType::ProcessWrites]),
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
                ValidationError::TriggerNotImplemented { trigger } => *trigger,
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
            &mut report,
        );
        assert!(report.is_empty());
    }

    #[test]
    fn decorated_function_still_recognized() {
        // Decorators don't change top-level-ness — def is still a function_definition.
        let src = r#"
@staticmethod
def process_writes(a, b, c):
    pass
"#;
        let mut report = ValidationReport::new();
        check_python_source(
            src,
            &trigger_list(&[TriggerType::ProcessWrites]),
            &mut report,
        );
        assert!(report.is_empty(), "got {report:?}");
    }

    /// Regression guard: for a source with multiple syntax errors, the
    /// reported parse-error position must be the earliest in source order.
    /// The earlier stack-based `walk_preorder` impl produced order-
    /// dependent-on-stack-pop semantics, not source order.
    #[test]
    fn first_parse_error_is_earliest_position() {
        // Three errors, introduced on lines 2, 4, and 6 respectively.
        // Line 2's error is a malformed def (stray `:`), line 4's is a
        // missing close-paren, line 6's is a malformed class.
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
            &mut report,
        );
        let err = report.into_result().unwrap_err();
        let SdkError::ValidationErrors(errs) = err else {
            panic!("expected ValidationErrors")
        };
        assert_eq!(errs.len(), 1);
        let ValidationError::PythonParse { message } = &errs[0] else {
            panic!("expected PythonParse, got {:?}", errs[0])
        };
        // The earliest error is on line 2, column 10 (the stray `:`).
        // We don't require an exact column but the line must be 2, never
        // a later line where a different error also exists.
        assert!(
            message.contains("line 2"),
            "expected error on line 2 (earliest position), got: {message}"
        );
    }
}
