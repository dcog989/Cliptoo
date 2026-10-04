BINARY  := target/release/cliptoo
DESKTOP := packaging/cliptoo.desktop
PKGDIR  := .pkg

.PHONY: all build package install uninstall clean

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
