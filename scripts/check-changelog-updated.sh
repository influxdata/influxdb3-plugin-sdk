#!/usr/bin/env bash
# Enforces that CHANGELOG.md is updated when public-API source changes.
#
# Logic:
#   If the PR diff modifies .rs files in influxdb3-plugin-cli/src/ or
#   influxdb3-plugin-schemas/src/ (the two semver-stable crates), then
#   CHANGELOG.md must also be modified in the same PR.
#
# Does NOT require changelog updates for:
#   - influxdb3-plugin-sdk/src/ (internal crate; may change without notice)
#   - tests/, scripts/, .circleci/, docs/, justfile, RELEASE.md, etc.
#
# No escape hatch by design. If the gate fires, add an entry under
# ## [Unreleased] describing the user-visible change. See CONTRIBUTING.md.

set -euo pipefail

# Detect the base branch to diff against.
# On CircleCI PRs, CIRCLE_BRANCH is the PR branch; diff against origin/main.
# Locally, diff against origin/main as well.
BASE="${CHANGELOG_CHECK_BASE:-origin/main}"

# Get the list of changed files in this PR/branch.
changed_files="$(git diff --name-only "$BASE"...HEAD 2>/dev/null || git diff --name-only "$BASE" HEAD)"

# Check if any public-API source files changed.
public_api_changed=false
while IFS= read -r file; do
    case "$file" in
        influxdb3-plugin-cli/src/*.rs) public_api_changed=true ;;
        influxdb3-plugin-schemas/src/*.rs) public_api_changed=true ;;
    esac
done <<< "$changed_files"

if [ "$public_api_changed" = false ]; then
    echo "No public-API source changes (cli/src/ or schemas/src/). Changelog update not required."
    exit 0
fi

# Public API changed — verify CHANGELOG.md was also modified.
if echo "$changed_files" | grep -q '^CHANGELOG.md$'; then
    echo "Public-API source changed + CHANGELOG.md updated. OK."
    exit 0
fi

echo "FAIL: public-API source files changed but CHANGELOG.md was not updated." >&2
echo "" >&2
echo "Changed public-API files:" >&2
echo "$changed_files" | grep -E '^influxdb3-plugin-(cli|schemas)/src/.*\.rs$' | sed 's/^/  /' >&2
echo "" >&2
echo "Add an entry under '## [Unreleased]' in CHANGELOG.md describing the" >&2
echo "user-visible change. Use Keep a Changelog format:" >&2
echo "  ### Added / Changed / Deprecated / Removed / Fixed / Security" >&2
echo "  - Description of the change" >&2
echo "" >&2
echo "See CONTRIBUTING.md for bump rules and cascade documentation." >&2
exit 1
