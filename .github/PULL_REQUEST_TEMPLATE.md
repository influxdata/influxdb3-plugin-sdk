## Summary

<!-- One-sentence description of what this PR does and why. -->

## Checklist

### Required (CI gates these)

- [ ] `cargo build --workspace` passes
- [ ] `cargo nextest run --workspace --no-fail-fast` passes (with `INSTA_UPDATE=no`)
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` is clean
- [ ] `cargo fmt --all -- --check` is clean
- [ ] `cargo deny check all` passes
- [ ] `cargo doc --workspace --no-deps` builds without warnings (`RUSTDOCFLAGS="-D warnings"`)
- [ ] `cargo-semver-checks` passes for `cli` + `schemas` (compared against latest tag)
- [ ] `cargo package --list` succeeds for all 3 crates
- [ ] Manifest invariants pass (`scripts/check-manifest-invariants.sh`)

### Manual (not CI-gated; reviewer checks)

- [ ] If this PR changes the public API of `cli` or `schemas`: added a one-line entry under `## [Unreleased]` in `CHANGELOG.md`
- [ ] If this PR is a breaking change that cascades (see `CONTRIBUTING.md`): called out in this PR description
- [ ] If this PR adds a new dependency: confirmed `deny.toml` allows its license
- [ ] If this PR modifies `.circleci/config.yml`: validated locally with `circleci config validate`
- [ ] If this PR modifies `justfile`: tested the affected recipe locally
