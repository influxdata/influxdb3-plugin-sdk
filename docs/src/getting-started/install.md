# Install The CLI

`influxdb3-plugin` is the standalone CLI installed by the `influxdb3-plugin-cli` crate.

## Public Go-Live Path

After the crates are public, install from crates.io:

```bash
cargo install influxdb3-plugin-cli --locked
```

For reproducible CI, pin the crate version:

```bash
cargo install influxdb3-plugin-cli --version X.Y.Z --locked
```

The Cargo install paths require a Rust toolchain.

## Current Transitional Path

Until the crates are publicly published, download the pinned binary for your platform from [GitHub Releases](https://github.com/influxdata/influxdb3-plugin-sdk/releases) and place it on your `PATH`, or build from a tagged source checkout:

```bash
cargo install --git https://github.com/influxdata/influxdb3-plugin-sdk --tag vX.Y.Z influxdb3-plugin-cli
```

Use a tag that matches the SDK release you want to run.

## Verify

```bash
influxdb3-plugin --version
```

Next: [Getting Started](./).
