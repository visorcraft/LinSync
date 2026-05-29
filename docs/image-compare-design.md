# Image Compare Design

> Status: design — implementation pending follow-up plan.

## Goals

- Add an `image` compare mode to `linsync-core` backed entirely by pure-Rust image processing.
- Surface the mode through the CLI (`--mode image`) and a dedicated GUI page.
- Support PNG, JPEG, WebP, AVIF, and TIFF with streaming decode for large files.
- Three tolerance levels: exact pixel, per-channel tolerance, and perceptual deltaE (CIEDE2000).

## Non-goals

- RAW camera formats (CR2, NEF, ARW, DNG, etc.).
- Animated GIF or APNG diffing.
- Color-profile management beyond sRGB.
- Exporting diff overlays as a separate image file.

## Compare modes

### Exact pixel

Pixel equality is a bitwise match on every channel after decoding both images to RGBA8. Dimensions must match; a mismatch in size is a fatal difference (exit 2). Reports a count of differing pixels and their bounding box.

### Tolerance pixel

Each channel is compared independently with a configurable `tolerance` value in [0, 255]. A pixel is "different" if any channel exceeds the threshold. Useful for JPEG-recompressed images where block artifacts are expected but large edits must still be caught. Default tolerance: 0 (same as exact).

### Perceptual (deltaE / CIEDE2000)

Pixels are converted from sRGB to CIELAB, then CIEDE2000 delta-E is computed per pixel pair. A pixel is "different" if its delta-E exceeds a configurable `delta_e_threshold` (default 2.3, the "just noticeable difference" threshold commonly used in image comparison tooling). This mode is slower but correct for detecting perceptually invisible resampling or compression changes.

**Algorithm choice — CIEDE2000:** `dssim` yields a single scalar, not per-pixel data. Y′CbCr is fast but not perceptually uniform (false positives near hue boundaries). CIEDE2000 is the industry-standard per-pixel metric, available via the `lab` crate (MIT), and feeds directly into the diff overlay.

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
| AVIF   | `image` feature `avif` via `libavif-sys` | MIT + BSD-2-Clause | Yes |
| TIFF   | `image` feature `tiff` | MIT | Yes          |

Note: `image`'s `avif` feature depends on `libavif-sys` → `dav1d` (BSD-2-Clause) — compliant. Verify at each lockfile update via `just deny`.

Format detection uses **magic bytes** via `image`'s `ImageReader::open`. Extension-based fallback is an explicit opt-in flag in `ImageCompareOptions` (default false). Magic-bytes-first is the right default: LinSync operates on paths where extensions may be absent or wrong.

## Streaming decode

The `image` crate's `ImageDecoder` trait supports row-by-row decode. For files exceeding 100 MB or 16 384 × 16 384 pixels the engine uses a row-stripe strategy: open both decoders, confirm dimensions match from headers only, then decode 64 rows at a time into two reusable stripe buffers, accumulating per-pixel diff counts. The GUI overlay is similarly computed stripe-by-stripe and written to a temp file the QML `Image` element loads from a `file://` URI.

TIFF, PNG, and JPEG all expose row-level decode. AVIF and WebP decode fully into memory; files in those formats above the threshold emit a warning in the JSON summary but proceed rather than silently OOM. A future streaming path can be added without breaking the API.

## Diff visualization

**Red-overlay with opacity slider.** The diff mask is an RGBA8 buffer where differing pixels are `rgba(255, 40, 40, 200)` and equal pixels are transparent. Three panels side-by-side: left image | right image | right image with diff mask composited at adjustable opacity (0–100% slider).

Animated blink is incompatible with AppStream screenshots. A split-slider requires pixel-perfect registration and fails on dimension mismatches. Red-overlay works for both same-size and mismatched-size images (smaller is padded with transparent pixels), is screenshot-friendly, and is the approach used by pixelmatch, reg-viz, and Kaleidoscope.

## API surface

```rust
// crates/linsync-core/src/image.rs

pub struct ImageCompareOptions {
    pub mode: ImageCompareMode,           // Exact | Tolerance | Perceptual
    pub tolerance: u8,                    // 0–255 per-channel, Tolerance mode
    pub delta_e_threshold: f32,           // default 2.3, Perceptual mode
    pub trust_extension_fallback: bool,   // default false
    pub stream_stripe_rows: u32,          // default 64
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
}

pub fn compare_images(
    left: &std::path::Path,
    right: &std::path::Path,
    options: &ImageCompareOptions,
) -> Result<ImageCompareResult, ImageCompareError>;
```

`ImageCompareError` covers `DimensionMismatch`, `UnsupportedFormat`, `DecodeError`, and `IoError`.

## CLI integration

`crates/linsync-cli/src/main.rs` gains a `--mode image` branch. JSON summary is written to stdout; the overlay buffer is suppressed in CLI mode. Exit codes: 0 = equal, 1 = different, 2 = error.

```
linsync-cli compare --mode image [--tolerance N] [--delta-e N] a.png b.png
```

## GUI integration

New file: `apps/linsync-gui/qml/ImageComparePage.qml`

The page hosts a `RowLayout` with three `Image` elements, an opacity `Slider`, a `ComboBox` for mode (Exact / Tolerance / Perceptual), a threshold `SpinBox`, and a "Run Compare" button. The button calls `sessionBridge.compare_images(left, right, optionsJson)` (cxx-qt path) or `POST /compare/image` (HTTP path). The bridge returns `ImageCompareResult` as JSON; the overlay buffer is written to a temp file and loaded via `file://` URI. The image section activates when both paths are detected as image files (magic-byte check in the bridge) or when the user explicitly selects "Image" in the compare-mode menu.

## Sandbox interaction

None required. Pure-Rust in-process execution means no child processes and no `execve`. Phase 6 is not a prerequisite. If the engine is ever replaced with an external helper, Phase 6 must be completed first.

## Test plan

New file: `crates/linsync-core/tests/image_compare.rs`. Cases:

- Identical PNG → `equal: true`, zero differing pixels.
- 1-pixel edit → `differing_pixels: 1`, non-empty `diff_bbox`.
- JPEG recompressed pair within tolerance threshold → equal.
- Gradient pair above deltaE threshold → different; within threshold → equal.
- Dimension mismatch → `DimensionMismatch` error.
- Unsupported format → `UnsupportedFormat`.
- Magic-byte detection: PNG file with `.jpg` extension decoded as PNG.
- Synthesised 200 MB PNG compared against itself → equal without OOM.
- CLI: `linsync-cli compare --mode image same.png same.png` exits 0, JSON `"equal":true`.

New fixtures: `tests/fixtures/image/{same-a.png, same-b.png, recompressed.jpg, gradient-left.png, gradient-right.png}`.

## Open issues

1. **AVIF streaming:** `libavif-sys` does not expose row-level decode. Files > 100 MB in AVIF load fully; emit a warning in JSON summary and gate with a pre-decode size check.
2. **HDR / 16-bit:** The CIEDE2000 path operates on 8-bit samples; 16-bit PNG/TIFF inputs are downsampled. Precision loss must be documented.
3. **ICC profiles:** Ignored in the initial implementation; a follow-up can feed ICC data into the Lab conversion.
