# Contributing to LinSync

Thanks for taking the time to help LinSync. This document describes how
to propose a change, what we expect from a pull request, and the coding
standards that apply to the codebase.

If anything here is unclear or out of date, open an issue or a PR.

## Code of conduct

Be kind, be specific, assume good faith. Disagree about the technical
details, not the person. Public reviews stay focused on the diff.

## How to propose a change

LinSync uses a standard **fork → branch → pull request** workflow on
GitHub.

1. **Fork** [`visorcraft/linsync`](https://github.com/visorcraft/linsync)
   to your GitHub account.
2. **Clone** your fork and add the upstream remote:

   ```sh
   git clone git@github.com:<you>/linsync.git
   cd linsync
   git remote add upstream https://github.com/visorcraft/linsync.git
   ```

3. **Branch** from `master`. Pick a descriptive, kebab-case branch
   name: `fix-trash-symlink-deletion`, `feature/csv-header-mode`,
   `docs/contributing-update`.

   ```sh
   git fetch upstream
   git switch -c my-change upstream/master
   ```

4. **Make focused commits.** One logical change per commit. Run the
   preflight (see below) before pushing.
5. **Open a pull request** against `master` on `visorcraft/linsync`.
   Fill in the PR template:
   - **What.** One paragraph summary of the change.
   - **Why.** Bug fix? New feature? Doc fix? Link the issue if one
     exists.
   - **How to test.** The exact commands a reviewer should run.
   - **Risk.** What might break? What did you not test?

PRs that touch UI behavior should include a screenshot or a short
recording. PRs that touch packaging should include the output of the
relevant `just package*` command.

## Before you push: preflight

Run the local CI preflight. It mirrors what GitHub Actions runs:

```sh
just ci          # = just fmt + just lint + just test
just deny        # cargo-deny: license + advisory policy
just audit       # cargo-audit: known security advisories
```

GUI-touching changes also need:

```sh
bash scripts/gui-smoke.sh
LINSYNC_GUI_SMOKE_CXXQT=1 bash scripts/gui-smoke.sh   # if you touched cxx-qt
```

If a check fails, fix the root cause — don't `--no-verify`, don't
silence clippy lints with `#[allow(...)]` unless you justify it in the
PR description.

## What we look for in a review

- The change does one thing and does it well.
- Behavior changes ship with tests. New `linsync-core` API: a unit
  test. New CLI flag: an integration test under
  `crates/linsync-cli/tests/`. New bridge endpoint: a `bridge_*` test
  in `apps/linsync-gui/src/main.rs`.
- The change keeps `linsync-core` as the source of truth. The CLI
  and GUI are both clients of the core; do not re-implement
  compare/merge/filter/plugin/storage/trash logic inside the GUI or
  CLI binaries.
- Documentation is updated alongside the code:
  `docs/known-limitations-1.0.md`, `docs/feature-parity.md`,
  `docs/command-line-parity.md`, and the in-app Credits/Licenses pages
  if dependencies changed.
- Commits have clear messages (see below).

## Coding standards

### Rust

- **Edition / toolchain.** Use the version pinned in
  `rust-toolchain.toml`. Don't bump it casually.
- **Formatting.** `cargo fmt --all` (run by `just fmt`). The
  `rustfmt.toml` at the repo root is canonical.
- **Linting.** `cargo clippy --workspace --all-targets -- -D warnings`
  (run by `just lint`) must pass with no warnings.
- **Errors.** Prefer `Result<T, E>` with crate-specific error enums
  over `String`-typed errors at API boundaries. Internal helpers may
  use `Result<T, String>` for brevity.
- **Panics.** Don't panic from library code (`linsync-core`). Use
  `Result`. `unwrap` / `expect` is acceptable in tests and in the
  CLI/GUI binaries only when an invariant is impossible to violate.
- **Unsafe.** New `unsafe` blocks require a comment explaining the
  invariant they uphold.
- **Public API.** Re-export new public types through `lib.rs`. Keep
  modules narrow; large `mod.rs` files are a smell.
- **Async.** LinSync is synchronous on purpose. Don't introduce
  `tokio`, `async-std`, or `futures-executor` without discussing
  first.
- **Dependencies.** New crates must pass `cargo-deny`'s license and
  advisory policy in `deny.toml`. Prefer crates that already appear in
  `Cargo.lock`.

### QML / Kirigami (GUI)

- One QML file per page. Shared widgets go through
  `apps/linsync-gui/qml/AppCard.qml`, `LinSyncNavItem.qml`, etc.
- Signals must not collide with auto-generated `<property>Changed`
  names. Use suffixes like `Edited`, `Toggled`, `Activated`.
- Kirigami theme: `Kirigami.Theme.separatorColor` does not exist. Use
  the locally computed `Kirigami.ColorUtils.tintWithAlpha(...)` already
  exposed as `separator` / `separatorColor` on each page.
- `ListView` / `Repeater` delegates that read `modelData` or `model`
  need explicit `required property var modelData` /
  `required property var model`.
- The bridge URL must never be hardcoded — read it from
  `Qt.application.arguments` via `loadLaunchArguments`.

### Bash / packaging scripts

- `set -euo pipefail` at the top.
- Quote every variable expansion.
- Prefer `[[ ... ]]` over `[ ... ]` in `bash`.

### Commit messages

- Subject line: imperative mood, ≤ 72 characters, no trailing period.
  Example: `Fix trash move when target path contains a colon`.
- Body: wrap at 72 characters. Explain *why*, not *what* (the diff
  shows the what).
- Reference issues with `Fixes #123` / `Refs #123` on a final line
  when applicable.

## Issue reports

A useful bug report includes:

- LinSync version (`linsync --version` or `linsync-cli --version`).
- Distro + Qt version (`qmake6 -v`).
- The exact command you ran or the steps to reproduce.
- The expected result and the actual result.
- The `$XDG_STATE_HOME/linsync/linsync.log` excerpt if relevant.

Feature requests are very welcome. Please describe the problem you're
trying to solve before proposing the solution.

## Releases

Releases are cut by pushing a git tag to `master`. The
`.github/workflows/release.yml` workflow fires automatically on every
tag push and produces:

| Artefact                                     | Built in                                      |
| -------------------------------------------- | --------------------------------------------- |
| `linsync-<version>-linux-x86_64.tar.gz`      | `archlinux:base-devel` container              |
| `LinSync-<version>-x86_64.AppImage`          | same container, via `linuxdeploy` + plugin-qt |
| `linsync-<version>-1-x86_64.pkg.tar.zst`     | `archlinux:base-devel` container (`makepkg`)  |
| `linsync_<version>-1_amd64.deb`              | `debian:trixie-slim` container (`dpkg-buildpackage`) |
| `linsync-<version>-1.fc*.x86_64.rpm`         | `fedora:latest` container (`rpmbuild`)        |
| `sha256sums.txt`                             | published alongside every artefact            |

The Arch container is used for the heavy GUI build because apt-shipped
Kirigami lags upstream. The `.deb` job runs inside `debian:trixie-slim`
because that is the only Debian-family release whose apt repos ship
`qml6-module-org-kde-kirigami` — Ubuntu noble does not, even though it
ships Qt 6. `.rpm` builds run in a Fedora container since RPM packaging
requires the RPM toolchain and the Fedora-style devel package names.

Steps to cut a release:

1. Bump `workspace.package.version` in `Cargo.toml`.
2. Bump matching versions in `packaging/arch/PKGBUILD` (`pkgver`) and
   `packaging/rpm/linsync.spec` (`Version:`). The release workflow
   verifies the git tag matches the workspace version and will fail
   fast if they drift.
3. Update `packaging/debian/changelog` with a new entry.
4. `git commit`, push, then `git tag vX.Y.Z && git push origin vX.Y.Z`.
5. Watch the **release** workflow in GitHub Actions. On success it
   creates the GitHub Release with the artefact bundle plus auto-
   generated release notes.

## Security

If you find a vulnerability, **do not** open a public GitHub issue.
Report it privately through GitHub's private vulnerability reporting —
the repository's **Security** tab → **Report a vulnerability**. The
full policy and what to expect are in `docs/SECURITY.md`.

The bridge between the Rust host and the QML UI is loopback-only and
token-gated; please review `docs/SECURITY.md` and
`docs/qt-bridge-spike.md` before touching that boundary.

## Licensing

LinSync is GPL-3.0-only. By contributing, you agree that your changes
are made available under the same licence.

- Do **not** paste code from other diff or merge tools unless you
  have done a file-by-file licence review first
  (`docs/licensing.md` describes the policy).
- New third-party dependencies must satisfy `deny.toml`. If you need a
  copyleft dependency, raise it on the PR and we will revisit
  `docs/licensing.md`.

Thanks again — looking forward to your PR.
