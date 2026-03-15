# Architecture Audit: Claude Code Prompts

Generated from the TSZ architecture audit (2026-03-15). Each section targets one
of the five architectural problems identified, with five escalating prompts per
problem — from diagnosis through implementation.

---

## Problem 1: Query Boundaries Bypass (159 direct solver imports vs 28 boundary files)

The `query_boundaries` module was designed to mediate all checker→solver calls,
but 159 checker files import `tsz_solver` directly while only 28 boundary files
exist. This 5.7:1 bypass ratio means the boundary is advisory, not enforced.

### Prompt 1.1 — Inventory & Classify Direct Imports

```
Audit every `use tsz_solver::` import in crates/tsz-checker/src/. For each
import, classify it into one of these categories:

1. SAFE: TypeId, TypeData, structural shapes (ObjectShape, FunctionShape, etc.),
   visitor functions (is_array_type, union_list_id, etc.) — these are read-only
   type handles that don't perform computation.
2. COMPUTATION: calls to solver functions that perform type computation —
   is_subtype_of, instantiate_type, instantiate_generic, infer_generic_function,
   evaluate_*, apply_contextual_type, etc.
3. CONSTRUCTION: calls that build new types — TypeInterner methods, type factory
   functions, ObjectShape::new, etc.
4. INTERNAL: anything that accesses solver-internal state — TypeKey, raw interner
   handles, cache internals.

Output a markdown table with columns: file path, import item, category, and
whether a query_boundaries wrapper already exists for this import. Do NOT make
any code changes. This is research only.
```

### Prompt 1.2 — Map Coverage Gaps in query_boundaries

```
Read every file in crates/tsz-checker/src/query_boundaries/ and catalog the
solver APIs that are already wrapped. Then cross-reference against the
COMPUTATION and CONSTRUCTION category imports from the direct-import audit.

For each unwrapped solver API that appears in 3+ checker files, document:
- The solver function/type being used
- Which checker files use it
- What the wrapper signature should look like
- Whether it should go in an existing query_boundaries submodule or a new one

Output a prioritized list sorted by number of call sites (most used first).
Do NOT make code changes yet.
```

### Prompt 1.3 — Create Wrappers for Top-10 Most-Used Bypasses

```
Using the coverage gap analysis, implement query_boundaries wrappers for the 10
most frequently bypassed solver COMPUTATION APIs. Follow these rules:

1. Place each wrapper in the appropriate existing query_boundaries submodule
   (assignability.rs for relation queries, checkers/call.rs for call-related, etc.)
2. Each wrapper should be a thin `pub(crate) fn` that delegates to the solver
3. Add a doc comment explaining what solver API it wraps
4. Do NOT change any caller code yet — just add the wrappers
5. Ensure the module's mod.rs re-exports the new functions

Run `cargo check -p tsz-checker` to verify compilation. Commit with message
"feat(checker): add query_boundaries wrappers for top-10 bypassed solver APIs".
```

### Prompt 1.4 — Migrate Callers to Use Wrappers

```
For each of the 10 new query_boundaries wrappers created in the previous step,
find all checker files that call the underlying solver function directly and
migrate them to use the wrapper instead.

Rules:
- Replace `tsz_solver::some_function(...)` with
  `crate::query_boundaries::some_function(...)`
- Remove the now-unused `use tsz_solver::some_function` import
- If the file still imports other tsz_solver items, keep those imports
- Do NOT change any behavior — this is a pure mechanical refactor

After migration, run `cargo check -p tsz-checker` and
`cargo test -p tsz-checker -- architecture` to verify. Commit each submodule
migration separately for easy review.
```

### Prompt 1.5 — Add Lint Enforcement for Boundary Policy

```
Add a new architecture contract test in
crates/tsz-checker/src/tests/architecture_contract_tests.rs that enforces the
query_boundaries policy. The test should:

1. Scan all .rs files under crates/tsz-checker/src/ (excluding tests/ and
   query_boundaries/ itself)
2. Parse `use tsz_solver::` imports
3. Maintain an allowlist of "SAFE" imports (TypeId, TypeData, visitor functions,
   structural shapes) that may be imported directly
4. FAIL if any non-allowlisted tsz_solver import is found outside
   query_boundaries/
5. Print a clear error message: "File {path} imports tsz_solver::{item} directly.
   Add a wrapper in query_boundaries/ and use that instead."

The allowlist should be a const array at the top of the test for easy
maintenance. Run the test and fix any immediate violations it catches, or add
them to a TODO list if the wrapper doesn't exist yet.
```

---

## Problem 2: Checker File Size Violations (14 files exceed 2000 LOC)

CLAUDE.md mandates "Checker files should stay under ~2000 LOC" but 14 files
exceed this limit, with the largest at 2476 lines. These monoliths accumulate
logic that should be factored into focused submodules.

### Prompt 2.1 — Profile the Worst Offenders

```
For each of the following files that exceed 2000 LOC, analyze their internal
structure and identify natural split points:

1. crates/tsz-checker/src/state/type_resolution/core.rs (2476 lines)
2. crates/tsz-checker/src/types/utilities/jsdoc.rs (2456 lines)
3. crates/tsz-checker/src/types/computation/complex.rs (2419 lines)
4. crates/tsz-checker/src/types/function_type.rs (2334 lines)
5. crates/tsz-checker/src/state/variable_checking/core.rs (2298 lines)

For each file, output:
- A list of logical sections (groups of related functions)
- Line ranges for each section
- Dependencies between sections (which sections call each other)
- A recommended split plan that would bring each resulting file under 1500 LOC
- Any shared state that would need to be passed between the new modules

Do NOT make any code changes. This is analysis only.
```

### Prompt 2.2 — Split type_resolution/core.rs

```
Split crates/tsz-checker/src/state/type_resolution/core.rs (2476 lines) into
focused submodules. Based on the analysis, this file likely contains:

- Core type resolution dispatch logic
- Literal type handling
- Generic/conditional type resolution
- Mapped type resolution
- Union/intersection resolution helpers

Create new files under state/type_resolution/ for each logical group. The main
core.rs should become a thin dispatcher that re-exports from submodules. Rules:

1. Each new file must be under 1000 LOC
2. Move complete function groups — don't split individual functions
3. Use `pub(crate)` for inter-module visibility
4. Update mod.rs to declare new submodules
5. Run `cargo check -p tsz-checker` after each file move
6. Run `cargo test -p tsz-checker` at the end to verify no regressions

Commit with message "refactor(checker): split type_resolution/core.rs into
focused submodules".
```

### Prompt 2.3 — Split types/computation/complex.rs

```
Split crates/tsz-checker/src/types/computation/complex.rs (2419 lines) into
focused submodules. Read the file first and identify the major function groups.

Likely candidates for extraction:
- Conditional type computation
- Mapped type computation
- Template literal type computation
- Index access type computation
- Keyof type computation

Create files under types/computation/ for each group. Follow the same rules as
the type_resolution split: each file under 1000 LOC, complete function groups,
pub(crate) visibility, cargo check after each move.

Commit with message "refactor(checker): split types/computation/complex.rs into
focused submodules".
```

### Prompt 2.4 — Split function_type.rs and variable_checking/core.rs

```
Split these two files into focused submodules:

1. crates/tsz-checker/src/types/function_type.rs (2334 lines)
   - Likely groups: signature construction, overload resolution, parameter
     checking, return type inference, generator/async handling
   - Create submodules under types/function_type/ (convert the file to a
     directory module)

2. crates/tsz-checker/src/state/variable_checking/core.rs (2298 lines)
   - Likely groups: declaration checking, assignment validation, definite
     assignment, scope-based resolution, destructuring
   - Create submodules under state/variable_checking/

Rules: each file under 1000 LOC, complete function groups, cargo check after
each move, cargo test at end. Commit each file split separately.
```

### Prompt 2.5 — Add LOC Budget Enforcement

```
Add a test in crates/tsz-checker/src/tests/architecture_contract_tests.rs that
enforces the 2000 LOC limit. The test should:

1. Walk all .rs files under crates/tsz-checker/src/
2. Exclude test files (tests/, test_utils.rs) and mod.rs files
3. Count non-empty, non-comment lines for each file
4. FAIL if any file exceeds 2000 lines
5. Maintain an allowlist of files that are grandfathered in (currently being
   worked on) with their current line count as a ceiling — they can shrink but
   not grow
6. Print: "File {path} has {n} lines (limit: 2000). Split into submodules."

For the grandfathered files, set the ceiling to their current line count so they
can only get smaller over time. Run the test and verify it passes with the
current allowlist.
```

---

## Problem 3: Diagnostic Centralization Gaps (13 direct push_diagnostic calls outside error_reporter)

The architecture requires diagnostics to flow through `error_reporter/` for
consistent formatting and solver-reason-based generation. But 13 calls in 7
files bypass this, creating ad-hoc diagnostic paths.

### Prompt 3.1 — Catalog All Bypass Diagnostics

```
Find every call to push_diagnostic (or ctx.push_diagnostic, self.ctx.push_diagnostic,
self.push_diagnostic) in crates/tsz-checker/src/ that is NOT inside the
error_reporter/ directory.

For each call, document:
- File path and line number
- The diagnostic code being emitted (e.g., TS2322, TS7006)
- Whether it uses a solver SubtypeFailureReason or constructs the message locally
- The context: what checker operation triggers this diagnostic
- Whether an equivalent error_reporter method already exists

Output a table. Do NOT make code changes.
```

### Prompt 3.2 — Identify Missing error_reporter Methods

```
Based on the bypass diagnostic catalog, identify which diagnostics have NO
corresponding error_reporter method. For each missing method:

1. Read the bypass code to understand what diagnostic information it needs
2. Check if the solver provides a SubtypeFailureReason or other structured
   failure data for this case
3. Design the error_reporter method signature:
   - It should accept structured data (TypeId, SymbolId, SubtypeFailureReason)
     rather than pre-formatted strings
   - It should handle source location attachment internally
   - It should follow the naming pattern of existing error_reporter methods

Output the proposed method signatures as a list. Do NOT implement yet.
```

### Prompt 3.3 — Implement Missing error_reporter Methods

```
Implement the missing error_reporter methods identified in the previous analysis.
Place each method in the appropriate error_reporter submodule:

- assignability.rs — for TS2322/TS2345/TS2416 family
- properties.rs — for property-related errors (TS2339, TS2551)
- core.rs — for general errors that don't fit elsewhere
- Create a new submodule if needed (e.g., jsdoc_errors.rs for JSDoc diagnostics)

Each method should:
1. Accept typed parameters (not pre-formatted strings)
2. Use solver's SubtypeFailureReason where applicable
3. Call self.ctx.push_diagnostic internally
4. Include the diagnostic code as a constant

Run `cargo check -p tsz-checker` after each method. Commit with message
"feat(checker): add error_reporter methods for uncentralized diagnostics".
```

### Prompt 3.4 — Migrate Bypass Callers to error_reporter

```
For each of the 13 direct push_diagnostic calls outside error_reporter/,
refactor them to use the corresponding error_reporter method:

Files to migrate:
- types/type_checking/global.rs (3 calls)
- types/utilities/jsdoc_params_helpers.rs (3 calls)
- state/state_checking/class.rs (2 calls)
- types/type_checking/unused.rs (2 calls)
- checkers/promise_checker.rs (1 call)
- context/core.rs (1 call)
- types/queries/class.rs (1 call)

For each migration:
1. Replace the inline Diagnostic::error(...) construction with the error_reporter
   method call
2. Remove any local string formatting that the error_reporter now handles
3. Verify the diagnostic message and code are preserved exactly
4. Run the relevant conformance tests to confirm no diagnostic regression

Commit each file separately for easy review.
```

### Prompt 3.5 — Add Diagnostic Centralization Guard

```
Add an architecture contract test that prevents new push_diagnostic calls from
appearing outside error_reporter/. The test should:

1. Scan all .rs files in crates/tsz-checker/src/ EXCLUDING:
   - error_reporter/ (the legitimate home for diagnostics)
   - context/core.rs (the push_diagnostic method definition itself)
   - tests/
2. Search for patterns: `push_diagnostic(`, `.push_diagnostic(`
3. Maintain an allowlist of known exceptions (empty if all migrations are done,
   or listing any that genuinely can't be moved)
4. FAIL if any non-allowlisted push_diagnostic call is found
5. Print: "File {path}:{line} calls push_diagnostic directly. Move this
   diagnostic to error_reporter/ instead."

Run the test and verify it passes. If any calls remain that genuinely cannot
be moved, add them to the allowlist with a comment explaining why.
```

---

## Problem 4: Architecture Test Coverage (1 test file for 186K LOC)

A single 1330-line test file (`architecture_contract_tests.rs`) is the only
automated guard for architectural invariants across 186K lines of checker code.
This is insufficient to prevent regression.

### Prompt 4.1 — Catalog Existing Architecture Invariants

```
Read the CLAUDE.md architecture rules (sections 3-6, 11-12, 15, 22) and the
existing architecture_contract_tests.rs. Create a checklist mapping each
architecture rule to whether it has automated test coverage:

Format:
- Rule: "Checker must not import TypeKey" → Test: [exists/missing]
- Rule: "Checker must not implement ad-hoc type algorithms" → Test: [exists/missing]
- Rule: "Solver is single source of truth for type computation" → Test: [exists/missing]
- Rule: "Checker files under 2000 LOC" → Test: [exists/missing]
- (continue for all rules)

For each missing test, note whether it can be tested via:
A) Static analysis (grep/parse imports, count lines)
B) Structural analysis (check function signatures, module dependencies)
C) Runtime assertion (check behavior during test execution)

Output the complete checklist. Do NOT write any code.
```

### Prompt 4.2 — Implement Dependency Direction Tests

```
Add tests to architecture_contract_tests.rs that enforce dependency direction
rules from CLAUDE.md section 4:

Test 1: "Binder must not import Solver"
- Scan crates/tsz-binder/src/**/*.rs for `use tsz_solver`
- FAIL if any import found

Test 2: "Emitter must not import Checker internals"
- Scan crates/tsz-emitter/src/**/*.rs for `use tsz_checker`
- FAIL if any import found (except pub diagnostics API)

Test 3: "Scanner must not import Parser/Binder/Checker/Solver"
- Scan crates/tsz-scanner/src/**/*.rs for imports from downstream crates
- FAIL if found

Test 4: "Parser must not import Binder/Checker/Solver"
- Same pattern

Test 5: "CLI must consume diagnostics via tsz_checker::diagnostics only"
- Scan crates/tsz-cli/src/**/*.rs for tsz_checker imports
- FAIL if importing from tsz_checker::types or other internal paths

Run all tests and fix any violations found. Commit with message
"test(checker): add dependency direction architecture tests".
```

### Prompt 4.3 — Implement Solver Encapsulation Tests

```
Add tests that enforce solver encapsulation rules:

Test 1: "No TypeKey in checker"
- Grep all checker source for `TypeKey`
- FAIL if found anywhere (TypeKey is solver-internal)

Test 2: "No raw interner access in checker"
- Grep checker source for direct TypeInterner method calls that bypass
  query_boundaries
- Check for patterns like `.intern(`, `.get_type_data(` outside of
  query_boundaries/

Test 3: "No solver cache access in checker"
- Grep checker source for QueryCache, RelationCacheKey, RelationCacheProbe
- These are solver-internal cache types

Test 4: "Checker does not construct SubtypeChecker directly"
- Grep for `SubtypeChecker::new` or `SubtypeChecker {` in checker source
  outside of query_boundaries/
- Relation checks should go through boundary helpers

Test 5: "Checker does not construct CompatChecker directly for TS2322 paths"
- Per CLAUDE.md section 22, TS2322-family must route through
  query_boundaries/assignability

Run tests and document any violations found. Commit with message
"test(checker): add solver encapsulation architecture tests".
```

### Prompt 4.4 — Implement Structural Health Tests

```
Add tests that enforce structural health rules:

Test 1: "No checker file exceeds 2000 LOC"
- Walk all .rs files, count lines, fail on violations
- Allowlist with ceiling counts for grandfathered files

Test 2: "All diagnostics route through error_reporter"
- Grep for push_diagnostic outside error_reporter/
- Allowlist for known exceptions

Test 3: "query_boundaries coverage ratio"
- Count files importing tsz_solver directly vs through boundaries
- WARN (not fail) if ratio exceeds 4:1
- This is a directional metric, not a hard gate

Test 4: "No semantic type computation in checker dispatch"
- Scan dispatch.rs for patterns that suggest inline type computation
  rather than solver delegation
- Check for match arms on TypeData variants (should use visitors)

Test 5: "Checker modules stay within responsibility boundaries"
- Verify that files in assignability/ don't import from declarations/
- Verify that files in flow/ don't import from types/computation/
- Cross-cutting imports suggest responsibility confusion

Run tests, document violations, commit.
```

### Prompt 4.5 — Create Architecture Test Runner Script

```
Create scripts/architecture-check.sh that:

1. Runs all architecture contract tests:
   cargo test -p tsz-checker -- architecture_contract

2. Runs a quick static analysis pass:
   - Count checker files over 2000 LOC
   - Count direct tsz_solver imports vs query_boundaries usage
   - Count push_diagnostic calls outside error_reporter
   - Count TODO/FIXME/HACK comments in checker code

3. Outputs a summary report:
   Architecture Health Report
   ==========================
   LOC violations:     X files over 2000 LOC (limit: 0)
   Boundary bypasses:  X direct imports (target: < 50)
   Diagnostic leaks:   X calls outside error_reporter (target: 0)
   Code smells:        X TODO/FIXME/HACK markers
   Test result:        PASS/FAIL

4. Returns exit code 1 if any architecture tests fail

Add this script to the pre-commit hook in scripts/githooks/ so it runs
automatically. Make it fast (< 5 seconds) by using grep, not cargo test,
for the static analysis portion.
```

---

## Problem 5: Solver API Surface Sprawl (70+ flat re-exports, no structural organization)

The solver's `lib.rs` re-exports 70+ items in a flat namespace. This makes the
boundary between "safe to use from checker" and "solver-internal" unclear, and
prevents trait-based API organization that would enable compile-time enforcement.

### Prompt 5.1 — Categorize the Solver's Public API

```
Read crates/tsz-solver/src/lib.rs and categorize every `pub use` re-export into
these API tiers:

Tier 1 — TYPE HANDLES (safe for anyone to import):
  TypeId, TypeData, ObjectShapeId, FunctionShapeId, etc.
  These are identity types with no computation.

Tier 2 — VISITORS (read-only queries, safe for checker):
  is_array_type, union_list_id, lazy_def_id, contains_type_parameters, etc.
  These inspect types but don't modify or create them.

Tier 3 — COMPUTATION (should go through query_boundaries):
  is_subtype_of, instantiate_type, evaluate_*, apply_contextual_type, etc.
  These perform type computation and are the primary boundary concern.

Tier 4 — CONSTRUCTION (type building, needs careful boundary management):
  TypeInterner methods, type factory functions, shape constructors.

Tier 5 — INTERNAL (should NOT be in pub API at all):
  Cache types, raw interner handles, internal state types.

Output a categorized list with item counts per tier. Identify any Tier 5 items
that are currently public but shouldn't be.
```

### Prompt 5.2 — Design Trait-Based API Modules

```
Based on the API categorization, design a trait-based organization for the
solver's public API. The goal is to replace flat re-exports with structured
module boundaries:

1. `tsz_solver::types` — Tier 1 type handles (TypeId, TypeData, shapes)
2. `tsz_solver::visitors` — Tier 2 read-only query functions
3. `tsz_solver::relations` — Tier 3 subtype/compat/overlap queries
4. `tsz_solver::evaluation` — Tier 3 type evaluation queries
5. `tsz_solver::instantiation` — Tier 3 instantiation/substitution
6. `tsz_solver::construction` — Tier 4 type building (behind TypeDatabase trait)

For each module, draft:
- The public items it would export
- A trait interface if applicable (e.g., `trait TypeRelations { fn is_subtype_of(...) }`)
- How checker code would import from the new structure

This is a DESIGN document only. Output it as a markdown spec. Do NOT write code.
```

### Prompt 5.3 — Organize Solver Exports into Modules

```
Restructure crates/tsz-solver/src/lib.rs to organize exports into the module
tiers designed in the previous step. This is a NON-BREAKING refactor:

1. Create facade modules in lib.rs that group related re-exports:
   ```rust
   pub mod type_handles { pub use crate::types::{TypeId, TypeData, ...}; }
   pub mod visitors { pub use crate::visitors::{is_array_type, ...}; }
   pub mod relations { pub use crate::relations::{is_subtype_of, ...}; }
   ```

2. KEEP all existing flat re-exports for backwards compatibility
3. Add #[doc(hidden)] or deprecation notices on flat re-exports that have
   module-based alternatives
4. Update the crate-level documentation to point users to the module-based API

Run `cargo check --workspace` to verify nothing breaks. Commit with message
"refactor(solver): organize public API into tiered modules".
```

### Prompt 5.4 — Migrate Checker to Module-Based Imports

```
Migrate checker code from flat solver imports to the new module-based imports:

Phase 1: Update query_boundaries/ files first (they're the intended boundary)
- Replace `use tsz_solver::is_subtype_of` with
  `use tsz_solver::relations::is_subtype_of`
- Replace `use tsz_solver::{TypeId, TypeData}` with
  `use tsz_solver::type_handles::{TypeId, TypeData}`

Phase 2: Update remaining checker files
- Use the same pattern for all SAFE (Tier 1-2) imports
- For COMPUTATION (Tier 3) imports, route through query_boundaries instead
  of switching to the new module path

After migration, remove the flat re-exports from solver's lib.rs that are no
longer used. Run `cargo check --workspace` and `cargo test -p tsz-checker`
after each phase.
```

### Prompt 5.5 — Seal Solver Internals

```
Audit and restrict solver items that should not be publicly accessible:

1. Check if any of these are currently `pub` and used by checker:
   - TypeKey (should be crate-private)
   - Internal cache types (QueryCache internals, RelationCacheKey)
   - Raw interner methods that bypass TypeDatabase trait
   - Memoization internals

2. For any that ARE used by checker, create a proper public API alternative:
   - If checker needs cache stats, add a `fn cache_statistics() -> CacheStats`
   - If checker needs interner access, ensure TypeDatabase trait covers it

3. Change visibility of true internals to `pub(crate)`:
   - Audit each item: if it's only used within tsz-solver, make it pub(crate)
   - If it's used by tsz-checker through query_boundaries, keep it pub but
     move it to the appropriate module tier

4. Run `cargo check --workspace` — any compilation errors reveal checker code
   that was reaching into solver internals and needs migration.

Commit with message "refactor(solver): seal internal types and restrict API surface".
```

---

## Usage Guide

### Recommended execution order

These prompts are designed to be executed in dependency order within each
problem, but the five problems can be worked **in parallel** across different
sessions or branches:

| Problem | Branch suggestion | Priority |
|---------|------------------|----------|
| 1. Query Boundaries | `campaign/query-boundaries` | HIGH — enables enforcement |
| 2. File Size | `campaign/checker-split` | MEDIUM — mechanical refactor |
| 3. Diagnostics | `campaign/diagnostic-centralization` | MEDIUM — correctness risk |
| 4. Architecture Tests | `campaign/arch-tests` | HIGH — prevents regression |
| 5. Solver API | `campaign/solver-api` | LOW — foundational but large |

### Prompt execution tips

- Run prompts sequentially within a problem (1.1 before 1.2, etc.)
- Problems 1 and 4 are complementary — do them together
- Problem 5 is the largest refactor; consider doing 5.1-5.2 (design) first,
  then revisit implementation after problems 1-4 are resolved
- Always run `cargo check --workspace` after code changes
- Use `./scripts/conformance/conformance.sh run --filter "pattern"` to verify
  no diagnostic regressions after changes
