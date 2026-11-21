# Changelog

All notable changes to Viewlume are documented in this file. The project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.3.0] - 2026-11-21

### Changed

- Group Settings into four tabs with a scrollable body. Apply edits only after Save succeeds.
- Refit fitted images on navigation and resize. Keep manual zoom and pan in place.
- Reduce redundant GPU uploads and apply image cache limit changes without restarting.

### Fixed

- Keep the native Windows resize frame and restore the previous window placement after fullscreen.
- Keep edited settings available if writing the configuration fails.
- Fix cached opens and texture lifetime during rendering. Report images that exceed the GPU texture limit.

### Added

- Add an opaque-window launch option (`VIEWLUME_OPAQUE_WINDOW=1`) and startup diagnostics for renderer compatibility.

## [0.2.2] - 2026-09-15

### Fixed

- Hide the Windows console, improve fullscreen overlay startup, and set the executable icon.

## [0.2.1] - 2026-09-15

### Fixed

- Correct WiX preprocessor argument syntax in the Windows MSI release build.

## [0.2.0] - 2026-09-15

### Added

- Animated GIF and WebP playback with pause, frame progress, and speed control.
- English and Russian interfaces with English as the default language.
- AppImage, DEB, and MSI release packages, file associations, SBOMs, provenance,
  and optional Authenticode signing.
- Recovery actions for failed image loads.

### Changed

- Reject files, dimensions, and animations that could exhaust available memory.
- Invalidate cached images and thumbnails when their source file changes.
- Exercise the built-in WebP, ICO, TGA, DDS, HDR, OpenEXR, Farbfeld, and PNM
  decoders in automated tests.

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

[Unreleased]: https://github.com/vaniley/viewlume/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/vaniley/viewlume/compare/v0.2.2...v0.3.0
[0.2.2]: https://github.com/vaniley/viewlume/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/vaniley/viewlume/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/vaniley/viewlume/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/vaniley/viewlume/releases/tag/v0.1.0
