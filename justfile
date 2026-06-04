# justfile for influxdb3-plugin-sdk release operator tasks.
# Adapted from influxdata/influxdb_pro/justfile for the SDK's per-crate
# versioning model (Group B). All recipes are operator-driven; CI does
# not invoke any of these.
#
# See RELEASE.md for the canonical release procedure.

# Map crate short name to directory.
crate_dir_for CRATE:
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{CRATE}}" in
        schemas) echo "influxdb3-plugin-schemas" ;;
        sdk)     echo "influxdb3-plugin-sdk" ;;
        cli)     echo "influxdb3-plugin-cli" ;;
        *)
            echo "ERROR: crate must be one of: schemas, sdk, cli (got '{{CRATE}}')" >&2
            exit 1
            ;;
    esac

# Validate a SemVer string (full SemVer including pre-release/build metadata).
_validate_semver VERSION:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! [[ "{{VERSION}}" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$ ]]; then
        echo "ERROR: version must be valid SemVer (got '{{VERSION}}')" >&2
        echo "       (do NOT include the leading 'v' — that's added by tag-version)" >&2
        exit 1
    fi

# Bump one crate's version. Updates the workspace path-dep version
# constraint when the crate is a workspace dep (schemas, sdk).
# Optionally cascades the same version bump to consumers (--cascade).
#
# Usage:
#   just cut-version cli 0.2.0
#   just cut-version schemas 0.2.0 --cascade
cut-version CRATE VERSION CASCADE='':
    #!/usr/bin/env bash
    set -euo pipefail
    just _validate_semver {{VERSION}}
    crate_dir="$(just crate_dir_for {{CRATE}})"

    # Sed -E for portability across GNU and BSD sed; the .bak suffix is
    # required for BSD sed compatibility (macOS), then removed.
    sed -i.bak -E 's/^version = "[^"]*"$/version = "{{VERSION}}"/' "$crate_dir/Cargo.toml"
    rm "$crate_dir/Cargo.toml.bak"

    # Verify the substitution actually fired (defensive against future
    # Cargo.toml reformats that no longer match the regex).
    if ! grep -qE '^version = "{{VERSION}}"$' "$crate_dir/Cargo.toml"; then
        echo "ERROR: failed to write version {{VERSION}} into $crate_dir/Cargo.toml" >&2
        echo "       (sed didn't match the expected pattern; check the file)" >&2
        exit 1
    fi

    # If the crate is referenced as a workspace path-dep (schemas or sdk),
    # update the version = "..." constraint in the root Cargo.toml.
    # cli is intentionally absent from this case — root Cargo.toml has
    # no path-dep on cli (cli is the binary; nothing depends on it).
    case "{{CRATE}}" in
        schemas|sdk)
            sed -i.bak -E "s|(influxdb3-plugin-{{CRATE}} = \\{ path = \"[^\"]*\", version = )\"[^\"]*\"|\\1\"{{VERSION}}\"|" Cargo.toml
            rm Cargo.toml.bak
            if ! grep -qF "influxdb3-plugin-{{CRATE}} = { path = \"$crate_dir\", version = \"{{VERSION}}\"" Cargo.toml; then
                echo "ERROR: failed to update path-dep constraint for {{CRATE}} in root Cargo.toml" >&2
                echo "       (sed didn't match the expected pattern; check the file)" >&2
                exit 1
            fi
            ;;
    esac

    # Verify the workspace still resolves (catches any sed mishaps).
    cargo check --workspace --quiet

    echo "Bumped {{CRATE}} to {{VERSION}}"

    if [ "{{CASCADE}}" = "--cascade" ]; then
        case "{{CRATE}}" in
            schemas)
                just cut-version sdk {{VERSION}}
                just cut-version cli {{VERSION}}
                ;;
            sdk)
                just cut-version cli {{VERSION}}
                ;;
            cli)
                # cli has no consumers in the workspace
                :
                ;;
        esac
    else
        # Without --cascade, warn if there are consumers that may need bumping.
        case "{{CRATE}}" in
            schemas)
                echo "NOTE: schemas is consumed by sdk and cli (cli also re-exports schemas types)." >&2
                echo "      For breaking bumps, also run: just cut-version sdk {{VERSION}} && just cut-version cli {{VERSION}}" >&2
                echo "      Or re-run with --cascade." >&2
                ;;
            sdk)
                echo "NOTE: sdk is consumed by cli." >&2
                echo "      For breaking bumps, also run: just cut-version cli {{VERSION}}" >&2
                echo "      Or re-run with --cascade." >&2
                ;;
        esac
    fi

# Lockstep bump all 3 crates + their path-dep constraints to the same version.
# Use only when intentionally aligning all crates (e.g., release rehearsal RC,
# first stable release).
cut-version-all VERSION:
    #!/usr/bin/env bash
    set -euo pipefail
    just _validate_semver {{VERSION}}
    just cut-version schemas {{VERSION}}
    just cut-version sdk {{VERSION}}
    just cut-version cli {{VERSION}}
    echo "Lockstep-bumped all 3 crates to {{VERSION}}"

# Create an annotated tag of the form vX.Y.Z anchored to cli's version.
# Refuses unless local HEAD == origin/main (prevents the squash-merge
# footgun where the operator forgets to update local main after gh pr merge).
# Refuses if cli's Cargo.toml version doesn't match the requested version.
# Refuses if the tag already exists.
tag-version VERSION:
    #!/usr/bin/env bash
    set -euo pipefail
    just _validate_semver {{VERSION}}

    # Validate working tree is clean (uncommitted Cargo.toml changes would
    # make cargo metadata report a version that differs from the committed
    # state, causing the tag to point at the wrong version).
    if ! git diff --exit-code --quiet 2>/dev/null; then
        echo "ERROR: working tree has uncommitted changes." >&2
        echo "       Commit or stash before tagging." >&2
        exit 1
    fi
    if ! git diff --cached --exit-code --quiet 2>/dev/null; then
        echo "ERROR: index has staged but uncommitted changes." >&2
        echo "       Commit or reset before tagging." >&2
        exit 1
    fi

    # Validate local HEAD == origin/main.
    git fetch origin main --quiet
    local_head="$(git rev-parse HEAD)"
    main_head="$(git rev-parse origin/main)"
    if [ "$local_head" != "$main_head" ]; then
        echo "ERROR: local HEAD ($local_head) != origin/main ($main_head)." >&2
        echo "       Run: git checkout main && git pull --ff-only origin main" >&2
        exit 1
    fi

    # Validate cli's Cargo.toml version matches.
    cli_version="$(cargo metadata --format-version 1 --no-deps \
        | jq -r '.packages[] | select(.name == "influxdb3-plugin-cli") | .version')"
    if [ "$cli_version" != "{{VERSION}}" ]; then
        echo "ERROR: cli's Cargo.toml version is $cli_version, not {{VERSION}}." >&2
        echo "       Run: just cut-version cli {{VERSION}} (or cut-version-all)" >&2
        exit 1
    fi

    # Validate tag doesn't already exist.
    if git rev-parse "v{{VERSION}}" >/dev/null 2>&1; then
        echo "ERROR: tag v{{VERSION}} already exists." >&2
        exit 1
    fi

    # Create the annotated tag.
    git tag -a "v{{VERSION}}" -m "Release v{{VERSION}}"
    echo "Created annotated tag v{{VERSION}}. Push with: git push origin v{{VERSION}}"

# Spot-check a published GitHub Release: verify the 4 expected tarballs
# and SHA256SUMS are present. Manual followup: download the Linux x86_64
# binary, extract, run --version, confirm revision matches the tag's
# commit SHA (PR-13 adds CI-side runtime verification).
verify-version VERSION:
    #!/usr/bin/env bash
    set -euo pipefail
    just _validate_semver {{VERSION}}

    # Confirm the release exists and lists all 4 tarballs + SHA256SUMS.
    # Use LC_ALL=C sort for byte-order determinism. SHA256SUMS sorts
    # FIRST byte-wise (capital S = 0x53 < lowercase i = 0x69).
    asset_names="$(gh release view "v{{VERSION}}" --repo influxdata/influxdb3-plugin-sdk \
        --json assets --jq '.assets[].name' | LC_ALL=C sort)"
    expected="$(printf '%s\n' \
        SHA256SUMS \
        influxdb3-plugin-aarch64-apple-darwin.tar.gz \
        influxdb3-plugin-aarch64-unknown-linux-gnu.tar.gz \
        influxdb3-plugin-x86_64-pc-windows-gnu.tar.gz \
        influxdb3-plugin-x86_64-unknown-linux-gnu.tar.gz)"
    if [ "$asset_names" != "$expected" ]; then
        echo "ERROR: assets on the v{{VERSION}} release don't match expectations." >&2
        echo "Expected (byte-sorted):" >&2
        echo "$expected" >&2
        echo "Got:" >&2
        echo "$asset_names" >&2
        exit 1
    fi
    echo "All 5 expected assets present on v{{VERSION}} (4 tarballs + SHA256SUMS)."
    echo "Manual follow-up: download Linux x86_64 binary, extract, run --version, confirm revision matches v{{VERSION}}'s commit SHA."

# Dry-run the crates.io publish plan for the current working tree.
# Prints what would be published; publishes nothing. Safe to run anytime.
publish-crates-io-dry-run:
    ./scripts/publish-crates-io.sh --dry-run

# Verify every crate's current Cargo.toml version is live on crates.io.
# Post-release check; non-zero exit if any version is missing.
verify-crates-io:
    ./scripts/publish-crates-io.sh --verify

# Manually publish unpublished crate versions to crates.io. EMERGENCY /
# RECOVERY ONLY — CI does this automatically on a stable vX.Y.Z tag. Requires
# CARGO_REGISTRY_TOKEN in the environment. crates.io versions are immutable.
publish-crates-io:
    ./scripts/publish-crates-io.sh
