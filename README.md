# Cliptoo

- Advanced clipboard manager.
- Cross-platform, Linux-first.
- Super fast, super slim, packed with features.
- Rust / Slint / rusqlite - handles thousands of clips without slowing down.
- Store and categorise clips, links, files, images, color swatches, code snippets.
- Use your clip history as a library.

---

## Features

### Performance

- **Persistent History:** SQLite with FTS5 full-text search.
- **Fast Search:** Real-time filtering with match highlighting on thousands of clips.
- **Virtualized Scrolling:** O(1) visible-row rendering regardless of list size.
- **Paste Suppression:** SHA-256 dedup discards self-paste events.

### Clipboard & Paste

- **Clipboard Capture:** Text, images (PNG/JPEG/WebP/BMP/TIFF), file URIs.
- **Paste Emulation:** Select clip → Enter → content pasted via `enigo`.
- **Global Hotkeys:** System-wide shortcuts via XDG Portal `GlobalShortcuts`.
- **Quick Paste Overlay:** Just hit the number pad to select and paste.

### Content Intelligence

- **Content-Aware Filtering:** Filter by text, links, images, colors, bookmarks, etc.
- **Image Previews:** Hover thumbnails for PNG, JPEG, WebP, AVIF, JXL, SVG.
- **Color Swatches:** `#hex`, `rgb()`, `hsl()`, `oklch()` - all with transparency.
- **Code Highlighting:** Syntax-highlighted editor via `syntect`.
- **URL Metadata:** Auto-fetches page titles and favicons.
- **File Info:** Size, modification date, type classification.

### Organization & Tools

- **Bookmarks:** Make clips permanent, avoid auto-cleanup.
- **Text Transformations:** E.g. `upper`, `lower`, `camel`, `kebab`, `strip whitespace`, etc.
- **"Send To":** Send clips to external programs, text diff tool integration.

### Storage & Maintenance

- **Auto-Cleanup:** Prune by age and / or count.
- **Clear Caches:** Reclaim disk space for thumbnails and metadata.
- **Deadhead:** Remove clips referencing deleted files.
- **Reclassify:** Re-run content classification on stored clips.
- **Export/Import:** Portable JSON backup and restore.
- **DB Statistics:** Total clips, paste count, database size.

### Customisation

- **Theming:** Light/Dark/System, custom accent colors (OKLCH-perceptually uniform).
- **Typography:** Font family, size, row padding (Compact/Standard/Luxury).

### System Integration

- **System Tray:** StatusNotifierItem tray with Show/Settings/Quit menu.

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
git clone https://github.com/dcog989/cliptoo.git && cd cliptoo
cargo build --release -p cliptoo
cargo run
./target/release/cliptoo
```

### Development

```sh
cargo install cargo-outdated        # initialize outdated tool

cargo build                         # debug build
cargo check                         # type-check only
cargo clippy --workspace --all-targets -- -D warnings
cargo fetch && cargo outdated -w    # check for updates above semver range
cargo fmt --all --check             # formatting
cargo run                           # build and run
cargo test --workspace              # tests
cargo update --verbose              # update Cargo.lock within semver ranges

cargo clean && rm -rf target/       # clean build artifacts
```

## Local Dev Install

```sh
cd /home/bubba/Projects/Cliptoo
cargo build --release -p cliptoo
sudo install -Dm755 target/release/cliptoo /usr/local/bin/cliptoo
sudo install -Dm644 packaging/cliptoo.desktop /usr/share/applications/cliptoo.desktop
```

## License

MIT
