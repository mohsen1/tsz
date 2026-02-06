#!/usr/bin/env bash
# Convenience wrapper for ask-gemini.mjs
# Usage: ./ask.sh [--preset] "your question"
#
# Examples:
#   ./ask.sh --solver "How does type inference work?"
#   ./ask.sh "General question"

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Forward all arguments to the ask-gemini.mjs script
node "$PROJECT_ROOT/scripts/ask-gemini.mjs" "$@"
