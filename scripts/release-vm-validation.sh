#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 VisorCraft LLC
# SPDX-License-Identifier: GPL-3.0-only
#
# Clean-VM packaging validation — pre-tag release gate.
#
# Installs each built artifact inside a fresh podman container, confirms
# `linsync --version` exits 0, and runs a 10-second offscreen GUI launch.
# Exits 0 only when every distro that has an artifact present passes.
#
# Usage:
#   VERSION=1.9.4 bash scripts/release-vm-validation.sh
#
# The artifacts are expected under the repo root in:
#   dist/linsync-${VERSION}-1-x86_64.pkg.tar.zst  (Arch)
#   dist/linsync_${VERSION}-1_amd64.deb              (Debian trixie / Ubuntu 24.04)
#   dist/linsync-${VERSION}-1.x86_64.rpm           (Fedora)
#
# See docs/known-limitations-1.0.md §"Packaging and release validation".
set -euo pipefail

VERSION="${VERSION:-1.9.4}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="${DIST_DIR:-${REPO_ROOT}/dist}"

# ── colour helpers ─────────────────────────────────────────────────────────────
if [[ -t 1 ]]; then
  RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; RESET='\033[0m'
else
  RED=''; GREEN=''; YELLOW=''; RESET=''
fi

pass()  { echo -e "${GREEN}[PASS]${RESET}  $*"; }
fail()  { echo -e "${RED}[FAIL]${RESET}  $*"; }
warn()  { echo -e "${YELLOW}[SKIP]${RESET}  $*"; }
info()  { echo "       $*"; }

# ── prereq check ──────────────────────────────────────────────────────────────
if ! command -v podman >/dev/null 2>&1; then
  echo "ERROR: podman is not installed. Install it first:" >&2
  echo "  pacman -S podman          # Arch / CachyOS" >&2
  echo "  apt install podman        # Debian / Ubuntu" >&2
  echo "  dnf install podman        # Fedora" >&2
  exit 2
fi

echo "=== LinSync clean-VM packaging validation (VERSION=${VERSION}) ==="
echo "    Artifact dir: ${DIST_DIR}"
echo

# ── distro definitions ────────────────────────────────────────────────────────
# Each entry: "name|image|artifact_glob|install_cmd"
# The install_cmd receives the *basename* of the artifact; the file is
# bind-mounted at /tmp/artifact/<basename> inside the container.
declare -a DISTROS=(
  "Arch|docker.io/archlinux:latest|linsync-${VERSION}-1-x86_64.pkg.tar.zst|pacman -Sy --noconfirm && pacman -U --noconfirm /tmp/artifact/linsync-${VERSION}-1-x86_64.pkg.tar.zst"
  "Debian-trixie|docker.io/debian:trixie-slim|linsync_${VERSION}-1_amd64.deb|DEBIAN_FRONTEND=noninteractive apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y /tmp/artifact/linsync_${VERSION}-1_amd64.deb"
  "Fedora|docker.io/fedora:latest|linsync-${VERSION}-1.x86_64.rpm|dnf install -y /tmp/artifact/linsync-${VERSION}-1.x86_64.rpm"
  "Ubuntu-24.04|docker.io/ubuntu:24.04|linsync_${VERSION}-1_amd64.deb|DEBIAN_FRONTEND=noninteractive apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y /tmp/artifact/linsync_${VERSION}-1_amd64.deb"
)

PASSED=0
FAILED=0
SKIPPED=0

# ── per-distro validation ──────────────────────────────────────────────────────
run_distro() {
  local name="$1"
  local image="$2"
  local artifact_name="$3"
  local install_cmd="$4"

  local artifact_path="${DIST_DIR}/${artifact_name}"

  echo "--- ${name} ---"

  # Skip if artifact is missing.
  if [[ ! -f "${artifact_path}" ]]; then
    warn "${name}: artifact not found: ${artifact_path} — skipping"
    SKIPPED=$(( SKIPPED + 1 ))
    return
  fi

  # Pull the container image; continue with remaining distros on failure.
  if ! podman pull --quiet "${image}" 2>/dev/null; then
    warn "${name}: could not pull image '${image}' (network issue?) — skipping"
    SKIPPED=$(( SKIPPED + 1 ))
    return
  fi

  # Build a minimal container script.
  local container_script
  container_script="$(cat <<SCRIPT
set -euo pipefail

# Update package index where needed (apt-based only).
if command -v apt-get >/dev/null 2>&1; then
  apt-get update -qq
fi

# Install the artifact.
${install_cmd}

# Verify the binary is on PATH and --version works.
if ! linsync --version; then
  echo "ERROR: linsync --version failed" >&2
  exit 1
fi

# 10-second offscreen GUI launch (timeout 124 = normal for GUI; 0 = clean exit).
export QT_QPA_PLATFORM=offscreen
export XDG_RUNTIME_DIR=/tmp/xdg-run
mkdir -p "\${XDG_RUNTIME_DIR}"
launch_exit=0
timeout 10 linsync || launch_exit=\$?
if [[ "\${launch_exit}" -eq 0 || "\${launch_exit}" -eq 124 ]]; then
  echo "offscreen launch OK (exit \${launch_exit})"
else
  echo "ERROR: linsync offscreen launch exited with \${launch_exit}" >&2
  exit 1
fi
SCRIPT
)"

  local rc=0
  podman run --rm \
    --volume "${artifact_path}:/tmp/artifact/${artifact_name}:ro,z" \
    --env QT_QPA_PLATFORM=offscreen \
    "${image}" \
    bash -c "${container_script}" \
    2>&1 | sed "s/^/    [${name}] /" \
    || rc=$?

  if [[ "${rc}" -eq 0 ]]; then
    pass "${name}: install + offscreen launch"
    PASSED=$(( PASSED + 1 ))
  else
    fail "${name}: validation failed (exit ${rc})"
    FAILED=$(( FAILED + 1 ))
  fi
  echo
}

for entry in "${DISTROS[@]}"; do
  IFS='|' read -r _name _image _artifact _install_cmd <<< "${entry}"
  run_distro "${_name}" "${_image}" "${_artifact}" "${_install_cmd}"
done

# ── summary ───────────────────────────────────────────────────────────────────
echo "=== Summary ==="
echo "  Passed:  ${PASSED}"
echo "  Failed:  ${FAILED}"
echo "  Skipped: ${SKIPPED}"
echo

if [[ "${PASSED}" -eq 0 && "${FAILED}" -eq 0 ]]; then
  echo "No artifacts were found in ${DIST_DIR}."
  echo "Build the packages first:"
  echo "  just package-arch   # → dist/*.pkg.tar.zst"
  echo "  just package-deb    # → dist/*.deb"
  echo "  just package-rpm    # → dist/*.rpm"
  exit 2
fi

if [[ "${FAILED}" -gt 0 ]]; then
  fail "Release validation FAILED — do not tag."
  exit 1
fi

pass "All present artifacts validated. Safe to tag ${VERSION}."
exit 0
