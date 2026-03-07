#!/usr/bin/env bash
# conformance-1: parser recovery + driver/config parity
source "$(git rev-parse --show-toplevel)/scripts/sessions/_conformance-core.sh"
emit_prompt parser-recovery
cat <<'EOF'

If your primary campaign is exhausted or blocked, do not stop. Claim the first unfinished follow-on campaign from this queue that is not already actively owned by another teammate:
1. property-resolution
2. big3
3. contextual-typing
4. narrowing-flow
5. jsdoc-jsx-salsa

When switching campaigns:
- say explicitly that parser-recovery is exhausted or blocked
- announce which follow-on campaign you are claiming
- restart from `./scripts/conformance.sh analyze --campaign <campaign>`
- keep the same campaign-first, root-cause discipline
EOF
