# Changelog

## [1.1.0] - 2026-03-19

### Added

- Add on-demand frame decoding pipeline for GIF and video playback.
- Add ANSI frame cache playback path (stores rendered terminal frames instead of raw RGBA).
- Add buffered ffmpeg decode prefetching via background worker thread (`--prefetch`).
- Add playback and scaling tuning flags: `--fps-limit`, `--cell-width`, `--cell-height`, `--no-cache`.
- Add grouped CLI help sections (`Display`, `Scaling`, `Playback`, `Performance`, `Diagnostics`).

### Changed

- Refactor rendering code into focused modules (`animated_playback`, `static_playback`, `media_render`, `terminal_utils`) with a slim `main.rs` dispatcher.
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
- Implement CLI argument parsing using clap, allowing users to specify image path, background color, verbosity, and opacity.
- Create main function to handle image loading, terminal size retrieval, and rendering logic.
- Implement rendering functions for both opaque and alpha blending modes, outputting ANSI colored blocks.

[1.1.0]: https://github.com/voximir-p/aski/compare/v1.0.1...v1.1.0
[1.0.1]: https://github.com/voximir-p/aski/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/voximir-p/aski/releases/tag/v1.0.0
