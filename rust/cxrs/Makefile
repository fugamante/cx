PREFIX ?= $(HOME)/.local
BINDIR ?= $(PREFIX)/bin

.PHONY: build release install check fmt

fmt:
	cargo fmt

check:
	cargo check

build:
	cargo build

release:
	cargo build --release

install: release
	mkdir -p "$(BINDIR)"
	cp target/release/cxrs "$(BINDIR)/cxrs"
	chmod +x "$(BINDIR)/cxrs"
	@echo "installed: $(BINDIR)/cxrs"
