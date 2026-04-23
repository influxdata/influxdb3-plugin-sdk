//! Template-driven scaffolding for new plugin directories and registry
//! directories.
//!
//! Templates are hardcoded module-level `&'static str` constants via
//! `include_str!`. User-extensible templates are out of v1 scope.
//!
//! Per Spec 2 Commands (`new`): both `plugin` and `registry` scaffolds
//! create their output directory if missing, and reject if any file they
//! would write already exists. No partial scaffolds — the command either
//! writes its full file set or nothing.

use influxdb3_plugin_schemas::{PluginName, TriggerType};
use std::path::{Path, PathBuf};

use crate::SdkError;

const PROCESS_WRITES_MANIFEST: &str = include_str!("templates/process_writes_manifest.toml");
const PROCESS_WRITES_INIT: &str = include_str!("templates/process_writes_init.py");
const PROCESS_SCHEDULED_CALL_MANIFEST: &str =
    include_str!("templates/process_scheduled_call_manifest.toml");
const PROCESS_SCHEDULED_CALL_INIT: &str = include_str!("templates/process_scheduled_call_init.py");
const PROCESS_REQUEST_MANIFEST: &str = include_str!("templates/process_request_manifest.toml");
const PROCESS_REQUEST_INIT: &str = include_str!("templates/process_request_init.py");
const REGISTRY_INDEX: &str = include_str!("templates/registry_index.json");
const README: &str = include_str!("templates/readme.md");

/// Default `database_version` written into a scaffolded manifest when the
/// caller does not override it. Matches the SemVer floor every v1
/// processing-engine release supports; release engineering may inject a
/// tighter pin via the CLI's `--database-version` flag (Spec 2 § Release
/// Engineering's `SDK_KNOWN_LATEST_DB`).
pub const DEFAULT_DATABASE_VERSION: &str = ">=3.0.0";

/// Scaffolds a new plugin directory under `dir`.
///
/// Writes three files at `dir/`:
/// - `manifest.toml` — from the trigger-specific template, with `{{name}}`
///   replaced by `name` and `{{database_version}}` replaced by
///   `database_version` (or [`DEFAULT_DATABASE_VERSION`] if `None`).
/// - `__init__.py` — from the trigger-specific template, containing a stub
///   implementation of the declared trigger
/// - `README.md` — generic stub with the plugin name
///
/// # Errors
///
/// Returns `SdkError::Schema` if `name` doesn't satisfy the `PluginName`
/// regex. Returns `SdkError::Io` if any of the three target paths already
/// exist, or on any file-creation failure. Creates `dir` if missing.
pub fn plugin(
    dir: &Path,
    name: &str,
    trigger: TriggerType,
    database_version: Option<&str>,
) -> Result<(), SdkError> {
    // Validate the name up-front. Fail fast before touching the filesystem.
    let _validated: PluginName = name.parse()?;

    let manifest_template = match trigger {
        TriggerType::ProcessWrites => PROCESS_WRITES_MANIFEST,
        TriggerType::ProcessScheduledCall => PROCESS_SCHEDULED_CALL_MANIFEST,
        TriggerType::ProcessRequest => PROCESS_REQUEST_MANIFEST,
    };
    let init_template = match trigger {
        TriggerType::ProcessWrites => PROCESS_WRITES_INIT,
        TriggerType::ProcessScheduledCall => PROCESS_SCHEDULED_CALL_INIT,
        TriggerType::ProcessRequest => PROCESS_REQUEST_INIT,
    };

    let manifest_path = dir.join("manifest.toml");
    let init_path = dir.join("__init__.py");
    let readme_path = dir.join("README.md");

    ensure_dir(dir)?;
    check_no_existing(&[&manifest_path, &init_path, &readme_path])?;

    let db_ver = database_version.unwrap_or(DEFAULT_DATABASE_VERSION);
    let manifest = manifest_template
        .replace("{{name}}", name)
        .replace("{{database_version}}", db_ver);
    write_file(&manifest_path, &manifest)?;
    write_file(&init_path, init_template)?;
    write_file(&readme_path, &README.replace("{{name}}", name))?;
    Ok(())
}

/// Scaffolds a new registry directory under `dir`.
///
/// Writes one file at `dir/index.json` with `index_schema_version = "1.0"`,
/// an empty `plugins` array, and `artifacts_url` set to either the supplied
/// value or — when `artifacts_url` is `None` — `file://<absolute dir>`. The
/// auto-derived default makes a freshly scaffolded local registry
/// immediately usable as a `file://` registry.
///
/// # Errors
///
/// Returns `SdkError::Io` if `dir/index.json` already exists, or on any
/// file-creation failure. Creates `dir` if missing. Returns
/// `SdkError::Archive` when an auto-derived `file://` URL cannot be
/// constructed from `dir` (rare; mostly a Windows UNC-path edge case).
pub fn registry(dir: &Path, artifacts_url: Option<&str>) -> Result<(), SdkError> {
    let index_path = dir.join("index.json");

    ensure_dir(dir)?;
    check_no_existing(&[&index_path])?;

    let url_string: String = match artifacts_url {
        Some(url) => url.to_owned(),
        None => {
            let absolute = std::fs::canonicalize(dir).map_err(|source| SdkError::Io {
                source,
                path: Some(dir.to_path_buf()),
            })?;
            // `Url::from_file_path` is the correct cross-platform way to build a
            // `file://` URL. Naïve `format!("file://{}", path.display())` breaks on
            // Windows UNC paths (`\\?\C:\...`) and mishandles non-UTF8 path bytes.
            url::Url::from_file_path(&absolute)
                .map_err(|()| SdkError::Archive {
                    message: format!(
                        "failed to construct file:// URL from scaffold path {}",
                        absolute.display()
                    ),
                })?
                .to_string()
        }
    };
    let contents = REGISTRY_INDEX.replace("{{artifacts_url}}", &url_string);
    write_file(&index_path, &contents)
}

fn ensure_dir(dir: &Path) -> Result<(), SdkError> {
    std::fs::create_dir_all(dir).map_err(|source| SdkError::Io {
        source,
        path: Some(dir.to_path_buf()),
    })
}

fn check_no_existing(paths: &[&PathBuf]) -> Result<(), SdkError> {
    for path in paths {
        if path.exists() {
            return Err(SdkError::Io {
                source: std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!("{} already exists", path.display()),
                ),
                path: Some((*path).clone()),
            });
        }
    }
    Ok(())
}

fn write_file(path: &Path, contents: &str) -> Result<(), SdkError> {
    std::fs::write(path, contents).map_err(|source| SdkError::Io {
        source,
        path: Some(path.to_path_buf()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use influxdb3_plugin_schemas::{Index, Manifest};
    use std::fs;

    #[test]
    fn scaffold_process_writes_plugin_creates_three_files() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("my-plugin");
        plugin(&dir, "my-plugin", TriggerType::ProcessWrites, None).unwrap();

        assert!(dir.join("manifest.toml").exists());
        assert!(dir.join("__init__.py").exists());
        assert!(dir.join("README.md").exists());
    }

    #[test]
    fn scaffold_writes_valid_manifest() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("downsampler");
        plugin(&dir, "downsampler", TriggerType::ProcessWrites, None).unwrap();

        let raw = fs::read_to_string(dir.join("manifest.toml")).unwrap();
        let manifest = Manifest::parse_toml(&raw).expect("scaffolded manifest must parse");
        assert_eq!(manifest.plugin.name.as_str(), "downsampler");
        assert_eq!(manifest.plugin.triggers, vec![TriggerType::ProcessWrites]);
    }

    /// `database_version = None` falls back to [`DEFAULT_DATABASE_VERSION`];
    /// passing `Some(...)` overrides the templated default end-to-end.
    #[test]
    fn scaffold_database_version_default_and_override() {
        let td = tempfile::tempdir().unwrap();
        let default_dir = td.path().join("default");
        let override_dir = td.path().join("override");

        plugin(&default_dir, "default", TriggerType::ProcessWrites, None).unwrap();
        plugin(
            &override_dir,
            "override",
            TriggerType::ProcessWrites,
            Some(">=3.5,<4"),
        )
        .unwrap();

        let default_raw = fs::read_to_string(default_dir.join("manifest.toml")).unwrap();
        assert!(
            default_raw.contains(&format!(
                "database_version = \"{DEFAULT_DATABASE_VERSION}\""
            )),
            "default manifest should bake in DEFAULT_DATABASE_VERSION, got:\n{default_raw}"
        );

        let override_raw = fs::read_to_string(override_dir.join("manifest.toml")).unwrap();
        assert!(
            override_raw.contains("database_version = \">=3.5,<4\""),
            "override should be substituted into the manifest, got:\n{override_raw}"
        );
        // Override path should also still parse via schemas.
        Manifest::parse_toml(&override_raw)
            .expect("override-database-version manifest must round-trip via schemas");
    }

    #[test]
    fn scaffold_rejects_invalid_name_up_front() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("bad-name-test");
        let err = plugin(&dir, "BAD_NAME", TriggerType::ProcessWrites, None).unwrap_err();
        assert!(matches!(
            err,
            SdkError::Schema(influxdb3_plugin_schemas::SchemaError::InvalidPluginName { .. })
        ));
        // No files written on upfront-failure path.
        assert!(!dir.join("manifest.toml").exists());
    }

    #[test]
    fn scaffold_rejects_existing_files() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("plugin");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("manifest.toml"), "pre-existing").unwrap();

        let err = plugin(&dir, "plugin", TriggerType::ProcessWrites, None).unwrap_err();
        assert!(matches!(err, SdkError::Io { .. }));
        // Original file unchanged.
        assert_eq!(
            fs::read_to_string(dir.join("manifest.toml")).unwrap(),
            "pre-existing"
        );
    }

    #[test]
    fn scaffold_each_trigger_kind_produces_matching_init() {
        for trigger in [
            TriggerType::ProcessWrites,
            TriggerType::ProcessScheduledCall,
            TriggerType::ProcessRequest,
        ] {
            let td = tempfile::tempdir().unwrap();
            let dir = td.path().join("p");
            plugin(&dir, "p", trigger, None).unwrap();
            let init = fs::read_to_string(dir.join("__init__.py")).unwrap();
            // The init stub should define the trigger function by name.
            let expected_def = format!("def {}(", trigger.as_str());
            assert!(
                init.contains(&expected_def),
                "expected `{expected_def}` in {trigger:?} init, got:\n{init}"
            );
        }
    }

    #[test]
    fn scaffold_registry_creates_parseable_index() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("my-registry");
        registry(&dir, None).unwrap();

        let raw = fs::read_to_string(dir.join("index.json")).unwrap();
        let index = Index::parse_json(&raw).expect("scaffolded index must parse");
        assert!(index.plugins.is_empty());
        // artifacts_url should be a file:// URL rooted in the scaffolded dir.
        let url = index.artifacts_url.to_string();
        assert!(url.starts_with("file://"), "got: {url}");
    }

    /// Regression guard: the scaffolded artifacts_url must round-trip through
    /// `url::Url::parse` + `.to_file_path()` back to the absolute scaffold
    /// directory. The earlier `format!("file://{}", path.display())` impl
    /// produced malformed URLs on Windows (UNC `\\?\C:\...` paths) — even on
    /// Unix, formatting via `.display()` can emit non-canonical bytes for
    /// non-UTF8 paths. `url::Url::from_file_path` is the correct API.
    #[test]
    fn scaffold_registry_artifacts_url_is_valid_file_url() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("my-registry");
        registry(&dir, None).unwrap();

        let raw = fs::read_to_string(dir.join("index.json")).unwrap();
        let index = Index::parse_json(&raw).unwrap();
        let url_str = index.artifacts_url.to_string();

        let parsed = url::Url::parse(&url_str).expect("artifacts_url must be a valid URL");
        assert_eq!(parsed.scheme(), "file");

        // Round-trip to a path and compare to the canonical scaffold dir.
        let recovered = parsed
            .to_file_path()
            .expect("file URL must convert back to a path");
        let expected = std::fs::canonicalize(&dir).unwrap();
        assert_eq!(recovered, expected);
    }

    /// Explicit `artifacts_url` is written verbatim — no canonicalize, no
    /// `file://` prefix. Lets the CLI surface its `--artifacts-url` flag
    /// through unchanged, including https:// and http:// values.
    #[test]
    fn scaffold_registry_uses_explicit_artifacts_url() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("r");
        registry(&dir, Some("https://plugins.example.com/artifacts")).unwrap();

        let raw = fs::read_to_string(dir.join("index.json")).unwrap();
        let index = Index::parse_json(&raw).expect("scaffolded index must parse");
        assert_eq!(
            index.artifacts_url.to_string(),
            "https://plugins.example.com/artifacts"
        );
    }

    #[test]
    fn scaffold_registry_rejects_existing_index() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("r");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("index.json"), "{}").unwrap();

        let err = registry(&dir, None).unwrap_err();
        assert!(matches!(err, SdkError::Io { .. }));
    }
}
