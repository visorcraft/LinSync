#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 VisorCraft LLC
# SPDX-License-Identifier: GPL-3.0-only
#
# LinSync unpacker / ocr_engine plugin using Tesseract.
#
# Protocol: receives one JSON `unpack_text` request on stdin.
# Emits one JSON PluginOperationResponse on stdout.
# Requires: tesseract on PATH.

set -euo pipefail

if ! command -v tesseract > "$LINSYNC_PLUGIN_TEMP_DIR/.check" 2>&1; then
    cat <<'JSON'
{"protocol_version":1,"request_id":"unknown","status":"error","error":{"code":"binary-not-found","message":"tesseract not found — install tesseract-ocr"},"diagnostics":[]}
JSON
    exit 1
fi

REQUEST=$(cat)

python3 - "$REQUEST" "$LINSYNC_PLUGIN_TEMP_DIR" <<'PY'
import sys, json, os, subprocess

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
options = req.get("options", {})
language = options.get("language", "eng") or "eng"

if not os.path.isfile(src):
    print(json.dumps({
        "protocol_version": 1, "request_id": request_id,
        "status": "error",
        "error": {"code": "unsupported-input", "message": f"source not found: {src}"},
        "diagnostics": []
    }))
    sys.exit(0)

# tesseract writes to <prefix>.txt; we give it a path inside tmp
out_base = os.path.join(tmp, "ocr-output")
out_txt = out_base + ".txt"

try:
    result = subprocess.run(
        ["tesseract", src, out_base, "-l", language, "txt"],
        capture_output=True, timeout=120
    )
except subprocess.TimeoutExpired:
    print(json.dumps({
        "protocol_version": 1, "request_id": request_id,
        "status": "error",
        "error": {"code": "internal-error", "message": "tesseract timed out"},
        "diagnostics": []
    }))
    sys.exit(0)

if result.returncode != 0:
    stderr_text = result.stderr.decode("utf-8", errors="replace").strip()
    print(json.dumps({
        "protocol_version": 1, "request_id": request_id,
        "status": "error",
        "error": {"code": "internal-error",
                  "message": f"tesseract failed (exit {result.returncode}): {stderr_text}"},
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
