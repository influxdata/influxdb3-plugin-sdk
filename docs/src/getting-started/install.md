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

Until the crates are publicly published, download the pinned binary for your platform from [GitHub Releases](https://github.com/influxdata/influxdb3-plugin-sdk/releases) and place it on your `PATH`, or build from a tagged source checkout.

### Pin to a specific release (recommended)

For reproducible installs, pin to a specific `vX.Y.Z` tag:

```bash
cargo install --git https://github.com/influxdata/influxdb3-plugin-sdk \
  --tag vX.Y.Z \
  influxdb3-plugin-cli \
  --force
```

Replace `vX.Y.Z` with the release you want. This is the recommended form for CI and for any environment where reproducibility matters.

### Track the latest stable release

For local development or quick upgrades, you can install from the floating `latest` tag, which the release pipeline force-moves to the most recent stable release (`vX.Y.Z` with no prerelease suffix):

```bash
cargo install --git https://github.com/influxdata/influxdb3-plugin-sdk \
  --tag latest \
  influxdb3-plugin-cli \
  --force
```

Re-running this command after a new stable release reinstalls the new version (the `--force` flag is required because cargo will not otherwise replace the existing binary). Note that `latest` is a moving ref, so this form is not reproducible across time — use the pinned form above for CI.

## Verify

```bash
influxdb3-plugin --version
```

Next: [Getting Started](./).
