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
test:
    cargo test --workspace

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

# Build the release binaries and bundle a self-contained AppImage via
# linuxdeploy + linuxdeploy-plugin-qt (mirrors the Grexa flow). Falls
# back to staging the AppDir when linuxdeploy isn't on PATH.
package:
    bash packaging/appimage/build-appdir.sh

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
