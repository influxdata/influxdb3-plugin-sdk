## Summary

<!-- One-sentence description of what this PR does and why. -->

## Changes

<!-- Briefly list the user-facing or maintainer-facing changes. -->

-

## Testing

<!-- List checks run locally, or explain why a check was not run. -->

-

## Reviewer Notes

<!-- Call out any public API, schema, release, dependency, or migration concerns. -->

-

## Checklist

### Contributor

- [ ] I have read `CONTRIBUTING.md`
- [ ] I have signed the InfluxData [CLA](https://www.influxdata.com/legal/cla/) if this PR includes code or documentation changes from outside InfluxData

### Required checks

- [ ] `cargo build --workspace --locked` passes
- [ ] `cargo nextest run --workspace --no-fail-fast --locked` passes (with `INSTA_UPDATE=no`)
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings` is clean
- [ ] `cargo fmt --all -- --check` is clean
- [ ] `cargo deny check all` passes
- [ ] `cargo doc --workspace --no-deps --locked` builds without warnings (`RUSTDOCFLAGS="-D warnings"`)
- [ ] `cargo-semver-checks` passes for `cli` + `schemas` compared against the latest tag
- [ ] `cargo package --list` succeeds for all 3 crates
- [ ] Manifest invariants pass (`scripts/check-manifest-invariants.sh`)
- [ ] Changelog format passes (`scripts/check-changelog-format.sh`)
- [ ] Changelog update gate passes (`scripts/check-changelog-updated.sh`)

### Required when applicable

- [ ] If this PR changes `influxdb3-plugin-cli/src/` or `influxdb3-plugin-schemas/src/`: added a one-line entry under `## [Unreleased]` in `CHANGELOG.md`
- [ ] If CLI output changed: updated insta snapshots
- [ ] If schema types changed: updated fixtures under `influxdb3-plugin-schemas/tests/fixtures/`
- [ ] If manifest or index schema behavior changed: kept `docs/src/reference/manifest.md`, `docs/src/reference/registry-index.md`, and `docs/internal/spec.md` in sync, or explained why no docs update was needed
- [ ] If this is a breaking change that cascades (see `CONTRIBUTING.md`): called it out in the PR description
- [ ] If this PR adds a dependency: confirmed `deny.toml` allows its license and the dependency is not banned
- [ ] If release process or CI checks changed: updated `RELEASE.md`, `.github/RELEASE_CHECKLIST.md`, scripts, and this template as needed
- [ ] If `.circleci/config.yml` changed: validated locally with `circleci config validate`
- [ ] If `justfile` changed: tested the affected recipe locally
- [ ] If docs/process changed: kept `README.md`, crate READMEs, `CONTRIBUTING.md`, `RELEASE.md`, and `AGENTS.md` in sync as applicable
