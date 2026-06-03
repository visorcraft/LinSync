set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

# Format every crate.
fmt:
    cargo fmt --all

# Check formatting without mutating the worktree. The `ci` recipe uses
# this so a local pre-push doesn't silently rewrite files.
fmt-check:
    cargo fmt --all -- --check

# Cargo build (debug) for the whole workspace.
build:
    cargo build --workspace

# Cargo build (release) for the whole workspace.
build-release:
    cargo build --workspace --release

# Run all tests.
#
# We set LINSYNC_SANDBOX_SKIP=1 (matching the standard CI test job) so that
# `just ci` and `just test` are reliable and non-flaky across environments:
# containers, GitHub runners, and dev setups (including this one) where
# Landlock ABI >=1 may be reported by the probe but actual filesystem
# read restriction does not take effect (e.g. due to user namespaces,
# container LSM policy, or tmpfs/overlay specifics). The sandbox_integration
# tests that assert real EACCES denial are still exercised on kernels+envs
# where they work by running the specific test without the var:
#
#   env -u LINSYNC_SANDBOX_SKIP cargo test -p linsync-sandbox --test sandbox_integration
#
# See crates/linsync-sandbox/tests/sandbox_integration.rs and the comment
# in .github/workflows/ci.yml for the full contract.
test:
    LINSYNC_SANDBOX_SKIP=1 cargo test --workspace

# Type-check without producing binaries.
check:
    cargo check --workspace

# Strict lint pass.
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# License/advisory check (requires `cargo install cargo-deny`).
deny:
    cargo deny --all-features check

# Advisory check (requires `cargo install cargo-audit`).
audit:
    cargo audit

# Regenerate (or print) the authoritative third-party crate/license table for
# the shipped feature set (cxxqt + cxxqt-app + web-engine on the Linux target,
# build deps included, dev deps excluded). The single generator
# (scripts/generate-credits.py) also maintains docs/third-party-crates.json
# (the canonical machine-readable list) and refreshes the generated blocks
# inside the three credit surfaces.
#
# `just credits` prints the table (for review or copy).
# `just credits-update` writes the JSON and patches the surfaces so that
# future dep changes only require running the update target (drift for the
# data parts becomes impossible by construction).
credits:
    python3 scripts/generate-credits.py table

# Update the committed JSON and the generated sections of the three surfaces
# (md table, CreditsPage crates array, LicensesPage counts+table). Run this
# after any Cargo.lock change that affects the shipped graph, then commit.
credits-update:
    python3 scripts/generate-credits.py update

# Build the release binaries and bundle a self-contained AppImage via
# linuxdeploy + linuxdeploy-plugin-qt. Falls back to staging the AppDir when
# linuxdeploy isn't on PATH.
#
# On hosts with very new glibc/toolchain (relr.dyn sections) the strip
# binary bundled inside linuxdeploy's AppImage can fail to recognise some
# staged libs; set NO_STRIP=1 to skip (AppDir is still produced and
# functional; the final AppImage can be manually compressed if needed).
# We default to NO_STRIP=1 here for broad host compatibility (the
# resulting artifacts are still valid for smoke/testing).
package:
    NO_STRIP=1 bash packaging/appimage/build-appdir.sh

# Build the Arch / CachyOS pacman package via makepkg (pacman-based hosts only).
package-arch:
    cd packaging/arch && makepkg -sf

# Build the Debian / Ubuntu .deb package via dpkg-buildpackage (Debian-based hosts only).
package-deb:
    dpkg-buildpackage -us -uc -b

# Links against the host's Qt. On non-Fedora hosts the resulting RPM
# will fail to install on Fedora because Qt's AOT QML output binds to
# private symbols that are pinned to the Qt minor version - use
# `package-rpm-fedora44` from any host to get a Fedora-installable RPM.
# Build the Fedora / RHEL .rpm via on-host rpmbuild (RPM-based hosts only).
package-rpm:
    bash -c 'set -euo pipefail; \
        cd packaging/rpm; \
        version=$(awk -F'"'"'"'"'"' '"'"'/^\[workspace\.package\]/ {s=1; next} s && /^\[/ {exit} s && $1 ~ /^version[[:space:]]*=/ {print $2; exit}'"'"' ../../Cargo.toml); \
        ( cd ../.. && git archive --format=tar.gz --prefix=linsync-${version}/ --output=packaging/rpm/linsync-${version}.tar.gz HEAD ); \
        rpmbuild --define "_topdir $(pwd)/_rpmbuild" --define "_sourcedir $(pwd)" -bb linsync.spec'

# The container path matters because cxx-qt-build's QML AOT compiler
# links against Qt's private API, which is pinned to the exact Qt
# minor version. An RPM built on Arch / CachyOS (Qt 6.11) fails to
# install on Fedora 44 (Qt 6.9). See packaging/rpm/Containerfile.fedora44.
# Build the Fedora 44 .rpm inside a podman container (Qt 6.9 matched).
package-rpm-fedora44:
    bash packaging/rpm/build-in-container.sh

# Run the CLI with arbitrary arguments. Example: `just run-cli compare a.txt b.txt`.
run-cli *args:
    cargo run -p linsync-cli -- {{args}}

# Launch the GUI (Qt 6 / Kirigami shell with an eight-section layout:
# Compare, Sessions, Filters, Plugins, Settings, About in the sidebar;
# Credits and Licenses reachable from About). The default build spawns
# an external `qml6`/`qml` runner; pass `--features cxxqt-app` for the
# in-process cxx-qt host the packaged builds use.
run-gui:
    cargo run -p linsync

# Vendor every cargo dependency into target/flatpak/vendor. Required
# before `just flatpak` so the sandboxed build can compile with no
# network access. Re-runs are cheap (cargo only re-downloads crates
# that aren't already in vendor/). Note: this does NOT touch
# .cargo/config.toml — `just flatpak` writes / removes it around the
# build so host cargo builds aren't redirected at the vendor tree.
flatpak-vendor:
    mkdir -p target/flatpak
    cargo vendor --locked target/flatpak/vendor > target/flatpak/vendor-config.toml
    @echo "wrote target/flatpak/vendor + target/flatpak/vendor-config.toml"

# Build the Flatpak into a local OSTree repo at target/flatpak/repo and
# the install tree at target/flatpak/build. Requires:
#   flatpak-builder
#   org.kde.Platform//6.10
#   org.kde.Sdk//6.10
#   org.freedesktop.Sdk.Extension.rust-stable//25.08
flatpak: flatpak-vendor
    # Stage the vendor config at .cargo/config.toml just long enough
    # for flatpak-builder to copy the source tree into the sandbox,
    # then restore the dev config (mold+sccache wiring) so subsequent
    # host cargo builds use the registry cache + accelerated linker
    # again. Without the backup/restore the dev config would be
    # silently deleted on every flatpak build. The trap also fires
    # on failure to keep the worktree clean.
    mkdir -p .cargo
    if [ -f .cargo/config.toml ]; then mv .cargo/config.toml .cargo/config.toml.bak; fi
    cp target/flatpak/vendor-config.toml .cargo/config.toml
    trap 'if [ -f .cargo/config.toml.bak ]; then mv .cargo/config.toml.bak .cargo/config.toml; else rm -f .cargo/config.toml; fi' EXIT INT TERM; \
        flatpak-builder --user --force-clean --disable-rofiles-fuse \
            --repo=target/flatpak/repo \
            target/flatpak/build \
            packaging/flatpak/com.visorcraft.LinSync.yml
    @echo "built target/flatpak/repo"

# Bundle the Flatpak repo into a single redistributable .flatpak file
# at target/release/linsync.flatpak.
flatpak-bundle: flatpak
    mkdir -p target/release
    flatpak build-bundle target/flatpak/repo \
        target/release/linsync.flatpak \
        com.visorcraft.LinSync master
    @echo "wrote target/release/linsync.flatpak"

# Convenience target for local CI. Uses `fmt-check` (not `fmt`) so
# the worktree isn't silently reformatted as a side effect.
ci: fmt-check lint test
    @echo "ci preflight passed"

# Validate release metadata and packaging stubs.
release-smoke:
    bash scripts/release-smoke.sh

# Extract translatable qsTr() strings from the QML into every i18n catalog
# (the source-language baseline is apps/linsync-gui/i18n/linsync_en.ts).
# Requires Qt's lupdate from qt6-tools. Translators copy the baseline to
# linsync_<lang>.ts and translate; rerunning refreshes existing catalogs.
l10n-update:
    lupdate=$(command -v lupdate6 || command -v lupdate-qt6 || command -v lupdate || echo /usr/lib/qt6/bin/lupdate); \
    for ts in apps/linsync-gui/i18n/*.ts; do "$lupdate" apps/linsync-gui/qml -ts "$ts"; done

# Compile every i18n/*.ts catalog to a runtime .qm (one per locale).
# Requires Qt's lrelease from qt6-tools.
l10n-release:
    lrelease=$(command -v lrelease6 || command -v lrelease-qt6 || command -v lrelease || echo /usr/lib/qt6/bin/lrelease); \
    for ts in apps/linsync-gui/i18n/*.ts; do "$lrelease" "$ts"; done
