//! JSON-mode output schemas per command.
//!
//! Every struct here is part of `influxdb3-plugin-cli`'s semver-stable
//! contract. Adding fields is a minor bump; renaming, removing, repurposing,
//! or narrowing the type of an existing field is a major bump. Consumers
//! must ignore unknown fields.
//!
//! No `output_schema_version` field is embedded — consumers pin via the
//! crate's published version.

use serde::Serialize;
use std::path::PathBuf;

/// `--output json` payload emitted by `yank` on success.
/// `outcome` collapses the (target_state, transition vs no-op) cross
/// product into one four-case enum. The wire form is the snake_case
/// representation.
#[derive(Debug, Serialize)]
pub(crate) struct YankOutput {
    pub name: String,
    pub version: String,
    pub published_at: String,
    pub outcome: YankOutcomeWire,
    pub index_path: PathBuf,
}

/// Wire-stable enum for `YankOutput.outcome`. The four values cover
/// every (action × pre-existing-state) cross product:
/// - `Yanked` — yank operation that actually flipped the bit.
/// - `Unyanked` — `--undo` operation that actually flipped the bit.
/// - `AlreadyYanked` — yank operation, entry was already yanked (no-op).
/// - `AlreadyUnyanked` — `--undo` operation, entry was already not yanked (no-op).
///
/// Stable wire enum.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum YankOutcomeWire {
    Yanked,
    Unyanked,
    AlreadyYanked,
    AlreadyUnyanked,
}

/// `--output json` payload emitted by `package` on success. Carries the
/// absolute paths of the artifact + derived index, the artifact's SHA-256
/// hash, and the new entry's identity and publication timestamp.
#[derive(Debug, Serialize)]
pub(crate) struct PackageOutput {
    pub artifact_path: PathBuf,
    pub index_path: PathBuf,
    pub hash: String,
    pub new_entry_name: String,
    pub new_entry_version: String,
    pub new_entry_published_at: String,
}

/// `--output json` payload emitted by `search` on success.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct IndexSearchOutput {
    pub hits: Vec<IndexSearchHitOutput>,
}

/// One projected row in a `search` JSON result.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct IndexSearchHitOutput {
    pub name: String,
    pub version: String,
    pub published_at: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub visibility: IndexVisibilityOutput,
}

/// CLI-owned visibility projection for index inspection output.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum IndexVisibilityOutput {
    Visible,
    Hidden {
        reasons: Vec<IndexVisibilityReasonOutput>,
    },
}

/// CLI-owned hidden-result reason projection for index inspection output.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum IndexVisibilityReasonOutput {
    Yanked,
    IncompatibleDatabaseVersion { required: String, actual: String },
}

/// `--output json` payload emitted by `info` on success.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub(crate) enum IndexInfoOutput {
    Found {
        plugin: Box<IndexInfoPluginOutput>,
    },
    NotFound {
        name: String,
        version: Option<String>,
    },
    FilteredOut {
        name: String,
        version: Option<String>,
        reasons: Vec<IndexVisibilityReasonOutput>,
    },
}

/// Full plugin metadata projected by `info`.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct IndexInfoPluginOutput {
    pub name: String,
    pub version: String,
    pub published_at: String,
    pub description: String,
    pub triggers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    pub artifact_url: String,
    pub dependencies: IndexDependenciesOutput,
    pub hash: String,
    pub visibility: IndexVisibilityOutput,
}

/// Dependency metadata projected by `info`.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct IndexDependenciesOutput {
    pub database_version: String,
    pub python: Vec<String>,
}

/// `--output json` payload emitted by `new list`. Stable (semver-locked)
/// schema.
#[derive(Debug, Serialize)]
pub(crate) struct ListOutput {
    pub templates: Vec<ListTemplate>,
}

/// One row of [`ListOutput`]. `name` is the human-facing display name;
/// `short_name` is the CLI arg a user would pass to `new`.
#[derive(Debug, Serialize)]
pub(crate) struct ListTemplate {
    pub name: &'static str,
    pub short_name: &'static str,
}

/// `--output json` payload emitted by `new` on success: stdout carries this
/// single document; failure paths leave stdout empty and write the error
/// to stderr.
#[derive(Debug, Serialize)]
pub(crate) struct NewOutput {
    /// `"plugin"` for trigger templates, `"index"` for the index
    /// template. Stable string tag; consumers can pattern-match.
    pub kind: &'static str,
    /// The template identifier the user passed (`process_writes`,
    /// `process_scheduled_call`, `process_request`, or `index`).
    pub template: &'static str,
    /// Absolute path of the directory the scaffold wrote into.
    pub target_dir: PathBuf,
    /// Plugin name written into `manifest.toml` for plugin templates;
    /// omitted for index templates, which carry no plugin name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Files the scaffold wrote, relative to `target_dir`. Order matches
    /// the SDK scaffold's documented write order.
    pub files_written: Vec<PathBuf>,
}

/// Universal `--output json` envelope. Every command emits exactly one
/// document of this shape on stdout. `R` is the per-command success
/// payload type; failure paths use the `Error` variant whose payload type
/// is fixed (`JsonError`).
///
/// Serialized form:
/// - `Envelope::Ok { result }`  → `{"status":"ok","result":{...}}`
/// - `Envelope::Error { error }` → `{"status":"error","error":{...}}`
///
#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub(crate) enum Envelope<R: Serialize> {
    Ok { result: R },
    Error { error: JsonError },
}

/// Structured error payload for `Envelope::Error`. Carries the stable
/// `code`, human `message`, and optional structured fields.
#[derive(Debug, Clone, Serialize)]
pub struct JsonError {
    /// Stable namespaced identifier from a closed enum.
    pub code: String,
    /// Source error's `Display` text, English.
    pub message: String,
    /// Dotted-path location, filename, or target identifier when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// Variant-specific structured payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    /// Sub-error array used by `validate::failed` and the `<command>::index_parse_failed` codes.
    /// Sub-elements are themselves `JsonError` but never carry their own `diagnostics` or `cause`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<JsonError>,
    /// `Error::source()` chain rendered as Display strings, outermost-first
    /// below the top-level message.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cause: Vec<String>,
}

/// Empty named-field struct used as the success result for `validate`.
/// Serializes as `{}` (empty object). A unit struct or `()` would
/// serialize as `null`, which the envelope contract forbids.
#[derive(Debug, Serialize)]
pub(crate) struct ValidateResult {}

/// Writes `Envelope::Ok { result }` as compact JSON with a single
/// trailing `\n`. Used by every command's success path.
pub(crate) fn write_envelope_ok<W: std::io::Write, R: Serialize>(
    writer: &mut W,
    result: R,
) -> std::io::Result<()> {
    let env = Envelope::Ok { result };
    serde_json::to_writer(&mut *writer, &env).map_err(std::io::Error::other)?;
    writer.write_all(b"\n")
}

/// Writes `Envelope::Error { error }` as compact JSON with a single
/// trailing `\n`. Used by `main.rs`'s error dispatch.
pub fn write_envelope_error<W: std::io::Write>(
    writer: &mut W,
    error: &JsonError,
) -> std::io::Result<()> {
    let env: Envelope<()> = Envelope::Error {
        error: error.clone(),
    };
    serde_json::to_writer(&mut *writer, &env).map_err(std::io::Error::other)?;
    writer.write_all(b"\n")
}

#[cfg(test)]
mod envelope_tests {
    use super::*;
    use serde::Serialize;

    const ALL_WIRE_CODES: &[&str] = &[
        "validate::failed",
        "validate::schema_reported",
        "validate::missing_required_file",
        "validate::python_parse",
        "validate::trigger_not_implemented",
        "validate::async_trigger_fn",
        "validate::name_version_conflict",
        "validate::index_read_failed",
        "validate::schema_error",
        "validate::io_failed",
        "validate::invalid_exclude_pattern",
        "validate::sdk_error",
        "package::canonical_collision",
        "package::already_published",
        "package::path_too_long",
        "package::archive_failed",
        "package::hash_failed",
        "package::schema_error",
        "package::path_overlap",
        "package::index_parse_failed",
        "package::io_failed",
        "package::invalid_exclude_pattern",
        "package::sdk_error",
        "index::index_read_failed",
        "index::index_parse_failed",
        "yank::entry_not_found",
        "yank::index_parse_failed",
        "yank::schema_error",
        "yank::io_failed",
        "yank::sdk_error",
        "new::scaffold_failed",
        "new::derived_name_invalid",
        "new::derived_name_unavailable",
        "new::path_resolution_failed",
        "new::sdk_error",
        "io::read_failed",
        "io::write_failed",
        "io::canonicalize_failed",
        "path::resolution_failed",
        "usage::missing_required_argument",
        "usage::invalid_value",
        "usage::value_validation",
        "usage::unknown_argument",
        "usage::invalid_subcommand",
        "usage::missing_subcommand",
        "usage::too_many_values",
        "usage::too_few_values",
        "usage::parse_error",
        "usage::invalid_name",
        "usage::invalid_artifacts_url",
        "usage::invalid_database_version",
        "usage::invalid_target",
        "usage::input_output_overlap",
        "usage::sibling_canonical_collision",
        "cli::unknown",
    ];

    #[derive(Serialize)]
    struct Demo {
        a: u32,
    }

    #[test]
    fn json_envelope_ok_serializes_shape() {
        let env = Envelope::Ok {
            result: Demo { a: 7 },
        };
        let s = serde_json::to_string(&env).unwrap();
        assert_eq!(s, r#"{"status":"ok","result":{"a":7}}"#);
    }

    #[test]
    fn json_envelope_error_serializes_shape() {
        let env: Envelope<()> = Envelope::Error {
            error: JsonError {
                code: "x::y".into(),
                message: "msg".into(),
                field: None,
                details: None,
                diagnostics: vec![],
                cause: vec![],
            },
        };
        let s = serde_json::to_string(&env).unwrap();
        assert_eq!(
            s,
            r#"{"status":"error","error":{"code":"x::y","message":"msg"}}"#
        );
    }

    #[test]
    fn json_error_omits_empty_optional_fields() {
        let e = JsonError {
            code: "c".into(),
            message: "m".into(),
            field: None,
            details: None,
            diagnostics: vec![],
            cause: vec![],
        };
        let s = serde_json::to_string(&e).unwrap();
        assert_eq!(s, r#"{"code":"c","message":"m"}"#);
    }

    #[test]
    fn json_error_keeps_non_empty_fields() {
        let e = JsonError {
            code: "c".into(),
            message: "m".into(),
            field: Some("f".into()),
            details: Some(serde_json::json!({"k": "v"})),
            diagnostics: vec![JsonError {
                code: "sub::a".into(),
                message: "sm".into(),
                field: None,
                details: None,
                diagnostics: vec![],
                cause: vec![],
            }],
            cause: vec!["root cause".into()],
        };
        let s = serde_json::to_string(&e).unwrap();
        assert!(s.contains(r#""field":"f""#));
        assert!(s.contains(r#""details":{"k":"v"}"#));
        assert!(s.contains(r#""diagnostics":[{"code":"sub::a","message":"sm"}]"#));
        assert!(s.contains(r#""cause":["root cause"]"#));
    }

    #[test]
    fn validate_success_result_is_empty_object() {
        let env = Envelope::Ok {
            result: ValidateResult {},
        };
        let s = serde_json::to_string(&env).unwrap();
        assert_eq!(s, r#"{"status":"ok","result":{}}"#);
    }

    #[test]
    fn write_envelope_ok_writes_compact_with_trailing_newline() {
        let mut buf = Vec::new();
        write_envelope_ok(&mut buf, Demo { a: 1 }).unwrap();
        assert_eq!(buf, b"{\"status\":\"ok\",\"result\":{\"a\":1}}\n");
    }

    #[test]
    fn write_envelope_error_writes_compact_with_trailing_newline() {
        let err = JsonError {
            code: "c".into(),
            message: "m".into(),
            field: None,
            details: None,
            diagnostics: vec![],
            cause: vec![],
        };
        let mut buf = Vec::new();
        write_envelope_error(&mut buf, &err).unwrap();
        assert_eq!(
            buf,
            b"{\"status\":\"error\",\"error\":{\"code\":\"c\",\"message\":\"m\"}}\n"
        );
    }

    #[test]
    fn json_output_is_compact_single_newline() {
        let mut buf = Vec::new();
        write_envelope_ok(&mut buf, Demo { a: 0 }).unwrap();
        let s = std::str::from_utf8(&buf).unwrap();
        assert!(!s.contains('\n') || s.ends_with('\n'));
        assert_eq!(s.matches('\n').count(), 1);
        assert!(!s.contains("  "));
    }

    #[test]
    fn yank_output_outcome_serializes_four_case_enum() {
        let payload = YankOutput {
            name: "p".into(),
            version: "1.0.0".into(),
            published_at: "2026-04-29T18:45:12Z".into(),
            outcome: YankOutcomeWire::Yanked,
            index_path: std::path::PathBuf::from("/abs/idx.json"),
        };
        let s = serde_json::to_string(&payload).unwrap();
        assert!(s.contains(r#""outcome":"yanked""#));
        assert!(!s.contains("target_state"));
    }

    #[test]
    fn yank_outcome_values_stable() {
        let cases = [
            YankOutcomeWire::Yanked,
            YankOutcomeWire::Unyanked,
            YankOutcomeWire::AlreadyYanked,
            YankOutcomeWire::AlreadyUnyanked,
        ];
        let strings: Vec<String> = cases
            .iter()
            .map(|c| serde_json::to_string(c).unwrap())
            .collect();
        assert_eq!(
            strings,
            vec![
                r#""yanked""#,
                r#""unyanked""#,
                r#""already_yanked""#,
                r#""already_unyanked""#,
            ],
        );
    }

    #[test]
    fn code_allocations_stable() {
        insta::assert_yaml_snapshot!(ALL_WIRE_CODES);
    }

    #[test]
    fn envelope_field_names_stable() {
        let ok = serde_json::to_value(Envelope::Ok {
            result: Demo { a: 0 },
        })
        .unwrap();
        let err: serde_json::Value = serde_json::to_value(Envelope::<()>::Error {
            error: JsonError {
                code: "c".into(),
                message: "m".into(),
                field: None,
                details: None,
                diagnostics: vec![],
                cause: vec![],
            },
        })
        .unwrap();
        let ok_keys: Vec<_> = ok.as_object().unwrap().keys().cloned().collect();
        let err_keys: Vec<_> = err.as_object().unwrap().keys().cloned().collect();
        insta::assert_yaml_snapshot!("envelope_ok_keys", ok_keys);
        insta::assert_yaml_snapshot!("envelope_error_keys", err_keys);
    }

    #[test]
    fn json_error_field_names_stable() {
        let e = JsonError {
            code: "c".into(),
            message: "m".into(),
            field: Some("f".into()),
            details: Some(serde_json::json!({})),
            diagnostics: vec![JsonError {
                code: "s".into(),
                message: "sm".into(),
                field: None,
                details: None,
                diagnostics: vec![],
                cause: vec![],
            }],
            cause: vec!["c1".into()],
        };
        let v = serde_json::to_value(&e).unwrap();
        let keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        insta::assert_yaml_snapshot!("json_error_keys", keys);
    }
}
