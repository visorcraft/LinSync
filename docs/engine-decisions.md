# Engine Decisions

This document records implementation choices for future specialized engines.
It does not mark the corresponding features as complete; it keeps the later
work from starting with unresolved architecture questions.

## Syntax Highlighting

Prefer KDE's syntax-highlighting stack for the Qt/Kirigami GUI if it integrates
cleanly with the chosen editor rendering path. It aligns with the KDE-first
desktop target and avoids duplicating language grammar management in LinSync.

Tree-sitter remains a fallback for cases where the editor rendering strategy
needs syntax spans from Rust or where KDE integration cannot support the
required QML/editor model. Broad syntax highlighting stays deferred until the
core file compare workflow is stable.

## Image Compare

Use Rust-side image processing for deterministic compare data and QML for
display. The core/image layer should produce dimensions, pixel-difference
summary, tolerance results, masks or overlay buffers, and navigation metadata.
The QML side should handle zoom, pan, fit, synchronized view state, and mode
switching.

Use Qt image APIs for loading/display only where they simplify desktop
integration. Do not make Qt-specific image processing the only compare engine
unless performance tests prove the Rust path is inadequate.

## Plugin Sandboxing

For the plugin MVP, use explicit user or distributor trust: plugins are external
helpers discovered from known XDG/system plugin directories, and the app must
not execute downloaded plugins automatically.

Before treating plugins as untrusted third-party extensions, evaluate:

- Flatpak portal constraints for helper execution and filesystem access.
- Bubblewrap or similar process sandboxing for non-Flatpak builds.
- Manifest-declared sandbox expectations and user-visible trust prompts.
- Distributor-installed plugins versus user-installed plugins.

## Folder Hashing

Use a configurable hashing strategy for enhanced folder compare methods:

- Default to BLAKE3 for content hashing.
- Offer SHA-256 only when users need a standard compliance/audit hash.
- Consider xxHash only for speed-oriented, non-security identity checks where
  collision risk is acceptable and clearly documented.

Hashing must remain separate from built-in metadata methods so users can
tell when LinSync is using an enhanced method rather than a compatibility method.
