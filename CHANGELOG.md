# Changelog

All notable changes to this project will be documented in this file. See [conventional commits](https://www.conventionalcommits.org/) for commit guidelines.

- - -
## v2.24.1 - 2026-11-05

#### Bug Fixes

- (d70b60e) settle before Ctrl+V so the first paste is not dropped - dcog989

- (6c43476) key favicon cache by page URL, not host - dcog989

- (f8e02df) label the deadhead action "Deadhead" again - dcog989

- - -

## v2.24.0 - 2026-11-05

#### Features

- (2fac74a) generate the site from README.md - dcog989

- (d39056e) register the AppImage in the desktop menu on launch - dcog989

- (3e9da73) add an update check button - dcog989

- (1d7b97f) default the settings window to the main window height and persist user resizes - dcog989

- (b21a378) move destructive confirmations onto the buttons with amber styling - dcog989

- (caadd9d) restore clip multi-select with bulk delete and merge - dcog989

- (72a6175) require a second activation to delete a clip - dcog989

- (a2f9f26) raise the hover preview delay limit to 3000 ms - dcog989

- (a470f5a) suppress hover preview for fully visible single-line text clips - dcog989

- (3575440) decode QR codes from image clips - dcog989

- (061ce5e) make settings, editor, toolbar, and filter grid keyboard-operable - dcog989

- (53f941b) expose the clip list and menus to assistive tech - dcog989

- (507765e) probe dark favicon siblings on any site - dcog989

- (87372fa) page the clip list so history past 1000 rows is reachable - dcog989

- (0c62f40) fill missing code, template, document, media and danger extensions - dcog989

- (8c567a1) show file size and modified time in file and image previews - dcog989

- (f5b8dab) jump clip list back to the first row on search change - dcog989

#### Bug Fixes

- (a1d2872) anchor the context-menu preview to the clip's row - dcog989

- (881db5a) report the update check result in the button label - dcog989

- (8f9a443) fill the toast layout so the Undo button sits at the right edge - dcog989

- (591e933) reconnect the context menu delete, bulk delete and merge actions - dcog989

- (408d747) stop the empty state flashing before the first clip query lands - dcog989

- (c05d526) use the colored app logo for the tray icon - dcog989

- (e465ce8) resolve KDE HIG review findings - dcog989

- (a9ef8c4) rebuild cache entries for displayed clips - dcog989

- (e264e4e) equal-width maintenance buttons - dcog989

- (645bb91) clear live cache files instead of only orphans - dcog989

- (9d0a627) fetch the new theme's icons on switch - dcog989

- (05ce9b5) inset the stepper hover fill from the field border - dcog989

- (4f38953) let settings rows follow their content height - dcog989

- (419151b) drop the 1x1 resize hack from the close path - dcog989

- (1b9050b) bound hover previews and release window buffers - dcog989

- (62ca84a) restore the translation catalog and stop slint-tr-extractor exhausting memory - dcog989

- (f5618e7) use the PascalCase member zbus dispatches on - dcog989

- (8362d0e) serve the active-window callback as a real D-Bus interface - dcog989

- (2e2a16e) keep the active-window tracker working across restarts - dcog989

- (582a707) detect a tracker script that loads but never reports - dcog989

- (42adf90) type an all-image multi-selection as file_image - dcog989

- (b8898ed) only suppress hover previews for clips the row fully shows - dcog989

- (1ac654c) select the clip on right-click - dcog989

- (c98009f) let the content field fill the edit window - dcog989

- (f6ce3c8) size the window to the tallest page so page switches do not move it - dcog989

- (7b93c3b) fill the color preview popup with the swatch - dcog989

- (690ef6c) stop in-app blurs from killing the hover preview and restore focus after child windows close - dcog989

- (6ca6a30) hide accent "Clear" while following the OS accent - dcog989

- (b175842) restore menu popup sizing after the accessibility refactor - dcog989

- (030e049) raise muted-text and empty-state contrast - dcog989

- (d18fdc6) stop pointer hover from moving keyboard selection in lists and menus - dcog989

- (45193fb) keep probing favicon candidates after a non-image 200 - dcog989

- (3258752) highlight search field focus with accent border - dcog989

- (37a24d5) translate the About window and elide long paths - dcog989

- (6121625) flush settings before quitting from the tray - dcog989

- (bbf3938) re-theme an open editor on live theme changes - dcog989

- (0790dcf) roll back the autostart toggle when the desktop file write fails - dcog989

- (23fc8d4) apply text-field side effects after the save debounce - dcog989

- (a7a5f2f) release hotkey focus after a capture - dcog989

- (3988be5) allow retention limits to be disabled and fit the database action grid - dcog989

- (d21fd60) keep the close guard armed after the hamburger popup closes - dcog989

- (9e22c5d) give each context popup its own focus flag and keyboard selection - dcog989

- (98d4f76) sync tags with the opened clip and keep a usable minimum size - dcog989

- (f0f5a9f) confirm Delete and add keyboard access to the menus - dcog989

- (6a53052) keep the hover preview on screen - dcog989

- (1ba0059) install the icon on make install - dcog989

- (d536e61) label the unique-clip counter correctly - dcog989

- (379e077) time out the file chooser, skip export refreshes - dcog989

- (19f8fcf) give a new lock time to write its pid - dcog989

- (2190ce4) migrate files atomically across filesystems - dcog989

- (6d0cedd) use the AppImage path and escape percent - dcog989

- (b594b3c) keep tag syntax on background refreshes - dcog989

- (a9592a7) repair portal detection and accent contrast - dcog989

- (870589b) apply live preview changes and coalesce writes - dcog989

- (4515734) apply retention changes promptly and refresh stats - dcog989

- (2323dc3) check the instance lock before migrating previews - dcog989

- (4a637c1) keep the loaded list and report failed clip actions - dcog989

- (4903b47) remove the unused multi-selection from the clip list - dcog989

- (133387b) harden link fetching and favicon caching - dcog989

- (3daa835) bound image reads and write cache files atomically - dcog989

- (fedcaad) reconnect the cached session bus connection - dcog989

- (cfa06c5) refresh suppression deadlines, wait for focus, surface failures - dcog989

- (fa59800) write edits through an exclusive temp file - dcog989

- (378e4f1) keep the global hotkey working after a portal restart - dcog989

- (3e2d506) bound clipboard reads and idle the poll with the watcher up - dcog989

- (683fa79) warn when a blacklist drops clips with no known source - dcog989

- (bec0613) keep a startup image's accessory text out of the history - dcog989

- (f7a15bd) confirm destructive maintenance and refresh after it - dcog989

- (102fccf) run the first scheduled pass after five minutes - dcog989

- (1b3784b) spare unreadable paths and skip no-op deadhead writes - dcog989

- (7c50e24) stop cache pruning from deleting live files - dcog989

- (4d39597) keep the bottom-pin keep signal on the clip - dcog989

- (bfa4ab2) harden persistence and default every field - dcog989

- (0641667) make history backup atomic and restorable - dcog989

- (f1e67a2) split case boundaries and keep text intact - dcog989

- (d77447f) reject non-ASCII ARGB literals and drop the unused hex field - dcog989

- (b8e425d) cap decoded images at 96 MiB (was 256 MiB) - dcog989

- (ba0f25e) cap remote fetches and fetch each page once - dcog989

- (5bc1c21) page reclassify, spare pins, prune favicon ledger - dcog989

- (0c0b023) stamp images at copy time and skip idle re-probes - dcog989

- (7f5586a) keep handoff files and reap launched processes - dcog989

- (b03e430) fsync edited files and bound image decoding - dcog989

- (c1c75e8) decode signed RTF unicode escapes - dcog989

- (c1237ae) unique pins, clear edit collisions, no import ties - dcog989

- (5d9a307) keep file clips from being downgraded to file_path on dedup - dcog989

- (59046dd) store hover previews at 2x so they stay sharp on HiDPI - dcog989

- (364613b) bound background work and unblock startup - dcog989

- (69740f9) protect file clips when saving edits - dcog989

- (bf9b8bd) keep the search view and bound captured payloads - dcog989

- (3cf439f) preview and edit text contents of file clips with text content - dcog989

- (208266e) stop path-looking text blocks from becoming file clips - dcog989

- (a303b47) various focus issues with hover preview - dcog989

- (80ecfd9) stop re-copies from downgrading file clips to file_path - dcog989

- (f655706) cap retention at the visible list limit - dcog989

- (a283527) serialise exports outside the database lock - dcog989

- (ba8a143) correct html stripping, entity decoding and camel case - dcog989

- (c2275d3) count pastes without reordering and handle empty searches - dcog989

- (c523050) prune orphaned images and keep deleted clip data off disk - dcog989

- (05ba94c) inset the clip row highlight from the list edges - dcog989

- (6519889) ship the app icon in deb and rpm packages - dcog989

- (3a643fb) wipe the .pkg tree before packaging - dcog989

#### Performance Improvements

- (429d2c9) skip clip-list rebuilds while the window is hidden - dcog989

- (1e515ef) load the clip list in 100-row pages - dcog989

- (7bae023) cut hover and list work - dcog989

- (ea0bb28) write reclassified rows one page at a time - dcog989

#### Refactoring

- (531bace) build child windows on first use and trim idle memory - dcog989

- (80b7b83) resize color picker swatch - dcog989

- (790105b) lay out the editor scroll areas with padding instead of absolute insets - dcog989

- (de3d962) centralize toast placement and anchor the hamburger menu to its button - dcog989

- (6d642ba) share clip row and text geometry helpers - dcog989

- (5c4d2c7) centralize menu metrics and size the Send To popup from content - dcog989

- (e3d18d3) reorder log rows and add separators - dcog989

- (e2b2f72) move hover preview delay to the General section - dcog989

- (0d28f57) move log level and latest-log link to the About window - dcog989

- (9d0bba8) share one InputBox component across every text field, consistent styling - dcog989

- (5a50f23) drop unused stepper and menu-row API - dcog989

- (0a3997e) drop unused clip-list data, offsets and callbacks - dcog989

- (05c6c8c) reap xdg-open and reuse the package URL - dcog989

- (8fa687d) one rendition reader, grouped hashes, no dead image hash - dcog989

- (5d144ea) share the spawn-and-reap helper - dcog989

- (af4ede5) remove the unused set_stat helper - dcog989

- (43c88e2) drop unused settings fields and log without panicking - dcog989

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

