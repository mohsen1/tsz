#!/bin/bash

# Setup TypeScript submodule with sparse checkout
# Includes:
#   - tests/ for conformance tests
#   - src/lib/ for embedded lib.d.ts files
#   - src/compiler, src/services, src/harness, etc. for test harness
#   - scripts/ for build tooling
# Run this after cloning the repo: ./scripts/setup-ts-submodule.sh

set -e

TS_REPO="https://github.com/microsoft/TypeScript.git"
TS_DIR="TypeScript"

echo "→ Setting up TypeScript submodule (sparse checkout)..."

# Cleanup: Remove any stale lock files
rm -f .git/modules/"$TS_DIR"/shallow.lock 2>/dev/null || true

# Step 1: Add the submodule (or update if exists)
if [ -d "$TS_DIR" ]; then
  echo "Submodule directory exists, updating..."
  git submodule update --init --depth 1 -- "$TS_DIR" 2>/dev/null || true
elif git config --file .gitmodules --get-regexp "submodule.*$TS_DIR" >/dev/null 2>&1; then
  echo "Submodule in .gitmodules but not checked out, cleaning up first..."
  # Clean up broken submodule state
  git submodule deinit -f "$TS_DIR" 2>/dev/null || true
  rm -rf ".git/modules/$TS_DIR" 2>/dev/null || true
  # Remove from .gitmodules and re-add
  git config -f .gitmodules --remove-section "submodule.$TS_DIR" 2>/dev/null || true
  git config -f .gitmodules --remove-section "submodule.typescript" 2>/dev/null || true
  echo "Adding submodule fresh..."
  git submodule add --depth 1 --name typescript -f "$TS_REPO" "$TS_DIR"
else
  echo "Adding submodule..."
  git submodule add --depth 1 -f "$TS_REPO" "$TS_DIR" 2>/dev/null || {
    echo "Failed to add submodule, trying update instead..."
    git submodule update --init --depth 1 -- "$TS_DIR" || {
      echo "Error: Could not setup submodule. Cleaning up and retrying..."
      git submodule deinit -f "$TS_DIR" 2>/dev/null || true
      rm -rf ".git/modules/$TS_DIR" 2>/dev/null || true
      git rm -f "$TS_DIR" 2>/dev/null || true
      git submodule add --depth 1 -f "$TS_REPO" "$TS_DIR"
    }
  }
fi

# Step 2: Move into the submodule directory
echo "Entering $TS_DIR..."
cd "$TS_DIR"

# Step 3: Enable sparse checkout
echo "Enabling sparse checkout..."
git sparse-checkout init --cone

# Step 4: Set the specific directories we want
# - tests/: conformance test cases
# - src/lib/: lib.d.ts source files
# - lib/: packaged lib files
# - src/compiler, src/services, src/harness: test harness dependencies
# - scripts/: build tooling for harness
echo "Checking out required directories..."
git sparse-checkout set \
    tests \
    src/lib \
    lib \
    src/compiler \
    src/services \
    src/harness \
    src/jsTyping \
    src/deprecatedCompat \
    src/server \
    src/executeCommandLine \
    src/typingsInstallerCore \
    src/cancellationToken \
    src/watchGuard \
    src/testRunner \
    scripts

# Go back to root
cd ..

# Show result
SHA=$(cd "$TS_DIR" && git rev-parse --short HEAD)
echo ""
echo "✓ Done! TypeScript@$SHA"
echo "  Tests: $TS_DIR/tests/"
echo "  Lib files: $TS_DIR/src/lib/"
echo "  Harness: $TS_DIR/src/harness/"
echo ""
echo "To build the test harness (required for conformance tests):"
echo "  cd $TS_DIR && npm ci && npx hereby tests --no-bundle"
echo ""
echo "To update TypeScript version:"
echo "  git submodule update --remote --merge $TS_DIR"
