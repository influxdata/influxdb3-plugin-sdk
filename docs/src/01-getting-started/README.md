# Getting Started

Choose the path that matches the plugin repository you maintain.

The v1 documentation is focused on private GitHub repositories that publish a registry to GitHub Releases from GitHub Actions. The migration path is additive: a repository can keep existing `gh:` consumers working while adding SDK-published artifacts and a registry index.

| Repo host | CI runner | Registry host | Visibility | Action | Guide |
|---|---|---|---|---|---|
| GitHub | GitHub Actions | GitHub Releases | Private | New repository | Added in the recipes stage |
| GitHub | GitHub Actions | GitHub Releases | Private | Migrate from `gh:` | Added in the recipes stage |

## Install The CLI

At public go-live, install from crates.io:

```bash
cargo install influxdb3-plugin-cli --locked
```

Until the crates are publicly published, use the current transitional path: install the pinned GitHub Release binary, or build from a tagged source checkout:

```bash
cargo install --git https://github.com/influxdata/influxdb3-plugin-sdk --tag vX.Y.Z influxdb3-plugin-cli
```

## What You Will Build

The v1 recipes produce the same end state:

- A plugin source repository with one or more `manifest.toml` files.
- A GitHub Actions workflow that validates and packages plugin artifacts.
- A private GitHub Release that stores the registry `index.json` and versioned plugin archives.
- A migration path that does not remove or break existing `gh:` source files.

Next: [Reference overview](../02-reference/).
