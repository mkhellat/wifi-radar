# wifi-radar Makefile
# Requires GNU make (uses include, ?=, .PHONY).
# Target platform is GNU/Linux only (nl80211 / cfg80211).
# Run ./configure first to generate config.mk.
#
# Usage:
#   ./configure            # detect prerequisites, write config.mk
#   make                   # bootstrap venv + install package
#   make help              # show all available targets

# Pull in generated configuration (fail gracefully for `make help`)
-include config.mk

# Fallbacks if config.mk not yet generated
PYTHON        ?= python3
VENV_DIR      ?= .venv
PREFIX        ?= /usr/local
SPHINX_BUILD  ?= sphinx-build
INSTALL_EXTRAS ?=

# Derived paths
VENV_BIN      := $(VENV_DIR)/bin
VENV_PYTHON   := $(VENV_BIN)/python
VENV_PIP      := $(VENV_BIN)/pip
VENV_PYTEST   := $(VENV_BIN)/pytest
VENV_RUFF     := $(VENV_BIN)/ruff
VENV_MYPY     := $(VENV_BIN)/mypy
VENV_SPHINX   := $(VENV_BIN)/sphinx-build

# Sentinel files
VENV_STAMP    := $(VENV_DIR)/.stamp
INSTALL_STAMP := $(VENV_DIR)/.installed

# ============================================================================
# DEFAULT TARGET
# ============================================================================

.PHONY: all
all: install ## Bootstrap venv and install package (default)

# ============================================================================
# HELP
# ============================================================================

.PHONY: help
help: ## Show this help
	@printf 'wifi-radar build system\n'
	@printf '=======================\n\n'
	@printf 'Workflow:\n'
	@printf '  1. ./configure [--python=... --venv=... --with-docs]\n'
	@printf '  2. make            (creates venv, installs package + dev deps)\n'
	@printf '  3. make check      (lint + type-check + test)\n\n'
	@printf 'Available targets:\n\n'
	@grep -E '^[a-zA-Z_-]+:.*## ' Makefile | \
		awk 'BEGIN {FS = ":.*## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'
	@printf '\nConfiguration variables (from config.mk or environment):\n\n'
	@printf '  PYTHON          = %s\n' '$(PYTHON)'
	@printf '  VENV_DIR        = %s\n' '$(VENV_DIR)'
	@printf '  PREFIX          = %s\n' '$(PREFIX)'
	@printf '  INSTALL_EXTRAS  = %s\n' '$(INSTALL_EXTRAS)'
	@printf '\nRun ./configure --help for configure options.\n'

# ============================================================================
# VIRTUAL ENVIRONMENT
# ============================================================================

$(VENV_STAMP): pyproject.toml configure config.mk
	@printf '>>> Creating virtual environment in %s ...\n' '$(VENV_DIR)'
	$(PYTHON) -m venv $(VENV_DIR)
	$(VENV_PIP) install --upgrade pip setuptools wheel >/dev/null
	@touch $@

.PHONY: venv
venv: $(VENV_STAMP) ## Create virtual environment

# ============================================================================
# INSTALL
# ============================================================================

$(INSTALL_STAMP): $(VENV_STAMP) pyproject.toml src/wifi_radar/__init__.py
	@printf '>>> Installing wifi-radar in editable mode with [dev] deps ...\n'
	$(VENV_PIP) install -e ".[dev]" >/dev/null
	@if [ -n "$(INSTALL_EXTRAS)" ]; then \
		printf '>>> Installing optional [%s] deps ...\n' '$(INSTALL_EXTRAS)'; \
		$(VENV_PIP) install -e ".[$(INSTALL_EXTRAS)]" >/dev/null; \
	fi
	@touch $@

.PHONY: install
install: $(INSTALL_STAMP) ## Install package + dev dependencies into venv

# ============================================================================
# QUALITY CHECKS
# ============================================================================

.PHONY: lint
lint: $(INSTALL_STAMP) ## Run ruff linter
	@printf '>>> Lint (ruff) ...\n'
	$(VENV_RUFF) check src/ tests/

.PHONY: format
format: $(INSTALL_STAMP) ## Auto-format with ruff
	$(VENV_RUFF) format src/ tests/

.PHONY: format-check
format-check: $(INSTALL_STAMP) ## Check formatting without modifying
	$(VENV_RUFF) format --check src/ tests/

.PHONY: typecheck
typecheck: $(INSTALL_STAMP) ## Run mypy strict type checking
	@printf '>>> Type check (mypy) ...\n'
	$(VENV_MYPY)

.PHONY: test
test: $(INSTALL_STAMP) ## Run pytest suite
	@printf '>>> Test (pytest) ...\n'
	$(VENV_PYTEST) -v

.PHONY: check
check: lint typecheck test ## Run all quality checks (lint + typecheck + test)
	@printf '>>> All checks passed.\n'

# ============================================================================
# DOCUMENTATION
# ============================================================================

.PHONY: docs
docs: $(INSTALL_STAMP) ## Build Sphinx HTML documentation
	@if [ ! -x "$(VENV_SPHINX)" ]; then \
		printf '>>> sphinx-build not found; installing [docs] extras ...\n'; \
		$(VENV_PIP) install -e ".[docs]" >/dev/null; \
	fi
	@printf '>>> Building documentation ...\n'
	$(VENV_SPHINX) -b html docs docs/_build/html
	@printf '>>> Docs built: docs/_build/html/index.html\n'

.PHONY: docs-clean
docs-clean: ## Remove built documentation
	rm -rf docs/_build

# ============================================================================
# RUN
# ============================================================================

.PHONY: run
run: $(INSTALL_STAMP) ## Run wifi-radar (pass ARGS="..." for extra arguments)
	$(VENV_BIN)/wifi-radar $(ARGS)

.PHONY: run-sudo
run-sudo: $(INSTALL_STAMP) ## Run wifi-radar with sudo (for monitor mode)
	sudo $(VENV_BIN)/wifi-radar $(ARGS)

# ============================================================================
# PACKAGING
# ============================================================================

.PHONY: dist
dist: $(INSTALL_STAMP) ## Build sdist + wheel into dist/
	@printf '>>> Building distribution packages ...\n'
	$(VENV_PYTHON) -m build

.PHONY: dist-clean
dist-clean: ## Remove dist/ and *.egg-info
	rm -rf dist/ src/*.egg-info

# ============================================================================
# OUI DATABASE
# ============================================================================

.PHONY: fetch-oui
fetch-oui: $(INSTALL_STAMP) ## Download IEEE OUI vendor database
	$(VENV_BIN)/wifi-radar --fetch-oui

# ============================================================================
# CLEAN
# ============================================================================

.PHONY: clean
clean: ## Remove venv, build artifacts, caches
	rm -rf $(VENV_DIR) dist/ src/*.egg-info docs/_build
	rm -rf .mypy_cache .pytest_cache .ruff_cache
	find . -type d -name __pycache__ -exec rm -rf {} + 2>/dev/null || true
	@printf '>>> Cleaned.\n'

.PHONY: distclean
distclean: clean ## Remove everything including config.mk
	rm -f config.mk
	@printf '>>> Dist-cleaned (re-run ./configure).\n'
