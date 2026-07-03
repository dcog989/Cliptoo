# AGENTS.md

## What this is

Cliptoo is a native, background-running clipboard manager for Linux desktops. Rust + SQLite + Slint (Qt backend). Targets Wayland and KDE Plasma 6 primarily. Handles many thousands of clipboard records without blocking the UI thread. Rewrite of the original Windows C#/.NET version.

NOTE: Slint 1.17 now in use — `Tooltip` element replaces the custom FilterGrid tooltip overlay; `SystemTrayIcon` (via `inherits SystemTrayIcon`) replaces the old `ksni`-based tray. See `/home/bubba/Projects/Cliptoo/.docs/slint.1.17.md` for the changelog.

## Hard constraints

- **Wayland-only.** There is no X11 fallback. Anything that touches windowing, input, or the clipboard is Wayland-only.
- **KDE Plasma 6 desktop.** Global hotkeys go through the XDG Desktop Portal (`org.freedesktop.portal.GlobalShortcuts`). On Wayland this is the only sanctioned mechanism — there is no `XGrabKey` path and no `KGlobalAccel` fallback. `packaging/PKGBUILD` declares `xdg-desktop-portal-kde` as a hard dep. See `src/ui/src/hotkeys.rs` for the rationale and the graceful-degradation behaviour.
- **Latest actively-maintained deps.** Check the workspace `Cargo.toml` before adding anything.

## Dev Environment

Linux CachyOS / KDE Plasma 6 + Firefox, Zed code editor, fish shell with Ghostty + Fresh editor. yay and bun package managers. All software is updated as of today.

## Build, lint, test

```bash
cargo build --release -p cliptoo   # production build
cargo check                        # fast type-check
cargo test --workspace             # tests live in src/core only
cargo fmt --all -- --check         # formatting
cargo clippy --workspace --all-targets -- -D warnings
```

`lefthook.yml` runs fmt + clippy on pre-commit, tests on pre-push.
CI: `.github/workflows/ci.yml`. Match its commands exactly.

## Code style

- `cargo fmt` defaults. No custom rustfmt config.
- Errors: `anyhow::Result` at boundaries, `thiserror` enums in the core library. `.context(...)` on fallible operations that cross a module boundary.
- Logging: `tracing::{info,warn,error,debug}` only. Initialised in `core/src/logger.rs` to stderr + a daily-rotating file in the app's log dir. Never `println!`, never `dbg!` in committed code.
- `unsafe` is not used in this codebase. Do not introduce it.

## Comments — when to add them

Default: no comments. The code should read like the spec.

**Add a comment when the next reader would otherwise have to spend non-trivial time understanding WHY something is the way it is:**

- Non-obvious platform / OS / protocol gotchas (e.g. *"Start listening BEFORE calling CreateSession to avoid race"* in `src/ui/src/hotkeys.rs`).
- Workarounds for upstream bugs, with the issue link.
- Magic numbers and timeouts — name the constraint.
- Security boundaries: untrusted input, arbitrary exec, IPC trust assumptions.
- A function whose signature alone does not convey its real contract (e.g. *"warns and returns `Ok` on portal absence — do not treat as failure"*).
- Pointers to a specific HLD / doc section when the design choice is non-obvious.

**Do not** comment to restate the code, narrate *"what"* the next line does, or repeat a function's name in prose. If a comment can be
removed without losing information, remove it.

## Slint rules (mandatory)

**Do NOT guess Slint syntax.** Look up exact signatures before writing code.

### Local doc bundle

Path: `.docs/.slint-docs/slint-docs-flat/`
Index: `.docs/.slint-docs/slint-docs-flat/INDEX.md` — find the page you need.

| Directory | What's in it |
|-----------|-------------|
| `widgets/` | Every widget: Button, ListView, LineEdit, TabWidget, etc. — exact properties, callbacks, examples |
| `elements/` | Built-in elements: Rectangle, Text, Image, Path, StyledText |
| `window/` | Window, Dialog, PopupWindow, SystemTrayIcon, Tooltip, ContextMenuArea |
| `gestures/` | TouchArea, Flickable, DragArea, DropArea, ScaleRotate, Swipe |
| `keyboard/` | FocusScope, TextInput, TextInputInterface, key bindings |
| `layouts/` | HorizontalLayout, VerticalLayout, GridLayout |
| `language/` | Language reference: file structure, properties, expressions, animations, states, globals, etc. |
| `reference/` | Type system, colors, built-in functions, platform, timer, enums, math, etc. |
| `guide/` | Best practices, custom controls, focus, fonts, debugging, translations, etc. |
| `tutorial/` | Walkthrough of building a Slint app |
| root | ai-plugins skill files (see below) |

### Before writing any Slint code

**Read these ai-plugins files first** — they correct the mistakes models consistently make:

1. `.docs/.slint-docs/slint-docs-flat/gotchas.md` — "this won't compile because..."
2. `.docs/.slint-docs/slint-docs-flat/language-and-layout.md` — sizing, filling, layout rules
3. `.docs/.slint-docs/slint-docs-flat/interop.md` — Rust↔Slint callbacks, globals, models

### Workflow

1. Find the widget/element you need in `INDEX.md`, read its doc page for exact properties and callbacks.
2. For Rust FFI patterns (ComponentHandle, Weak, Global, ModelRc, etc.): grep the ai-plugins `interop.md` or fetch from `https://docs.slint.dev/latest/docs/slint/<path>.md`.
3. After editing: suggest `slint-viewer --check path/to/file.slint` for compile diagnostics.
4. Never declare UI work done without verifying it renders.

Bad guesses compile fine and do the wrong thing at runtime.

## Layout

```text
src/core/            # cliptoo-core library: parser, db, settings, ...
src/ui/              # binary + OS integration (clipboard, hotkeys, tray, paste, theme)
src/ui/ui/*.slint    # UI components
src/ui/assets/       # icons, default filetype data
packaging/           # PKGBUILD, .desktop
.docs/               # HLD, porting notes, progress, slint-docs bundle
```

Use `ls` / `glob` for detail. Don't memorise the tree.

## Docs

- `.docs/HLD.md` — high-level design.
- `.docs/PORTING.md` — mapping from the C#/WPF original.
- `.docs/Progress.md` — what's done. **Read first** when picking up work.
- `.docs/ToDo.md` — outstanding tasks.

## Permissions

- access all content in /home/bubba/Projects/Cliptoo/ unless excluded below.

### Excluded Content

- ./**/.archive/

## Interaction style

Concise, precise, no analogies, no apologies. Answer the question asked. Don't prompt the next step or volunteer unrequested suggestions.
