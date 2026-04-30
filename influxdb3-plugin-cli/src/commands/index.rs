//! `influxdb3-plugin index` — read-only inspection of a local registry index.
//!
//! This module owns CLI concerns only: argument parsing, filesystem reads,
//! stable JSON projection, and human rendering. Search/filter semantics stay
//! in `influxdb3-plugin-schemas` through `Index::search` and `Index::info`.

use clap::{Args as ClapArgs, Subcommand, ValueEnum};
use influxdb3_plugin_schemas::{
    Dependencies, Index, IndexInfo, IndexInfoQuery, IndexInfoResult, IndexSearchHit,
    IndexSearchQuery, IndexVersionVisibility, IndexVisibilityReason, PluginName, TriggerType,
};
use influxdb3_plugin_sdk::ValidationError;
use std::path::{Path, PathBuf};

use crate::cli_error::CliError;
use crate::output::error_mapping::json_error_from_validation;
use crate::output::json::{
    IndexDependenciesOutput, IndexInfoOutput, IndexInfoPluginOutput, IndexSearchHitOutput,
    IndexSearchOutput, IndexVisibilityOutput, IndexVisibilityReasonOutput, JsonError,
    write_envelope_ok,
};
use crate::output::{Env, OutputMode, RealEnv, resolve_output_mode};

/// `index` command namespace.
#[derive(Debug, Subcommand)]
pub(crate) enum IndexCommand {
    /// Search plugins in a local registry index.
    Search(SearchArgs),
    /// Inspect one plugin in a local registry index.
    Info(InfoArgs),
}

impl IndexCommand {
    pub(crate) fn run(self) -> anyhow::Result<()> {
        match self {
            Self::Search(args) => run_search(args, &RealEnv),
            Self::Info(args) => run_info(args, &RealEnv),
        }
    }
}

/// Parsed `index search` arguments.
#[derive(Debug, ClapArgs)]
pub(crate) struct SearchArgs {
    /// Optional text query. Omitted or whitespace-only matches all visible plugins.
    #[arg(value_name = "QUERY")]
    query: Option<String>,

    /// Output format. Auto-detected from stdout's TTY status and `CI` when omitted.
    #[arg(long, value_enum)]
    output: Option<OutputMode>,

    /// Input registry index (read-only).
    #[arg(long)]
    index: PathBuf,

    /// Keep only plugins whose selected matching version supports this trigger type.
    #[arg(long, value_enum)]
    trigger_type: Option<TriggerTypeArg>,

    /// Database version used for compatibility filtering.
    #[arg(long)]
    database_version: Option<String>,

    /// Include yanked versions in search selection.
    #[arg(long)]
    include_yanked: bool,

    /// Include versions incompatible with --database-version in search selection.
    #[arg(long)]
    include_incompatible: bool,
}

/// Parsed `index info` arguments.
#[derive(Debug, ClapArgs)]
pub(crate) struct InfoArgs {
    /// Plugin name to inspect.
    #[arg(value_name = "NAME")]
    name: String,

    /// Output format. Auto-detected from stdout's TTY status and `CI` when omitted.
    #[arg(long, value_enum)]
    output: Option<OutputMode>,

    /// Input registry index (read-only).
    #[arg(long)]
    index: PathBuf,

    /// Exact plugin version to inspect.
    #[arg(long)]
    version: Option<String>,

    /// Database version used for compatibility visibility.
    #[arg(long)]
    database_version: Option<String>,

    /// Include yanked versions when selecting by name without --version.
    #[arg(long)]
    include_yanked: bool,

    /// Include incompatible versions when selecting by name without --version.
    #[arg(long)]
    include_incompatible: bool,
}

/// Clap-facing trigger enum. Keeping this separate from the schema enum gives
/// clap a closed value list and the standard invalid-value usage error.
#[derive(Debug, Clone, Copy, ValueEnum)]
#[clap(rename_all = "snake_case")]
enum TriggerTypeArg {
    #[value(name = "process_writes")]
    Writes,
    #[value(name = "process_scheduled_call")]
    ScheduledCall,
    #[value(name = "process_request")]
    Request,
}

impl From<TriggerTypeArg> for TriggerType {
    fn from(value: TriggerTypeArg) -> Self {
        match value {
            TriggerTypeArg::Writes => TriggerType::ProcessWrites,
            TriggerTypeArg::ScheduledCall => TriggerType::ProcessScheduledCall,
            TriggerTypeArg::Request => TriggerType::ProcessRequest,
        }
    }
}

fn run_search(args: SearchArgs, env: &dyn Env) -> anyhow::Result<()> {
    let mode = resolve_output_mode(args.output, env);
    let database_version = parse_database_version(args.database_version)?;
    let index = read_index(&args.index)?;

    let query = IndexSearchQuery {
        query: args.query,
        trigger_type: args.trigger_type.map(Into::into),
        database_version,
        include_yanked: args.include_yanked,
        include_incompatible: args.include_incompatible,
    };
    let result = index.search(&query);
    let payload = search_output(result.hits);

    match mode {
        OutputMode::Human => render_search_human(&payload, &mut std::io::stdout())?,
        OutputMode::Json => write_envelope_ok(&mut std::io::stdout(), &payload)?,
    }
    Ok(())
}

fn run_info(args: InfoArgs, env: &dyn Env) -> anyhow::Result<()> {
    let mode = resolve_output_mode(args.output, env);
    let name = parse_plugin_name(args.name)?;
    let version = parse_exact_version(args.version)?;
    let database_version = parse_database_version(args.database_version)?;
    let index = read_index(&args.index)?;

    let query = IndexInfoQuery {
        name,
        version,
        database_version,
        include_yanked: args.include_yanked,
        include_incompatible: args.include_incompatible,
    };
    let result = index.info(&query);
    let payload = info_output(result);

    match mode {
        OutputMode::Human => render_info_human(&payload, &mut std::io::stdout())?,
        OutputMode::Json => write_envelope_ok(&mut std::io::stdout(), &payload)?,
    }
    Ok(())
}

fn parse_database_version(raw: Option<String>) -> anyhow::Result<Option<semver::Version>> {
    raw.map(|value| {
        semver::Version::parse(&value).map_err(|source| {
            CliError::usage(JsonError {
                code: "usage::invalid_database_version".into(),
                message: format!("invalid --database-version {value:?}: {source}"),
                field: Some("--database-version".into()),
                details: Some(serde_json::json!({
                    "value": value,
                    "reason": source.to_string(),
                })),
                diagnostics: vec![],
                cause: vec![],
            })
        })
    })
    .transpose()
}

fn parse_exact_version(raw: Option<String>) -> anyhow::Result<Option<semver::Version>> {
    raw.map(|value| {
        semver::Version::parse(&value).map_err(|source| {
            CliError::usage(JsonError {
                code: "usage::value_validation".into(),
                message: format!("invalid --version {value:?}: {source}"),
                field: Some("--version".into()),
                details: Some(serde_json::json!({
                    "arg": "--version",
                    "value": value,
                    "reason": source.to_string(),
                })),
                diagnostics: vec![],
                cause: vec![],
            })
        })
    })
    .transpose()
}

fn parse_plugin_name(raw: String) -> anyhow::Result<PluginName> {
    raw.parse::<PluginName>().map_err(|source| {
        CliError::usage(JsonError {
            code: "usage::invalid_name".into(),
            message: format!("invalid plugin name {raw:?}: {source}"),
            field: Some("NAME".into()),
            details: Some(serde_json::json!({
                "value": raw,
                "reason": source.to_string(),
            })),
            diagnostics: vec![],
            cause: vec![],
        })
    })
}

fn read_index(path: &Path) -> anyhow::Result<Index> {
    let raw = std::fs::read_to_string(path).map_err(|e| {
        CliError::runtime(JsonError {
            code: "index::index_read_failed".into(),
            message: format!("failed to read --index {}: {e}", path.display()),
            field: Some(path.display().to_string()),
            details: Some(serde_json::json!({
                "path": path.display().to_string(),
                "io_kind": format!("{:?}", e.kind()),
            })),
            diagnostics: vec![],
            cause: vec![e.to_string()],
        })
    })?;

    Index::parse_json(&raw).map_err(|schema_errors| {
        let diagnostics = schema_errors
            .into_iter()
            .map(|reported| json_error_from_validation(&ValidationError::SchemaReported(reported)))
            .collect();

        CliError::runtime(JsonError {
            code: "index::index_parse_failed".into(),
            message: format!(
                "failed to parse --index {} as a registry index",
                path.display()
            ),
            field: Some(path.display().to_string()),
            details: None,
            diagnostics,
            cause: vec![],
        })
    })
}

fn search_output(hits: Vec<IndexSearchHit>) -> IndexSearchOutput {
    IndexSearchOutput {
        hits: hits.into_iter().map(search_hit_output).collect(),
    }
}

fn search_hit_output(hit: IndexSearchHit) -> IndexSearchHitOutput {
    IndexSearchHitOutput {
        name: hit.name.as_str().to_owned(),
        version: hit.version.to_string(),
        published_at: hit.published_at.as_str().to_owned(),
        description: hit.description.as_str().to_owned(),
        triggers: trigger_strings(&hit.triggers),
        visibility: visibility_output(hit.visibility),
    }
}

fn info_output(result: IndexInfoResult) -> IndexInfoOutput {
    match result {
        IndexInfoResult::Found(info) => IndexInfoOutput::Found {
            plugin: Box::new(info_plugin_output(*info)),
        },
        IndexInfoResult::NotFound { name, version } => IndexInfoOutput::NotFound {
            name: name.as_str().to_owned(),
            version: version.map(|v| v.to_string()),
        },
        IndexInfoResult::FilteredOut {
            name,
            version,
            reasons,
        } => IndexInfoOutput::FilteredOut {
            name: name.as_str().to_owned(),
            version: version.map(|v| v.to_string()),
            reasons: reasons.into_iter().map(reason_output).collect(),
        },
    }
}

fn info_plugin_output(info: IndexInfo) -> IndexInfoPluginOutput {
    IndexInfoPluginOutput {
        name: info.name.as_str().to_owned(),
        version: info.version.to_string(),
        published_at: info.published_at.as_str().to_owned(),
        description: info.description.as_str().to_owned(),
        triggers: trigger_strings(&info.triggers),
        homepage: info.homepage.map(|url| url.to_string()),
        repository: info.repository.map(|url| url.to_string()),
        documentation: info.documentation.map(|url| url.to_string()),
        dependencies: dependencies_output(info.dependencies),
        hash: info.hash.as_str().to_owned(),
        visibility: visibility_output(info.visibility),
    }
}

fn trigger_strings(triggers: &[TriggerType]) -> Vec<String> {
    triggers.iter().map(|t| t.as_str().to_owned()).collect()
}

fn dependencies_output(deps: Dependencies) -> IndexDependenciesOutput {
    IndexDependenciesOutput {
        database_version: deps.database_version.to_string(),
        python: deps
            .python
            .into_iter()
            .map(|p| p.as_str().to_owned())
            .collect(),
    }
}

fn visibility_output(vis: IndexVersionVisibility) -> IndexVisibilityOutput {
    match vis {
        IndexVersionVisibility::Visible => IndexVisibilityOutput::Visible,
        IndexVersionVisibility::Hidden { reasons } => IndexVisibilityOutput::Hidden {
            reasons: reasons.into_iter().map(reason_output).collect(),
        },
    }
}

fn reason_output(reason: IndexVisibilityReason) -> IndexVisibilityReasonOutput {
    match reason {
        IndexVisibilityReason::Yanked => IndexVisibilityReasonOutput::Yanked,
        IndexVisibilityReason::IncompatibleDatabaseVersion { required, actual } => {
            IndexVisibilityReasonOutput::IncompatibleDatabaseVersion {
                required: required.to_string(),
                actual: actual.to_string(),
            }
        }
    }
}

fn render_search_human(
    payload: &IndexSearchOutput,
    writer: &mut impl std::io::Write,
) -> std::io::Result<()> {
    if payload.hits.is_empty() {
        writeln!(writer, "No matching plugins found.")?;
        return Ok(());
    }

    let rows: Vec<_> = payload
        .hits
        .iter()
        .map(|hit| {
            let triggers = hit.triggers.join(",");
            let description = match &hit.visibility {
                IndexVisibilityOutput::Visible => hit.description.clone(),
                IndexVisibilityOutput::Hidden { reasons } => {
                    format!("{} {}", hidden_marker(reasons), hit.description)
                }
            };
            (hit, triggers, description)
        })
        .collect();

    let name_width = rows
        .iter()
        .map(|(hit, _, _)| hit.name.len())
        .max()
        .unwrap_or(0);
    let version_width = rows
        .iter()
        .map(|(hit, _, _)| hit.version.len())
        .max()
        .unwrap_or(0);
    let triggers_width = rows
        .iter()
        .map(|(_, triggers, _)| triggers.len())
        .max()
        .unwrap_or(0);

    for (hit, triggers, description) in rows {
        writeln!(
            writer,
            "{:<name_width$}  {:<version_width$}  {:<triggers_width$}  {}",
            hit.name, hit.version, triggers, description
        )?;
    }
    Ok(())
}

fn render_info_human(
    payload: &IndexInfoOutput,
    writer: &mut impl std::io::Write,
) -> std::io::Result<()> {
    match payload {
        IndexInfoOutput::Found { plugin } => {
            writeln!(writer, "{}", plugin.name)?;
            writeln!(writer, "{}", plugin.description)?;
            writeln!(writer, "version: {}", plugin.version)?;
            writeln!(writer, "published_at: {}", plugin.published_at)?;
            writeln!(writer, "triggers: {}", plugin.triggers.join(","))?;
            writeln!(writer, "database: {}", plugin.dependencies.database_version)?;
            writeln!(
                writer,
                "python: {}",
                if plugin.dependencies.python.is_empty() {
                    "<none>".to_owned()
                } else {
                    plugin.dependencies.python.join(", ")
                }
            )?;
            if let Some(homepage) = &plugin.homepage {
                writeln!(writer, "homepage: {homepage}")?;
            }
            if let Some(repository) = &plugin.repository {
                writeln!(writer, "repository: {repository}")?;
            }
            if let Some(documentation) = &plugin.documentation {
                writeln!(writer, "documentation: {documentation}")?;
            }
            writeln!(writer, "hash: {}", plugin.hash)?;
            writeln!(
                writer,
                "visibility: {}",
                visibility_label(&plugin.visibility)
            )?;
        }
        IndexInfoOutput::NotFound { name, version } => match version {
            Some(version) => writeln!(writer, "Plugin version not found: {name}@{version}")?,
            None => writeln!(writer, "Plugin not found: {name}")?,
        },
        IndexInfoOutput::FilteredOut { name, reasons, .. } => {
            writeln!(writer, "No selectable version found for {name}.")?;
            for reason in reasons {
                writeln!(writer, "  reason: {}", reason_label(reason))?;
            }
        }
    }
    Ok(())
}

fn visibility_label(visibility: &IndexVisibilityOutput) -> String {
    match visibility {
        IndexVisibilityOutput::Visible => "visible".to_owned(),
        IndexVisibilityOutput::Hidden { reasons } => {
            format!("hidden ({})", reason_list_label(reasons))
        }
    }
}

fn hidden_marker(reasons: &[IndexVisibilityReasonOutput]) -> String {
    format!("[{}]", reason_list_label(reasons))
}

fn reason_list_label(reasons: &[IndexVisibilityReasonOutput]) -> String {
    reasons
        .iter()
        .map(reason_label)
        .collect::<Vec<_>>()
        .join(", ")
}

fn reason_label(reason: &IndexVisibilityReasonOutput) -> String {
    match reason {
        IndexVisibilityReasonOutput::Yanked => "yanked".to_owned(),
        IndexVisibilityReasonOutput::IncompatibleDatabaseVersion { required, actual } => {
            format!("incompatible: requires {required}, actual {actual}")
        }
    }
}
