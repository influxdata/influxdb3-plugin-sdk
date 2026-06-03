# influxdb3-plugin-schemas

Schema types for InfluxDB 3 plugin manifests and registry indexes.

This crate is consumed by:
- [`influxdb3-plugin-sdk`](../influxdb3-plugin-sdk/) — author-side packaging
- [`influxdb3-plugin-cli`](../influxdb3-plugin-cli/) — the `influxdb3-plugin` binary
- the InfluxDB 3 Processing Engine runtime — for install-time manifest parsing

## Overview

The crate exposes the core types plus their supporting newtypes:

- `Manifest` — parsed `manifest.toml` with `PluginMetadata` and `Dependencies`
- `Index` / `IndexEntry` — parsed `index.json` with canonical serialization
  and required per-version `PublishedAt` publication timestamps
- `PluginId` — the `(source, name, version)` identity tuple
- `plugin_format` — the **pure** plugin-directory validation contract: the
  diagnostic type (`ValidationError`), the entry-point classification rule
  (`classify_entry_point`), the trigger-binding rule (`check_trigger_bindings`), the
  extraction rules (`TopLevelFunctionDef`), the success payload (`ValidatedPluginDefinition` /
  `EntryPoint`), and the executable `TOP_LEVEL_DEF_CONFORMANCE_CASES` that any extractor
  must satisfy. This module has no filesystem or `tree-sitter` dependency; the
  SDK supplies the mechanism that feeds these checks.

`Manifest::parse_toml` and `Index::parse_json` perform two-phase parsing:
syntax/required-field decoding first, then field-level validation with
multi-error collection. Structural syntax failures still come back as a
single root-level `SchemaError::TomlParse` / `SchemaError::JsonParse`, but
field-level defects in one document are returned together as
`SchemaErrors`.

## Parsing And Errors

- `Manifest::parse_toml` returns `Result<Manifest, SchemaErrors>`.
- `Index::parse_json` returns `Result<Index, SchemaErrors>`.
- Each `SchemaErrors` contains one or more `ReportedError` values, each with:
  - `path`: the field path where the error was detected
  - `error`: the underlying `SchemaError` variant

Direct callers that previously matched a single `SchemaError` should now
iterate the collection:

```rust
use influxdb3_plugin_schemas::{Manifest, SchemaError};

match Manifest::parse_toml(source) {
    Ok(manifest) => { /* use manifest */ }
    Err(errors) => {
        for reported in &errors {
            match &reported.error {
                SchemaError::InvalidPluginName { .. } => {
                    eprintln!("{}: invalid plugin name", reported.path);
                }
                other => {
                    eprintln!("{}: {other}", reported.path);
                }
            }
        }
    }
}
```

## Stability

Per the plugin SDK design, this crate targets a semver-stable public API.
Schema formats evolve independently via `manifest_schema_version` and
`index_schema_version` fields; this crate exposes those version types as
first-class.

The crate is licensed `MIT OR Apache-2.0`. The stability commitment
applies to the types defined here and is anchored at first crates.io
publish.

## Spec Coverage

Tracks alignment between this crate's parsing/validation behavior and the
internal plugin version management specification. Updated when a deliberate
decision lands or a deviation is reconciled.

### `plugin.name` rule (1–64 characters)

- **Approved rule:** `[a-zA-Z][a-zA-Z0-9_-]*` (1-64 ASCII characters,
  starting with an ASCII letter; Windows reserved device names are
  rejected case-insensitively) — aligned with Cargo's `validate_create_ident`.
- **Code:** enforced by `PluginName::validate` in `src/identity.rs`.
- **Tests:** `plugin_name_length_boundaries` in `identity.rs` pins the
  empty / 1 / 64 / 65-char edges; `reserved_names_rejected` covers the
  Windows-device-name set.
- **Remaining gaps:** none.

### Index-entry validation alignment

- **Approved rule:** index entries follow the same field-level rules as
  manifest entries for `triggers` (closed set, non-empty), optional URL
  schemes (`http` / `https` only), and `dependencies.database_version`
  (SemVer range). Documented in core design doc's "Index-entry validation
  mirrors manifest validation" subsection.
- **Code:** `Index::validate()` extended to enforce non-empty triggers
  and URL-scheme allowlist on every entry. Existing duplicate
  `(name, version)` check unchanged.
- **Tests:** new inline tests in `src/index.rs` for empty triggers,
  invalid URL schemes, and unknown top-level field tolerance. New invalid
  fixtures under `tests/fixtures/invalid/`.
- **Remaining gaps:** none.

### Published plugin-version timestamps

- **Approved rule:** every published plugin-version index entry carries a
  required `published_at` value matching Cargo registry-index `pubtime`:
  `YYYY-MM-DDTHH:MM:SSZ` in UTC, with no offsets or fractional seconds.
- **Code:** `PublishedAt` validates and serializes the timestamp, and
  `IndexEntry` exposes it directly. `Index::parse_json` reports missing,
  non-string, or malformed values at `plugins[N].published_at`.
- **Tests:** inline tests in `src/index.rs`, fixture coverage under
  `tests/fixtures/`, and query tests that ensure search and info results
  expose the selected version's publication timestamp.
- **Remaining gaps:** none.

### Error policy for invalid parsed fields

- **Approved policy:** the parser entry points preserve dedicated
  `SchemaError` variants where defined (`InvalidPluginName`,
  `InvalidVersion`, `InvalidDatabaseVersion`, `InvalidPythonRequirement`,
  `InvalidUrl`, `InvalidUrlScheme`, `InvalidHash`, etc.) and attach field
  paths via `ReportedError`.
- **Boundary:** syntax-level and required-field decode failures still
  surface as root-level `SchemaError::TomlParse` / `JsonParse`.
- **Remaining gaps:** callers that bypass `parse_toml` / `parse_json` and
  deserialize public schema types directly through serde still inherit
  serde's wrapper-style error model rather than field-path-aware
  `ReportedError`s.

### SemVer precedence in canonical ordering

- **Approved rule:** `Index::to_canonical_json` sorts by name ascending
  then version ascending **per SemVer 2.0.0 precedence** (prereleases sort
  before the corresponding release at same major.minor.patch; build
  metadata is ignored).
- **Code:** `to_canonical_json` now uses `Version::cmp_precedence`. The
  earlier `Version::cmp` would have produced lexical ordering of build
  metadata, violating Spec 1's "per SemVer 2.0.0 precedence" wording. This
  bug was latent (no current entries carry build metadata) and was caught
  by the new test below.
- **Tests:** inline tests in `src/index.rs` for prerelease ordering and
  build-metadata equivalence (the latter asserts both `cmp_precedence`
  returns `Equal` and `cmp` does NOT — the second assertion documents why
  the impl had to be `cmp_precedence`).
- **Remaining gaps:** none.
