PREFIX ?= $(CURDIR)/../../releases/keygen

.PHONY: build install test test-e2e clean

build:
	cargo build --release

install: build
	install -Dm755 target/release/ccu-keygen $(PREFIX)/ccu-keygen
	@echo "Installed: $(PREFIX)/ccu-keygen"

test:
	cargo test

test-e2e: build
	node scripts/e2e.mjs

clean:
	cargo clean
