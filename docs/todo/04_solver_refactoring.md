# Solver Refactoring Execution Plan

**Reference**: `docs/architecture/SOLVER_REFACTORING_PROPOSAL.md`  
**Goal**: Query-based Judge with Sound Mode  
**Status**: Planning

---

## Phase 1: Bug Fixes (Immediate)

**Estimated Impact**: ~25% conformance improvement

### 1.1 Fix Evaluate-Before-Cycle-Check Bug
- [ ] Open `src/solver/subtype.rs`
- [ ] Find `check_subtype()` function
- [ ] Move `in_progress.insert()` call BEFORE `evaluate_type()` calls
- [ ] Run conformance tests, measure improvement
- [ ] Commit

### 1.2 Return Provisional on Limit Hit
- [ ] Find all places in `subtype.rs` that return `False` when limits hit
- [ ] Change to return `SubtypeResult::Provisional`
- [ ] Set `depth_exceeded` flag for diagnostics
- [ ] Run conformance tests
- [ ] Commit

### 1.3 Add DefId-Level Cycle Detection
- [ ] Add `seen_defs: HashSet<(DefId, DefId)>` to SubtypeChecker
- [ ] When comparing `Lazy(DefId)` types, check `seen_defs` first
- [ ] Return `Provisional` on DefId-level cycle
- [ ] Run conformance tests
- [ ] Commit

---

## Phase 2: Salsa Prerequisites (Blockers)

### 2.1 Refactor CheckerContext to Use Trait
- [ ] Create `QueryDatabase` trait in `src/solver/db.rs` (if not exists)
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

## Phase 3: Salsa Integration

### 3.1 Create Judge Trait
- [ ] Create `src/solver/judge.rs`
- [ ] Define `Judge` trait with Salsa query group attribute
- [ ] Add `is_subtype()` query with cycle recovery
- [ ] Add `evaluate()` query with cycle recovery
- [ ] Add configuration inputs (`strict_null_checks`, etc.)
- [ ] Commit

### 3.2 Implement Core Queries
- [ ] Extract `check_subtype_inner()` logic into standalone function
- [ ] Wire it as `is_subtype` query implementation
- [ ] Extract `evaluate_type_inner()` logic into standalone function
- [ ] Wire it as `evaluate` query implementation
- [ ] Add fast-path short-circuits before Salsa invocation
- [ ] Commit

### 3.3 Add Classifier Queries
- [ ] Implement `classify_iterable() -> IterableKind`
- [ ] Implement `classify_callable() -> CallableKind`
- [ ] Implement `classify_primitive() -> PrimitiveFlags`
- [ ] Implement `classify_truthiness() -> TruthinessResult`
- [ ] Commit

### 3.4 Migrate Checker to Use Judge
- [ ] Update `CheckerContext` to hold Judge reference
- [ ] Replace `SubtypeChecker::check_subtype()` calls with Judge queries
- [ ] Replace `TypeEvaluator::evaluate()` calls with Judge queries
- [ ] Remove manual cycle tracking (`in_progress` sets)
- [ ] Run full test suite
- [ ] Commit

---

## Phase 4: DefId Migration

### 4.1 Create DefId Infrastructure
- [ ] Create `src/solver/def.rs`
- [ ] Define `DefId(u32)` struct
- [ ] Define `DefKind` enum (TypeAlias, Interface, Class, Enum)
- [ ] Create `DefinitionStore` with DashMap storage
- [ ] Commit

### 4.2 Add Lazy(DefId) to TypeKey
- [ ] Add `TypeKey::Lazy(DefId)` variant
- [ ] Keep `TypeKey::Ref(SymbolRef)` temporarily for compatibility
- [ ] Update `TypeInterner` to handle both
- [ ] Commit

### 4.3 Migrate Lowering
- [ ] Update type lowering to produce `Lazy(DefId)` for interfaces
- [ ] Update type lowering to produce `Lazy(DefId)` for classes
- [ ] Update type lowering to produce `Lazy(DefId)` for type aliases
- [ ] Run conformance tests after each change
- [ ] Commit

### 4.4 Remove Legacy SymbolRef Variants
- [ ] Remove `TypeKey::Ref(SymbolRef)` after all uses migrated
- [ ] Lower `TypeQuery(SymbolRef)` to concrete type during checking
- [ ] Encode `UniqueSymbol` as nominal brand property
- [ ] Lower `ModuleNamespace` to Object type
- [ ] Remove `TypeResolver` trait
- [ ] Commit

---

## Phase 5: Checker Cleanup

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

## Phase 6: Sound Mode

### 6.1 Add Sound Mode Flag
- [ ] Add `sound_mode: bool` to `CompilerOptions`
- [ ] Add `--sound` CLI flag
- [ ] Add `// @ts-sound` pragma support
- [ ] Commit

### 6.2 Create Sound Lawyer
- [ ] Create `SoundLawyer` that bypasses TypeScript quirks
- [ ] Implement strict function parameter contravariance
- [ ] Implement strict array invariance
- [ ] Implement `any` as only top type (not bottom)
- [ ] Commit

### 6.3 Implement Sticky Freshness
- [ ] Add `StickyFreshnessTracker` in Checker
- [ ] Track freshness by `SymbolId` for variable bindings
- [ ] Propagate freshness through inferred assignments
- [ ] Consume freshness at explicit type annotations
- [ ] Wire into Sound Lawyer's excess property checking
- [ ] Commit

### 6.4 Add Sound Mode Diagnostics
- [ ] Define TS9xxx error codes for sound mode errors
- [ ] TS9001: Excess property via sticky freshness
- [ ] TS9002: Mutable array covariance
- [ ] TS9003: Method bivariance
- [ ] TS9004: Any escape
- [ ] TS9005: Enum-number assignment
- [ ] Commit

---

## Verification Checkpoints

### After Phase 1
- [ ] Conformance pass rate improved by ~25%
- [ ] No regressions in existing passing tests

### After Phase 3
- [ ] All Solver operations go through Salsa
- [ ] Manual cycle tracking removed
- [ ] Performance acceptable (profile if needed)

### After Phase 4
- [ ] No SymbolRef in TypeKey
- [ ] TypeResolver trait deleted
- [ ] All tests passing

### After Phase 5
- [ ] Zero TypeKey matches in Checker
- [ ] Checker only uses Judge queries

### After Phase 6
- [ ] Sound mode catches known unsoundness
- [ ] Normal mode 100% tsc compatible
- [ ] Sound mode documented with examples

---

## Notes

- Run conformance tests after every commit
- Profile performance if any phase seems slow
- Keep commits small and focused
- Use `ask-gemini.mjs` for complex decisions
- Update this document as work progresses
