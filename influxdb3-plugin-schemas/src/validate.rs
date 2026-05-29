//! Pure plugin-directory validation contract.
//!
//! This module defines *what a valid plugin is* — the layout rules
//! (entry-point classification) and the trigger-binding rule — as pure
//! functions over in-memory inputs. It is the shared contract that every
//! consumer (the packaging CLI today, the InfluxDB runtime later) must agree
//! on.
//!
//! # Purity
//!
//! This module is deliberately **pure**: no filesystem access, no Python
//! parser, no `tree-sitter` dependency. The heavyweight mechanism — walking a
//! directory and extracting top-level Python definitions — lives in
//! `influxdb3-plugin-sdk`, which feeds its results into the pure checks here.
//! A consumer that cannot take a `tree-sitter` dependency (or that loads
//! plugins from a non-filesystem source) implements the extraction rules
//! documented on [`TopLevelDef`] against its own parser and then calls
//! [`check_triggers`].
//!
//! # Extraction drift
//!
//! Because the extractor is not single-sourced (the SDK uses tree-sitter; a
//! runtime might use the CPython AST), the rules an extractor must satisfy are
//! captured three ways: the prose on [`TopLevelDef`], the SDK's reference
//! implementation, and the executable [`TOP_LEVEL_DEF_CORPUS`] that any
//! extractor's test suite can iterate to prove conformance.

use crate::{Manifest, ReportedError, TriggerType};

/// A validated plugin: the parsed manifest plus the classified entry point.
///
/// Returned by the SDK's `validate::plugin_dir` on success so callers don't
/// re-parse the TOML or re-derive the entry-point kind.
///
/// `#[non_exhaustive]`: fields may be added without a breaking change; use
/// [`ValidatedPlugin::new`] to construct.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ValidatedPlugin {
    /// The parsed manifest.
    pub manifest: Manifest,
    /// The classified entry point.
    pub entry_point: EntryPoint,
}

impl ValidatedPlugin {
    /// Constructs a [`ValidatedPlugin`].
    ///
    /// Required because `#[non_exhaustive]` blocks struct-literal construction
    /// from other crates (e.g. the SDK orchestrator).
    pub fn new(manifest: Manifest, entry_point: EntryPoint) -> Self {
        Self {
            manifest,
            entry_point,
        }
    }
}

/// The classified Python entry point of a plugin directory.
///
/// `#[non_exhaustive]`: variants may be added without a breaking change.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EntryPoint {
    /// Entry point is the sole top-level `.py` file (single-file plugin).
    Single { file_name: String },
    /// Entry point is `__init__.py` (multi-file plugin).
    Multi,
}

impl EntryPoint {
    /// The entry-point filename. Returns the constant `"__init__.py"` for
    /// [`EntryPoint::Multi`].
    pub fn file_name(&self) -> &str {
        match self {
            EntryPoint::Single { file_name } => file_name,
            EntryPoint::Multi => "__init__.py",
        }
    }
}

/// A top-level Python definition extracted from an entry-point source.
///
/// An extractor (e.g. `influxdb3-plugin-sdk`'s reference `extract_top_level_defs`)
/// **must** capture, in source order:
///
/// - a top-level `def foo(...)` — sync,
/// - a top-level `async def foo(...)` — async,
/// - a top-level decorated function (`@deco` then `def foo`) — a decorator is
///   not indirection.
///
/// It **must not** capture:
///
/// - class methods,
/// - nested definitions (a `def` inside another `def`),
/// - definitions guarded by `if`/`try`/etc. (not at module top level),
/// - re-exports / module-level assignments (`foo = bar`).
///
/// The extractor does **not** dedup: a redefined name appears once per
/// occurrence, in source order. Last-occurrence-wins resolution is the
/// responsibility of [`check_triggers`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopLevelDef {
    /// The function name.
    pub name: String,
    /// Whether the definition is `async def`.
    pub is_async: bool,
}

/// An individual validation failure.
///
/// The diagnostic type of the plugin-validation contract. Collected by the
/// SDK into a `ValidationReport` and surfaced together so multiple issues
/// appear in one pass.
///
/// `#[non_exhaustive]`: new variants may be added without a breaking change.
/// (On an enum, `#[non_exhaustive]` blocks exhaustive matching from outside
/// the crate, not construction of existing variants — so the SDK can still
/// build any variant.)
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ValidationError {
    /// Wraps a structural [`ReportedError`] from the schemas crate's
    /// two-phase parse (`Manifest::parse_toml` / `Index::parse_json`),
    /// preserving the field path and inner `SchemaError` losslessly so the
    /// CLI can render structural and cross-file diagnostics in one array.
    #[error(transparent)]
    SchemaReported(ReportedError),

    #[error("required file {file:?} is missing from the plugin directory")]
    MissingRequiredFile { file: String },

    #[error("{entry_point} does not parse as valid Python: {message}")]
    PythonParse {
        entry_point: String,
        message: String,
    },

    #[error(
        "trigger {trigger:?} is declared in manifest.toml but has no matching \
         top-level `def {}(...)` in {entry_point}",
        .trigger.as_str()
    )]
    TriggerNotImplemented {
        trigger: TriggerType,
        entry_point: String,
    },

    #[error(
        "trigger {trigger:?} is implemented as `async def` in {entry_point}; \
         the runtime invokes trigger functions synchronously"
    )]
    AsyncTriggerFn {
        trigger: TriggerType,
        entry_point: String,
    },

    #[error("no Python entry point found in the plugin directory (no .py files at the top level)")]
    NoEntryPoint,

    #[error(
        "multiple .py files found at the top level without __init__.py: {files:?}; add __init__.py for a multi-file plugin, or keep only one .py file"
    )]
    AmbiguousEntryPoint { files: Vec<String> },

    /// Plugin `(name, version)` already exists in the target index. Surfaces
    /// from the SDK's `validate::plugin_dir_with_index` so index-aware
    /// validation can collect uniqueness conflicts alongside other validation
    /// errors.
    #[error("plugin ({name:?}, {version:?}) already exists in the target index")]
    NameVersionConflict { name: String, version: String },
}

impl ValidationError {
    /// Stable tag per variant; backs the CLI's JSON code mapping. The
    /// exhaustive match forces new variants to be registered (drift guard).
    pub fn variant_name(&self) -> &'static str {
        match self {
            Self::SchemaReported(_) => "SchemaReported",
            Self::MissingRequiredFile { .. } => "MissingRequiredFile",
            Self::PythonParse { .. } => "PythonParse",
            Self::TriggerNotImplemented { .. } => "TriggerNotImplemented",
            Self::AsyncTriggerFn { .. } => "AsyncTriggerFn",
            Self::NoEntryPoint => "NoEntryPoint",
            Self::AmbiguousEntryPoint { .. } => "AmbiguousEntryPoint",
            Self::NameVersionConflict { .. } => "NameVersionConflict",
        }
    }
}

/// Classifies the entry point of a plugin directory from its top-level
/// regular-file names.
///
/// `file_names` must already be filtered to top-level regular files
/// (symlinks and subdirectories excluded — that filesystem mechanism is owned
/// by the SDK lister). Classification is exact and case-sensitive.
///
/// Rules:
///
/// - `__init__.py` present → [`EntryPoint::Multi`] (priority, regardless of
///   any other `.py` files).
/// - no `__init__.py`, exactly one `.py` → [`EntryPoint::Single`].
/// - no `__init__.py`, zero `.py` → [`ValidationError::NoEntryPoint`].
/// - no `__init__.py`, two or more `.py` →
///   [`ValidationError::AmbiguousEntryPoint`], with the file list **sorted
///   here** (the lister yields OS-dependent `read_dir` order; determinism is a
///   contract property the CLI snapshots depend on).
pub fn classify_entry_point(file_names: &[String]) -> Result<EntryPoint, ValidationError> {
    if file_names.iter().any(|name| name == "__init__.py") {
        return Ok(EntryPoint::Multi);
    }

    let mut py_files: Vec<String> = file_names
        .iter()
        .filter(|name| name.ends_with(".py"))
        .cloned()
        .collect();

    match py_files.len() {
        0 => Err(ValidationError::NoEntryPoint),
        1 => Ok(EntryPoint::Single {
            file_name: py_files.pop().unwrap(),
        }),
        _ => {
            py_files.sort();
            Err(ValidationError::AmbiguousEntryPoint { files: py_files })
        }
    }
}

/// Checks that each declared trigger has a matching top-level synchronous
/// `def` in the extracted definitions.
///
/// Resolution is **last-occurrence-wins** over `defs`: when a name is defined
/// more than once, the last matching entry decides sync/async. This mirrors
/// the historical `HashMap` last-insert behavior, including the case where a
/// sync `def` is later redefined as `async def` (the later `async` kind wins).
///
/// For each declared trigger, in manifest order:
///
/// - no matching def → [`ValidationError::TriggerNotImplemented`],
/// - matched by an `async def` → [`ValidationError::AsyncTriggerFn`],
/// - matched by a sync `def` → ok.
///
/// Extra defs not declared as triggers are ignored.
pub fn check_triggers(
    declared: &[TriggerType],
    defs: &[TopLevelDef],
    entry_point: &str,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    for trigger in declared {
        let expected = trigger.as_str();
        // Last-occurrence-wins: scan from the end for the first match.
        let resolved = defs.iter().rev().find(|d| d.name == expected);
        match resolved {
            None => errors.push(ValidationError::TriggerNotImplemented {
                trigger: *trigger,
                entry_point: entry_point.to_owned(),
            }),
            Some(def) if def.is_async => errors.push(ValidationError::AsyncTriggerFn {
                trigger: *trigger,
                entry_point: entry_point.to_owned(),
            }),
            Some(_) => {}
        }
    }
    errors
}

// ---------------------------------------------------------------------------
// Executable conformance corpus
// ---------------------------------------------------------------------------

/// One conformance case: a Python source and the extraction result any
/// conforming extractor must produce.
///
/// Pure data — `schemas` never parses Python. The corpus is `pub` so other
/// crates' test suites (the SDK's reference extractor today, a runtime's
/// extractor later) can iterate it and assert their extractor conforms.
#[derive(Debug, Clone, Copy)]
pub struct ConformanceCase {
    pub label: &'static str,
    pub source: &'static str,
    pub expected: Expectation,
}

/// The expected outcome of extracting top-level defs from a source.
#[derive(Debug, Clone, Copy)]
pub enum Expectation {
    /// Source parses; the extractor must return exactly these defs, in order.
    Defs(&'static [ExpectedDef]),
    /// Source is unparseable; the extractor must return a parse error.
    ParseError,
}

/// An expected top-level def in a [`ConformanceCase`].
///
/// Distinct from [`TopLevelDef`] by design: `ExpectedDef` uses `&'static str`
/// so the whole corpus can live in a `pub const`, whereas `TopLevelDef` uses
/// owned `String` because runtime extractors build it from arbitrary parsed
/// source. Same shape, different lifetimes — do not collapse.
#[derive(Debug, Clone, Copy)]
pub struct ExpectedDef {
    pub name: &'static str,
    pub is_async: bool,
}

/// Canonical input → expected-output table for top-level-def extraction.
///
/// Covers the F6–F15 extraction rules in one table. Any extractor must
/// satisfy every case; the SDK's `tests/conformance.rs` iterates this against
/// its reference extractor and a runtime would mirror that loop.
pub const TOP_LEVEL_DEF_CORPUS: &[ConformanceCase] = &[
    ConformanceCase {
        label: "plain_def",
        source: "def foo(): pass",
        expected: Expectation::Defs(&[ExpectedDef {
            name: "foo",
            is_async: false,
        }]),
    },
    ConformanceCase {
        label: "async_def",
        source: "async def foo(): pass",
        expected: Expectation::Defs(&[ExpectedDef {
            name: "foo",
            is_async: true,
        }]),
    },
    ConformanceCase {
        label: "decorated_def",
        source: "@staticmethod\ndef foo(): pass",
        expected: Expectation::Defs(&[ExpectedDef {
            name: "foo",
            is_async: false,
        }]),
    },
    ConformanceCase {
        label: "class_method",
        source: "class C:\n    def foo(self): pass",
        expected: Expectation::Defs(&[]),
    },
    ConformanceCase {
        label: "nested_def",
        source: "def outer():\n    def inner(): pass",
        expected: Expectation::Defs(&[ExpectedDef {
            name: "outer",
            is_async: false,
        }]),
    },
    ConformanceCase {
        label: "guarded_if",
        source: "if True:\n    def foo(): pass",
        expected: Expectation::Defs(&[]),
    },
    ConformanceCase {
        label: "reexport",
        source: "from bar import foo",
        expected: Expectation::Defs(&[]),
    },
    ConformanceCase {
        label: "assignment",
        source: "foo = bar",
        expected: Expectation::Defs(&[]),
    },
    ConformanceCase {
        label: "same_kind_redefinition",
        source: "def foo(): pass\ndef foo(): pass",
        expected: Expectation::Defs(&[
            ExpectedDef {
                name: "foo",
                is_async: false,
            },
            ExpectedDef {
                name: "foo",
                is_async: false,
            },
        ]),
    },
    ConformanceCase {
        label: "sync_then_async",
        source: "def foo(): pass\nasync def foo(): pass",
        expected: Expectation::Defs(&[
            ExpectedDef {
                name: "foo",
                is_async: false,
            },
            ExpectedDef {
                name: "foo",
                is_async: true,
            },
        ]),
    },
    ConformanceCase {
        label: "unparseable_source",
        source: "def foo(:",
        expected: Expectation::ParseError,
    },
    ConformanceCase {
        label: "empty_source",
        source: "",
        expected: Expectation::Defs(&[]),
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FieldPath, SchemaError};
    use pretty_assertions::assert_eq;

    // -- EntryPoint / ValidatedPlugin data shapes ---------------------------

    #[test]
    fn entry_point_file_name() {
        assert_eq!(
            EntryPoint::Single {
                file_name: "my_plugin.py".into()
            }
            .file_name(),
            "my_plugin.py"
        );
        assert_eq!(EntryPoint::Multi.file_name(), "__init__.py");
    }

    #[test]
    fn validated_plugin_new_constructs() {
        let manifest = Manifest::parse_toml(
            "manifest_schema_version = \"1.0\"\n\
             [plugin]\n\
             name = \"p\"\n\
             version = \"0.1.0\"\n\
             description = \"x\"\n\
             triggers = [\"process_writes\"]\n\
             [dependencies]\n\
             database_version = \">=3.0.0\"\n",
        )
        .expect("fixture manifest parses");
        let vp = ValidatedPlugin::new(manifest, EntryPoint::Multi);
        assert_eq!(vp.entry_point, EntryPoint::Multi);
        assert_eq!(vp.manifest.plugin.name.as_str(), "p");
    }

    // -- classify_entry_point  A1-A8 ----------------------------------------

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a1_init_py_present_is_multi() {
        assert_eq!(
            classify_entry_point(&names(&["__init__.py"])).unwrap(),
            EntryPoint::Multi
        );
    }

    #[test]
    fn a2_single_py_is_single() {
        assert_eq!(
            classify_entry_point(&names(&["plugin.py"])).unwrap(),
            EntryPoint::Single {
                file_name: "plugin.py".into()
            }
        );
    }

    #[test]
    fn a3_zero_py_is_no_entry_point() {
        let err = classify_entry_point(&names(&["README.md"])).unwrap_err();
        assert!(matches!(err, ValidationError::NoEntryPoint));
    }

    #[test]
    fn a4_multiple_py_is_ambiguous_sorted() {
        // Input deliberately unsorted; classify must sort.
        let err = classify_entry_point(&names(&["foo.py", "bar.py", "aaa.py"])).unwrap_err();
        match err {
            ValidationError::AmbiguousEntryPoint { files } => {
                assert_eq!(files, vec!["aaa.py", "bar.py", "foo.py"]);
            }
            other => panic!("expected AmbiguousEntryPoint, got {other:?}"),
        }
    }

    #[test]
    fn a5_non_py_files_ignored() {
        assert_eq!(
            classify_entry_point(&names(&["plugin.py", "requirements.txt", "README.md"])).unwrap(),
            EntryPoint::Single {
                file_name: "plugin.py".into()
            }
        );
    }

    #[test]
    fn a6_init_py_plus_helper_is_multi() {
        assert_eq!(
            classify_entry_point(&names(&["__init__.py", "helper.py"])).unwrap(),
            EntryPoint::Multi
        );
    }

    #[test]
    fn a7_bare_dot_py_counts() {
        assert_eq!(
            classify_entry_point(&names(&[".py"])).unwrap(),
            EntryPoint::Single {
                file_name: ".py".into()
            }
        );
    }

    #[test]
    fn a8_classification_is_case_sensitive() {
        // `Foo.PY` does not end with ".py"; `__INIT__.py` is not `__init__.py`.
        // `__INIT__.py` does end in ".py" though, so it's the sole single-file.
        assert_eq!(
            classify_entry_point(&names(&["__INIT__.py"])).unwrap(),
            EntryPoint::Single {
                file_name: "__INIT__.py".into()
            }
        );
        let err = classify_entry_point(&names(&["Foo.PY"])).unwrap_err();
        assert!(
            matches!(err, ValidationError::NoEntryPoint),
            "Foo.PY must not be recognized as a .py file"
        );
    }

    // -- check_triggers  F1-F5, F13 -----------------------------------------

    fn def(name: &str, is_async: bool) -> TopLevelDef {
        TopLevelDef {
            name: name.into(),
            is_async,
        }
    }

    #[test]
    fn f1_sync_def_matches_trigger() {
        let errs = check_triggers(
            &[TriggerType::ProcessWrites],
            &[def("process_writes", false)],
            "__init__.py",
        );
        assert!(errs.is_empty(), "expected no errors, got {errs:?}");
    }

    #[test]
    fn f2_no_matching_def_is_not_implemented() {
        let errs = check_triggers(
            &[TriggerType::ProcessWrites],
            &[def("something_else", false)],
            "__init__.py",
        );
        assert_eq!(errs.len(), 1);
        assert!(matches!(
            errs[0],
            ValidationError::TriggerNotImplemented {
                trigger: TriggerType::ProcessWrites,
                ..
            }
        ));
    }

    #[test]
    fn f3_async_def_is_async_trigger_fn() {
        let errs = check_triggers(
            &[TriggerType::ProcessWrites],
            &[def("process_writes", true)],
            "__init__.py",
        );
        assert_eq!(errs.len(), 1);
        assert!(matches!(errs[0], ValidationError::AsyncTriggerFn { .. }));
    }

    #[test]
    fn f4_multiple_bad_triggers_all_reported() {
        let errs = check_triggers(
            &[
                TriggerType::ProcessWrites,
                TriggerType::ProcessScheduledCall,
                TriggerType::ProcessRequest,
            ],
            &[def("unrelated", false)],
            "__init__.py",
        );
        assert_eq!(errs.len(), 3);
    }

    #[test]
    fn f5_extra_defs_ignored() {
        let errs = check_triggers(
            &[TriggerType::ProcessWrites],
            &[def("process_writes", false), def("helper", false)],
            "__init__.py",
        );
        assert!(errs.is_empty());
    }

    #[test]
    fn f13_redefinition_last_wins_sync_only() {
        // Two sync defs: last wins, still sync → ok.
        let errs = check_triggers(
            &[TriggerType::ProcessWrites],
            &[def("process_writes", false), def("process_writes", false)],
            "__init__.py",
        );
        assert!(errs.is_empty());
    }

    #[test]
    fn f13_redefinition_last_wins_sync_then_async() {
        // Sync then async: later async kind wins → AsyncTriggerFn.
        let errs = check_triggers(
            &[TriggerType::ProcessWrites],
            &[def("process_writes", false), def("process_writes", true)],
            "__init__.py",
        );
        assert_eq!(errs.len(), 1);
        assert!(matches!(errs[0], ValidationError::AsyncTriggerFn { .. }));
    }

    #[test]
    fn f13_redefinition_last_wins_async_then_sync() {
        // Async then sync: later sync kind wins → ok.
        let errs = check_triggers(
            &[TriggerType::ProcessWrites],
            &[def("process_writes", true), def("process_writes", false)],
            "__init__.py",
        );
        assert!(errs.is_empty());
    }

    // -- ValidationError display + variant tags (moved from sdk) -------------

    fn every_validation_variant() -> Vec<ValidationError> {
        vec![
            ValidationError::SchemaReported(ReportedError::new(
                FieldPath::root().field("plugin").field("description"),
                SchemaError::DescriptionEmpty,
            )),
            ValidationError::MissingRequiredFile {
                file: "__init__.py".into(),
            },
            ValidationError::PythonParse {
                entry_point: "__init__.py".into(),
                message: "unexpected token".into(),
            },
            ValidationError::TriggerNotImplemented {
                trigger: TriggerType::ProcessWrites,
                entry_point: "__init__.py".into(),
            },
            ValidationError::AsyncTriggerFn {
                trigger: TriggerType::ProcessScheduledCall,
                entry_point: "__init__.py".into(),
            },
            ValidationError::NoEntryPoint,
            ValidationError::AmbiguousEntryPoint {
                files: vec!["a.py".into(), "b.py".into()],
            },
            ValidationError::NameVersionConflict {
                name: "downsampler".into(),
                version: "1.2.0".into(),
            },
        ]
    }

    #[test]
    fn every_validation_variant_covered() {
        // Sanity: the fixture lists one of every variant_name.
        let tags: Vec<&'static str> = every_validation_variant()
            .iter()
            .map(ValidationError::variant_name)
            .collect();
        assert_eq!(tags.len(), 8);
    }

    #[test]
    fn validation_error_display_stable() {
        let rendered: Vec<String> = every_validation_variant()
            .iter()
            .map(|e| e.to_string())
            .collect();
        insta::assert_yaml_snapshot!("validation_error_display", rendered);
    }

    #[test]
    fn validation_error_variant_tags_stable() {
        let tags: Vec<&'static str> = every_validation_variant()
            .iter()
            .map(ValidationError::variant_name)
            .collect();
        insta::assert_yaml_snapshot!("validation_error_variant_tags", tags);
    }

    /// `ValidationError::SchemaReported` wraps `ReportedError` losslessly
    /// (path + inner variant), so downstream callers can pattern-match on
    /// the original `SchemaError` variant and field path.
    #[test]
    fn schemas_error_structured_payload_preserved_via_validation_schema_reported() {
        let reported = ReportedError::new(
            FieldPath::root().field("plugin").field("description"),
            SchemaError::DescriptionEmpty,
        );
        let wrapped = ValidationError::SchemaReported(reported);
        match &wrapped {
            ValidationError::SchemaReported(r) => {
                assert_eq!(r.path.as_str(), "plugin.description");
                assert!(matches!(r.error, SchemaError::DescriptionEmpty));
            }
            other => panic!("expected SchemaReported, got {other:?}"),
        }
    }

    // -- conformance corpus sanity ------------------------------------------

    #[test]
    fn corpus_labels_unique() {
        let mut labels: Vec<&str> = TOP_LEVEL_DEF_CORPUS.iter().map(|c| c.label).collect();
        let count = labels.len();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), count, "corpus labels must be unique");
    }

    #[test]
    fn corpus_covers_required_rules() {
        let labels: Vec<&str> = TOP_LEVEL_DEF_CORPUS.iter().map(|c| c.label).collect();
        for required in [
            "plain_def",
            "async_def",
            "decorated_def",
            "class_method",
            "nested_def",
            "guarded_if",
            "reexport",
            "assignment",
            "same_kind_redefinition",
            "sync_then_async",
            "unparseable_source",
            "empty_source",
        ] {
            assert!(
                labels.contains(&required),
                "corpus missing required case `{required}`"
            );
        }
    }
}
