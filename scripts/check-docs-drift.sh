#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 VisorCraft LLC
# SPDX-License-Identifier: GPL-3.0-only
#
# Advisory CI gate: warn when user-facing code changes without an update to
# docs/feature-matrix.md or docs/known-limitations-1.0.md.

set -euo pipefail

allowlist=(
    "crates/linsync-core/src/text.rs"
    "crates/linsync-core/src/folder.rs"
    "crates/linsync-core/src/binary.rs"
    "crates/linsync-core/src/table.rs"
    "crates/linsync-core/src/image.rs"
    "crates/linsync-core/src/document.rs"
    "crates/linsync-core/src/webpage.rs"
    "crates/linsync-core/src/merge.rs"
    "crates/linsync-core/src/filter.rs"
    "crates/linsync-core/src/plugin.rs"
    "crates/linsync-core/src/archive_write.rs"
    "apps/linsync-gui/qml/Main.qml"
    "apps/linsync-gui/qml/ImageComparePage.qml"
    "apps/linsync-gui/qml/DocumentComparePage.qml"
    "apps/linsync-gui/qml/WebpageComparePage.qml"
    "apps/linsync-gui/qml/MergePage.qml"
    "apps/linsync-gui/qml/FiltersPage.qml"
    "apps/linsync-gui/qml/PluginsPage.qml"
    "apps/linsync-gui/qml/SessionsPage.qml"
    "apps/linsync-gui/qml/SettingsPage.qml"
)

base_ref=""
if [[ -n "${GITHUB_BASE_REF:-}" ]]; then
    if [[ "${GITHUB_BASE_REF}" == origin/* ]]; then
        base_ref="${GITHUB_BASE_REF}"
    else
        base_ref="origin/${GITHUB_BASE_REF}"
    fi
elif git rev-parse --verify origin/master &>/dev/null; then
    base_ref="origin/master"
else
    base_ref="HEAD~1"
fi

merge_base="${base_ref}"
if git merge-base --is-ancestor "${merge_base}" HEAD 2>/dev/null; then
    merge_base="$(git merge-base "${merge_base}" HEAD)"
fi

mapfile -t changed_files < <(git diff --name-only "${merge_base}..HEAD")

changed_allowlist=()
for path in "${changed_files[@]}"; do
    for allowed in "${allowlist[@]}"; do
        if [[ "${path}" == "${allowed}" ]]; then
            changed_allowlist+=("${path}")
            break
        fi
    done
done

if [[ ${#changed_allowlist[@]} -eq 0 ]]; then
    exit 0
fi

docs_changed=false
for path in "${changed_files[@]}"; do
    if [[ "${path}" == "docs/feature-matrix.md" ]] || [[ "${path}" == "docs/known-limitations-1.0.md" ]]; then
        docs_changed=true
        break
    fi
done

bypassed=false
if [[ -n "${PR_TITLE:-}" ]] && [[ "${PR_TITLE}" == *"[docs-drift-ok]"* ]]; then
    bypassed=true
fi
if [[ "${bypassed}" != true ]]; then
    head_body="$(git log -1 --pretty=format:%b)"
    if [[ "${head_body}" == *"[docs-drift-ok]"* ]]; then
        bypassed=true
    fi
fi

if [[ "${bypassed}" == true ]]; then
    echo "docs-drift: bypass marker found; skipping check."
    exit 0
fi

if [[ "${docs_changed}" == true ]]; then
    exit 0
fi

echo "::warning title=Docs drift check::User-facing code changed without updating docs/feature-matrix.md or docs/known-limitations-1.0.md. Include '[docs-drift-ok]' in the PR title to bypass, or update the docs."
echo ""
echo "Changed files:"
for f in "${changed_allowlist[@]}"; do
    echo "  - ${f}"
done
echo ""
echo "If the matrix and known-limitations already cover these changes, include"
echo "[docs-drift-ok] in the PR title or the HEAD commit body to bypass this warning."

if [[ "${LINSYNC_REQUIRE_DOCS_DRIFT_CHECK:-0}" == "1" ]]; then
    exit 1
fi

exit 0
