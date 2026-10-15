# Agent Directives

## Project Specifics

Cliptoo: native background clipboard manager for Wayland/KDE Plasma 6. Rust + SQLite (rusqlite) + Slint (Qt backend). Wayland-only (no X11 fallback); hotkeys via `org.freedesktop.portal.GlobalShortcuts`. Workspace dep versions in `Cargo.toml` — check before adding or using a crate.

### Key files

- `src/core/` — cliptoo-core: parser, db, settings, logger
- `src/ui/` — binary + OS integration (`main.rs`, `hotkeys.rs`)
- `src/ui/ui/*.slint`
- `packaging/PKGBUILD`, `lefthook.yml`, `Makefile`

### Verification (critical)

**Never run `cargo check/clippy/test/build/fmt` or `slint-viewer` unless the user explicitly asks.** The user tests their own changes; Lefthook (fmt + `slint-lsp format -i` on staged files, biome on `website/`, pre-push `cargo test` + `clippy`) catches regressions. Reference only:

```sh
cargo build --release -p cliptoo   # production build
cargo check                        # fast type-check
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

### Release (Cocogitto)

Cocogitto is an external binary (`cargo install cocogitto` / `pacman -S cocogitto`); config in `cog.toml` (`tag_prefix = "v"`).

- `cog bump --auto` — bump from conventional commits, sync manifests (`scripts/sync_version.sh`), write `CHANGELOG.md`, commit, tag, push. `pre_bump_hooks` run tests + clippy first.
- Manual version: `cog bump --version V=1.2.3`. Preview: `cog changelog`.
- Commit messages verified by `cog verify` (Merge/Revert lines bypass).

### File System

- Root: `/home/bubba/Projects/Cliptoo/`; all subdirs allowed, confirm anything outside root.
- Read-only: `.env*`, `.git/`, `~/.cargo/registry/src/` (library source).
- Disallowed: `.assets/`, `.docs/ToDo.md`, `.docs/archive/`, `node_modules/`.
- Confirm before: adding/removing deps, changes outside `src/`.

### Rules

- **Errors:** `anyhow::Result` at boundaries, `thiserror` enums in `src/core/`, `.context(...)` across module boundaries; never suppress errors.
- **Logging:** `tracing::{info,warn,error,debug}` only (never `println!`/`dbg!`); init in `src/core/src/logger.rs`.
- **Unsafe:** none except the Qt FFI shim `src/ui/src/drag.rs` (`#![allow(unsafe_code)]`); crate roots `#![deny(unsafe_code)]`.

### Slint (mandatory)

1. Before writing Slint, read `.docs/.slint-docs/slint-docs-flat/gotchas.md` → `language-and-layout.md` → `interop.md`; never guess syntax. Widget/element properties + callbacks: `INDEX.md`.
2. Rust FFI: grep `interop.md` or fetch `https://docs.slint.dev/latest/docs/slint/<path>.md`. Child windows use `activate_window<C: slint::ComponentHandle>(ui: &C)`; soundness invariants in `src/ui/src/drag.rs`.
3. Slint 1.18: `WindowMoveArea`, `FlexboxLayout`, dynamic `z`, `spring` easing, model `push`/`remove`/`insert`, `string.starts-with`/`ends-with`/`replace-all` available. `Flickable.viewport-*` renamed to `content-*` (old names deprecated). Changelog: `.docs/.slint-docs/slint.1.18.md`.

---

## General Guidelines

### Code Changes

- For non-trivial work, propose an approach and confirm before implementing.
- Keep modifications minimal and scoped; prefer incremental improvements over rewrites. Ask before architectural changes.
- Use explicit types and named constants (no magic numbers).
- Return explicit error types; do not suppress exceptions.
- Follow standard repository linting and formatting configs.
- Decompose files over 400 lines if they mix concerns.
- Use clear naming over comments; reserve comments for complex workarounds or non-obvious issues — why, not what.
- Never run git mutations (commit, push, reset, rebase, amend) unless explicitly instructed.
- Do not create documentation files unless explicitly requested.

### Verification

- Do not run test, lint, format, or type-check commands; the user builds, tests, and lints manually.
- Run them only when the user explicitly asks.

### Author Environment

- CachyOS, KDE Plasma 6, Wayland, Btrfs.
- fish shell, Ghostty terminal, Fresh TUI editor, yay package manager, bun npm manager, Firefox, and Zed code editor.

### Testing

- Do not create test files for trivial changes, or for behavior that is not reliably unit-testable in the test environment (e.g. UI layout/click mapping). Prefer no new files; only add a test when the logic is genuinely testable and worth guarding.

### Definition of Done

- Logic fully implemented.
- Existing docs updated if public interfaces changed.
- When required by the `Verification` rules, run the corresponding `Workflow` command.
- On completion of an update or fix, print a conventional commit message in a fenced code block. Add a body only when the "why" isn't obvious. Do not wrap lines.

### Communication Style

- Provide concise, actionable responses.
- Answer in the fewest words that fully answer. No preamble, no restating the request, no summary of what you just did, no tables unless asked for.
- Ask clarifying questions when requirements are ambiguous.
- Flag potential risks or edge cases proactively.
- Do not pretend to understand how the user feels.
- Never editorialise your answer. No "honest answer", hedging, disclaimers, or meta-commentary — just answer.
- Never end with "want me to…?" or a list of options unless a real decision is needed. If nothing is needed, end.
