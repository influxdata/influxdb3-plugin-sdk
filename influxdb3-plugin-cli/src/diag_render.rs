//! Shared human-mode renderer for the `diagnostics` list produced by
//! `validate` and (on validation-failure path) `package`.
//!
//! Keeping this in one place locks the "authors fix everything in one
//! pass" shape and avoids two independent text
//! formats drifting. Also houses the canonical
//! `ValidationError -> Diagnostic` mapping so `validate` and `package`
//! share one implementation.

use crate::output::json::{Diagnostic, JsonError};
use crate::style::Palette;
use influxdb3_plugin_sdk::ValidationError;
use std::io;

/// Extracts the field path / target file from a `ValidationError`, for
/// the `Diagnostic.field` JSON column and for the human renderer's
/// "field already embedded in message" check.
pub(crate) fn field_of(err: &ValidationError) -> Option<String> {
    match err {
        ValidationError::SchemaReported(r) => {
            let p = r.path.as_str();
            if p.is_empty() {
                None
            } else {
                Some(p.to_owned())
            }
        }
        ValidationError::MissingRequiredFile { file } => Some(file.clone()),
        ValidationError::PythonParse { .. }
        | ValidationError::TriggerNotImplemented { .. }
        | ValidationError::AsyncTriggerFn { .. } => Some("__init__.py".to_owned()),
        ValidationError::NameVersionConflict { name, version } => Some(format!("{name}@{version}")),
        ValidationError::IndexReadFailed { path, .. } => Some(path.display().to_string()),
        _ => None,
    }
}

/// Builds a `Diagnostic` from a `ValidationError` for both JSON output
/// and the human renderer. `message` is the variant's `Display` text
/// verbatim — `SchemaReported` already embeds its field path (e.g.
/// `"plugin.name: plugin name ..."`), and this function does NOT
/// re-prepend the path.
pub(crate) fn diagnostic_from(err: &ValidationError) -> Diagnostic {
    Diagnostic {
        variant: err.variant_name(),
        message: err.to_string(),
        field: field_of(err),
    }
}

pub(crate) fn render_human(
    diagnostics: &[Diagnostic],
    palette: Palette,
    writer: &mut dyn io::Write,
) -> io::Result<()> {
    let header = palette.error.render();
    let header_reset = palette.error.render_reset();
    writeln!(
        writer,
        "{header}validation failed: {} diagnostic(s){header_reset}",
        diagnostics.len()
    )?;
    let dim = palette.dim.render();
    let dim_reset = palette.dim.render_reset();
    let tag = palette.tag.render();
    let tag_reset = palette.tag.render_reset();
    for (i, d) in diagnostics.iter().enumerate() {
        // For `SchemaReported` diagnostics, `message` already begins with
        // the field path (e.g. "plugin.name: plugin name ..."). For other
        // variants (`MissingRequiredFile`, `PythonParse`, etc.), the
        // message does not embed the field path, so we prepend it
        // explicitly when present. The check is "message already starts
        // with `field:`" so we don't double-prefix.
        let already_prefixed = d
            .field
            .as_deref()
            .map(|f| d.message.starts_with(&format!("{f}:")))
            .unwrap_or(false);
        match (&d.field, already_prefixed) {
            (Some(field), false) => {
                writeln!(
                    writer,
                    "  {dim}{}.{dim_reset} {tag}[{}]{tag_reset} {field}: {}",
                    i + 1,
                    d.variant,
                    d.message
                )?;
            }
            _ => {
                writeln!(
                    writer,
                    "  {dim}{}.{dim_reset} {tag}[{}]{tag_reset} {}",
                    i + 1,
                    d.variant,
                    d.message
                )?;
            }
        }
    }
    Ok(())
}

/// Top-level entry point for human-mode error rendering. Dispatches on
/// the `JsonError` shape: if `diagnostics[]` is non-empty we render the
/// numbered list block; otherwise we render one line with optional cause
/// chain. Spec § 4.7.
pub fn render_human_error(
    err: &JsonError,
    palette: Palette,
    writer: &mut dyn io::Write,
) -> io::Result<()> {
    if err.diagnostics.is_empty() {
        render_single_issue(err, palette, writer)
    } else {
        render_multi_issue(err, palette, writer)
    }
}

fn render_single_issue(
    err: &JsonError,
    palette: Palette,
    writer: &mut dyn io::Write,
) -> io::Result<()> {
    let tag = palette.tag.render();
    let tag_reset = palette.tag.render_reset();
    match err.field.as_deref() {
        Some(f) => writeln!(
            writer,
            "{tag}[{}]{tag_reset} {}: {}",
            err.code, f, err.message
        )?,
        None => writeln!(writer, "{tag}[{}]{tag_reset} {}", err.code, err.message)?,
    }
    let dim = palette.dim.render();
    let dim_reset = palette.dim.render_reset();
    for c in &err.cause {
        writeln!(writer, "  {dim}cause:{dim_reset} {c}")?;
    }
    Ok(())
}

fn render_multi_issue(
    err: &JsonError,
    palette: Palette,
    writer: &mut dyn io::Write,
) -> io::Result<()> {
    let header = palette.error.render();
    let header_reset = palette.error.render_reset();
    writeln!(
        writer,
        "{header}validation failed: {} diagnostic(s){header_reset}",
        err.diagnostics.len()
    )?;
    let dim = palette.dim.render();
    let dim_reset = palette.dim.render_reset();
    let tag = palette.tag.render();
    let tag_reset = palette.tag.render_reset();
    for (i, d) in err.diagnostics.iter().enumerate() {
        match d.field.as_deref() {
            Some(f) => writeln!(
                writer,
                "  {dim}{}.{dim_reset} {tag}[{}]{tag_reset} {f}: {}",
                i + 1,
                d.code,
                d.message,
            )?,
            None => writeln!(
                writer,
                "  {dim}{}.{dim_reset} {tag}[{}]{tag_reset} {}",
                i + 1,
                d.code,
                d.message,
            )?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::json::Diagnostic;

    fn d(variant: &'static str, msg: &str, field: Option<&str>) -> Diagnostic {
        Diagnostic {
            variant,
            message: msg.to_owned(),
            field: field.map(str::to_owned),
        }
    }

    /// No-op palette — every style field collapses to an empty ANSI escape
    /// so the byte-level assertions below remain identical to the
    /// pre-colorization shape.
    fn plain() -> Palette {
        Palette::default()
    }

    /// `SchemaReported`: message already embeds the field path; the
    /// renderer must NOT double-prefix.
    #[test]
    fn schema_reported_does_not_double_prefix_field() {
        let diag = vec![d(
            "SchemaReported",
            "plugin.name: plugin name \"X\" must ...",
            Some("plugin.name"),
        )];
        let mut buf = Vec::<u8>::new();
        render_human(&diag, plain(), &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert_eq!(
            s,
            "validation failed: 1 diagnostic(s)\n  \
             1. [SchemaReported] plugin.name: plugin name \"X\" must ...\n",
        );
        assert!(!s.contains("plugin.name: plugin.name:"));
    }

    /// `MissingRequiredFile`: message does NOT embed the file name; the
    /// renderer prepends `field:` so the output still tells the user
    /// which file is affected.
    #[test]
    fn missing_required_file_prepends_field_when_absent() {
        let diag = vec![d(
            "MissingRequiredFile",
            "required file \"manifest.toml\" is missing from the plugin directory",
            Some("manifest.toml"),
        )];
        let mut buf = Vec::<u8>::new();
        render_human(&diag, plain(), &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert_eq!(
            s,
            "validation failed: 1 diagnostic(s)\n  \
             1. [MissingRequiredFile] manifest.toml: required file \
             \"manifest.toml\" is missing from the plugin directory\n",
        );
    }

    #[test]
    fn multiple_diagnostics_numbered() {
        let diag = vec![
            d("SchemaReported", "plugin.name: bad", Some("plugin.name")),
            d(
                "SchemaReported",
                "plugin.version: bad",
                Some("plugin.version"),
            ),
        ];
        let mut buf = Vec::<u8>::new();
        render_human(&diag, plain(), &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.starts_with("validation failed: 2 diagnostic(s)\n"));
        assert!(s.contains("  1. [SchemaReported] plugin.name: bad\n"));
        assert!(s.contains("  2. [SchemaReported] plugin.version: bad\n"));
    }

    /// No field at all: the renderer emits only `[variant] message`.
    #[test]
    fn no_field_no_prefix() {
        let diag = vec![d("Hash", "hash computation failed: …", None)];
        let mut buf = Vec::<u8>::new();
        render_human(&diag, plain(), &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert_eq!(
            s,
            "validation failed: 1 diagnostic(s)\n  1. [Hash] hash computation failed: …\n",
        );
    }

    // ---- render_human_error tests (JsonError-based) ----

    fn je(code: &str, message: &str, field: Option<&str>) -> JsonError {
        JsonError {
            code: code.into(),
            message: message.into(),
            field: field.map(str::to_owned),
            details: None,
            diagnostics: vec![],
            cause: vec![],
        }
    }

    #[test]
    fn render_human_error_single_issue_emits_one_line_with_code_field_message() {
        let err = je(
            "package::canonical_collision",
            "name conflicts",
            Some("plugin.name"),
        );
        let mut buf = Vec::new();
        render_human_error(&err, plain(), &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert_eq!(
            s,
            "[package::canonical_collision] plugin.name: name conflicts\n",
        );
    }

    #[test]
    fn render_human_error_single_issue_omits_field_prefix_when_absent() {
        let err = je("usage::missing_subcommand", "subcommand required", None);
        let mut buf = Vec::new();
        render_human_error(&err, plain(), &mut buf).unwrap();
        assert_eq!(
            String::from_utf8(buf).unwrap(),
            "[usage::missing_subcommand] subcommand required\n",
        );
    }

    #[test]
    fn render_human_error_renders_cause_chain_when_present() {
        let err = JsonError {
            code: "io::read_failed".into(),
            message: "failed to read --index /tmp/idx.json".into(),
            field: Some("/tmp/idx.json".into()),
            details: None,
            diagnostics: vec![],
            cause: vec!["No such file or directory (os error 2)".into()],
        };
        let mut buf = Vec::new();
        render_human_error(&err, plain(), &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.starts_with(
            "[io::read_failed] /tmp/idx.json: failed to read --index /tmp/idx.json\n"
        ));
        assert!(s.contains("  cause: No such file or directory (os error 2)\n"));
    }

    #[test]
    fn render_human_error_multi_issue_renders_numbered_block() {
        let err = JsonError {
            code: "validate::failed".into(),
            message: "2 validation diagnostic(s)".into(),
            field: None,
            details: None,
            diagnostics: vec![
                je(
                    "validate::missing_required_file",
                    "required file \"x\" missing",
                    Some("x"),
                ),
                je("validate::python_parse", "syntax", Some("__init__.py")),
            ],
            cause: vec![],
        };
        let mut buf = Vec::new();
        render_human_error(&err, plain(), &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.starts_with("validation failed: 2 diagnostic(s)\n"));
        assert!(s.contains("  1. [validate::missing_required_file]"));
        assert!(s.contains("  2. [validate::python_parse]"));
    }

    #[test]
    fn render_human_error_schema_reported_does_not_double_prefix_field() {
        let err = JsonError {
            code: "validate::failed".into(),
            message: "1 validation diagnostic(s)".into(),
            field: None,
            details: None,
            diagnostics: vec![je(
                "validate::schema_reported",
                "plugin name \"X\" must match ...",
                Some("plugin.name"),
            )],
            cause: vec![],
        };
        let mut buf = Vec::new();
        render_human_error(&err, plain(), &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert_eq!(s.matches("plugin.name:").count(), 1);
    }
}
