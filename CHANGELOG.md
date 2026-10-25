# Changelog

All notable changes to this project will be documented in this file. See [conventional commits](https://www.conventionalcommits.org/) for commit guidelines.

- - -
## v2.23.0 - 2026-10-25

#### Features

- (33e7f26) unify settings rows on shared components and add accent hover states - dcog989

- (e3a7504) add multi-select to the clip list - dcog989

- (ce6e801) add a Shortcuts page with hotkey inputs and a keyboard cheat sheet - dcog989

- (ca2e22a) add Delete, Ctrl+C and Ctrl+E selection shortcuts - dcog989

- (1a58e3a) add a Compact Database maintenance action - dcog989

- (d8ea28c) enable FTS5 secure-delete in v5 migration - dcog989

- (8074cf6) move clip-list selection on hover so only one row is highlighted - dcog989

- (5e12db1) forward PageUp/PageDown to the clip list from the search field - dcog989

- (7090724) arrow-key navigation and Enter activation - dcog989

- (a7d6f7e) strip BOM and outer whitespace in the LF normalise - dcog989

#### Bug Fixes

- (adad250) read the FTS secure-delete flag as an integer - dcog989

- (f99a330) re-map the window on the shortcut so it always comes back focused - dcog989

- (0bd5e97) complete clicks whose press the compositor ate on window activation - dcog989

- (ed135f4) truncate the WAL after VACUUM in compact - dcog989

- (a46dbb6) coerce rusqlite results into anyhow in clip_metadata test - dcog989

- (676e347) tolerate NULL Content/PreviewContent on read - dcog989

- (dca6fe2) check and optimize the FTS index in the maintenance pass - dcog989

- (07c95f9) wait for a competing connection's lock instead of failing instantly - dcog989

- (b5a87e2) refresh clip metadata when a duplicate copy bumps a row - dcog989

- (f0cc581) classify color-only rich text as a color clip - dcog989

- (b3d0a8b) highlight the selected clip row with the accent - dcog989

- (376dc16) keep file and folder clips file clips when saving an edit - dcog989

- (7e1bf0d) record and reorder on paste-as from the context menu - dcog989

- (857be64) apply text field edits as you type and size the menu for Send To - dcog989

- (6f0abe7) offer a paste row for every pasteable clip type - dcog989

#### Performance Improvements

- (1965bc4) cap FTS snippets by stored SizeInBytes, not length(Content) - dcog989

- (b59c6f6) run PRAGMA optimize after each maintenance pass - dcog989

- (0c2024f) use synchronous=NORMAL on the WAL connection - dcog989

#### Refactoring

- (bdc2791) put open-log link beside the log level selector - dcog989

- - -

## v2.22.0 - 2026-10-24

#### Features

- (ee7b83c) colour the About tagline with the logo gold - dcog989

- (865d66e) use one fixed danger palette across destructive elements - dcog989

- (81a122f) tune the destructive context-menu row palette - dcog989

- (27cad5b) destructive context-menu row hovers as a red fill with white content - dcog989

- (5dd89cf) add a separator above Delete in the clip context menu - dcog989

#### Bug Fixes

- (a2c46f9) require a leading tag before classifying content as HTML - dcog989

- (4982318) let a plain-text paste flatten file clips too - dcog989

- - -

## v2.21.0 - 2026-10-15

#### Features

- (c978e06) show dimmed logo + "No Results" overlay when the clip list is empty - dcog989

#### Bug Fixes

- (f7bf29b) build the uinput keyboard at startup so the first paste isn't dropped - dcog989

- (d2f505d) drop unused OpenOptionsExt import in temp - dcog989

- (3dc3743) stage temp files under $XDG_RUNTIME_DIR and stop settings debris - dcog989

- - -

## v2.20.0 - 2026-10-12

#### Features

- (3a08e24) group clip-type colours by hue family, colour text/other files, fix bookmark icon - dcog989

- (3bf03f5) give search-match highlight strong chroma and contrast - dcog989

- (14aad73) color code clip list text and filter icons by clip type - dcog989

#### Bug Fixes

- (83a6c3c) ignore an already-installed rustls provider instead of aborting - dcog989

- (f4527a5) install clang and mold for the committed cargo linker config - dcog989

- (8e3730a) swap rustls to the ring provider and install it at startup - dcog989

- (2b12f6e) hover the row with the accent's own hue and chroma - dcog989

- (5c98783) split accent into text and fill grades, fix status contrast - dcog989

- (29d65ca) move db to data dir and logs to state dir, honour XDG_CONFIG_HOME - dcog989

- (0959f40) dismiss context-menu preview when search field gains focus - dcog989

#### Performance Improvements

- (b5770c3) cap FTS highlight content size to fix slow broad searches - dcog989

#### Refactoring

- (ea7983f) split accent into text and fill grades - dcog989

- (56f3eea) demote per-clip ingestion logs from info to debug - dcog989

- (59ed7cb) measure theme contrast with APCA-W3 instead of WCAG 2.x - dcog989

- (488e115) remove search/filter from the settings window - dcog989

- - -

## v2.19.0 - 2026-10-09

#### Features

- (eb29d51) color code clip list text by clip type - dcog989

- (ba2eac6) round main window corners - dcog989

#### Bug Fixes

- (93d296d) don't cache empty or malformed SVG thumbnails - dcog989

- (d5bbbd8) scroll clip list by wheel angleDelta, not Wayland pixelDelta - dcog989

- - -

## v2.18.0 - 2026-10-04

#### Features

- (ae4362c) weight search results by bookmark and paste frequency - dcog989

#### Bug Fixes

- (3b7dea2) order bookmark icon color branches to match clip text - dcog989

- (b6d0277) install desktop entry under /usr/local prefix - dcog989

- (a9c3700) drop stale package artifacts before makepkg - dcog989

- (dc6a017) make search rank boosts multiplicative to be scale-invariant - dcog989

- (e147b6f) color bookmark icon to match clip text - dcog989

- - -

## v2.17.1 - 2026-09-21

#### Bug Fixes

- (94357ff) keep SVG markup out of HTML classification and render source thumbnails - dcog989

- (86a219f) keep bookmark fg color on selected clip row - dcog989

#### Refactoring

- (aa9a45b) replace hand-rolled OKLCH/Okhsl math with palette - dcog989

- (4523b6d) split theme module by concern - dcog989

- (1836db1) split maintenance module by task - dcog989

- (423479e) split settings module by concern - dcog989

- - -

## v2.17.0 - 2026-09-16

#### Features

- (d1f3982) expose search fields to assistive tech - dcog989

- (2b4908d) replace accent sliders with native color picker - dcog989

- (8d11f91) make preview size setting apply to all previews - dcog989

- (383e743) render hover preview in a standalone tooltip window - dcog989

#### Bug Fixes

- (5993f31) keep window visible when the system tray fails to show - dcog989

- (e522211) migrate deprecated Flickable viewport-* properties to content-* - dcog989

- (c23eb90) offer plain text before uri-list so text aliases carry the path - dcog989

- (55a39ec) correct hover preview tooltip, remove 'image' - dcog989

- (6c3efe7) make hover image preview size setting affect the popup - dcog989

- (7948179) store regenerable cache under XDG cache dir - dcog989

#### Performance Improvements

- (86dace5) mutate clip list in place for single-row delete/move - dcog989

#### Refactoring

- (7bc48f3) move window via WindowMoveArea instead of Qt FFI - dcog989

- - -

## v2.16.2 - 2026-09-12

#### Features

- (19a0d36) add Arch .pkg.tar.zst build target and release artifacts - dcog989

#### Bug Fixes

- (27535cc) include hidden .pkg dir in release artifact upload - dcog989

- - -

## v2.16.1 - 2026-09-12

#### Bug Fixes

- (27535cc) include hidden .pkg dir in release artifact upload - dcog989

- - -

## v2.16.0 - 2026-09-12

#### Features

- (19a0d36) add Arch .pkg.tar.zst build target and release artifacts - dcog989

- - -

## v2.15.1 - 2026-09-06

#### Bug Fixes

- (c7966c2) satisfy clippy 1.98 collapsible-if and single-match lints - dcog989

- - -

## v2.15.0 - 2026-09-06

#### Features

- (5b55e25) extend De-slug to readable-text across camelCase, PascalCase, SCREAMING_SNAKE, dot.case - dcog989

- (0cc63b4) cap favicon fetch attempts per clip at five - dcog989

#### Bug Fixes

- (5623e24) attribute clips to the app focused at copy time - dcog989

- (40b4b2c) resolve favicon hrefs against the page path, not the domain root - dcog989

- (7e5c022) reload all rows after a favicon fetch lands - dcog989

- (98591f7) fetch each favicon domain at most once per session - dcog989

- (32b52ea) render degenerate SVGs and fetch the site's own favicon - dcog989

- (d2b2ed8) stop re-logging accessory text on every stale poll - dcog989

- (41c67c4) don't break lines on inline-styled divs in html strip - dcog989

- - -

## v2.14.1 - 2026-08-31

- - -

## v2.14.0 - 2026-08-31

#### Features

- (64b2e01) classify markdown family as document files - dcog989

- (4c0808f) use rich-text and pencil-line icons in the clip context menu - dcog989

- (da04b83) capture and paste text/html rich-text clips - dcog989

#### Bug Fixes

- (f467340) don't re-raise main window over a just-opened child window - dcog989

- (3a01db6) close app to tray when clicking away while a popup menu is open + ESC close - dcog989

- (92ff1d5) don't swallow plain text when rich clipboard payload is empty - dcog989

- (3ba9259) avoid duplicate clip when clipboard offers rtf + html - dcog989

- (8d60f71) consistent bg hover colors - dcog989

- (5bde8e0) clip / file categorisation and icons - dcog989

#### Refactoring

- (f1ba0de) Removed the dead close-popups ESC fallback - dcog989

- - -

## v2.13.8 - 2026-08-25

#### Refactoring

- (887237f) makefile tidy - dcog989

- - -

## v2.13.7 - 2026-08-24

- - -

## v2.13.6 - 2026-08-24

- - -

## v2.13.5 - 2026-08-24

- - -

## v2.13.4 - 2026-08-24

- - -

## v2.13.3 - 2026-08-24

- - -

## v2.13.2 - 2026-08-24

#### Bug Fixes

- (03c708b) use correct cargo-generate-rpm asset keys - dcog989

- (b40466e) add rpm assets metadata - dcog989

- - -

## v2.13.1 - 2026-08-24

- - -

## v2.13.0 - 2026-08-24

- - -

