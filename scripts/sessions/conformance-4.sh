#!/usr/bin/env bash
# conformance-4: contextual typing + generic inference generalization
source "$(git rev-parse --show-toplevel)/scripts/sessions/_conformance-core.sh"
emit_prompt contextual-typing
cat <<'EOF'

If your primary campaign is exhausted or blocked, do not stop. Claim the first unfinished follow-on campaign from this queue that is not already actively owned by another teammate:
1. big3
2. property-resolution
3. narrowing-flow
4. jsdoc-jsx-salsa
5. parser-recovery

When switching campaigns:
- say explicitly that contextual-typing is exhausted or blocked
- announce which follow-on campaign you are claiming
- restart from `./scripts/conformance.sh analyze --campaign <campaign>`
- keep the same campaign-first, root-cause discipline
EOF
