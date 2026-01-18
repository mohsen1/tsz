#!/bin/bash

# Setup TypeScript submodule with sparse checkout (tests/ directory only)
# Run this after cloning the repo: ./scripts/setup-ts-submodule.sh

set -e

TS_REPO="https://github.com/microsoft/TypeScript.git"
TS_DIR="TypeScript"

echo "→ Setting up TypeScript submodule (sparse: tests/ only)..."

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

# Step 4: Set the specific directory we want
echo "Checking out tests/ directory only..."
git sparse-checkout set tests

# Go back to root
cd ..

# Show result
SHA=$(cd "$TS_DIR" && git rev-parse --short HEAD)
echo ""
echo "✓ Done! TypeScript@$SHA"
echo "  Tests location: $TS_DIR/tests/"
echo ""
echo "To update later:"
echo "  git submodule update --remote --merge $TS_DIR"
