# influxdb3-plugin-sdk

Rust tooling for authoring, validating, packaging, and publishing InfluxDB 3 plugins.

This repository contains the author-side SDK for InfluxDB 3 Processing Engine plugins. It provides the `influxdb3-plugin` CLI plus shared Rust crates for plugin manifest and registry-index handling.

## Getting Started

Start with the [`influxdb3-plugin-cli` README](influxdb3-plugin-cli/README.md) for installation and command usage.

For crate-specific details, see:

- [`influxdb3-plugin-schemas`](influxdb3-plugin-schemas/README.md)
- [`influxdb3-plugin-sdk`](influxdb3-plugin-sdk/README.md)
- [`influxdb3-plugin-cli`](influxdb3-plugin-cli/README.md)

## Development

Requires Rust version pinned in `rust-toolchain.toml`.

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for contribution guidance, versioning rules, and release discipline. Use [`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md) as the source of truth for PR checks.

## Maintainers

This project is maintained by the InfluxData Product team.

Primary point of contact: Ryan Cater <rcater@influxdata.com>

## Help

For usage questions and community discussion, use the [InfluxData Community Forum](https://community.influxdata.com/) or [InfluxData Community Slack](https://www.influxdata.com/slack/).

Use GitHub issues for reproducible bugs and feature requests.

## Security

Please report security issues privately. See `SECURITY.md`.

## License

This project is licensed under either the MIT license or the Apache License, Version 2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE`.
