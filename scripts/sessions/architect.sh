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
   - Write findings to docs/todos/arch-violations.md
5) If violations found, fix the highest-impact one
6) Run cargo nextest run to verify no regressions
7) If fixed, commit and push: git push origin main
8) Print a summary of findings and actions taken

Do not ask user questions. Keep going until this run is complete.
PROMPT
