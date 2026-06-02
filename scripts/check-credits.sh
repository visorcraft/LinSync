#!/usr/bin/env bash
#
# Drift-guard for third-party attribution.
#
# The three credit surfaces — docs/third-party-notices.md, the in-app Credits
# page (apps/linsync-gui/qml/CreditsPage.qml), and the Licenses page
# (apps/linsync-gui/qml/LicensesPage.qml) — must list exactly the set of
# third-party crates distributed in the released binaries. That set is computed
# from `cargo tree` over the shipped feature set (cxxqt-app + web-engine) on the
# Linux target, including build dependencies and excluding dev-only crates.
#
# This guards the crate *name set* (the failure mode that bit us: feature-gated
# crates silently going unattributed). Versions in the surfaces are maintained
# by hand; regenerate them with `just credits` after any dependency change.
#
# Exit 0 when every surface matches the authoritative set; non-zero otherwise.
set -euo pipefail

cd "$(dirname "$0")/.."

TARGET="x86_64-unknown-linux-gnu"

# Names are read with `--format '{p}'` (package id from the resolved lockfile)
# rather than '{p}|{l}': the license field forces cargo to read each crate's
# manifest, which fails on a CI runner whose CARGO_HOME has not fetched the
# feature-gated (web-engine) crate sources. The guard only needs the name set,
# and {p} comes straight from Cargo.lock without any manifest read.
authoritative_names() {
    {
        cargo tree -p linsync --no-default-features \
            --features cxxqt,cxxqt-app,web-engine -e no-dev \
            --target "${TARGET}" --format '{p}'
        cargo tree -p linsync-cli --features web-engine -e no-dev \
            --target "${TARGET}" --format '{p}'
    } | sed -E 's/^[^a-zA-Z0-9]*//; s/ \(proc-macro\)//; s/ \(\*\)$//' \
      | grep -E '^[a-zA-Z0-9_-]+ v[0-9]' \
      | grep -vE '^(linsync|linsync-core|linsync-cli|linsync-sandbox|linsync-webengine) ' \
      | sed -E 's/ v[0-9].*//' | sort -u
}

# Crate names listed in docs/third-party-notices.md (excluding the external
# runtime-helper binaries, which are not Cargo crates).
notices_names() {
    # shellcheck disable=SC2016  # backticks here are literal Markdown code-span delimiters
    grep -E '^\| `' docs/third-party-notices.md \
      | grep -vE 'pdftotext|tesseract|libreoffice' \
      | sed -E 's/^\| `([^`]+)`.*/\1/' | sort -u
}

# Crate names in the CreditsPage `crates` array. Matching the object-literal
# prefix `{ name: "..."` excludes both `icon.name: "..."` properties and the
# `runtimeComponents` entries (whose names carry spaces/capitals anyway).
credits_names() {
    grep -oE '\{ name: "[a-z0-9][a-z0-9_-]*"' apps/linsync-gui/qml/CreditsPage.qml \
      | sed -E 's/\{ name: "([a-z0-9_-]+)"/\1/' | sort -u
}

# Crate names in the LicensesPage "Cargo Dependencies" Markdown table.
licenses_names() {
    grep -oE '"\| [a-z0-9][a-z0-9_-]* ' apps/linsync-gui/qml/LicensesPage.qml \
      | sed -E 's/"\| ([a-z0-9_-]+) /\1/' | sort -u
}

EXPECTED="$(authoritative_names)"
expected_count="$(echo "${EXPECTED}" | grep -c . || true)"
echo "Authoritative distributed crates (shipped build): ${expected_count}"

# Guard against a cargo/resolve hiccup producing a short or empty list, which
# would otherwise masquerade as "everything drifted". The real set is ~113.
if [[ "${expected_count}" -lt 80 ]]; then
    echo "ERROR: cargo tree returned only ${expected_count} crates (expected ~113)." >&2
    echo "This is a resolve/toolchain failure, not attribution drift. Aborting." >&2
    exit 2
fi

fail=0
check_surface() {
    local label="$1" actual="$2"
    local missing extra
    missing="$(comm -23 <(echo "${EXPECTED}") <(echo "${actual}"))"
    extra="$(comm -13 <(echo "${EXPECTED}") <(echo "${actual}"))"
    if [[ -n "${missing}" || -n "${extra}" ]]; then
        echo "DRIFT in ${label}:"
        [[ -n "${missing}" ]] && echo "  missing (distributed but unlisted):" && echo "${missing}" | awk '{ print "    - " $0 }'
        [[ -n "${extra}" ]]   && echo "  extra (listed but not distributed):" && echo "${extra}" | awk '{ print "    + " $0 }'
        fail=1
    else
        echo "OK: ${label} (matches all ${expected_count} crates)"
    fi
}

check_surface "docs/third-party-notices.md" "$(notices_names)"
check_surface "CreditsPage.qml"             "$(credits_names)"
check_surface "LicensesPage.qml"            "$(licenses_names)"

if [[ "${fail}" -ne 0 ]]; then
    echo
    echo "Third-party attribution is out of sync with the shipped dependency graph."
    echo "Run 'just credits' and update the three surfaces above, then re-run this check."
    exit 1
fi
echo "All three credit surfaces are in sync with the shipped dependency graph."
