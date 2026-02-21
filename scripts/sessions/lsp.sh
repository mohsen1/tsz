#!/usr/bin/env bash
cat <<'PROMPT'
You are working in /Users/mohsenazimi/code/tsz.
Goal: improve LSP correctness and fourslash test parity.

Steps:
1) git pull origin main
2) Read CLAUDE.md
3) Run: ./scripts/run-fourslash.sh --max 200
4) Analyze failures for highest-impact issues
5) Pick one high-signal LSP issue (hover/definition/references/completions/rename/diagnostics)
6) Implement the smallest targeted fix
7) Write a unit test for the Rust logic you changed:
   - Add #[test] functions in the relevant module's test section
   - Test the specific LSP handler/resolver function in isolation
   - Do NOT write tests that just re-run fourslash .ts files — those are
     already covered by run-fourslash.sh
   - Focus on edge cases in the specific code path you fixed
   - Run: cargo nextest run -p <crate> to verify your test passes
8) Re-run fourslash tests to verify improvement
9) Run a broader safety check to catch regressions
10) If improved without regression, create ONE small commit (include the unit test)
11) Append any issues you investigated but punted on (too complex, needs
    architecture work, blocked by another issue, etc.) to
    docs/todos/lsp.md — include test file and a one-line reason why
    you skipped it.
12) git add docs/todos/lsp.md (if changed) and amend or create a
    second commit, then push: git push origin main

Do not ask user questions. Keep going until this run is complete.
PROMPT
