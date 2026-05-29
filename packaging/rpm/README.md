# RPM Packaging

`linsync.spec` builds an `linsync` package for the Fedora / RHEL family
(Fedora 40+, CentOS Stream 9+, AlmaLinux 9+, Rocky Linux 9+) - any RPM
distribution that ships Qt 6 and KF6 Kirigami in the system repositories.

## Qt private API and why this needs a container on non-Fedora hosts

cxx-qt-build runs `qmlcachegen` in full AOT-to-C++ mode, which links
against Qt's private API. Private symbols are pinned to the exact Qt
minor version they were compiled against. An RPM produced on Arch /
CachyOS (Qt 6.11) will not install on Fedora 44 (Qt 6.9) - dnf rejects
it with `nothing provides libQt6Qml.so.6(Qt_6_PRIVATE_API)`. To get a
Fedora-installable RPM from a non-Fedora host, use the container path
below.

## Build (Fedora host)

From this directory:

```sh
sudo dnf install rpm-build cargo rust \
                 qt6-qtbase-devel qt6-qtdeclarative-devel \
                 kf6-kirigami-devel pkgconf-pkg-config

# Stage a source tarball matching the spec's Source0 expectation.
( cd ../.. && git archive --format=tar.gz \
    --prefix=linsync-1.0.1/ --output=packaging/rpm/linsync-1.0.1.tar.gz HEAD )

rpmbuild --define "_topdir $(pwd)/_rpmbuild" \
         --define "_sourcedir $(pwd)" \
         -bb linsync.spec
```

The resulting `linsync-1.0.1-1.<dist>.x86_64.rpm` lands under
`_rpmbuild/RPMS/x86_64/`.

## Build (any host with podman, targets Fedora 44)

Use this from Arch / CachyOS / macOS / wherever - the build happens
inside a Fedora 44 container so the RPM links against Fedora's Qt 6.9.

From the repo root:

```sh
just package-rpm-fedora44
```

Or, equivalently:

```sh
bash packaging/rpm/build-in-container.sh
```

First run builds the image (~2-3 min on a reasonable connection).
Subsequent runs reuse it - pass `--rebuild-image` after editing the
`Containerfile.fedora44` or to pick up newer Fedora base updates.

The finished RPM lands under
`packaging/rpm/_rpmbuild-fedora44/RPMS/x86_64/`. Test it by passing it
to a Fedora 44 container:

```sh
podman run --rm -v ./packaging/rpm/_rpmbuild-fedora44/RPMS/x86_64:/rpms:ro \
    fedora:44 \
    dnf install -y /rpms/linsync-*.rpm
```

Cargo registry and target dir caches persist between runs in named
podman volumes (`linsync-fedora44-cargo`, `linsync-fedora44-target`),
so iterative rebuilds are incremental and do not contaminate the
host's own `target/`. To reclaim disk:

```sh
podman volume rm linsync-fedora44-cargo linsync-fedora44-target
```

## Notes

- The spec enables the `cxxqt` and `cxxqt-app` Cargo features so the
  resulting binary ships with the Qt 6 / Kirigami UI.
- Post-install scriptlets refresh the desktop, icon, and MIME caches.
- For COPR / Fedora-Submission, replace the `git archive` source-prep
  step with a stable `Source0` URL and run `cargo vendor` upstream so
  network-free builds remain reproducible.
