# Agent Directives

## Verification (critical)

**NEVER run `cargo check`, `cargo clippy`, `cargo test`, `cargo build`, `cargo fmt`, or `slint-viewer` unless the user explicitly asks.** The user tests their own changes. The lefthook pre-commit hooks (fmt + clippy on `.rs`, `slint-lsp format -i` on `.slint`, pre-push `cargo test --workspace`) catch regressions. Reference only:

```sh
cargo build --release -p cliptoo   # production build
cargo check                        # fast type-check
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

## Project

Cliptoo: native background clipboard manager for Wayland/KDE Plasma 6. Rust + SQLite (rusqlite) + Slint (Qt backend). Wayland-only (no X11 fallback); hotkeys via `org.freedesktop.portal.GlobalShortcuts`.

- Workspace dep versions in `Cargo.toml` — check before adding or using a crate.
- Key files: `src/core/` (cliptoo-core: parser, db, settings, logger), `src/ui/` (binary + OS integration), `src/ui/ui/*.slint`, `src/ui/src/main.rs`, `src/ui/src/hotkeys.rs`, `.docs/HLD.md`, `.docs/PORTING.md`, `.docs/Progress.md` (read first), `.docs/ToDo.md`, `packaging/PKGBUILD`, `lefthook.yml`.

## File System

- Root: `/home/bubba/Projects/Cliptoo/`. All subdirs allowed.
- Read-only: `.env*`, `.git/`, `~/.cargo/registry/src/` (read freely for library source).
- Disallowed: `.assets/`, `.docs/ToDo.md`, `.git/`, `node_modules/`, `.docs/archive/`.
- Confirm before: adding/removing deps, changes outside `src/`, anything outside project root.

## Rules

- **Minimal, scoped changes.** Incremental over rewrites. Ask before architectural changes. No new doc files without confirmation.
- **Never run git mutations** (commit/push/reset/rebase/amend) unless explicitly asked.
- **Style:** explicit types + named constants (no magic numbers); `cargo fmt` defaults; self-documenting names; comments only for WHY.
- **Errors:** `anyhow::Result` at boundaries, `thiserror` enums in `src/core/`, `.context(...)` across module boundaries; never suppress errors.
- **Logging:** `tracing::{info,warn,error,debug}` only (never `println!`/`dbg!`); init in `core/src/logger.rs`.
- **No `unsafe`** except the Qt FFI shim `src/ui/src/drag.rs` (`#![allow(unsafe_code)]`); crate roots `#![deny(unsafe_code)]`. Do not introduce `unsafe` elsewhere.
- **Decompose** files over 400 lines if they mix concerns.
- **Commit message:** on completion, print a concise conventional commit message in a fenced code block.

## Rust FFI

Generic over `ComponentHandle` for child windows: `activate_window<C: slint::ComponentHandle>(ui: &C)`. Soundness invariants in `src/ui/src/drag.rs`.

## Slint (mandatory)

1. Read `.docs/.slint-docs/slint-docs-flat/gotchas.md` → `language-and-layout.md` → `interop.md` before writing Slint; never guess syntax.
2. Look up the widget/element in `INDEX.md` for properties and callbacks.
3. Rust FFI: grep `interop.md` or fetch `https://docs.slint.dev/latest/docs/slint/<path>.md`.
4. Slint 1.18: `Tooltip` and `SystemTrayIcon` (via `inherits SystemTrayIcon`) are available; `.docs/slint.1.17.md` changelog still relevant.

## Communication

- Concise, actionable responses.
- Ask clarifying questions when ambiguous.
- Flag risks and edge cases proactively.

## Definition of Done

- Logic fully implemented; new or modified features have tests (in `src/core/`).
- Doc comments updated if public interfaces changed.
- Print a conventional commit message.
