BINARY  := target/release/cliptoo
DESKTOP := packaging/cliptoo.desktop

.PHONY: all build install uninstall clean release version changelog

all: build

build:
	cargo build --release -p cliptoo

install: build
	sudo install -Dm755 $(BINARY) /usr/local/bin/cliptoo
	sudo install -Dm644 $(DESKTOP) /usr/share/applications/cliptoo.desktop

uninstall:
	sudo rm -f /usr/local/bin/cliptoo /usr/share/applications/cliptoo.desktop

clean:
	cargo clean -p cliptoo

# Cocogitto release: bump version from conventional commits, sync manifests,
# write CHANGELOG.md, commit, tag, and push. Manual version: `make version V=2.13.0`.
release:
	cog bump --auto

version:
	cog bump --version $(V)

changelog:
	cog changelog