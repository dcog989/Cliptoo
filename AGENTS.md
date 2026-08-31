# Agent Directives

## Project Specifics

Cliptoo: native background clipboard manager for Wayland/KDE Plasma 6. Rust + SQLite (rusqlite) + Slint (Qt backend). Wayland-only (no X11 fallback); hotkeys via `org.freedesktop.portal.GlobalShortcuts`. Workspace dep versions in `Cargo.toml` — check before adding or using a crate.

Key files: `src/core/` (cliptoo-core: parser, db, settings, logger), `src/ui/` (binary + OS integration), `src/ui/ui/*.slint`, `src/ui/src/main.rs`, `src/ui/src/hotkeys.rs`, `.docs/HLD.md`, `.docs/PORTING.md`, `.docs/Progress.md` (read first), `.docs/ToDo.md`, `packaging/PKGBUILD`, `lefthook.yml`.

### Verification (critical)

**Never run `cargo check/clippy/test/build/fmt` or `slint-viewer` unless the user explicitly asks.** The user tests their own changes; lefthook hooks (fmt + clippy on `.rs`, `slint-lsp format -i` on `.slint`, pre-push `cargo test --workspace`) catch regressions. Reference only:

```sh
cargo build --release -p cliptoo   # production build
cargo check                        # fast type-check
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

### Release (Cocogitto)

- `make release` (`cog bump --auto`) — external Rust binary (`cargo install cocogitto` / `pacman -S cocogitto`); config in `cog.toml`, `tag_prefix = "v"`. Bumps version from conventional commits, syncs manifests via `scripts/sync_version.sh`, writes `CHANGELOG.md` (template `changelog.tpl`), commits/tags/pushes (post_bump_hooks). Manual: `make version V=1.2.3`.
- Changelog preview: `make changelog` (`cog changelog`).
- Commit-msg enforced by `lefthook.yml` via `cog verify` (Merge/Revert lines bypass).

### File System

- Root: `/home/bubba/Projects/Cliptoo/`; all subdirs allowed, confirm anything outside root.
- Read-only: `.env*`, `.git/`, `~/.cargo/registry/src/` (read freely for library source).
- Disallowed: `.assets/`, `.docs/ToDo.md`, `.git/`, `node_modules/`, `.docs/archive/`.
- Confirm before: adding/removing deps, changes outside `src/`.

### Rules

- **Style:** explicit types + named constants (no magic numbers); `cargo fmt` defaults; self-documenting names; comments only for WHY.
- **Errors:** `anyhow::Result` at boundaries, `thiserror` enums in `src/core/`, `.context(...)` across module boundaries; never suppress errors.
- **Logging:** `tracing::{info,warn,error,debug}` only (never `println!`/`dbg!`); init in `core/src/logger.rs`.
- **No `unsafe`** except the Qt FFI shim `src/ui/src/drag.rs` (`#![allow(unsafe_code)]`); crate roots `#![deny(unsafe_code)]`.

### Rust FFI

Generic over `ComponentHandle` for child windows: `activate_window<C: slint::ComponentHandle>(ui: &C)`. Soundness invariants in `src/ui/src/drag.rs`.

### Slint (mandatory)

1. Read `.docs/.slint-docs/slint-docs-flat/gotchas.md` → `language-and-layout.md` → `interop.md` before writing Slint; never guess syntax.
2. Look up the widget/element in `INDEX.md` for properties and callbacks.
3. Rust FFI: grep `interop.md` or fetch `https://docs.slint.dev/latest/docs/slint/<path>.md`.
4. Slint 1.18: `Tooltip` and `SystemTrayIcon` (via `inherits SystemTrayIcon`) are available; `.docs/slint.1.17.md` changelog still relevant.

---

## General Guidelines

### Code Changes

- Keep modifications minimal and scoped; prefer incremental improvements over rewrites. Ask before architectural changes.
- Use explicit types and named constants (no magic numbers).
- Return explicit error types; do not suppress exceptions.
- Follow standard repository linting and formatting configs.
- Decompose files over 400 lines if they mix concerns.
- Self-documenting code via clear naming. Use comments only for complex workarounds or issues that need noting — why, not what.
- Never run git mutations (commit, push, reset, rebase, amend) unless explicitly instructed.
- Do not create documentation files unless explicitly requested.

### Verification

- Do not run test, lint, clippy, biome, format, or type-check commands. The user builds, tests, and lints manually.
- Exception to above: run them for major refactors, or when the user explicitly asks.

### Dev Environment

- CachyOS, KDE Plasma 6, Wayland, Btrfs.
- fish shell, Ghostty terminal, Fresh TUI editor, yay package manager, bun npm manager, Firefox, and Zed code editor.
- All software is up to date as of today.

### Testing

- Do not create test files for trivial changes, or for behavior that is not reliably unit-testable in the test environment (e.g. UI layout/click mapping). Prefer no new files; only add a test when the logic is genuinely testable and worth guarding.

### Definition of Done

- Logic fully implemented.
- Existing docs updated if public interfaces changed.
- On completion of an update or fix, print a concise conventional commit message in a fenced code block.

### Communication Style

- Provide concise, actionable responses.
- Ask clarifying questions when requirements are ambiguous.
- Flag potential risks or edge cases proactively.
- Do not pretend to understand how the user feels.
- Never editorialise your answer. No "to be honest", "honestly", hedging, disclaimers, or meta-commentary — just answer.
