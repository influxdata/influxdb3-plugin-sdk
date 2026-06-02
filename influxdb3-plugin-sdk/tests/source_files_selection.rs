//! SDK-level integration tests for manifest-driven source-file selection.
//! Tests integration behavior, not gitignore matching the `ignore` crate covers.

#![allow(unused_crate_dependencies)]

mod common;

use common::empty_index;
use influxdb3_plugin_sdk::{archive::canonical_tar_gz, package, plugin_source_files};
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
fn gitignore_files_above_and_in_dir_have_no_effect() {
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
}

#[test]
fn clean_plugin_with_no_exclude_packages_deterministically() {
    // Byte-identity before/after the exclude feature is guaranteed by
    // construction (ASCII flat paths: normalized-string order == old byte
    // order; clean plugin has nothing to exclude). Here we pin determinism.
    let td = tempfile::tempdir().unwrap();
    let dir = td.path().join("p");
    write(
        &dir,
        "manifest.toml",
        "manifest_schema_version = \"1.2\"\n[plugin]\nname=\"p\"\nversion=\"0.1.0\"\n\
         description=\"x\"\ntriggers=[\"process_writes\"]\n[dependencies]\ndatabase_version=\">=3.0.0\"\n",
    );
    write(&dir, "__init__.py", "def process_writes(a,b,c): pass\n");
    let a = canonical_tar_gz(
        &dir,
        &"p".parse().unwrap(),
        &semver::Version::new(0, 1, 0),
        &[],
    )
    .unwrap();
    let b = canonical_tar_gz(
        &dir,
        &"p".parse().unwrap(),
        &semver::Version::new(0, 1, 0),
        &[],
    )
    .unwrap();
    assert_eq!(a, b);
}
