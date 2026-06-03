# Release Checklist

Copy this into the release-prep PR description and check off each item as you go. See `RELEASE.md` for the full procedure with commands.

## Pre-release

- [ ] Decide the version (`X.Y.Z`) and which crates need bumping
- [ ] Confirm you're on `main` with a clean tree: `git checkout main && git pull --ff-only origin main && git status`
- [ ] Confirm `just` and `gh` CLI are installed and authenticated

## Version bump

- [ ] Create feature branch: `git checkout -b chore/release-X.Y.Z`
- [ ] Bump version(s):
  - [ ] Single crate: `just cut-version <crate> X.Y.Z` (with `--cascade` if breaking)
  - [ ] All crates: `just cut-version-all X.Y.Z` (for unified releases)
- [ ] Verify `cargo check --workspace` passes after the bump
- [ ] Update `CHANGELOG.md`: move `[Unreleased]` entries into `## [X.Y.Z] - YYYY-MM-DD`
- [ ] Leave `## [Unreleased]` section empty for the next cycle

## PR + merge

- [ ] Commit: `git commit -am "chore: release X.Y.Z"`
- [ ] Push + open PR: `git push -u origin HEAD && gh pr create --title "chore: release X.Y.Z"`
- [ ] All 9 CI checks pass
- [ ] PR reviewed and squash-merged

## Tag + publish GitHub Release

- [ ] Pull the squash-merge commit: `git checkout main && git pull --ff-only origin main`
- [ ] Create tag: `just tag-version X.Y.Z` (validates clean tree + HEAD == origin/main + version match)
- [ ] Push tag: `git push origin vX.Y.Z`
- [ ] Watch CircleCI release workflow at `https://app.circleci.com/pipelines/github/influxdata/influxdb3-plugin-sdk?branch=vX.Y.Z`

## Verify

- [ ] All release pipeline jobs pass (build-release × 4, generate-checksums, verify-release-binaries, publish-github-release)
- [ ] `just verify-version X.Y.Z` reports all 5 assets present
- [ ] GitHub Release page exists at `https://github.com/influxdata/influxdb3-plugin-sdk/releases/tag/vX.Y.Z`
- [ ] Release is marked as prerelease (if RC) or latest (if stable)
- [ ] **Stable releases only:** `latest` release recreated at the new release's commit: `git fetch --tags --force origin && git rev-parse latest^{commit}` matches `git rev-parse vX.Y.Z^{commit}`
- [ ] **Stable releases only:** `latest` release carries all 5 assets: `gh release view latest --repo influxdata/influxdb3-plugin-sdk --json assets --jq '.assets | length'` reports `5`
- [ ] Download + run the binary for your platform:
  ```bash
  gh release download vX.Y.Z --repo influxdata/influxdb3-plugin-sdk --pattern '*aarch64-apple-darwin*' --dir /tmp
  tar -xzf /tmp/influxdb3-plugin-aarch64-apple-darwin.tar.gz -C /tmp
  xattr -d com.apple.quarantine /tmp/influxdb3-plugin  # macOS only
  /tmp/influxdb3-plugin --version
  ```
- [ ] `--version` output shows `influxdb3-plugin X.Y.Z, revision <40-hex-sha>`
- [ ] Revision SHA matches the tagged commit: `git rev-parse vX.Y.Z^{commit}`

## If something goes wrong

- [ ] **Build fails:** fix in a follow-up PR, delete the tag (`git push origin --delete vX.Y.Z && git tag -d vX.Y.Z`), re-tag after merge
- [ ] **Verify fails but builds succeeded:** check `docs/ci-cd-lessons-learned.md` for known gotchas; likely a script bug, not a binary bug
- [ ] **GitHub Release publish fails (PAT scope, gh CLI):** check the `influxdb3-plugin-sdk-github` CircleCI context has a valid `GH_TOKEN`
- [ ] **`latest` release stale, missing, or has no assets after a stable release:** the `publish-github-release` job's "Update floating 'latest' release" step failed. See `RELEASE.md` "What to do if things go wrong" for manual recovery.
