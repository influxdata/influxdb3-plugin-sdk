//! Shared human-mode renderer for error paths.
//!
//! One entry point — `render_human_error` — dispatches by `JsonError`
//! shape: multi-issue (`diagnostics[]` non-empty) renders the numbered
//! block; single-issue renders one line plus the optional `cause[]`
//! chain.

use crate::output::json::JsonError;
use crate::style::Palette;
use std::io;

/// Top-level entry point for human-mode error rendering. Dispatches on
/// the `JsonError` shape: if `diagnostics[]` is non-empty we render the
/// numbered list block; otherwise we render one line with optional cause
/// chain.
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

    fn plain() -> Palette {
        Palette::default()
    }

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
