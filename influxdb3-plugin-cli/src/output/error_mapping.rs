//! Maps SDK / clap errors into the wire-stable `JsonError` shape.
//! CLI-owned, decoupled from internal Rust type names so SDK refactors
//! don't break the wire.

use crate::output::json::JsonError;
use influxdb3_plugin_schemas::SchemaError;
use influxdb3_plugin_sdk::{SdkError, ValidationError};

/// Identifies the calling command so the error mapper can pick the
/// correct namespace for variants whose code dispatches by call site
/// (`SdkError::Io`, `SdkError::Archive`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorContext {
    Validate,
    Package,
    Yank,
    NewPlugin,
    NewIndex,
}

/// Maps a single [`ValidationError`] to the wire-stable [`JsonError`] shape.
///
/// Each variant maps to a `validate::*` code. `SchemaReported` delegates to
/// [`schema_error_details`] for the inner `SchemaError` flattening.
pub(crate) fn json_error_from_validation(err: &ValidationError) -> JsonError {
    match err {
        ValidationError::SchemaReported(reported) => {
            let field = if reported.path.as_str().is_empty() {
                None
            } else {
                Some(reported.path.as_str().to_owned())
            };
            // Use the inner SchemaError's Display, which never contains the
            // field-path prefix (ReportedError::Display prepends it, but we
            // deliberately bypass that layer).
            let message = reported.error.to_string();
            let details = Some(schema_error_details(&reported.error));
            JsonError {
                code: "validate::schema_reported".into(),
                message,
                field,
                details,
                diagnostics: vec![],
                cause: vec![],
            }
        }
        ValidationError::MissingRequiredFile { file } => JsonError {
            code: "validate::missing_required_file".into(),
            message: err.to_string(),
            field: Some(file.clone()),
            details: Some(serde_json::json!({ "file": file })),
            diagnostics: vec![],
            cause: vec![],
        },
        ValidationError::PythonParse { message } => JsonError {
            code: "validate::python_parse".into(),
            message: err.to_string(),
            field: Some("__init__.py".into()),
            details: Some(serde_json::json!({ "parse_message": message })),
            diagnostics: vec![],
            cause: vec![],
        },
        ValidationError::TriggerNotImplemented { trigger } => JsonError {
            code: "validate::trigger_not_implemented".into(),
            message: err.to_string(),
            field: Some("__init__.py".into()),
            details: Some(serde_json::json!({ "trigger": trigger.as_str() })),
            diagnostics: vec![],
            cause: vec![],
        },
        ValidationError::AsyncTriggerFn { trigger } => JsonError {
            code: "validate::async_trigger_fn".into(),
            message: err.to_string(),
            field: Some("__init__.py".into()),
            details: Some(serde_json::json!({ "trigger": trigger.as_str() })),
            diagnostics: vec![],
            cause: vec![],
        },
        ValidationError::NameVersionConflict { name, version } => JsonError {
            code: "validate::name_version_conflict".into(),
            message: format!(
                "{}; increment version in manifest.toml or run `yank` instead",
                err
            ),
            field: Some(format!("{name}@{version}")),
            details: Some(serde_json::json!({ "name": name, "version": version })),
            diagnostics: vec![],
            cause: vec![],
        },
        _ => JsonError {
            code: "validate::unknown".into(),
            message: err.to_string(),
            field: None,
            details: None,
            diagnostics: vec![],
            cause: vec![],
        },
    }
}

/// Returns a JSON object with `"schema_variant"` plus variant-specific fields
/// for the given [`SchemaError`]. Used by [`json_error_from_validation`] when
/// flattening `SchemaReported` details.
pub(crate) fn schema_error_details(err: &SchemaError) -> serde_json::Value {
    use SchemaError as SE;
    let variant = err.variant_name();
    match err {
        SE::InvalidPluginName { name } => {
            serde_json::json!({ "schema_variant": variant, "name": name })
        }
        SE::ReservedPluginName { name } => {
            serde_json::json!({ "schema_variant": variant, "name": name })
        }
        SE::InvalidVersion { version, source } => {
            serde_json::json!({
                "schema_variant": variant,
                "version": version,
                "parse_error": source.to_string(),
            })
        }
        SE::DescriptionTooLong { len } => {
            serde_json::json!({ "schema_variant": variant, "len": len })
        }
        SE::DescriptionEmpty => {
            serde_json::json!({ "schema_variant": variant })
        }
        SE::DescriptionMultiline { len } => {
            serde_json::json!({ "schema_variant": variant, "len": len })
        }
        SE::InvalidUrlScheme { url, scheme } => {
            serde_json::json!({ "schema_variant": variant, "url": url, "scheme": scheme })
        }
        SE::InvalidUrl { url, source } => {
            serde_json::json!({
                "schema_variant": variant,
                "url": url,
                "parse_error": source.to_string(),
            })
        }
        SE::UnknownTriggerType { trigger } => {
            serde_json::json!({ "schema_variant": variant, "trigger": trigger })
        }
        SE::EmptyTriggers => {
            serde_json::json!({ "schema_variant": variant })
        }
        SE::InvalidDatabaseVersion { range, source } => {
            serde_json::json!({
                "schema_variant": variant,
                "range": range,
                "parse_error": source.to_string(),
            })
        }
        SE::InvalidPythonRequirement {
            requirement,
            source,
        } => {
            serde_json::json!({
                "schema_variant": variant,
                "requirement": requirement,
                "parse_error": source.to_string(),
            })
        }
        SE::UnsupportedArtifactScheme { url, scheme } => {
            serde_json::json!({ "schema_variant": variant, "url": url, "scheme": scheme })
        }
        SE::InvalidHash { value } => {
            serde_json::json!({ "schema_variant": variant, "value": value })
        }
        SE::DuplicateIndexEntry { name, version } => {
            serde_json::json!({ "schema_variant": variant, "name": name, "version": version })
        }
        SE::CanonicalCollision {
            name,
            canonical,
            existing,
        } => {
            let existing_arr: Vec<serde_json::Value> = existing
                .iter()
                .map(|(n, v)| serde_json::json!([n, v]))
                .collect();
            serde_json::json!({
                "schema_variant": variant,
                "name": name,
                "canonical": canonical,
                "existing": existing_arr,
            })
        }
        SE::UnsupportedManifestMajor { found, supported } => {
            serde_json::json!({
                "schema_variant": variant,
                "found": found,
                "supported": supported,
            })
        }
        SE::UnsupportedIndexMajor { found, supported } => {
            serde_json::json!({
                "schema_variant": variant,
                "found": found,
                "supported": supported,
            })
        }
        SE::MalformedSchemaVersion { value } => {
            serde_json::json!({ "schema_variant": variant, "value": value })
        }
        SE::TomlParse { source } => {
            serde_json::json!({
                "schema_variant": variant,
                "parse_error": source.to_string(),
            })
        }
        SE::JsonParse { source } => {
            serde_json::json!({
                "schema_variant": variant,
                "parse_error": source.to_string(),
            })
        }
        SE::JsonSerialize { source } => {
            serde_json::json!({
                "schema_variant": variant,
                "serialize_error": source.to_string(),
            })
        }
        _ => {
            serde_json::json!({
                "schema_variant": "variant_unmapped",
                "display": err.to_string(),
            })
        }
    }
}

/// Maps a [`clap::Error`] to the wire-stable [`JsonError`] shape.
///
/// Dispatches by `err.kind()` to a `usage::*` code.
/// `ValueValidation` with `ContextKind::InvalidArg == "<NAME@VERSION>"` refines
/// to `usage::invalid_target`.
pub fn json_error_from_clap(err: &clap::Error) -> JsonError {
    use clap::error::ErrorKind;
    let message = collapse_clap_message(err);
    let kind = err.kind();
    match kind {
        ErrorKind::MissingRequiredArgument => JsonError {
            code: "usage::missing_required_argument".into(),
            message,
            field: None,
            details: argument_details(err),
            diagnostics: vec![],
            cause: vec![],
        },
        ErrorKind::InvalidValue => JsonError {
            code: "usage::invalid_value".into(),
            message,
            field: None,
            details: argument_value_details(err),
            diagnostics: vec![],
            cause: vec![],
        },
        ErrorKind::ValueValidation => {
            let is_name_at_version = clap_context_arg(err)
                .as_deref()
                .map(|a| a == "<NAME@VERSION>")
                .unwrap_or(false);
            let code = if is_name_at_version {
                "usage::invalid_target"
            } else {
                "usage::value_validation"
            };
            JsonError {
                code: code.into(),
                message,
                field: None,
                details: argument_value_details(err),
                diagnostics: vec![],
                cause: vec![],
            }
        }
        ErrorKind::UnknownArgument => JsonError {
            code: "usage::unknown_argument".into(),
            message,
            field: None,
            details: argument_details(err),
            diagnostics: vec![],
            cause: vec![],
        },
        ErrorKind::InvalidSubcommand => JsonError {
            code: "usage::invalid_subcommand".into(),
            message,
            field: None,
            details: subcommand_details(err),
            diagnostics: vec![],
            cause: vec![],
        },
        ErrorKind::MissingSubcommand => JsonError {
            code: "usage::missing_subcommand".into(),
            message,
            field: None,
            details: None,
            diagnostics: vec![],
            cause: vec![],
        },
        ErrorKind::TooManyValues => JsonError {
            code: "usage::too_many_values".into(),
            message,
            field: None,
            details: argument_value_details(err),
            diagnostics: vec![],
            cause: vec![],
        },
        ErrorKind::TooFewValues => JsonError {
            code: "usage::too_few_values".into(),
            message,
            field: None,
            details: argument_value_details(err),
            diagnostics: vec![],
            cause: vec![],
        },
        other => {
            let clap_kind = format!("{other:?}");
            JsonError {
                code: "usage::parse_error".into(),
                message,
                field: None,
                details: Some(serde_json::json!({ "clap_kind": clap_kind })),
                diagnostics: vec![],
                cause: vec![],
            }
        }
    }
}

/// Collapses clap's multi-line error message into a single line.
///
/// Strips the trailing "For more information" footer and joins remaining
/// non-empty lines with a space.
fn collapse_clap_message(err: &clap::Error) -> String {
    let rendered = err.to_string();
    let lines: Vec<&str> = rendered
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter(|l| !l.trim_start().starts_with("For more information"))
        .collect();
    lines.join(" ")
}

/// Returns the value of `ContextKind::InvalidArg` from a clap error, if present.
fn clap_context_arg(err: &clap::Error) -> Option<String> {
    use clap::error::{ContextKind, ContextValue};
    err.get(ContextKind::InvalidArg).and_then(|cv| {
        if let ContextValue::String(s) = cv {
            Some(s.clone())
        } else {
            None
        }
    })
}

/// Returns the value of `ContextKind::InvalidValue` from a clap error, if present.
fn clap_context_value(err: &clap::Error) -> Option<String> {
    use clap::error::{ContextKind, ContextValue};
    err.get(ContextKind::InvalidValue).and_then(|cv| {
        if let ContextValue::String(s) = cv {
            Some(s.clone())
        } else {
            None
        }
    })
}

/// Builds `{"argument": arg}` details from a clap error's `InvalidArg` context.
fn argument_details(err: &clap::Error) -> Option<serde_json::Value> {
    clap_context_arg(err).map(|arg| serde_json::json!({ "argument": arg }))
}

/// Builds `{"argument": arg, "value": val}` details from a clap error's context.
///
/// Omits fields that are absent. Returns `None` if both are absent.
fn argument_value_details(err: &clap::Error) -> Option<serde_json::Value> {
    let arg = clap_context_arg(err);
    let val = clap_context_value(err);
    match (arg, val) {
        (None, None) => None,
        (arg, val) => {
            let mut map = serde_json::Map::new();
            if let Some(a) = arg {
                map.insert("argument".into(), serde_json::Value::String(a));
            }
            if let Some(v) = val {
                map.insert("value".into(), serde_json::Value::String(v));
            }
            Some(serde_json::Value::Object(map))
        }
    }
}

/// Builds `{"subcommand": sub}` details from a clap error's `InvalidSubcommand` context.
fn subcommand_details(err: &clap::Error) -> Option<serde_json::Value> {
    use clap::error::{ContextKind, ContextValue};
    err.get(ContextKind::InvalidSubcommand).and_then(|cv| {
        if let ContextValue::String(s) = cv {
            Some(serde_json::json!({ "subcommand": s }))
        } else {
            None
        }
    })
}

/// Returns `"{namespace}::{suffix}"` where namespace is derived from the
/// [`ErrorContext`].
fn namespace_for(ctx: ErrorContext, suffix: &str) -> String {
    let ns = match ctx {
        ErrorContext::Validate => "validate",
        ErrorContext::Package => "package",
        ErrorContext::Yank => "yank",
        ErrorContext::NewPlugin | ErrorContext::NewIndex => "new",
    };
    format!("{ns}::{suffix}")
}

/// Maps an I/O error to [`JsonError`], picking the wire code from context.
///
/// Context-dependent `io_failed` / `scaffold_failed` / `unknown`.
fn io_error_to_json(
    source: &std::io::Error,
    path: Option<&std::path::Path>,
    ctx: ErrorContext,
) -> JsonError {
    let code = match ctx {
        ErrorContext::Validate => "validate::io_failed",
        ErrorContext::Package => "package::io_failed",
        ErrorContext::Yank => "yank::io_failed",
        ErrorContext::NewPlugin | ErrorContext::NewIndex => "new::scaffold_failed",
    };
    let field = path.map(|p| p.display().to_string());
    let details = Some(serde_json::json!({
        "path": path.map(|p| p.display().to_string()),
        "io_kind": format!("{:?}", source.kind()),
    }));
    // The io::Error's Display is already the message; putting the same
    // string in `cause` would double it in the human renderer. Only add
    // inner source chain entries that differ from the top-level message.
    let msg = source.to_string();
    let cause: Vec<String> = std::error::Error::source(source)
        .into_iter()
        .map(|s| s.to_string())
        .filter(|s| s != &msg)
        .collect();
    JsonError {
        code: code.into(),
        message: msg,
        field,
        details,
        diagnostics: vec![],
        cause,
    }
}

/// Maps an [`SdkError`] to the wire-stable [`JsonError`] shape.
///
/// Context-dispatched variants (`Io`, `Archive`, `PathOverlap`) pick their
/// namespace from [`ErrorContext`].
pub(crate) fn json_error_from_sdk(err: &SdkError, ctx: ErrorContext) -> JsonError {
    match err {
        SdkError::Io { source, path } => io_error_to_json(source, path.as_deref(), ctx),

        SdkError::Schema(schema_err) => JsonError {
            code: namespace_for(ctx, "schema_error"),
            message: err.to_string(),
            field: None,
            details: Some(schema_error_details(schema_err)),
            diagnostics: vec![],
            cause: vec![],
        },

        SdkError::ValidationErrors(errs) => JsonError {
            code: "validate::failed".into(),
            message: err.to_string(),
            field: None,
            details: None,
            diagnostics: errs.iter().map(json_error_from_validation).collect(),
            cause: vec![],
        },

        SdkError::Archive { message } => match ctx {
            ErrorContext::NewPlugin | ErrorContext::NewIndex => JsonError {
                code: "new::scaffold_failed".into(),
                message: err.to_string(),
                field: None,
                details: None,
                diagnostics: vec![],
                cause: vec![],
            },
            _ => JsonError {
                code: "package::archive_failed".into(),
                message: err.to_string(),
                field: None,
                details: Some(serde_json::json!({ "archive_message": message })),
                diagnostics: vec![],
                cause: vec![],
            },
        },

        SdkError::PathTooLong {
            archive_path,
            limit,
        } => JsonError {
            code: "package::path_too_long".into(),
            message: err.to_string(),
            field: Some(archive_path.clone()),
            details: Some(serde_json::json!({
                "archive_path": archive_path,
                "limit_bytes": limit,
            })),
            diagnostics: vec![],
            cause: vec![],
        },

        SdkError::Hash { source } => JsonError {
            code: "package::hash_failed".into(),
            message: err.to_string(),
            field: None,
            details: None,
            diagnostics: vec![],
            cause: vec![source.to_string()],
        },

        SdkError::AlreadyPublished {
            name,
            version,
            existing_versions,
        } => JsonError {
            code: "package::already_published".into(),
            message: format!(
                "plugin ({name:?}, {version:?}) already exists in the target index; \
                 existing versions: {existing_versions:?}. \
                 Increment version in manifest.toml or run `yank` instead."
            ),
            field: Some(format!("{name}@{version}")),
            details: Some(serde_json::json!({
                "name": name,
                "version": version,
                "existing_versions": existing_versions,
            })),
            diagnostics: vec![],
            cause: vec![],
        },

        SdkError::CanonicalCollision {
            name,
            canonical,
            existing,
        } => {
            let existing_arr: Vec<serde_json::Value> = existing
                .iter()
                .map(|(n, v)| serde_json::json!({"name": n, "version": v.to_string()}))
                .collect();
            JsonError {
                code: "package::canonical_collision".into(),
                message: format!(
                    "canonical collision: plugin name {name:?} conflicts with existing \
                     entries sharing canonical form {canonical:?}: {existing:?}. \
                     Rename to one of the existing spellings or choose a distinct name."
                ),
                field: Some("plugin.name".into()),
                details: Some(serde_json::json!({
                    "name": name,
                    "canonical": canonical,
                    "existing": existing_arr,
                })),
                diagnostics: vec![],
                cause: vec![],
            }
        }

        SdkError::EntryNotFound { name, version } => JsonError {
            code: "yank::entry_not_found".into(),
            message: err.to_string(),
            field: Some(format!("{name}@{version}")),
            details: Some(serde_json::json!({
                "name": name,
                "version": version,
            })),
            diagnostics: vec![],
            cause: vec![],
        },

        _ => JsonError {
            code: namespace_for(ctx, "sdk_error"),
            message: err.to_string(),
            field: None,
            details: None,
            diagnostics: vec![],
            cause: vec![],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use influxdb3_plugin_schemas::{FieldPath, ReportedError, SchemaError, TriggerType};
    use influxdb3_plugin_sdk::ValidationError;

    /// Helper: construct one of each `ValidationError` variant (mirrors the
    /// SDK's `every_validation_variant` fixture).
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
                message: "unexpected token".into(),
            },
            ValidationError::TriggerNotImplemented {
                trigger: TriggerType::ProcessWrites,
            },
            ValidationError::AsyncTriggerFn {
                trigger: TriggerType::ProcessScheduledCall,
            },
            ValidationError::NameVersionConflict {
                name: "downsampler".into(),
                version: "1.2.0".into(),
            },
        ]
    }

    #[test]
    fn validation_error_variants_map_to_codes() {
        let expected_codes = [
            "validate::schema_reported",
            "validate::missing_required_file",
            "validate::python_parse",
            "validate::trigger_not_implemented",
            "validate::async_trigger_fn",
            "validate::name_version_conflict",
        ];
        let variants = every_validation_variant();
        assert_eq!(variants.len(), expected_codes.len());
        for (err, expected_code) in variants.iter().zip(expected_codes.iter()) {
            let je = json_error_from_validation(err);
            assert_eq!(
                &je.code,
                expected_code,
                "variant {:?} produced wrong code",
                std::mem::discriminant(err)
            );
        }
    }

    #[test]
    fn schema_reported_strips_field_from_message() {
        // ReportedError.to_string() prepends "plugin.description: " to the
        // inner error.  json_error_from_validation uses the inner error's
        // Display (which has NO prefix), so the message must not contain
        // the field path.
        let reported = ReportedError::new(
            FieldPath::root().field("plugin").field("description"),
            SchemaError::DescriptionEmpty,
        );
        let ve = ValidationError::SchemaReported(reported);
        let je = json_error_from_validation(&ve);

        // The outer ReportedError would produce "plugin.description: description must not be empty"
        // but the inner SchemaError produces just "description must not be empty".
        assert!(
            !je.message.starts_with("plugin.description"),
            "message should not contain the field-path prefix, got: {}",
            je.message
        );
        assert!(
            je.message.contains("description must not be empty"),
            "message should contain the inner error text, got: {}",
            je.message
        );
        assert_eq!(je.field.as_deref(), Some("plugin.description"));
    }

    #[test]
    fn schema_reported_details_include_inner_variant() {
        let reported = ReportedError::new(
            FieldPath::root().field("plugin").field("name"),
            SchemaError::InvalidPluginName {
                name: "Bad Name".into(),
            },
        );
        let ve = ValidationError::SchemaReported(reported);
        let je = json_error_from_validation(&ve);

        let details = je.details.as_ref().expect("details should be Some");
        assert_eq!(
            details["schema_variant"], "InvalidPluginName",
            "details.schema_variant must match SchemaError::variant_name()"
        );
        assert_eq!(details["name"], "Bad Name");
    }

    #[test]
    fn trigger_details_use_trigger_type_as_str() {
        let ve = ValidationError::TriggerNotImplemented {
            trigger: TriggerType::ProcessWrites,
        };
        let je = json_error_from_validation(&ve);
        let details = je.details.as_ref().expect("details should be Some");
        assert_eq!(
            details["trigger"], "process_writes",
            "details.trigger should use the canonical snake_case from TriggerType::as_str()"
        );

        let ve2 = ValidationError::AsyncTriggerFn {
            trigger: TriggerType::ProcessScheduledCall,
        };
        let je2 = json_error_from_validation(&ve2);
        let details2 = je2.details.as_ref().expect("details should be Some");
        assert_eq!(details2["trigger"], "process_scheduled_call");
    }

    /// Drift guard: constructs every `SchemaError` variant and asserts that
    /// `schema_error_details` does not fall through to the `_` arm (which
    /// sets `schema_variant` to `"variant_unmapped"`). If a new variant is
    /// added to `SchemaError` without updating `schema_error_details`, this
    /// test will fail.
    #[test]
    fn schema_error_details_covers_every_variant() {
        use serde::ser::Error as _;

        let all_schema_errors: Vec<SchemaError> = vec![
            SchemaError::InvalidPluginName {
                name: "Bad Name".into(),
            },
            SchemaError::ReservedPluginName { name: "con".into() },
            SchemaError::InvalidVersion {
                version: "1.2".into(),
                source: semver::Version::parse("1.2").unwrap_err(),
            },
            SchemaError::DescriptionTooLong { len: 201 },
            SchemaError::DescriptionEmpty,
            SchemaError::DescriptionMultiline { len: 201 },
            SchemaError::InvalidUrlScheme {
                url: "ftp://bad".into(),
                scheme: "ftp".into(),
            },
            SchemaError::InvalidUrl {
                url: "not a url".into(),
                source: url::Url::parse("not a url").unwrap_err(),
            },
            SchemaError::UnknownTriggerType {
                trigger: "on_startup".into(),
            },
            SchemaError::EmptyTriggers,
            SchemaError::InvalidDatabaseVersion {
                range: ">=bad".into(),
                source: semver::VersionReq::parse(">=bad").unwrap_err(),
            },
            SchemaError::InvalidPythonRequirement {
                requirement: "requests>>=2.0".into(),
                source: Box::new(
                    "requests>>=2.0"
                        .parse::<pep508_rs::Requirement<pep508_rs::VerbatimUrl>>()
                        .unwrap_err(),
                ),
            },
            SchemaError::UnsupportedArtifactScheme {
                url: "s3://bucket/foo".into(),
                scheme: "s3".into(),
            },
            SchemaError::InvalidHash {
                value: "notahash".into(),
            },
            SchemaError::DuplicateIndexEntry {
                name: "dup".into(),
                version: "1.0.0".into(),
            },
            SchemaError::CanonicalCollision {
                name: "my-plugin".into(),
                canonical: "my_plugin".into(),
                existing: vec![("my_plugin".into(), "1.0.0".into())],
            },
            SchemaError::UnsupportedManifestMajor {
                found: "2.0".into(),
                supported: 1,
            },
            SchemaError::UnsupportedIndexMajor {
                found: "2.0".into(),
                supported: 1,
            },
            SchemaError::MalformedSchemaVersion {
                value: "abc".into(),
            },
            SchemaError::TomlParse {
                source: toml::from_str::<toml::Value>("= ").unwrap_err(),
            },
            SchemaError::JsonParse {
                source: serde_json::from_str::<serde_json::Value>("{").unwrap_err(),
            },
            SchemaError::JsonSerialize {
                source: serde_json::Error::custom("forced"),
            },
        ];

        assert_eq!(
            all_schema_errors.len(),
            22,
            "expected 22 SchemaError variants"
        );

        for se in &all_schema_errors {
            let details = schema_error_details(se);
            let sv = details["schema_variant"]
                .as_str()
                .expect("schema_variant key must exist");
            assert_ne!(
                sv,
                "variant_unmapped",
                "SchemaError variant {:?} fell through to the _ arm in schema_error_details",
                se.variant_name()
            );
            assert_eq!(
                sv,
                se.variant_name(),
                "schema_variant must match SchemaError::variant_name()"
            );
        }
    }

    use influxdb3_plugin_sdk::SdkError;

    #[test]
    fn sdk_error_variants_map_to_codes_package_context() {
        let cases: Vec<(SdkError, &str)> = vec![
            (
                SdkError::Io {
                    source: std::io::Error::other("boom"),
                    path: Some(std::path::PathBuf::from("/tmp/x")),
                },
                "package::io_failed",
            ),
            (
                SdkError::Schema(SchemaError::DescriptionEmpty),
                "package::schema_error",
            ),
            (
                SdkError::ValidationErrors(vec![ValidationError::MissingRequiredFile {
                    file: "__init__.py".into(),
                }]),
                "validate::failed",
            ),
            (
                SdkError::Archive {
                    message: "tar fail".into(),
                },
                "package::archive_failed",
            ),
            (
                SdkError::PathTooLong {
                    archive_path: "long/path".into(),
                    limit: 255,
                },
                "package::path_too_long",
            ),
            (
                SdkError::Hash {
                    source: std::io::Error::other("read failed"),
                },
                "package::hash_failed",
            ),
            (
                SdkError::AlreadyPublished {
                    name: "p".into(),
                    version: "1.0.0".into(),
                    existing_versions: vec!["1.0.0".into()],
                },
                "package::already_published",
            ),
            (
                SdkError::CanonicalCollision {
                    name: "my-plugin".into(),
                    canonical: "my_plugin".into(),
                    existing: vec![("my_plugin".into(), semver::Version::new(1, 0, 0))],
                },
                "package::canonical_collision",
            ),
            (
                SdkError::EntryNotFound {
                    name: "p".into(),
                    version: "1.0.0".into(),
                },
                "yank::entry_not_found",
            ),
        ];

        for (err, expected_code) in &cases {
            let je = json_error_from_sdk(err, ErrorContext::Package);
            assert_eq!(
                &je.code,
                expected_code,
                "SdkError::{} with Package context produced wrong code",
                err.variant_name()
            );
        }
    }

    #[test]
    fn io_error_code_depends_on_context() {
        let err = SdkError::Io {
            source: std::io::Error::other("boom"),
            path: Some(std::path::PathBuf::from("/tmp/x")),
        };
        let cases = [
            (ErrorContext::Validate, "validate::io_failed"),
            (ErrorContext::Package, "package::io_failed"),
            (ErrorContext::Yank, "yank::io_failed"),
            (ErrorContext::NewPlugin, "new::scaffold_failed"),
            (ErrorContext::NewIndex, "new::scaffold_failed"),
        ];
        for (ctx, expected_code) in &cases {
            let je = json_error_from_sdk(&err, *ctx);
            assert_eq!(
                &je.code, expected_code,
                "Io with context {:?} produced wrong code",
                ctx
            );
            // field should be the path
            assert_eq!(je.field.as_deref(), Some("/tmp/x"));
            // cause should only contain source-chain entries that differ
            // from the top-level message; `Error::other("boom")` has no
            // inner source, so cause is empty.
            assert!(
                je.cause.is_empty(),
                "cause should be empty for Io with no inner source; got: {:?}",
                je.cause
            );
        }
    }

    #[test]
    fn archive_code_depends_on_context() {
        let err = SdkError::Archive {
            message: "tar fail".into(),
        };
        // Package context → package::archive_failed with details
        let je_pkg = json_error_from_sdk(&err, ErrorContext::Package);
        assert_eq!(je_pkg.code, "package::archive_failed");
        let details = je_pkg.details.as_ref().expect("details should exist");
        assert_eq!(details["archive_message"], "tar fail");

        // NewPlugin context → new::scaffold_failed without details
        let je_new = json_error_from_sdk(&err, ErrorContext::NewPlugin);
        assert_eq!(je_new.code, "new::scaffold_failed");
        assert!(
            je_new.details.is_none(),
            "scaffold_failed should have no details"
        );

        // NewRegistry context → new::scaffold_failed
        let je_reg = json_error_from_sdk(&err, ErrorContext::NewIndex);
        assert_eq!(je_reg.code, "new::scaffold_failed");
    }

    #[test]
    fn yank_entry_not_found_maps() {
        let err = SdkError::EntryNotFound {
            name: "downsampler".into(),
            version: "1.2.0".into(),
        };
        let je = json_error_from_sdk(&err, ErrorContext::Yank);
        assert_eq!(je.code, "yank::entry_not_found");
        assert_eq!(je.field.as_deref(), Some("downsampler@1.2.0"));
        let details = je.details.as_ref().expect("details should exist");
        assert_eq!(details["name"], "downsampler");
        assert_eq!(details["version"], "1.2.0");
    }

    #[test]
    fn schema_error_in_package_context_maps_to_package_schema_error() {
        let err = SdkError::Schema(SchemaError::DescriptionEmpty);
        let je = json_error_from_sdk(&err, ErrorContext::Package);
        assert_eq!(je.code, "package::schema_error");
        let details = je.details.as_ref().expect("details should exist");
        assert_eq!(details["schema_variant"], "DescriptionEmpty");
    }

    use clap::error::{ContextKind, ContextValue, ErrorKind};
    use clap::{Arg, Command, Error as ClapError};

    fn make_clap_err(kind: ErrorKind) -> ClapError {
        ClapError::raw(kind, "boom")
    }

    #[test]
    fn clap_error_kinds_map_to_codes() {
        let cases = [
            (
                ErrorKind::MissingRequiredArgument,
                "usage::missing_required_argument",
            ),
            (ErrorKind::InvalidValue, "usage::invalid_value"),
            (ErrorKind::ValueValidation, "usage::value_validation"),
            (ErrorKind::UnknownArgument, "usage::unknown_argument"),
            (ErrorKind::InvalidSubcommand, "usage::invalid_subcommand"),
            (ErrorKind::MissingSubcommand, "usage::missing_subcommand"),
            (ErrorKind::TooManyValues, "usage::too_many_values"),
            (ErrorKind::TooFewValues, "usage::too_few_values"),
        ];
        for (kind, expected) in cases {
            let err = make_clap_err(kind);
            let je = json_error_from_clap(&err);
            assert_eq!(je.code, expected, "for {kind:?}");
        }
    }

    #[test]
    fn clap_unmapped_kind_falls_back_to_parse_error() {
        let err = make_clap_err(ErrorKind::Format);
        let je = json_error_from_clap(&err);
        assert_eq!(je.code, "usage::parse_error");
        let details = je.details.expect("details");
        assert_eq!(
            details.get("clap_kind").and_then(|v| v.as_str()),
            Some("Format"),
            "unmapped ErrorKind::Format should surface as clap_kind \"Format\""
        );
    }

    #[test]
    fn clap_value_validation_for_name_at_version_emits_invalid_target() {
        let cmd = Command::new("yank").arg(Arg::new("NAME@VERSION").required(true));
        let mut err = ClapError::new(ErrorKind::ValueValidation).with_cmd(&cmd);
        err.insert(
            ContextKind::InvalidArg,
            ContextValue::String("<NAME@VERSION>".into()),
        );
        err.insert(
            ContextKind::InvalidValue,
            ContextValue::String("badformat: msg".into()),
        );
        let je = json_error_from_clap(&err);
        assert_eq!(je.code, "usage::invalid_target");
    }
}
