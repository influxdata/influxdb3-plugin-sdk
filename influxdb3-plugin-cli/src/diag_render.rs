//! Shared human-mode renderer for the `diagnostics` list produced by
//! `validate` and (on validation-failure path) `package`.
//!
//! Keeping this in one place locks the "authors fix everything in one
//! pass" shape and avoids two independent text
//! formats drifting. Also houses the canonical
//! `ValidationError -> Diagnostic` mapping so `validate` and `package`
//! share one implementation.

use crate::output::json::Diagnostic;
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
            if p.is_empty() { None } else { Some(p.to_owned()) }
        }
        ValidationError::MissingRequiredFile { file } => Some(file.clone()),
        ValidationError::PythonParse { .. }
        | ValidationError::TriggerNotImplemented { .. }
        | ValidationError::AsyncTriggerFn { .. } => Some("__init__.py".to_owned()),
        ValidationError::NameVersionConflict { name, version } => {
            Some(format!("{name}@{version}"))
        }
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
            d("SchemaReported", "plugin.version: bad", Some("plugin.version")),
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
}
