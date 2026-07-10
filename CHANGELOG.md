# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). The version anchor for a tagged release is the `influxdb3-plugin-cli` crate; library crates (`influxdb3-plugin-schemas`, `influxdb3-plugin-sdk`) may have different versions per the per-crate versioning model documented in `CONTRIBUTING.md`.

## [Unreleased]

## [0.6.0] - 2026-07-10

### Added
- Inter-plugin dependencies: the manifest and index `dependencies` object gains an optional `plugins` field — an array of fully-resolved references `{index_url, name, version}` where `version` is a SemVer range. Manifest schema bumps to 1.3, index schema to 2.1 (both additive).
- `influxdb3-plugin-schemas`: new `IndexUrl` and `PluginDependency` types, `Dependencies.plugins` field, and error variants `UnsupportedIndexUrlScheme`, `InvalidPluginDependencyVersion`, and `DuplicatePluginDependency`. Both parsers validate entries per-field with path-aware errors and reject duplicates by `(index_url, canonical name)`.
- `influxdb3-plugin-cli`: `info` JSON output gains `dependencies.plugins`; human output gains a `plugins:` line.

### Changed
- `influxdb3-plugin-schemas`: `Index::to_canonical_json` now always stamps the current `index_schema_version` — legacy `2.0` indexes upgrade implicitly on write. Empty `dependencies.plugins` is omitted from serialized output, so pre-existing entries keep their serialized form unchanged.
- `influxdb3-plugin-schemas` (breaking): `PluginId::Registry.index_url` and `PluginId::registry()` now use the `IndexUrl` newtype instead of raw `url::Url`, so plugin identities and `dependencies.plugins` references share one validation and normalization rule. The `IndexUrl` scheme set (`https`, `http`, `file`) can widen later without breaking.

## [0.5.1] - 2026-06-04

### Added
- Automated, idempotent crates.io publishing on stable releases (`publish-crates-io` CircleCI job + `scripts/publish-crates-io.sh`). Decoupled from the floating-`latest` update (now its own `update-latest-release` job) so a `latest` failure never blocks it, and safe to re-run via CircleCI "Rerun from failed".

### Changed
- Patch bump of all three crates (`influxdb3-plugin-schemas` 0.3.1, `influxdb3-plugin-sdk` 0.4.1, `influxdb3-plugin-cli` 0.5.1) — the first stable release published to crates.io by the automated pipeline. No functional or API changes.

## [0.5.1-1.rc.0] - 2026-06-04

### Changed
- Release-pipeline rehearsal RC for the automated crates.io publishing release. Exercises the GitHub Release path end to end — the idempotent `publish-github-release` job and the split `update-latest-release` job — without publishing to crates.io (RC tags are excluded from `publish-crates-io`). No code changes since 0.5.0.

## [0.5.0] - 2026-06-02

### Added
- `influxdb3-plugin-schemas`: added the pure plugin-directory validation contract (`validate` module) used by the SDK validator — entry-point classification, trigger binding, the `TopLevelFunctionDef` extraction rules, the `ValidatedPluginDefinition` success payload, and an executable conformance corpus. The `ValidationError` diagnostic type now lives here.
- New `docs/src/reference/plugin-format.md` documenting the plugin-directory layout contract.
- Optional `[plugin].exclude` manifest field: an array of gitignore-style patterns, relative to the plugin root, that omits files from packaging and validation. Patterns are compiled by the SDK (via the `ignore` crate); the same selection feeds both `package` and `validate`. `manifest_schema_version` bumps to `1.2` (additive minor; existing `1.x` manifests remain valid). Generated plugin templates now ship a recommended `exclude` list. Documented in `docs/src/reference/manifest.md`.

### Changed
- `influxdb3-plugin-sdk`: validation now returns structured success metadata (`ValidatedPluginDefinition`) and a focused `ValidationFailure` error, while preserving CLI diagnostics and JSON codes. The `tree-sitter` extractor is exposed as `validate::extract_top_level_defs`. `ValidationError` is re-imported from `influxdb3-plugin-schemas` and no longer re-exported by the SDK.
- Source-file selection is now driven solely by `[plugin].exclude`. The previously hard-coded packaging excludes (`target/`, `.git/`, `__pycache__/`, `*.pyc`) are removed — a plugin with no `exclude` packages every regular file. Those defaults now live in generated template manifests as editable recommendations.
- `influxdb3-plugin` `validate`/`package`: validation now parses `manifest.toml` before detecting the entry point. A missing or malformed manifest is reported on its own; entry-point detection runs only against the manifest-selected (post-`exclude`) top-level files, so excluded `.py` files no longer count toward entry-point ambiguity. Invalid `exclude` patterns surface as the `validate::invalid_exclude_pattern` / `package::invalid_exclude_pattern` error codes, naming the offending pattern.

## [0.4.2] - 2026-05-28

### Changed
- **CI**: the sccache compilation cache is now persisted on a stable path across runs (bounded by `SCCACHE_CACHE_SIZE` LRU eviction) instead of being discarded with each job's workspace, warming `rustc` compiles and reducing the cold-build memory spike behind intermittent self-hosted-runner timeouts. No functional changes to the `influxdb3-plugin` CLI or libraries.

## [0.4.1] - 2026-05-28

### Changed
- **Release pipeline**: the floating `latest` ref is now published as a full GitHub Release carrying the same assets as the stable `vX.Y.Z` release it tracks, instead of a bare git tag. Users can download prebuilt binaries with `gh release download latest --repo influxdata/influxdb3-plugin-sdk` in addition to building from source with `cargo install --tag latest`. The `latest` release is marked `--latest=false` so the `vX.Y.Z` release keeps GitHub's "Latest" badge.
- `influxdb3-plugin-schemas`: expanded `Index` and `ArtifactHash` test coverage to pin documented invariants (hex-only hash zone, malformed-JSON short-circuit at root, per-entry `InvalidVersion` rejection, `git+ssh://` artifact URL rejection). No behavior change.

## [0.3.0] - 2026-05-04

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
