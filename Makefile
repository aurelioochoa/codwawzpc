# The standard Makefile.
#
# Copy this into a project, fill in the identity header, then replace each verb
# below with that stack's real command. Every repo answers the same eleven
# verbs, so `make` in any of them lists the same vocabulary.
#
#   help  setup  dev  build  run  test  check  verify  fmt  clean  distclean
#
# Rules that keep the set uniform:
#
#   * `make` on its own always prints the help. It never builds, installs or
#     starts anything.
#   * A verb that does not apply still EXISTS and explains why, via $(call NA,
#     '...'). A missing target gives make's "No rule to make target", which
#     reads like a broken Makefile rather than an answer.
#   * `check` is the fast gate and `verify` is `check` plus a build.
#   * `clean` is always safe to run. Anything that costs a reinstall — node_
#     modules, .venv, target/, docker volumes — belongs to `distclean`.
#   * A project's own targets keep their own `##@ Section`. Renaming one breaks
#     the docs, so the old name stays as an alias with no `##` comment, which
#     keeps it out of the help.
#
# The block between the two sentinels is byte-identical in every repo, so
# checking for drift is a diff and needs no parser.

# ─── Identity ───────────────────────────────────────────────────────────────
PROJECT       := codwawzpc
TAGLINE       := Call of Duty Zombies (iPhone 1.5.0) Linux PC host
# Keep these recursive so the defaults below appear in help.
HELP_VARS      = CARGO=$(CARGO) PYTHON=$(PYTHON) CARGO_BUILD_JOBS=$(CARGO_BUILD_JOBS) ARGS=$(ARGS) IPA=/path/to/game.ipa
HELP_EXAMPLE   = make import IPA="/path/to/game.ipa"

# ─── Preamble ────────────────────────────────────────────────────────────────
# -e stops a recipe at the first failing command; -o pipefail stops a gate that
# pipes through a filter from reporting the filter's exit code instead of the
# command's — without it, a suite that fails into `tail` "passes".
SHELL         := /usr/bin/env bash
.SHELLFLAGS   := -eo pipefail -c
.DEFAULT_GOAL := help
MAKEFLAGS     += --no-print-directory

# ─── Colour ──────────────────────────────────────────────────────────────────
# MAKE_TERMOUT (GNU Make >= 4.1) is set only when stdout is a terminal, and is
# the only reliable test available here: `test -t 1` inside $(shell ...) always
# reports false, because make captures that command's stdout through a pipe.
# So `make help | less` and CI logs stay clean. NO_COLOR disables, FORCE_COLOR
# overrides both.
COLOR ?= $(if $(MAKE_TERMOUT),1,0)
ifdef NO_COLOR
  COLOR := 0
endif
ifdef FORCE_COLOR
  COLOR := 1
endif
ifeq ($(COLOR),1)
  # Real ESC bytes, so a plain `echo` renders them without needing -e.
  C_HEAD := $(shell printf '\033[1m')
  C_CMD  := $(shell printf '\033[36m')
  C_OK   := $(shell printf '\033[32m')
  C_WARN := $(shell printf '\033[33m')
  C_ERR  := $(shell printf '\033[31m')
  C_DIM  := $(shell printf '\033[2m')
  C_OFF  := $(shell printf '\033[0m')
endif

# Explains why a core verb does not apply here, then fails.
NA = @printf '  $(C_ERR)make $@$(C_OFF) does not apply to $(PROJECT).\n  $(C_DIM)%s$(C_OFF)\n\n' $(1) >&2; exit 2

# Width of the target-name column in help; widen it where names are long.
HELP_PAD ?= 18

# PROJECT, TAGLINE, HELP_VARS and HELP_EXAMPLE are interpolated into a
# single-quoted shell string below, so none of them may contain an apostrophe.
##@ General
.PHONY: help
help: ## List the available targets
	@printf '\n  $(C_HEAD)$(PROJECT)$(C_OFF) — $(TAGLINE)\n'
	@printf '  $(C_DIM)usage: make <target>$(C_OFF)\n'
	@awk 'BEGIN { FS = ":.*?## " } \
	  /^##@ / { printf "\n  $(C_HEAD)%s$(C_OFF)\n", substr($$0, 5); next } \
	  /^[a-zA-Z0-9_.-]+:.*?## / { printf "    $(C_CMD)%-$(HELP_PAD)s$(C_OFF) %s\n", $$1, $$2 }' \
	  $(MAKEFILE_LIST)
	@printf '\n  $(C_DIM)Variables:$(C_OFF) $(HELP_VARS)\n'
	@printf '  $(C_DIM)Example:$(C_OFF)   $(HELP_EXAMPLE)\n\n'

# ─── End of the shared block ─────────────────────────────────────────────────

CARGO ?= cargo
PYTHON ?= python3
CARGO_BUILD_JOBS ?= 4
ARGS ?=
IPA ?=
# Pass archive paths literally, including spaces, dollar signs and shell characters.
override IPA := $(value IPA)
export CARGO_BUILD_JOBS IPA

##@ Setup
.PHONY: setup submodules
setup: submodules ## Initialize pinned native dependencies and fetch Rust crates
	$(CARGO) fetch --locked
submodules: ## Initialize pinned native dependencies
	git submodule update --init --recursive

##@ Development
.PHONY: dev build debug run launcher install-desktop
dev: launcher ## Build optimized and open the native launcher
launcher: build ## Open the Shi No Numa native launcher
	CODWAWZPC_PYTHON="$(PYTHON)" ./target/release/codwawzpc launcher
install-desktop: build ## Add the launcher to your Linux applications menu
	$(PYTHON) -m launcher.desktop --binary target/release/codwawzpc
build: ## Build the release executable (extra Cargo flags via ARGS)
	$(CARGO) build --locked --release --bin codwawzpc $(ARGS)
debug: ## Build the debug executable (extra Cargo flags via ARGS)
	$(CARGO) build --locked --bin codwawzpc $(ARGS)
run: ## Build and play in release mode (run options via ARGS)
	$(CARGO) run --locked --release --bin codwawzpc -- run $(ARGS)

##@ Game
.PHONY: import check-game
import: ## Import a game archive: make import IPA="/path/to/game.ipa"
	@test -n "$$IPA" || { printf '%s\n' 'Usage: make import IPA="/path/to/game.ipa"' >&2; exit 2; }
	$(CARGO) run --locked --release --bin codwawzpc -- import "$$IPA"
check-game: ## Verify the imported game files
	$(CARGO) run --locked --release --bin codwawzpc -- check

##@ Gates
.PHONY: test check verify fmt fmt-check test-cli test-launcher
test-launcher: build ## Check launcher backend and native interactions (needs a display)
	$(PYTHON) -m unittest discover -s tests -p test_launcher.py
	$(PYTHON) scripts/verify_launcher.py
check: ## Check Rust library and binaries without linking
	$(CARGO) check --locked --lib --bins
	@printf 'CHECK_PASS\n'
test: ## Run host, co-op protocol and relay server tests
	$(CARGO) test --locked --lib
	$(CARGO) test --locked -p codwawzpc_coop_proto -p codwawzpc-server
verify: check test build test-cli fmt-check ## Check, test, build and validate the CLI without game data
	@printf 'VERIFY_PASS\n'
fmt: ## Format Rust sources
	$(CARGO) fmt --all
fmt-check: ## Check Rust formatting without modifying files
	$(CARGO) fmt --all -- --check
test-cli: build ## Check CLI and import behavior (set CODWAWZPC_TEST_IPA for a real import)
	$(PYTHON) scripts/verify_cli.py

##@ Cleaning
.PHONY: clean distclean
clean: ## Nothing to remove besides Cargo artifacts, which distclean handles
	@printf 'Nothing to clean; game data and saves live in XDG directories.\n'
distclean: clean ## Remove Cargo build artifacts (target/)
	$(CARGO) clean
