#!/bin/bash
#
# setup.sh — One-stop setup after a fresh clone of tsz.
#
# Re-running is fast: each step checks whether work is actually needed and
# prints "up to date" when nothing changed.
#
# Usage:
#   ./scripts/setup.sh          # full setup
#   ./scripts/setup.sh --quick  # skip cargo check at the end
#   ./scripts/setup.sh --force  # redo every step even if already done
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

# ── Colours ──────────────────────────────────────────────────────────────────
bold()  { printf '\033[1m%s\033[0m' "$*"; }
green() { printf '\033[32m%s\033[0m' "$*"; }
dim()   { printf '\033[2m%s\033[0m' "$*"; }
red()   { printf '\033[31m%s\033[0m' "$*"; }
step()  { echo ""; echo "$(bold "→ $1")"; }
skip()  { echo "  $(dim "$1")"; }

# ── Flags ────────────────────────────────────────────────────────────────────
QUICK=false
FORCE=false
for arg in "$@"; do
  case "$arg" in
    --quick) QUICK=true ;;
    --force) FORCE=true ;;
  esac
done

# ── 1. Prerequisites ────────────────────────────────────────────────────────
step "Checking prerequisites"

missing=()
command -v node  &>/dev/null || missing+=("node (Node.js — required for lib-asset generation)")
command -v npm   &>/dev/null || missing+=("npm (comes with Node.js)")
command -v cargo &>/dev/null || missing+=("cargo (Rust toolchain — install via https://rustup.rs)")
command -v git   &>/dev/null || missing+=("git")

if [ ${#missing[@]} -ne 0 ]; then
  echo "$(red "Missing required tools:")"
  for tool in "${missing[@]}"; do
    echo "  • $tool"
  done
  exit 1
fi

echo "  node  $(node --version)"
echo "  npm   $(npm --version)"
echo "  cargo $(cargo --version | awk '{print $2}')"
echo "  git   $(git --version | awk '{print $3}')"
echo "  $(green "All prerequisites met.")"

# ── 2. TypeScript submodule ─────────────────────────────────────────────────
step "TypeScript submodule"

# Fast-path: submodule is checked out and has a HEAD
TS_SHA=$(cd TypeScript 2>/dev/null && git rev-parse --short HEAD 2>/dev/null || true)
if [ "$FORCE" = false ] && [ -n "$TS_SHA" ]; then
  skip "Already initialised (TypeScript@$TS_SHA)."
else
  if [ -f "$SCRIPT_DIR/setup-ts-submodule.sh" ]; then
    bash "$SCRIPT_DIR/setup-ts-submodule.sh"
  else
    git submodule update --init --depth 1 -- TypeScript
  fi
fi

# ── 3. npm dependencies ─────────────────────────────────────────────────────
step "npm dependencies"

# Only run npm install when node_modules is missing or package.json changed.
# We use a .npm-stamp file so that random mtime changes inside node_modules
# don't cause false negatives.
install_npm() {
  local dir="$1"
  local label="$2"
  [ -f "$dir/package.json" ] || return 0

  local stamp="$dir/node_modules/.setup-stamp"
  if [ "$FORCE" = false ] && [ -f "$stamp" ] && [ "$stamp" -nt "$dir/package.json" ]; then
    skip "$label — up to date."
  else
    echo "  $label …"
    (cd "$dir" && npm install --no-audit --no-fund --loglevel=error >/dev/null)
    mkdir -p "$dir/node_modules"
    touch "$stamp"
  fi
}

install_npm "$SCRIPT_DIR"            "scripts/"
install_npm "$SCRIPT_DIR/emit"       "scripts/emit/"
install_npm "$SCRIPT_DIR/fourslash"  "scripts/fourslash/"

# ── 4. Embedded lib-assets ───────────────────────────────────────────────────
step "Embedded lib-assets"

LIB_VERSION_FILE="$ROOT_DIR/src/lib-assets/lib_version.json"
if [ "$FORCE" = false ] && [ -f "$LIB_VERSION_FILE" ]; then
  skip "Already generated (use --force or LIB_ASSETS_FORCE=1 to regenerate)."
else
  node "$SCRIPT_DIR/generate-lib-assets.mjs"
  echo "  $(green "Lib-assets generated.")"
fi

# ── 5. Git hooks ─────────────────────────────────────────────────────────────
step "Git hooks"

CURRENT_HOOKS_PATH=$(git config --get core.hooksPath 2>/dev/null || true)
if [ "$FORCE" = false ] && [ "$CURRENT_HOOKS_PATH" = "scripts/githooks" ]; then
  skip "Already configured (core.hooksPath = scripts/githooks)."
else
  GITHOOKS_DIR="$ROOT_DIR/scripts/githooks"
  GIT_HOOKS_DIR="$ROOT_DIR/.git/hooks"

  # Clean up stale symlinks / old hooks that might conflict
  if [ -d "$GIT_HOOKS_DIR" ]; then
    for hook in pre-commit prepare-commit-msg commit-msg post-commit pre-push; do
      hook_path="$GIT_HOOKS_DIR/$hook"
      if [ -L "$hook_path" ]; then
        rm -f "$hook_path"
      elif [ -f "$hook_path" ] && [ ! -f "$hook_path.sample" ]; then
        grep -q "scripts/githooks" "$hook_path" 2>/dev/null || mv "$hook_path" "$hook_path.bak"
      fi
    done
  fi

  git config core.hooksPath scripts/githooks
  chmod +x "$GITHOOKS_DIR"/* 2>/dev/null || true
  echo "  $(green "Hooks installed (core.hooksPath = scripts/githooks).")"
fi

# ── 6. Cargo check ──────────────────────────────────────────────────────────
if [ "$QUICK" = true ]; then
  step "Cargo check"
  skip "Skipped (--quick)."
else
  step "Cargo check"
  CHECK_OUTPUT=$(cargo check 2>&1) || {
    echo "$CHECK_OUTPUT"
    echo "  $(red "cargo check failed.")"
    exit 1
  }
  echo "  $(green "Build verified.")"
fi

# ── Done ─────────────────────────────────────────────────────────────────────
echo ""
echo "$(green "$(bold "✓ Setup complete!")")"
echo ""
echo "  Useful commands:"
echo "    cargo build              Build tsz"
echo "    cargo test               Run unit tests"
echo "    ./scripts/conformance.sh Run conformance tests"
echo "    cargo run -- file.ts     Type-check a file"
echo ""
