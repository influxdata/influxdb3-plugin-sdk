#!/usr/bin/env bash
# Verify Group B manifest-shape invariants.
# Fails if any of these regress:
#   - root Cargo.toml has [workspace.package].version (lockstep versioning)
#   - any member crate uses version.workspace = true (lockstep inheritance)
#   - any path-dep in [workspace.dependencies] lacks a version constraint
#
# Note on `errors=$((errors+1))` with `set -e`: arithmetic expansion that
# evaluates to non-zero is treated as success by the shell — the
# expansion *result* matters, not the value. Do NOT "fix" this to
# `((errors++))`, which DOES trigger set -e when the pre-increment value
# is 0 and would defeat the counter pattern.

set -euo pipefail
cd "$(dirname "$0")/.."

errors=0

# Check 1: no [workspace.package].version (Group B replaced lockstep
# inheritance with literal per-crate versions). Use awk to bound the
# search to the [workspace.package] section regardless of section length.
# `\d` is NOT POSIX ERE — it would silently never match in `grep -E`.
if ! awk '
    /^\[workspace\.package\]/ { in_section=1; next }
    /^\[/ && in_section { in_section=0 }
    in_section && /^version[[:space:]]*=/ { found=1 }
    END { exit (found ? 1 : 0) }
' Cargo.toml ; then
    echo "FAIL: root Cargo.toml has [workspace.package].version (lockstep versioning regression)" >&2
    errors=$((errors+1))
fi

# Check 2: no `version.workspace = true` anywhere in the workspace.
# Anchored to start-of-line so `rust-version.workspace = true` (which is
# intentional and inherits the workspace's MSRV) doesn't match.
# Capture matches (no -q) so we can FAIL-prefix them clearly to stderr.
if matches="$(grep -rEn '^version\.workspace = true$' --include='Cargo.toml' . 2>/dev/null)"; then
    if [ -n "$matches" ]; then
        echo "FAIL: at least one Cargo.toml uses version.workspace = true (Group B regression):" >&2
        printf '%s\n' "$matches" | sed 's/^/  /' >&2
        errors=$((errors+1))
    fi
fi

# Check 3: every path-dep in [workspace.dependencies] has both path = and version =.
# Accumulate violations across all matching lines (don't exit on first).
if ! awk '
    /^\[workspace\.dependencies\]/ { in_section=1; next }
    /^\[/ && in_section { in_section=0 }
    in_section && /path = "/ && !/version = / {
        print "FAIL: path-dep without version constraint:", $0 > "/dev/stderr"
        violations=violations+1
    }
    END { exit (violations > 0 ? 1 : 0) }
' Cargo.toml ; then
    errors=$((errors+1))
fi

if [ "$errors" -gt 0 ]; then
    echo "" >&2
    echo "$errors invariant violation(s) found. See FAIL lines above." >&2
    exit 1
fi

echo "All manifest invariants pass."
