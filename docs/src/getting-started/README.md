# Getting Started

Choose the path that matches the plugin repository you maintain.

The v1 documentation is focused on private GitHub repositories that publish a registry to GitHub Releases from GitHub Actions. The migration path is additive: a repository can keep existing `gh:` consumers working while adding SDK-published artifacts and a registry index.

| Repo host | CI runner | Registry host | Visibility | Action | Guide |
|---|---|---|---|---|---|
| GitHub | GitHub Actions | GitHub Releases | Private | New repository | [Create a new private registry](./recipes/github--ghactions--ghreleases--private--new.md) |
| GitHub | GitHub Actions | GitHub Releases | Private | Migrate from `gh:` | [Migrate from `gh:`](./recipes/github--ghactions--ghreleases--private--migrate.md) |

## Install The CLI

Install `influxdb3-plugin` before running a recipe. See [Install the CLI](./install.md) for the canonical install commands and current channel guidance.

## What You Will Build

The v1 recipes produce the same end state:

- A plugin source repository with one or more `manifest.toml` files.
- A GitHub Actions workflow that validates and packages plugin artifacts.
- A private GitHub Release that stores the registry `index.json` and versioned plugin archives.
- A migration path that does not remove or break existing `gh:` source files.

Next: [Reference overview](../reference/).
