#!/usr/bin/env bash
cat <<'PROMPT'
You are working in /Users/mohsenazimi/code/tsz.
Goal: improve TypeScript conformance parity (second half of test suite).

Steps:
1) git pull origin main
2) Read CLAUDE.md
3) Read docs/todos/conformance.md — this contains notes from previous sessions
   (known issues, skipped items, prior investigations). Use it to avoid
   re-investigating already-known issues and to pick up where the last session
   left off. Prioritize items marked as high-impact or easy wins.
4) Verify pre-commit hooks are installed: ls -la .git/hooks/pre-commit
   If missing, run: ./scripts/setup.sh
   NEVER use --no-verify on git commit. The pre-commit hook runs tests and
   lint to catch regressions BEFORE they reach CI. Skipping it is forbidden.
5) Run: ./scripts/conformance.sh run --offset 6000
6) Run: ./scripts/conformance.sh analyze
7) Identify the highest-impact error code (most fixable failures)
8) Implement a minimal, targeted fix for that error code
9) Write a unit test for the Rust logic you changed:
   - Add #[test] functions in the relevant module's test section
   - Test the specific function/method behavior, not the full diagnostic pipeline
   - Do NOT write tests that just re-check conformance .ts files — those are
     already covered by the conformance suite
   - Focus on edge cases, boundary conditions, or the specific bug you fixed
   - Run: cargo nextest run -p <crate> to verify your test passes
10) Re-run conformance for that error code to verify improvement
11) Run a broader safety slice to catch regressions
12) If improved without regression, create ONE small commit (include the unit test).
    NEVER use --no-verify. Let the pre-commit hook run all tests and lint.
    If the hook fails, fix the issue and try again — do NOT bypass the hook.
13) Append any issues you investigated but punted on (too complex, needs
    architecture work, blocked by another issue, etc.) to
    docs/todos/conformance.md — include error code, test file, and a
    one-line reason why you skipped it.
14) git add docs/todos/conformance.md (if changed) and amend or create
    a second commit, then push: git push origin main

Do not ask user questions. Keep going until this run is complete.
PROMPT
