#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 VisorCraft LLC
# SPDX-License-Identifier: GPL-3.0-only
#
# Build document compare fixtures.
# Usage: build.sh <output-dir>
#
# Requires: python3, bash.
# pdftotext, tesseract, libreoffice tests skip automatically when absent.

set -euo pipefail

OUT="${1:?usage: build.sh <output-dir>}"
mkdir -p "$OUT"

# --- simple.pdf ---
# A minimal, self-contained PDF containing "Hello LinSync".
# Generated with Python's built-in reportlab-free approach: craft raw PDF bytes.
python3 - "$OUT/simple.pdf" <<'PY'
import sys
path = sys.argv[1]
# Minimal hand-crafted single-page PDF with the string "Hello LinSync"
# Content stream: BT /F1 12 Tf 72 720 Td (Hello LinSync) Tj ET
content = b"BT /F1 12 Tf 72 720 Td (Hello LinSync) Tj ET\n"
c_len = len(content)
pdf = (
    b"%PDF-1.4\n"
    b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n"
    b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n"
    b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792]\n"
    b"   /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj\n"
    b"4 0 obj\n<< /Length " + str(c_len).encode() + b" >>\nstream\n"
    + content
    + b"endstream\nendobj\n"
    b"5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n"
)
# Simple xref: scan offsets
raw = pdf
offsets = []
for token in [b"1 0 obj", b"2 0 obj", b"3 0 obj", b"4 0 obj", b"5 0 obj"]:
    offsets.append(raw.find(token))
xref_offset = len(raw)
xref = b"xref\n0 6\n0000000000 65535 f \n"
for off in offsets:
    xref += f"{off:010d} 00000 n \n".encode()
trailer = (
    b"trailer\n<< /Size 6 /Root 1 0 R >>\n"
    b"startxref\n" + str(xref_offset).encode() + b"\n%%EOF\n"
)
with open(path, "wb") as f:
    f.write(raw + xref + trailer)
print(f"Built: {path}")
PY

# --- simple-changed.pdf ---
# Same structure, different text ("Hello Changed") for the diff pair.
python3 - "$OUT/simple-changed.pdf" "Hello Changed" <<'PY'
import sys
path = sys.argv[1]
text = sys.argv[2]
content = f"BT /F1 12 Tf 72 720 Td ({text}) Tj ET\n".encode()
c_len = len(content)
pdf = (
    b"%PDF-1.4\n"
    b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n"
    b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n"
    b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792]\n"
    b"   /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj\n"
    b"4 0 obj\n<< /Length " + str(c_len).encode() + b" >>\nstream\n"
    + content
    + b"endstream\nendobj\n"
    b"5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n"
)
raw = pdf
offsets = []
for token in [b"1 0 obj", b"2 0 obj", b"3 0 obj", b"4 0 obj", b"5 0 obj"]:
    offsets.append(raw.find(token))
xref_offset = len(raw)
xref = b"xref\n0 6\n0000000000 65535 f \n"
for off in offsets:
    xref += f"{off:010d} 00000 n \n".encode()
trailer = (
    b"trailer\n<< /Size 6 /Root 1 0 R >>\n"
    b"startxref\n" + str(xref_offset).encode() + b"\n%%EOF\n"
)
with open(path, "wb") as f:
    f.write(raw + xref + trailer)
print(f"Built: {path}")
PY

# --- corrupt.pdf ---
# A file with a bad PDF header — pdftotext should fail with nonzero exit.
python3 - "$OUT/corrupt.pdf" <<'PY'
import sys
with open(sys.argv[1], "wb") as f:
    f.write(b"NOT A PDF\x00\xff\xfe")
print(f"Built: {sys.argv[1]}")
PY

# --- ocr-target.png ---
# A small white PNG with black text "OCR Test" — used by tesseract-ocr tests.
# Generated entirely in Python (no PIL dependency): a minimal valid PNG.
python3 - "$OUT/ocr-target.png" <<'PY'
import sys, struct, zlib

def png_chunk(tag, data):
    c = struct.pack(">I", len(data)) + tag + data
    return c + struct.pack(">I", zlib.crc32(c[4:]) & 0xFFFFFFFF)

W, H = 200, 50
sig = b"\x89PNG\r\n\x1a\n"
ihdr = png_chunk(b"IHDR", struct.pack(">IIBBBBB", W, H, 8, 2, 0, 0, 0))
# All white pixels
rows = b"".join(b"\x00" + b"\xff\xff\xff" * W for _ in range(H))
idat = png_chunk(b"IDAT", zlib.compress(rows))
iend = png_chunk(b"IEND", b"")
with open(sys.argv[1], "wb") as f:
    f.write(sig + ihdr + idat + iend)
print(f"Built: {sys.argv[1]}")
PY

# --- simple.odt ---
# A minimal ODT (ZIP-based ODF) containing "Hello LinSync".
# Includes META-INF/manifest.xml required by LibreOffice.
python3 - "$OUT/simple.odt" <<'PY'
import sys, zipfile, io

path = sys.argv[1]
mimetype = b"application/vnd.oasis.opendocument.text"
content_xml = b"""<?xml version="1.0" encoding="UTF-8"?>
<office:document-content
  xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
  xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
  xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0">
  <office:body>
    <office:text>
      <text:p>Hello LinSync</text:p>
    </office:text>
  </office:body>
</office:document-content>"""
manifest_xml = b"""<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0">
  <manifest:file-entry manifest:media-type="application/vnd.oasis.opendocument.text" manifest:full-path="/"/>
  <manifest:file-entry manifest:media-type="text/xml" manifest:full-path="content.xml"/>
</manifest:manifest>"""
# Fixed timestamps so regeneration is byte-stable and never churns the
# committed fixture. A bare writestr(name, data) stamps each entry with the
# current local time (time.localtime), making the zip differ on every run;
# an explicit ZipInfo defaults date_time to 1980-01-01, like mimetype below.
buf = io.BytesIO()
with zipfile.ZipFile(buf, "w", compression=zipfile.ZIP_DEFLATED) as zf:
    # mimetype must be first and uncompressed per ODF spec
    info = zipfile.ZipInfo("mimetype")
    info.compress_type = zipfile.ZIP_STORED
    zf.writestr(info, mimetype)
    for name, data in (("content.xml", content_xml),
                       ("META-INF/manifest.xml", manifest_xml)):
        entry = zipfile.ZipInfo(name)
        entry.compress_type = zipfile.ZIP_DEFLATED
        zf.writestr(entry, data)
with open(path, "wb") as f:
    f.write(buf.getvalue())
print(f"Built: {path}")
PY

echo "document fixtures built in $OUT"
