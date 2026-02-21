#!/usr/bin/env bash
cat <<'PROMPT'
You are working in /Users/mohsenazimi/code/tsz.
Goal: architecture audit and CI health check.

Steps:
1) git pull origin main
2) Read docs/architecture/NORTH_STAR.md and CLAUDE.md
3) Check CI via: gh run list --workflow ci.yml --limit 5
   - If the latest run is red, investigate and fix
4) Architecture audit:
   - Check for TypeKey leakage outside solver crate
   - Check for solver imports in binder
   - Check for checker files exceeding 2000 LOC
   - Check for forbidden cross-layer imports per CLAUDE.md rules
5) If violations found:
   a) Fix the highest-impact one
   b) Run cargo nextest run to verify no regressions
   c) Update docs/todos/arch-violations.md with the new findings
   d) Commit the fix + updated report and push: git push origin main
6) If NO violations found:
   - Do NOT commit. Do NOT update docs/todos/arch-violations.md.
   - Just print a short "Architecture audit: all clear" summary and exit.
   - An "all clear" result is a success — it does not need a commit.
7) If any checker files are within 50 lines of 2000 LOC limit, note them
   in your summary output (but do NOT commit just to report near-threshold
   files — only commit if you actually split or fix a file).

IMPORTANT: Only commit when you make an actual code change (fix a violation,
split a large file, fix CI). Documentation-only commits that just say
"all clear" are wasteful — do not create them.

Do not ask user questions. Keep going until this run is complete.
PROMPT
