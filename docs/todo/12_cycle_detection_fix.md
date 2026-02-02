# Fix Coinductive Cycle Detection

**Reference**: Architectural Review Summary - Issue #6  
**Severity**: 🟠 High  
**Status**: TODO  
**Priority**: High - Correctness issue

---

## Problem

`subtype.rs` implements universal Greatest Fixed Point (GFP) semantics. TypeScript is more nuanced - specific rules about where recursion is allowed (e.g., "tail-recursive" conditional types). Expansive types (grow on every recursion) might be incorrectly marked as valid.

**Impact**: Will accept invalid recursive types that `tsc` rejects as "Type instantiation is excessively deep" or "Circular constraint".

**Location**: `src/solver/subtype.rs`

---

## Solution: TypeScript-Specific Recursion Rules

Implement TypeScript's specific recursion rules instead of universal GFP.

### TypeScript's Recursion Rules

1. **Finite/Cyclic Recursion (Coinductive)**:
   - The type graph forms a closed loop (e.g., `interface List<T> { next: List<T> }`).
   - **Rule**: Valid. `in_progress` detection is correct.

2. **Expansive Recursion**:
   - The type grows indefinitely without repeating (e.g., `type T<X> = T<Box<X>>`).
   - **Rule**: Invalid. TypeScript emits `TS2589` ("Type instantiation is excessively deep").
   - **Constraint**: The solver should reject these or strictly limit them.

3. **Tail-Recursive Conditional Types**:
   - **Rule**: TypeScript allows significantly deeper recursion (limit ~1000 instead of ~50) for conditional types where the recursive branch is in a tail position.
   - **Pattern**: `type Loop<T> = T extends [infer A, ...infer Rest] ? Loop<Rest> : void`.
   - **Optimization**: These should be evaluated iteratively (loop) rather than recursively.

---

## Design: New Cycle Detection

### Refined SubtypeResult

Split `Provisional` into distinct states to separate "valid cycle" from "resource exhaustion".

```rust
pub enum SubtypeResult {
    True,
    False,
    CycleDetected, // Valid recursion (GFP) -> True
    DepthExceeded, // Invalid expansion -> False/Error
}
```

### Recursion Strategy

1. **Exact Match**: Keep `in_progress` for finite cycles. Returns `CycleDetected`.
2. **Depth Limit**: When `MAX_SUBTYPE_DEPTH` is reached, return `DepthExceeded` (treat as `False` to be sound, or `Error`).
3. **Tail Recursion**: Implement an iterative evaluator for conditional types to bypass standard depth limits for valid tail-recursive patterns.

### Expansive Type Guard

To fix the `Deep<Box<T>>` issue, we rely on the depth limit returning `False` (or `Error`) instead of `True`. This ensures that if we cannot prove subtyping within the limit (because types keep growing), we reject the relation rather than unsoundly accepting it.

---

## Implementation Phases

### Phase 1: Refactor SubtypeResult

Modify `src/solver/subtype.rs` to distinguish between cycles and depth limits.

- Update `SubtypeResult` enum.
- Update `check_subtype` to return `DepthExceeded` when depth limit is hit.
- Update `is_subtype_of` to treat `CycleDetected` as `true` and `DepthExceeded` as `false` (soundness fix).

### Phase 2: Implement Tail-Recursion Optimization

Modify `src/solver/evaluate_rules/conditional.rs` to detect and optimize tail recursion.

- Identify if `true_type` or `false_type` is a direct recursion of the current conditional.
- If tail recursive, use a `while` loop to evaluate instead of recursive `evaluate()` calls.
- Increase the effective depth limit for these specific patterns.

### Phase 3: Expansive Type Instantiation Check (Optional/Advanced)

If Phase 1 is too aggressive (rejects valid deep types), implement an "Instantiation Stack" in `instantiate.rs`.

- Track `(DefId, Vec<TypeId>)` pairs during instantiation.
- If we see the same `DefId` with "larger" type arguments (structurally containing the previous args), abort immediately with `Error`.

---

## Test Cases

### Case 1: Finite Recursion (Should Pass)
```typescript
// Standard coinductive case
interface Node { next: Node; }
type A = Node;
type B = { next: B };
// A <: B should be True
```

### Case 2: Expansive Recursion (Should Fail)
```typescript
// Expansive case - currently passes unsoundly
type Deep<T> = { val: T; next: Deep<Box<T>> };
declare let a: Deep<number>;
declare let b: Deep<string>;
// a <: b should be False (number vs string mismatch eventually)
// Currently returns True due to depth limit + GFP
```

### Case 3: Tail Recursion (Should Pass)
```typescript
// Tail recursive conditional
type Trim<S> = S extends ` ${infer R}` ? Trim<R> : S;
type T = Trim<"      hello">;
// Should evaluate to "hello" without hitting depth limit
```

### Case 4: Non-Tail Recursion (Should Fail/Limit)
```typescript
// Non-tail recursive (stack accumulation)
type Reverse<T> = T extends [infer H, ...infer R] ? [...Reverse<R>, H] : [];
// Should hit standard depth limit for large tuples
```

---

## Action Plan Summary

1. **Modify `SubtypeResult`** in `src/solver/subtype.rs` to distinguish `CycleDetected` from `DepthExceeded`.
2. **Change Logic**: `DepthExceeded` -> `False` (Soundness fix).
3. **Optimize**: Implement iterative evaluation for tail-recursive conditionals in `src/solver/evaluate_rules/conditional.rs`.
4. **Verify**: Run conformance tests to ensure no regression in valid recursive types.

---

## Acceptance Criteria

- [ ] `SubtypeResult` distinguishes cycles from depth exceeded
- [ ] Depth exceeded treated as `False` (sound)
- [ ] Tail-recursive conditionals optimized
- [ ] Expansive types correctly rejected
- [ ] Finite cycles still work correctly
- [ ] Conformance tests pass with no regressions
