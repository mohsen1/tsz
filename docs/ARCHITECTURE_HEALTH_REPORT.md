# Architecture Health Report

**Date:** 2026-03-15
**Scope:** Full codebase analysis of tsz (14 crates, ~570K LOC, 1046 .rs files)

## Executive Summary

The tsz codebase is **architecturally sound** with clean layer separation and no circular
dependencies. The project adheres well to its NORTH_STAR.md principles. However, there are
concrete improvements that can make the repo healthier, easier to maintain, and easier to test.

**Overall Health Score: B+ (Good, with actionable improvements)**

---

## 1. Architecture Boundary Compliance

### Status: GOOD (1 active violation)

The codebase correctly enforces:
- **Binder isolation**: Zero solver imports in binder. CLEAN.
- **Emitter isolation**: Zero checker imports in emitter. CLEAN.
- **TypeKey encapsulation**: Zero TypeKey usage in checker (outside tests). CLEAN.
- **Solver API tiers**: Well-organized 4-tier export structure (type_handles → query → computation → construction).

### Active Violation

**`crates/tsz-checker/src/error_reporter/core.rs:1703`** — Direct access to
`tsz_solver::types::ObjectFlags::FRESH_LITERAL`, a solver-internal type constant.

```rust
.contains(tsz_solver::types::ObjectFlags::FRESH_LITERAL)
```

**Fix**: Add `is_fresh_literal_object(type_id)` query to `query_boundaries` and call it instead.

**`crates/tsz-checker/src/error_reporter/core.rs:83`** — Direct pattern match on
`tsz_solver::def::DefKind::ClassConstructor`. Lower severity but still a boundary leak.

---

## 2. Code Size & Maintainability

### Large Production Files (>2000 LOC, violating §12 guideline)

| Lines | File | Recommendation |
|-------|------|----------------|
| 17,270 | `tsz-common/src/diagnostics/data.rs` | Generated data — acceptable |
| 7,615 | `tsz-emitter/src/declaration_emitter/helpers.rs` | Split by declaration kind |
| 5,149 | `tsz-core/src/config.rs` | Extract option enums to submodule |
| 4,161 | `tsz-emitter/src/declaration_emitter/core.rs` | Split emit phases |
| 4,088 | `tsz-core/src/module_resolver.rs` | Extract resolution strategies |
| 3,372 | `tsz-solver/src/diagnostics/format.rs` | Extract message builders |
| 3,360 | `tsz-solver/src/operations/generic_call.rs` | Extract inference phases |
| 3,218 | `tsz-cli/src/bin/tsz_server/handlers_code_fixes.rs` | Split by fix category |
| 3,086 | `tsz-emitter/src/emitter/declarations/class.rs` | Extract member emitters |
| 3,036 | `tsz-scanner/src/scanner_impl.rs` | Extract token-specific scanners |
| 2,997 | `tsz-cli/src/bin/tsz.rs` | Extract subcommand handlers |

**39 files** exceed the 2000-line guideline. The checker has the highest density with several
files at 2,300+ lines.

### Mega Test Files (testing maintenance burden)

| Lines | File | Recommendation |
|-------|------|----------------|
| 42,455 | `tsz-solver/tests/evaluate_tests.rs` | Split into ~8 category files |
| 35,015 | `tsz-core/tests/checker_state_tests.rs` | Split into ~6 domain files |
| 25,987 | `tsz-solver/tests/subtype_tests.rs` | Split into ~5 type-category files |
| 19,848 | `tsz-core/tests/source_map_tests_4.rs` | Already split (4 parts) |
| 15,470 | `tsz-solver/tests/infer_tests.rs` | Split into ~3 inference category files |
| 15,259 | `tsz-checker/tests/conformance_issues.rs` | Split by diagnostic code family |

These files are difficult to navigate, slow to compile incrementally, and hard to review
in PRs. Splitting by logical category would improve all three.

---

## 3. Technical Debt Inventory

### TODO Distribution (104 total)

| Crate | Count | Category |
|-------|-------|----------|
| tsz-solver | 30 | Mostly in tests — known limitations (inference bounds, tuple spreads) |
| tsz-checker | 30 | Mixed: architecture TODOs + test-known-issues |
| tsz-core | 28 | Mostly in tests — known conformance gaps |
| tsz-lsp | 7 | Feature gaps (cross-file search, scope cache) |
| tsz-cli | 5 | Driver/resolution improvements |
| tsz-emitter | 3 | Declaration emit gaps |
| tsz-wasm | 1 | Source maps not implemented |

**Positive**: Zero FIXME, HACK, or WORKAROUND comments. The codebase avoids quick patches.

### High-Priority Architecture TODOs

1. **`checker/tests/architecture_contract_tests.rs:1131`** — Refactor generic_checker to use
   solver query helpers (known bypass of query_boundaries)
2. **`checker/tests/architecture_contract_tests.rs:1582`** — Computation APIs need
   query_boundaries wrappers
3. **`checker/src/state/state_checking/property.rs:1252`** — Move PropertyAccessEvaluator
   resolution into solver
4. **`core/src/config.rs:1164`** — Remove non-strict-mode conformance workaround

---

## 4. Dead Code & Unused Imports

### Status: MINIMAL (good hygiene)

- **3** `#[allow(dead_code)]` annotations:
  - `tsz-core/src/module_resolver_helpers.rs:498` — JSON deserialization fields
  - `tsz-emitter/src/emitter/es5/mod.rs:11` — `loop_capture` module (used by statements.rs,
    `dead_code` suppresses warnings on internal items not yet wired up)
  - `tsz-emitter/src/emitter/core.rs:32` — Reserved `is_static` field

- **3** `#[allow(unused_imports)]` remaining in production code (down from 14):
  - 1 in `tsz-solver/src/intern/mod.rs` — intentional for test access
  - 1 in `tsz-checker/src/query_boundaries/type_construction.rs`
  - 1 in `tsz-solver/src/diagnostics/core.rs`

---

## 5. Dependency Health

### Status: EXCELLENT

- **No circular dependencies** — clean DAG from common → scanner → parser → binder → solver → checker → emitter
- **No forbidden cross-layer imports** (except the 1 violation noted above)
- **External dependencies are minimal and well-chosen**: rustc-hash, smallvec, dashmap, indexmap, bitflags, ena

---

## 6. Testability Assessment

### Strengths
- **13,529 test functions** across 317 test files — strong coverage
- Conformance harness compares against tsc (85.9% pass rate)
- Fourslash tests for LSP (99.2% pass rate)
- Architecture contract tests that catch boundary violations

### Weaknesses
- **Mega test files** are hard to run selectively and slow to compile
- **No test categorization** — can't easily run "just narrowing tests" or "just inference tests"
- **Ignored tests** scattered with TODO comments rather than tracked in a central registry

### Recommendations
1. Split mega test files into category-based modules
2. Add `#[cfg(test)]` feature flags for test categories (e.g., `narrowing`, `inference`, `compat`)
3. Create a tracking issue or file for all `#[ignore]` tests with their blockers

---

## 7. Actionable Improvement Plan

### Tier 1: Fix Now (architecture correctness)
- [x] Fix `error_reporter/core.rs` boundary violation — resolved upstream (code path simplified)
- [x] Clean up `allow(unused_imports)` in `lowering/mod.rs` — replaced with `pub(super)` re-exports
- [x] Investigate `loop_capture` module — confirmed actively used by `statements.rs`
- [x] Split `member_access.rs` (2038 LOC) to stay under 2000-line limit
- [x] Fix `assignability_checker.rs` (2001 LOC) to stay under 2000-line limit
- [x] Replace all production `unwrap()` with `expect()` messages across all crates
  - tsz-solver: 29 calls, tsz-binder: 13 calls, tsz-checker: ~35 calls,
    tsz-emitter: ~47 calls, tsz-core: 16 calls, tsz-lsp: 4 calls,
    tsz-cli: 3 calls, tsz-wasm: 1 call
- [x] Tighten `pub` to `pub(crate)` for solver-internal items (evaluation constants,
  inference types, unused re-exports)
- [x] Remove dead code: unused `ConditionalResult`, `MAX_VISITING_SET_SIZE`,
  dead `InferenceContext` re-export from lib.rs

### Tier 2: Near-term (maintainability)
- [ ] Split `evaluate_tests.rs` (42K lines) into category files
- [ ] Split `checker_state_tests.rs` (35K lines) into domain files
- [ ] Split `subtype_tests.rs` (26K lines) into type-category files
- [ ] Add query_boundaries wrappers for generic_checker computation APIs
- [ ] Move PropertyAccessEvaluator resolution into solver

### Tier 3: Ongoing (code quality)
- [ ] Track and triage all 104 TODOs in a dedicated tracking file
- [ ] Split production files over 3000 lines when touching them
- [ ] Enforce 2000-line file limit in CI (warning, not blocking)
- [ ] Add `#[ignore]` test registry with blocker tracking

---

## 8. What's Working Well

- **Clean architecture boundaries** — the solver-first principle is well-enforced
- **Zero quick hacks** — no FIXME/HACK/WORKAROUND comments
- **Strong test infrastructure** — 13,529 tests with conformance harness
- **Well-documented architecture** — NORTH_STAR.md, BOUNDARIES.md, HOW_TO_CODE.md
- **Tiered solver API** — clean 4-tier export structure prevents accidental coupling
- **Architecture contract tests** — automated enforcement of boundary rules
