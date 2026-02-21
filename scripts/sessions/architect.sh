#!/usr/bin/env bash
cat <<'PROMPT'
You are working in /Users/mohsenazimi/code/tsz.
Goal: actively improve code quality, enforce architecture rules, and reduce tech debt.

There is ALWAYS something to improve in a 100K+ LOC codebase. Your job is to
find it and fix it. Do not declare "all clear" and stop — dig deeper.

Steps:
1) git pull origin main
2) Read CLAUDE.md (architecture rules and responsibility split)
3) Check CI via: gh run list --workflow ci.yml --limit 5
   - If the latest run is red, fix it first — that's your top priority
4) Find something to improve. Check these in order and fix the FIRST issue
   you find (one fix per session — keep commits small and focused):

   a) HARD VIOLATIONS (fix immediately):
      - TypeKey leakage outside solver crate
      - Solver imports in binder
      - Checker files exceeding 2000 LOC (split them)
      - Forbidden cross-layer imports per CLAUDE.md rules

   b) DRY VIOLATIONS (duplicated logic):
      - Search for duplicated code blocks across checker modules
      - Look for copy-pasted match arms, repeated type-checking patterns
      - Extract shared helpers or consolidate into existing utilities
      - Use: grep -rn for repeated patterns, look for similar function names

   c) DEAD CODE & UNNECESSARY COMPLEXITY:
      - Find unused functions, imports, or struct fields
      - Look for #[allow(dead_code)] annotations — remove the dead code
      - Simplify overly complex match arms or nested conditionals
      - Remove commented-out code blocks

   d) FILE SIZE & MODULE ORGANIZATION:
      - Split any checker file approaching 1800+ LOC (don't wait for 2000)
      - Move misplaced logic to its correct architectural layer
      - Break up functions longer than ~100 lines

   e) CODE QUALITY:
      - Consolidate similar error-handling patterns
      - Replace magic numbers/strings with named constants
      - Improve type safety (replace stringly-typed APIs with enums)
      - Fix inconsistent naming patterns within a module

5) Implement the fix
6) Write a unit test if the change alters behavior (not needed for pure
   refactors like extracting a helper or removing dead code)
7) Run cargo nextest run to verify no regressions
8) Create ONE small, focused commit and push: git push origin main
9) If you found other issues while investigating, append them to
   docs/todos/arch-violations.md — include file path, line range, and a
   one-line description. Only update this file if you have NEW issues to
   report (not previously listed ones).

IMPORTANT: Every session should produce exactly one code-improving commit.
If you genuinely cannot find anything to improve after thorough searching
(unlikely in a 100K+ LOC codebase), only then exit without committing.

Do not ask user questions. Keep going until this run is complete.
PROMPT
