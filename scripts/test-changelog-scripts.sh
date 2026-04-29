#!/usr/bin/env bash
# Validation tests for check-changelog-format.sh and check-changelog-updated.sh.
# Run from repo root: ./scripts/test-changelog-scripts.sh

set -euo pipefail
cd "$(dirname "$0")/.."

TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0
FAILURES=""

pass() { TESTS_RUN=$((TESTS_RUN+1)); TESTS_PASSED=$((TESTS_PASSED+1)); echo "  ✓ $1"; }
fail() { TESTS_RUN=$((TESTS_RUN+1)); TESTS_FAILED=$((TESTS_FAILED+1)); FAILURES="${FAILURES}\n  ✗ $1"; echo "  ✗ $1"; }

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"; cp "$TMPDIR_BACKUP/CHANGELOG.md" CHANGELOG.md 2>/dev/null || true' EXIT

# Backup the real CHANGELOG.md
TMPDIR_BACKUP="$TMPDIR/backup"
mkdir -p "$TMPDIR_BACKUP"
cp CHANGELOG.md "$TMPDIR_BACKUP/CHANGELOG.md"

FORMAT_SCRIPT="./scripts/check-changelog-format.sh"

# Helper: write fixture to CHANGELOG.md, run format check, restore
test_format() {
    local desc="$1"
    local expect="$2"  # "pass" or "fail"
    local fixture="$3"

    echo "$fixture" > CHANGELOG.md
    local rc=0
    $FORMAT_SCRIPT >/dev/null 2>&1 || rc=$?
    cp "$TMPDIR_BACKUP/CHANGELOG.md" CHANGELOG.md

    if [ "$expect" = "pass" ] && [ "$rc" -eq 0 ]; then pass "$desc"
    elif [ "$expect" = "fail" ] && [ "$rc" -ne 0 ]; then pass "$desc"
    elif [ "$expect" = "pass" ]; then fail "$desc (expected pass, got exit $rc)"
    else fail "$desc (expected fail, got pass)"
    fi
}

# ─────────────────────────────────────────────────────────
echo "== check-changelog-format.sh =="
# ─────────────────────────────────────────────────────────

test_format "valid minimal changelog" pass \
"# Changelog

## [Unreleased]"

test_format "valid full changelog with versions + subsections" pass \
"# Changelog

## [Unreleased]

### Added
- New feature

## [0.2.0] - 2026-04-29

### Changed
- Something changed

### Fixed
- Bug fix

## [0.1.0] - 2026-04-28

### Added
- Initial release"

test_format "valid pre-release version header" pass \
"# Changelog

## [Unreleased]

## [0.1.0-1.rc.0] - 2026-04-28

### Added
- Rehearsal"

test_format "all 6 valid subsection types" pass \
"# Changelog

## [Unreleased]

### Added
### Changed
### Deprecated
### Removed
### Fixed
### Security"

test_format "FAIL: missing title" fail \
"## [Unreleased]"

test_format "FAIL: missing Unreleased section" fail \
"# Changelog

## [0.1.0] - 2026-04-28

### Added
- Something"

test_format "FAIL: version header missing date" fail \
"# Changelog

## [Unreleased]

## [0.1.0]

### Added
- Something"

test_format "FAIL: version header wrong date format" fail \
"# Changelog

## [Unreleased]

## [0.1.0] - 04-28-2026

### Added
- Something"

test_format "FAIL: invalid subsection name (Adds)" fail \
"# Changelog

## [Unreleased]

### Adds
- Something"

test_format "FAIL: duplicate version sections" fail \
"# Changelog

## [Unreleased]

## [0.1.0] - 2026-04-28

### Added
- First

## [0.1.0] - 2026-04-28

### Fixed
- Duplicate"

test_format "FAIL: Unreleased after versioned section" fail \
"# Changelog

## [0.1.0] - 2026-04-28

### Added
- Something

## [Unreleased]"

test_format "FAIL: invalid subsection Bugfixes" fail \
"# Changelog

## [Unreleased]

### Bugfixes
- Something"

test_format "FAIL: version missing brackets" fail \
"# Changelog

## [Unreleased]

## 0.1.0 - 2026-04-28

### Added
- Something"

# ─────────────────────────────────────────────────────────
echo ""
echo "== check-changelog-updated.sh =="
# ─────────────────────────────────────────────────────────

# Create a temp repo with a controlled baseline
TESTREPO="$TMPDIR/test-repo"
git init -b main "$TESTREPO" --quiet

# Set up baseline files
mkdir -p "$TESTREPO/influxdb3-plugin-cli/src"
mkdir -p "$TESTREPO/influxdb3-plugin-schemas/src"
mkdir -p "$TESTREPO/influxdb3-plugin-sdk/src"
mkdir -p "$TESTREPO/tests"
mkdir -p "$TESTREPO/scripts"
mkdir -p "$TESTREPO/.circleci"
echo "initial" > "$TESTREPO/CHANGELOG.md"
echo "fn main() {}" > "$TESTREPO/influxdb3-plugin-cli/src/main.rs"
echo "pub fn schema() {}" > "$TESTREPO/influxdb3-plugin-schemas/src/lib.rs"
echo "pub fn sdk() {}" > "$TESTREPO/influxdb3-plugin-sdk/src/lib.rs"
echo "fn test() {}" > "$TESTREPO/tests/smoke.rs"
echo "echo hi" > "$TESTREPO/scripts/check.sh"
echo "[workspace]" > "$TESTREPO/Cargo.toml"
echo "version: 2.1" > "$TESTREPO/.circleci/config.yml"
cp scripts/check-changelog-updated.sh "$TESTREPO/scripts/"
git -C "$TESTREPO" add -A
git -C "$TESTREPO" commit -m "initial" --quiet

# Helper: create a branch, modify files, run the check, cleanup
test_updated() {
    local desc="$1"
    local expect="$2"  # "pass" or "fail"
    shift 2
    # remaining args are "file:content" pairs

    local branch="test-$$-$RANDOM"
    git -C "$TESTREPO" checkout -b "$branch" --quiet

    for change in "$@"; do
        local file="${change%%:*}"
        local content="${change#*:}"
        mkdir -p "$TESTREPO/$(dirname "$file")"
        echo "$content" > "$TESTREPO/$file"
    done
    git -C "$TESTREPO" add -A
    git -C "$TESTREPO" commit -m "test" --quiet

    local rc=0
    (cd "$TESTREPO" && CHANGELOG_CHECK_BASE=main ./scripts/check-changelog-updated.sh) >/dev/null 2>&1 || rc=$?

    git -C "$TESTREPO" checkout main --quiet
    git -C "$TESTREPO" branch -D "$branch" --quiet 2>/dev/null

    if [ "$expect" = "pass" ] && [ "$rc" -eq 0 ]; then pass "$desc"
    elif [ "$expect" = "fail" ] && [ "$rc" -ne 0 ]; then pass "$desc"
    elif [ "$expect" = "pass" ]; then fail "$desc (expected pass, got exit $rc)"
    else fail "$desc (expected fail, got pass)"
    fi
}

test_updated "no source changes (scripts only)" pass \
    "scripts/check.sh:echo updated"

test_updated "only sdk/src changes (internal crate)" pass \
    "influxdb3-plugin-sdk/src/lib.rs:pub fn new_sdk() {}"

test_updated "only test file changes" pass \
    "tests/smoke.rs:fn new_test() {}"

test_updated "only Cargo.toml changes" pass \
    "Cargo.toml:[workspace]\nversion = 1"

test_updated "only CI config changes" pass \
    ".circleci/config.yml:version: 2.1\n# updated"

test_updated "cli/src changed + CHANGELOG updated" pass \
    "influxdb3-plugin-cli/src/main.rs:fn new_main() {}" \
    "CHANGELOG.md:updated changelog"

test_updated "schemas/src changed + CHANGELOG updated" pass \
    "influxdb3-plugin-schemas/src/lib.rs:pub fn new_schema() {}" \
    "CHANGELOG.md:updated changelog"

test_updated "cli + schemas changed + CHANGELOG updated" pass \
    "influxdb3-plugin-cli/src/main.rs:fn both() {}" \
    "influxdb3-plugin-schemas/src/lib.rs:pub fn both() {}" \
    "CHANGELOG.md:updated both"

test_updated "FAIL: cli/src changed, CHANGELOG NOT updated" fail \
    "influxdb3-plugin-cli/src/main.rs:fn forgot_changelog() {}"

test_updated "FAIL: schemas/src changed, CHANGELOG NOT updated" fail \
    "influxdb3-plugin-schemas/src/lib.rs:pub fn forgot_changelog() {}"

test_updated "FAIL: cli + schemas changed, CHANGELOG NOT updated" fail \
    "influxdb3-plugin-cli/src/main.rs:fn both_forgot() {}" \
    "influxdb3-plugin-schemas/src/lib.rs:pub fn both_forgot() {}"

# ─────────────────────────────────────────────────────────
echo ""
echo "== Results =="
echo "$TESTS_RUN tests, $TESTS_PASSED passed, $TESTS_FAILED failed"
if [ "$TESTS_FAILED" -gt 0 ]; then
    echo -e "\nFailures:$FAILURES"
    exit 1
fi
echo "All tests passed."
