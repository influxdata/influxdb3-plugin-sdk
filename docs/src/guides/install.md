# Installation

The easiest way to install the CLI is to first [install Cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html), and then you can install `influxdb3-plugin` from `crates.io`:

```bash
cargo install influxdb3-plugin-cli --locked
```

Alternatively, you can install from the [repo's GitHub Releases](https://github.com/influxdata/influxdb3-plugin-sdk/releases):

```bash
cargo install --git https://github.com/influxdata/influxdb3-plugin-sdk --tag latest influxdb3-plugin-cli
```

After installing, validate by running:

```bash
influxdb3-plugin --version
```

---

Back: [Guides](./) | Next: [Initialize a Registry](./initialize-registry.md)
