#!/usr/bin/env bash
# Verify the workspace's per-crate path-dep constraints have the expected
# caret shape (^0.x or ^0.x.y). Catches accidental constraint relaxations
# (e.g., dropping the version constraint, switching to wildcard).

set -euo pipefail
cd "$(dirname "$0")/.."

errors=0

# Each row is "consumer:dep" — verify cargo metadata reports the right shape.
for edge in 'cli:schemas' 'cli:sdk' 'sdk:schemas'; do
    IFS=: read -r consumer dep <<< "$edge"
    req=$(cargo metadata --format-version 1 --no-deps \
          | jq -r ".packages[] | select(.name == \"influxdb3-plugin-$consumer\") | .dependencies[] | select(.name == \"influxdb3-plugin-$dep\") | .req")
    if [ -z "$req" ]; then
        echo "FAIL: $consumer's dependency on $dep not found in cargo metadata" >&2
        errors=$((errors+1))
        continue
    fi
    # Expected shape: ^0.x or ^0.x.y (caret-default for 0.x versions).
    # Loosen this regex when the workspace moves past 0.x.
    if ! [[ "$req" =~ ^\^0\.[0-9]+(\.[0-9]+)?$ ]]; then
        echo "FAIL: $consumer→$dep req='$req' (expected ^0.x or ^0.x.y caret shape)" >&2
        errors=$((errors+1))
    fi
done

if [ "$errors" -gt 0 ]; then
    exit 1
fi

echo "All cargo metadata sanity checks pass."
