# Image Compare Design

> Status: implemented with remaining limitations tracked in `PLAN.md` Phase 5.

## Goals

- Add an `image` compare mode to `linsync-core` backed entirely by pure-Rust image processing.
- Surface the mode through the CLI (`compare --type image`) and a dedicated GUI
  page.
- Support PNG, JPEG, WebP, TIFF, GIF, Radiance HDR (`.hdr`), and OpenEXR (`.exr`)
  by default, with AVIF available when the `image-avif` cargo feature is enabled.
- Three tolerance levels: exact pixel, per-channel tolerance, and perceptual deltaE (CIEDE2000).
- Optional frame-by-frame comparison of animated GIF / APNG / animated WebP.

## Non-goals

- RAW camera formats (CR2, NEF, ARW, DNG, etc.).
- ICC color-profile conversion, true HDR validation, or color management beyond
  the decoded 8-bit RGBA sample values (HDR/EXR inputs are tone-mapped to RGBA8
  before comparison — see the limitations section).
- Editing source images or writing visual changes back into either input file.

## Compare modes

### Exact pixel

Pixel equality is a bitwise match on every channel after decoding both images to
RGBA8. Images with different dimensions are padded to a common transparent
canvas, reported as unequal, and include `padded: true` in the result metadata.
Reports a count of differing pixels, bounding box, and connected diff regions.

### Tolerance pixel

Each channel is compared independently with a configurable `tolerance` value in [0, 255]. A pixel is "different" if any channel exceeds the threshold. Useful for JPEG-recompressed images where block artifacts are expected but large edits must still be caught. Default tolerance: 0 (same as exact).

### Perceptual (deltaE / CIEDE2000)

Pixels are converted from sRGB to CIELAB, then CIEDE2000 delta-E is computed per pixel pair. A pixel is "different" if its delta-E exceeds a configurable `delta_e_threshold` (default 2.3, the "just noticeable difference" threshold commonly used in image comparison tooling). This mode is slower but correct for detecting perceptually invisible resampling or compression changes.

**Algorithm choice — CIEDE2000:** `dssim` yields a single scalar, not per-pixel data. Y′CbCr is fast but not perceptually uniform (false positives near hue boundaries). CIEDE2000 is the industry-standard per-pixel metric, available via the `lab` crate (MIT), and feeds directly into the diff overlay.

## Animated (frame) compare

Animated GIF, APNG, and animated WebP can be compared either as a single still
image or frame-by-frame, selected by `ImageCompareOptions.frame_mode`
(`FrameCompareMode`, serialized snake_case):

- **`first_frame` (default):** Compare only the first decoded frame — today's
  behavior and the only path for still images (a still is always a single "first
  frame"). Existing profiles are unchanged because this is the serde default.
- **`all_frames`:** Decode every frame of both inputs and compare corresponding
  frames pairwise. The result reports the total `frame_count` and a
  `per_frame_summaries` array of `FrameSummary { frame, equal, diff_ratio,
  differing_pixels, one_sided }`. A frame-count mismatch marks the extra frames
  as `one_sided` (and therefore different). Inputs whose decoder exposes only a
  single still frame degrade gracefully to a one-frame comparison, so
  `all_frames` is safe to request on any input.

The pixel comparison inside each frame uses the same exact / tolerance /
perceptual mode as a still compare. The CLI selects the mode with
`--image-frames first|all` and the bridge with `?frames=first|all`.

## Color-type metadata

Each comparison records the decoded color type of both sides on the result as
`color_type_left` / `color_type_right` (the `image` crate's `ColorType` debug
string, e.g. `"Rgba8"`, `"Rgb16"`, `"Rgb32F"` for HDR/EXR). These are surfaced so
a client can flag a color-type or bit-depth mismatch even when the tone-mapped
RGBA8 pixels happen to compare equal. They are `None` for results that predate
metadata capture (for example a JSON round-trip of an older saved result).

## Engine choice

**Pure-Rust: `image` + `lab`.**

1. **No sandbox dependency.** Phase 6 (sandbox) is still in design. ImageMagick as a helper would block this phase on Phase 6; pure-Rust runs in-process safely.
2. **License compatibility.** `image` and `lab` are MIT; all on `deny.toml`'s allow-list. ImageMagick's optional dependencies (Ghostscript, some libtiff builds) can introduce GPL-incompatible terms — a compliance risk not worth taking.
3. **No FFI, no child processes, no IPC.** Audit surface is minimal.

ImageMagick would only be reconsidered if performance benchmarks show pure-Rust cannot decode a 100 MB TIFF within acceptable wall-clock time on a typical desktop.

## File format support

| Format | Crate           | License   | deny.toml OK? |
|--------|-----------------|-----------|---------------|
| PNG    | `image` (built-in) | MIT    | Yes           |
| JPEG   | `image` + `jpeg-decoder` | MIT | Yes        |
| WebP   | `image` feature `webp` | MIT | Yes         |
| AVIF   | Optional `image-avif` feature | MIT + BSD-2-Clause | Yes |
| TIFF   | `image` feature `tiff` | MIT | Yes          |
| GIF    | `image` feature `gif` (`gif`, `color_quant`) | MIT, MIT OR Apache-2.0 | Yes |
| Radiance HDR | `image` feature `hdr` | MIT | Yes      |
| OpenEXR | `image` feature `exr` (`exr`, `lebe`, `bit_field`, `zune-inflate`) | BSD-3-Clause, MIT, MIT OR Apache-2.0 | Yes |

Note: AVIF support is intentionally feature-gated because its dependency graph
is larger than the default image stack. The `gif`, `hdr`, and `exr` features are
enabled by default and add the transitive decoder crates listed above (all on
`deny.toml`'s allow-list). Verify at each lockfile update via `just deny`.

Format detection uses **magic bytes** via `image`'s `ImageReader::open`. Extension-based fallback is an explicit opt-in flag in `ImageCompareOptions` (default false). Magic-bytes-first is the right default: LinSync operates on paths where extensions may be absent or wrong.

## Large-image path

For files exceeding 100 MB or 16 384 × 16 384 pixels the engine switches to a
large-image compare path that processes rows in stripes after decoding. The
current implementation still decodes through `image` into RGBA buffers before
comparison; true decoder-level streaming remains future work. Mismatched
dimensions are padded to a common transparent canvas before stripe comparison.
The GUI overlay is written to a temp file the QML `Image` element loads from a
`file://` URI.

PNG, JPEG, WebP, and TIFF therefore share the same public result shape today.
AVIF follows the same behavior when the `image-avif` feature is enabled. A
future decoder-level streaming path can be added without breaking the API.

## Diff visualization

**Red-overlay with opacity slider.** The diff mask is an RGBA8 buffer where differing pixels are `rgba(255, 40, 40, 200)` and equal pixels are transparent. Three panels side-by-side: left image | right image | right image with diff mask composited at adjustable opacity (0–100% slider).

Animated blink is incompatible with AppStream screenshots. A split-slider requires pixel-perfect registration and fails on dimension mismatches. Red-overlay works for both same-size and mismatched-size images (smaller is padded with transparent pixels), is screenshot-friendly, and is the approach used by pixelmatch, reg-viz, and Kaleidoscope.

The GUI saves the generated temp overlay PNG through
`/compare/image/save-overlay?path=...`, copying the last generated overlay to a
user-selected durable path.

## Color, alpha, HDR, and animation limitations

LinSync compares decoded RGBA8 samples, not source color-management metadata.
That keeps the engine deterministic and pure Rust, but it creates several
intentional limitations:

- **ICC profiles and wide-gamut color:** ICC profiles are not transformed into a
  common working color space. Exact and tolerance modes compare the decoded RGBA
  channel values directly. Perceptual mode treats the decoded RGB channels as
  sRGB before converting to Lab, so files that rely on embedded display profiles
  or wide-gamut interpretation can report differences that are not visually
  representative on a managed display.
- **HDR, EXR, and high bit depth:** Radiance HDR (`.hdr`) and OpenEXR (`.exr`)
  now decode (via the `image` crate's `hdr`/`exr` features), but every input —
  including high-bit-depth PNG/TIFF and 32-bit-float HDR/EXR — is **tone-mapped /
  reduced to 8-bit RGBA8** before comparison. The float-to-8-bit conversion loses
  precision and discards the original transfer function and scene-referred values,
  so two HDR images that differ only below the 8-bit quantization step can compare
  equal. The decoded `color_type_left`/`color_type_right` metadata lets a client
  notice the higher-precision source even though the comparison itself runs at
  8 bits. Image compare is therefore appropriate for release-artifact checks, not
  for validating HDR mastering or color-pipeline fidelity. **ICC profile
  interpretation stays out of scope** — see the ICC bullet above.
- **Alpha:** Exact and tolerance modes include alpha as a fourth channel, so
  transparency changes are differences. Perceptual mode currently ignores alpha
  and computes CIEDE2000 from RGB only; a pure alpha change with identical RGB
  samples is not reported in perceptual mode. Dimension padding uses transparent
  black pixels, so padded extents are visible to exact/tolerance comparisons and
  may be invisible to perceptual comparison if only alpha differs.
- **Animated inputs:** LinSync compares the first frame by default
  (`frame_mode: first_frame`) and can compare every frame pairwise with
  `frame_mode: all_frames` for GIF, APNG, and animated WebP (see "Animated (frame)
  compare" above). What it still does **not** model is the animation *timeline*:
  per-frame delays, disposal/blend modes, and loop counts are ignored — frames are
  compared by index, not by playback time. Animated AVIF is not frame-decoded. Use
  a dedicated animation tool when frame timing or disposal semantics matter.

## API surface

```rust
// crates/linsync-core/src/image.rs

pub struct ImageCompareOptions {
    pub mode: ImageCompareMode,           // Exact | Tolerance | Perceptual
    pub tolerance: u8,                    // 0–255 per-channel, Tolerance mode
    pub delta_e_threshold: f32,           // default 2.3, Perceptual mode
    pub trust_extension_fallback: bool,   // default false
    pub stream_stripe_rows: u32,          // default 64
    pub frame_mode: FrameCompareMode,     // FirstFrame (default) | AllFrames
}

pub struct ImageCompareResult {
    pub equal: bool,
    pub left_dims: (u32, u32),
    pub right_dims: (u32, u32),
    pub total_pixels: u64,
    pub differing_pixels: u64,
    pub diff_ratio: f64,
    pub mode_used: ImageCompareMode,
    pub diff_bbox: Option<(u32, u32, u32, u32)>,
    #[serde(skip)]
    pub overlay: Vec<u8>,  // RGBA8; empty for CLI, populated for GUI
    pub padded: bool,
    pub diff_regions: Vec<DiffRegion>,
    pub color_type_left: Option<String>,   // decoded ColorType, e.g. "Rgba8"
    pub color_type_right: Option<String>,
    pub frame_count: Option<usize>,        // Some(n) for AllFrames, else None
    pub per_frame_summaries: Vec<FrameSummary>, // empty unless AllFrames
}

pub fn compare_images(
    left: &std::path::Path,
    right: &std::path::Path,
    options: &ImageCompareOptions,
) -> Result<ImageCompareResult, ImageCompareError>;
```

`ImageCompareError` covers `DimensionMismatch`, `UnsupportedFormat`,
`DecodeError`, and `IoError`. Dimension mismatch is retained as an error variant
for compatibility, but the current compare path pads mismatched dimensions to a
common canvas and reports the result with `padded: true`.

## CLI integration

`crates/linsync-cli/src/main.rs` routes `compare --type image` through the core
image engine. JSON summary is written to stdout; the overlay buffer is
suppressed in CLI mode. Exit codes: 0 = equal, 1 = different, 2 = error.

```
linsync-cli compare --type image [--image-mode exact|tolerance|perceptual] \
    [--image-tolerance N] [--image-delta-e N] [--image-frames first|all] \
    [--save-result FILE] a.png b.png
```

`--image-frames all` selects animated frame-by-frame compare; the JSON summary
then includes `frame_count` and a `differing_frames` count. `--save-result FILE`
writes a `{schema_version: 1, kind: "image", result}` envelope that
`report --from-json FILE` re-renders to HTML via
`ImageCompareResult::to_html_report` (see `docs/feature-matrix.md`).

## GUI integration

New file: `apps/linsync-gui/qml/ImageComparePage.qml`

The page hosts left, right, and overlay image panes, an opacity `Slider`, a
`ComboBox` for mode (Exact / Tolerance / Perceptual), threshold `SpinBox`
controls, zoom/fit/split controls, and a "Run Compare" button. The button calls
the HTTP bridge with `GET /compare/image?...&overlay=true`; the bridge returns
`ImageCompareResult` as JSON, writes the overlay buffer to a temp file, and
returns a `file://` URI that QML loads into the overlay pane. The bridge accepts a
`?frames=first|all` override (alongside `?mode`, `?tolerance`, `?delta_e`) to
select frame-by-frame animated compare.

## Sandbox interaction

None required. Pure-Rust in-process execution means no child processes and no `execve`. Phase 6 is not a prerequisite. If the engine is ever replaced with an external helper, Phase 6 must be completed first.

## Test plan

New file: `crates/linsync-core/tests/image_compare.rs`. Cases:

- Identical PNG → `equal: true`, zero differing pixels.
- 1-pixel edit → `differing_pixels: 1`, non-empty `diff_bbox`.
- JPEG recompressed pair within tolerance threshold → equal.
- Gradient pair above deltaE threshold → different; within threshold → equal.
- Dimension mismatch → `padded: true`, `equal: false`, and diff regions on the
  common canvas.
- Unsupported format → `UnsupportedFormat`.
- Magic-byte detection: PNG file with `.jpg` extension decoded as PNG.
- Synthesised 200 MB PNG compared against itself → equal without OOM.
- CLI: `linsync-cli compare --type image same.png same.png` exits 0, JSON `"equal":true`.

New fixtures: `tests/fixtures/image/{same-a.png, same-b.png, recompressed.jpg, gradient-left.png, gradient-right.png}`.

## Open issues

1. **Decoder-level streaming:** the large-image path processes rows in stripes
   after decode, but still materializes RGBA buffers first. True decoder-level
   streaming remains future work, including for AVIF when `image-avif` is
   enabled.
2. **ICC/HDR fidelity:** Documented limitation. A follow-up can add explicit ICC
   conversion, high-bit-depth compare paths, or alpha-aware perceptual metrics.
