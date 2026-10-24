BINARY  := target/release/cliptoo
DESKTOP := packaging/cliptoo.desktop
PKGDIR  := .pkg

VERSION := $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml)
PO      := src/ui/lang/en/LC_MESSAGES/cliptoo.po

.PHONY: all build package install uninstall clean translations

all: build

build:
	cargo build --release -p cliptoo

package: build
	mkdir -p $(PKGDIR)
	# Drop prior versions first: makepkg -f only overwrites the same version, so
	# stale bundles accumulate and `pacman -U .pkg/cliptoo-*.pkg.tar.zst` then
	# fails with "duplicate target" (same pkgname listed twice).
	rm -f $(PKGDIR)/*.pkg.tar.zst
	cp packaging/PKGBUILD $(PKGDIR)/
	cd $(PKGDIR) && makepkg -f --nodeps --noconfirm

install: build
	sudo install -Dm755 $(BINARY) /usr/local/bin/cliptoo
	# Install under /usr/local so a local `make install` never drops unowned
	# files into /usr/share and blocks a later `pacman -U` with file conflicts.
	sudo install -Dm644 $(DESKTOP) /usr/local/share/applications/cliptoo.desktop

uninstall:
	sudo rm -f /usr/local/bin/cliptoo /usr/local/share/applications/cliptoo.desktop

clean:
	cargo clean -p cliptoo

# Refresh $(PO) from the @tr() calls. msgctxt must be the enclosing
# component/global name: that is the key Slint looks up, and a stale one
# silently falls back to English. -j keeps existing msgstr and needs the file
# to exist, so deleting the catalog first gives a clean sweep of dead entries.
translations:
	command -v slint-tr-extractor >/dev/null || cargo install slint-tr-extractor
	slint-tr-extractor $(if $(wildcard $(PO)),-j,) -d cliptoo --package-name cliptoo \
		--package-version $(VERSION) -o $(PO) src/ui/ui/*.slint
