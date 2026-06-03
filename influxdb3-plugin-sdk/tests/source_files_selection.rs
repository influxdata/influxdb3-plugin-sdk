//! SDK-level integration tests for manifest-driven source-file selection.
//! Tests integration behavior, not gitignore matching the `ignore` crate covers.

#![allow(unused_crate_dependencies)]

mod common;

use common::empty_index;
use influxdb3_plugin_sdk::{archive::canonical_tar_gz, package, plugin_source_files, validate};
use std::fs;

fn write(dir: &std::path::Path, rel: &str, body: &str) {
    let p = dir.join(rel);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(p, body).unwrap();
}

fn list_tar_paths(bytes: &[u8]) -> Vec<String> {
    use flate2::read::GzDecoder;
    let mut decoder = GzDecoder::new(bytes);
    let mut tar_bytes = Vec::new();
    std::io::copy(&mut decoder, &mut tar_bytes).unwrap();
    let mut archive = tar::Archive::new(std::io::Cursor::new(tar_bytes));
    archive
        .entries_with_seek()
        .unwrap()
        .filter_map(|e| {
            e.ok()
                .and_then(|e| e.header().path().ok().map(|p| p.display().to_string()))
        })
        .collect()
}

const MANIFEST_WITH_EXCLUDE: &str = "manifest_schema_version = \"1.2\"\n\
    [plugin]\nname = \"downsampler\"\nversion = \"1.2.0\"\ndescription = \"x\"\n\
    triggers = [\"process_writes\"]\nexclude = [\"tests/**\", \"*.pyc\"]\n\
    [dependencies]\ndatabase_version = \">=3.0.0\"\n";

#[test]
fn validate_and_package_select_identical_sets() {
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("p");
    write(&dir, "manifest.toml", MANIFEST_WITH_EXCLUDE);
    write(&dir, "__init__.py", "def process_writes(a,b,c): pass\n");
    write(&dir, "tests/test_it.py", "ignored");
    write(&dir, "build.pyc", "ignored");

    let exclude = vec!["tests/**".to_string(), "*.pyc".to_string()];
    let selected: Vec<String> = plugin_source_files::select(&dir, &exclude)
        .unwrap()
        .into_iter()
        .map(|f| f.normalized)
        .collect();
    assert_eq!(selected, vec!["__init__.py", "manifest.toml"]);

    // Package's archive listing must equal validate's selected set, under root.
    let out = package::package_plugin(&dir, empty_index()).unwrap();
    let mut packaged: Vec<String> = list_tar_paths(&out.archive_bytes)
        .into_iter()
        .map(|p| p.trim_start_matches("downsampler-1.2.0/").to_string())
        .collect();
    packaged.sort();
    assert_eq!(
        packaged, selected,
        "package archive listing must equal validate's selected set"
    );
}

#[test]
fn ignore_files_have_no_effect_on_selection() {
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("p");
    write(
        &dir,
        "manifest.toml",
        "manifest_schema_version = \"1.2\"\n[plugin]\nname=\"p\"\nversion=\"0.1.0\"\n\
         description=\"x\"\ntriggers=[\"process_writes\"]\n[dependencies]\ndatabase_version=\">=3.0.0\"\n",
    );
    write(&dir, "__init__.py", "def process_writes(a,b,c): pass\n");
    write(&dir, ".gitignore", "__init__.py\n");
    write(td.path(), ".gitignore", "*.py\n"); // above the plugin dir
    write(&dir, ".ignore", "__init__.py\n");
    write(&dir, ".git/info/exclude", "__init__.py\n");
    let selected: Vec<String> = plugin_source_files::select(&dir, &[])
        .unwrap()
        .into_iter()
        .map(|f| f.normalized)
        .collect();
    // No manifest exclude → .gitignore must NOT remove anything.
    assert!(
        selected.contains(&"__init__.py".to_string()),
        "got {selected:?}"
    );
    assert!(
        selected.contains(&".gitignore".to_string()),
        "got {selected:?}"
    );
    assert!(
        selected.contains(&"__init__.py".to_string()),
        ".ignore / .git/info/exclude must not affect selection: {selected:?}"
    );
    // The ignore files themselves are ordinary files under no exclude: none of
    // them is special-cased, and there is no residual hard-coded `.git/`
    // removal, so `.git/info/exclude` is selected like any other file.
    assert!(
        selected.contains(&".ignore".to_string()),
        ".ignore must be selected under no exclude: {selected:?}"
    );
    assert!(
        selected.contains(&".git/info/exclude".to_string()),
        ".git/info/exclude must be selected (no hard-coded .git/ removal): {selected:?}"
    );
}

#[test]
fn exclude_changes_validate_outcome_and_package_contents() {
    // Two top-level .py files with no __init__.py is ambiguous → validate
    // fails. Excluding one resolves it to a single-file plugin, so validate
    // PASSING proves validation applied the manifest exclude; the package
    // archive omitting the excluded file proves packaging used it too.
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("p");
    write(
        &dir,
        "manifest.toml",
        "manifest_schema_version = \"1.2\"\n[plugin]\nname=\"downsampler\"\nversion=\"1.2.0\"\n\
         description=\"x\"\ntriggers=[\"process_writes\"]\nexclude=[\"b.py\"]\n\
         [dependencies]\ndatabase_version=\">=3.0.0\"\n",
    );
    write(&dir, "a.py", "def process_writes(a, b, c):\n    pass\n");
    write(&dir, "b.py", "def helper():\n    pass\n");

    // validate: without the exclude this would be AmbiguousEntryPoint; with
    // exclude=["b.py"] it is the single-file plugin a.py → Ok.
    let validated =
        validate::plugin_dir(&dir).expect("exclude must resolve ambiguity to single-file a.py");
    assert_eq!(validated.manifest.plugin.name.as_str(), "downsampler");

    // package: archive must omit the excluded b.py.
    let out = package::package_plugin(&dir, empty_index()).unwrap();
    let packaged: Vec<String> = list_tar_paths(&out.archive_bytes)
        .into_iter()
        .map(|p| p.trim_start_matches("downsampler-1.2.0/").to_string())
        .collect();
    assert!(
        packaged.contains(&"a.py".to_string()),
        "a.py must be packaged: {packaged:?}"
    );
    assert!(
        !packaged.iter().any(|p| p == "b.py"),
        "b.py must be excluded: {packaged:?}"
    );
    assert!(
        packaged.contains(&"manifest.toml".to_string()),
        "manifest.toml must be packaged: {packaged:?}"
    );
}

#[test]
fn package_plugin_rejects_manifest_excluded_from_selection() {
    use influxdb3_plugin_sdk::SdkError;
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("p");
    write(
        &dir,
        "manifest.toml",
        "manifest_schema_version = \"1.2\"\n[plugin]\nname=\"p\"\nversion=\"0.1.0\"\n\
         description=\"x\"\ntriggers=[\"process_writes\"]\nexclude=[\"manifest.toml\"]\n\
         [dependencies]\ndatabase_version=\">=3.0.0\"\n",
    );
    write(
        &dir,
        "__init__.py",
        "def process_writes(a, b, c):\n    pass\n",
    );
    // Excluding the manifest must make packaging fail (no manifest-less archive),
    // surfacing the missing required file as a validation error.
    let err = package::package_plugin(&dir, empty_index()).unwrap_err();
    match err {
        SdkError::ValidationErrors(errs) => assert!(
            errs.iter().any(|e| matches!(
                e, influxdb3_plugin_schemas::ValidationError::MissingRequiredFile { file } if file == "manifest.toml")),
            "expected MissingRequiredFile(manifest.toml) among {errs:?}"
        ),
        other => panic!("expected ValidationErrors with MissingRequiredFile, got {other:?}"),
    }
}

#[test]
fn package_plugin_rejects_invalid_exclude_pattern() {
    use influxdb3_plugin_sdk::SdkError;
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("p");
    write(
        &dir,
        "manifest.toml",
        "manifest_schema_version = \"1.2\"\n[plugin]\nname=\"p\"\nversion=\"0.1.0\"\n\
         description=\"x\"\ntriggers=[\"process_writes\"]\nexclude=[\"[z-a]\"]\n\
         [dependencies]\ndatabase_version=\">=3.0.0\"\n",
    );
    write(
        &dir,
        "__init__.py",
        "def process_writes(a, b, c):\n    pass\n",
    );
    let err = package::package_plugin(&dir, empty_index()).unwrap_err();
    match err {
        SdkError::InvalidExcludePattern { pattern, .. } => assert_eq!(pattern, "[z-a]"),
        other => panic!("expected SdkError::InvalidExcludePattern, got {other:?}"),
    }
}

#[test]
fn clean_plugin_with_no_exclude_has_stable_archive_bytes() {
    // A clean ASCII no-exec plugin with no exclude must produce byte-stable
    // canonical archive bytes. Pinning the SHA-256 guards against accidental
    // canonicalization drift (sort key, headers, gzip params). The bytes are
    // cross-platform identical because the plugin carries no executable files.
    use influxdb3_plugin_sdk::hash;
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("p");
    write(
        &dir,
        "manifest.toml",
        "manifest_schema_version = \"1.2\"\n[plugin]\nname=\"p\"\nversion=\"0.1.0\"\n\
         description=\"x\"\ntriggers=[\"process_writes\"]\n[dependencies]\ndatabase_version=\">=3.0.0\"\n",
    );
    write(
        &dir,
        "__init__.py",
        "def process_writes(a, b, c):\n    pass\n",
    );
    let bytes = canonical_tar_gz(
        &dir,
        &"p".parse().unwrap(),
        &semver::Version::new(0, 1, 0),
        &[],
    )
    .unwrap();
    let digest = hash::sha256_of_bytes(&bytes);
    assert_eq!(
        digest.as_str(),
        "sha256:9ec4a7ccb674b7f21178bc7dc8fbca65d51501e25ef8032fbd59b7c7ef39abf2",
        "canonical archive bytes drifted; if this is an intentional canonicalization change, update the golden hash"
    );
}
