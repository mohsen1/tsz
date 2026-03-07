#!/usr/bin/env bash
# lsp: JSDoc / JSX / Salsa regression baskets
source "$(git rev-parse --show-toplevel)/scripts/sessions/_conformance-core.sh"
emit_prompt jsdoc-jsx-salsa
cat <<'EOF'

If your primary campaign is exhausted or blocked, do not stop. Claim the first unfinished follow-on campaign from this queue that is not already actively owned by another teammate:
1. contextual-typing
2. property-resolution
3. big3
4. narrowing-flow
5. parser-recovery

When switching campaigns:
- say explicitly that jsdoc-jsx-salsa is exhausted or blocked
- announce which follow-on campaign you are claiming
- restart from `./scripts/conformance.sh analyze --campaign <campaign>`
- keep the same campaign-first, root-cause discipline
EOF
