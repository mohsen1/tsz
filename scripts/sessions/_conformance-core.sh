#!/usr/bin/env bash
# _conformance-core.sh — Shared template for all conformance sessions.
# Sourced by individual session scripts. Call: emit_prompt <rank>
#
# Rank determines which worst-pass-rate area the session targets:
#   rank 1 = worst area, rank 2 = second worst, etc.
# This prevents duplicate work across parallel sessions.
#
# NOTE: Do NOT use BASH_SOURCE or dirname for REPO_ROOT — run-session.sh
# copies session scripts to temp locations, breaking relative paths.
# Use git rev-parse instead, which works from any directory in the repo.

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"

emit_prompt() {
  local rank="${1:?Usage: emit_prompt <rank>}"

  cat <<PROMPT
You are working in $REPO_ROOT.
Goal: improve TypeScript conformance parity by fixing the DEEPEST type-system
gaps, prioritized by worst-performing feature area.

═══════════════════════════════════════════════════════════════════
PHASE 1 — SETUP
═══════════════════════════════════════════════════════════════════

1) git pull origin main
2) Read CLAUDE.md (architecture rules and responsibility split)
3) Read docs/todos/conformance.md — contains notes from previous sessions.
   Use it to avoid re-investigating known issues and to pick up where
   the last session left off. Prioritize items marked high-impact.
4) Verify pre-commit hooks: ls -la .git/hooks/pre-commit
   If missing, run: ./scripts/setup.sh
   NEVER use --no-verify on git commit.

═══════════════════════════════════════════════════════════════════
PHASE 2 — GET CONFORMANCE DATA (use cached snapshot if fresh)
═══════════════════════════════════════════════════════════════════

5) Check if scripts/conformance-snapshot.json exists.
   Read it and check the "git_sha" field against current HEAD:
     current_sha=\$(git rev-parse HEAD)
   If the snapshot's git_sha matches current HEAD → skip to step 7.

6) If stale or missing, generate a fresh snapshot:
     ./scripts/conformance.sh snapshot
   This runs the full test suite + analysis and saves results.
   The snapshot includes BOTH error-code AND fingerprint pass rates.

7) Read scripts/conformance-snapshot.json. The "areas_by_pass_rate" array
   is sorted worst-first. Your assigned rank is: $rank
   Pick the area at index $((rank - 1)) (0-based) from that array.

   Fallback rules:
   - First, consider only areas with pass_rate < 30% AND failed >= 15.
   - If fewer than $rank areas qualify, pick the LAST qualifying area.
   - If NO areas are < 30%, use areas < 50% instead.
   - If NO areas are < 50%, pick from all areas sorted by pass rate.

   IMPORTANT: Once you pick your area, LOCK ONTO IT for the entire session.
   Do NOT re-read the snapshot or re-pick your area mid-session even if
   another session pushes an updated snapshot. Your area is fixed at this step.

   Also note the snapshot summary which includes:
   - pass_rate_error_code: error-code level pass rate
   - pass_rate_fingerprint: fingerprint level pass rate (code + location + message)
   - fingerprint_total / fingerprint_passed: exact fingerprint counts
   Use this to understand overall progress and gap between ec and fp rates.

═══════════════════════════════════════════════════════════════════
PHASE 3 — DRILL DOWN & CLASSIFY (mandatory before coding)
═══════════════════════════════════════════════════════════════════

8) Drill into your chosen area:
     ./scripts/conformance.sh areas --depth 2 --drilldown <area>

9) Pick ONE specific failing test from that area.
   Read the .ts file, read the tsc expected output (from the snapshot or
   by running with --verbose), read our actual output.

   Use --verbose to get FULL fingerprint-level detail (code + location + message):
     ./scripts/conformance.sh run --filter "<test_name>" --verbose
   This shows the exact mismatch between expected and actual diagnostics
   including line numbers and diagnostic message text.

10) CLASSIFICATION (you MUST write this down before writing any code):

    TEST FILE: <path>
    EXPECTED (fingerprint): <error codes + locations + messages>
    ACTUAL (fingerprint):   <our output codes + locations + messages>

    ROOT CAUSE LAYER:
      [ ] Parser — AST is wrong
      [ ] Binder — symbols/scopes are wrong
      [ ] Solver — type evaluation/relation is wrong
      [ ] Checker — orchestration/diagnostic emission is wrong
      [ ] Emitter — output formatting is wrong

    SPECIFIC GAP: <what exactly is wrong>
    FIX BELONGS IN: <which file/function>
    ESTIMATED SCOPE: <lines of code>
    OTHER TESTS AFFECTED: <estimated count in this area>

    Priority ladder:
      SOLVER gap   → HIGHEST VALUE. Spend the full session. Even partial
                     progress is valuable.
      CHECKER gap  → HIGH if calling solver queries. MEDIUM if just if-branch.
      PARSER/BINDER → MEDIUM. Usually localized.
      Message text  → LOW. Skip unless real fix done early.
      Emitter fmt   → ZERO VALUE in conformance sessions.

═══════════════════════════════════════════════════════════════════
PHASE 4 — SHALLOW WORK BAN
═══════════════════════════════════════════════════════════════════

11) BEFORE implementing your fix, check:
      ./scripts/conformance.sh areas --min-tests 15
    Does ANY area have pass rate below 30%?

    If YES → your fix MUST target one of those sub-30% areas, and MUST
    be classified as SOLVER or CHECKER level. Do NOT work on:
      - Diagnostic message text alignment
      - Config validation edge cases
      - Emitter formatting
      - Removing stale #[ignore] markers
      - Code organization / refactoring

    If NO (all areas >= 30%) → shallower fixes are acceptable.

═══════════════════════════════════════════════════════════════════
PHASE 5 — IMPLEMENT
═══════════════════════════════════════════════════════════════════

12) Implement the fix. Focus on solver-level changes that compound.

13) Write a #[test] in the relevant module for edge cases you fixed:
    - Add #[test] functions in the module's test section
    - Test the specific function/method behavior, not the diagnostic pipeline
    - Do NOT write tests that re-check conformance .ts files
    - Focus on edge cases, boundary conditions, the specific bug
    - Run: cargo nextest run -p <crate> to verify

14) Re-run conformance for the specific area:
      ./scripts/conformance.sh run --filter "<area_pattern>"
    Verify improvement without regression.

    Also run with --verbose on the specific tests you fixed to confirm
    fingerprint-level match (not just error-code level):
      ./scripts/conformance.sh run --filter "<test_name>" --verbose
    If error-code passes but fingerprint doesn't, note the gap — it may
    indicate a message text or location offset issue.

═══════════════════════════════════════════════════════════════════
PHASE 6 — COMMIT OR DOCUMENT
═══════════════════════════════════════════════════════════════════

15) If improved without regression:
    Create ONE small commit (include the unit test).
    NEVER use --no-verify. Let the pre-commit hook run.
    If the hook fails, fix the issue — do NOT bypass.

    If you spent the session investigating but couldn't finish the fix:
    a) Do NOT commit a half-working partial fix.
    b) DO write detailed analysis to docs/todos/conformance.md:
       - Which solver function needs to change
       - What tsc's logic does (with file:line references if found)
       - What our solver currently does differently
       - Estimated LOC for the full fix
       - Which tests would flip once fixed
    c) This is a VALID session outcome. Deep understanding is prerequisite
       to correct fixes.

16) Update the conformance snapshot:
      ./scripts/conformance.sh snapshot
      git add scripts/conformance-snapshot.json
      git commit -m "chore: update conformance snapshot"

17) Push: git push origin main
    If push fails (someone else pushed first):
      git pull --rebase origin main
    If rebase conflicts on scripts/conformance-snapshot.json:
      Just accept whatever version git gives you and continue:
        git checkout --ours scripts/conformance-snapshot.json
        git add scripts/conformance-snapshot.json
        git rebase --continue
      Then regenerate the snapshot with your fix included:
        ./scripts/conformance.sh snapshot
        git add scripts/conformance-snapshot.json
        git commit -m "chore: update conformance snapshot"
        git push origin main

18) If context is getting large, use /compact before continuing.

Do not ask user questions. Keep going until this run is complete.
PROMPT
}
