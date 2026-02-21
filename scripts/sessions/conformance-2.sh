#!/usr/bin/env bash
cat <<'PROMPT'
You are working in /Users/mohsenazimi/code/tsz.
Goal: improve TypeScript conformance parity (second half of test suite).

Steps:
1) git pull origin main
2) Read CLAUDE.md
3) Run: ./scripts/conformance.sh run --offset 6000
4) Run: ./scripts/conformance.sh analyze
5) Identify the highest-impact error code (most fixable failures)
6) Implement a minimal, targeted fix for that error code
7) Write a unit test for the Rust logic you changed:
   - Add #[test] functions in the relevant module's test section
   - Test the specific function/method behavior, not the full diagnostic pipeline
   - Do NOT write tests that just re-check conformance .ts files — those are
     already covered by the conformance suite
   - Focus on edge cases, boundary conditions, or the specific bug you fixed
   - Run: cargo nextest run -p <crate> to verify your test passes
8) Re-run conformance for that error code to verify improvement
9) Run a broader safety slice to catch regressions
10) If improved without regression, create ONE small commit (include the unit test)
11) Append any issues you investigated but punted on (too complex, needs
    architecture work, blocked by another issue, etc.) to
    docs/todos/conformance.md — include error code, test file, and a
    one-line reason why you skipped it.
12) git add docs/todos/conformance.md (if changed) and amend or create
    a second commit, then push: git push origin main

Do not ask user questions. Keep going until this run is complete.
PROMPT
