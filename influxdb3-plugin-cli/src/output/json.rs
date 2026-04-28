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

/// `--output json` payload emitted by `validate` on both pass and fail: a
/// single document on stdout always, with `diagnostics` empty on a clean
/// pass and populated on failure.
#[derive(Debug, Serialize)]
pub(crate) struct ValidateOutput {
    /// Validation diagnostics, ordered as the SDK collected them. Empty
    /// on a clean pass.
    pub diagnostics: Vec<Diagnostic>,
}

/// One validation diagnostic. `variant` is a stable string tag drawn from
/// [`influxdb3_plugin_sdk::ValidationError::variant_name`]; consumers can
/// pattern-match on it. `message` is the variant's `Display` text.
/// `field` is the field path (e.g., `plugin.name`) or filename (e.g.,
/// `manifest.toml`) the error refers to, when applicable.
#[derive(Debug, Serialize)]
pub(crate) struct Diagnostic {
    pub variant: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

/// `--output json` payload emitted by `yank`.
/// `outcome` is the [`influxdb3_plugin_sdk::mutate_index::YankOutcome`]
/// rendered as `"transitioned"` or `"already_in_desired_state"`.
/// `target_state` is `true` after `yank`, `false` after `yank --undo`.
#[derive(Debug, Serialize)]
pub(crate) struct YankOutput {
    pub name: String,
    pub version: String,
    pub outcome: &'static str,
    pub target_state: bool,
    pub index_path: PathBuf,
}

/// `--output json` payload emitted by `package` on success. Carries the
/// absolute paths of the artifact + derived index, the artifact's SHA-256
/// hash, and the new entry's identity.
#[derive(Debug, Serialize)]
pub(crate) struct PackageOutput {
    pub artifact_path: PathBuf,
    pub index_path: PathBuf,
    pub hash: String,
    pub new_entry_name: String,
    pub new_entry_version: String,
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
    /// `"plugin"` for trigger templates, `"registry"` for the registry
    /// template. Stable string tag; consumers can pattern-match.
    pub kind: &'static str,
    /// The template identifier the user passed (`process_writes`,
    /// `process_scheduled_call`, `process_request`, or `registry`).
    pub template: &'static str,
    /// Absolute path of the directory the scaffold wrote into.
    pub target_dir: PathBuf,
    /// Plugin name written into `manifest.toml` for plugin templates;
    /// omitted for registry templates, which carry no plugin name.
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
/// Per spec § 4.1 / § 6.1.
#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub(crate) enum Envelope<R: Serialize> {
    Ok { result: R },
    Error { error: JsonError },
}

/// Structured error payload for `Envelope::Error`. Carries the stable
/// `code`, human `message`, and optional structured fields per spec § 4.3
/// and § 4.5.1.
#[derive(Debug, Serialize)]
pub(crate) struct JsonError {
    /// Stable namespaced identifier from a closed enum (spec § 4.5).
    pub code: String,
    /// Source error's `Display` text, English.
    pub message: String,
    /// Dotted-path location, filename, or target identifier when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// Variant-specific structured payload per spec § 4.5.1.
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

#[cfg(test)]
mod envelope_tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct Demo { a: u32 }

    #[test]
    fn json_envelope_ok_serializes_shape() {
        let env = Envelope::Ok { result: Demo { a: 7 } };
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
        assert_eq!(s, r#"{"status":"error","error":{"code":"x::y","message":"msg"}}"#);
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
        let env = Envelope::Ok { result: ValidateResult {} };
        let s = serde_json::to_string(&env).unwrap();
        assert_eq!(s, r#"{"status":"ok","result":{}}"#);
    }
}
