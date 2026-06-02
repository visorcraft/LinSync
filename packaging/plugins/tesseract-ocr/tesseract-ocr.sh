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
want_positions = bool(options.get("want_positions", False))

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

# When the caller wants per-word boxes, also emit Tesseract's TSV so we can
# parse word geometry alongside the plain text transcript.
configs = ["txt"]
if want_positions:
    configs.append("tsv")

try:
    result = subprocess.run(
        ["tesseract", src, out_base, "-l", language, *configs],
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

output = {
    "role": role,
    "kind": "text",
    "path": out_txt,
    "encoding": "utf-8",
    "line_ending": "lf",
}

# Parse Tesseract's TSV into per-line word bounding boxes when requested. The
# TSV columns are: level page block par line word left top width height conf text.
# Words are level 5; we group them into per-line arrays. This assumes Tesseract
# emits rows in reading order (it does); a (block, par, line) tuple seen earlier
# keeps its array slot. Confidence parsing is best-effort — a non-numeric value
# is dropped (field omitted) rather than failing the whole extraction.
if want_positions:
    tsv_path = out_base + ".tsv"
    positions = []
    line_index = {}
    try:
        with open(tsv_path, encoding="utf-8") as fh:
            fh.readline()  # skip the header row
            for row in fh:
                cols = row.rstrip("\n").split("\t")
                if len(cols) < 12 or cols[0] != "5":
                    continue
                text = cols[11]
                if not text.strip():
                    continue
                key = (cols[2], cols[3], cols[4])  # block, par, line
                if key not in line_index:
                    line_index[key] = len(positions)
                    positions.append([])
                li = line_index[key]
                word = {
                    "text": text,
                    "line": li,
                    "x": int(cols[6]),
                    "y": int(cols[7]),
                    "width": int(cols[8]),
                    "height": int(cols[9]),
                }
                try:
                    conf = int(round(float(cols[10])))
                    if conf >= 0:
                        word["confidence"] = conf
                except ValueError:
                    pass
                positions[li].append(word)
        if positions:
            output["word_positions"] = positions
    except OSError:
        pass  # positions are best-effort; fall back to text only

print(json.dumps({
    "protocol_version": 1,
    "request_id": request_id,
    "status": "ok",
    "outputs": [output],
    "diagnostics": []
}))
PY
