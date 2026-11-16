# Verify the repo: the same lint, format check and tests CI runs.
default: check

# Format check, clippy and the tests.
check: lint test
    cargo fmt --all -- --check

# Clippy, warnings as errors.
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Workspace tests.
test:
    cargo test --workspace

# Rewrite Rust and Slint formatting.
format:
    cargo fmt --all
    slint-lsp format -i src/ui/ui/*.slint

# Release binary.
build:
    cargo build --release -p cliptoo

# Debug build and run.
run:
    cargo run

# Arch package from packaging/PKGBUILD.
package: build
    # Wipe the whole tree, not just the tarballs: makepkg leaves pkg/ and src/
    # behind (~47 MB of extracted binary) and only overwrites the same version,
    # so stale bundles accumulate and `pacman -U .pkg/cliptoo-*.pkg.tar.zst`
    # then fails with "duplicate target".
    rm -rf .pkg
    mkdir -p .pkg
    cp packaging/PKGBUILD .pkg/
    cd .pkg && makepkg -f --nodeps --noconfirm

# Install to /usr/local, where a later `pacman -U` cannot conflict.
install: build
    sudo install -Dm755 target/release/cliptoo /usr/local/bin/cliptoo
    sudo install -Dm644 packaging/cliptoo.desktop /usr/local/share/applications/cliptoo.desktop
    sudo install -Dm644 src/ui/assets/cliptoo.svg /usr/local/share/icons/hicolor/scalable/apps/cliptoo.svg

# Remove the /usr/local install.
uninstall:
    sudo rm -f /usr/local/bin/cliptoo /usr/local/share/applications/cliptoo.desktop /usr/local/share/icons/hicolor/scalable/apps/cliptoo.svg

# Remove this package's build output.
clean:
    cargo clean -p cliptoo

# Live website rendered from README.md.
website:
    bun run website

# Static site in website/dist, the command the Pages workflow runs.
website-build:
    bun run website:build

# Do not run: the extractor thrashes RAM and can leave the catalog at 0 bytes.
translations:
    #!/usr/bin/env bash
    set -euo pipefail
    # Add .po entries by hand until a fixed extractor is confirmed against a copy
    # with an explicit memory cap.
    po=src/ui/lang/en/LC_MESSAGES/cliptoo.po
    size="$(stat -c%s "$po" 2>/dev/null || echo 0)"
    # A clean catalog is ~26 KB; anything near this is the reference-list blowup.
    test "$size" -lt 200000 || { echo "$po is not a clean catalog; restore it from git first"; exit 1; }
    command -v slint-tr-extractor >/dev/null || cargo install slint-tr-extractor
    cp -f "$po" "$po.tmp" 2>/dev/null || : > "$po.tmp"
    version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
    # -j keeps existing msgstr; -o is truncated before extracting, so replace the
    # catalog only once the complete, deduplicated output exists.
    join=""
    [ -s "$po" ] && join="-j"
    slint-tr-extractor $join -d cliptoo --package-name cliptoo --package-version "$version" -o "$po.tmp" src/ui/ui/*.slint
    # -j appends the newly found references behind the ones already present, so
    # every run doubles them; collapse the duplicates.
    awk '!(/^#: / && $0 == prev) { print } { prev = $0 }' "$po.tmp" > "$po.new"
    mv "$po.new" "$po"
    rm -f "$po.tmp"

# Refresh dependencies to the latest allowed versions.
update:
    cargo upgrade && cargo update --verbose
    bun update --latest
