//! Shared test fixtures + spawn helpers for CLI integration tests.

#![allow(dead_code)]

use assert_cmd::Command;
use std::path::Path;

pub(crate) const VALID_MANIFEST: &str = r#"manifest_schema_version = "1.0"

[plugin]
name = "downsampler"
version = "1.2.0"
description = "Test plugin."
triggers = ["process_writes"]

[dependencies]
database_version = ">=3.0.0"
"#;

pub(crate) const VALID_INIT: &str = "def process_writes(a, b, c):\n    pass\n";

pub(crate) const EMPTY_INDEX: &str = r#"{
  "index_schema_version": "2.0",
  "artifacts_url": "https://plugins.example.com/artifacts",
  "plugins": []
}
"#;

pub(crate) const SEEDED_INDEX: &str = r#"{
  "index_schema_version": "2.0",
  "artifacts_url": "https://plugins.example.com/artifacts",
  "plugins": [
    {
      "name": "downsampler",
      "version": "1.2.0",
      "published_at": "2026-04-29T18:45:12Z",
      "description": "seed entry",
      "triggers": ["process_writes"],
      "dependencies": { "database_version": ">=3.0.0", "python": [] },
      "hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000"
    }
  ]
}
"#;

pub(crate) fn write_valid_plugin(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("manifest.toml"), VALID_MANIFEST).unwrap();
    std::fs::write(dir.join("__init__.py"), VALID_INIT).unwrap();
}

pub(crate) fn cli_cmd() -> Command {
    Command::cargo_bin("influxdb3-plugin").expect("binary builds")
}
