# aski

> True-color terminal renderer for images, GIFs, and videos.

[![Rust](https://img.shields.io/badge/rust-2024-orange?logo=rust)](https://www.rust-lang.org/) [![License: GPL v3](https://img.shields.io/badge/license-GPLv3-blue.svg)](LICENSE) [![ffmpeg](https://img.shields.io/badge/video-ffmpeg-black)](https://ffmpeg.org/)

aski turns regular media into ANSI block output using full-color escape codes, with aspect-ratio correction,
transparency support, streaming video decode, and cached playback inside the terminal, plus a reusable Rust rendering
library.

## Overview

- Render static **images** directly in the terminal
- Play **standard input** as it arrives with `aski -`
- Download direct **HTTP/HTTPS media URLs** with `--url` / `-u`
- Play animated **GIFs** natively
- Play **videos** through ffmpeg with buffered streaming decode
- Draw two vertically stacked pixels per terminal cell using `▀` and separate foreground/background colors
- Preserve aspect ratio using terminal cell geometry
- Blend transparency against a configurable background color
- Cache rendered ANSI frames for smoother repeat playback
- Rebuild automatically when the terminal is resized

## Quick Start

```sh
aski image.png
aski - < clip.mkv
aski --url "https://example.com/photo.jpg"
aski -u "https://example.com/animation.gif" --loop
aski photo.jpg -b "#1e1e2e"
aski logo.png --opaque
aski animation.gif --loop
aski video.mp4 --precompute --loop
aski video.mp4 --prefetch 16 --fps-limit 30
aski clip.mp4 --no-cache --cell-width 9 --cell-height 20
```

## Examples

### Transparent static image

![aski transparent static image example](assets/example4.gif)
[Source](https://github.com/klpod221/klpod0s)

### Opaque static image

![aski opaque static image example](assets/example3.gif)
[Source](https://www.instagram.com/p/CLRnmbSFzq-/)

### Animated GIF

![aski animated GIF example](assets/example2.gif)
[Source](https://tenor.com/view/tung-tungtung-tungtungtung-sahur-tungtungtungsahur-tungtungsahur-gif-6699270143817937548)

### Video playback

![aski video playback example](assets/example1.gif)
[Source](https://blurbusters.com/hfr-120fps-video-game-recording/)

## Why aski

aski combines a few focused rendering techniques:

- 24-bit ANSI foreground and background colors using `▀` for two pixels per cell
- area-weighted downsampling instead of nearest-neighbor shortcuts
- correct handling of non-square terminal cells
- on-demand frame decode for animated media
- ANSI-string caching instead of retaining raw decoded frames in memory

## Feature Highlights

| Feature                    | What it does                                                                                          |
|----------------------------|-------------------------------------------------------------------------------------------------------|
| Two pixels per cell        | Uses `▀` with independent top and bottom colors to double vertical detail within the same terminal area |
| Aspect-ratio aware scaling | Fits media to the terminal using configurable cell dimensions (`--cell-width`, `--cell-height`)       |
| Real transparency          | Blends alpha against a configurable background instead of discarding it                               |
| Fast opaque path           | `--opaque` skips alpha math for images without meaningful transparency                                |
| Streaming video decode     | Uses ffmpeg in a background worker with bounded prefetch buffering                                    |
| ANSI frame cache           | Reuses rendered terminal frames on repeat playback instead of re-rendering every loop                 |
| Resize recovery            | Detects terminal size changes frame-by-frame and rebuilds output for the new size                     |
| Alternate-screen playback  | Keeps animation redraw isolated and restores the original terminal contents on exit                   |
| Runtime analytics          | `--verbose` reports dimensions, render timing, FPS, cache behavior, late frames, and playback summary |

## Playback Model

aski decodes GIF files natively and uses ffmpeg for video files and standard input.

| Mode           | Behavior                                                                                                               |
|----------------|------------------------------------------------------------------------------------------------------------------------|
| Default        | Decode, render, and display frames on demand. Cache ANSI frames when `--loop` is requested, unless `--no-cache` is set |
| `--precompute` | Render all frames from a file or URL before playback starts                                                            |
| `--loop`       | Replay a file or URL until interrupted with `Ctrl+C`                                                                   |
| `--fps-limit`  | Slow playback below the source frame rate without skipping frames                                                      |
| `--prefetch`   | Buffer more decoded video frames ahead of display to absorb decode hiccups                                             |
| `--no-cache`   | Disable ANSI frame caching when memory matters more than repeated-loop efficiency                                      |

## Installation

### Pre-built binary

Download the latest executable from [GitHub Releases](https://github.com/voximir-p/aski/releases).

### From source

```sh
cargo install --path .
```

### Manual build

```sh
cargo build --release
./target/release/aski image.png
```

## Usage

```text
Usage: aski [OPTIONS] <MEDIA>
```

### Arguments

| Argument  | Description                                                                  |
|-----------|------------------------------------------------------------------------------|
| `<MEDIA>` | A media file path, `-` for standard input, or a direct HTTP/HTTPS address with `--url` |

### Standard input

Use `-` as the media argument to play a pipe or redirected input. For example, this command generates a live test pattern.

```sh
ffmpeg -v error -f lavfi -i testsrc2=size=320x180:rate=24 -f yuv4mpegpipe - | aski -
```

One `ffmpeg` process reads stdin, identifies the format, and delivers frames at their stream timestamps.
aski renders each frame as it arrives and releases it before requesting the next. Playback starts as soon as the
decoder has enough input and ends at EOF or Ctrl+C. Resizing takes effect on the next frame from the same pipe.
Standard input uses no media file or frame cache. `--prefetch` applies only to video files, and `--loop` and
`--precompute` require a file or URL.

Use a format that carries its own headers and can be read sequentially, such as YUV4MPEG2, NUT, or streaming Matroska.
For headerless pixels, supply the geometry, pixel format, and frame rate to the producer. For example, to play a
640×480 RGBA capture at 30 FPS, wrap the pixels in NUT as they are read.

```sh
ffmpeg -v error -f rawvideo -pixel_format rgba -video_size 640x480 -framerate 30 -i frames.rgba -c:v copy -f nut - | aski -
```

### URL input

Use `-u` or `--url` to treat `<MEDIA>` as a URL:

```sh
aski --url "https://example.com/photo.jpg"
aski "https://example.com/animation.gif" -u --loop
aski -u "https://example.com/download?id=42&format=mp4" --precompute
```

Supply a direct media URL, rather than a web page such as a YouTube watch page,
a GIF sharing page, or a streaming playlist. Quote URLs to protect shell characters
such as `&`. HTTP and HTTPS are supported, including redirects (up to 10).
HTTPS certificates are verified.

A direct URL can still be protected by the hosting site. aski cannot complete
browser verification such as a Cloudflare challenge or use your browser's login
session. If a download reports browser verification or HTTP 403, open the URL in
your browser, complete any verification, and save the media. Then render the local
file without `--url`. Changing the user agent alone does not necessarily resolve these restrictions.

The complete file is downloaded to a temporary directory before rendering, using
bounded download buffers instead of loading the whole response into memory. Images
and GIFs are identified by their contents, so missing extensions, query strings, and
misleading filenames work. Other files go through the video decoder and require
`ffmpeg` and `ffprobe`, just like local videos.

Downloads show progress on stderr in a terminal or with `--verbose`. Ctrl+C cancels
downloading or playback. The temporary file is reused across loops and resizes,
including with `--no-cache`, and removed on normal exit, errors, or Ctrl+C.
A connection has 10 seconds to open; a stalled response times out after 30 seconds
without data. There is no total download duration limit. With `--url`, the argument must be an HTTP or HTTPS address.

### Display options

| Option                     | Description                                                            |
|----------------------------|------------------------------------------------------------------------|
| `-b, --background <COLOR>` | Background color for transparent areas. Default: `#1a1515`             |
| `-o, --opaque`             | Skip alpha blending math for faster rendering on opaque sources        |
| `-r, --reserve <ROWS>`     | Keep rows below the image; animation uses one for status. Default: `2` |

### Scaling options

| Option               | Description                                                    |
|----------------------|----------------------------------------------------------------|
| `--cell-width <PX>`  | Terminal cell width hint for aspect correction. Default: `10`  |
| `--cell-height <PX>` | Terminal cell height hint for aspect correction. Default: `22` |

Both settings describe a whole character cell. Each cell contains two vertically stacked output pixels;
aski accounts for their half-height internally, so use your font's full cell height for `--cell-height`.

### Playback options

| Option              | Description                                           |
|---------------------|-------------------------------------------------------|
| `-l, --loop`        | Loop playback until interrupted                       |
| `-p, --precompute`  | Render all frames before playback starts              |
| `--fps-limit <FPS>` | Cap playback FPS. `0` means use the source frame rate |

### Performance options

| Option                | Description                                                         |
|-----------------------|---------------------------------------------------------------------|
| `--prefetch <FRAMES>` | Queued video frames, `0`–`1024`. Default: `8`; `0` means unbuffered |
| `--no-cache`          | Disable ANSI frame caching                                          |

### Diagnostics

| Option          | Description                                      |
|-----------------|--------------------------------------------------|
| `-v, --verbose` | Print runtime diagnostics and playback analytics |

## Terminal UI

Use `aski -h` for a compact option list and examples, or `aski --help` for full explanations.
Help colors follow terminal detection and `NO_COLOR`; media output always uses true-color ANSI sequences.

During animation, the default status line shows the playback mode, displayed frame count, loop number,
and stop control. `--verbose` adds FPS and, in wider terminals, render time and cache/late-frame counts:

```text
PLAY  frame 42  |  loop 1  |  Ctrl+C stop
CACHE  42  |  29.9/30.0 FPS  |  0 late  |  Ctrl+C stop
```

`PLAY` means frames are decoded and rendered on demand. `CACHE` means previously rendered frames are reused.
The displayed count is cumulative across loops; `late` counts missed timing deadlines, not dropped frames.

Precomputation reports its progress without needing `--verbose`:

```text
PREPARING  120 frames  |  3.4 MiB  |  0.8s  |  Ctrl+C stop
```

Status lines shorten as the terminal narrows. Set `--reserve 0` to maximize image space; `--verbose`
reserves a status row when possible. In a one-row terminal, the image takes priority.
Resizing restarts file playback and rebuilds any cached frames. Standard input continues from its current position.
Press **Ctrl+C** to stop; the cursor, line wrapping, and original screen are restored before diagnostics appear.

## Background Color Formats

The `--background` option accepts the CSS-like formats below, case-insensitively. RGB percentages and
hue units (`deg`, `rad`, `grad`, `turn`) are supported. Optional alpha must be `1` or `100%` because
the background itself is opaque. Named colors, relative colors, and `none` are not supported.
Invalid input is rejected rather than silently replaced with the default.

| Format                | Example                         |
|-----------------------|---------------------------------|
| Hex 6-digit           | `#15161c`, `0xff00ff`, `ae6742` |
| Hex 3-digit shorthand | `#abc`                          |
| `rgb()`               | `rgb(21, 22, 28)`               |
| `hsl()`               | `hsl(235, 14%, 10%)`            |
| `hwb()`               | `hwb(235 8% 86%)`               |
| `lab()`               | `lab(8 1 -3)`                   |
| `lch()`               | `lch(8 3 290)`                  |
| `oklab()`             | `oklab(0.11 0.002 -0.015)`      |
| `oklch()`             | `oklch(0.11 0.015 270)`         |

## How It Works

Each terminal cell holds two vertically stacked output pixels, drawn using the Unicode upper half block `▀`.
The 24-bit ANSI foreground color fills the top half, and the background color fills the bottom half. An output of
80 columns by 22 terminal rows therefore samples an 80-by-44 pixel grid, doubling vertical detail within the same area.

For each image or animation frame, aski fits the source to the terminal using whole-cell dimensions to preserve its
aspect ratio, then samples each half-cell using area-weighted averaging. Fractional pixel coverage is preserved along
both axes, and images smaller than the terminal are enlarged without leaving empty cells. Channels are averaged in
sRGB space. Transparency in each half is blended independently against the color selected by `--background`;
`--opaque` skips that blending in both halves.
The renderer reuses sampling geometry and row buffers and emits foreground or background escapes only when that
color changes. The background resets before each line break to keep scrolling from tinting empty columns;
both colors are reset at the end of each frame.
Interior pixel sums use bounded 32-bit chunks that are combined into 64-bit totals, preserving exact accumulation
for wide images. ANSI color values are formatted in a reusable stack buffer without general-purpose formatting.

For animated media, frames are decoded as a stream and rendered one by one. For looping or precomputed playback, cached
data is the
rendered ANSI output rather than the raw RGBA frame buffer, which keeps repeated playback efficient without holding full
decoded frame history in memory.

## Supported Formats

| Type            | Support                                                                                                              |
|-----------------|----------------------------------------------------------------------------------------------------------------------|
| Static images   | Anything supported by the [`image`](https://crates.io/crates/image) crate: PNG, JPEG, WEBP, BMP, TIFF, TGA, and more |
| Animated images | GIF                                                                                                                  |
| Video           | MP4, MKV, AVI, MOV, WebM, FLV, WMV, M4V, TS, OGV via [ffmpeg](https://ffmpeg.org/)                                   |
| Standard input  | FFmpeg inputs readable from a pipe, including encoded images, image sequences, and streamable video containers         |

## Performance Tuning

- Use `--opaque` when transparency is irrelevant
- Increase `--prefetch` if video playback stalls during decode bursts
- Use `--fps-limit` to lower CPU usage per second when slower playback is acceptable
- Use `--no-cache` for very long or high-resolution videos if memory is the priority
- Adjust `--cell-width` and `--cell-height` if your terminal font looks stretched or squashed

## Notes

- Animated playback uses the terminal alternate screen buffer when stdout is a terminal
- Resize events invalidate stale frame cache entries and trigger a rebuild
- The playback status line adapts to terminal width; `--verbose` adds analytics and a summary after terminal restoration
- `--precompute` requires caching and conflicts with `--no-cache`
- `--fps-limit` extends frame durations; it slows the media rather than dropping frames to preserve its duration
- One-shot streaming playback does not retain a frame cache
- Invalid colors and zero cell dimensions are reported as argument errors
- Video files require `ffmpeg` and `ffprobe`. Standard input requires only `ffmpeg`. Playback is silent
- Ctrl+C cancels video probing as well as playback and cleans up the probe process
- Video files use their average frame rate. GIF files and standard input follow frame timing
- Video is decoded in its encoded orientation; rotation metadata is not applied
- Broken output pipes exit quietly, and decode failures return a nonzero exit status

## Troubleshooting

| Symptom                                          | What to check                                                                                                                        |
|--------------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------|
| `Failed to start ffprobe` or `ffmpeg`            | Put `ffmpeg` on `PATH` for standard input. Video files also require `ffprobe`. Static image files and GIF files use built-in decoders. |
| A video plays from a file but fails from a pipe | Use a streaming container or open the file by path. Some containers need to seek back to earlier data.                               |
| Escape codes appear as text or colors look wrong | Use a terminal that supports ANSI escape sequences and 24-bit color. Output redirected to a file still contains ANSI codes.          |
| Image looks stretched                            | Adjust the cell dimensions to match your font. Lower `--cell-width` for horizontally squashed output; raise it for stretched output. |
| High memory use when looping                     | Use `--no-cache` and reduce `--prefetch`. Precomputing stores every rendered frame.                                                  |
| Status line has fewer details                    | Widen the terminal. Compact layouts prioritize frame count, FPS, and the stop hint.                                                  |
| Video plays more slowly with `--fps-limit`       | This option extends frame durations. Use `0` to follow source timing.                                                                |

## Rust Library

The package also exposes a library without terminal side effects or signal handlers:

```rust
use aski::{Renderer, compute_output_dims, parse_color};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let image = image::open("image.png")?.to_rgba8();
    let (width, height) = image.dimensions();
    let (columns, rows) = compute_output_dims(width, height, 80, 22, 10, 22);
    let mut renderer = Renderer::new(
        width, height, columns, rows, parse_color("#15161c")?, false,
    )?;
    println!("{}", renderer.render(image.as_raw())?);
    Ok(())
}
```

`compute_output_dims` returns terminal columns and rows, and `Renderer::new` accepts those same whole-cell dimensions.
The renderer samples twice as many vertical pixels as terminal rows and packs each pair into one `▀` character.
Output width and twice the output height must fit in `u32`.

Reuse a `Renderer` for successive frames with the same dimensions. `render` borrows its output buffer;
call `.to_owned()` only when retaining a frame. Output uses CRLF between rows, ends in an ANSI reset,
and has no trailing newline. Invalid geometry and incorrectly sized RGBA buffers return `RenderError`.

`open_stream(path, &MediaType::Gif, 8)` lazily decodes animation frames. Videos use `MediaType::Video`;
dropping their stream closes the queue, kills and reaps ffmpeg, and joins the decoder worker.
`frames::open_stream_with_cancel` accepts an `Arc<AtomicBool>` for cancelling video probing, waits for video frames,
and further GIF frame decoding. Cancellation is checked between GIF frames; it does not interrupt a frame already
being decoded. Cancelling an in-progress probe terminates and reaps `ffprobe` before returning an empty stream.
Stream errors implement `std::error::Error` and include decoding context.

`frames::open_stdin_stream(cancelled)` reads the process's standard input once. Each frame has a zero delay because
FFmpeg already paces delivery. Use the same cancellation flag to interrupt a wait on an idle pipe.

## Development

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
# Requires ffmpeg and ffprobe:
cargo test --locked --test cli --test url -- --include-ignored
cargo bench --bench render
# Unix terminal resize, live stdin, Ctrl+C, and restoration checks (requires ffmpeg):
cargo build --locked
python3 tests/terminal_smoke.py target/debug/aski
```

Tests cover half-cell sampling against an independent two-dimensional reference, independent foreground/background
colors, transparency in both halves, upscaling, wide-image accumulator overflow, background resets at line breaks,
color parsing, variable frame timing, CLI failures, GIF cancellation, video decoder cleanup, stalled-probe cancellation,
large probe diagnostics,
and URL downloads against a local HTTP server (redirects, content detection, errors, and cancellation).
The benchmark measures reused rendering buffers at 360p, 1080p, and 4K, each rendered into 80-by-22 terminal cells
(80-by-44 output pixels); it excludes decoding and terminal I/O.

Release builds use the cargo-dist version pinned in `dist-workspace.toml`.
Windows ARM64 runs on `windows-11-arm` with its native Windows SDK. This avoids
a `ring` / cargo-xwin compiler mismatch where Clang receives MSVC-style `/imsvc`
flags. CI also builds that target with the `dist` profile and runs its `--version`
command on Windows ARM64.

After changing release configuration, use the pinned cargo-dist version to check it:

```sh
dist generate --check
dist plan
```

## Changelog

Release history is tracked in [CHANGELOG.md](CHANGELOG.md).

## License

aski is licensed under the [GNU General Public License v3.0](https://www.gnu.org/licenses/gpl-3.0.html#license-text).
See [LICENSE](LICENSE).
