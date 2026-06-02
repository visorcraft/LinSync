#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 VisorCraft LLC
# SPDX-License-Identifier: GPL-3.0-only
#
# LinSync pdf_renderer plugin: rasterize a PDF's pages to PNG images.
#
# Protocol: receives one JSON `render_pages` request on stdin:
#   {"op":"render_pages","source":"<path>"}
# Writes page-N.png files into $LINSYNC_PLUGIN_TEMP_DIR (the sandbox-writable
# scratch dir the host then copies out) and emits one JSON response on stdout:
#   {"ok":true,"pages":["<temp>/page-1.png", ...]}   (in page order)
#   {"ok":false,"error":"..."}
# Diagnostics go to stderr.
#
# Requires: pdftoppm (from poppler-utils) on PATH.

set -euo pipefail

# Redirect the probe into the sandbox-writable temp dir, not /dev/null:
# under the Landlock plugin sandbox /dev/null is not writable, so a
# `>/dev/null` redirect would fail and mask the real check.
if ! command -v pdftoppm > "$LINSYNC_PLUGIN_TEMP_DIR/.pdftoppm-check" 2>&1; then
    printf '{"ok":false,"error":"pdftoppm not found — install poppler-utils"}\n'
    exit 0
fi

REQUEST=$(cat)

python3 - "$REQUEST" "$LINSYNC_PLUGIN_TEMP_DIR" <<'PY'
import sys, json, os, subprocess, glob

raw, tmp = sys.argv[1], sys.argv[2]
try:
    req = json.loads(raw)
except json.JSONDecodeError as e:
    print(json.dumps({"ok": False, "error": f"invalid JSON request: {e}"}))
    sys.exit(0)

source = req.get("source")
if not source or not os.path.isfile(source):
    print(json.dumps({"ok": False, "error": f"source not found: {source!r}"}))
    sys.exit(0)

dpi = "150"
opts = req.get("options") or {}
if isinstance(opts.get("resolution_dpi"), int):
    dpi = str(opts["resolution_dpi"])

prefix = os.path.join(tmp, "page")
try:
    subprocess.run(
        ["pdftoppm", "-png", "-r", dpi, source, prefix],
        check=True, capture_output=True,
    )
except subprocess.CalledProcessError as e:
    msg = e.stderr.decode("utf-8", "replace").strip() or str(e)
    print(json.dumps({"ok": False, "error": f"pdftoppm failed: {msg}"}))
    sys.exit(0)

# pdftoppm names files like page-1.png, page-2.png (zero-padded for many pages);
# sort numerically by the trailing page number so the host gets page order.
def page_num(path):
    base = os.path.splitext(os.path.basename(path))[0]
    digits = "".join(ch for ch in base if ch.isdigit())
    return int(digits) if digits else 0

pages = sorted(glob.glob(prefix + "*.png"), key=page_num)
print(json.dumps({"ok": True, "pages": pages}))
PY
