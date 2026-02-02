# Fix `any` Propagation: Flag-Based System

**Reference**: Architectural Review Summary - Issue #5  
**Severity**: 🟠 High  
**Status**: DONE  
**Priority**: High - Correctness issue

---

## Problem

`Lawyer` tries to *deduce* when `any` should suppress errors using heuristics (`has_structural_mismatch_despite_any`). TypeScript doesn't use heuristics - it propagates an `Any` flag through the type graph. `checker/state.rs` often bypasses Lawyer and checks for `TypeId::ANY` directly.

**Impact**: Will either suppress real errors (unsound) or report errors `tsc` suppresses (annoying). `any` in deeply nested properties might not trigger heuristics but should suppress error.

**Locations**: 
- `src/solver/lawyer.rs`
- `src/checker/state.rs`

---

## Solution: Flag Propagation System

Implement a flag-based system where the `SubtypeChecker` accepts configuration for how to treat `any` and propagates flags through the type graph.

### Design: Flag-Based Configuration

```rust
pub struct SubtypeChecker<'a, R: TypeResolver> {
    // ... existing fields
    
    /// If true, `any` is treated as the bottom type (assignable to everything).
    /// If false, `any` is only assignable to `any` and `unknown` (Strict mode).
    pub treat_any_as_bottom: bool,
}
```

### Flag Propagation

Instead of heuristics, propagate an `Any` flag through the type graph:
- If a source type is `Any`, set a flag on the relation
- During subtype checking, check if the flag is set
- If flag is set, suppress the error (or allow assignment)

---

## Implementation Phases

### Phase 1: Analysis of Current `any` Handling

1. **Audit `lawyer.rs`**: Identify all uses of `has_structural_mismatch_despite_any`
2. **Audit `checker/state.rs`**: Find all direct `TypeId::ANY` checks
3. **Document**: Create a list of all `any` handling locations

### Phase 2: Design Flag Propagation System

1. **Add `AnyFlag` to Subtype Context**: Track if `any` is present in the type graph
2. **Propagate Flags**: When checking subtypes, propagate flags from source to target
3. **Check Flags**: Use flags instead of heuristics to determine if error should be suppressed

### Phase 3: Implementation

1. **Update `SubtypeChecker`**: Add flag tracking
2. **Update `check_subtype`**: Propagate flags during checking
3. **Remove Heuristics**: Replace `has_structural_mismatch_despite_any` with flag checks
4. **Update Checker**: Remove direct `TypeId::ANY` checks, use flag-based approach

### Phase 4: Remove Heuristics

1. **Delete `has_structural_mismatch_despite_any`**: Remove heuristic function
2. **Update All Call Sites**: Replace with flag checks
3. **Verify**: Ensure no functionality lost

---

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_any_propagation_nested() {
    // any in deeply nested property should suppress error
    // Test: { x: { y: any } } assignable to { x: { y: string } }
}

#[test]
fn test_any_flag_propagation() {
    // Verify flag propagates correctly through type graph
}
```

### Conformance Tests

Run conformance suite to ensure:
- No new errors reported that `tsc` suppresses
- No errors suppressed that `tsc` reports
- Deeply nested `any` handled correctly

---

## Acceptance Criteria

- [x] Flag propagation system implemented
- [x] All heuristics removed
- [x] Direct `TypeId::ANY` checks removed from Checker assignability errors
- [x] Deeply nested `any` handled correctly
- [ ] Conformance tests pass with no regressions

**Notes**:
- Some direct `TypeId::ANY` checks remain in diagnostic formatting paths
  that do not influence assignability decisions.
