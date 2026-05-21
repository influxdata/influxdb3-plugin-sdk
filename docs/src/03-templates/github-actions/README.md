# GitHub Actions Publish Workflow

This template publishes plugin artifacts and a registry `index.json` to a private GitHub Release.

The workflow is intended for plugin repository maintainers. It validates plugin directories, packages only versions that are not already present in the registry index, uploads new archives, and replaces `index.json` after the artifact uploads succeed.

## Template File

Copy [publish.yml](./publish.yml) into your plugin source repository at:

```text
.github/workflows/publish.yml
```

## What To Customize

Set these values at the top of the workflow:

| Variable | Description |
|---|---|
| `SDK_VERSION` | The published `influxdb3-plugin-cli` crate version to install. Use a fixed version in CI. |
| `PLUGIN_ROOT` | Directory containing one subdirectory per plugin. The template default is `plugins`. |
| `REGISTRY_REPO` | Repository that owns the registry release, for example `YOUR_ORG/YOUR_REGISTRY_REPO`. |
| `REGISTRY_TAG` | Release tag that stores `index.json` and plugin archive assets. |

At public go-live, the workflow installs the SDK CLI from crates.io:

```bash
cargo install influxdb3-plugin-cli --version "${SDK_VERSION}" --locked
```

Until the crates are publicly published, replace that install step with a pinned GitHub Release binary download or a tagged source install:

```bash
cargo install --git https://github.com/influxdata/influxdb3-plugin-sdk --tag vX.Y.Z influxdb3-plugin-cli
```

## Authentication

The workflow expects a secret named `GH_RELEASE_TOKEN`.

Create a fine-grained GitHub personal access token with:

- Resource owner: the organization or user that owns `REGISTRY_REPO`.
- Repository access: only the registry repository.
- Repository permissions: Contents read and write.

Add it to the plugin source repository:

```bash
gh secret set GH_RELEASE_TOKEN --repo YOUR_ORG/YOUR_PLUGIN_SOURCE_REPO
```

The workflow uses the token only for `gh release download` and `gh release upload`.

## Registry Release

Before the workflow can run, create the release and seed an empty index:

```bash
REGISTRY_REPO="YOUR_ORG/YOUR_REGISTRY_REPO"
REGISTRY_TAG="plugin-registry"
ARTIFACTS_URL="https://github.com/${REGISTRY_REPO}/releases/download/${REGISTRY_TAG}"

gh release create "${REGISTRY_TAG}" \
  --repo "${REGISTRY_REPO}" \
  --title "Plugin Registry" \
  --notes "Plugin registry index and artifacts"

SEED_DIR="$(mktemp -d)"
influxdb3-plugin new index "${SEED_DIR}" --artifacts-url "${ARTIFACTS_URL}"
gh release upload "${REGISTRY_TAG}" "${SEED_DIR}/index.json" --repo "${REGISTRY_REPO}"
```

## Repository Layout

The template expects this shape by default:

```text
plugins/
  downsampler/
    manifest.toml
    __init__.py
  request-router/
    manifest.toml
    __init__.py
```

Change `PLUGIN_ROOT` if your plugin directories live somewhere else.

## How It Publishes

The workflow:

1. Downloads the current `index.json` from the registry release.
2. Finds plugin directories under `PLUGIN_ROOT`.
3. Reads each `manifest.toml`.
4. Skips `(name, version)` pairs already present in the index.
5. Runs `influxdb3-plugin validate`.
6. Runs `influxdb3-plugin package`.
7. Uploads new `{name}-{version}.tar.gz` artifacts.
8. Uploads the derived `index.json` with `--clobber`.

Existing plugin versions are immutable. To publish a change, bump the plugin version in `manifest.toml`.

## Coexistence With `gh:`

The workflow does not delete, rename, or move existing source files used by `gh:` consumers. A repository can keep those files in place while publishing SDK artifacts from `plugins/` or another directory.

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `gh release download` cannot find `index.json` | The registry release was not seeded. | Create the release and upload the initial index. |
| `HTTP 404` or `Resource not accessible by integration` | `GH_RELEASE_TOKEN` is missing or lacks access to `REGISTRY_REPO`. | Recreate the fine-grained token with Contents read/write on the registry repo. |
| `already present in registry index` | The plugin version is already published. | Bump `plugin.version` in `manifest.toml` before republishing. |
| Validation fails | The manifest or Python entry point does not match the schema. | Run `influxdb3-plugin validate <plugin-dir>` locally and fix the reported diagnostics. |

Back to [Templates](../).

Next: [Create a new private registry](../../01-getting-started/recipes/github--ghactions--ghreleases--private--new.md).
