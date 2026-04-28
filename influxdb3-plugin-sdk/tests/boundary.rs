//! Verifies that SDK production source and templates do not contain
//! CLI-specific terms that should live only in the CLI crate.

#![allow(unused_crate_dependencies)]

use std::fs;
use std::path::Path;

const BANNED_STRINGS: &[&str] = &[
    "--index",
    "--out",
    "failed to read --index",
    "run `yank`",
    "influxdb3-plugin validate",
    "influxdb3-plugin package",
    "CliError",
];

fn collect_source_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    if !dir.exists() {
        return files;
    }
    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let is_source = matches!(ext, "rs" | "toml" | "json" | "md" | "py");
            let is_snapshot = path.components().any(|c| c.as_os_str() == "snapshots");
            if is_source && !is_snapshot {
                files.push(path.to_path_buf());
            }
        }
    }
    files
}

#[test]
fn sdk_production_code_does_not_contain_cli_terms() {
    let sdk_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files = collect_source_files(&sdk_src);
    assert!(!files.is_empty(), "sanity: found source files");

    let mut violations = Vec::new();
    for file in &files {
        let content = fs::read_to_string(file).unwrap();
        for banned in BANNED_STRINGS {
            if content.contains(banned) {
                violations.push(format!(
                    "{}:{} contains banned string {:?}",
                    file.display(),
                    content
                        .lines()
                        .enumerate()
                        .find(|(_, l)| l.contains(banned))
                        .map(|(n, _)| n + 1)
                        .unwrap_or(0),
                    banned
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "SDK production code contains CLI-specific terms:\n{}",
        violations.join("\n")
    );
}

#[test]
fn sdk_does_not_depend_on_cli_crates() {
    let output = std::process::Command::new("cargo")
        .args(["tree", "-p", "influxdb3-plugin-sdk", "--edges", "normal", "--prefix", "none"])
        .output()
        .expect("cargo tree failed");
    let tree = String::from_utf8_lossy(&output.stdout);

    let banned_deps = ["clap", "anyhow", "tokio", "anstyle"];
    let mut violations = Vec::new();
    for dep in banned_deps {
        if tree.lines().any(|l| l.starts_with(dep) || l.contains(&format!(" {dep} "))) {
            violations.push(dep);
        }
    }

    assert!(
        violations.is_empty(),
        "SDK depends on CLI-only crates: {violations:?}\nFull tree:\n{tree}"
    );
}
