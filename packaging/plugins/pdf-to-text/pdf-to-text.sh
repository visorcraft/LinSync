#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 VisorCraft LLC
# SPDX-License-Identifier: GPL-3.0-only
#
# LinSync unpacker / document_text_extractor plugin for PDF files.
#
# Protocol: receives one JSON `unpack_text` request on stdin.
# Emits one JSON response (PluginOperationResponse) on stdout.
# All diagnostics go to stderr.
#
# Requires: pdftotext (from poppler-utils) on PATH.

set -euo pipefail

if ! command -v pdftotext > "$LINSYNC_PLUGIN_TEMP_DIR/.check" 2>&1; then
    cat <<'JSON'
{"protocol_version":1,"request_id":"unknown","status":"error","error":{"code":"binary-not-found","message":"pdftotext not found — install poppler-utils"},"diagnostics":[]}
JSON
    exit 1
fi

REQUEST=$(cat)

python3 - "$REQUEST" "$LINSYNC_PLUGIN_TEMP_DIR" <<'PY'
import sys, json, os, subprocess, tempfile

raw = sys.argv[1]
tmp = sys.argv[2]

try:
    req = json.loads(raw)
except json.JSONDecodeError as e:
    print(json.dumps({
        "protocol_version": 1,
        "request_id": "unknown",
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
# page_range not yet plumbed to pdftotext -f/-l; accepted and ignored in v1
_page_range = options.get("page_range", "all")

if not os.path.isfile(src):
    print(json.dumps({
        "protocol_version": 1, "request_id": request_id,
        "status": "error",
        "error": {"code": "unsupported-input", "message": f"source not found: {src}"},
        "diagnostics": []
    }))
    sys.exit(0)

out_path = os.path.join(tmp, "extracted.txt")
try:
    result = subprocess.run(
        ["pdftotext", src, out_path],
        capture_output=True, timeout=60
    )
except subprocess.TimeoutExpired:
    print(json.dumps({
        "protocol_version": 1, "request_id": request_id,
        "status": "error",
        "error": {"code": "internal-error", "message": "pdftotext timed out"},
        "diagnostics": []
    }))
    sys.exit(0)

if result.returncode != 0:
    stderr_text = result.stderr.decode("utf-8", errors="replace").strip()
    print(json.dumps({
        "protocol_version": 1, "request_id": request_id,
        "status": "error",
        "error": {"code": "internal-error", "message": f"pdftotext failed (exit {result.returncode}): {stderr_text}"},
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
        "path": out_path,
        "encoding": "utf-8",
        "line_ending": "lf"
    }],
    "diagnostics": []
}))
PY
