# Viewlume

[![CI](https://github.com/vaniley/viewlume/actions/workflows/ci.yml/badge.svg)](https://github.com/vaniley/viewlume/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Viewlume is a fast, GPU-accelerated image viewer for Linux and Windows. It keeps the image in focus with a transparent fullscreen mode, fluid zoom and pan, keyboard navigation, and a lightweight thumbnail carousel.

## Highlights

- GPU rendering through `egui`, `eframe`, and `wgpu`.
- Background decoding, bounded thumbnail work, adjacent-image prefetching, and RAM caches.
- Visual size and on-screen position preserved when switching between different resolutions.
- Cursor-centered zoom and short, configurable pan inertia.
- Frameless fullscreen overlay with auto-hiding controls.
- Scrollable cover-flow carousel with virtualized thumbnails.
- Natural filename sorting and EXIF orientation support.
- Configurable transitions, filtering, cache limits, navigation, and carousel behavior.

## Supported formats

JPEG, PNG, WebP, BMP, TIFF, QOI, and the first frame of GIF images.

## Install

Download the archive for your platform from [GitHub Releases](https://github.com/vaniley/viewlume/releases), extract it, and run `viewlume` (`viewlume.exe` on Windows). You can pass an image or directory as the first argument:

```console
viewlume path/to/image.png
viewlume path/to/folder
```

## Controls

| Input | Action |
| --- | --- |
| `←` / `A`, `→` / `D` | Previous or next image |
| `Home`, `End` | First or last image in the folder |
| Mouse wheel | Zoom toward the pointer |
| Mouse wheel over carousel | Scroll thumbnails horizontally |
| Drag with left, middle, or right mouse button | Pan image |
| Double click | Configured fullscreen/fit action |
| `F` / `0` | Fit image to viewport |
| `1` | Actual size (100%) |
| `F11` | Toggle fullscreen overlay |
| `T` | Cycle carousel visibility |
| `N` | Cycle texture filtering |
| `I` | Toggle image information |
| `O` | Open an image |
| `S` | Open settings |
| `Esc` | Close settings or leave fullscreen |

Files can also be opened with drag and drop.

## Build from source

Install the stable Rust toolchain, then run:

```console
cargo build --release --locked
```

The executable is written to `target/release/viewlume` on Linux or `target/release/viewlume.exe` on Windows.

Linux builds may require the Wayland/X11 development packages supplied by the distribution. On Ubuntu:

```console
sudo apt-get install libwayland-dev libxkbcommon-dev
```

## Configuration

Viewlume stores settings in the platform configuration directory:

- Linux: `~/.config/viewlume/config.toml`
- Windows: `%APPDATA%\vaniley\viewlume\config\config.toml`

An existing configuration from the former `image-viewer` name is migrated automatically on first launch.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Security reports should follow [SECURITY.md](SECURITY.md).

## License

Viewlume is available under the [MIT License](LICENSE).
