#!/usr/bin/env bash
# conformance-2: property resolution + index access proof semantics
source "$(git rev-parse --show-toplevel)/scripts/sessions/_conformance-core.sh"
emit_prompt property-resolution
cat <<'EOF'

If your primary campaign is exhausted or blocked, do not stop. Claim the first unfinished follow-on campaign from this queue that is not already actively owned by another teammate:
1. big3
2. contextual-typing
3. narrowing-flow
4. jsdoc-jsx-salsa
5. parser-recovery

When switching campaigns:
- say explicitly that property-resolution is exhausted or blocked
- announce which follow-on campaign you are claiming
- restart from `./scripts/conformance.sh analyze --campaign <campaign>`
- keep the same campaign-first, root-cause discipline
EOF
