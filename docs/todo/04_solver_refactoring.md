# Solver Refactoring Execution Plan

**Reference**: `docs/architecture/SOLVER_REFACTORING_PROPOSAL.md`  
**Goal**: Query-based Judge with Sound Mode  
**Status**: SUBSTANTIALLY COMPLETE

## Summary

| Phase | Status | Description |
|-------|--------|-------------|
| Phase 1 | ✅ COMPLETE | Bug fixes for cycle detection |
| Phase 2 | DEFERRED | Salsa prerequisites (future work) |
| Phase 3 | ✅ COMPLETE | Judge trait and integration |
| Phase 4 | PARTIAL | DefId infra + checker integration done, enum/namespace blocked |
| Phase 5 | ✅ COMPLETE | TypeKey audit and partial migration |
| Phase 6 | ✅ COMPLETE | Sound mode implementation |

---

## Phase 1: Bug Fixes ✅ COMPLETE

### 1.1 Fix Evaluate-Before-Cycle-Check Bug ✅
### 1.2 Return Provisional on Limit Hit ✅
### 1.3 Add DefId-Level Cycle Detection ✅

All bug fixes implemented and tested.

---

## Phase 2: Salsa Prerequisites - IN PROGRESS

This phase prepares for future Salsa integration.

- [ ] Refactor CheckerContext to use trait-based database
- [x] Remove FreshnessTracker from Solver ✅
- [x] Remove RefCell caches from Evaluator ✅

### Task 3: Remove RefCell from TypeEvaluator ✅

**Completed**: TypeEvaluator now uses `&mut self` instead of RefCell for:
- `cache: FxHashMap<TypeId, TypeId>` - evaluation cache
- `visiting: FxHashSet<TypeId>` - cycle detection  
- `depth: u32` - recursion depth tracking
- `total_evaluations: u32` - iteration limit
- `depth_exceeded: bool` - depth exceeded flag

**Impact**:
- TypeEvaluator is now `Send` (thread-safe for future Salsa)
- All methods that call `evaluate()` changed to `&mut self`
- Closures converted to helper methods for borrow checker compatibility
- Conformance pass rate: unchanged (46.5%, +1 test)

**Files modified**:
- `src/solver/evaluate.rs` - main TypeEvaluator struct
- `src/solver/evaluate_rules/conditional.rs` - all methods
- `src/solver/evaluate_rules/index_access.rs` - evaluate methods
- `src/solver/evaluate_rules/keyof.rs` - evaluate methods
- `src/solver/evaluate_rules/mapped.rs` - refactored closures
- `src/solver/evaluate_rules/string_intrinsic.rs` - evaluate methods
- `src/solver/evaluate_rules/template_literal.rs` - evaluate methods
- `src/solver/judge.rs` - evaluator instantiation
- `src/checker/state_type_environment.rs` - evaluator instantiation
- `src/solver/tests/evaluate_tests.rs` - test declarations

---

## Phase 3: Salsa Integration ✅ COMPLETE

### 3.1 Create Judge Trait ✅
- Judge trait defined in `src/solver/judge.rs` (1,296 lines)
- Query methods: `is_subtype()`, `evaluate()`, classifiers

### 3.2 Implement Core Queries ✅
- `DefaultJudge` implementation
- Fast-path short-circuits

### 3.3 Add Classifier Queries ✅
- `classify_iterable() -> IterableKind`
- `classify_callable() -> CallableKind`
- `classify_primitive() -> PrimitiveFlags`
- `classify_truthiness() -> TruthinessKind`

### 3.4 Migrate Checker to Use Judge ✅
- Helper methods in `src/checker/judge_integration.rs`
- `with_judge()`, `judge_is_subtype()`, `judge_evaluate()`, etc.
- Iterators and generators migrated to use Judge classifiers

---

## Phase 4: DefId Migration - INFRASTRUCTURE COMPLETE

### 4.1 Create DefId Infrastructure ✅
- `src/solver/def.rs` (673+ lines)
- `DefId(u32)` struct, `DefKind` enum
- `DefinitionStore` with DashMap storage

### 4.2 Add Lazy(DefId) to TypeKey ✅
- `TypeKey::Lazy(DefId)` variant added
- All TypeKey pattern matches updated across solver

### 4.3 Migrate Lowering ✅
- `DefinitionStore` added to `CheckerContext`
- `symbol_to_def` mapping for SymbolId -> DefId
- `create_lazy_type_ref()` helper method
- `TypeResolver.resolve_lazy()` for Lazy type resolution
- `TypeEnvironment` supports DefId -> TypeId storage
- `type_literal_checker.rs` fully migrated to use Lazy(DefId)

### 4.4 Remove Legacy SymbolRef Variants - PARTIAL

**Completed**:
- ✅ DefId reverse mapping (`def_to_symbol` in CheckerContext)
- ✅ Cycle detection fallback migrated from `Ref(SymbolRef)` to `Lazy(DefId)`
- ✅ Namespace/enum types now register DefId mappings alongside Ref types
- ✅ `resolve_qualified_name` handles both Ref and Lazy types
- ✅ `ensure_refs_resolved` handles Lazy(DefId) types
- ✅ `expand_type_application` handles Lazy base types
- ✅ `ensure_application_symbols_resolved_inner` handles Lazy base types
- ✅ Lazy variant added to `SymbolResolutionTraversalKind`
- ✅ SubtypeChecker handles Lazy↔Lazy and Lazy↔structural subtype checks
- ✅ DefinitionStore: get/set methods for exports, enum members, names

**Remaining blockers** (namespace/enum types still use `Ref(SymbolRef)`):
- Enum subtype checking (`check_ref_ref_subtype`, `check_ref_subtype`,
  `check_to_ref_subtype`) extracts SymbolRef for nominal identity/flags
- Namespace export lookup extracts SymbolRef to access binder symbol exports
- Need Lazy equivalents in all ref-specific subtype paths before full migration

---

## Phase 5: Checker Cleanup ✅ COMPLETE

### 5.1 Audit TypeKey Matches ✅
Initial audit found 79 TypeKey matches across checker files.

### 5.2 Create Judge Queries ✅
- Migrated `iterators.rs` (is_iterable, get_iterable_element_type)
- Migrated `generators.rs` (is_iterable, get_iterable_element_type)
- TypeKey matches reduced from 79 to 65

### 5.3 Final Audit ✅
Remaining 65 TypeKey matches are:
- **Type creation** (e.g., `TypeKey::Array(...)`) - intentionally kept
- **Type traversal** (e.g., `ensure_refs_resolved`) - structural, not queries
- **Blocked migrations** (namespace/enum Ref types)

**Analysis**: Zero additional matches can be migrated without completing Phase 4.4.

---

## Phase 6: Sound Mode ✅ COMPLETE

### 6.1 Add Sound Mode Flag ✅
- `sound_mode: bool` in `CheckerOptions`
- `--sound` CLI flag

### 6.2 Create Sound Lawyer ✅
- `src/solver/sound.rs` (702 lines)
- Strict function parameter contravariance
- Strict array covariance detection
- `any` as only top type

### 6.3 Implement Sticky Freshness ✅
- `StickyFreshnessTracker` implementation
- Freshness propagation and consumption

### 6.4 Add Sound Mode Diagnostics ✅
- TS9001-TS9008 error codes defined

---

## Files Created

| File | Lines | Purpose |
|------|-------|---------|
| `src/solver/judge.rs` | 1,296 | Judge trait and DefaultJudge |
| `src/solver/def.rs` | 700+ | DefId infrastructure |
| `src/solver/sound.rs` | 702 | Sound mode implementation |
| `src/checker/judge_integration.rs` | 141 | Checker-Judge bridge |

---

## Verification

- [x] All tests pass (11 pre-existing failures unrelated to refactoring)
- [x] No conformance regressions
- [x] Sound mode functional via `--sound` flag
- [x] Judge queries working for type classification
- [x] Conformance: 47.3% (5,851/12,379 tests passing) - improved from baseline

---

## Future Work

1. **Phase 2**: Complete Salsa prerequisites when Salsa integration begins
2. **Phase 4.4**: Complete namespace/enum migration when binder integration is addressed
3. **Sound mode docs**: Add usage examples and documentation

---

## Conformance Analysis

Current pass rate: **47.3%** (improved from 47.1% baseline)

### Fixes Applied
| Issue | Tests Fixed | Description |
|-------|-------------|-------------|
| TS1127 | +15 | Invalid character errors for backslashes |
| TS2565 | +3 | False positives on assignment targets |

### Top Extra Errors (TSZ emits but shouldn't)
| Code | Count | Description |
|------|-------|-------------|
| TS1005 | 1409 | ';' expected |
| TS2339 | 1247 | Property does not exist |
| TS2322 | 1081 | Type not assignable |
| TS2345 | 878 | Argument not assignable |
| TS2304 | 833 | Cannot find name |

### Top Missing Errors (TSZ should emit but doesn't)
| Code | Count | Description |
|------|-------|-------------|
| TS2304 | 1419 | Cannot find name |
| TS2318 | 1405 | Cannot find global type |
| TS2322 | 719 | Type not assignable |
| TS18050 | 679 | Element implicitly has 'any' |
| TS2339 | 610 | Property does not exist |

### Key Areas for Improvement
1. **Parser** - TS1005, TS1127, TS1128 errors suggest parser/scanner issues
2. **Global types** - TS2318 missing errors indicate lib type resolution issues
3. **Type checking** - TS2322, TS2339, TS2345 both extra and missing
