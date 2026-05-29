# Webpage Compare Decision

Webpage compare is a post-MVP specialized comparison feature. It must not become the
application shell: LinSync remains a native Qt/Kirigami desktop app, and any
browser engine is limited to the compare surface that explicitly needs rendered
web content.

## Scope

Planned webpage compare modes:

- Rendered page compare for visual differences.
- Screenshot compare by sending captured viewports or full pages to image
  compare.
- HTML source compare through the text compare path.
- Extracted page text compare through the text compare path.
- Resource-tree compare through the folder compare path.

URL input should be accepted only when the user explicitly starts a webpage or
URL compare. Plain file/folder compare must not fetch network resources.

## Browser Engine Boundary

Qt WebEngine is the likely candidate for rendered-page compare because it fits
the Qt desktop stack, but it must remain optional and feature-gated until the
licensing, packaging, sandboxing, and data-retention costs are reviewed. The
default application shell must not depend on Qt WebEngine.

If Qt WebEngine is unavailable, LinSync should still offer a non-rendered
fallback for fetched HTML, extracted text, and downloaded resource trees where
security policy allows network access.

## Privacy And Security

Webpage compare is network-active and privacy-sensitive.

Required controls before enabling it:

- A user-visible start action before any URL is fetched.
- Clear indication that third-party page resources may be requested while
  rendering.
- Separate browsing profile or storage for webpage compare data.
- Controls to clear cache, cookies, history, local storage, and downloaded
  resources created by webpage compare.
- Cache placement under `$XDG_CACHE_HOME/linsync` with documented cleanup.
- No reuse of personal browser profiles, cookies, or saved credentials.
- Flatpak notes for network access, sandbox behavior, and host integration.
- Tests using controlled local servers or fixtures instead of live third-party
  websites.

Until these controls exist, URL/webpage compare remains disabled and documented
as pending.
