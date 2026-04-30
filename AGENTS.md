# Agent Instructions for influxdb3-plugin-sdk

This document is the source of truth for AI agents working on this repo. All agents (Claude, Copilot, Codex, etc.) must follow these instructions.

## Before making any change

Read and understand:
- `CONTRIBUTING.md` — versioning model, cascade rules, stability tiers
- `RELEASE.md` — release procedure (do NOT cut releases without explicit user authorization)
- `.github/PULL_REQUEST_TEMPLATE.md` — every PR must satisfy this checklist

## Change checklist

When modifying this repo, verify every applicable item before pushing.

### Content changes

- [ ] **CHANGELOG.md** updated under `## [Unreleased]` if the change touches `influxdb3-plugin-cli/src/` or `influxdb3-plugin-schemas/src/` (CI enforces this — the build WILL fail if you skip it)
- [ ] **CHANGELOG.md** format follows [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/) (CI enforces: `# Changelog` title, `## [Unreleased]` section, version sections as `## [X.Y.Z] - YYYY-MM-DD`, subsections only `### Added/Changed/Deprecated/Removed/Fixed/Security`)
- [ ] **Snapshot tests** updated if CLI output changes (`cargo insta review` — ~20+ `.snap` files under `tests/snapshots/`)
- [ ] **Test fixtures** updated if schema types change (`influxdb3-plugin-schemas/tests/fixtures/`)
- [ ] **Scripts and justfile** updated if release process or CI checks change

### Documentation (keep in sync with code)

- [ ] `README.md` (top-level project overview, dev commands)
- [ ] Per-crate READMEs (`influxdb3-plugin-cli/README.md`, `influxdb3-plugin-sdk/README.md`, `influxdb3-plugin-schemas/README.md`)
- [ ] `RELEASE.md` (must match actual justfile recipes + CI workflow behavior)
- [ ] `CONTRIBUTING.md` (bump rules, cascade graph, stability tiers)
- [ ] `AGENTS.md` (this file — update if process changes)

### Dependency / manifest changes

- [ ] **Version cascade**: if bumping a crate version, follow the cascade per `CONTRIBUTING.md`. Use `just cut-version <crate> X.Y.Z --cascade` for breaking bumps. The cascade is: `schemas` → `sdk` + `cli`; `sdk` → `cli`; `cli` has no consumers.
- [ ] **Workspace path-dep constraints**: root `Cargo.toml`'s `[workspace.dependencies]` version constraints must match member crate versions. Cargo enforces this at build time; CI's `manifest-invariants` job double-checks.
- [ ] **deny.toml**: if adding a new dependency, verify its license is in the allowlist and it's not in the ban list. Run `cargo deny check all` locally.
- [ ] **clippy.toml**: if adding methods that touch global state (process exit, tracing subscribers, signal handlers, panic hooks), check the `disallowed-methods` policy. The SDK library surface must not install global state.

### CI / infrastructure changes

- [ ] **`.github/PULL_REQUEST_TEMPLATE.md`**: if CI checks change, keep the template's checklist in sync
- [ ] **`.github/RELEASE_CHECKLIST.md`**: if release process changes

### Pre-push checks (run all of these locally before pushing)

```bash
cargo build --workspace --locked
cargo nextest run --workspace --no-fail-fast --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
cargo deny check all
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo semver-checks -p influxdb3-plugin-cli --baseline-rev "$(git describe --tags --abbrev=0)"
cargo semver-checks -p influxdb3-plugin-schemas --baseline-rev "$(git describe --tags --abbrev=0)"
cargo package --list -p influxdb3-plugin-schemas --locked
cargo package --list -p influxdb3-plugin-sdk --locked
cargo package --list -p influxdb3-plugin-cli --locked
./scripts/check-manifest-invariants.sh
./scripts/check-changelog-format.sh
```

### Things NOT to do

- **Do NOT set `publish = true`** on any crate. This is gated on security + legal review (Group H). The crates are `publish = false` by design.
- **Do NOT modify the tag format** (`vX.Y.Z`) without updating the justfile, `.circleci/config.yml` tag filter, `RELEASE.md`, and `.github/RELEASE_CHECKLIST.md` in lockstep.
- **Do NOT cut a release** (run `just tag-version`, push a `v*` tag) without explicit user authorization. Releases trigger the full build + publish pipeline.
