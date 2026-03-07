#!/usr/bin/env bash
# architect: narrowing / control-flow parity
source "$(git rev-parse --show-toplevel)/scripts/sessions/_conformance-core.sh"
emit_prompt narrowing-flow
cat <<'EOF'

If your primary campaign is exhausted or blocked, do not stop. Claim the first unfinished follow-on campaign from this queue that is not already actively owned by another teammate:
1. big3
2. contextual-typing
3. property-resolution
4. parser-recovery
5. jsdoc-jsx-salsa

When switching campaigns:
- say explicitly that narrowing-flow is exhausted or blocked
- announce which follow-on campaign you are claiming
- restart from `./scripts/conformance.sh analyze --campaign <campaign>`
- keep the same campaign-first, root-cause discipline
EOF
