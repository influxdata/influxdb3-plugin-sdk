# influxdb3-plugin-sdk

Tools for publishing versioned InfluxDB 3 Processing Engine plugins.

This repository contains the public SDK for maintainers of InfluxDB 3 plugin repositories. It provides the `influxdb3-plugin` CLI plus Rust crates for validating plugin manifests, packaging plugin artifacts, and maintaining registry indexes.

## Why Use It

- Publish versioned plugin artifacts instead of relying only on `gh:` source-file fetches.
- Validate `manifest.toml` and `index.json` before a broken plugin reaches users.
- Automate private plugin registry publishing from CI while keeping existing `gh:` consumers working during migration.

## Quickstart

At public go-live, install the CLI from crates.io:

```bash
cargo install influxdb3-plugin-cli --locked
influxdb3-plugin --help
```

Until the crates are publicly published, install the pinned GitHub Release binary or build from a tagged source checkout:

```bash
cargo install --git https://github.com/influxdata/influxdb3-plugin-sdk --tag vX.Y.Z influxdb3-plugin-cli
```

Start with the documentation when setting up a plugin repository:

- [Documentation source](docs/src/introduction.md)
- [Getting started](docs/src/01-getting-started/)
- [Manifest format](docs/src/02-reference/manifest.md)
- [Index format](docs/src/02-reference/registry-index.md)
- [Templates overview](docs/src/03-templates/)

The rendered documentation site will be published at <https://influxdata.github.io/influxdb3-plugin-sdk/>.

## Workspace Layout

| Path | Purpose |
|---|---|
| `influxdb3-plugin-schemas/` | Public schema types and validation for manifests and registry indexes. |
| `influxdb3-plugin-sdk/` | Library code for scaffolding, validating, packaging, hashing, and archive generation. |
| `influxdb3-plugin-cli/` | The `influxdb3-plugin` command-line interface. |
| `docs/` | mdBook source and internal design/reference material. |

## Install

Download the `influxdb3-plugin` binary for your platform from the
[GitHub Releases](https://github.com/influxdata/influxdb3-plugin-sdk/releases)
page and place it on your `PATH`.

The Rust crates in this workspace are currently not published to crates.io. The
CLI is the supported public interface for plugin authors.

## Quick start

Create and validate a new plugin:

```shell
influxdb3-plugin new process_writes ./my-plugin
influxdb3-plugin validate ./my-plugin
```

Package it against a local plugin index:

```shell
influxdb3-plugin new index ./my-registry --artifacts-url https://plugins.example.com/artifacts
influxdb3-plugin package ./my-plugin --index ./my-registry/index.json --out ./build
```

## Development

Requires the Rust version pinned in `rust-toolchain.toml`.

```bash
cargo build --workspace --locked
cargo nextest run --workspace --no-fail-fast --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidance, versioning rules, and release discipline. Use [.github/PULL_REQUEST_TEMPLATE.md](.github/PULL_REQUEST_TEMPLATE.md) as the source of truth for PR checks.

## Security

Report security issues privately. See [SECURITY.md](SECURITY.md).

## Maintainers

This project is maintained by the InfluxData Product team.

Primary point of contact: Ryan Cater <rcater@influxdata.com>

For usage questions and community discussion, use the [InfluxData Community Forum](https://community.influxdata.com/) or [InfluxData Community Slack](https://www.influxdata.com/slack/). Use GitHub issues for reproducible bugs and feature requests.

## License

This project is licensed under either the MIT license or the Apache License, Version 2.0, at your option. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
