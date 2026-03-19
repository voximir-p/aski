# aski

Render any image as true-color ANSI block art — directly in your terminal.

```sh
aski image.png
aski photo.jpg -b "#1e1e2e"
aski logo.png --opaque
aski animation.gif --loop
aski video.mp4 --precompute --loop
aski video.mp4 --prefetch 16 --fps-limit 30
aski clip.mp4 --no-cache --cell-width 9 --cell-height 20
```

---

## How it works

Each terminal cell becomes one pixel of output, drawn with the Unicode full-block character `█` colored with a 24-bit ANSI escape code. Since terminal cells are taller than they are wide (~10 wide × 22 tall in most fonts), aski corrects for that aspect ratio automatically so the image doesn't look squashed or stretched.

To map many image pixels onto fewer terminal cells, aski averages the colors of all source pixels that fall into each cell. This means even small outputs look smooth rather than blocky — it's a proper area-weighted downsample, not just nearest-neighbor.

For videos and GIFs, frames are decoded as a stream (on-demand) and rendered one-by-one. Cached data is stored as rendered ANSI strings (not raw RGBA frame buffers), reducing long-loop memory pressure.

---

## Features

### True aspect-ratio scaling

aski measures the actual terminal size, computes the correct output dimensions accounting for the 10:22 cell aspect ratio, and fits the image to fill as much of the terminal as possible without distorting it — width-first if it fits, height-constrained otherwise.

### Real transparency

For images with alpha (PNGs, WEBPs, etc.), each pixel's opacity is used to smoothly blend the image color toward the background color. A pixel that is 50% transparent will appear as a 50/50 mix. Fully transparent areas show the background color cleanly. The background can be any color you like (see below).

### `--opaque` flag for maximum performance

If you know your image has no meaningful transparency, pass `-o` / `--opaque` to skip all alpha math entirely. This path uses 32-bit accumulators instead of 64-bit, halves the memory bandwidth of the hot loop, and is compiled fully independently so the optimizer can go as far as possible.

### Streaming video decode + buffered prefetch

Video playback uses ffmpeg in a background decode thread and streams raw frames through a bounded prefetch queue (`--prefetch`). This keeps playback smoother under transient decode stalls while avoiding full upfront frame decode into RAM.

### Rich color input for `--background`

Set your background color in any format you'd use in CSS:

| Format                | Example                         |
|-----------------------|---------------------------------|
| Hex 6-digit           | `#15161c`, `0xff00ff`, `ae6742` |
| Hex 3-digit shorthand | `#abc` (same as `#aabbcc`)      |
| `rgb()`               | `rgb(21, 22, 28)`               |
| `hsl()`               | `hsl(235, 14%, 10%)`            |
| `hwb()`               | `hwb(235 8% 86%)`               |
| `lab()`               | `lab(8 1 -3)`                   |
| `lch()`               | `lch(8 3 290)`                  |
| `oklab()`             | `oklab(0.11 0.002 -0.015)`      |
| `oklch()`             | `oklch(0.11 0.015 270)`         |

The default background is `#15161c`.

### Line reservation

By default, aski reserves 2 lines at the bottom of the terminal so the image doesn't push your shell prompt off screen. Adjust with `-r` / `--reserve`.

### Verbose mode

Pass `-v` / `--verbose` to print debug info to stderr: image dimensions, terminal size, effective output size, and total cell count.

### Video & animated image playback

aski can play animated GIFs natively and any video format via ffmpeg (MP4, MKV, AVI, MOV, WebM, etc.).

- **Realtime mode** (default): each frame is decoded, rendered, and displayed on the fly. After the first complete pass, rendered ANSI frames are cached in memory (unless `--no-cache` is set), so subsequent loops avoid re-rendering.
- **`--precompute` mode** (`-p`): all frames are rendered to ANSI strings before playback begins. This trades startup time for smooth playback from the first shown frame.
- **`--loop` mode** (`-l`): plays the animation/video in an infinite loop until you press Ctrl+C. Without `--loop`, playback runs once and exits.
- **Terminal resize adaptation**: aski checks terminal size on every frame. On resize, it clears the screen, invalidates stale cache data, and rebuilds at the new dimensions automatically.
- **Alternate-screen playback**: animated playback runs in an alternate terminal screen buffer, keeping redraw stable and restoring your original terminal contents on exit.

---

## Installation

```sh
cargo install --path .
```

Or build manually:

```sh
cargo build --release
./target/release/aski image.png
```

---

## Usage

```text
Usage: aski [OPTIONS] <IMAGE>

Arguments:
  <IMAGE>  Path to the image, GIF, or video file to render in the terminal

Options:
  -h, --help                     Print help
  -V, --version                  Print version

Display:
  -b, --background <BACKGROUND>  Background color for transparent areas [default: #15161c]
  -o, --opaque                   Skip alpha blending math (faster for opaque sources)
  -r, --reserve <ROWS>           Rows to reserve at terminal bottom [default: 2]

Scaling:
      --cell-width <PX>          Terminal cell width hint for aspect correction [default: 10]
      --cell-height <PX>         Terminal cell height hint for aspect correction [default: 22]

Playback:
  -l, --loop                     Loop playback until Ctrl+C
  -p, --precompute               Render all frames before playback starts
      --fps-limit <FPS>          Cap playback FPS (0 = source FPS) [default: 0]

Performance:
      --prefetch <FRAMES>        Buffered video frames for ffmpeg decode thread [default: 8]
      --no-cache                 Disable ANSI frame cache

Diagnostics:
  -v, --verbose                  Print runtime stats and playback diagnostics
```

---

## Supported formats

**Static images**: anything the [`image`](https://crates.io/crates/image) crate supports — PNG, JPEG, WEBP, BMP, TIFF, TGA, and more.

**Animated images**: GIF (native, no external dependencies).

**Video**: MP4, MKV, AVI, MOV, WebM, FLV, WMV, M4V, TS, OGV — requires [ffmpeg](https://ffmpeg.org/) installed and on your PATH.

---

## Performance notes

- Use `--opaque` for sources without meaningful transparency.
- Use `--prefetch` to smooth decode bursts on slower systems.
- Use `--fps-limit` to reduce CPU usage for very high-FPS inputs.
- Use `--no-cache` for very long/high-resolution videos when memory matters more than loop-time CPU.
- If output appears stretched, tune `--cell-width` and `--cell-height` to match your terminal font.

---

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for a detailed history of changes and improvements.

---

## License

aski is licensed under the [GNU General Public License v3.0](https://www.gnu.org/licenses/gpl-3.0.html#license-text) ([LICENSE](LICENSE))
