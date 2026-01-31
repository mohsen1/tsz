# Solver Refactoring Execution Plan

**Reference**: `docs/architecture/SOLVER_REFACTORING_PROPOSAL.md`  
**Goal**: Query-based Judge with Sound Mode  
**Status**: Phase 1, 3, 4.1, 6 Infrastructure Complete

---

## Phase 1: Bug Fixes (Immediate) ✅ COMPLETE

**Estimated Impact**: ~25% conformance improvement

### 1.1 Fix Evaluate-Before-Cycle-Check Bug ✅
- [x] Open `src/solver/subtype.rs`
- [x] Find `check_subtype()` function
- [x] Move `in_progress.insert()` call BEFORE `evaluate_type()` calls
- [x] Run conformance tests, measure improvement
- [x] Commit

### 1.2 Return Provisional on Limit Hit ✅
- [x] Find all places in `subtype.rs` that return `False` when limits hit
- [x] Change to return `SubtypeResult::Provisional`
- [x] Set `depth_exceeded` flag for diagnostics
- [x] Run conformance tests
- [x] Commit

### 1.3 Add DefId-Level Cycle Detection ✅
- [x] Add `seen_refs: HashSet<(SymbolRef, SymbolRef)>` to SubtypeChecker
- [x] When comparing `Ref(SymbolRef)` types, check `seen_refs` first
- [x] Return `Provisional` on ref-level cycle
- [x] Run conformance tests
- [x] Commit

---

## Phase 2: Salsa Prerequisites (Blockers) - NOT STARTED

### 2.1 Refactor CheckerContext to Use Trait
- [x] Create `QueryDatabase` trait in `src/solver/db.rs` (exists)
- [ ] Add all methods Checker needs to `QueryDatabase`
- [ ] Change `CheckerContext.types: &TypeInterner` to `CheckerContext.db: &dyn QueryDatabase`
- [ ] Update all call sites in `src/checker/`
- [ ] Verify compilation
- [ ] Run all tests
- [ ] Commit

### 2.2 Remove FreshnessTracker from Solver
- [ ] Open `src/solver/lawyer.rs`
- [ ] Remove `FreshnessTracker` struct
- [ ] Add `is_fresh: bool` parameter to relevant Lawyer methods
- [ ] Create `is_fresh_expression()` function in Checker
- [ ] Update Checker to pass freshness flag to Lawyer
- [ ] Verify tsc conformance unchanged
- [ ] Commit

### 2.3 Remove RefCell Caches from Evaluator
- [ ] Open `src/solver/evaluate.rs`
- [ ] Remove `cache: RefCell<FxHashMap<...>>`
- [ ] Remove `visiting: RefCell<FxHashSet<...>>`
- [ ] For now, make evaluator stateless (Salsa will cache later)
- [ ] Run conformance tests (may be slower temporarily)
- [ ] Commit

---

## Phase 3: Salsa Integration - INFRASTRUCTURE COMPLETE ✅

### 3.1 Create Judge Trait ✅
- [x] Create `src/solver/judge.rs` (1,296 lines)
- [x] Define `Judge` trait with query methods
- [x] Add `is_subtype()` query with cycle recovery
- [x] Add `evaluate()` query with cycle recovery
- [x] Add configuration inputs (`JudgeConfig` with strict_null_checks, etc.)
- [x] Commit

### 3.2 Implement Core Queries ✅
- [x] Implement `is_subtype` in `DefaultJudge`
- [x] Implement `evaluate` in `DefaultJudge`
- [x] Add fast-path short-circuits
- [x] Commit

### 3.3 Add Classifier Queries ✅
- [x] Implement `classify_iterable() -> IterableKind`
- [x] Implement `classify_callable() -> CallableKind`
- [x] Implement `classify_primitive() -> PrimitiveFlags`
- [x] Implement `classify_truthiness() -> TruthinessKind`
- [x] Commit

### 3.4 Migrate Checker to Use Judge - PARTIALLY COMPLETE
- [x] Create `src/checker/judge_integration.rs` with helper methods
- [x] Add `with_judge()`, `judge_is_subtype()`, `judge_evaluate()`, etc.
- [ ] Replace `SubtypeChecker::check_subtype()` calls with Judge queries
- [ ] Replace `TypeEvaluator::evaluate()` calls with Judge queries
- [ ] Remove manual cycle tracking (`in_progress` sets)
- [ ] Run full test suite
- [ ] Commit

---

## Phase 4: DefId Migration - 4.1 & 4.2 COMPLETE ✅

### 4.1 Create DefId Infrastructure ✅
- [x] Create `src/solver/def.rs` (673 lines)
- [x] Define `DefId(u32)` struct
- [x] Define `DefKind` enum (TypeAlias, Interface, Class, Enum)
- [x] Create `DefinitionStore` with DashMap storage
- [x] Create `ContentAddressedDefIds` for LSP mode
- [x] Commit

### 4.2 Add Lazy(DefId) to TypeKey ✅
- [x] Add `TypeKey::Lazy(DefId)` variant
- [x] Keep `TypeKey::Ref(SymbolRef)` temporarily for compatibility
- [x] Update all TypeKey pattern matches across solver
- [x] Commit

### 4.3 Migrate Lowering - NOT STARTED
- [ ] Update type lowering to produce `Lazy(DefId)` for interfaces
- [ ] Update type lowering to produce `Lazy(DefId)` for classes
- [ ] Update type lowering to produce `Lazy(DefId)` for type aliases
- [ ] Run conformance tests after each change
- [ ] Commit

### 4.4 Remove Legacy SymbolRef Variants - NOT STARTED
- [ ] Remove `TypeKey::Ref(SymbolRef)` after all uses migrated
- [ ] Lower `TypeQuery(SymbolRef)` to concrete type during checking
- [ ] Encode `UniqueSymbol` as nominal brand property
- [ ] Lower `ModuleNamespace` to Object type
- [ ] Remove `TypeResolver` trait
- [ ] Commit

---

## Phase 5: Checker Cleanup - NOT STARTED

### 5.1 Audit TypeKey Matches
- [ ] Run `grep -r "TypeKey::" src/checker/` to find all matches
- [ ] Create list of all TypeKey matches with file:line
- [ ] Prioritize by frequency

### 5.2 Create Judge Queries for Each Match
- [ ] For each unique operation, determine if existing query covers it
- [ ] If not, add new Judge query
- [ ] Replace TypeKey match with query call
- [ ] Verify behavior unchanged
- [ ] Commit incrementally (one file at a time)

### 5.3 Final Audit
- [ ] Re-run `grep -r "TypeKey::" src/checker/`
- [ ] Target: zero matches
- [ ] Document any exceptions with justification
- [ ] Commit

---

## Phase 6: Sound Mode - COMPLETE ✅

### 6.1 Add Sound Mode Flag ✅
- [x] Add `sound_mode: bool` to `CheckerOptions`
- [x] Add `--sound` CLI flag
- [ ] Add `// @ts-sound` pragma support (future enhancement)
- [x] Commit

### 6.2 Create Sound Lawyer ✅
- [x] Create `src/solver/sound.rs` (702 lines)
- [x] Create `SoundLawyer` that bypasses TypeScript quirks
- [x] Implement strict function parameter contravariance
- [x] Implement strict array covariance detection
- [x] Implement `any` as only top type (not bottom)
- [x] Commit

### 6.3 Implement Sticky Freshness ✅
- [x] Add `StickyFreshnessTracker` in `src/solver/sound.rs`
- [x] Track freshness by `SymbolId` for variable bindings
- [x] Propagate freshness through `transfer_freshness()`
- [x] Consume freshness at explicit type annotations
- [x] Wire into Sound Lawyer's excess property checking
- [x] Commit

### 6.4 Add Sound Mode Diagnostics ✅
- [x] Define TS9xxx error codes in `SoundDiagnosticCode` enum
- [x] TS9001: Excess property via sticky freshness
- [x] TS9002: Mutable array covariance
- [x] TS9003: Method bivariance
- [x] TS9004: Any escape
- [x] TS9005: Enum-number assignment
- [x] TS9006: Missing index signature
- [x] TS9007: Unsafe type assertion
- [x] TS9008: Unchecked indexed access
- [x] Commit

---

## Verification Checkpoints

### After Phase 1 ✅
- [x] Bug fixes implemented
- [x] No regressions in existing passing tests (46.4% pass rate maintained)

### After Phase 3 - PARTIAL
- [x] Judge trait and queries implemented
- [ ] All Solver operations go through Judge (migration incomplete)
- [ ] Manual cycle tracking removed

### After Phase 4 - NOT STARTED
- [ ] No SymbolRef in TypeKey
- [ ] TypeResolver trait deleted
- [ ] All tests passing

### After Phase 5 - NOT STARTED
- [ ] Zero TypeKey matches in Checker
- [ ] Checker only uses Judge queries

### After Phase 6 ✅
- [x] Sound mode infrastructure created
- [x] Sound mode wired into CLI (`--sound` flag)
- [ ] Sound mode documented with examples (future enhancement)

---

## Files Created/Modified

### New Files
- `src/solver/judge.rs` - Judge trait and DefaultJudge (1,296 lines)
- `src/solver/def.rs` - DefId infrastructure (673 lines)
- `src/solver/sound.rs` - Sound mode implementation (702 lines)
- `src/solver/tests/solver_refactoring_tests.rs` - Comprehensive tests (903 lines)
- `src/checker/judge_integration.rs` - Checker integration helpers (141 lines)

### Modified Files
- `src/solver/subtype.rs` - Cycle detection bug fixes
- `src/solver/subtype_rules/generics.rs` - Ref-level cycle detection
- `src/solver/mod.rs` - Module exports
- `src/checker/mod.rs` - Judge integration module

---

## Notes

- Run conformance tests after every commit
- Profile performance if any phase seems slow
- Keep commits small and focused
- Use `ask-gemini.mjs` for complex decisions
- Infrastructure is in place; migration work remains
