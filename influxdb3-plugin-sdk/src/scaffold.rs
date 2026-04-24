//! Template-driven scaffolding for new plugin and registry directories.
//!
//! Templates are compiled-in via `include_str!`; user-extensible templates
//! are out of v1 scope.
//!
//! Both scaffolds create `dir` if missing. When `overwrite` is false, they
//! reject if any file they would write already exists (no partial
//! scaffolds). When `overwrite` is true, conflicting files in the
//! template's write set are replaced; unrelated files in `dir` are left
//! alone.

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

/// Default `database_version` baked into a scaffolded manifest when the
/// caller doesn't override it. Matches the SemVer floor every v1
/// processing-engine release supports; release engineering may inject a
/// tighter pin via the CLI's `--database-version` flag.
pub const DEFAULT_DATABASE_VERSION: &str = ">=3.0.0";

/// Scaffolds a new plugin directory under `dir`.
///
/// Writes `manifest.toml`, `__init__.py`, and `README.md`. `{{name}}` and
/// `{{database_version}}` placeholders in the manifest template are filled
/// from `name` and `database_version` (defaulting to
/// [`DEFAULT_DATABASE_VERSION`]). The init template carries a stub for the
/// declared `trigger`.
///
/// When `overwrite` is true, existing files in the template's write set are
/// replaced; when false, the scaffold errors if any already exist.
///
/// # Errors
///
/// `SdkError::Schema` if `name` fails the `PluginName` check. `SdkError::Io`
/// if any target path already exists (only when `overwrite` is false) or on
/// file-write failure.
pub fn plugin(
    dir: &Path,
    name: &str,
    trigger: TriggerType,
    database_version: Option<&str>,
    overwrite: bool,
) -> Result<(), SdkError> {
    // Fail fast on bad name, before touching the filesystem.
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
    if !overwrite {
        check_no_existing(&[&manifest_path, &init_path, &readme_path])?;
    }

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
/// Writes `dir/index.json` with `index_schema_version = "1.0"`, an empty
/// `plugins` array, and `artifacts_url` set to either `artifacts_url` or,
/// when `None`, `file://<absolute dir>` — making a fresh local registry
/// immediately usable as a `file://` registry.
///
/// When `overwrite` is true, an existing `index.json` is replaced. When
/// false, the scaffold errors if `index.json` already exists.
///
/// # Errors
///
/// `SdkError::Io` if `dir/index.json` already exists (only when `overwrite`
/// is false) or on write failure. `SdkError::Archive` when an auto-derived
/// `file://` URL cannot be built from `dir` (rare; Windows UNC-path edge
/// case).
pub fn registry(dir: &Path, artifacts_url: Option<&str>, overwrite: bool) -> Result<(), SdkError> {
    let index_path = dir.join("index.json");

    ensure_dir(dir)?;
    if !overwrite {
        check_no_existing(&[&index_path])?;
    }

    let url_string: String = match artifacts_url {
        Some(url) => url.to_owned(),
        None => {
            let absolute = std::fs::canonicalize(dir).map_err(|source| SdkError::Io {
                source,
                path: Some(dir.to_path_buf()),
            })?;
            // `Url::from_file_path` handles Windows UNC paths and non-UTF8
            // bytes correctly; naïve `format!("file://{}", path.display())`
            // does not.
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
                // Bare message — the outer `SdkError::Io`'s Display adds
                // ` at {path}` via `path_suffix`, so including the path here
                // too produces a duplicate in the rendered error chain.
                source: std::io::Error::new(std::io::ErrorKind::AlreadyExists, "already exists"),
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
        plugin(&dir, "my-plugin", TriggerType::ProcessWrites, None, false).unwrap();

        assert!(dir.join("manifest.toml").exists());
        assert!(dir.join("__init__.py").exists());
        assert!(dir.join("README.md").exists());
    }

    #[test]
    fn scaffold_writes_valid_manifest() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("downsampler");
        plugin(&dir, "downsampler", TriggerType::ProcessWrites, None, false).unwrap();

        let raw = fs::read_to_string(dir.join("manifest.toml")).unwrap();
        let manifest = Manifest::parse_toml(&raw).expect("scaffolded manifest must parse");
        assert_eq!(manifest.plugin.name.as_str(), "downsampler");
        assert_eq!(manifest.plugin.triggers, vec![TriggerType::ProcessWrites]);
    }

    #[test]
    fn scaffold_database_version_default_and_override() {
        let td = tempfile::tempdir().unwrap();
        let default_dir = td.path().join("default");
        let override_dir = td.path().join("override");

        plugin(
            &default_dir,
            "default",
            TriggerType::ProcessWrites,
            None,
            false,
        )
        .unwrap();
        plugin(
            &override_dir,
            "override",
            TriggerType::ProcessWrites,
            Some(">=3.5,<4"),
            false,
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
        Manifest::parse_toml(&override_raw)
            .expect("override-database-version manifest must round-trip via schemas");
    }

    #[test]
    fn scaffold_rejects_invalid_name_up_front() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("bad-name-test");
        let err = plugin(&dir, "1bad", TriggerType::ProcessWrites, None, false).unwrap_err();
        assert!(matches!(
            err,
            SdkError::Schema(influxdb3_plugin_schemas::SchemaError::InvalidPluginName { .. })
        ));
        assert!(!dir.join("manifest.toml").exists());
    }

    #[test]
    fn scaffold_rejects_existing_files() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("plugin");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("manifest.toml"), "pre-existing").unwrap();

        let err = plugin(&dir, "plugin", TriggerType::ProcessWrites, None, false).unwrap_err();
        assert!(matches!(err, SdkError::Io { .. }));
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
            plugin(&dir, "p", trigger, None, false).unwrap();
            let init = fs::read_to_string(dir.join("__init__.py")).unwrap();
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
        registry(&dir, None, false).unwrap();

        let raw = fs::read_to_string(dir.join("index.json")).unwrap();
        let index = Index::parse_json(&raw).expect("scaffolded index must parse");
        assert!(index.plugins.is_empty());
        let url = index.artifacts_url.to_string();
        assert!(url.starts_with("file://"), "got: {url}");
    }

    /// Regression guard against formatting `file://` URLs by hand: the
    /// scaffolded `artifacts_url` must round-trip via `url::Url::parse` +
    /// `.to_file_path()` back to the absolute scaffold directory. The
    /// previous `format!("file://{}", path.display())` form produced
    /// malformed URLs on Windows UNC paths and on non-UTF8 bytes.
    #[test]
    fn scaffold_registry_artifacts_url_is_valid_file_url() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("my-registry");
        registry(&dir, None, false).unwrap();

        let raw = fs::read_to_string(dir.join("index.json")).unwrap();
        let index = Index::parse_json(&raw).unwrap();
        let url_str = index.artifacts_url.to_string();

        let parsed = url::Url::parse(&url_str).expect("artifacts_url must be a valid URL");
        assert_eq!(parsed.scheme(), "file");

        let recovered = parsed
            .to_file_path()
            .expect("file URL must convert back to a path");
        let expected = std::fs::canonicalize(&dir).unwrap();
        assert_eq!(recovered, expected);
    }

    /// Explicit `artifacts_url` is written verbatim (no canonicalize, no
    /// `file://` prefix), so http/https values pass through unchanged.
    #[test]
    fn scaffold_registry_uses_explicit_artifacts_url() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("r");
        registry(&dir, Some("https://plugins.example.com/artifacts"), false).unwrap();

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

        let err = registry(&dir, None, false).unwrap_err();
        assert!(matches!(err, SdkError::Io { .. }));
        // Pre-existing content must survive — no partial write.
        assert_eq!(fs::read_to_string(dir.join("index.json")).unwrap(), "{}");
    }

    #[test]
    fn scaffold_plugin_overwrite_replaces_existing_files() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("plugin");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("manifest.toml"), "pre-existing").unwrap();
        fs::write(dir.join("__init__.py"), "pre-existing").unwrap();
        fs::write(dir.join("README.md"), "pre-existing").unwrap();

        plugin(&dir, "plugin", TriggerType::ProcessWrites, None, true).unwrap();

        let manifest = fs::read_to_string(dir.join("manifest.toml")).unwrap();
        assert!(
            manifest.contains("name = \"plugin\""),
            "manifest not replaced: {manifest}"
        );
        let init = fs::read_to_string(dir.join("__init__.py")).unwrap();
        assert!(
            init.contains("def process_writes("),
            "init not replaced: {init}"
        );
        let readme = fs::read_to_string(dir.join("README.md")).unwrap();
        assert!(
            !readme.contains("pre-existing"),
            "readme not replaced: {readme}"
        );
    }

    #[test]
    fn scaffold_plugin_overwrite_leaves_unrelated_files_alone() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("plugin");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("notes.txt"), "keep me").unwrap();

        plugin(&dir, "plugin", TriggerType::ProcessWrites, None, true).unwrap();

        assert_eq!(
            fs::read_to_string(dir.join("notes.txt")).unwrap(),
            "keep me"
        );
        assert!(dir.join("manifest.toml").exists());
    }

    #[test]
    fn scaffold_registry_overwrite_replaces_existing_index() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("r");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("index.json"), "{}").unwrap();

        registry(&dir, Some("https://x.example/"), true).unwrap();

        let raw = fs::read_to_string(dir.join("index.json")).unwrap();
        assert!(
            raw.contains("https://x.example/"),
            "index not replaced: {raw}"
        );
    }
}
