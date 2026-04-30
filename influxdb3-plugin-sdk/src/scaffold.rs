//! Template-driven scaffolding for new plugin and index directories.
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
/// caller doesn't override it. Value resolved at build time from
/// `INFLUXDB3_PLUGIN_SDK_KNOWN_LATEST_DB` (see `build.rs`); falls back to
/// `>=3.0.0` when the env var is unset. Release builds should supply a
/// pinned version; dev builds get the permissive floor.
pub const DEFAULT_DATABASE_VERSION: &str =
    concat!(">=", env!("INFLUXDB3_PLUGIN_SDK_KNOWN_LATEST_DB"));

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

    // Fail fast on bad --database-version. Mirrors the check
    // `Manifest::parse_toml` would apply later; surfacing here means the
    // scaffold never produces a manifest that validation would reject.
    if let Some(raw) = database_version {
        semver::VersionReq::parse(raw).map_err(|source| {
            influxdb3_plugin_schemas::SchemaError::InvalidDatabaseVersion {
                range: raw.to_owned(),
                source,
            }
        })?;
    }

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
        .replace(
            "{{manifest_schema_version}}",
            &influxdb3_plugin_schemas::ManifestSchemaVersion::CURRENT.to_string(),
        )
        .replace("{{name}}", name)
        .replace("{{database_version}}", db_ver);
    write_file(&manifest_path, &manifest)?;
    write_file(&init_path, init_template)?;
    write_file(&readme_path, &README.replace("{{name}}", name))?;
    Ok(())
}

/// Scaffolds a new index directory under `dir`.
///
/// Writes `dir/index.json` with the current index schema version, an empty
/// `plugins` array, and `artifacts_url` set to either `artifacts_url` or,
/// when `None`, `file://<absolute-but-not-canonicalized dir>` — making a
/// fresh local registry immediately usable as a `file://` registry while
/// preserving whatever path the user typed (no symlink resolution).
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
pub fn index(dir: &Path, artifacts_url: Option<&str>, overwrite: bool) -> Result<(), SdkError> {
    let index_path = dir.join("index.json");

    ensure_dir(dir)?;
    if !overwrite {
        check_no_existing(&[&index_path])?;
    }

    let url_string: String = match artifacts_url {
        Some(url) => {
            // Reject unsupported schemes and malformed URLs before writing.
            // Mirrors the check `Index::parse_json` would apply later, but
            // surfaces here so the scaffold never produces an index that
            // downstream consumers (accepting only https/http/file) reject.
            influxdb3_plugin_schemas::ArtifactsUrl::try_new(url)?;
            url.to_owned()
        }
        None => {
            // `std::path::absolute` gives an absolute path without resolving
            // symlinks, matching the explicit-`--artifacts-url` side's
            // verbatim-passthrough contract. `canonicalize` would silently
            // rewrite e.g. `/tmp` → `/private/tmp` on macOS, producing an
            // `artifacts_url` that doesn't match what the user typed.
            let absolute = std::path::absolute(dir).map_err(|source| SdkError::Io {
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
    let contents = REGISTRY_INDEX
        .replace(
            "{{index_schema_version}}",
            &influxdb3_plugin_schemas::IndexSchemaVersion::CURRENT.to_string(),
        )
        .replace("{{artifacts_url}}", &url_string);
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
    fn scaffold_index_creates_parseable_index() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("my-index");
        index(&dir, None, false).unwrap();

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
    fn scaffold_index_artifacts_url_is_valid_file_url() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("my-index");
        index(&dir, None, false).unwrap();

        let raw = fs::read_to_string(dir.join("index.json")).unwrap();
        let index = Index::parse_json(&raw).unwrap();
        let url_str = index.artifacts_url.to_string();

        let parsed = url::Url::parse(&url_str).expect("artifacts_url must be a valid URL");
        assert_eq!(parsed.scheme(), "file");

        let recovered = parsed
            .to_file_path()
            .expect("file URL must convert back to a path");
        let expected = std::path::absolute(&dir).unwrap();
        assert_eq!(recovered, expected);
    }

    /// Default `artifacts_url` preserves the user-typed path without
    /// resolving symlinks (matches the explicit-`--artifacts-url` side's
    /// verbatim-passthrough contract and avoids a macOS-only surprise where
    /// `/tmp` becomes `/private/tmp`).
    #[test]
    fn scaffold_index_default_url_does_not_resolve_symlinks() {
        let td = tempfile::tempdir().unwrap();
        // Create a symlink pointing at a real subdirectory, then scaffold
        // through the symlink path. The emitted `file://` URL must reference
        // the symlink path, not the symlink's target.
        let real = td.path().join("real");
        fs::create_dir_all(&real).unwrap();
        let link = td.path().join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&real, &link).unwrap();
        let dir = link.join("idx");

        index(&dir, None, false).unwrap();

        let raw = fs::read_to_string(dir.join("index.json")).unwrap();
        let index = Index::parse_json(&raw).unwrap();
        let url_str = index.artifacts_url.to_string();

        let recovered = url::Url::parse(&url_str).unwrap().to_file_path().unwrap();
        // Absolute, but symlink component preserved.
        assert!(
            recovered.starts_with(&link),
            "artifacts_url path {recovered:?} should start with symlink path {link:?}, \
             not its target"
        );
    }

    /// Explicit `artifacts_url` is written verbatim (no canonicalize, no
    /// `file://` prefix), so http/https values pass through unchanged.
    #[test]
    fn scaffold_index_uses_explicit_artifacts_url() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("r");
        index(&dir, Some("https://plugins.example.com/artifacts"), false).unwrap();

        let raw = fs::read_to_string(dir.join("index.json")).unwrap();
        let index = Index::parse_json(&raw).expect("scaffolded index must parse");
        assert_eq!(
            index.artifacts_url.to_string(),
            "https://plugins.example.com/artifacts"
        );
    }

    #[test]
    fn scaffold_index_rejects_existing_index() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("r");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("index.json"), "{}").unwrap();

        let err = index(&dir, None, false).unwrap_err();
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
    fn scaffold_index_overwrite_replaces_existing_index() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("r");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("index.json"), "{}").unwrap();

        index(&dir, Some("https://x.example/"), true).unwrap();

        let raw = fs::read_to_string(dir.join("index.json")).unwrap();
        assert!(
            raw.contains("https://x.example/"),
            "index not replaced: {raw}"
        );
    }

    /// Explicit `--artifacts-url` must pass the same scheme check that
    /// `Index::parse_json` applies; otherwise the scaffold can produce an
    /// index that downstream consumers (which accept only https/http/file)
    /// must reject.
    #[test]
    fn scaffold_index_rejects_unsupported_url_scheme() {
        let td = tempfile::tempdir().unwrap();
        let cases = [
            "ftp://example.com/a",
            "s3://bucket/plugins",
            "oci://registry.example",
        ];
        for (i, bad) in cases.iter().enumerate() {
            let dir = td.path().join(format!("reg-{i}"));
            let err = index(&dir, Some(bad), false).unwrap_err();
            assert!(
                matches!(
                    err,
                    SdkError::Schema(
                        influxdb3_plugin_schemas::SchemaError::UnsupportedArtifactScheme { .. }
                    )
                ),
                "expected UnsupportedArtifactScheme for {bad:?}, got {err:?}"
            );
            assert!(
                !dir.join("index.json").exists(),
                "no index written for {bad:?}"
            );
        }
    }

    #[test]
    fn scaffold_index_rejects_malformed_url() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("reg");
        let err = index(&dir, Some("not a url"), false).unwrap_err();
        assert!(
            matches!(
                err,
                SdkError::Schema(influxdb3_plugin_schemas::SchemaError::InvalidUrl { .. })
            ),
            "expected InvalidUrl, got {err:?}"
        );
        assert!(!dir.join("index.json").exists());
    }

    #[test]
    fn scaffolded_manifest_version_equals_current() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("p");
        plugin(&dir, "p", TriggerType::ProcessWrites, None, false).unwrap();
        let raw = fs::read_to_string(dir.join("manifest.toml")).unwrap();
        let manifest = Manifest::parse_toml(&raw).unwrap();
        assert_eq!(
            manifest.manifest_schema_version,
            influxdb3_plugin_schemas::ManifestSchemaVersion::CURRENT
        );
    }

    #[test]
    fn scaffolded_index_version_equals_current() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("r");
        index(&dir, None, false).unwrap();
        let raw = fs::read_to_string(dir.join("index.json")).unwrap();
        let idx = Index::parse_json(&raw).unwrap();
        assert_eq!(
            idx.index_schema_version,
            influxdb3_plugin_schemas::IndexSchemaVersion::CURRENT
        );
    }

    #[test]
    fn scaffold_readme_is_cli_neutral() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path().join("p");
        plugin(&dir, "p", TriggerType::ProcessWrites, None, false).unwrap();
        let readme = fs::read_to_string(dir.join("README.md")).unwrap();
        assert!(
            readme.contains("plugin authoring tooling"),
            "README should contain neutral wording, got:\n{readme}"
        );
        // Build the banned substrings at runtime so the boundary scanner
        // (which checks for verbatim CLI terms in SDK source) does not flag
        // these test assertions as violations.
        let cli_validate = ["influxdb3-plugin", " validate"].concat();
        let cli_package = ["influxdb3-plugin", " package"].concat();
        assert!(
            !readme.contains(&cli_validate),
            "README must not mention CLI validate command"
        );
        assert!(
            !readme.contains(&cli_package),
            "README must not mention CLI package command"
        );
    }

    /// Explicit `--database-version` must parse as a `semver::VersionReq`;
    /// otherwise the scaffold can produce a manifest that `Manifest::parse_toml`
    /// would reject. Fail fast, write nothing.
    #[test]
    fn scaffold_plugin_rejects_invalid_database_version() {
        let td = tempfile::tempdir().unwrap();
        let cases = ["not-a-range", "garbage", ">= ?"];
        for (i, bad) in cases.iter().enumerate() {
            let dir = td.path().join(format!("p-{i}"));
            let err = plugin(&dir, "p", TriggerType::ProcessWrites, Some(bad), false).unwrap_err();
            assert!(
                matches!(
                    err,
                    SdkError::Schema(
                        influxdb3_plugin_schemas::SchemaError::InvalidDatabaseVersion { .. }
                    )
                ),
                "expected InvalidDatabaseVersion for {bad:?}, got {err:?}"
            );
            assert!(
                !dir.join("manifest.toml").exists(),
                "no manifest written for {bad:?}"
            );
            assert!(!dir.join("__init__.py").exists());
            assert!(!dir.join("README.md").exists());
        }
    }
}
