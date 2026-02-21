#!/usr/bin/env bash
cat <<'PROMPT'
You are working in /Users/mohsenazimi/code/tsz.
Goal: incrementally improve repo layout, code organization, and code quality.

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

   b) FILE & MODULE ORGANIZATION (incremental cleanup):
      This is your bread and butter. Make the repo layout clean and intuitive.
      Do ONE of these per session:
      - Group related top-level files into meaningful subdirectories
        (e.g., checker files like call_checker.rs, assignability_checker.rs,
        assignment_checker.rs could live under a common subdirectory)
      - Move functions that ended up in the wrong file to the right one
        (e.g., a function in context.rs that only deals with class checking
        belongs in the classes/ directory)
      - Split large files (1500+ LOC) into focused sub-modules BEFORE they
        hit the 2000 line limit
      - Fix inconsistent directory naming (e.g., hyphens vs underscores)
      - Flatten unnecessary nesting or consolidate tiny files (<50 LOC) that
        belong together
      - Ensure mod.rs files have clear, organized re-exports
      - Move misplaced logic to its correct architectural layer

      When moving files/functions:
      - Update all imports and mod declarations
      - Keep re-exports from old locations if other crates depend on them,
        but mark with a // TODO: remove re-export comment
      - Run cargo nextest run to verify nothing breaks

   c) DRY VIOLATIONS (duplicated logic):
      - Search for duplicated code blocks across modules
      - Look for copy-pasted match arms, repeated patterns
      - Extract shared helpers or consolidate into existing utilities

   d) DEAD CODE & UNNECESSARY COMPLEXITY:
      - Find unused functions, imports, or struct fields
      - Look for #[allow(dead_code)] — remove the dead code instead
      - Simplify overly complex match arms or nested conditionals
      - Remove commented-out code blocks

   e) CODE QUALITY:
      - Consolidate similar error-handling patterns
      - Replace magic numbers/strings with named constants
      - Fix inconsistent naming patterns within a module
      - Break up functions longer than ~100 lines

5) Implement the fix
6) Write a unit test if the change alters behavior (not needed for pure
   refactors like moving files or removing dead code)
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
