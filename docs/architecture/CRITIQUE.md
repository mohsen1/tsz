# CRITIQUE: TSZ architecture and design

Here are the highest‑leverage changes that would make TSZ **fundamentally** better (not just “cleaner”), given the North Star doc’s stated principles.

## Current operating status (step mode)

This document is intended to be driven from highest impact to lowest impact:

1) lock boundaries (must happen first, no exceptions),
2) collapse the dual-type-world risk,
3) centralize assignability/explainability and then build query/incremental scaffolding.

To keep this actionable, treat each major heading below as a checkpoint you can close only when its “Definition of done” is fully met and architecture guardrail tests pass for that milestone.

## Step tracker (fill as you go)

- [x] A) Establish hard boundary checks (forbidden imports, solver/lexer direction, TypeKey leakage scan).
- [x] B) Remove checker-local semantic typing primitives from the public API by gating legacy type exports and surfacing diagnostics through `tsz_checker::diagnostics`.
- [x] C) Hide raw `TypeKey` behind solver constructors.
- [x] D) Route all TS2322/TS2345/TS2416-like compatibility checks through one gateway.
- [x] E) Move `Lazy(DefId)`-resolution and type-shape preconditions into solver visitors.
- [x] F) Add query-level cache/invalidations and connect checker to the query outputs.

### Step → AGENTS clause mapping

- A ↔ clauses 4, 6, 15, 16, 21
- B ↔ clauses 1, 3, 6, 7, 16, 19
- C ↔ clauses 2, 4, 6, 14, 16
- D ↔ clauses 4, 5, 15, 18, 22, 23
- E ↔ clauses 3, 4, 5, 10, 23
- F ↔ clauses 7, 11, 13, 18

## Evidence required per tracker step

- A: automated boundary scan and a failing/fixed sample proving forbidden imports are blocked.
- B: zero public exposure of checker-local semantic type ids/constructors by default.
- C: checker no longer imports/constructs solver `TypeKey` internals directly.
- D: one code path for each assignability diagnostic family (`TS2322`, `TS2345`, `TS2416`) in checklist + corresponding code location.
- E: no remaining checker-level type-shape recursion for `Lazy(DefId)` discovery.
- F: solver query-cache outputs are used for subtype relation reuse in checker hot paths (`is_subtype_of` consults `QueryDatabase::lookup_subtype_cache`).
- F: subtype and assignability relation memoization in checker hot paths now uses `QueryDatabase` cache APIs (`lookup_*_cache`/`insert_*_cache`) instead of checker-local `relation_cache`.

### Current implementation evidence

- A is live through `scripts/arch_guard.py` and the boundary checks in:
  - `scripts/check-checker-boundaries.sh`
  - `crates/tsz-checker/src/tests/architecture_contract_tests.rs`
  - `crates/tsz-solver/src/tests/typekey_contract_tests.rs`
- B is implemented by:
  - Gating checker legacy type re-exports under `legacy-type-arena` in `crates/tsz-checker/src/lib.rs`.
  - Moving all downstream diagnostic uses to `tsz_checker::diagnostics`.
  - Keeping `types` as an internal transition module only.
- D is implemented by routing all compatibility checks through `crates/tsz-checker/src/query_boundaries/assignability.rs`:
  - `is_assignable_with_overrides`
  - `is_assignable_with_resolver`
  - `is_assignable_bivariant_with_resolver`
  - `is_subtype_with_resolver`
  - `is_redeclaration_identical_with_resolver`
  - Verified by `crates/tsz-checker/src/tests/architecture_contract_tests.rs::test_assignability_checker_routes_relation_queries_through_query_boundaries`.
- E is implemented by moving `Lazy(DefId)`/type-shape precondition discovery to solver-provided traversals and recursively resolving dependencies from `crates/tsz-checker/src/state_type_environment.rs`:
  - `collect_referenced_types`
  - `collect_lazy_def_ids`
  - `collect_enum_def_ids`
  - `collect_type_queries`
- F is implemented in `crates/tsz-checker/src/assignability_checker.rs` by routing subtype relation reuse through `QueryDatabase` caching primitives:
  - `is_subtype_of` consults `self.ctx.types.lookup_subtype_cache` before relation computation.
  - `is_subtype_of` stores relation results with `self.ctx.types.insert_subtype_cache`.
  - `is_subtype_of` no longer uses the checker-local `relation_cache` for subtype memoization.
  - `is_assignable_to` and `is_assignable_to_bivariant` now route relation memoization through `self.ctx.types.lookup_assignability_cache` and `insert_assignability_cache` using the same `RelationCacheKey` strategy as subtype checks.
  - Checker-side relation cache containers were removed from `TypeCache`/`CheckerContext`; relation caching now lives purely in solver query caches.

## 1) Make “Solver-first” real by eliminating the second type system

Right now the checker crate still ships a **full parallel type representation** (`TypeArena`, its own `TypeId`, and a big `Type` enum + flags) that sits beside the solver’s `TypeId/TypeKey` world. That’s a structural violation of “single source of truth” and a long‑term maintenance trap because it guarantees drift, duplicate bugs, and confusion about which `TypeId` you’re holding. 

### What to do

* Pick **one** canonical semantic type system: it should be `tsz-solver`’s.
* Move anything still needed from `tsz-checker::types` into solver (or delete it).
* If you truly still need the legacy representation (debugging, migration, tests), quarantine it:

  * Put it behind a feature flag (`legacy-type-arena`)
  * Or move it into a separate crate (`tsz-checker-legacy`) so the main pipeline cannot depend on it.

### “Definition of done”

* There is exactly **one** `TypeId` type in the workspace API surface.
* The checker no longer re-exports its own `Type`, `TypeId`, `type_flags`, etc.
* No new code can accidentally mix type IDs from different worlds.

This single change will reduce conceptual complexity more than any file-splitting effort.

---

## 2) Stop letting checker code touch `TypeKey` and raw interning

Even if checker never *matches* on `TypeKey`, the moment checker can freely do `intern(TypeKey::…)`, you’ve leaked solver internals across the boundary. That defeats the point of “thin wrappers” and makes it hard to change representation/canonicalization rules later.

### What to do

In `tsz-solver`, introduce a **TypeFactory / TypeBuilder API** that covers *all* construction:

* `array(element)`
* `union(types)`
* `intersection(types)`
* `type_param(info)`
* `lazy(def_id)`
* `index_access(obj, key)`
* `mapped(...)`, `conditional(...)`, etc.

…and keep `TypeKey` **crate-private** inside the solver.

Checker becomes incapable of building malformed/non-canonical types, and the solver becomes the only place where invariants live (union flattening, sorting, dedup, “readonly unwrap rules”, etc.).

### “Definition of done”

* `TypeKey` is not imported by checker at all.
* The only interner entrypoints checker sees are safe constructors.

---

## 3) Collapse TS2322 / assignability into a single, policy-driven gate

Your doc already has the right parity strategy: **relation first**, **reason second**, **location/message last**. The fundamental improvement is to make it *impossible* to bypass that pipeline.

### What to do

Create exactly one “assignability gate” used everywhere (assignment, args, returns, property writes, spreads, etc.):

* Input: `(source TypeId, target TypeId, RelationPolicy, TypeEnvironment snapshot, context)`
* Output: `RelationResult { related: bool, reason: Option<ReasonTree> }`

Checker responsibilities:

* choose span(s)
* choose suppression strategy (error/any/unknown cascades, etc.)
* map reason → TS diagnostic code/message (or let solver provide canonical templates)

Solver responsibilities:

* all compatibility rules (Judge/Lawyer)
* all special cases (`() => void`, weak types, excess properties, `any` mode, variance)
* explainable failure reasons (structured)

### “Definition of done”

* There are **zero** ad-hoc “type A vs type B” structural checks outside solver.
* All TS2322-ish error emission goes through the same `check_assignable(...)` wrapper.

---

## 4) Move traversal + “type preconditions” out of checker recursion

`ensure_refs_resolved` is a good example of something that’s currently too coupled: checker is doing deep traversal over type structure via `TypeTraversalKind`, and it must stay in sync with every type variant forever. 

This violates the doc’s “Visitor patterns” rule in spirit: the traversal logic is still in checker; it’s just *indirect*.

### What to do

In solver, add visitor utilities that return exactly what checker needs for “WHERE”:

* `collect_lazy_def_ids(type_id) -> SmallVec<DefId>`
* `collect_type_queries(type_id) -> SmallVec<SymbolRef>`
* `walk_referenced_types(type_id, |child| ...)`

Then checker does only:

1. iterate DefIds → resolve → insert into `TypeEnvironment`
2. call solver relation query

Checker should not know how to recurse through tuples/functions/mapped/conditional/etc.

### “Definition of done”

* Checker does not contain any “type graph traversal” code.
* Adding a new `TypeKey` variant requires updating solver visitors, not checker modules.

---

## 5) Consolidate caches: checker should not own solver-computation caches

Your checker’s shared cache has a lot of entries that are *type evaluation* concerns (application eval, mapped eval, object spread property collection, element access computation, etc.). Those are “WHAT” caches and belong in solver memoization, not checker state. 

### What to do

* Keep only these caches in checker:

  * `node -> TypeId`
  * `symbol -> TypeId`
  * LSP-level “project graph” caches (reverse deps, file versioning)
  * flow-analysis caches tied to CFG nodes (that’s “WHERE”)

* Move these into solver:

  * evaluation caches (mapped, conditional, application)
  * property collection caches
  * index access / keyof / template literal evaluation caches
  * relation caches keyed by `RelationCacheKey` and policy flags

### “Definition of done”

* Checker cache shrinks to “AST & symbols & flow & diagnostics”.
* Solver becomes the only place with memoization for type algorithms.

This makes correctness better (one cache semantics) and also reduces the risk of “cache poisoning” inconsistencies.

**Completed in this iteration (Milestone 5 sub-item):**

* Added solver-level query/caching for object spread and element access type extraction in `tsz-solver`:
  * `QueryDatabase::resolve_element_access_type`
  * `QueryDatabase::collect_object_spread_properties`
  * `QueryCache` cache keys and memoized query implementations
* Added `ObjectLiteralBuilder::collect_spread_properties` and routed checker spread/type-extraction behavior through the query boundary via `CheckerState::collect_object_spread_properties` and `get_element_access_type`.

---

## 6) Turn the “North Star” into an enforceable contract (not a wiki page)

You already have architecture tests mentioned, but the doc itself is drifting from reality (it claims specific file sizes/line counts and even repeats section numbering).  Drift is deadly because it teaches contributors the doc is aspirational, not binding.

### What to do

**Enforce** the rules mechanically:

* **Dependency direction tests**

  * checker must not depend on solver internals (TypeKey, internal modules)
  * binder must not import solver
  * emitter must not import checker

* **Forbidden import checks**

  * fail CI if `tsz-checker` imports `tsz_solver::types::TypeKey` (or any internal module)
  * fail CI if checker contains `match` on type internals (if you still have any)

* **Auto-generated architecture metrics**

  * generate a file report (top N largest modules, forbidden deps, etc.)
  * don’t hand-maintain “state.rs is X lines” in docs; generate it

### “Definition of done”

* If someone violates “solver-first”, CI breaks immediately.
* The doc is either auto-synced or only states rules that are enforced.

---

## 7) Make incremental + LSP a first-class design constraint (query engine)

The doc implies salsa-like caching (“QueryDatabase traits”), reverse dependencies, and persistent state. The next step is to formalize it into a consistent query layer so incremental correctness doesn’t depend on “did we remember to invalidate that HashMap”.

### What to do

Adopt a real query model for:

* `parse(file) -> NodeArena`
* `bind(file) -> BinderState`
* `def_info(def_id) -> DefinitionInfo`
* `type_of_symbol(sym) -> TypeId`
* `type_of_node(node) -> TypeId`
* `relation(source, target, policy) -> RelationResult`

Whether you use `salsa` or a minimal homegrown equivalent, the key is:

* explicit dependencies
* deterministic invalidation
* cheap recomputation

### “Definition of done”

* edits invalidate only impacted queries
* you can explain “why did this recompute” and “what depends on what”

---

## 8) Correctness strategy: differential + minimization beats heroic debugging

You already have a conformance harness crate in the repo. The big jump is to make it do three things relentlessly:

1. **differential diagnostics** against `tsc`
2. **testcase minimization** when you diverge
3. **performance regression detection** on the same suite

### What to do

* Always store:

  * tsz diagnostics (structured)
  * tsc diagnostics (structured)
  * normalized diff (by code, span, message template)
* When mismatch occurs:

  * auto-minimize the input (delta-debug) until you get the smallest reproducer
  * keep that as a permanent regression test

### “Definition of done”

* “Parity work” becomes an assembly line, not ad-hoc debugging.

---

# If you only do three things

1. **Delete/quarantine the checker’s legacy type system** so there is one semantic `TypeId` universe. 
2. **Hide `TypeKey` behind solver constructors** so checker cannot build or depend on solver internals.
3. **Make one assignability gate** (relation → reason → diag) and force every TS2322-ish path through it.


# TSZ Roadmap 2026: Make the North Star Real

**Goal:** converge the implementation toward the North Star architecture in a way that is (a) enforceable, (b) testable, (c) incremental/LSP-friendly, and (d) parity-driven.

This roadmap is designed so every milestone has:

* **Objective** (what gets better)
* **Deliverables** (what code exists afterward)
* **Exit criteria** (how you know it’s done)
* **Follow-on unlocks** (what it enables next)

---

## Non‑negotiable invariants

These are the “project constitution.” Every milestone either enforces one or removes blockers.

1. **Single semantic type universe**

* Exactly one `TypeId` and one canonical type graph/interning system.
* No parallel “checker-local type system” that can diverge.
  (Right now the checker still contains a full `TypeArena` and type constructors like `create_union`, `create_intersection`, etc., which is precisely the drift risk.)

2. **Sealed type representation**

* `TypeKey` is not accessible in Checker (and ideally not outside solver/types internals).
* Checker can only request types via solver constructors/query APIs.
  (Right now checker code directly imports `TypeKey` and interns it.)

3. **Solver-first “WHAT”**

* All type algorithms live in solver: relations, evaluation, instantiation, narrowing, canonicalization, explanation.
* Checker owns only “WHERE”: AST traversal, source spans, diagnostic selection, suppression policy.

4. **One relation gateway**

* Every assignability/subtype/comparability check used for diagnostics routes through a single checker gateway (which in turn calls solver relation + solver explain).

5. **Type traversal lives in solver**

* Checker must not recursively traverse type structures to “prepare” solver checks.
  (Right now checker performs deep traversal in `ensure_refs_resolved` via traversal classification and manually recurses into type shapes.)

6. **DefId-centric resolution is guaranteed**

* `Lazy(DefId)` resolution is consistent and centralized (no scattered “insert into env if cached path skipped” patches).

7. **Architecture is enforced by CI**

* If someone violates boundaries, CI fails immediately.

---

## Workstreams (kept in sync by milestones)

* **A. Guardrails & enforcement**
* **B. Type system consolidation**
* **C. Solver API + purity**
* **D. Relations + Explain + diagnostics**
* **E. Canonicalization & interning invariants**
* **F. Query graph / incremental core**
* **G. Performance & memory**
* **H. Parity + testing + minimization**
* **I. Documentation & contributor UX**

The milestones below interleave these so you don’t “refactor blind.”

---

# Milestone 0 — Guardrails first (make drift impossible)

### Objective

Stop architectural regression while you refactor aggressively.

### Deliverables

1. **Architecture CI checks**

* Forbid forbidden imports/patterns (examples below).
* Enforce dependency direction (crate/module boundaries).

2. **Auto-generated architecture report**

* Largest files/modules, forbidden import hits, “TypeKey leakage count,” etc.

3. **Parity harness skeleton**

* A minimal “run tsc + run tsz + diff diagnostics” pipeline (even if diff is crude initially).

### Exit criteria

* CI fails if Checker imports `TypeKey`.
* CI fails if Solver imports parser/checker crates.
* Report is generated in CI artifacts (or printed) each run.

### Reference implementation: one complete guard script

Drop this in `scripts/arch_guard.py` and run it in CI.

```python
#!/usr/bin/env python3
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]

CHECKS = [
    # 1) Checker must not touch TypeKey
    ("Checker imports TypeKey",
     ROOT / "crates" / "tsz-checker",
     re.compile(r"\btsz_solver::.*TypeKey\b|\bTypeKey::", re.MULTILINE)),

    # 2) Checker must not reach into solver internals module tree (tighten as needed)
    ("Checker imports solver internals",
     ROOT / "crates" / "tsz-checker",
     re.compile(r"\btsz_solver::types::\b|\btsz_solver::internals::\b", re.MULTILINE)),

    # 3) Solver must not depend on parser/checker (adjust paths to your workspace layout)
    ("Solver imports parser/checker crates",
     ROOT / "crates" / "tsz-solver",
     re.compile(r"\btsz_parser::\b|\btsz_checker::\b", re.MULTILINE)),
]

EXCLUDE_DIRS = {".git", "target", "node_modules"}

def iter_rs_files(base: pathlib.Path):
    for p in base.rglob("*.rs"):
        if any(part in EXCLUDE_DIRS for part in p.parts):
            continue
        yield p

def main() -> int:
    failures = []
    for name, base, pattern in CHECKS:
        if not base.exists():
            # allow flexible layouts, but be strict once stabilized
            continue
        for f in iter_rs_files(base):
            txt = f.read_text(encoding="utf-8", errors="ignore")
            if pattern.search(txt):
                failures.append((name, f))
    if failures:
        print("ARCH GUARD FAILURES:")
        for name, f in failures:
            print(f" - {name}: {f.relative_to(ROOT)}")
        return 1
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
```

---

### Status update (2026-02-14)

* **Status:** Completed (except ongoing hardening)
* **Completed in this iteration (Milestone 0 sub-item):**
  * Enforced dependency-direction **freeze guardrail** in `scripts/check-checker-boundaries.sh` so CI fails if any new non-test solver source (outside legacy `crates/tsz-solver/src/lower.rs`) imports `tsz_parser::` or `tsz_checker::`.
  * Added focused architecture contract coverage in `crates/tsz-checker/src/tests/architecture_contract_tests.rs` that recursively scans non-test solver source files and fails on parser/checker import patterns outside the `lower.rs` legacy quarantine.
  * Added a checker boundary guardrail in `scripts/check-checker-boundaries.sh` that fails on any non-test `tsz_solver::types::...` import path usage, forcing checker code onto public `tsz_solver::*` APIs only.
* **Remaining for Milestone 0:**
  * Complete the `lower.rs` migration so solver has zero parser/checker crate imports.
  * Add/standardize architecture report generation in CI artifacts. **Completed in this iteration.**
  * Keep parity harness skeleton aligned with the roadmap deliverables.

# Milestone 1 — Unify the type system (one `TypeId` world)

### Objective

Eliminate the biggest correctness + maintainability risk: having two competing semantic type representations.

### Current blocker (from code)

The checker crate still contains a full local type arena and constructors for unions/intersections/templates/etc. That guarantees drift and confusion about which `TypeId` you hold. 

### Deliverables

1. **Deprecate/remove checker-local type system**

* Identify every use of the checker `TypeArena` / checker `TypeId`.
* Replace with solver `TypeId` + solver constructors/query APIs.
* Quarantine any legacy components behind a feature flag if you can’t delete immediately.

2. **One canonical “type construction API”**

* A solver-owned `TypeFactory` / `TypeBuilder` API is the only way to create compound types.

### Exit criteria

* The checker does not define/own a semantic `TypeArena` that can construct semantic types.
* There is one “type creation surface” in solver used by checker.

### Unlocks

* You can safely harden TypeKey privacy (next milestone).
* You can trust memoization/canonicalization decisions.

### Status update (2026-02-14)

* **Status:** In progress
* **Completed in this iteration (Milestone 1 sub-item):**
  * Quarantined checker legacy `TypeArena` surface behind an explicit crate feature flag:
    * `crates/tsz-checker/Cargo.toml` now defines `legacy-type-arena` (off by default).
    * `crates/tsz-checker/src/lib.rs` now gates `pub mod arena;` and `pub use arena::TypeArena;` behind `#[cfg(feature = "legacy-type-arena")]`.
  * Added focused architecture contract coverage in `crates/tsz-checker/src/tests/architecture_contract_tests.rs` to lock this boundary and fail if the legacy `TypeArena` module/re-export become default-visible again.
* **Remaining for Milestone 1:**
  * Continue migrating/deleting checker-local type-system internals (`types`/`arena`) from active checker paths.
  * Remove the `legacy-type-arena` feature entirely once migration users are gone.

---

# Milestone 2 — Seal the solver representation (no `TypeKey` leakage)

### Objective

Make it mechanically impossible for checker code to depend on solver internals.

### Current blocker (from code)

Checker modules import `TypeKey` and intern types directly (example: array type helper that does `intern(TypeKey::Array(..))`).

### Deliverables

1. **Introduce solver constructors**
   Add a module like:

* `solver::factory::{array, union, intersection, readonly, keyof, index_access, function, callable, ...}`

2. **Migrate all checker construction calls**

* Replace direct `intern(TypeKey::...)` with `factory::*` calls.

3. **Make `TypeKey` crate-private**

* `pub(crate)` or private, depending on crate layout.
* Re-export only `TypeId` + safe APIs.

### Exit criteria

* `tsz-checker` compiles without importing `TypeKey`.
* CI guard forbidding TypeKey in checker stays green.

### Unlocks

* Canonicalization can be centralized and enforced in constructors.
* Solver internals can evolve without huge refactors.

### Status update (2026-02-14)

* **Status:** In progress
* **Completed in this iteration (Milestone 2 sub-item):**
  * Migrated checker array-oriented construction sites from direct `TypeKey` interning to solver constructor APIs:
    * `create_array_type` now uses `types.array(...)`
    * `ReadonlyArray<T>` construction paths now use `types.readonly_type(types.array(...))`
  * Added an architecture regression test that guards these array helper paths against direct `TypeKey` interning.
* **Completed in this iteration (Milestone 2 sub-item, follow-up):**
  * Added solver-owned safe constructors for `keyof` and indexed access (`types.keyof(...)`, `types.index_access(...)`).
  * Migrated checker type-node and JSX intrinsic-element indexed access paths away from direct `TypeKey` interning:
    * `type_node` now uses `types.readonly_type(...)`, `types.keyof(...)`, and `types.index_access(...)`
    * `jsx_checker` now uses `types.index_access(...)`
  * Added focused regression tests:
    * architecture guard assertions for `type_node` and `jsx_checker` to prevent direct `TypeKey` usage on these paths
    * solver interner test covering `keyof` and `index_access` constructor behavior
* **Completed in this iteration (Milestone 2 sub-item, follow-up):**
  * Added solver-owned safe constructor for type parameters (`types.type_param(...)`) and reused `types.lazy(...)` constructor APIs in checker paths.
  * Migrated remaining direct checker interning in this slice:
    * `context::create_lazy_type_ref` now uses `types.lazy(...)`
    * `state_type_resolution` default generic argument application now uses `types.lazy(...)`
    * `type_checking_queries` now uses `types.lazy(...)` and `types.type_param(...)` instead of direct `TypeKey` interning
  * Added focused regression tests:
    * architecture guard assertions for `context`, `state_type_resolution`, and `type_checking_queries` to prevent direct `TypeKey::Lazy`/`TypeKey::TypeParameter` interning in these helpers
    * solver interner test covering `lazy` and `type_param` constructor behavior
* **Completed in this iteration (Milestone 2 sub-item, follow-up):**
  * Migrated `state_checking_members` missing-name type-parameter scope construction away from direct `intern(TypeKey::TypeParameter(...))` calls to solver constructor API (`types.type_param(...)`).
  * Extended checker architecture guard coverage so `state_checking_members` is explicitly checked for `TypeKey::TypeParameter` regressions.
* **Completed in this iteration (Milestone 2 sub-item, follow-up):**
  * Migrated `control_flow_narrowing` ArrayBuffer predicate construction away from direct `intern(TypeKey::Lazy(...))` calls to solver constructor API (`types.lazy(...)`).
  * Extended checker architecture guard coverage so `control_flow_narrowing` is explicitly checked for `TypeKey::Lazy` regression on this path.
* **Completed in this iteration (Milestone 2 sub-item, follow-up):**
  * Added solver-owned safe constructor for type queries (`types.type_query(...)`).
  * Migrated `state_type_analysis` direct interning sites for `TypeKey::TypeQuery`, `TypeKey::TypeParameter`, and `TypeKey::Lazy` to solver constructor APIs (`types.type_query(...)`, `types.type_param(...)`, `types.lazy(...)`).
  * Extended checker architecture guard coverage so `state_type_analysis` is explicitly checked for regressions on these constructors.
  * Added focused solver interner coverage for `type_query` constructor behavior.
* **Completed in this iteration (Milestone 2 sub-item, follow-up):**
  * Migrated `function_type::push_enclosing_type_parameters` away from direct `intern(TypeKey::TypeParameter(...))` to solver constructor API (`types.type_param(...)`).
  * Extended checker architecture guard coverage so `function_type` is explicitly checked for `TypeKey::TypeParameter` regression on this path.
* **Completed in this iteration (Milestone 2 sub-item, follow-up):**
  * Added solver-owned safe constructor for nominal enum types (`types.enum_type(def_id, structural_type)`).
  * Migrated checker enum/lazy/literal construction paths away from direct interning in this slice:
    * `state_type_analysis` now uses `types.enum_type(...)` for enum/member nominal types and `types.lazy(...)` for namespace lazy references.
    * `state_type_environment` now uses `types.enum_type(...)` for enum object-member properties and `types.literal_string_atom(...)` for mapped-key literal substitution.
  * Extended focused regression tests:
    * checker architecture guard assertions for `state_type_analysis` and `state_type_environment` to prevent direct `TypeKey::Enum`/`TypeKey::Literal` interning regressions.
    * solver interner coverage for `enum_type` constructor behavior.
* **Completed in this iteration (Milestone 2 sub-item, follow-up):**
  * Migrated `type_computation_complex::resolve_type_param_for_construct` away from direct `intern(tsz_solver::TypeKey::TypeParameter(...))` to solver constructor API (`types.type_param(...)`).
  * Extended checker architecture guard coverage so `type_computation_complex` is explicitly checked for `TypeKey::TypeParameter` regression on this path.
* **Completed in this iteration (Milestone 2 sub-item, follow-up):**
  * Added a generalized checker architecture guard test that scans checker source files for direct `TypeKey` imports and direct `intern(TypeKey::...)` usage patterns.
  * This moves Milestone 2 from path-by-path spot checks to a broad enforcement gate for checker-side `TypeKey` leakage.
* **Completed in this iteration (Milestone 2 sub-item, follow-up):**
  * Wired checker `TypeKey` leakage enforcement into top-level architecture CI guardrails by extending `scripts/check-checker-boundaries.sh` with an explicit failure check for direct `TypeKey` import/intern patterns in non-test checker code.
  * Strengthened checker architecture contract coverage by making the direct-`TypeKey` usage test recurse through checker source subdirectories, preventing blind spots outside top-level `src/*.rs`.
* **Completed in this iteration (Milestone 2 sub-item, follow-up):**
  * Tightened top-level checker boundary guardrails so direct `TypeKey` import/intern checks now cover all non-test checker code (including `query_boundaries`), removing the previous path exception.
  * Added an explicit raw-interner checker guard in `scripts/check-checker-boundaries.sh` that fails on `.intern(...)` usage in non-test checker code, enforcing solver-constructor-only type construction.
  * Expanded checker architecture contract tests to fail on direct raw interner usage (`.intern(...)`) in checker source files.
* **Completed in this iteration (Milestone 2 sub-item, follow-up):**
  * Hardened `scripts/arch_guard.py` checker-side `TypeKey` leakage detection to also fail on fully-qualified raw constructor usage (`intern(tsz_solver::TypeKey::...)`) and direct `TypeKey::...` construction references in non-test checker code.
  * Tuned the same guardrail to ignore comment-only lines so boundary checks fail on executable code usage, not migration/docs commentary text.
* **Completed in this iteration (Milestone 2 sub-item, follow-up):**
  * Migrated checker non-test code from solver-internal module paths (`tsz_solver::types::...`) to public solver exports (`tsz_solver::...`) across relation flags, symbol refs, visibility, intrinsic/index helpers, and index signatures.
  * Strengthened architecture contract coverage to fail if non-test checker source imports `tsz_solver::types::...`, preventing new solver-internal module coupling.
* **Completed in this iteration (Milestone 2 sub-item, follow-up):**
  * Added a solver-side enforcement rule that direct `.intern(TypeKey::...)` construction is quarantined to `tsz-solver/src/intern.rs`.
  * Added a dedicated solver contract test (`typekey_contract_tests.rs`) to enforce this invariant continuously.
* **Remaining for Milestone 2:**
  * Keep tightening guard patterns as solver constructor surface expands (for example, extend CI matching beyond direct `.intern(TypeKey::...)` aliases and new raw-construction forms).

---

# Milestone 3 — Move type traversal into solver visitors

### Objective

Checker shouldn’t need to understand the type graph structure (variants, shapes, recursion patterns).

### Current blocker (from code)

Checker does deep type traversal itself to resolve `Lazy(DefId)` and other referenced types before relation checks. That traversal must stay in sync with every type form forever. 

### Deliverables

1. **Solver visitor utilities**
   Add solver helpers like:

* `collect_lazy_def_ids(types, type_id) -> SmallVec<DefId>`
* `collect_type_queries(types, type_id) -> SmallVec<SymbolRef>`
* `walk_referenced_types(types, type_id, |child| ...)`

2. **Checker “precondition” becomes simple**
   Replace checker recursion with:

* `for def in solver::visitor::collect_lazy_def_ids(type_id) { env.ensure(def) }`

3. **Centralize DefId → TypeId insertion**

* Make “ensuring env contains def mapping” a single helper.
* Remove scattered “if cached path skipped insert_def…” patches.

### Exit criteria

* No checker code recursively walks through function shapes/tuple lists/conditional/mapped internals to find references.
* Adding a new type variant requires updating solver visitors, not checker.

### Unlocks

* Cleaner relation gateway (next milestone).
* Less bug surface area for new type variants.

### Status update (2026-02-14)

* **Status:** In progress
* **Completed in this iteration (Milestone 3 sub-item):**
  * Added solver visitor utilities for traversal-oriented preconditions:
    * `visitor::walk_referenced_types(...)`
    * `visitor::collect_lazy_def_ids(...)`
    * `visitor::collect_type_queries(...)`
  * Migrated checker `assignability_checker::ensure_refs_resolved` away from checker-owned recursive `TypeTraversalKind` branching to solver visitor collectors plus checker-only DefId/type-environment orchestration.
  * Added focused regression tests:
    * solver visitor tests for lazy/type-query collector behavior (unique + transitive collection)
    * checker architecture contract assertions preventing `assignability_checker` from regressing to direct traversal classification/branching.
* **Completed in this iteration (Milestone 3 sub-item, follow-up):**
  * Migrated checker `state_type_environment::ensure_application_symbols_resolved_inner` away from checker-owned recursive `SymbolResolutionTraversalKind` branching to solver visitor traversal (`visitor::walk_referenced_types(...)`) plus checker-only symbol/type-environment orchestration.
  * Removed now-unused checker query-boundary traversal classifier plumbing for symbol resolution traversal in `query_boundaries/state_type_environment`.
  * Added focused regression tests:
    * checker architecture contract assertions preventing `state_type_environment` from regressing to `SymbolResolutionTraversalKind`/`classify_for_symbol_resolution_traversal(...)` branching.
* **Completed in this iteration (Milestone 3 sub-item, follow-up):**
  * Migrated checker `query_boundaries/diagnostics::classify_property_traversal` away from checker-side `TypeTraversalKind`/`classify_for_traversal(...)` branching to a solver-owned query API (`type_queries::classify_property_traversal(...)`).
  * Added focused regression tests:
    * checker unit test coverage for property-traversal classification outcomes (`Object`, `Callable`, `Members`, `Other`) through the query boundary API
    * checker architecture contract assertions preventing `query_boundaries/diagnostics` regression back to direct traversal classification.
* **Completed in this iteration (Milestone 3 sub-item, follow-up):**
  * Centralized checker DefId precondition orchestration for assignability lazy-ref preparation by adding `resolve_and_insert_def_type(...)` in `state_type_environment`.
  * Migrated `assignability_checker::ensure_refs_resolved` to use the centralized helper instead of inline DefId->symbol->type resolution and direct `type_env` insertion.
  * Added focused regression tests:
    * checker architecture contract assertions that `assignability_checker` uses the centralized DefId resolver helper and avoids direct `env.insert_def(...)` calls.
* **Completed in this iteration (Milestone 3 sub-item, follow-up):**
  * Migrated checker `state_type_environment::ensure_application_symbols_resolved_inner` symbol-precondition extraction from checker-owned per-node filtering over referenced types to solver visitor collectors:
    * `collect_lazy_def_ids(...)` for lazy DefId resolution
    * `collect_enum_def_ids(...)` for enum DefId resolution
    * `collect_type_queries(...)` for type-query symbol resolution
  * Kept checker focused on orchestration only (DefId/SymbolId bridge + TypeEnvironment insertion), while solver owns the traversal/filter logic for these preconditions.
  * Added focused regression tests:
    * solver visitor coverage for `collect_enum_def_ids(...)` transitive + unique behavior
    * checker architecture contract assertions that `state_type_environment` uses solver collector helpers (`collect_lazy_def_ids`, `collect_enum_def_ids`, `collect_type_queries`) for this path
* **Completed in this iteration (Milestone 3 sub-item, follow-up):**
  * Adopted solver visitor utility `visitor::collect_referenced_types(...)` for checker precondition traversal consumption instead of local callback buffering.
  * Migrated checker `state_type_environment::ensure_application_symbols_resolved_inner` from direct `walk_referenced_types(...)` callback collection to `collect_referenced_types(...)`, keeping checker focused on symbol/type-environment orchestration.
  * Added focused regression tests:
    * solver visitor coverage for `collect_referenced_types(...)` transitive + unique behavior
    * checker architecture contract assertion that `state_type_environment` uses `collect_referenced_types(...)` for traversal preconditions
* **Completed in this iteration (Milestone 3 sub-item, follow-up):**
  * Moved checker diagnostic property-name traversal recursion out of `error_reporter` into solver query API:
    * added `type_queries::collect_property_name_atoms_for_diagnostics(...)` in solver
    * added checker query-boundary wrapper `query_boundaries::diagnostics::collect_property_name_atoms_for_diagnostics(...)`
    * `error_reporter::collect_type_property_names` now handles rendering only
  * Added focused regression tests:
    * checker architecture contract assertions that `error_reporter` uses the boundary helper and no longer defines recursive traversal helper
    * checker diagnostics boundary behavior test for depth-limited, transitive property-name collection
* **Completed in this iteration (Milestone 3 sub-item, follow-up):**
  * Centralized `DefId` precondition resolution in `state_type_environment` for application-symbol resolution by introducing dedicated helpers:
    * `resolve_lazy_def_for_type_env(...)`
    * `resolve_enum_def_for_type_env(...)`
  * Migrated `ensure_application_symbols_resolved_inner` lazy/enum loops to call these helpers instead of duplicating `DefId -> SymbolId -> resolve -> insert` orchestration inline.
  * Added focused regression tests:
    * checker architecture contract assertions that `state_type_environment` keeps using the dedicated lazy/enum DefId helper path.
* **Completed in this iteration (Milestone 3 sub-item, follow-up):**
  * Aligned subtype relation preconditions with assignability in `assignability_checker` by resolving both lazy refs and application symbols before subtype cache lookup.
  * Moved subtype cache access to occur after preconditions are established, reducing stale-cache risk from precondition-dependent relation answers.
  * Added focused architecture contract coverage to lock this ordering and enforce application-symbol preconditions in `is_subtype_of` and `is_subtype_of_with_env`.
* **Completed in this iteration (Milestone 3 sub-item, follow-up):**
  * Removed redundant checker-local precondition traversal from generic-constraint validation in `crates/tsz-checker/src/generic_checker.rs` (`ensure_refs_resolved(type_arg/instantiated_constraint)`), relying on centralized `is_assignable_to(...)` precondition orchestration.
  * Added architecture contract coverage to guard generic constraint checks against reintroducing local ref-resolution traversal preconditions.
* **Completed in this iteration (Milestone 3 sub-item, follow-up):**
  * Added centralized relation precondition helpers in `crates/tsz-checker/src/assignability_checker.rs`:
    * `ensure_relation_input_ready(...)`
    * `ensure_relation_inputs_ready(...)`
  * Migrated call-resolution precondition setup in `crates/tsz-checker/src/call_checker.rs` and `crates/tsz-checker/src/type_computation_complex.rs` to use these helpers instead of open-coded `ensure_refs_resolved + ensure_application_symbols_resolved` loops.
  * Added architecture contract coverage to lock call-resolution modules onto the centralized relation precondition helper path.
* **Completed in this iteration (Milestone 3 sub-item, follow-up):**
  * Removed remaining manual application-symbol precondition loops in `crates/tsz-checker/src/type_computation_complex.rs` constructor/call resolution paths and routed both through centralized relation precondition helpers.
  * Added architecture contract coverage to prevent `type_computation_complex` from reintroducing direct `ensure_application_symbols_resolved(...)` orchestration.
* **Remaining for Milestone 3:**
  * Migrate other checker precondition traversal paths to solver visitors (beyond `ensure_refs_resolved`).

---

# Milestone 4 — One relation gateway + structured Explain everywhere

### Objective

Stop TS2322/TS2345/TS2416/etc. from each evolving their own mismatch logic and becoming inconsistent.

### Current state signal

There is already a lot of checker orchestration around assignability, suppression, weak unions, excess properties, etc. (and it’s spread across many modules). You’ve begun centralizing it, but it’s not “impossible to bypass” yet. 

### Deliverables

1. **Single checker entrypoint for relations**
   Create:

* `checker::relations::check_assignable(node_ctx, source, target, policy) -> Result<(), DiagnosticParts>`
* `checker::relations::check_subtype(...)`
* `checker::relations::check_comparable(...)`

Make all assignment/call/return/property-write checks call these.

2. **Solver Explain as a stable structured tree**

* `RelationResult { related: bool, reason: Option<ReasonTree> }`
* Reason nodes like:

  * `MissingProperty { name, expected, got }`
  * `UnionMemberFailed { member, reason }`
  * `CallSignatureMismatch { param_index, expected, got }`
  * `IndexAccessKeyMismatch { object, key }`
  * …

3. **Reason minimizer**

* Cap depth, pick smallest counterexample, compress repetitive branches.

4. **Checker renders**

* Checker picks span/message/code policy.
* Solver provides the “why.”

### Exit criteria

* There is exactly one codepath that emits TS2322-style “not assignable” diagnostics.
* No module emits assignability errors by doing custom structural checks.

### Unlocks

* Differential testing becomes dramatically easier because behavior becomes consistent.
* LSP hover/code actions can reuse structured reasons.

### Status update (2026-02-14)

* **Status:** In progress
* **Completed in this iteration (Milestone 4 sub-item):**
  * Extended the central assignability gateway in `assignability_checker` with `check_assignable_or_report_at(...)` to decouple weak-union source location from diagnostic anchor location while preserving one mismatch/suppression policy.
  * Migrated assignment compatibility diagnostics in `assignment_checker` to route through `check_assignable_or_report_at(...)` instead of open-coded `is_assignable_to + weak-union + direct error` logic.
  * Migrated binding default-value assignability diagnostics in `type_checking` to route through `check_assignable_or_report(...)` instead of open-coded mismatch checks.
  * Added architecture contract coverage to lock these gateway routes.
* **Completed in this iteration (Milestone 4 sub-item, follow-up):**
  * Added centralized bivariant mismatch decision helper `should_report_assignability_mismatch_bivariant(...)` in `assignability_checker`.
  * Migrated class-member compatibility checks (`TS2416`/`TS2417` decision path) in `class_checker` to use centralized mismatch helper entrypoints for both regular and bivariant relation modes.
  * Extended architecture contract coverage to lock class-member compatibility onto centralized mismatch helpers.
* **Completed in this iteration (Milestone 4 sub-item, follow-up):**
  * Migrated additional checker mismatch call sites to the central assignability gateway:
    * parameter initializers in `parameter_checker` now route via `check_assignable_or_report(...)`
    * `for...of` expression initializer compatibility and non-destructuring variable-initializer checks in `state_checking` now route via gateway helpers
    * `'in'` expression RHS object-compatibility checks in `type_computation` now route via `check_assignable_or_report(...)`
    * class-member and JS export-assignment style checks in `state_checking_members` now route via `check_assignable_or_report(...)`
    * `satisfies` expression assignability checks in `dispatch` now route via `check_assignable_or_report(...)`
    * non-`yield*` bare `yield` mismatch reporting in `dispatch` now routes via `check_assignable_or_report(...)`
    * call/new argument mismatch checks in `type_computation_complex` now route through `check_argument_assignable_or_report(...)`
    * call/new callback constraint-violation TS2322 paths in `type_computation_complex` now route through `check_assignable_or_report_generic_at(...)`
    * destructuring generic mismatch checks in `state_checking` now route through `check_assignable_or_report_generic_at(...)`
    * class-member mismatch decision points now route through `query_boundaries/class` helpers (`should_report_member_type_mismatch*`)
    * shared error-emission trait helpers in `error_handler` now route TS2322-style reporting through `check_assignable_or_report(...)` instead of direct reporter calls
  * Extended architecture contract coverage to lock these modules onto centralized assignability gateway entrypoints.

---

# Milestone 5 — Canonicalization in constructors (make O(1) identity meaningful)

### Objective

Guarantee that interning gives you stable identity and stable memoization keys.

### Deliverables

Canonicalize in solver constructors (not at callsites):

* **Union**

  * flatten, dedup, stable sort
  * absorb `never`, handle `any` per policy
* **Intersection**

  * flatten, dedup, stable sort
  * absorb `unknown`, handle `any` per policy
* **Object shapes**

  * stable property ordering by `Atom`
  * stable encoding of optional/readonly
* **Type applications**

  * stable arg ordering/encoding; eliminate identity substitutions if desired

### Exit criteria

* Property-based tests:

  * `union([a,b]) == union([b,a])`
  * `union([a,a]) == a`
  * `intersection([a,unknown]) == a`
* Deterministic interning across runs.

### Unlocks

* More effective solver memoization.
* More stable diagnostics and diffing.

---

# Milestone 6 — Move algorithmic caches into solver (checker keeps only “WHERE” caches)

### Objective

Prevent cache inconsistency and “cache poisoning” across layers.

### Current state signal

Checker maintains relation caches keyed by solver types and flags, does inference checks, and handles evaluation-related concerns as preconditions. That’s a smell: solver should own algorithm caching. 

### Deliverables

1. **Solver-owned caches**

* Relation result cache
* Evaluation cache (conditional/mapped/applications)
* Expensive query caches (keyof, index access, etc.)

2. **Checker caches shrink**
   Keep only:

* `node -> TypeId`
* `symbol -> TypeId`
* CFG/flow-narrowing caches
* diagnostics collection

### Exit criteria

* Checker does not maintain caches of “type algorithm outputs” except node/symbol results.
* Relation/evaluation caches live in solver DB or query engine.

### Unlocks

* Incremental query graph becomes straightforward (next milestone).

### Status update (2026-02-14)

* **Status:** In progress
* **Completed in this iteration (Milestone 6 sub-item):**
  * Removed checker algorithm-evaluation cache fields (`application_eval_*`, `mapped_eval_*`) from persistent `TypeCache` in `crates/tsz-checker/src/context.rs`.
  * Updated `with_cache` / `with_cache_and_options` to initialize those evaluation caches as context-local ephemeral state instead of restoring them from persisted cache blobs.
  * Updated `with_parent` context construction to initialize evaluation caches as context-local state instead of sharing parent algorithm-evaluation cache state across checker contexts.
  * Removed constructor-access algorithm cache fields (`abstract/protected/private constructor type` sets) from persistent `TypeCache`.
  * Updated cache-restore paths to initialize constructor-access caches as context-local state instead of restoring them from persisted cache blobs.
  * Updated `with_parent` context construction to keep constructor-access caches context-local instead of inheriting parent constructor-access cache state.
  * Removed checker-owned `contains_infer_types` memo cache state (`contains_infer_types_true`, `contains_infer_types_false`) from `CheckerContext` and switched `assignability_checker` to query solver visitor APIs directly for infer-shape detection.
  * Removed per-file resets for checker infer-shape memo state in `state_checking` because infer-shape cache ownership now lives in solver queries only.
  * Removed checker-local evaluation result caches from `CheckerContext` (`application_eval_cache`, `mapped_eval_cache`) and corresponding read/write paths in `state_type_environment`, keeping only recursion guards (`*_eval_set`) on checker side.
  * Inlined infer-shape solver query usage in assignability/subtype cacheability checks and removed checker-local wrapper helpers.
  * Continued shrinking checker cache ownership toward AST/symbol/flow concerns while moving algorithm memoization ownership to solver query APIs.
  * Added architecture contract coverage to enforce that `TypeCache` no longer exposes persisted eval or constructor-access algorithm cache fields.
  * Added architecture contract coverage to enforce that `CheckerContext` does not reintroduce checker-owned infer-shape memo caches.
  * Added architecture contract coverage to enforce that `CheckerContext` does not reintroduce application/mapped evaluation result caches.

---

# Milestone 7 — Query graph core: incremental is the default mode

### Objective

Unify CLI and LSP on one dependency-driven engine so they don’t diverge.

### Deliverables

1. **Explicit query model**

* `parse(file_id) -> AST`
* `bind(file_id) -> symbols/scopes/flow`
* `type_of_node(node_id) -> TypeId`
* `type_of_symbol(sym_id) -> TypeId`
* `relation(source, target, policy) -> RelationResult`
* `diagnostics(file_id) -> Vec<Diagnostic>`

2. **Stable invalidation**

* Keyed by file text hash/version and compiler options.
* Reverse deps managed by project.

3. **Interner lifetime strategy for LSP**

* epochs/generations or reset strategy + monitoring.

### Exit criteria

* LSP and CLI call the same query APIs.
* A file edit recomputes only dependent queries.

### Unlocks

* Real incremental speedups.
* Better memory control over long-lived sessions.

---

# Milestone 8 — Performance & memory sweep (after invariants are enforced)

### Objective

Make performance wins durable and measurable.

### Deliverables

1. **Atom-based names in hot structs**

* Replace `String` names with `Atom` where possible.
* Only materialize strings for diagnostics/emission.

2. **Arena-backed small collections**

* Replace hot-path `Vec`/`Box` allocations with:

  * typed pools (`ListId`)
  * `SmallVec` for common small cases

3. **Tracing + metrics**

* relation cache hit rate
* evaluation counts
* fuel usage distribution
* top N expensive queries

### Exit criteria

* Benchmarks show predictable improvements.
* No new allocations in hot paths (tracked by optional instrumentation builds).

---

# Milestone 9 — Parity pipeline: scoreboard + minimization + regression lock-in

### Objective

Turn “parity” into a steady manufacturing process.

### Deliverables

1. **Differential harness against `tsc`**

* Run TSZ + tsc on a corpus.
* Normalize diagnostics (code + approximate span + category).
* Report diff summary in CI.

2. **Auto-minimization**

* When mismatch is detected, shrink the input to a minimal reproducer.
* Commit minimized repro as regression test.

3. **Priority error surfaces**
   Ramp coverage in this order:

* TS2322 (assignability)
* TS2345 (call args)
* TS2339 (property access)
* TS2536/TS7053 (indexing)
* strict nullability family

### Exit criteria

* CI produces a parity report artifact every run.
* Every fixed mismatch becomes a permanent regression test.

---

# Milestone 10 — Documentation & contributor experience (make the project easy to extend)

### Objective

Reduce “tribal knowledge” and make refactors safe.

### Deliverables

1. **Boundary docs**

* One page that defines:

  * what lives in Solver vs Checker
  * what TypeEnvironment owns
  * what “relation policy flags” are and where they are set

2. **Auto-generated architecture tables**

* Keep the North Star doc accurate by generating metrics (largest files, forbidden deps).

3. **Contribution checklist**
   Every PR must answer:

* Is this “WHAT” or “WHERE”?
* Which solver query does it use?
* Which parity tests changed?

### Exit criteria

* New contributors can add a type feature by touching solver + tests, without spelunking.

### Status update (2026-02-14)

* **Status:** In progress
* **Completed in this iteration (Milestone 10 sub-item):**
  * Added dedicated boundary contract doc:
    * `docs/architecture/BOUNDARIES.md`
  * Added dedicated contribution checklist doc:
    * `docs/architecture/CONTRIBUTION_CHECKLIST.md`
  * Wired architecture report artifact generation into `scripts/check-checker-boundaries.sh`:
    * emits `artifacts/architecture/arch_guard_report.json` on each run.

---

## “First 5 PRs” (a concrete jumpstart sequence)

If you want the fastest path to momentum, do these in order:

1. **PR 1: Add CI architecture guard + report**
   (Milestone 0, deliverable #1/#2)

2. **PR 2: Add solver TypeFactory module (thin wrapper over intern for now)**
   No behavior change yet. Just new API.

3. **PR 3: Migrate 1–2 checker modules off `TypeKey` (start with array/object helpers)**
   You already have a pattern of query-boundary modules; extend that approach.
   (This directly targets the current `TypeKey` leakage.)

4. **PR 4: Make `TypeKey` crate-private and fix all compilation fallout**
   This forces boundary discipline permanently.

5. **PR 5: Replace checker-side recursive type traversal with solver visitor `collect_lazy_def_ids`**
   Start by eliminating `ensure_refs_resolved` recursion into type shapes. 

### Execution status for first five PRs

- [x] PR 1: Add CI architecture guard + report
- [x] PR 2: Add solver TypeFactory module (thin wrapper over intern for now)
- [x] PR 3: Migrate 1–2 checker modules off `TypeKey`
- [ ] PR 4: Make `TypeKey` crate-private and fix fallout
- [ ] PR 5: Replace recursive checker traversal with solver visitor collection

After those five, the rest of the roadmap becomes dramatically easier because the codebase is no longer allowed to regress.

## Practical cadence for this roadmap

Use one lightweight rhythm for each step.
1. Define the checker-to-solver boundary change for that step.
2. Apply only syntax-level edits for the target module(s).
3. Run the narrow architecture guard checks relevant to that step.
4. Merge the smallest safe commit and sync with `main` before the next step.

---

## What this roadmap explicitly fixes in your current snapshot

* **Mega-file reality:** you’ve already split a lot, but `state.rs` and other modules remain extremely large and the “2k lines” rule was deemed unrealistic. The roadmap doesn’t hinge on line limits; it hinges on *boundary sealing + single source of truth*, which naturally reduces file bloat over time. 

* **TypeKey leakage:** checker currently constructs types directly via TypeKey interning; roadmap seals that. 

* **Checker owns type traversal:** `ensure_refs_resolved` does deep traversal and knows too much; roadmap moves that traversal into solver visitors. 

* **Duplicate type system risk:** checker’s `TypeArena` still implements semantic type construction; roadmap removes/quarantines it so there’s only one meaning of `TypeId`. 
