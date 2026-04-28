# Contributing to influxdb3-plugin-sdk

This document covers contribution conventions for the SDK workspace, with a focus on **versioning and the per-crate cascade**. For release operator instructions (`cut-version`, `tag-version`, etc.), see `RELEASE.md`.

## Workspace structure

The SDK workspace has three crates with distinct stability tiers:

- **`influxdb3-plugin-schemas`** — semver-stable, library. Public types consumed by `influxdb3-plugin-cli` (re-exported) and the future db runtime.
- **`influxdb3-plugin-sdk`** — internal, library. May change without notice. Consumers must go through `influxdb3-plugin-cli`'s public API, not directly through `sdk`.
- **`influxdb3-plugin-cli`** — semver-stable, binary. The user-facing release artifact; the `vX.Y.Z` git tag is anchored to this crate's version.

Each crate has its own `[package].version` (per the per-crate versioning model). The workspace `Cargo.toml`'s `[workspace.dependencies]` declares path-deps with explicit caret-versioned constraints.

## The dependency cascade

Internal dependency graph:

```
influxdb3-plugin-cli  ──┬──>  influxdb3-plugin-sdk    ──>  influxdb3-plugin-schemas
                        │
                        └──>  influxdb3-plugin-schemas (re-exports types via cli/src/lib.rs)
```

cli depends on both sdk and schemas directly (schemas via re-export of types in `cli/src/lib.rs`).

**Breaking changes propagate through this graph:**

- A breaking change in `schemas` cascades through **both** `sdk` (direct dep) **and** `cli` (direct dep + re-exports schemas types via `cli/src/lib.rs`).
- A breaking change in `sdk` cascades through `cli` (direct dep).
- A breaking change in `cli` has no internal consumers.

**Non-breaking changes** (additive: new public items, new fields with defaults, new Cargo features that don't change existing behavior) propagate at compile time without requiring constraint updates, because the workspace path-deps use caret semantics (`^0.x.y`).

**Cargo enforces the cascade at build time:** if you bump a crate without updating consumers, `cargo build --workspace` refuses to resolve. CI's `manifest-invariants` and `cargo-package-check` jobs additionally check the constraint shape (added in later PRs).

## Version bump rules (when to bump what)

| Change | Semver impact | Bump |
|---|---|---|
| Add a new public item, new optional Cargo feature, etc. | Minor (or patch in 0.x) | Patch in 0.x; minor in 1.x+ |
| Modify behavior of an existing public item (signature change, removed field, narrowed type) | Breaking | Minor in 0.x; major in 1.x+ |
| Bug fix with no public-API impact | Patch | Patch always |
| Schema type change in `schemas` | Breaking → cascades | Bump schemas + sdk + cli |
| Internal refactor in `sdk` with no `cli`-public-API impact | Non-breaking from cli's perspective | Patch in sdk; cli unchanged |
| `--version` output shape or SHA precedence change | Breaking (per S2-21) | Major in cli (post-1.0); free pre-1.0 |
| JSON output schema change in any cli command | Breaking (per S2-16) | Major in cli (post-1.0) |

For breaking bumps that cascade, use `just cut-version <crate> X.Y.Z --cascade` (see `RELEASE.md`).

## Stability tiers (when do they bind?)

The three SDK crates have distinct stability policies that bind at each crate's own `1.0.0` release. During `0.x`, Cargo's SemVer convention permits breaking changes at any minor bump; the policies below describe the contract that engages at `1.0.0` and after.

In practice, while every crate is at `0.x`:
- Breaking changes are allowed at any minor bump (per Cargo convention).
- The release manager still uses `cut-version` + `--cascade` to keep consumers consistent at build time.
- `cargo-semver-checks` (added in a later PR) runs against the latest tag baseline for `cli` and `schemas` to surface API breaks.

After a crate hits `1.0.0`:
- Breaking changes require a major bump (and `cargo-semver-checks` will refuse otherwise).
- `cli`'s embedding contract, the JSON output schema, and the `--version` output shape become hard contracts.

## PR checklist (informal — no CI gate)

For PRs that touch crate code:

- [ ] If your change is a public-API change in `cli` or `schemas`, add a one-line entry under `## [Unreleased]` in `CHANGELOG.md` describing the user-visible change.
- [ ] If your change is a breaking change requiring a cascade (per the cascade table above), call it out in the PR description so the next release manager remembers to bump consumers.
- [ ] If your change adds a new dependency, confirm `deny.toml` is satisfied (CI's `cargo-deny` will catch this anyway).
- [ ] Run `cargo test --workspace` locally before pushing.

There is **no automated changeset gate** — discipline is in the PR review.

## Related docs

- Release procedure: `RELEASE.md`
