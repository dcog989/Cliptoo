# Cliptoo

![Cliptoo logo](assets/cliptoo-48.svg) Advanced clipboard manager for Linux KDE. Use your clip history as a library of things. Small footprint, fast performance.

![screen-1](assets/screen-1.webp) ![screen-2](assets/screen-2.webp) ![screen-3](assets/screen-3.webp) ![screen-4](assets/screen-4.webp) ![screen-5](assets/screen-5.webp)

## Features

### Performance

- **Persistent History:** SQLite FTS5 search with prefix matching.
- **Fast Search:** Real-time filtering with match highlighting.
- **Virtualized Scrolling:** only visible rows are rendered.
- **Deduplication:** SHA-256 hashing skips repeat copies.

### Clipboard & Paste

- **Capture:** text, rich text, HTML, images, and file paths.
- **Paste:** Enter pastes via an `evdev` virtual keyboard; optional plain-text mode.
- **Global Hotkey:** toggle the window via keyboard shortcut (default `Ctrl+Alt+Q`).
- **Quick Paste:** hold Right Alt and press `1`–`9`.

### Content

- **Filters:** text, links, images, colors, bookmarks, ++.
- **Images:** hover previews for all common image formats.
- **QR codes:** decode QR images via right-click; copy the text or open the URL.
- **Colors:** displays color swatch for all CSS formats.
- **URLs and Files:** fetched titles and favicons; file size, mtime, and type.
- **Tags:** `##`-prefixed search.
- **Code:** monospace view toggle for code clips.

### Organization & Tools

- **Bookmarks:** exempt from auto-cleanup.
- **Multi-select:** Ctrl+click, Shift+click, Ctrl+A to select; Delete to remove (double click safeguard).
- **Merge:** multi-select then Ctrl+M (or right-click → Merge Selected) joins into one clip.
- **Reordering:** move clips up/down or top/bottom.
- **Transforms:** `upper`, `lower`, `camel`, `kebab`, `snake`, `trim`, `timestamp`, ++.
- **Send To / Compare:** specify external apps or a diff tool.

### Storage & Maintenance

- **Cleanup:** prune by age and/or count, clear caches, remove dead file clips, reclassify.
- **Backup:** JSON export/import, full history or bookmarks only.
- **Statistics:** DB size, clip count, copy/paste counts.

### Customisation

- Light/Dark/System theme, accent color, fonts, row padding, hover-preview delay and size.

### System Integration

- **Tray and autostart.**
- **App blacklist:** ignore selected applications.

---

## Install

Download [GitHub Latest Release](https://github.com/dcog989/Cliptoo/releases/latest):

| Platform | Format | Install |
|----------|--------|---------|
| Linux | AppImage | `chmod +x cliptoo-*-x86_64.AppImage && ./cliptoo-*-x86_64.AppImage` |
| Debian / Ubuntu | `.deb` | `sudo apt install ./cliptoo_*_amd64.deb` |
| Fedora / RHEL | `.rpm` | `sudo dnf install ./cliptoo-*.x86_64.rpm` |
| Arch / CachyOS | `.pkg.tar.zst` | `sudo pacman -U cliptoo-*.pkg.tar.zst` |
| Arch / CachyOS | `PKGBUILD` (local build) | `make package && sudo pacman -U .pkg/cliptoo-*.pkg.tar.zst` |
| Any | source | `make install` (installs to `/usr/local`; see [requirements](#requirements)) |

> [!NOTE]
> The AppImage adds itself to the application menu, installs its icon, and repoints the entry when you move or rename it.

---

## Development

### Tech Stack

| Component | Technology |
|-----------|------------|
| Language | Rust 2024 |
| UI | Slint, Qt 6 backend |
| Clipboard | `wl-clipboard-rs`, `wayland-client`, wlr/ext data-control |
| Storage | SQLite + FTS5 (`rusqlite`) |
| Runtime | Tokio |
| Images | `image`, `resvg`, `jxl-oxide`, `rqrr` |
| Colour | `palette` |
| D-Bus / portals | `ashpd`, `zbus` |
| Tray / input | Slint `SystemTrayIcon`, `evdev` |
| HTTP | `reqwest` (rustls) |

Data lives in `~/.local/share/Cliptoo/` (DB, images), config in `~/.config/Cliptoo/`, cache in `~/.cache/Cliptoo/`, logs in `~/.local/state/Cliptoo/Logs/`.

### Requirements

Wayland with KDE Plasma 6. `xdg-desktop-portal-kde` provides the global hotkey portal and ships with Plasma.

Building needs Rust stable (2024 edition, [rustup](https://rustup.rs)) plus the Qt 6, Wayland, xkbcommon, fontconfig, and dav1d development headers, `clang`, and `mold`:

| Distro        | Command |
|---------------|---------|
| Debian/Ubuntu | `sudo apt install qt6-base-dev qt6-wayland libwayland-dev libxkbcommon-dev libfontconfig1-dev libdav1d-dev pkg-config clang mold` |
| Fedora        | `sudo dnf install qt6-qtbase-devel qt6-qtwayland libwayland-devel libxkbcommon-devel fontconfig-devel dav1d-devel clang mold pkgconfig` |
| Arch          | `sudo pacman -S qt6-base qt6-wayland wayland wayland-protocols libxkbcommon fontconfig dav1d clang mold pkgconf` |

Local builds must use those linkers: set `linker = "clang"` and `rustflags = ["-C", "link-arg=-fuse-ld=mold"]` under `[target.x86_64-unknown-linux-gnu]` in `~/.cargo/config.toml`. CI passes the same values through `CARGO_TARGET_*`.

### Build & Run

```sh
git clone https://github.com/dcog989/cliptoo.git && cd cliptoo
```

```sh
cargo install cargo-edit cargo-outdated cargo-sweep   # optional

cargo run                              # debug build and run
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo fmt --all                        # apply formatting
slint-lsp format -i src/ui/ui/*.slint
make translations                      # rebuild PO catalogs from @tr()

cargo outdated -w
cargo upgrade && cargo update --verbose
cargo sweep --time 14
cargo clean
```

### Release

Releases use [Cocogitto](https://docs.cocogitto.io/) (`cargo install cocogitto` / `sudo pacman -S cocogitto`) and [Conventional Commits](https://www.conventionalcommits.org/), configured in `cog.toml`.

```sh
cog bump --auto               # bump version, sync manifests, write `CHANGELOG.md`, commit, tag, push
cog bump --version 1.23.4     # manual version
cog changelog                 # changelog preview
```

---

## License

[GNU General Public License v3.0](LICENSE).
