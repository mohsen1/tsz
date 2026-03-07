#!/usr/bin/env bash
# _conformance-core.sh — Shared template for campaign-based conformance sessions.
# Sourced by individual session scripts. Call: emit_prompt <campaign-key>
#
# NOTE: Do NOT use BASH_SOURCE or dirname for REPO_ROOT — run-session.sh
# copies session scripts to temp locations, breaking relative paths.
# Use git rev-parse instead, which works from any directory in the repo.

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"

emit_prompt() {
  local campaign="${1:?Usage: emit_prompt <campaign-key>}"
  local campaign_title="" campaign_why="" campaign_query=""
  local campaign_signals="" campaign_modules="" campaign_focus="" campaign_avoid=""

  case "$campaign" in
    parser-recovery)
      campaign_title="Parser recovery + driver/config parity"
      campaign_why="Highest ROI per week. Clears parser/config noise that cascades into checker suppressions."
      campaign_query="./scripts/conformance.sh analyze --campaign parser-recovery"
      campaign_signals="$(cat <<'EOF'
- Codes: TS1005, TS1128, TS1109, TS1434, TS5107
- Representative tests: ambiguousGenericAssertion1.ts, arrowFunctionsMissingTokens.ts,
  parser recovery import/export/class member failures, config/tsconfig parity cases
- Common smell: parser emits generic catch-all recovery codes, then checker feature code
  suppresses fallout later
EOF
)"
      campaign_modules="$(cat <<'EOF'
- crates/tsz-parser/src/**
- crates/tsz-cli/src/driver/**
- conformance wrapper / config handling where TS5107-like noise is introduced
EOF
)"
      campaign_focus="$(cat <<'EOF'
- Prefer context-specific parser recovery over downstream checker suppression
- Fix driver/config parity at the source, not by hiding emitted diagnostics later
- Treat TS1005 as a root cause, not as an isolated code to "implement"
EOF
)"
      campaign_avoid="$(cat <<'EOF'
- Do not "fix" parser fallout by adding checker-side suppression branches
- Do not spend the session on diagnostic wording only
- Do not pivot into emitter or formatting work
EOF
)"
      ;;
    property-resolution)
      campaign_title="Property resolution + index access proof semantics"
      campaign_why="High-confidence structural wins around TS2339/TS7053/TS2536 families."
      campaign_query="./scripts/conformance.sh analyze --campaign property-resolution"
      campaign_signals="$(cat <<'EOF'
- Codes: TS2339, TS7053, TS2536, TS2304
- Representative tests: mappedTypeRelationships.ts, staticIndexSignature2.ts,
  cannotIndexGenericWritingError.ts, constraintWithIndexedAccess.ts,
  complicatedIndexesOfIntersectionsAreInferencable.ts
- Common smell: property/index proof logic stays symbolic too long or chooses the wrong
  numeric-vs-string precedence
EOF
)"
      campaign_modules="$(cat <<'EOF'
- crates/tsz-checker/src/types/computation/access.rs
- crates/tsz-checker/src/state/type_environment/**
- crates/tsz-checker/src/state/type_analysis/**
- crates/tsz-solver/src/type_queries/**
EOF
)"
      campaign_focus="$(cat <<'EOF'
- Build one resolver-aware "is this key proven for this object?" path
- Reuse that proof path for element access, property access, mapped constraints, and writes
- Push proof semantics into shared queries/boundaries, not feature-local if branches
EOF
)"
      campaign_avoid="$(cat <<'EOF'
- Do not special-case one property access syntax form only
- Do not add new checker-local property existence heuristics if a shared query can exist
- Do not chase isolated TS2339s without a reusable proof invariant
EOF
)"
      ;;
    big3)
      campaign_title="Big 3 compatibility hardening (TS2322 / TS2339 / TS2345)"
      campaign_why="Highest structural upside. The same shared gaps create both missing and extra diagnostics."
      campaign_query="./scripts/conformance.sh analyze --campaign big3"
      campaign_signals="$(cat <<'EOF'
- Codes: TS2322, TS2339, TS2345, plus TS2416 / TS2769 spillover
- Representative tests: badInferenceLowerPriorityThanGoodInference.ts, bestChoiceType.ts,
  contextuallyTypedSymbolNamedProperties.ts, contravariantInferenceAndTypeGuard.ts,
  assignment and call-site compatibility clusters
- Common smell: feature modules bypass the shared assignability/property boundary and make
  their own mismatch or bailout decisions
EOF
)"
      campaign_modules="$(cat <<'EOF'
- crates/tsz-checker/src/query_boundaries/assignability.rs
- crates/tsz-checker/src/assignability/**
- crates/tsz-checker/src/error_reporter/assignability.rs
- feature modules that still make direct compatibility/property decisions (for example JSX)
EOF
)"
      campaign_focus="$(cat <<'EOF'
- Centralize all TS2322/TS2345/TS2416/TS2769 decisions through the same gateway
- Move feature-specific mismatch policy into relation flags / boundary helpers
- Fix false positives and missing diagnostics together, not code-by-code
EOF
)"
      campaign_avoid="$(cat <<'EOF'
- No direct CompatChecker calls from feature modules for TS2322-family paths
- No per-test suppressions for extra TS2322/TS2339/TS2345
- No local feature bailouts when the boundary can be extended instead
EOF
)"
      ;;
    contextual-typing)
      campaign_title="Contextual typing + generic inference generalization"
      campaign_why="High medium-term upside. This is the anti-whackamole path for TS7006 + Big 3 contextual failures."
      campaign_query="./scripts/conformance.sh analyze --campaign contextual-typing"
      campaign_signals="$(cat <<'EOF'
- Codes: TS7006, TS2322, TS2345, TS2769
- Representative tests: contextualTypeCaching.ts,
  contextualComputedNonBindablePropertyType.ts,
  contextualTypeOfIndexedAccessParameter.ts,
  contextualOverloadListFromArrayUnion.ts,
  instantiateContextualTypes.ts
- Common smell: contextual types are lost through Lazy/Application/indexed access/template
  shapes before relation or callback parameter typing happens
EOF
)"
      campaign_modules="$(cat <<'EOF'
- crates/tsz-checker/src/types/computation/call.rs
- crates/tsz-checker/src/types/computation/object_literal.rs
- crates/tsz-checker/src/state/type_analysis/**
- crates/tsz-solver/src/operations/generic_call.rs
- crates/tsz-checker/src/symbols/symbol_resolver.rs
EOF
)"
      campaign_focus="$(cat <<'EOF'
- Preserve contextual information through Lazy/Application/indexed access evaluation
- Prefer one reusable contextual inference service over template-specific patches
- Treat TS7006 as a downstream signal of context transport failure, not an isolated error
EOF
)"
      campaign_avoid="$(cat <<'EOF'
- No one-off callback parameter annotation hacks
- No per-test object-literal special cases unless they generalize through the shared path
- Do not stop at the first passing test if the broader contextual basket still fails
EOF
)"
      ;;
    narrowing-flow)
      campaign_title="Narrowing / control-flow parity"
      campaign_why="Mandatory for 100%. Correct CFG facts and narrowing transport flip many diagnostics at once."
      campaign_query="./scripts/conformance.sh analyze --campaign narrowing-flow"
      campaign_signals="$(cat <<'EOF'
- Codes: TS2322, TS2339, TS2345, TS18048, control-flow-related false positives
- Representative tests: computedPropertiesNarrowed.ts, optional-chain CFA cases,
  assertion predicate cases, loop back-edge / exhaustive switch / IIFE control-flow cases
- Common smell: binder CFG shape is incomplete or solver narrowing does not consume those facts
  through one shared guard/narrowing path
EOF
)"
      campaign_modules="$(cat <<'EOF'
- crates/tsz-binder/src/** (CFG facts, edges, hoisting interactions)
- crates/tsz-checker/src/flow/**
- crates/tsz-checker/src/state/state.rs
- crates/tsz-solver/src/**narrow** or relation/evaluation paths consuming flow facts
EOF
)"
      campaign_focus="$(cat <<'EOF'
- Keep CFG shape fixes in binder and narrowing semantics in solver
- Add or repair shared guard/narrowing APIs instead of checker-local narrowed-type guesses
- Validate with a basket of optional-chain, predicate, and loop/switch cases
EOF
)"
      campaign_avoid="$(cat <<'EOF'
- No checker heuristics that "pretend" a node is narrowed without CFG support
- Do not patch one syntax form if the underlying guard transport is still wrong
- Do not spend the session on message text or location-only differences
EOF
)"
      ;;
    jsdoc-jsx-salsa)
      campaign_title="JSDoc / JSX / Salsa regression baskets"
      campaign_why="These are broad consumers of the same semantic gaps. Use them to validate shared fixes, not to play feature whack-a-mole."
      campaign_query="./scripts/conformance.sh analyze --campaign jsdoc-jsx-salsa"
      campaign_signals="$(cat <<'EOF'
- Areas: jsdoc, jsx, salsa
- Representative tests: booleanLiteralsContextuallyTypedFromUnion.tsx,
  checkJsdocTypeTagOnExportAssignment2.ts, JSX generic component failures,
  Salsa JS/JSX parity regressions
- Common smell: feature-specific code bypasses the same shared solver/checker boundaries that
  other campaigns need
EOF
)"
      campaign_modules="$(cat <<'EOF'
- crates/tsz-checker/src/checkers/jsx_checker.rs
- JSDoc handling in checker/state/type lowering paths
- Salsa / JS-mode integration paths
- Shared query boundaries used by these features
EOF
)"
      campaign_focus="$(cat <<'EOF'
- Treat JSDoc/JSX/Salsa as regression baskets for shared boundary fixes
- If you touch feature code, the goal is to route into shared queries/boundaries
- Prefer fixes that also improve non-JSX / non-JSDoc cases
EOF
)"
      campaign_avoid="$(cat <<'EOF'
- No bag-of-cases JSX/JSDoc heuristics
- No message-text-only alignment work
- No local LSP/feature patches that do not improve shared semantics
EOF
)"
      ;;
    *)
      echo "Unknown campaign key: $campaign" >&2
      return 1
      ;;
  esac

  cat <<PROMPT
You are working in $REPO_ROOT.
Goal: improve TypeScript conformance parity by advancing ONE root-cause campaign.

Assigned campaign: $campaign_title
Why this campaign: $campaign_why
Primary campaign query:
  $campaign_query

Representative signals:
$campaign_signals

Likely modules:
$campaign_modules

Campaign focus:
$campaign_focus

Out of scope for this session:
$campaign_avoid

═══════════════════════════════════════════════════════════════════
PHASE 1 — SETUP
═══════════════════════════════════════════════════════════════════

1) git switch main
2) git pull --ff-only origin main
3) Read AGENTS.md (architecture rules and ownership boundaries)
4) Read docs/todos/conformance.md
   Focus on:
   - Strategic Analysis
   - Root-Cause Campaigns
   - Known Architecture Drift
   - Most recent session notes for your campaign
5) Verify pre-commit hooks: ls -la .git/hooks/pre-commit
   If missing, run: ./scripts/setup.sh
   NEVER use --no-verify on git commit.

═══════════════════════════════════════════════════════════════════
PHASE 2 — OFFLINE TRIAGE FIRST (mandatory)
═══════════════════════════════════════════════════════════════════

6) Read scripts/conformance-snapshot.json and scripts/conformance-detail.json.
   Treat them as source of truth for planning.
   Do NOT run the full conformance suite for research.

7) Run:
     ./scripts/conformance.sh analyze --campaigns
     $campaign_query

   Optional supporting queries:
     python3 scripts/query-conformance.py --campaign $campaign
     python3 scripts/query-conformance.py --close 2
     python3 scripts/query-conformance.py --one-missing
     python3 scripts/query-conformance.py --false-positives

8) Build a representative basket of 3-6 failing tests for THIS campaign.
   Requirements:
   - Same underlying invariant, not just same error code
   - Prefer a mix of missing + extra diagnostics if the campaign has both
   - Use JSDoc/JSX/Salsa as regression baskets, not first-choice root causes

9) For each basket test, inspect the source file and compare expected vs actual:
     ./scripts/conformance.sh run --filter "<test_name>" --verbose

   Only run targeted tests. No full suite yet.

═══════════════════════════════════════════════════════════════════
PHASE 3 — CLASSIFY BEFORE CODING
═══════════════════════════════════════════════════════════════════

10) You MUST write this down before changing code:

    CAMPAIGN: $campaign_title
    REPRESENTATIVE TEST BASKET: <3-6 tests>
    SHARED INVARIANT: <one sentence>

    ROOT CAUSE LAYER:
      [ ] Parser — AST / recovery / driver/config parity
      [ ] Binder — symbols / scopes / CFG facts
      [ ] Solver — type evaluation / inference / relation / narrowing
      [ ] Checker — boundary routing / orchestration / diagnostic selection
      [ ] Emitter — only if this campaign explicitly requires it

    SPECIFIC GAP: <what exactly is wrong>
    FIX BELONGS IN: <which file/function/query boundary>
    ESTIMATED SCOPE: <lines of code>
    BLAST RADIUS: <which other tests should flip if correct>

11) Anti-whackamole rules:
    - Do NOT optimize for a single test first.
    - Do NOT pick a new campaign mid-session.
    - Do NOT spend the session on message text or location-only cleanup.
    - Do NOT add feature-local heuristics if a shared boundary/query can be extended instead.
    - If you cannot name the shared invariant, keep investigating. Do NOT code yet.

═══════════════════════════════════════════════════════════════════
PHASE 4 — IMPLEMENT
═══════════════════════════════════════════════════════════════════

12) Implement ONE general fix in the owning layer.
    Preferred order:
    - Solver/query-boundary fix
    - Checker routing/boundary fix
    - Binder/Parser fix if the campaign clearly belongs there

13) Add focused unit tests in the relevant module:
    - Test the semantic function/query/boundary directly
    - Do NOT add new conformance-harness-only tests
    - Cover the edge case and one nearby variant if cheap

14) Verify with targeted commands only:
    - cargo nextest run -p <crate>
    - ./scripts/conformance.sh run --filter "<basket test 1>" --verbose
    - ./scripts/conformance.sh run --filter "<basket test 2>" --verbose
    - Run a slightly broader targeted filter if useful for regression coverage

    If you fixed error-code parity but fingerprint parity is still off, note it.
    Do not pivot into message-text work unless the core semantic fix is already done.

═══════════════════════════════════════════════════════════════════
PHASE 5 — COMMIT OR DOCUMENT
═══════════════════════════════════════════════════════════════════

15) If the targeted basket improves without regression:
    - Create ONE small commit
    - NEVER use --no-verify
    - Keep commit scope aligned to this campaign

16) If you spent the session on deep investigation but did not finish the fix:
    - Do NOT commit a half-working change
    - DO write detailed notes to docs/todos/conformance.md:
      - shared invariant
      - exact owning function/query that must change
      - what tsc does differently
      - estimated scope for the real fix
      - which representative tests would flip

17) Only after a real conformance-affecting fix lands:
    - Run ./scripts/conformance.sh snapshot once
    - Commit the refreshed snapshot if it changed

18) Push to main:
    - git push origin main
    - If someone else pushed first: git pull --rebase origin main
    - If snapshot files conflict, keep going and regenerate the snapshot once after rebase

19) If context is getting large, use /compact before continuing.

Do not ask user questions. Keep going until this run is complete.
PROMPT
}
