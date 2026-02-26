PREFIX ?= $(HOME)/.local
BINDIR ?= $(PREFIX)/bin

.PHONY: build release install check fmt compat-check parity-check quality

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

compat-check:
	./scripts/compat_check.sh $(N)

parity-check:
	./scripts/parity_check.sh

quality:
	./tools/quality_gate.py --src src --max-file-lines 400 --max-fn-lines 50 --allow-fn execute_task
