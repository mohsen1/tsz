#!/usr/bin/env bash
# conformance-3: Big 3 compatibility hardening
source "$(git rev-parse --show-toplevel)/scripts/sessions/_conformance-core.sh"
emit_prompt big3
cat <<'EOF'

If your primary campaign is exhausted or blocked, do not stop. Claim the first unfinished follow-on campaign from this queue that is not already actively owned by another teammate:
1. contextual-typing
2. property-resolution
3. narrowing-flow
4. parser-recovery
5. jsdoc-jsx-salsa

When switching campaigns:
- say explicitly that big3 is exhausted or blocked
- announce which follow-on campaign you are claiming
- restart from `./scripts/conformance.sh analyze --campaign <campaign>`
- keep the same campaign-first, root-cause discipline
EOF
