# Linux Metadata Mapping

LinSync does not copy Windows metadata semantics into Linux compare/filter
behavior. Windows-only attributes must either map to a Linux-native property,
produce an explicit unsupported diagnostic, or remain non-applicable.

## File Attributes

Windows DOS attributes map as follows:

| Windows attribute | LinSync replacement |
| --- | --- |
| Read-only | Unix write permission and filesystem read-only errors. |
| Hidden | Leading-dot filename convention and desktop metadata only where available. |
| System | No direct Linux equivalent; unsupported in filters. |
| Archive | No direct Linux equivalent; unsupported in filters. |
| Temporary | No direct Linux equivalent; use path/cache filters instead. |
| Offline / compressed / encrypted | No direct portable Linux equivalent; document as unsupported unless a mounted filesystem exposes a reliable standard attribute. |

## Timestamps

Linux compare behavior uses modified time where timestamp comparison is needed.
Creation time is not portable across Linux filesystems and must not be treated as
a required filter or compare attribute. If a filesystem exposes birth time
through a stable API later, it can be added as an optional attribute with a clear
"unavailable" state.

## Version Resources

Windows executable version resources have no general Linux equivalent. Linux
packages, ELF metadata, AppStream metadata, and language-specific manifests can
be compared as ordinary files or through future helper plugins, but they are not
a direct replacement for Windows version-resource filters.

## Shell Properties

Windows shell properties are replaced by Linux-native sources only when they are
real files or stable desktop metadata:

- FreeDesktop `.desktop`, MIME, and AppStream files compare as structured text.
- Extended attributes may be considered later, but only with explicit opt-in and
  filesystem support checks.
- Tracker, Baloo, or file-manager database metadata must not be a required
  dependency for core compare behavior.

## Filter Diagnostics

Unsupported Windows-only attributes should produce migration diagnostics rather
than silently matching everything or nothing. Diagnostics should name the
attribute and suggest the closest Linux-native replacement when one exists.
