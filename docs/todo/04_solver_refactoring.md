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
| Phase 4 | PARTIAL | DefId infrastructure complete, migration partial |
| Phase 5 | ✅ COMPLETE | TypeKey audit and partial migration |
| Phase 6 | ✅ COMPLETE | Sound mode implementation |

---

## Phase 1: Bug Fixes ✅ COMPLETE

### 1.1 Fix Evaluate-Before-Cycle-Check Bug ✅
### 1.2 Return Provisional on Limit Hit ✅
### 1.3 Add DefId-Level Cycle Detection ✅

All bug fixes implemented and tested.

---

## Phase 2: Salsa Prerequisites - DEFERRED

This phase prepares for future Salsa integration. Not required for current functionality.

- [ ] Refactor CheckerContext to use trait-based database
- [ ] Remove FreshnessTracker from Solver
- [ ] Remove RefCell caches from Evaluator

**Rationale**: Salsa integration is future work. Current implementation works correctly without these changes.

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

### 4.4 Remove Legacy SymbolRef Variants - BLOCKED

**Blockers** (require deeper binder integration):
- Namespace/Module types: need export map population
- Enum types: need enum member population
- Cycle detection fallback: classes mid-resolution

**Status**: Infrastructure exists via `get_lazy_export()` and `get_lazy_enum_member()` 
in TypeResolver, but actual population requires significant binder changes.

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

---

## Future Work

1. **Phase 2**: Complete Salsa prerequisites when Salsa integration begins
2. **Phase 4.4**: Complete namespace/enum migration when binder integration is addressed
3. **Sound mode docs**: Add usage examples and documentation
