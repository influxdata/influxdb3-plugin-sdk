# Releasing influxdb3-plugin-sdk

This document is the canonical release runbook for `influxdb3-plugin-sdk`. It mirrors `influxdata/influxdb_pro/RELEASE.md`'s structure, adapted for the SDK's per-crate versioning model and CircleCI release pipeline.

## Prerequisites

- Local clone of the repo at `main`, working tree clean.
- `just` installed (`cargo install just --locked` or download from <https://github.com/casey/just/releases>).
- `gh` CLI authenticated to github.com with repo access to `influxdata/influxdb3-plugin-sdk`.
- Push permission on `main` (you'll be opening a PR, but you also need to push tags).
- The CircleCI release pipeline is wired and these contexts exist (one-time setup; see `.circleci/config.yml`):
  - `influxdb3-plugin-sdk-github` with a `GH_TOKEN` PAT (GitHub Release + floating `latest`).
  - `influxdb3-plugin-sdk-cratesio` with a scoped `CARGO_REGISTRY_TOKEN` (crates.io publish; scope restricted to the three crate names).

## Versioning model

The SDK workspace has three crates, each with its own version:

- `influxdb3-plugin-schemas` (semver-stable, library)
- `influxdb3-plugin-sdk` (semver-stable, library)
- `influxdb3-plugin-cli` (semver-stable, binary — what gets released)

The `vX.Y.Z` git tag is **always anchored to cli's version**. cli is the user-facing binary release; the tag matches its version. The library crates may be at different versions internally — that's fine and is the deliberate consequence of the per-crate versioning model documented in `CONTRIBUTING.md`.

Crates.io publication follows the workspace dependency order. Release automation must publish the actual versions in each crate's `Cargo.toml`; do not assume the three crate versions are identical:

1. `influxdb3-plugin-schemas`
2. `influxdb3-plugin-sdk`
3. `influxdb3-plugin-cli`

Crates.io versions are immutable. If a publish succeeds and a later release step fails, you must recover with a new version; yanking prevents new dependency resolution but does not delete the uploaded crate.

Crates.io publishing is automated. On a **stable** `vX.Y.Z` tag, the CircleCI `publish-crates-io` job runs after the versioned `publish-github-release` job (independently of the floating-`latest` update, so a `latest` failure cannot block it) and publishes any crate whose `Cargo.toml` version is not yet on crates.io, in the dependency order above (`scripts/publish-crates-io.sh`). It is idempotent: re-running a release publishes nothing new, and a cli-only bump publishes only cli. RC tags (`vX.Y.Z-N.(alpha|beta|rc).N`) do **not** publish to crates.io — they ship GitHub Release binaries only. Do not run `cargo publish` by hand except for the documented recovery below. To preview what a publish would do for the current tree without touching crates.io, run `just publish-crates-io-dry-run` — it validates the index diff/skip logic only, not packaging or the verify recompile.

In addition to the per-release `vX.Y.Z` tag, the repo carries a single floating `latest` release. The CircleCI release pipeline recreates `latest` at the commit of each newly-published **stable** release, attaching the same assets as that `vX.Y.Z` release. Prereleases (`vX.Y.Z-N.(alpha|beta|rc).N`) do **not** update `latest`. The `latest` release is marked `--latest=false` so the `vX.Y.Z` release keeps GitHub's "Latest" badge.

Users who want to track the most recent stable release without pinning a version can build from source:

```bash
cargo install --git https://github.com/influxdata/influxdb3-plugin-sdk --tag latest influxdb3-plugin-cli --force
```

…or download the prebuilt binaries:

```bash
gh release download latest --repo influxdata/influxdb3-plugin-sdk
```

`latest` is a moving ref and is excluded from the release workflow's tag-filter regex, so recreating it does not re-trigger the pipeline.

## Standard release procedure

1. **Prepare the version bump locally** (on a feature branch off `main`):

   For a cli-only bump (most common):

   ```bash
   git checkout main && git pull --ff-only origin main
   git checkout -b chore/cli-X.Y.Z/version-bump
   just cut-version cli X.Y.Z
   # cli has no consumers in the workspace, so no cascade needed.
   ```

   For a schemas or sdk bump that should propagate to cli:

   ```bash
   just cut-version schemas X.Y.Z --cascade
   # equivalent to: cut-version schemas + cut-version sdk + cut-version cli
   ```

   For a unified release where all 3 crates align (rehearsal RCs, first stable release):

   ```bash
   just cut-version-all X.Y.Z
   ```

2. **Update CHANGELOG.md**: move entries from the `## [Unreleased]` section into a new `## [X.Y.Z] - YYYY-MM-DD` section. Leave `## [Unreleased]` empty for the next release cycle.

3. **Commit and push the bump**:

   ```bash
   git commit -am "chore(cli): bump to X.Y.Z"   # adjust crate name in scope
   git push -u origin HEAD
   gh pr create --title "chore(cli): bump to X.Y.Z" --body "Release prep for vX.Y.Z."
   ```

4. **Wait for CI to pass on the PR**, then merge via the GitHub UI (squash merge is fine).

5. **Tag the release** — CRITICAL: pull main first to pick up the squash-merge SHA:

   ```bash
   git checkout main
   git pull --ff-only origin main   # CRITICAL: squash-merge produces a different SHA than the feature branch HEAD
   just tag-version X.Y.Z
   git push origin vX.Y.Z
   ```

6. **Watch the CircleCI release workflow** at <https://app.circleci.com/pipelines/github/influxdata/influxdb3-plugin-sdk?branch=vX.Y.Z>. Expected runtime: ~30 minutes (4 cross-compile builds + checksums + verification + upload).

7. **Verify the published GitHub Release**:

   ```bash
   just verify-version X.Y.Z
   ```

   After a stable release, also confirm the floating `latest` release was recreated at the same commit and carries the assets:

   ```bash
   git fetch --tags --force origin
   test "$(git rev-parse latest^{commit})" = "$(git rev-parse vX.Y.Z^{commit})" \
     && echo "latest -> vX.Y.Z OK" \
     || echo "MISMATCH: latest does not point at vX.Y.Z"
   gh release view latest --repo influxdata/influxdb3-plugin-sdk --json assets \
     --jq '.assets | length'   # expect 5
   ```

   The CircleCI `update-latest-release` job recreates `latest` automatically; this check verifies it succeeded.

   Then manually: download the `x86_64-unknown-linux-gnu` tarball from the release page, extract, run `./influxdb3-plugin --version`, confirm the revision SHA matches the commit `vX.Y.Z` points at.

8. **Verify the crates.io publish** (stable releases only):

   ```bash
   just verify-crates-io
   ```

   Confirms each crate's current version is live on crates.io. Then spot-check a clean install:

   ```bash
   cargo install influxdb3-plugin-cli --version X.Y.Z --locked --root /tmp/i3pc-verify --force
   /tmp/i3pc-verify/bin/influxdb3-plugin --version   # expect X.Y.Z + the tagged commit SHA
   ```

## Pre-release (RC) procedure

RC tags use the format `vX.Y.Z-N.(alpha|beta|rc).N` (matching `influxdb_pro`'s convention). Example: `v0.1.0-1.rc.0`.

The procedure is identical to the standard release with one difference: in step 1, use `just cut-version-all 0.1.0-1.rc.0` (lockstep bump) since RCs are typically unified across all 3 crates. The CircleCI release workflow auto-detects the RC suffix (`-` in the tag) and passes `--prerelease` to `gh release create`, so the GitHub release is marked as a prerelease.

## What to do if things go wrong

- **CircleCI release fails mid-build**: the tag exists but the release is incomplete. Delete the tag (`git push --delete origin vX.Y.Z` and `git tag -d vX.Y.Z`), fix the issue in a follow-up PR, re-cut the version, and re-tag.
- **`just tag-version` refuses with "HEAD != origin/main"**: you forgot to pull main after the squash merge. Run `git checkout main && git pull --ff-only origin main` and retry.
- **`just tag-version` refuses with "Cargo.toml version mismatch"**: the merged commit doesn't have the version bump you expected. Investigate before re-tagging — likely the version-bump PR was merged with stale Cargo.toml content.
- **Anything else unexpected**: stop, capture the output, surface to the team. Don't improvise tag manipulation.
- **`latest` release missing, stale, or has no assets**: the `update-latest-release` job failed (most commonly, the `influxdb3-plugin-sdk-github` context lacks a valid `GH_TOKEN`, or the token lacks `contents:write` on the repo). It is a sibling of `publish-crates-io`, so the versioned release and the crates.io publish are unaffected. Inspect the failed job's logs. To recover manually after fixing the underlying issue, download the `vX.Y.Z` assets and recreate `latest` from them:

  ```bash
  rm -rf /tmp/latest-assets && mkdir -p /tmp/latest-assets
  gh release download vX.Y.Z --repo influxdata/influxdb3-plugin-sdk --dir /tmp/latest-assets
  gh release delete latest --repo influxdata/influxdb3-plugin-sdk --yes --cleanup-tag || true
  git push --force origin :refs/tags/latest || true
  gh release create latest --repo influxdata/influxdb3-plugin-sdk \
    --target "$(git rev-parse vX.Y.Z^{commit})" --title latest --latest=false \
    --notes "Floating pointer to the newest stable release (vX.Y.Z)." \
    /tmp/latest-assets/*
  ```

  Do not improvise tag manipulation on `vX.Y.Z` tags — `latest` is the only release operators may recreate manually.
- **crates.io publish failed or was partial:** crates.io versions are immutable — never reuse a version. To recover, use CircleCI's **Rerun workflow from failed** — *not* a fresh `vX.Y.Z` tag push (the tag already exists and re-pushing it does nothing). Rerun-from-failed repeats only the failed jobs, and both are safe to repeat: `publish-github-release` is idempotent (it re-uploads assets when the release already exists instead of erroring), and `publish-crates-io` skips already-published crates and retries only the missing ones. crates.io publishing requires only the versioned `publish-github-release` job (not the floating-`latest` update), so a `latest` failure never blocks it. Alternatively, recover by hand with the token in your environment: `just publish-crates-io`. If a *bad* version was published, `cargo yank <crate>@X.Y.Z` (stops new resolution) and ship a corrected new version. Check the `influxdb3-plugin-sdk-cratesio` context has a valid `CARGO_REGISTRY_TOKEN` if the job failed on auth. If the verify build fails resolving a just-published sibling under `--locked` (stale toolchain below cargo 1.66, or an index-propagation race), drop `--locked` from the `cargo publish` step in `scripts/publish-crates-io.sh`.

## Post-release follow-ups

After a stable (non-RC) release ships:

- Optional: bump `main`'s `Cargo.toml`s to the next development version. This is a manual edit (no recipe enforces it).

## Related docs

- Bump rules + cascade: `CONTRIBUTING.md`
