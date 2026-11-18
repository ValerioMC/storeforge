NPM ?= npm
CARGO ?= cargo
MANIFEST := src-tauri/Cargo.toml
APP_BUNDLE := src-tauri/target/release/bundle/macos/StoreForge.app
APPLICATIONS_DIR ?= /Applications

.DEFAULT_GOAL := help
.PHONY: help dev build app install test check

help:
	@printf '%s\n' 'make dev      Start StoreForge in development mode' 'make build    Build desktop installers for this platform' 'make install  Build and install StoreForge.app in /Applications' 'make test     Run Vue and Rust tests' 'make check    Verify formatting, Clippy, and the frontend build'

dev:
	$(NPM) ci
	$(NPM) run tauri dev

build:
	$(NPM) ci
	$(NPM) run tauri -- build -- --locked

app:
	@test "$$(uname -s)" = Darwin || { printf '%s\n' 'The app bundle target requires macOS.' >&2; exit 1; }
	$(NPM) ci
	$(NPM) run tauri -- build --bundles app -- --locked

install: app
	./scripts/macos-install.sh "$(APP_BUNDLE)" "$(APPLICATIONS_DIR)"

test:
	$(NPM) test
	$(CARGO) test --locked --manifest-path $(MANIFEST)

check:
	$(CARGO) fmt --manifest-path $(MANIFEST) --check
	$(CARGO) clippy --locked --manifest-path $(MANIFEST) --all-targets -- -D warnings
	$(NPM) run build
