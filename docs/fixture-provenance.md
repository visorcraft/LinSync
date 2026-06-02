# Fixture Provenance

LinSync fixtures must be small, project-created examples derived from documented
behavior. They must not copy third-party source files, bundled examples, filters,
translations, screenshots, or proprietary sample data.

Current fixture families under `tests/fixtures/`:

- `text/`: tiny line-oriented files for equality, insertion/deletion, changed
  lines, base comparisons, patch output, and CLI text compare behavior.
- `binary/`: tiny byte sequences created for binary/hex difference tests.
- `table/`: project-created CSV rows for table compare behavior.
- `folders/`: project-created folder trees for left-only, right-only,
  identical, different, skipped, and error-state coverage.
- `merge/`: tiny base/left/right examples for three-way conflict tests.
- `image/`: a generated PPM checker pattern for image smoke coverage.
- `archive/`: placeholder member-list text used only to reserve archive fixture
  structure until real archive-helper tests exist.
- `symlink/` and `permissions/`: placeholders because platform-specific symlink
  and permission fixtures should be created by tests that need those behaviors.

When adding a fixture:

- Prefer generated or hand-written minimal data.
- State the behavior it covers in the test name or fixture README.
- Keep third-party files out unless their license has been reviewed and noted.
- Do not import third-party examples unless a later file-specific review proves
  GPL-3.0-only compatibility and the fixture is isolated with attribution.
