# InfluxDB 3 Plugin SDK

The InfluxDB 3 Plugin SDK helps plugin repository maintainers publish versioned plugin artifacts with machine-checkable metadata.

If you currently distribute plugins with the `gh:` prefix mechanism, the SDK gives you an incremental path to a registry-based workflow. Existing `gh:` consumers can continue to work while you add manifests, package artifacts, and publish an index for new consumers.

## What The SDK Provides

- `influxdb3-plugin`, a CLI for scaffolding, validating, packaging, searching, inspecting, and yanking plugin registry entries.
- `manifest.toml`, the public metadata format for one plugin version.
- `index.json`, the public registry format that lists published plugin versions and artifact hashes.
- CI-friendly commands that operate on local files and fit into GitHub Actions or another runner.

## Install The CLI

At public go-live, install from crates.io:

```bash
cargo install influxdb3-plugin-cli --locked
```

For reproducible CI, pin the crate version:

```bash
cargo install influxdb3-plugin-cli --version X.Y.Z --locked
```

Until the crates are publicly published, use the current transitional path: install the pinned GitHub Release binary, or build from a tagged source checkout:

```bash
cargo install --git https://github.com/influxdata/influxdb3-plugin-sdk --tag vX.Y.Z influxdb3-plugin-cli
```

## Pick A Path

Start with [Getting Started](./01-getting-started/README.md) to choose the repository setup that matches your situation.

The v1 documentation focuses on plugin repository maintainers. Plugin-author tutorials, plugin-user guides, and full CLI reference coverage are planned for later phases.

## Reference

The public schema contracts are:

- `manifest.toml`, owned by each plugin version.
- `index.json`, owned by each registry.

The reference section is where those formats are documented as stable contracts. See the [Reference overview](./02-reference/README.md).

## Templates

The templates section collects copy-pasteable CI/CD files and their walkthroughs. See the [Templates overview](./03-templates/README.md).
