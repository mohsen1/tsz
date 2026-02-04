# Session tsz-2: Checker Context & Cache Unification

**Started**: 2026-02-04
**Status**: 🟡 Active (Phase 2: Architectural Fix)
**Previous**: BCT, Intersection Reduction, Nominal Subtyping (COMPLETED)

## CURRENT FOCUS: Checker Context & Cache Unification

### Problem Statement

The **Cache Isolation Bug** prevents lib.d.ts type aliases (like `Partial<T>`, `Pick<T,K>`) from resolving correctly. When the checker resolves types from lib files, it creates temporary `CheckerState` instances with private caches that are discarded. The main `CheckerContext` never sees these resolved types, causing them to resolve to `unknown`.

**Why This Matters**:
1. **North Star Alignment**: Section 4.5 states `CheckerContext` should be the shared source of truth
2. **Blocks Type Metaprogramming**: Mapped types, conditional types can't work without proper lib type resolution
3. **Downstream Impact**: tsz-3 (Narrowing) and tsz-4 (Emit) rely on accurate TypeIds from Checker

### Root Cause

In `src/checker/state_type_resolution.rs`, `get_type_of_symbol` creates temporary `CheckerState` instances:
```rust
let mut checker = CheckerState::new(
    symbol_arena.as_ref(),
    self.ctx.binder,
    self.ctx.types,
    self.ctx.file_name.clone(),
    self.ctx.compiler_options.clone(),
);
// checker has its own private ctx and symbol_types cache!
```

When this temporary checker is destroyed, all resolved types are lost.

## Implementation Plan

### Goal
Refactor `CheckerState` and `get_type_of_symbol` to ensure all type resolutions persist in the global `CheckerContext`, eliminating the "Cache Isolation Bug".

### Approach
1. **Audit Current Architecture**: Understand why temporary CheckerState instances are created
2. **Design Shared Context Pattern**: Ensure all type resolution uses the primary CheckerContext
3. **Handle Borrowing Issues**: RefCell borrowing conflicts must be resolved
4. **Test & Validate**: Verify Partial<T>, Pick<T,K> resolve correctly

## Previous Investigation Work

### Attempt 1: TypeEnvironment Registration ✅
- Added `insert_def_with_params` call in `resolve_lib_type_by_name`
- Result: No improvement (31.1% pass rate)
- Issue: Returns structural body instead of Lazy

### Attempt 2: Return Lazy(DefId) ✅
- Modified `resolve_lib_type_by_name` to return `Lazy(def_id)`
- Result: No improvement (31.1% pass rate)
- Issue: Cache isolation discards the resolved types

### Root Cause Discovery ✅
**Cache Isolation Bug** - temporary CheckerState instances discard their caches, preventing main context from seeing resolved lib types.

## Success Criteria

1. **Type Resolution**:
   - [ ] `Partial<T>` resolves to mapped type structure
   - [ ] `Pick<T,K>` resolves correctly
   - [ ] All lib.d.ts type aliases resolve to their actual types

2. **Conformance**:
   - [ ] Mapped type pass rate improves from 31.1%
   - [ ] No regressions in existing tests

3. **Architecture**:
   - [ ] No temporary CheckerState instances with private caches
   - [ ] All type resolution persists in global CheckerContext
   - [ ] North Star alignment maintained

## Session History

- 2026-02-04: Started as "Intersection Reduction and Advanced Type Operations"
- 2026-02-04: **COMPLETED** BCT, Intersection Reduction, Literal Widening
- 2026-02-04: **COMPLETED** Phase 1: Nominal Subtyping (all 4 tasks)
- 2026-02-04: **REDEFINED** to "Checker-Solver Bridge & Type Alias Resolution"
- 2026-02-04: **INVESTIGATED** TypeEnvironment registration issue
- 2026-02-04: **DISCOVERED** Cache Isolation Bug as root cause
- 2026-02-04: **REDEFINED** to "Checker Context & Cache Unification"

## Completed Commits (History)

- `7bf0f0fc6`: Intersection Reduction
- `7dfee5155`: BCT for Intersections + Lazy Support
- `c3d5d36d0`: Literal Widening for BCT
- `f84d65411`: Fix intersection sorting
- `d0b548766`: Add Visibility enum and parent_id
- `ec7a3e06b`: Add visibility detection helpers
- `43fd74dbf`: Populate visibility for class members
- `ac1e4432f`: Implement nominal subtyping for properties
- `8bb483b73`: Implement visibility-aware inheritance
- `3fbf499da`: Attempt TypeEnvironment registration
- `e28ca24f6`: Return Lazy(DefId) for lib type aliases
- `eccd47123`: Document Cache Isolation Bug

## Complexity: HIGH

**Why High**:
- Deep architectural change to CheckerContext
- Requires careful RefCell borrowing management
- Must maintain North Star alignment
- Risk of breaking existing functionality

**Mitigation**: Follow Two-Question Rule strictly. Use --pro flag for all architectural changes. All changes must be reviewed by Gemini Pro.
