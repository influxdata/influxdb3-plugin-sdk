# CI/CD Implementation Lessons Learned

Everything discovered during the CI/CD rollout for `influxdata/influxdb3-plugin-sdk` that wasn't in the original plan. Future maintainers: read this before modifying `.circleci/config.yml` or the release pipeline.

## Self-hosted runner gotchas

**Working directory namespacing is mandatory.** The self-hosted runners (`runner-ns/clustered-linux-vm{,-arm}`) are non-ephemeral — multiple pipelines can land on the same host. The `working_directory: /tmp/workspace/<< pipeline.id >>` in the executor config prevents collisions in the build tree, but other shared state is NOT namespaced:

- **`/tmp`** is shared. Any path in `/tmp` (e.g., for temporary clones) must include `$CIRCLE_PIPELINE_ID` or `<< pipeline.id >>` to avoid collisions. We hit this with `cargo-semver-checks` baseline clones.
- **`~/.config/gcloud/`** is user-level. `gcloud auth login` modifies the global gcloud config. Concurrent jobs switching active accounts race on this file. Fix: `export CLOUDSDK_CONFIG=/tmp/workspace/<< pipeline.id >>/gcloud` to isolate per pipeline.
- **`~/.docker/config.json`** is pre-configured with credential helpers (`"credHelpers": {"us-east1-docker.pkg.dev": "gcloud"}`). Docker ignores `docker login` tokens and uses the gcloud credential helper instead. This is fine as long as gcloud is authed correctly; it's confusing if you expect `docker login` to be authoritative.
- **Multiple service accounts are pre-authenticated** on the runners (e.g., `ci-support-ro@influxdata-team-edge...`, `circleci-k8s-idpe@...`). These are from other projects sharing the same runner fleet. Don't depend on them.
- **`git fetch --unshallow` doesn't reliably restore all objects** on self-hosted runners. The checkout may use a mechanism that `--unshallow` can't undo. Solution for tools needing full history: clone the specific tag/ref into a fresh directory outside the workspace.

## CircleCI config syntax

- **`<<` is reserved** in config v2.1 for parameter expansion. Shell heredocs (`<<EOF`) must be escaped as `\<<EOF`.
- **`run.environment` values are literal strings** — no shell expansion. Setting `environment: GIT_HASH: "$CIRCLE_SHA1"` bakes the literal string `$CIRCLE_SHA1` into the env, not its value. Export inside the `command:` body instead: `export GIT_HASH="$CIRCLE_SHA1"`.
- **Status check names** reported to GitHub are `ci/circleci: <job-name>` (with the `ci/circleci: ` prefix and a space after the colon). Branch protection `contexts` must use this exact format.
- **CircleCI has no `workflow_dispatch`** (that's GitHub Actions). The equivalent is "Trigger Pipeline" in the CircleCI UI or the pipeline-trigger API endpoint.
- **Context env vars require per-job declaration** in the workflow YAML. Project-level context scoping (in the CircleCI UI) only controls which projects CAN use a context; the actual injection requires `context: [name]` on the job invocation. Without it, env vars are silently absent.
- **Workspace persist/attach** is the mechanism for fan-in across matrix jobs. Each `build-release` matrix entry persists its tarball to `artifacts/influxdb3-plugin-<target>.tar.gz` (per-target filename prevents collisions), then downstream jobs attach and read all artifacts.

## GCP Workload Identity Federation (WIF)

- **Use V1 OIDC token, not V2.** CircleCI's V1 token (`$CIRCLE_OIDC_TOKEN`) has a `sub` claim of exactly 127 bytes (the GCP limit for `google.subject` mapped attributes). V2 (`$CIRCLE_OIDC_TOKEN_V2`) exceeds it and fails at the STS token-exchange step.
- **`allowed-audiences` must be set explicitly** on the WIF provider. CircleCI's OIDC token has `aud` = the org UUID (`c699aced-...`). GCP's default audience expectation doesn't match this; you must `gcloud iam workload-identity-pools providers update-oidc --allowed-audiences="<org-uuid>"`.
- **GAR IAM bindings are at the REPOSITORY level**, not the image level. The image URL `us-east1-docker.pkg.dev/<project>/<repo>/<image>` has three levels; IAM bindings go on `<repo>` (e.g., `ci-support`), not `<image>` (e.g., `ci-cross-influxdb3`). We lost significant time because we initially passed the image name to `gcloud artifacts repositories add-iam-policy-binding` instead of the repo name.
- **`testIamPermissions` requires admin perms.** Having `roles/artifactregistry.reader` on a repo lets you pull images, but does NOT let you call `testIamPermissions` on that repo. The API returns 403 even though your read access works fine. Don't use `testIamPermissions` to verify reader grants.
- **CircleCI context `store-secret`** reads the secret value from stdin: `echo -n "value" | circleci context store-secret github <org> <context> <var>`.

## Cross-compilation with the org's cross-builder image

- **`$TARGET` env var is essential.** The `ci-cross-influxdb3` image's `target-env` wrapper script reads `$TARGET` to decide which cross-compilers to configure (CC, AR, CARGO_TARGET_*_LINKER). Without it, `target-env` is a no-op and cargo uses the host compiler, producing errors like `cc: error: unrecognized command-line option '-arch'` (darwin) or `rust-lld: error: ... is incompatible with elf64-x86-64` (aarch64 linux).
- **`RUSTC_WRAPPER=""`** must be set for release builds (disables sccache). Match `influxdb_pro`'s pattern.
- **`rcodesign sign`** (ad-hoc, no certificate) prevents macOS "this binary is damaged" errors but does NOT bypass Gatekeeper's quarantine popup for downloaded binaries. Users downloading from GitHub Releases still need `xattr -d com.apple.quarantine <binary>`. Full bypass requires Apple Developer ID signing + notarization ($99/year).
- **The cross-builder image is referenced by SHA256 digest** (`@sha256:98b0553...`), not by tag. This pins the exact image version. To update, find the new digest in GAR and update the SHA in `.circleci/config.yml`.

## Cargo / Rust

- **`cargo package --no-verify` still does dependency resolution.** It skips the "extract and build the tarball" step but still runs "prepare local package for uploading" which resolves path-deps from crates.io. Crates with unpublished path-deps (sdk, cli) fail at this step. Only `cargo package --list` (no resolution) works pre-Group-H for all crates; `cargo package --no-verify` works only for the leaf crate (schemas).
- **`cargo-deny` version matters.** v0.16.x can't parse CVSS 4.0 advisory records (e.g., `RUSTSEC-2026-0035`). Upgrade to 0.19.x+.
- **`cargo fmt` wasn't enforced before CI.** The first CI run surfaced 28 files of formatting drift. Bundle the cleanup with the PR that adds the `cargo-fmt` check (same precedent as adding a linter to any project).
- **`cargo-semver-checks` needs full git history** for `--baseline-rev`. Shallow clones (CircleCI's default) are missing tree/blob objects from the baseline commit. Solution: clone the baseline tag into a fresh directory with `git clone --depth=1 --branch <tag> <origin> <dir>` and use `--baseline-root <dir>`. The directory must be OUTSIDE the workspace tree (otherwise cargo finds duplicate manifests).

## Shell scripting under `set -euo pipefail`

- **`strings <binary> | grep -qF <pattern>` = SIGPIPE.** `grep -q` exits immediately on match, closing stdin. `strings` gets SIGPIPE (exit 141). `pipefail` propagates 141. The script thinks "not found" even though the match exists. Fix: use `grep -cF <pattern> > /dev/null` (counts matches; reads all input; no early close).
- **`${#!v}` (indirect variable length) fails in some bash versions.** Use a temp variable: `val="${!v:-}"; echo "${#val}"`.
- **`\d` is not POSIX ERE.** `grep -E '^\d+:'` silently matches nothing on standard grep. Use `[0-9]+`.
- **`grep -rn 'version.workspace = true'` matches `rust-version.workspace = true`.** Anchor with `^`: `grep -rEn '^version\.workspace = true'`.
- **Multi-line strings in justfiles** must have every continuation line indented at the recipe body's indentation level. Unindented lines are treated as top-level justfile content (syntax error). Use `printf '%s\n' ...` for multi-value strings instead of heredocs/multi-line assignments.
- **`errors=$((errors+1))` is safe with `set -e`** — arithmetic expansion that evaluates to non-zero is treated as success. Do NOT "fix" this to `((errors++))` which DOES trigger `set -e` when the pre-increment value is 0.

## Process / workflow

- **Probe PRs are invaluable.** Branch-filtered workflows (`when: equal: [<branch>, << pipeline.git.branch >>]`) let you test auth flows, image pulls, and env-var availability without committing to the production config. We used 3 probe iterations to discover and fix WIF issues before PR-7.
- **The "wrong resource name" bug was the biggest time sink.** We spent multiple SRE interactions because `gcloud artifacts repositories add-iam-policy-binding ci-cross-influxdb3` targets the image name, not the repository name (`ci-support`). The GAR URL format `<host>/<project>/<repo>/<image>` makes this non-obvious. Always parse the URL before writing the IAM command.
- **Provide SRE with exact `gcloud` commands** instead of descriptions. Copy-pasteable commands get executed correctly; descriptions get misinterpreted. Include the full `--member`, `--role`, `--project`, `--location` flags.
- **Non-ephemeral runners accumulate state** in `~/.docker/config.json`, `~/.config/gcloud/`, `/tmp/`, and any user-level config. Every path that a job writes to must be pipeline-namespaced or cleaned up at the start of the step.
- **CircleCI squash-merges may not include all commits** if you merge from the GitHub UI while additional commits are still being pushed to the PR branch. Verify what's on main after merge; don't assume the latest push was included.
