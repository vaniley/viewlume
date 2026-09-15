# Changelog

All notable changes to Viewlume are documented in this file. The project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0] - 2026-09-15

### Added

- GPU-accelerated image canvas with cursor-centered zoom and inertial panning.
- Transparent fullscreen overlay and regular window mode.
- Keyboard, edge-button, and thumbnail-carousel navigation.
- Virtualized cover-flow carousel with background thumbnail loading.
- RAM caching and adjacent-image prefetching.
- JPEG/JFIF, PNG, WebP, GIF, BMP, TIFF, QOI, ICO, TGA, DDS, OpenEXR,
  Radiance HDR, Farbfeld, and PNM decoding.
- EXIF orientation handling and natural filename sorting.
- Persistent settings with migration from the former application name.

### Fixed

- Duplicate thumbnail requests that could exhaust memory and crash the application.
- Delayed display of completed background image loads.
- Incorrect GPU texture after a cache hit.
- Zoom and position loss when switching between images with different resolutions.
- Runaway panning caused by applying cumulative drag displacement every frame.

[Unreleased]: https://github.com/vaniley/viewlume/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/vaniley/viewlume/releases/tag/v0.1.0
