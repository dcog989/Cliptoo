# Cliptoo

Advanced clipboard manager for KDE Plasma 6 / Wayland. Rust backend, Slint UI. <20 MB RAM, handles 10k+ records without blocking the UI thread.

> Rust/Slint rewrite of the original WindowsC#/.NET version. See [HLD.md](.docs/HLD.md).

---

## Features

- **Persistent History:** SQLite with FTS5 full-text search.
- **Fast Search:** Real-time filtering with match highlighting on thousands of clips.
- **Content-Aware Filtering:** Filter by text, links, images, colors, bookmarks.
- **Bookmarks:** Keep items; they bypass auto-cleanup.
- **Virtualized Scrolling:** O(1) visible-row rendering regardless of list size.
- **Image Previews:** Hover thumbnails for PNG, JPEG, WebP, AVIF, JXL, SVG.
- **Color Swatches:** `#hex`, `rgb()`, `hsl()`, `oklch()` with transparency.
- **Code Highlighting:** Syntax-highlighted editor via `syntect`.
- **URL Metadata:** Auto-fetches page titles and favicons.
- **File Info:** Size, modification date, type classification.
- **Global Hotkeys:** System-wide shortcuts via XDG Portal `GlobalShortcuts`.
- **Paste Emulation:** Select clip → Enter → content pasted via `enigo`.
- **Paste Suppression:** SHA-256 dedup discards self-paste events.
- **Clipboard Capture:** Text, images (PNG/JPEG/WebP/BMP/TIFF), file URIs.
- **System Tray:** StatusNotifierItem tray with Show/Settings/Quit menu.
- **Theming:** Light/Dark/System, custom accent colors (OKLCH-perceptually uniform).
- **Typography:** Font family, size, row padding (Compact/Standard/Luxury).
- **Launch Position:** Cursor, center, screen edges, fixed coordinates.
- **Auto-Cleanup:** Prune by age or count.
- **Clear Caches:** Reclaim disk space for thumbnails and metadata.
- **Deadhead:** Remove clips referencing deleted files.
- **Reclassify:** Re-run content classification on stored clips.
- **Export/Import:** Portable JSON backup and restore.
- **DB Statistics:** Total clips, paste count, database size.
- **Quick Paste overlay:** Just hit the number pad to select and paste.
- **Text Transformations:** E.g. `upper`, `lower`, `camel`, `kebab`, `strip whitespace`, etc.
- **"Send To":** Send clips to external programs, text diff tool integration.

## Tech Stack

  | Component           | Technology                                |
  |---------------------|-------------------------------------------|
  | Language            | Rust (edition 2024)                       |
  | UI Framework        | Slint 1.17 (Qt 6 backend)                 |
  | Window System       | Wayland (wlr-data-control, KDE Plasma 6)  |
  | Database            | SQLite + FTS5 (`rusqlite`)                |
  | Async Runtime       | Tokio                                     |
  | Image Processing    | `image`, `resvg`, `jxl-oxide`             |
  | Syntax Highlighting | `syntect`                                 |
  | Colour Science      | Custom OKLCH→sRGB (Björn Ottosson spec)   |
  | D-Bus / Portals     | `zbus`                                    |
  | System Tray         | D-Bus StatusNotifierItem via `zbus`       |
  | Input Emulation     | `enigo`                                   |
  | HTTP Client         | `reqwest` (rustls)                        |

## Building

### Prerequisites

- **Rust toolchain** (stable): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Qt 6 + Wayland dev headers:**

| Distro       | Command                                                                     |
|--------------|-----------------------------------------------------------------------------|
| Debian/Ubuntu| `sudo apt install qt6-base-dev qt6-wayland libwayland-dev libxkbcommon-dev pkg-config` |
| Fedora       | `sudo dnf install qt6-qtbase-devel qt6-qtwayland libwayland-devel libxkbcommon-devel pkgconfig` |
| Arch         | `sudo pacman -S qt6-base qt6-wayland wayland wayland-protocols libxkbcommon pkgconf` |

### Build & Run

```sh
git clone https://github.com/dcog989/cliptoo-next.git && cd cliptoo-next
cargo build --release -p cliptoo
./target/release/cliptoo
```

### Development

```sh
cargo install cargo-outdated        # initialize outdated tool

cargo check                         # type-check only
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace              # tests
cargo fmt --all --check             # formatting
cargo outdated -w                   # check for updates above semver range
cargo update --verbose              # update Cargo.lock within semver ranges
cargo build                         # debug build
cargo run                           # build and run

cargo clean && rm -rf target/       # clean build artifacts
```

### Packages

- **.deb:** `cargo deb -p cliptoo --no-build` (requires `cargo-deb`)
- **.rpm:** `cargo generate-rpm -p src/ui` (requires `cargo-generate-rpm`)

## Docs

- [High-Level Design](.docs/HLD.md) — architecture, data flow, protocols.
- [Progress](.docs/Progress.md) — conversion progress checklist.
- [Slint](.docs/.slint-docs) — Slint reference docs

## License

MIT
