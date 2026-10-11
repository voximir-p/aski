# Changelog

# [2.0.1] - 2026-10-11

### Added

- Detect Cloudflare browser-verification responses and explain how to render a browser-downloaded file; add actionable
  guidance for other HTTP 403 failures.

## [2.0.0] - 2026-10-11

### Added

- `--url` / `-u` input for direct HTTP/HTTPS images, GIFs, and videos, with redirects, content detection, download
  progress, cancellation, and temporary-file cleanup.
- Adaptive playback status with PLAY/CACHE modes, a Ctrl+C hint, and progress during precomputation.
- Reusable Rust library with area-filtered rendering, color parsing, lazy frame streams, and typed errors.
- Regression tests, a rendering benchmark, and cross-platform formatting, lint, and test CI.
- Cooperative cancellation for video frame waits and unbuffered video delivery with `--prefetch 0`.

### Changed

- Use separable area filtering with precomputed coverage, reusable row buffers, and integer interior sums.
- Avoid repeated ANSI color escapes and retain frame caches only for looping or precomputed playback.
- Validate colors, cell geometry, prefetch sizes, and conflicting playback flags during argument parsing.
- Refresh short and long CLI help, usage examples, argument names, README, troubleshooting, and library documentation.
- Document that FPS limiting slows playback without dropping frames.

### Fixed

- Render small images correctly instead of rejecting them or leaving holes when upscaling.
- Handle Unicode color input without panicking; support RGB percentages, hue units, and CSS Lab's D50 white point.
- Preserve per-frame GIF timing and count displayed frames correctly with caching disabled or interrupted playback.
- Report decoder failures, truncated video frames, and output errors with a failing exit status.
- Restore terminal state on failures and cancellation, and reap ffmpeg on early stream drop.
- Disable implicit video rotation so raw frame geometry agrees with probed dimensions.
- End static output on a fresh line and handle broken pipes quietly.

## [1.1.2] - 2026-03-20

### Fixed

- Fix FPS analytics reporting incorrectly.

## [1.1.1] - 2026-03-20

### Changed

- Improve CLI help formatting and descriptions for all flags.

## [1.1.0] - 2026-03-19

### Added

- Add on-demand frame decoding pipeline for GIF and video playback.
- Add ANSI frame cache playback path (stores rendered terminal frames instead of raw RGBA).
- Add buffered ffmpeg decode prefetching via background worker thread (`--prefetch`).
- Add playback and scaling tuning flags: `--fps-limit`, `--cell-width`, `--cell-height`, `--no-cache`.
- Add grouped CLI help sections (`Display`, `Scaling`, `Playback`, `Performance`, `Diagnostics`).

### Changed

- Refactor rendering code into focused modules (`animated_playback`, `static_playback`, `media_render`,
  `terminal_utils`) with a slim `main.rs` dispatcher.
- Improve verbose analytics to report display-time FPS (excluding precompute startup work).
- Move animated playback to alternate screen for stable in-place redraw behavior.
- Place verbose runtime analytics directly under the rendered video area.
- Reworked README to highlight new features and usage patterns, and added a detailed feature table.
- Add usage examples to README to demonstrate different features.

### Fixed

- Fix frame stacking/line-drift during animation redraw across terminals.
- Fix resize handling to invalidate stale frame caches and rebuild at new dimensions.
- Fix terminal recovery edge cases by hardening cleanup paths when playback exits unexpectedly.
- Fix precompute verbose banner placement to always render at terminal top-left after resizes.

## [1.0.1] - 2026-03-19

### Fixed

- Improve argument descriptions and formatting in README and CLI

## [1.0.0] - 2026-03-19

### Added

- Add Cargo.toml with dependencies for clap, image, and terminal_size.
- Implement CLI argument parsing using clap, allowing users to specify image path, background color, verbosity, and
  opacity.
- Create main function to handle image loading, terminal size retrieval, and rendering logic.
- Implement rendering functions for both opaque and alpha blending modes, outputting ANSI colored blocks.

[2.0.1]: https://github.com/voximir-p/aski/compare/v2.0.0...v2.0.1
[2.0.0]: https://github.com/voximir-p/aski/compare/v1.1.2...v2.0.0
[1.1.2]: https://github.com/voximir-p/aski/compare/v1.1.1...v1.1.2
[1.1.1]: https://github.com/voximir-p/aski/compare/v1.1.0...v1.1.1
[1.1.0]: https://github.com/voximir-p/aski/compare/v1.0.1...v1.1.0
[1.0.1]: https://github.com/voximir-p/aski/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/voximir-p/aski/releases/tag/v1.0.0
