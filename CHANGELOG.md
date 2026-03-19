# Changelog

## [1.0.1] - 2026-03-19

### Fixed

- Improve argument descriptions and formatting in README and CLI

## [1.0.0] - 2026-03-19

### Added

- Add Cargo.toml with dependencies for clap, image, and terminal_size.
- Implement CLI argument parsing using clap, allowing users to specify image path, background color, verbosity, and opacity.
- Create main function to handle image loading, terminal size retrieval, and rendering logic.
- Implement rendering functions for both opaque and alpha blending modes, outputting ANSI colored blocks.

[1.0.1]: https://github.com/voximir-p/aski/compare/v1.0.1...HEAD
[1.0.0]: https://github.com/voximir-p/aski/compare/v1.0.0...v1.0.1
