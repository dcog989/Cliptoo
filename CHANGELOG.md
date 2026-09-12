# Changelog

All notable changes to this project will be documented in this file. See [conventional commits](https://www.conventionalcommits.org/) for commit guidelines.

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

