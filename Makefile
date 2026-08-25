BINARY  := target/release/cliptoo
DESKTOP := packaging/cliptoo.desktop

.PHONY: all build install uninstall clean

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
