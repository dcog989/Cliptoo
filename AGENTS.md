# Agent Directives

## Project Specifics

- Name: Cliptoo
- Description: native background clipboard manager for Wayland / KDE Plasma 6.
- Tech: Rust 2024 workspace — `cliptoo-core` (lib) + `cliptoo` (bin). SQLite/FTS5 (`rusqlite`, bundled), Slint 1.18 (Qt backend), Tokio, `wl-clipboard-rs` capture, `enigo` paste, `zbus` / `ashpd` for tray and portal hotkeys (`org.freedesktop.portal.GlobalShortcuts`).

### Key files

- `src/core/src/` — cliptoo-core: `content/` parser + classifier, `db/` (schema, queries, pool), `settings.rs`, `logger.rs`, `maintenance/`, `color/`, `export.rs`, `transform.rs`.
- `src/ui/src/` — binary and OS integration: `main.rs` (wiring), `hotkeys.rs` (portal), `drag.rs` (only `unsafe`), `clipboard/`, `settings/`, `theme/`.
- `src/ui/ui/*.slint` — windows and components; Rust types come from `build.rs` (`include_modules!`).
- `src/ui/lang/en/LC_MESSAGES/cliptoo.po` — translations; edit by hand (see Gotchas: never run `make translations`).
- `src/core/tests/` — integration tests (temp dirs, no fixtures).
- `packaging/PKGBUILD`, `Makefile`, `lefthook.yml`, `cog.toml`, `scripts/sync_version.sh`.

### Workflow

- Install: `make install` (runs sudo; confirm first)
- Dev: `cargo run`
- Test: `cargo test --workspace`
- Lint: `cargo clippy --workspace --all-targets -- -D warnings`
- Format: `cargo fmt --all` and `slint-lsp format -i src/ui/ui/*.slint`
- Build: `cargo build --release -p cliptoo`; package: `make package`

### Common Patterns

- New Slint callback or property: declare it in the `.slint` file, register the handler in a `setup_*()` fn under `src/ui/src/`, call that fn from `main.rs`. Inside handlers use `ui.as_weak()`; Slint properties are UI-thread only.
- DB access: `Arc<DbPool>` + `db.with(|conn| ...)`; add queries to `src/core/src/db/queries.rs`, migrations to `schema.rs`.
- Shared state: `Settings` is `Rc<RefCell<Settings>>` on the UI thread. Background tasks get `Arc<Mutex<_>>`, `tokio::sync::watch`, or `mpsc` channels — never the Slint handle's properties.
- Multi-window: each Slint window has its own `Theme` global; register it via `theme::register_*_theme_filler` so live theme changes reach it.
- Strings: wrap user-facing text in `@tr()`; `msgctxt` must match the enclosing component or global name.

### Gotchas

- `#![deny(unsafe_code)]` in both crates; the sole exception is `src/ui/src/drag.rs` (Qt FFI).
- `reqwest` uses `rustls-no-provider`: `main.rs` installs the ring provider before any client is built.
- `zbus` is pinned to 5 to match `ashpd`; bump both together.
- **Never run `make translations` or `slint-tr-extractor`.** The extractor exhausts RAM and swap (a 2 GB memory cap still thrashes) and the Makefile truncates `cliptoo.po` before extraction, so a killed run leaves the catalog at 0 bytes. Add or update `.po` entries by hand. Only consider the extractor again after confirming a fixed version and running it against a copy with an explicit memory cap.

### Tools

- File tools (glob, grep, read, write, edit, shell) are called directly, never as `tools.*` inside execute. execute exposes only `browser.*`, `opencode.*`, and search results.

### File System Access

- Allowed: repository root and all contained files; `/tmp/opencode/*`.
- Read-Only: `.env*`, `.git/`, `.docs/`.
- Disallowed: everything else unless the user grants permission.
- Require confirmation: dependency changes, `sudo` / install / package commands, any operation outside the repo.
- Do not delete files or make destructive changes without confirmation.

### Verification

- Do not run test, lint, format, or type-check commands unless the user explicitly asks.

### Release (Cocogitto)

Cocogitto is an external binary (`cargo install cocogitto` / `pacman -S cocogitto`); config in `cog.toml` (`tag_prefix = "v"`).

- `cog bump --auto` — bump from conventional commits, sync manifests (`scripts/sync_version.sh`), write `CHANGELOG.md`, commit, tag, push. `pre_bump_hooks` run tests + clippy first.
- Manual version: `cog bump --version 1.2.3`. Preview: `cog changelog`.
- Commit messages verified by `cog verify` (Merge/Revert lines bypass).

### Slint (mandatory)

1. Before writing Slint, read `.docs/.slint-docs/slint-docs-flat/gotchas.md` → `language-and-layout.md` → `interop.md`; never guess syntax. Widget/element properties + callbacks: `INDEX.md`.
2. Rust FFI: grep `interop.md` or fetch `https://docs.slint.dev/latest/docs/slint/<path>.md`. Child windows use `activate_window<C: slint::ComponentHandle>(ui: &C)`; soundness invariants in `src/ui/src/drag.rs`.
3. Slint 1.18: `WindowMoveArea`, `FlexboxLayout`, dynamic `z`, `spring` easing, model `push`/`remove`/`insert`, `string.starts-with`/`ends-with`/`replace-all` available. `Flickable.viewport-*` renamed to `content-*` (old names deprecated). Changelog: `.docs/.slint-docs/slint.1.18.md`.

---

## General Guidelines

### Code Changes

- For non-trivial work, propose an approach and confirm before implementing, including architectural changes.
- Keep modifications minimal and scoped; prefer incremental improvements over rewrites.
- Use explicit types and named constants (no magic numbers).
- Return explicit error types; do not suppress exceptions.
- Follow standard repository linting and formatting configs.
- Decompose files over 400 lines if they mix concerns.
- Name things clearly; reserve comments for the non-obvious why (workarounds, edge cases).
- Never run git mutations (commit, push, reset, rebase, amend) unless explicitly instructed.
- Do not create documentation files unless explicitly requested.
- Adhere to <https://develop.kde.org/hig/>.

### Author Environment

- Ryzen 5800X3D, Radeon RX 7900 XT, Display (AW3225QF); CachyOS, KDE, KWin / Wayland, Btrfs.
- fish shell, Ghostty terminal, Fresh TUI editor, yay package manager, bun npm manager, Firefox, Zed code editor.

### Testing

- Add tests only when the logic is genuinely testable and worth guarding. No test files for trivial changes or behavior the environment cannot test reliably (e.g. UI layout/click mapping).

### Definition of Done

- Logic fully implemented.
- Existing docs updated if public interfaces changed.
- Run the corresponding `Workflow` command when `Verification` requires it.

### Commit Messages

- On completion of an update or fix, print a conventional commit message in a fenced code block.
- Add a body only when the "why" isn't obvious.
- Never hard-wrap: one physical line per paragraph, however long.

### Communication Style

- Format: short declarative sentences, one idea per line.
- Instructions: numbered steps, one action each (exact command, path, or code); no prose recap.
- Explanations: essential context only. End question answers with `TL;DR:`, one sentence; omit for status updates and completed work.
- Tone: no preamble, restating, hedging, disclaimers, emotional framing, or closing offers. Keep every fact, command, flag, path, default, caveat.
- Edge Cases: state risks explicitly; ask when requirements are ambiguous.
