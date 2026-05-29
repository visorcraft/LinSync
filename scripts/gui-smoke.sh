#!/usr/bin/env bash
set -euo pipefail

# --check-a11y: grep every Button/Switch/SpinBox/ComboBox/TextField in the QML
# tree and fail if any lacks both `text:` and `Accessible.name`.  This is a
# lightweight static check; it does not replace a manual screen-reader pass.
if [[ "${1:-}" == "--check-a11y" ]]; then
    echo "--- A11y grep check ---"
    QML_DIR="apps/linsync-gui/qml"
    FAIL=0
    # For each QML file, find blocks that open one of the target controls on a
    # single line (the pattern the formatter uses) and check whether the
    # surrounding context provides text: or Accessible.name.
    # Strategy: extract line numbers of control declarations, then look in the
    # surrounding 20-line window for either marker.
    # Exclude the App* wrapper files — they are abstract base classes whose
    # root element is the control itself; the Accessible.name is provided by
    # each callsite that instantiates them.
    while IFS=: read -r file lineno _; do
        # Skip the App* wrapper definitions (AppComboBox.qml, AppSpinBox.qml, etc.)
        basename_file="$(basename "${file}")"
        if [[ "${basename_file}" == App*.qml ]]; then
            continue
        fi
        # Grab a window of 20 lines starting at the control declaration.
        window=$(awk "NR>=${lineno} && NR<=${lineno}+20" "${file}" 2>/dev/null || true)
        if echo "${window}" | grep -qE 'text\s*:|Accessible\.name'; then
            :  # OK
        else
            echo "MISSING a11y label: ${file}:${lineno}"
            FAIL=1
        fi
    done < <(grep -nE '^\s*(Controls\.)?(Button|Switch|SpinBox|ComboBox|TextField)\s*\{' \
                  "${QML_DIR}"/*.qml 2>/dev/null || true)
    if [[ "${FAIL}" -eq 0 ]]; then
        echo "a11y grep OK"
        exit 0
    else
        exit 1
    fi
fi

if ! command -v qml6 >/dev/null 2>&1 && ! command -v qml >/dev/null 2>&1; then
  echo "Skipping GUI smoke: qml6/qml runner not found"
  exit 0
fi

tmpdata="$(mktemp -d)"
trap 'rm -rf "${tmpdata}"' EXIT

export QT_QPA_PLATFORM="${QT_QPA_PLATFORM:-offscreen}"
# Sandbox setup quirks in CI (containers without Landlock or bwrap) must
# not block the GUI smoke. Default LINSYNC_SANDBOX_SKIP=1 puts the
# plugin-helper sandbox into degraded mode (logs a WARN, runs unsandboxed).
# Override with `LINSYNC_SANDBOX_SKIP= bash scripts/gui-smoke.sh` to test
# real sandbox enforcement.
export LINSYNC_SANDBOX_SKIP="${LINSYNC_SANDBOX_SKIP-1}"
export XDG_CONFIG_HOME="${tmpdata}/config"
export XDG_DATA_HOME="${tmpdata}/data"
export XDG_CACHE_HOME="${tmpdata}/cache"
export XDG_STATE_HOME="${tmpdata}/state"

run_gui_smoke() {
  local code
  set +e
  timeout "${LINSYNC_GUI_SMOKE_TIMEOUT:-3s}" cargo run -q -p linsync -- "$@"
  code=$?
  set -e

  if [[ "${code}" -eq 0 || "${code}" -eq 124 ]]; then
    return 0
  fi

  return "${code}"
}

run_gui_smoke tests/fixtures/text/left.txt tests/fixtures/text/right.txt
run_gui_smoke tests/fixtures/folders/left tests/fixtures/folders/right

# Exercise the new sidebar-page bridge surfaces through the CLI so each
# section's backend is touched even when we cannot drive QML interactions.
echo "Exercising sidebar-page CLI surfaces"
cargo run -q -p linsync-cli -- filter validate "name: Smoke
wf:*.rs" || {
  echo "filter validate failed" >&2
  exit 1
}
cargo run -q -p linsync-cli -- filter list >/dev/null

# Plugin discovery (no-op when no plugins are installed) — just confirm exit 0/1.
set +e
cargo run -q -p linsync-cli -- folders --count \
  tests/fixtures/folders/left tests/fixtures/folders/right >/dev/null
code=$?
set -e
if [[ "${code}" -ne 0 && "${code}" -ne 1 ]]; then
  echo "folders smoke exited with unexpected code ${code}" >&2
  exit "${code}"
fi

# Settings round-trip smoke — exercises the bridge HTTP handler through its
# Rust unit test because the bridge runs on a random port + token that cannot
# be reached via a plain curl in this script.
echo "--- Settings round-trip smoke ---"
cargo test -q -p linsync bridge_settings_round_trip_through_core_store 2>&1 \
  | grep -v "^$" \
  | tail -5
echo "settings round-trip OK"

# Filters round-trip smoke — exercises walk-option and filter save/list through
# the test-support wrappers that mirror the /walk/set and /filters/* handlers.
echo "--- Filters round-trip smoke ---"
cargo test -q -p linsync --test filters_bridge --features test-support 2>&1 \
  | grep -v "^$" \
  | tail -8
echo "filters round-trip OK"

# Plugins round-trip smoke — exercises plugin-enable persistence and fixture
# discovery via the test-support wrappers that back the /plugins/list and
# /plugins/toggle bridge handlers.
echo "--- Plugins round-trip smoke ---"
cargo test -q -p linsync --test plugins_bridge --features test-support 2>&1 \
  | grep -v "^$" \
  | tail -8
echo "plugins round-trip OK"

# Plugin options smoke — exercises plugin-option persistence and the
# /plugins/options/get + /plugins/options/set bridge handlers.
echo "--- Plugin options smoke ---"
cargo test -q -p linsync bridge_plugin_options 2>&1 \
  | grep -v "^$" \
  | tail -8
echo "plugin options OK"

# Filters migrate round-trip smoke — exercises legacy .flt → LinSync translation
# through the unit test that backs the /filters/migrate bridge handler.
echo "--- Filters migrate round-trip smoke ---"
cargo test -q -p linsync bridge_filters_migrate 2>&1 \
  | grep -v "^$" \
  | tail -5
echo "filters migrate round-trip OK"

# Integration-level migrate smoke — exercises the core migrate_filter_text
# function via the test-support wrappers.
cargo test -q -p linsync --test filters_bridge --features test-support \
  migrate_filter 2>&1 \
  | grep -v "^$" \
  | tail -8
echo "filters migrate integration OK"

# Three-way merge smoke — exercises the merge3 bridge endpoints through their
# Rust unit tests: start → resolve → save round-trip.
echo "--- Three-way merge smoke ---"
cargo test -q -p linsync bridge_merge3 2>&1 \
  | grep -v "^$" \
  | tail -8
echo "three-way merge OK"

# Image compare smoke — exercises the /compare/image bridge handler through the
# test-support wrappers (identical to how filters/plugins are tested above).
echo "--- Image compare smoke ---"
cargo test -q -p linsync --test image_compare_bridge --features test-support 2>&1 \
  | grep -v "^$" \
  | tail -8
echo "image compare smoke OK"

# Image compare GUI section smoke — verify ImageComparePage is wired into Main.qml
# and the core image compare engine is exercised via the bridge test suite.
echo "--- Image compare GUI section smoke ---"
grep -q "ImageComparePage" apps/linsync-gui/qml/Main.qml \
  || { echo "ImageComparePage not found in Main.qml" >&2; exit 1; }
# Narrow to image_compare tests only — running the full linsync-core
# suite here re-executes plugin_archive_e2e which has a pre-existing
# parallel race on the shared zip-fixture file. The image-compare
# smoke doesn't need the full suite.
cargo test -q -p linsync-core --features image-compare --test image_compare 2>&1 \
  | grep -v "^$" \
  | tail -5
echo "image-compare GUI smoke OK"

# Document compare smoke — exercises the /compare/document bridge handler
# through the test-support wrappers (skips automatically when pdftotext absent).
echo "--- Document compare smoke ---"
cargo test -q -p linsync --test document_compare_bridge --features test-support 2>&1 \
  | grep -v "^$" \
  | tail -8
echo "document compare smoke OK"

# Document compare GUI section smoke — verify DocumentComparePage is wired into Main.qml.
echo "--- Document compare GUI section smoke ---"
grep -q "DocumentComparePage" apps/linsync-gui/qml/Main.qml \
  || { echo "DocumentComparePage not found in Main.qml" >&2; exit 1; }
echo "document-compare GUI smoke OK"

# Moved-block detection smoke — verifies detect_moves: true produces Moved
# blocks in the compare result and that the JSON serialisation round-trips.
echo "--- Moved-block detection smoke ---"
cargo test -q -p linsync-core --test integration moved_block_detection_smoke 2>&1 \
  | grep -v "^$" \
  | tail -5
echo "moved-block detection OK"

if [[ "${LINSYNC_GUI_SMOKE_CXXQT:-0}" == "1" ]]; then
  if ! command -v qmake6 >/dev/null 2>&1; then
    echo "Skipping cxx-qt GUI smoke: qmake6 not found"
    exit 0
  fi

  QT_VERSION_MAJOR=6 timeout "${LINSYNC_GUI_SMOKE_TIMEOUT:-3s}" \
    cargo run -q -p linsync --features cxxqt-app -- \
    tests/fixtures/text/left.txt tests/fixtures/text/right.txt || code=$?
  if [[ "${code:-0}" -ne 0 && "${code:-0}" -ne 124 ]]; then
    exit "${code}"
  fi

  code=0
  QT_VERSION_MAJOR=6 timeout "${LINSYNC_GUI_SMOKE_TIMEOUT:-3s}" \
    cargo run -q -p linsync --features cxxqt-app -- \
    tests/fixtures/folders/left tests/fixtures/folders/right || code=$?
  if [[ "${code:-0}" -ne 0 && "${code:-0}" -ne 124 ]]; then
    exit "${code}"
  fi
fi
