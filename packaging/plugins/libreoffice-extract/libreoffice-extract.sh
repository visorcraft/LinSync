#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 VisorCraft LLC
# SPDX-License-Identifier: GPL-3.0-only
#
# LinSync unpacker / document_text_extractor plugin using LibreOffice headless.
#
# Protocol: receives one JSON `unpack_text` request on stdin.
# Emits one JSON PluginOperationResponse on stdout.
# Requires: libreoffice on PATH.
#
# Note: LibreOffice startup can take 1-3 seconds on cold systems.
# The host sets a 30 s timeout by default; increase via PluginExecutionOptions
# if needed for very large documents.

set -euo pipefail

# Defensive cleanup: if this script is killed (host timeout, cancellation,
# parent crash) make sure any soffice / oosplash grandchildren we may have
# spawned die with us. The host kills the process group too, but a bash
# trap is cheap insurance against process-group misses (no-op when LO
# already exited cleanly).
cleanup_lo() {
    pkill -KILL -P $$ -f 'soffice.bin|oosplash' 2>/dev/null || true
}
trap cleanup_lo EXIT INT TERM

if ! command -v libreoffice > "$LINSYNC_PLUGIN_TEMP_DIR/.check" 2>&1; then
    cat <<'JSON'
{"protocol_version":1,"request_id":"unknown","status":"error","error":{"code":"binary-not-found","message":"libreoffice not found — install libreoffice"},"diagnostics":[]}
JSON
    exit 1
fi

REQUEST=$(cat)

python3 - "$REQUEST" "$LINSYNC_PLUGIN_TEMP_DIR" <<'PY'
import sys, json, os, subprocess, glob

raw = sys.argv[1]
tmp = sys.argv[2]

try:
    req = json.loads(raw)
except json.JSONDecodeError as e:
    print(json.dumps({
        "protocol_version": 1, "request_id": "unknown",
        "status": "error",
        "error": {"code": "internal-error", "message": f"invalid JSON request: {e}"},
        "diagnostics": []
    }))
    sys.exit(0)

request_id = req.get("request_id", "unknown")
inputs = req.get("inputs", [])
if not inputs:
    print(json.dumps({
        "protocol_version": 1, "request_id": request_id,
        "status": "error",
        "error": {"code": "unsupported-input", "message": "no inputs provided"},
        "diagnostics": []
    }))
    sys.exit(0)

src = inputs[0].get("path", "")
role = inputs[0].get("role", "left")

if not os.path.isfile(src):
    print(json.dumps({
        "protocol_version": 1, "request_id": request_id,
        "status": "error",
        "error": {"code": "unsupported-input", "message": f"source not found: {src}"},
        "diagnostics": []
    }))
    sys.exit(0)

try:
    result = subprocess.run(
        [
            "libreoffice", "--headless", "--norestore",
            "--convert-to", "txt:Text",
            "--outdir", tmp,
            src
        ],
        capture_output=True, timeout=120
    )
except subprocess.TimeoutExpired:
    print(json.dumps({
        "protocol_version": 1, "request_id": request_id,
        "status": "error",
        "error": {"code": "internal-error", "message": "libreoffice timed out"},
        "diagnostics": []
    }))
    sys.exit(0)

if result.returncode != 0:
    stderr_text = result.stderr.decode("utf-8", errors="replace").strip()
    print(json.dumps({
        "protocol_version": 1, "request_id": request_id,
        "status": "error",
        "error": {"code": "internal-error",
                  "message": f"libreoffice failed (exit {result.returncode}): {stderr_text}"},
        "diagnostics": []
    }))
    sys.exit(0)

# LibreOffice writes <basename>.txt into the outdir
basename = os.path.splitext(os.path.basename(src))[0]
out_txt = os.path.join(tmp, basename + ".txt")
if not os.path.isfile(out_txt):
    # Some LO versions produce different stems; try globbing
    candidates = glob.glob(os.path.join(tmp, "*.txt"))
    if candidates:
        out_txt = candidates[0]
    else:
        print(json.dumps({
            "protocol_version": 1, "request_id": request_id,
            "status": "error",
            "error": {"code": "internal-error",
                      "message": f"libreoffice did not produce a .txt file in {tmp}"},
            "diagnostics": []
        }))
        sys.exit(0)

print(json.dumps({
    "protocol_version": 1,
    "request_id": request_id,
    "status": "ok",
    "outputs": [{
        "role": role,
        "kind": "text",
        "path": out_txt,
        "encoding": "utf-8",
        "line_ending": "lf"
    }],
    "diagnostics": []
}))
PY
