# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). The version anchor for a tagged release is the `influxdb3-plugin-cli` crate; library crates (`influxdb3-plugin-schemas`, `influxdb3-plugin-sdk`) may have different versions per the per-crate versioning model documented in `CONTRIBUTING.md`.

## [Unreleased]

## [0.1.0-2.rc.0] - 2026-04-28

Second release rehearsal. Fixes cross-compilation TARGET env var for aarch64-apple-darwin and aarch64-unknown-linux-gnu.

## [0.1.0-1.rc.0] - 2026-04-28

Initial release rehearsal (partial — 2 of 4 targets succeeded; aarch64 targets failed due to missing TARGET env var).
