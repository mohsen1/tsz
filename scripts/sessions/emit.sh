#!/usr/bin/env bash
cat <<'PROMPT'
You are working in /Users/mohsenazimi/code/tsz.
Goal: improve emitter correctness and parity with TypeScript output.

Steps:
1) git pull origin main
2) Read CLAUDE.md
3) Read docs/todos/emit.md — this contains notes from previous sessions
   (known issues, skipped items, prior investigations). Use it to avoid
   re-investigating already-known issues and to pick up where the last session
   left off.
4) Run: ./scripts/emit/run.sh
5) Analyze the output for mismatches and regressions
6) Pick the highest-impact emitter issue
7) Implement the smallest targeted fix in the emitter/transform pipeline
8) Write a unit test for the Rust logic you changed:
   - Add #[test] functions in the relevant module's test section
   - Test the specific transform/emit function behavior in isolation
   - Do NOT write tests that just re-run the emit test suite on .ts files —
     those are already covered by scripts/emit/run.sh
   - Focus on edge cases in the specific code path you fixed
   - Run: cargo nextest run -p <crate> to verify your test passes
9) Re-run emitter checks to verify improvement
10) Run a broader safety check to catch regressions
11) If improved without regression, create ONE small commit (include the unit test)
12) Append any issues you investigated but punted on (too complex, needs
    architecture work, blocked by another issue, etc.) to
    docs/todos/emit.md — include test file and a one-line reason why
    you skipped it.
13) git add docs/todos/emit.md (if changed) and amend or create a
    second commit, then push: git push origin main

Do not ask user questions. Keep going until this run is complete.
PROMPT
