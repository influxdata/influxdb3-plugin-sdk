# GitHub Releases + GitHub Actions

Publish versioned InfluxDB 3 plugin artifacts to a private GitHub Release by using GitHub Actions.

For the model that explains how this pipeline relates to other backends and runners, see [How publish pipelines vary](../concepts/publish-pipeline.md).

## Scenario

- Registry backend: a single GitHub Release that stores `index.json` and `{name}-{version}.tar.gz` assets.
- CI runner: GitHub Actions on the plugin source repository.
- Repo host: GitHub.
- Visibility: private (see [Public visibility](#public-visibility) for the public variant).

## Choose your starting state

- [New repository](#new) — start a plugin source repository from scratch.
- [Migrate from `gh:`](#migrate) — add the SDK to a repository that already distributes plugins via the `gh:` prefix mechanism.

After the state-specific steps, both paths converge on [Common steps](#common-steps).

## Prerequisites

- A GitHub account that can create the target repository.
- The `gh` CLI installed and authenticated.
- The `influxdb3-plugin` CLI, installed with the current path from [Install the CLI](../install.md).

## New

### Step 1: Create the repository

Create a private plugin source repository:

```bash
PLUGIN_REPO="YOUR_ORG/my-private-plugins"

gh repo create "${PLUGIN_REPO}" --private --clone
cd my-private-plugins
```

Create the default plugin directory layout:

```bash
mkdir -p plugins
influxdb3-plugin new process_writes plugins/downsampler --name downsampler
```

The scaffold writes:

```text
plugins/downsampler/
  manifest.toml
  __init__.py
  README.md
```

When the repository is initialized and the `plugins/<plugin>/` scaffold is in place, continue to [Common steps](#common-steps).

## Migrate

### Step 1: Clone the existing repository

```bash
PLUGIN_REPO="YOUR_ORG/existing-plugin-repo"

gh repo clone "${PLUGIN_REPO}"
cd existing-plugin-repo
```

Keep the existing `gh:` source layout in place. Add an SDK packaging area alongside it:

```bash
mkdir -p plugins
```

For each plugin you want to publish through the registry, create one directory under `plugins/`:

```bash
mkdir -p plugins/downsampler
cp path/to/existing/downsampler.py plugins/downsampler/__init__.py
```

If an existing plugin already lives in a directory with `__init__.py`, copy that directory instead:

```bash
cp -R path/to/existing/downsampler plugins/downsampler
```

Do not delete the old files used by `gh:` consumers.

When the existing `gh:` files are preserved and the new SDK packaging area is in place, continue to [Common steps](#common-steps).

## Common steps

### Step 1: Author the manifest

Author `plugins/downsampler/manifest.toml`. If you followed the New path, the scaffold wrote a stub for you to fill in; if you followed the Migrate path, create the file now.

Minimal shape:

```toml
manifest_schema_version = "1.1"

[plugin]
name = "downsampler"
version = "0.1.0"
description = "Downsample incoming writes."
triggers = ["process_writes"]

[dependencies]
database_version = ">=3.2.0,<4.0.0"
```

The `triggers` array must match the functions implemented by the Python file. See [The Manifest Format](../../reference/manifest.md) for all fields and validation rules.

Validate locally before wiring CI:

```bash
influxdb3-plugin validate plugins/downsampler
```

### Step 2: Create the registry release

Use one GitHub Release as the registry. The release stores `index.json` and all `{name}-{version}.tar.gz` artifacts.

```bash
REGISTRY_REPO="${PLUGIN_REPO}"
REGISTRY_TAG="plugin-registry"
ARTIFACTS_URL="https://github.com/${REGISTRY_REPO}/releases/download/${REGISTRY_TAG}"

gh release create "${REGISTRY_TAG}" \
  --repo "${REGISTRY_REPO}" \
  --title "Plugin Registry" \
  --notes "Plugin registry index and artifacts"
```

### Step 3: Seed the index

Generate and upload the initial empty registry index:

```bash
SEED_DIR="$(mktemp -d)"
influxdb3-plugin new index "${SEED_DIR}" --artifacts-url "${ARTIFACTS_URL}"
gh release upload "${REGISTRY_TAG}" "${SEED_DIR}/index.json" --repo "${REGISTRY_REPO}"
```

See [The Registry Index Format](../../reference/registry-index.md) for the index schema.

### Step 4: Add the GitHub Actions workflow

Create the workflow directory and copy the template:

```bash
mkdir -p .github/workflows
curl -fsSLo .github/workflows/publish.yml \
  https://raw.githubusercontent.com/influxdata/influxdb3-plugin-sdk/main/docs/src/templates/github-actions/publish.yml
```

Edit `.github/workflows/publish.yml`:

```yaml
env:
  SDK_VERSION: "X.Y.Z"
  PLUGIN_ROOT: "plugins"
  REGISTRY_REPO: "YOUR_ORG/my-private-plugins"
  REGISTRY_TAG: "plugin-registry"
```

The template walkthrough is [GitHub Actions publish workflow](../../templates/github-actions/).

### Step 5: Configure authentication

Create a fine-grained GitHub personal access token:

- Resource owner: `YOUR_ORG`.
- Repository access: `${REGISTRY_REPO}`.
- Repository permissions: Contents read and write.

Save it as a repository secret:

```bash
gh secret set GH_RELEASE_TOKEN --repo "${PLUGIN_REPO}"
```

### Step 6: Trigger the first publish

Commit and push:

```bash
git add .
git commit -m "Add initial plugin registry"
git push origin main
```

Watch the workflow:

```bash
gh run list --repo "${PLUGIN_REPO}" --workflow publish.yml
```

After it succeeds, the registry release contains:

- `index.json`
- one `{name}-{version}.tar.gz` artifact for each newly published plugin version

Verify the registry locally:

```bash
gh release download "${REGISTRY_TAG}" \
  --repo "${REGISTRY_REPO}" \
  --pattern index.json \
  --dir /tmp/plugin-registry \
  --clobber

influxdb3-plugin search --index /tmp/plugin-registry/index.json downsampler
```

### Step 7: Verify installation

Download and extract the published artifact:

```bash
gh release download "${REGISTRY_TAG}" \
  --repo "${REGISTRY_REPO}" \
  --pattern "downsampler-0.1.0.tar.gz" \
  --dir /tmp/downsampler-artifact \
  --clobber

mkdir -p /tmp/downsampler-extract
tar -xzf /tmp/downsampler-artifact/downsampler-0.1.0.tar.gz -C /tmp/downsampler-extract
find /tmp/downsampler-extract -maxdepth 2 -type f | sort
```

The archive extracts to a top-level `downsampler-0.1.0/` directory. Copy that directory, or its contents, into the plugin directory configured for your InfluxDB 3 host.

If you use the HTTP API path instead of a manual file move, extract the archive first and send the extracted file entries to `PUT /api/v3/plugins/directory`. Do not send the tarball bytes to `/api/v3/plugins/files`; that endpoint accepts single-file content, not plugin archives.

## Coexistence with `gh:`

The old `gh:` path and the SDK registry path can exist side by side:

- Existing consumers that reference `gh:` keep using the old source files.
- New consumers can use artifacts and metadata from the registry release.
- The GitHub Actions workflow only reads `PLUGIN_ROOT`; it does not modify old `gh:` files.

Keep both paths until every consumer has moved to the registry.

## Public visibility

The default scenario uses a private repository and a private GitHub Release. To publish to a public registry instead:

- Create the repository with `--public` instead of `--private`, or change the registry repository's visibility under repo settings.
- The `GH_RELEASE_TOKEN` secret still controls write access; tokens for read are not required because public release assets download without authentication.
- Consumers of a public registry use the same `artifacts_url` and the same `index.json`; the URL is the same shape, just unauthenticated.

Every other step in this recipe is identical for public and private registries.

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `gh release upload` returns 404 | The token cannot access the registry repo. | Check `GH_RELEASE_TOKEN` and repository permissions. |
| Workflow says the version is already present | The same `(name, version)` is already in `index.json`. | Bump `plugin.version` in `manifest.toml`. |
| `influxdb3-plugin package` fails validation | Manifest metadata and Python entry points do not match. | Run `influxdb3-plugin validate plugins/downsampler` locally. |
| `validate` reports a missing trigger implementation | The manifest declares a trigger that the copied Python file does not implement. | Update `triggers` or the Python entry point. |
| No plugins are packaged | `PLUGIN_ROOT` points at the wrong directory. | Set `PLUGIN_ROOT` to the directory containing plugin subdirectories. |
| Existing `gh:` users break | Old files were moved or deleted. | Restore the original source layout and keep SDK packaging files alongside it. |
| `gh release download` cannot find `index.json` | The registry release was not seeded. | Run the seed-index step again. |

Back to [Getting Started](../).

Next: [The Manifest Format](../../reference/manifest.md).
