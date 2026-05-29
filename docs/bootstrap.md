# Developer Bootstrap

LinSync currently builds as a Rust workspace with a QML/Kirigami shell skeleton.
The `linsync` binary locates `apps/linsync-gui/qml/Main.qml` in source builds or
`share/linsync/qml/Main.qml` in packaged builds, then launches it through `qml6`
or `qml`. Install the Qt 6 QML runtime and Kirigami QML modules before running
the GUI locally.

## Common

Required now:

- Rust stable with `rustfmt` and `clippy`
- `cargo`

Optional now:

- `appstreamcli`
- `desktop-file-validate`
- Qt 6 QML runtime, `qml6`, and Kirigami QML modules for the GUI shell
- Qt 6 development headers and `qmake6` for `cargo check -p linsync --features cxxqt-smoke` and `cargo check -p linsync --features cxxqt-app`
- `cargo-deny`
- `cargo-audit`
- `just`

## Fedora

```sh
sudo dnf install rust cargo appstream desktop-file-utils just
```

## Arch

```sh
sudo pacman -S rust appstream desktop-file-utils just
```

## Debian And Ubuntu

```sh
sudo apt install cargo rustc appstream desktop-file-utils just
```

## openSUSE

```sh
sudo zypper install rust cargo AppStream desktop-file-utils just
```

## Local Verification

```sh
cargo fmt --all
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
desktop-file-validate packaging/com.visorcraft.LinSync.desktop
appstreamcli validate --no-net packaging/com.visorcraft.LinSync.metainfo.xml
```
