# Agent Directives

**IMPERATIVE** — read the Slint docs bundle (see Slint rules below) before writing any Slint code; never guess syntax.

## Project

- Cliptoo: native background clipboard manager for Wayland/KDE Plasma 6. Rust + SQLite (rusqlite) + Slint (Qt backend). Handles thousands of records without blocking the UI.
- Workspace dep versions in `Cargo.toml` — check before adding.
- Key files: `src/core/` (cliptoo-core: parser, db, settings, logger), `src/ui/` (binary + OS integration), `src/ui/ui/*.slint` (components), `src/ui/src/main.rs` (entry), `src/ui/src/hotkeys.rs` (hotkey rationale), `.docs/HLD.md`, `.docs/PORTING.md`, `.docs/Progress.md` (read first), `.docs/ToDo.md`, `packaging/PKGBUILD`, `lefthook.yml`.

## Build / Check

**NEVER run `cargo build` unless the user explicitly asks.** No verification
builds, no "fallback" builds — for any change, trivial or not. If the user
wants a build, they will ask for it. The check commands below are run at the
agent's discretion per the trivial-change rule. Reference:

```sh
cargo build --release -p cliptoo   # production build
cargo check                        # fast type-check
cargo test --workspace             # tests in src/core only
cargo fmt --all -- --check         # formatting
cargo clippy --workspace --all-targets -- -D warnings
```

Hooks (lefthook): fmt + clippy on `.rs`, `slint-lsp format -i` on `.slint`; pre-push: `cargo test --workspace`.

## File System Access

- Root: `/home/bubba/Projects/Cliptoo/`
- Allowed: all subdirectories
- Read-Only: `.env*`, `.git/`, `~/.cargo/registry/src/` (read freely — full source, `.rs` and `.slint` alike, for checking library internals)
- Disallowed: `.assets/`, `.docs/ToDo.md`, `.git/`, `node_modules/`, `.docs/archive/`
- Require confirmation: dependency add/remove, changes outside `src/`, anything outside project root

## Rules

- Minimal, scoped changes; incremental over rewrites. Ask before architectural changes. No destructive changes or new doc files without confirmation.
- Never run git mutations (commit/push/reset/rebase/amend) unless explicitly asked.
- After each change, print a one-line commit message (e.g. `fix:`) in a code block.
- Style: explicit types + named constants (no magic numbers); `cargo fmt` defaults; self-documenting names; comments only for WHY.
- Errors: `anyhow::Result` at boundaries, `thiserror` enums in `src/core/`, `.context(...)` across module boundaries; never suppress errors.
- Logging: `tracing::{info,warn,error,debug}` only (never `println!`/`dbg!`); init in `core/src/logger.rs`.
- No `unsafe` except the Qt FFI shim `src/ui/src/drag.rs` (`#![allow(unsafe_code)]`); crate roots `#![deny(unsafe_code)]`. Do not introduce `unsafe` elsewhere.
- Decompose files over 400 lines if they mix concerns.
- Wayland-only (no X11 fallback); hotkeys via `org.freedesktop.portal.GlobalShortcuts`.
- **NEVER** run `cargo check`/`clippy`/`fmt --check`/`cargo test`/`slint-viewer` for trivial changes (single-line edits, string/field removal, UI copy/color/structure tweaks, reordering items). Verify nothing; just make the edit and report the commit message. The lefthook pre-commit hooks (fmt + clippy on `.rs`, `slint-lsp format` on `.slint`) catch regressions on the next commit, so hand-running checks adds no value.
- Do NOT run `cargo test` after every change. Reserve it for changes that touch tested logic (new/changed code paths or tests in `src/core`, new unit tests) or when the user asks. `cargo clippy` is the default verification for Rust changes — it type-checks and catches Slint compile errors without the test-suite overhead. Run the full `cargo test --workspace` once per task when warranted, not per edit.
- **NEVER** run `cargo build` for any change — trivial or not — unless the user explicitly instructs it. No "fallback" builds when `slint-viewer` is missing, no self-verification builds. This rule overrides Slint rule #4 and Definition of Done.

## Slint rules (mandatory)

1. Read `gotchas.md` → `language-and-layout.md` → `interop.md` in `.docs/.slint-docs/slint-docs-flat/`.
2. Look up the exact widget/element in `INDEX.md` for its properties and callbacks.
3. Rust FFI patterns: grep `interop.md` or fetch `https://docs.slint.dev/latest/docs/slint/<path>.md`.
4. After editing: suggest `slint-viewer --check path/to/file.slint`; never declare UI work done without verifying it renders.
5. 1.18 note: `Tooltip` and `SystemTrayIcon` (via `inherits SystemTrayIcon`) are available; `.docs/slint.1.17.md` changelog still relevant.

## Communication

- Concise, precise, no analogies or apologies; answer the question asked; don't volunteer next steps.

## Definition of Done

- Logic fully implemented; new/modified features have tests.
- `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` pass.
- Docs updated if public interfaces changed; UI verified with `slint-viewer --check`.
