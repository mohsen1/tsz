#!/usr/bin/env bash
# =============================================================================
# setup-machine.sh — Turnkey setup for a new machine to run campaign agents
# =============================================================================
#
# Usage:
#   scripts/session/setup-machine.sh              # Setup with defaults
#   scripts/session/setup-machine.sh --agents 5   # Specify number of agents
#
# What it does:
#   1. Runs project setup (git hooks, submodules)
#   2. Configures shared CARGO_TARGET_DIR to save disk
#   3. Ensures .worktrees is gitignored
#   4. Verifies the build works
#   5. Shows available campaigns and instructions
#
# For overnight machines — just run this script, then start agents.
#
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"

NUM_AGENTS=5
while [[ $# -gt 0 ]]; do
    case "$1" in
        --agents) NUM_AGENTS="$2"; shift 2 ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

echo "============================================="
echo "TSZ Machine Setup"
echo "============================================="
echo ""

# --- 1. Project setup ---
echo "Step 1: Project setup..."
if [[ -f "$REPO_ROOT/scripts/setup/setup.sh" ]]; then
    bash "$REPO_ROOT/scripts/setup/setup.sh" 2>&1 | tail -5
else
    echo "  setup.sh not found, skipping"
fi

# --- 2. Submodules ---
echo ""
echo "Step 2: Git submodules..."
git -C "$REPO_ROOT" submodule update --init --recursive 2>&1 | tail -3 || echo "  (no submodules or already up to date)"

# --- 3. Shared CARGO_TARGET_DIR ---
echo ""
echo "Step 3: Shared build cache..."
SHARED_TARGET="$HOME/.cache/tsz-target"
mkdir -p "$SHARED_TARGET"

if [[ -z "${CARGO_TARGET_DIR:-}" ]]; then
    echo "  Setting CARGO_TARGET_DIR=$SHARED_TARGET"
    echo ""
    echo "  Add this to your shell profile for persistence:"
    echo "    # For bash:"
    echo "    echo 'export CARGO_TARGET_DIR=$SHARED_TARGET' >> ~/.bashrc"
    echo "    # For zsh:"
    echo "    echo 'export CARGO_TARGET_DIR=$SHARED_TARGET' >> ~/.zshrc"
    echo "    # For fish:"
    echo "    echo 'set -gx CARGO_TARGET_DIR $SHARED_TARGET' >> ~/.config/fish/config.fish"
    export CARGO_TARGET_DIR="$SHARED_TARGET"
else
    echo "  CARGO_TARGET_DIR already set: $CARGO_TARGET_DIR"
fi

# --- 4. Gitignore .worktrees ---
echo ""
echo "Step 4: Worktree directory..."
if ! git -C "$REPO_ROOT" check-ignore -q .worktrees 2>/dev/null; then
    echo ".worktrees" >> "$REPO_ROOT/.gitignore"
    echo "  Added .worktrees to .gitignore"
else
    echo "  .worktrees already gitignored"
fi
mkdir -p "$REPO_ROOT/.worktrees"

# --- 5. Build verification ---
echo ""
echo "Step 5: Build verification..."
echo "  Running cargo check (this may take a few minutes on first run)..."
if cargo check --manifest-path "$REPO_ROOT/Cargo.toml" 2>&1 | tail -3; then
    echo "  Build: OK"
else
    echo "  Build: FAILED"
    echo "  Fix build errors before starting agents."
    exit 1
fi

# --- 6. Show available campaigns ---
echo ""
echo "============================================="
echo "Setup complete! Available campaigns:"
echo "============================================="

"$SCRIPT_DIR/check-status.sh" --compact 2>/dev/null || {
    echo ""
    grep -E '^  [a-z]' "$SCRIPT_DIR/campaigns.yaml" | sed 's/://' | while read -r name; do
        printf "  %s\n" "$name"
    done
}

echo ""
echo "============================================="
echo "Quick Start ($NUM_AGENTS agents):"
echo "============================================="
echo ""
echo "Option A — Launch all agents headlessly:"
echo ""
echo "  scripts/session/launch-agents-opencode.sh --max $NUM_AGENTS"
echo ""
echo "Option B — Interactive mode (one terminal per agent):"
echo ""
echo '  # Pick an AVAILABLE campaign from the list above'
echo '  scripts/session/start-campaign.sh <campaign-name>'
echo '  cd .worktrees/<campaign-name>'
echo '  opencode -m alibaba/qwen3.6-plus'
echo ""
echo "Each agent will plan, implement, verify ALL test suites, and push to main."
echo ""
echo "Designate one session as the integrator:"
echo '  scripts/session/start-campaign.sh integrator'
echo '  cd .worktrees/integrator'
echo '  opencode -m alibaba/qwen3.6-plus'
echo ""
echo "Monitor headless agents:"
echo '  tail -f scripts/session/logs/<campaign>.log'
echo '  cat scripts/session/logs/agent-pids.txt'
echo ""
