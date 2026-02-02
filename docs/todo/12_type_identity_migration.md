# Complete Type Identity Migration (SymbolRef → DefId)

**Reference**: Architectural Review Summary - Issue #9  
**Severity**: 🟠 High  
**Status**: TODO  
**Priority**: High - Correctness and consistency

---

## Problem

Half-migrated state between `SymbolRef` (Binder-owned) and `DefId` (Solver-owned). `TypeKey::Ref(SymbolRef)` and `TypeKey::Lazy(DefId)` both exist. `TypeInterner` treats them as different types even if they refer to same symbol.

**Impact**: Breaks O(1) equality promise. Valid subtypes fail checks because different handles used for same symbol.

**Locations**: 
- `src/solver/types.rs`
- `src/checker/state_type_resolution.rs`
- `src/solver/lower.rs`

---

## Goal

Establish `DefId` as the single source of truth for nominal type identity in the Solver, removing `SymbolRef` and `TypeKey::Ref`. This ensures O(1) type equality, fixes "split identity" bugs, and decouples the Solver from the Binder.

---

## Analysis of Current Usage

| Feature | Current Implementation | Problem |
|---------|------------------------|---------|
| **Type Representation** | `TypeKey::Ref(SymbolRef)` AND `TypeKey::Lazy(DefId)` | `intern(Ref(1))` ≠ `intern(Lazy(1))` even if they refer to the same type. |
| **Type Lowering** | `src/solver/lower.rs` produces `TypeKey::Ref` by default. | New code paths produce `Lazy`, old produce `Ref`. |
| **Symbol Resolution** | `CheckerState` resolves to `SymbolId`, then wraps in `Ref`. | Solver has to call back to Checker to resolve `Ref`. |
| **Type Environment** | Stores both `SymbolRef -> TypeId` and `DefId -> TypeId`. | Redundant storage; potential for sync issues. |
| **Cycle Detection** | `SubtypeChecker` tracks `seen_refs` (SymbolRef pairs). | Needs to track `DefId` pairs instead. |

**Conclusion**: The existence of two keys for the same logical type breaks the interner's O(1) equality guarantee. We must migrate fully to `DefId`.

---

## Selected Identity System: `DefId`

We will standardize on **`DefId`** (Solver-owned Definition ID).

- **Definition**: `pub struct DefId(pub u32)` in `src/solver/def.rs`.
- **Storage**: `DefinitionStore` maps `DefId` -> `DefinitionInfo` (metadata, body TypeId).
- **Mapping**: Checker maintains `SymbolId <-> DefId` mapping.
- **TypeKey**: `TypeKey::Ref(SymbolRef)` will be removed. `TypeKey::Lazy(DefId)` will be renamed to `TypeKey::Ref(DefId)`.

---

## Migration Phases

### Phase 1: Infrastructure Bridge (The "Mapper")

Ensure `TypeLowering` can translate `SymbolId` to `DefId` without direct access to the Binder.

1. **Update `TypeLowering` Context**:
   - Modify `TypeLowering` struct in `src/solver/lower.rs`.
   - Replace `type_resolver: Fn(NodeIndex) -> Option<u32>` (returns SymbolId) with a `def_id_resolver: Fn(NodeIndex) -> Option<DefId>`.
2. **Update Checker Bridge**:
   - In `src/checker/state_type_resolution.rs`, update the closure passed to `TypeLowering`.
   - The closure should call `ctx.get_or_create_def_id(symbol_id)` immediately upon resolving a symbol.

### Phase 2: Switch Producers (Lowering)

Stop producing `TypeKey::Ref(SymbolRef)`.

1. **Refactor `lower_type_reference` (`src/solver/lower.rs`)**:
   - When resolving an identifier/qualified name, use the `def_id_resolver`.
   - Emit `TypeKey::Lazy(def_id)` instead of `TypeKey::Ref`.
2. **Refactor `lower_type_query`**:
   - Resolve value symbols to `DefId`s (requires `DefinitionInfo` to support value definitions or a separate ValueDefId).
   - *Interim*: Keep `TypeQuery` using `SymbolRef` if `DefId` is strictly for types, OR extend `DefKind` to include Values.
   - *Decision*: For this plan, focus on **Type** identity. `TypeQuery` can remain `SymbolRef` temporarily or be migrated in a sub-phase.

### Phase 3: Switch Consumers (Solver Logic)

Update the Solver to understand and resolve `DefId`s exclusively.

1. **Update `TypeResolver` Trait (`src/solver/subtype.rs`)**:
   - Promote `resolve_lazy` to the primary resolution method.
   - Deprecate `resolve_ref`.
2. **Update `SubtypeChecker`**:
   - Update `check_ref_ref_subtype` to compare `DefId`s.
   - Use `DefId` for cycle detection (`seen_defs` instead of `seen_refs`).
3. **Update `TypeEvaluator` (`src/solver/evaluate.rs`)**:
   - Ensure `evaluate_type` handles `TypeKey::Lazy` by resolving via `TypeEnvironment::get_def`.

### Phase 4: Cleanup & Unification

Remove the old system.

1. **Modify `TypeKey` (`src/solver/types.rs`)**:
   - Remove `Ref(SymbolRef)`.
   - Rename `Lazy(DefId)` to `Ref(DefId)`.
2. **Clean `TypeEnvironment`**:
   - Remove `types` (SymbolRef map).
   - Rename `def_types` to `types`.
3. **Clean `CheckerState`**:
   - Remove `symbol_types` cache (or key it by `DefId`).

---

## Update Checklist (Files & References)

| File | Change |
|------|--------|
| `src/solver/types.rs` | Remove `TypeKey::Ref(SymbolRef)`. Rename `Lazy(DefId)` → `Ref(DefId)`. |
| `src/solver/lower.rs` | Update `lower_type_reference`, `lower_qualified_name_type` to produce `DefId` refs. |
| `src/checker/context.rs` | Ensure `get_or_create_def_id` is robust and used everywhere. |
| `src/checker/state_type_resolution.rs` | Update `get_type_from_type_reference` to use DefId resolver. |
| `src/solver/subtype.rs` | Remove `resolve_ref`. Update `check_ref_ref_subtype` to use `DefId`. |
| `src/solver/evaluate.rs` | Update `evaluate_type` to handle new `Ref(DefId)`. |
| `src/solver/format.rs` | Update `TypeFormatter` to look up names via `DefId` (needs access to `DefinitionStore`). |
| `src/solver/intern.rs` | Update `intern` method to handle new keys. |

---

## Testing Strategy

### Identity Verification (The "O(1)" Test)

Create a test case that defines a type and references it in two ways that previously diverged.

```rust
#[test]
fn test_def_id_identity_convergence() {
    let code = "
        type A = number;
        type B = A; // Ref 1
        type C = A; // Ref 2
    ";
    // ... parse and check ...
    
    // Get TypeId for B's body (Ref to A)
    let type_b_body = ...; 
    // Get TypeId for C's body (Ref to A)
    let type_c_body = ...;

    // Assert they are the EXACT same TypeId (u32 equality)
    assert_eq!(type_b_body, type_c_body, "Type references to same symbol must intern to same TypeId");
}
```

### Recursion & Cycle Detection

Verify that `DefId` based cycle detection works for recursive types.

```rust
#[test]
fn test_recursive_type_alias_def_id() {
    let code = "type List<T> = { next: List<T> }";
    // Should not stack overflow
    // Should resolve to a TypeId that refers to itself via DefId
}
```

### Conformance

Run the full conformance suite. The migration should be purely internal refactoring; no observable behavior change in diagnostics (except potentially fixing bugs related to split identity).

### Performance

Measure memory usage. `DefId` migration should slightly reduce memory by deduplicating `Ref` and `Lazy` variants of the same type.

---

## Immediate Next Step

Execute **Phase 3.2**: Unify Type Resolution to standardize on DefId-first resolution.

---

## Progress Updates

### ✅ Completed: Phase 3.1 - Add DefId Cycle Detection (Feb 3, 2026)

**Approach**: Mirror `check_ref_ref_subtype` pattern for DefId
- **Problem**: Lazy(DefId) types need cycle detection at DefId level
- **Solution**: Add `seen_defs: HashSet<(DefId, DefId)>` parallel to `seen_refs`

**Implementation:**
1. Added `seen_defs` field to `SubtypeChecker` struct
   - Tracks DefId pairs during subtype checking
   - Prevents infinite recursion in recursive type aliases

2. Created `check_lazy_lazy_subtype()` in `src/solver/subtype_rules/generics.rs`
   - Mirrors `check_ref_ref_subtype()` exactly
   - Identity check: same DefId → True
   - Cycle detection: check seen_defs before resolving
   - Resolution: resolve Lazy(DefId) → structural form
   - Cleanup: remove pair after checking

3. Updated `check_subtype_inner()` in `src/solver/subtype.rs`
   - Call `check_lazy_lazy_subtype()` when both types have DefIds
   - Replaced existing lazy handling that lacked proper cycle detection

**Benefits:**
- Lazy(DefId) types now have same cycle detection as Ref(SymbolRef)
- Prevents infinite recursion in recursive type aliases using DefId
- Foundation for Phase 3.2 (unified resolution)

**Testing:**
- Before: 7795 passed, 13 failed, 170 ignored
- After: 7796 passed, 12 failed, 170 ignored
- **One test fixed!** DefId cycle detection is working correctly

**Commits:**
- `a9cbd7e1a`: Phase 3.1 complete

**Acceptance Criteria Progress:**
- [x] Phase 1 infrastructure in place
- [x] Phase 2: TypeLowering prefers DefId
- [x] Phase 3.1: DefId cycle detection added
- [ ] Phase 3.2: Unified type resolution
- [ ] Phase 3.3: Unified generic application
- [ ] Phase 3.4: resolve_ref() deprecated
- [ ] Phase 3.5: TypeEnvironment optimized
- [ ] `TypeKey::Ref(SymbolRef)` removed
- [ ] `TypeKey::Lazy(DefId)` renamed to `TypeKey::Ref(DefId)`
- [ ] All type references use `DefId`
- [ ] O(1) equality preserved
- [ ] Cycle detection uses `DefId`
- [x] Conformance tests pass with no regressions
- [ ] Memory usage reduced

---

## Progress Updates

### ✅ Completed: Phase 2 - Switch Producers (Feb 2, 2026)

**Approach**: Hybrid Resolver with DefId Preference
- **Problem**: TypeLowering needs to prefer `DefId` when available, but fall back to `SymbolId` for types without DefIds
- **Solution**: Create `with_hybrid_resolver()` constructor that accepts both resolvers
- **Key Insight**: TypeLowering can check for existing DefIds and use them, while still creating SymbolRefs for new types

**Implementation:**
1. Added `get_existing_def_id()` helper to `src/checker/context.rs`
   - Looks up existing DefIds without creating new ones
   - Returns None if DefId doesn't exist yet

2. Added `with_hybrid_resolver()` constructor to `src/solver/lower.rs`
   - Accepts both `type_resolver` and `def_id_resolver`
   - Sets both fields in TypeLowering struct

3. Modified `lower_qualified_name_type()` and `lower_identifier_type()`
   - Prefer DefId: `resolve_def_id()` → `intern(TypeKey::Lazy(def_id))`
   - Fall back to SymbolId: `resolve_type_symbol()` → `reference(SymbolRef)`

4. Updated 2 TypeLowering call sites in `src/checker/state_type_resolution.rs`
   - Use `with_hybrid_resolver()` instead of `with_resolvers()`
   - DefId resolver closure: `resolve_type_symbol_for_lowering()` → `get_existing_def_id()`
   - Kept post-processing step to create DefIds for types without them

**Benefits:**
- TypeLowering now creates `Lazy(DefId)` directly when DefId exists
- Eliminates double interning for repeated type references (same symbol → same DefId → same TypeId)
- Maintains backward compatibility with SymbolRef fallback
- No new test failures

**Testing:**
- Before Phase 2: 7795 passed, 13 failed, 170 ignored
- After Phase 2: 7795 passed, 13 failed, 170 ignored
- Pre-existing failures confirmed (not caused by Phase 2)
- Code compiles cleanly

**Commits:**
- (To be committed)

**Acceptance Criteria Progress:**
- [x] Phase 1 infrastructure in place
- [x] TypeLowering can produce Lazy(DefId) via post-processing
- [x] TypeLowering prefers DefId when available (Phase 2)
- [ ] `TypeKey::Ref(SymbolRef)` removed
- [ ] `TypeKey::Lazy(DefId)` renamed to `TypeKey::Ref(DefId)`
- [ ] All type references use `DefId` (still uses Ref for new types)
- [ ] O(1) equality preserved
- [ ] Cycle detection uses `DefId`
- [x] Conformance tests pass with no regressions
- [ ] Memory usage reduced (not yet measured)

---

### ✅ Completed: Phase 1 - Infrastructure Bridge (Feb 2, 2026)

**Approach**: Hybrid Pattern (Option 1 from Gemini analysis)
- **Problem**: TypeLowering API expects `Fn` resolvers, but `get_or_create_def_id` requires `&mut self`
- **Solution**: Post-process TypeLowering output to convert `TypeKey::Ref` → `TypeKey::Lazy`
- **Key Insight**: Phase 4.3 (type_literal_checker.rs) bypasses TypeLowering entirely

**Implementation:**
1. Added `maybe_create_lazy_from_resolved()` helper to `src/checker/context.rs`
   - Post-processes TypeId from TypeLowering
   - Converts `TypeKey::Ref(SymbolRef)` → `TypeKey::Lazy(DefId)` when applicable
   - Creates DefIds after lowering (when &mut self is available)

2. Updated 2 TypeLowering call sites in `src/checker/state_type_resolution.rs`
   - Pattern: `lowering.lower_type()` → `maybe_create_lazy_from_resolved()` → return
   - No changes to TypeLowering API or resolver types

3. Infrastructure from earlier commits:
   - Added `def_id_resolver` field to `TypeLowering` (for future use)
   - Added `lazy()` convenience method to `TypeInterner`

**Testing:**
- All 3394 solver tests passing
- No regressions
- Code compiles cleanly

**Commits:**
- `05796d28e`: Added DefId infrastructure (partial)
- `55ca84f09`: Completed Phase 1 with hybrid pattern

**Acceptance Criteria Progress:**
- [x] Phase 1 infrastructure in place
- [x] TypeLowering can produce Lazy(DefId) via post-processing
- [ ] `TypeKey::Ref(SymbolRef)` removed
- [ ] `TypeKey::Lazy(DefId)` renamed to `TypeKey::Ref(DefId)`
- [ ] All type references use `DefId` (production only, still creates Ref internally)
- [ ] O(1) equality preserved
- [ ] Cycle detection uses `DefId`
- [x] Conformance tests pass with no regressions
- [ ] Memory usage reduced (not yet measured)

---

## Acceptance Criteria

- [ ] `TypeKey::Ref(SymbolRef)` removed
- [ ] `TypeKey::Lazy(DefId)` renamed to `TypeKey::Ref(DefId)`
- [ ] All type references use `DefId`
- [ ] O(1) equality preserved
- [ ] Cycle detection uses `DefId`
- [ ] Conformance tests pass with no regressions
- [ ] Memory usage reduced (deduplication)
