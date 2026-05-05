# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). The version anchor for a tagged release is the `influxdb3-plugin-cli` crate; library crates (`influxdb3-plugin-schemas`, `influxdb3-plugin-sdk`) may have different versions per the per-crate versioning model documented in `CONTRIBUTING.md`.

## [Unreleased]

### Added
- Single-file plugin support: `validate` and `package` now accept plugin directories containing a single `.py` file (no `__init__.py`) as the entry point.
- New validation errors: `NoEntryPoint` (no `.py` files found) and `AmbiguousEntryPoint` (multiple `.py` files without `__init__.py`).

### Changed
- Validation error messages for `PythonParse`, `TriggerNotImplemented`, and `AsyncTriggerFn` now name the actual entry point file instead of hardcoding `__init__.py`.
- The `missing_init` validation case now produces `NoEntryPoint` instead of `MissingRequiredFile`.

## [0.2.0] - 2026-04-30

### Added
- `influxdb3-plugin-schemas`: required Cargo-style UTC `published_at` timestamps on every published plugin-version entry, exposed through index entries, search hits, and info results.
- `influxdb3-plugin-cli`: package and yank JSON success payloads now include the affected plugin version's publication timestamp.
- `influxdb3-plugin-cli`: `search` and `info` commands for read-only local registry index inspection in human and JSON modes.

### Changed
- `influxdb3-plugin-schemas`: index schema version is now `2.0`; non-empty indexes must include strict `YYYY-MM-DDTHH:MM:SSZ` UTC publication timestamps.
- `influxdb3-plugin-sdk`: package assigns current UTC publication time to new entries and yank/unyank preserves the original publication time.

### Fixed
- `influxdb3-plugin-schemas`: published timestamp tests no longer depend on live wall-clock bounds.

## [0.1.1] - 2026-04-29

### Added
- `influxdb3-plugin-schemas`: index query primitives (`Index::search`, `Index::info`) for shared registry browsing across CLI, UI backend, and database consumers

### Changed
- `new registry` subcommand renamed to `new index` — the command creates an `index.json` file, not a full registry. Template metadata, SDK function, and docs updated accordingly.

### Fixed
- `package` command now emits typed JSON error code `package::already_published` (was `cli::unknown`) when `(name, version)` already exists in the target index. Same fix applied to `CanonicalCollision` errors.
- Future unmapped SDK errors now fall back to command-scoped JSON codes like `package::sdk_error` instead of generic `cli::unknown`.

## [0.1.0] - 2026-04-28

First stable release of the InfluxDB 3 plugin SDK.

### Added
- `influxdb3-plugin` CLI binary with `new`, `validate`, `package`, and `yank` commands
- `influxdb3-plugin-schemas` crate with canonical `Manifest`, `Index`, `PluginId`, and related types
- `influxdb3-plugin-sdk` author-side packaging library (scaffold, validate, package, mutate-index)
- CircleCI CI/CD pipeline: 9 gating checks (build, test, clippy, fmt, deny, doc, manifest-invariants, semver-checks, package-check) + 4-target release workflow (x86_64-linux-gnu, aarch64-linux-gnu, aarch64-apple-darwin, x86_64-windows-gnu)
- Operator tooling: `justfile` with `cut-version`, `tag-version`, `verify-version` recipes
- `RELEASE.md` operator runbook, `CONTRIBUTING.md` with bump rules + cascade docs

## [0.1.0-2.rc.0] - 2026-04-28

Second release rehearsal. Fixes cross-compilation TARGET env var for aarch64-apple-darwin and aarch64-unknown-linux-gnu.

## [0.1.0-1.rc.0] - 2026-04-28

Initial release rehearsal (partial — 2 of 4 targets succeeded; aarch64 targets failed due to missing TARGET env var).
