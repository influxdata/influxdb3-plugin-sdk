//! `new list` subcommand — enumerate built-in templates.
//!
//! Human mode renders a two-column (template name, short name) table;
//! json mode emits a [`ListOutput`] document. Descriptions are
//! intentionally withheld — they appear in `new <template> -h`.
//!
//! `list` writes no files, so its `Args` deliberately does NOT flatten
//! `GlobalFlags`: `--force` is rejected at parse time rather than
//! silently accepted as a no-op.

use crate::commands::new::templates;
use crate::output::{
    OutputMode, RealEnv, json::ListOutput, json::ListTemplate, resolve_output_mode,
};
use clap::Args as ClapArgs;

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Output format. Auto-detected from stdout's TTY status and `CI`
    /// when omitted.
    #[arg(long, value_enum)]
    pub output: Option<OutputMode>,
}

pub(crate) fn run(args: Args) -> anyhow::Result<()> {
    let mode = resolve_output_mode(args.output, &RealEnv);
    let mut stdout = std::io::stdout();
    match mode {
        OutputMode::Human => render_human(&mut stdout)?,
        OutputMode::Json => render_json(&mut stdout)?,
    }
    Ok(())
}

fn render_human(writer: &mut impl std::io::Write) -> std::io::Result<()> {
    let name_width = "Template Name".len().max(
        templates::ALL
            .iter()
            .map(|t| t.name.len())
            .max()
            .unwrap_or(0),
    );
    let short_width = "Short Name".len().max(
        templates::ALL
            .iter()
            .map(|t| t.short_name.len())
            .max()
            .unwrap_or(0),
    );
    writeln!(
        writer,
        "{:<name_width$}  {:<short_width$}",
        "Template Name", "Short Name",
    )?;
    writeln!(writer, "{:-<name_width$}  {:-<short_width$}", "", "",)?;
    for t in templates::ALL {
        writeln!(
            writer,
            "{:<name_width$}  {:<short_width$}",
            t.name, t.short_name,
        )?;
    }
    Ok(())
}

fn render_json(writer: &mut impl std::io::Write) -> anyhow::Result<()> {
    let payload = ListOutput {
        templates: templates::ALL
            .iter()
            .map(|t| ListTemplate {
                name: t.name,
                short_name: t.short_name,
            })
            .collect(),
    };
    serde_json::to_writer_pretty(&mut *writer, &payload)?;
    writeln!(writer)?;
    Ok(())
}
