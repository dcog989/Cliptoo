BINARY  := target/release/cliptoo
DESKTOP := packaging/cliptoo.desktop
ICON    := src/ui/assets/cliptoo.svg
PKGDIR  := .pkg

VERSION := $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml)
PO      := src/ui/lang/en/LC_MESSAGES/cliptoo.po
# A clean catalog is ~26 KB; anything near this is the reference-list blowup.
PO_MAX  := 200000

.PHONY: all build package install uninstall clean translations

all: build

build:
	cargo build --release -p cliptoo

package: build
	# Wipe the whole tree, not just the tarballs: makepkg leaves pkg/ and src/
	# behind (~47 MB of extracted binary) and only overwrites the same version,
	# so stale bundles accumulate and `pacman -U .pkg/cliptoo-*.pkg.tar.zst`
	# then fails with "duplicate target" (same pkgname listed twice).
	rm -rf $(PKGDIR)
	mkdir -p $(PKGDIR)
	cp packaging/PKGBUILD $(PKGDIR)/
	cd $(PKGDIR) && makepkg -f --nodeps --noconfirm

install: build
	sudo install -Dm755 $(BINARY) /usr/local/bin/cliptoo
	# Install under /usr/local so a local `make install` never drops unowned
	# files into /usr/share and blocks a later `pacman -U` with file conflicts.
	sudo install -Dm644 $(DESKTOP) /usr/local/share/applications/cliptoo.desktop
	sudo install -Dm644 $(ICON) /usr/local/share/icons/hicolor/scalable/apps/cliptoo.svg

uninstall:
	sudo rm -f /usr/local/bin/cliptoo /usr/local/share/applications/cliptoo.desktop /usr/local/share/icons/hicolor/scalable/apps/cliptoo.svg

clean:
	cargo clean -p cliptoo

# Refresh $(PO) from the @tr() calls. msgctxt must be the enclosing
# component/global name: that is the key Slint looks up, and a stale one
# silently falls back to English. -j keeps existing msgstr and needs the file
# to exist, so deleting the catalog first gives a clean sweep of dead entries.
translations:
	# -j appends the newly found references behind the ones already in the
	# catalog, so every run doubles them; once the catalog is bloated that way
	# the extractor thrashes until the kernel kills it. Refuse an implausible
	# catalog and collapse the duplicates, which keeps a run from growing it.
	@test "$$(stat -c%s $(PO) 2>/dev/null || echo 0)" -lt $(PO_MAX) || \
		{ echo "$(PO) is not a clean catalog; restore it from git first"; exit 1; }
	command -v slint-tr-extractor >/dev/null || cargo install slint-tr-extractor
	cp -f $(PO) $(PO).tmp 2>/dev/null || : > $(PO).tmp
	slint-tr-extractor $(if $(wildcard $(PO)),-j,) -d cliptoo --package-name cliptoo \
		--package-version $(VERSION) -o $(PO).tmp src/ui/ui/*.slint
	# -o is truncated before extracting, so replace the catalog only once the
	# complete, deduplicated output exists.
	awk '!(/^#: / && $$0 == prev) { print } { prev = $$0 }' $(PO).tmp > $(PO).new
	mv $(PO).new $(PO)
	rm -f $(PO).tmp
