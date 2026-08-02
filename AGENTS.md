# Agent Directives

**IMPERATIVE** — you MUST read the local Slint docs bundle before writing any Slint code. Guessing Slint syntax produces wrong code consistently. See Slint rules below.

## Project Context

- Name: Cliptoo
- Description: Native, background-running clipboard manager for Linux desktops (Wayland/KDE Plasma 6). Rust + SQLite + Slint (Qt backend). Handles many thousands of clipboard records without blocking the UI thread.
- Tech: Rust, SQLite (rusqlite), Slint 1.17, tracing, anyhow/thiserror, Wayland (xdg-desktop-portal for global hotkeys)

## Key Files

- `Cargo.toml` — workspace root, dep versions
- `src/core/` — cliptoo-core library: parser, db, settings, logger
- `src/ui/` — binary + OS integration (clipboard, hotkeys, tray, paste, theme)
- `src/ui/ui/*.slint` — UI components
- `src/ui/src/hotkeys.rs` — global hotkey rationale / graceful degradation
- `src/ui/src/main.rs` — entry point
- `.docs/HLD.md` — high-level design
- `.docs/PORTING.md` — mapping from C#/WPF original
- `.docs/Progress.md` — what's done (read first when picking up work)
- `.docs/ToDo.md` — outstanding tasks
- `packaging/PKGBUILD` — Arch packaging
- `lefthook.yml` — pre-commit/pre-push hooks

## Dev Environment

- CachyOS, Limine bootloader, KDE Plasma 6, Wayland, and Btrfs.
- fish shell, Ghostty terminal, Fresh TUI editor, yay package manager, bun npm manager, Firefox, and Zed code editor.

## Development Workflow

```bash
cargo build --release -p cliptoo   # production build
cargo check                        # fast type-check
cargo test --workspace             # tests in src/core only
cargo fmt --all -- --check         # formatting
cargo clippy --workspace --all-targets -- -D warnings
```

Pre-commit hooks (lefthook): fmt + clippy on `.rs` files, `slint-lsp format -i` on `.slint` files.
Pre-push hooks: `cargo test --workspace`.

## File System Access

- Root: `/home/bubba/Projects/Cliptoo/`
- Allowed: All subdirectories
- Read-Only: `.env*`, `.git/`, `~/.cargo/registry/src/**/*.slint`
- Disallowed: `.assets/`, `.docs/ToDo.md`, `.git/`, `node_modules/`, `.docs/archive/`
- Require confirmation: adding/removing dependencies, changes outside `src/`, any operation outside project root

## Rules

- Keep modifications minimal and scoped. Ask before architectural changes.
- Do not delete files or make destructive changes without confirmation.
- Do not create documentation files unless explicitly requested.
- Prefer incremental improvements over rewrites.
- Do not run full checks (`cargo build`, `cargo clippy`, `cargo fmt --check`, `cargo test`) for trivial changes (single-line edits, string/field removal, UI copy tweaks). Run them only for non-trivial logic changes.
- After each completed fix/update, provide a concise single line commit message with e.g. `fix:`, printed to a code block to allow the user to copy it.
- Use explicit types and named constants (no magic numbers).
- Return explicit error types; do not suppress exceptions.
- `cargo fmt` defaults. No custom rustfmt config.
- Errors: `anyhow::Result` at boundaries, `thiserror` enums in `src/core/`. Use `.context(...)` on fallible ops crossing a module boundary.
- Logging: `tracing::{info,warn,error,debug}` only (never `println!`, never `dbg!`). Initialised in `core/src/logger.rs`.
- `unsafe` is **not used** in this codebase, with a single carve-out: the Qt FFI shim in `src/ui/src/drag.rs` (module-level `#![allow(unsafe_code)]`). Both crate roots `#![deny(unsafe_code)]`, so it cannot spread. Do not introduce `unsafe` anywhere else.
- Decompose files over 400 lines if they mix concerns.
- Never run git mutations (commit, push, reset, rebase, amend) unless explicitly asked.
- Self-documenting code via clear naming. Comments only for WHY (non-obvious platform gotchas, upstream bug workarounds, security boundaries, magic constants). Never restate the code.
- Wayland-only. No X11 fallback. KDE Plasma 6 desktop — global hotkeys go through `org.freedesktop.portal.GlobalShortcuts`.
- Check the workspace `Cargo.toml` before adding any dependency.

### Slint rules (mandatory — do NOT guess syntax)

1. Read `.docs/.slint-docs/slint-docs-flat/gotchas.md` first
2. Read `.docs/.slint-docs/slint-docs-flat/language-and-layout.md`
3. Read `.docs/.slint-docs/slint-docs-flat/interop.md`
4. Find the widget/element in `.docs/.slint-docs/slint-docs-flat/INDEX.md` and read its doc page for exact properties and callbacks
5. For Rust FFI patterns: grep `interop.md` or fetch from `https://docs.slint.dev/latest/docs/slint/<path>.md`
6. After editing: suggest `slint-viewer --check path/to/file.slint` for compile diagnostics
7. Never declare UI work done without verifying it renders

Slint 1.18 note: `Tooltip` element and `SystemTrayIcon` (via `inherits SystemTrayIcon`) are available. See `.docs/slint.1.17.md` for the 1.17 changelog (still relevant).

## Communication Style

- Concise, precise, no analogies, no apologies.
- Answer the question asked. Don't prompt the next step or volunteer unrequested suggestions.
- Do not pretend to understand how the user feels.

## Definition of Done

- Logic fully implemented.
- `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` pass with zero errors.
- New/modified features have tests.
- Existing docs updated if public interfaces changed.
- For UI work: verified with `slint-viewer --check`.
