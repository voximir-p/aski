# aski

Render any image as true-color ANSI block art — directly in your terminal.

```sh
aski image.png
aski photo.jpg -b "#1e1e2e"
aski logo.png --opaque
aski animation.gif --loop
aski video.mp4 --precompute --loop
```

---

## How it works

Each terminal cell becomes one pixel of output, drawn with the Unicode full-block character `█` colored with a 24-bit ANSI escape code. Since terminal cells are taller than they are wide (~10 wide × 22 tall in most fonts), aski corrects for that aspect ratio automatically so the image doesn't look squashed or stretched.

To map many image pixels onto fewer terminal cells, aski averages the colors of all source pixels that fall into each cell. This means even small outputs look smooth rather than blocky — it's a proper area-weighted downsample, not just nearest-neighbor.

---

## Features

### True aspect-ratio scaling

aski measures the actual terminal size, computes the correct output dimensions accounting for the 10:22 cell aspect ratio, and fits the image to fill as much of the terminal as possible without distorting it — width-first if it fits, height-constrained otherwise.

### Real transparency

For images with alpha (PNGs, WEBPs, etc.), each pixel's opacity is used to smoothly blend the image color toward the background color. A pixel that is 50% transparent will appear as a 50/50 mix. Fully transparent areas show the background color cleanly. The background can be any color you like (see below).

### `--opaque` flag for maximum performance

If you know your image has no meaningful transparency, pass `-o` / `--opaque` to skip all alpha math entirely. This path uses 32-bit accumulators instead of 64-bit, halves the memory bandwidth of the hot loop, and is compiled fully independently so the optimizer can go as far as possible.

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

- **Realtime mode** (default): each frame is rendered and displayed on the fly. After the first complete loop, all frames are cached in memory so subsequent loops play back instantly with no rendering overhead.
- **`--precompute` mode** (`-p`): all frames are rendered to ANSI strings before playback begins. This trades a longer startup for guaranteed smooth playback from the very first frame.
- **`--loop` mode** (`-l`): plays the animation/video in an infinite loop until you press Ctrl+C. Without `--loop`, playback runs once and exits.
- **Terminal resize adaptation**: aski checks the terminal size on every frame. If you resize the terminal mid-playback, it clears the screen, invalidates the frame cache, and re-renders at the new size automatically.

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
  <IMAGE>  Path to the input image

Options:
  -r, --reserve <RESERVE>        Lines to reserve at the bottom of the terminal [default: 2]
  -b, --background <BACKGROUND>  Background color [default: #15161c]
  -v, --verbose                  Verbose output
  -o, --opaque                   Skip alpha blending (may increase performance)
  -l, --loop                     Loop playback until Ctrl+C
  -p, --precompute               Precompute all frames before displaying
  -h, --help                     Print help
  -V, --version                  Print version
```

---

## Supported formats

**Static images**: anything the [`image`](https://crates.io/crates/image) crate supports — PNG, JPEG, WEBP, BMP, TIFF, TGA, and more.

**Animated images**: GIF (native, no external dependencies).

**Video**: MP4, MKV, AVI, MOV, WebM, FLV, WMV, M4V, TS, OGV — requires [ffmpeg](https://ffmpeg.org/) installed and on your PATH.

---

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for a detailed history of changes and improvements.

---

## License

aski is licensed under the [GNU General Public License v3.0](https://www.gnu.org/licenses/gpl-3.0.html#license-text) ([LICENSE](LICENSE))
