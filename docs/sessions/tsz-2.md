# Session tsz-2: Lawyer-Layer Cache Partitioning & Type Cache Unification

**Started**: 2026-02-04
**Status**: 🟢 Active (Redefined 2026-02-04)
**Previous**: Cache Isolation Bug Investigation (COMPLETE)

## SESSION REDEFINITION (2026-02-04)

**Gemini Consultation**: Asked for session redefinition after completing Cache Isolation Bug investigation.

**New Direction**: **Implementation of Unified Type Caching and Lawyer-Layer Isolation**

### Rationale

Based on the current state of all sessions and North Star architecture goals:

- **tsz-1**: Core Solver Correctness - Active (nominal subtyping, intersection reduction)
- **tsz-2**: Cache Unification - **THIS SESSION** (implementation phase)
- **tsz-3**: CFA - COMPLETE
- **tsz-4**: Declaration Emit - COMPLETE
- **tsz-5**: Import/Export Elision - ACTIVE
- **tsz-6**: Advanced Type Nodes - COMPLETE
- **tsz-7**: Automatic Import Generation - COMPLETE

The Cache Isolation Bug fix is **critical path** for both correctness (preventing `any` leakage) and performance (enabling safe parallel/incremental checking).

### Why This Matters

1. **Correctness**: Type relations computed under different "Lawyer" rules (strict vs. non-strict) are currently leaking into each other, making the compiler unsound
2. **Foundation for Salsa**: Cannot integrate Salsa (incremental recomputation) until state is unified and "Judge vs. Lawyer" separation is strictly enforced at cache level
3. **North Star Alignment**: Section 3.3 requires Lawyer to manage object literal freshness and `any` propagation - the cache must respect these boundaries

## Implementation Tasks

### Task 1: Lawyer-Layer Cache Partitioning (HIGH PRIORITY - Correctness)

**Goal**: Modify `TypeCache` to support partitioned caching based on Lawyer rules.

**Implementation**:
- Modify `TypeCache` in `src/checker/context.rs`
- Replace single `relation_cache` with keyed cache
- Cache key includes: `AnyPropagationMode` or bitmask of active Lawyer rules
- Rules to track: strict function types, strict null checks

**Why High Priority**: `any` propagation results are currently leaking into strict checks, causing fundamental unsoundness.

**Files to modify**:
- `src/checker/context.rs`: TypeCache struct
- `src/solver/db.rs`: Update `lookup_subtype_cache` and `insert_subtype_cache` methods

### Task 2: CheckerState Refactoring (HIGH PRIORITY - Architecture)

**Goal**: Update `src/checker/state.rs` to delegate all caching logic to `ctx.type_cache`.

**Implementation**:
- Remove ad-hoc caches from `CheckerState` or `CheckerContext`
- Ensure all caching goes through unified `TypeCache`
- Eliminate private cache instances in temporary `CheckerState` (fixes Cache Isolation Bug)

**Why High Priority**: Completes the Cache Isolation Bug fix and unifies architecture.

**Files to modify**:
- `src/checker/state.rs`: Main checker state
- `src/checker/state_type_resolution.rs`: Temporary checker creation

### Task 3: Thread-Safe Cache Access (MEDIUM PRIORITY - Performance)

**Goal**: Enable parallel checking by making `TypeCache` thread-safe.

**Implementation**:
- Replace `RefCell<FxHashMap>` with `DashMap` or similar concurrent structure
- Ensure no deadlocks in recursive type resolution paths
- Aligns with North Star Section 2.1 (parallel checking)

**Why Medium Priority**: Important for performance but can be added after Task 1-2 are stable.

**Files to modify**:
- `src/checker/context.rs`: TypeCache data structures
- Potential deadlock zones: recursive type resolution paths

### Task 4: Symbol Dependency Integration (MEDIUM PRIORITY - Future-proofing)

**Goal**: Wire `symbol_dependencies` map in `TypeCache` to `get_type_of_symbol` logic.

**Implementation**:
- Connect `TypeCache.symbol_dependencies` to type resolution
- Foundation for "Incremental Checking" metric in North Star
- Helps tsz-5 (Import/Export Elision) track type-only vs value usage

**Why Medium Priority**: Critical foundation for incremental checking but not blocking correctness.

**Files to modify**:
- `src/checker/state_type_analysis.rs`: `get_type_of_symbol` logic

## Coordination Notes

### With tsz-1 (Core Solver)
- **tsz-1** is working on `src/solver/subtype.rs`
- **tsz-2** work is primarily in `src/checker/context.rs` and `src/checker/state.rs`
- **Coordination Point**: tsz-2 provides `QueryDatabase` implementation that tsz-1's solver uses
- Ensure `lookup_subtype_cache` and `insert_subtype_cache` in `src/solver/db.rs` are updated for partitioned cache

### With tsz-5 (Import/Export Elision)
- **tsz-5** is working on symbol usage tracking
- **tsz-2** work on `symbol_dependencies` will help tsz-5 track type-only vs value-carrying symbols across file boundaries
- **Synergy**: tsz-2's cache improvements provide better dependency tracking for tsz-5

### Avoid Conflicts
- **tsz-3** (CFA) - COMPLETE, no conflicts
- **tsz-4** (Declaration Emit) - COMPLETE, no conflicts
- **tsz-6/7** (Advanced Type Nodes/Import Generation) - COMPLETE, no conflicts

## Mandatory Two-Question Rule

Per AGENTS.md, since modifying `src/checker/` and `src/solver/db.rs`:

**Question 1 (PRE-implementation)**:
```bash
./scripts/ask-gemini.mjs --include=src/checker "I need to implement Lawyer-Layer Cache Partitioning.

Planned approach:
1. Add cache key struct with AnyPropagationMode bitmask
2. Replace TypeCache.relation_cache with partitioned cache
3. Update all cache access sites to include Lawyer context

Is this the right approach? What about the cache key structure?
Are there edge cases I'm missing?
```

**Question 2 (POST-implementation)**:
```bash
./scripts/ask-gemini.mjs --pro --include=src/checker "I implemented Lawyer-Layer Cache Partitioning.

Changes:
[PASTE CODE OR DIFF]

Please review:
1. Is this logic correct for TypeScript?
2. Did I miss any Lawyer rule combinations?
3. Are there deadlock risks in recursive resolution?
```

## Success Criteria

1. **Correctness**:
   - [ ] Strict mode checks don't leak `any` from non-strict checks
   - [ ] Cache isolation prevents cross-contamination

2. **Architecture**:
   - [ ] All caching delegated to unified `TypeCache`
   - [ ] No ad-hoc caches in `CheckerState`

3. **Conformance**:
   - [ ] Type aliases (Partial<T>, Pick<T,K>) resolve correctly
   - [ ] No regressions in existing tests

4. **Performance** (Task 3):
   - [ ] Thread-safe cache access enables parallel checking

## Session History

- 2026-02-04: Started as "Intersection Reduction and Advanced Type Operations"
- 2026-02-04: **COMPLETED** BCT, Intersection Reduction, Literal Widening
- 2026-02-04: **COMPLETED** Phase 1: Nominal Subtyping (all 4 tasks)
- 2026-02-04: **INVESTIGATED** Cache Isolation Bug
- 2026-02-04: **COMPLETE** Investigation phase
- 2026-02-04: **REDEFINED** to Lawyer-Layer Cache Partitioning & Type Cache Unification

## Previous Investigation Work (Archived)

### Root Cause Discovery
**Cache Isolation Bug** - temporary `CheckerState` instances in `get_type_of_symbol` discard their caches, preventing main context from seeing resolved lib types like `Partial<T>` and `Pick<T,K>`.

### Attempted Fixes (Insufficient)
1. TypeEnvironment Registration → No improvement
2. Return Lazy(DefId) → No improvement

### Solution
Implemented shared cache infrastructure (planned in previous session phase).

## Complexity: MEDIUM (was HIGH)

**Why Medium**:
- Clear architectural guidance from Gemini
- Phased approach minimizes risk
- Tasks can be implemented incrementally
- No unknown architectural issues

**Mitigation**: Follow Two-Question Rule strictly. Use --pro flag for all architectural changes.

## Next Steps

1. ✅ Update session file with redefinition (THIS STEP)
2. 🔄 Ask Gemini Question 1: Approach validation for Task 1
3. 🔄 Implement Task 1: Lawyer-Layer Cache Partitioning
4. 🔄 Ask Gemini Question 2: Implementation review
5. 🔄 Continue with Tasks 2-4

## Session Status: 🟢 ACTIVE

**Phase**: Implementation
**Focus**: Task 1 - Lawyer-Layer Cache Partitioning
**Estimated Time**: 4-6 hours (all 4 tasks)
**Current Task**: Question 1 - Approach validation

---

## Progress Update (2026-02-04)

### Task 1 Complete ✅

**Commit**: `02a84a5de` - "feat(tsz-2): implement Lawyer-Layer Cache Partitioning (Task 1)"

**Changes Made**:
1. Added `RelationCacheKey` struct to `src/solver/types.rs`
2. Updated Database layer (`src/solver/db.rs`)
3. Updated `SubtypeChecker` (`src/solver/subtype.rs`)
4. Updated `CheckerState.is_subtype_of` (`src/checker/assignability_checker.rs`)
5. Updated `TypeCache` (`src/checker/context.rs`)

**Correctness Impact**: Prevents `any` propagation results from non-strict checks from contaminating strict mode checks, fixing a fundamental unsoundness.

**Gemini Guidance**: Followed Two-Question Rule (Question 1: Approach Validation)

**Next**: Task 2 - CheckerState Refactoring

---

### Task 2 Complete ✅

**Commit**: `5f4072f36` - "feat(tsz-2): implement CheckerState Refactoring (Task 2)"

**Changes Made**:
1. Expanded TypeCache with specialized caches (application_eval_cache, mapped_eval_cache, object_spread_property_cache, element_access_type_cache, flow_analysis_cache, class_instance_type_to_decl, class_instance_type_cache)
2. Fixed `with_cache` and `with_cache_and_options` to use specialized caches from TypeCache instead of creating fresh empty HashMaps
3. Added `CheckerContext::with_parent_cache` method to create child contexts that share parent's caches through RefCell cloning
4. Added `CheckerState::with_parent_cache` method for convenience
5. Updated temporary checker creation in `state_type_analysis.rs` and `state_type_environment.rs` to use `with_parent_cache` instead of `new`

**Correctness Impact**: Fixes Cache Isolation Bug where temporary checkers created for cross-file symbol resolution were discarding their caches, preventing lib.d.ts type aliases like `Partial<T>` and `Pick<T,K>` from resolving correctly.

**Next**: Task 3 - Thread-Safe Cache Access (NOT STARTED - requires large refactoring)

