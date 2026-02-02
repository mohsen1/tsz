# Rewrite Inference Algorithm: Candidate-Based System

**Reference**: Architectural Review Summary - Issue #2  
**Severity**: 🔴 Critical  
**Status**: TODO  
**Priority**: Critical - Correctness divergence from tsc

---

## Problem

Current implementation uses `ena::unify::InPlaceUnificationTable` (standard unification). TypeScript does NOT use standard unification - it collects candidates and performs "Best Common Type" or "Widening".

**Example**: `f<T>(a: T, b: T)` called with `f(1, "a")` - unification fails, but TypeScript infers `string | number`.

**Impact**: Any generic function with disparate arguments will fail in `tsz` but pass in `tsc`. High divergence risk.

**Location**: `src/solver/infer.rs`

---

## Solution: Candidate-Based Inference

Replace Union-Find approach with candidate-based inference system that collects candidates and then reduces them.

### Design: Candidate Collection System

```rust
/// Priority of an inference candidate (matches TypeScript)
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum InferencePriority {
    /// Inferred from a return type (lowest)
    ReturnType,
    /// Inferred from a circular dependency
    Circular,
    /// Inferred from an argument (standard)
    Argument,
    /// Inferred from a literal type (highest, subject to widening)
    Literal,
}

/// A candidate type for an inference variable
#[derive(Clone, Debug)]
pub struct InferenceCandidate {
    pub type_id: TypeId,
    pub priority: InferencePriority,
    // Tracks if this candidate came from a "fresh" literal (for widening)
    pub is_fresh_literal: bool, 
}

/// The value stored in the Unification Table
#[derive(Clone, Debug, Default)]
pub struct InferenceInfo {
    pub candidates: Vec<InferenceCandidate>,
    pub upper_bounds: Vec<TypeId>, // T extends U
}
```

### Algorithm Changes

1. **Collection**: When `constrain_types(source, target)` sees `target` is an inference variable, it adds `source` as a **Candidate** with `InferencePriority::Argument`.
2. **Unification**: When `unify_vars(A, B)` occurs, we **merge** their candidate lists and upper bounds.
3. **Resolution**:
   - Filter candidates by priority.
   - Perform **Widening**: If candidates are all literals of the same primitive (e.g., `1`, `2`), widen to the primitive (`number`).
   - Perform **Best Common Type (BCT)**: Find a supertype among candidates.
   - Fallback to **Union**.

---

## Implementation Phases

### Phase 1: Scaffolding (Safe Additions)
- Define `InferencePriority` and `InferenceCandidate` in `src/solver/infer.rs`.
- Create `InferenceInfo` struct to replace `ConstraintSet`.
- Implement `UnifyValue` for `InferenceInfo` to handle merging (concatenating vectors).

### Phase 2: Migration of InferenceContext
- Modify `InferenceContext` to use `InPlaceUnificationTable<InferenceVar>` where the value is `InferenceInfo` instead of `InferenceValue`.
- Replace `add_lower_bound` with `add_candidate(var, type_id, priority)`.
- Remove `unify_var_type` (strict unification) in favor of adding a candidate.

### Phase 3: BCT and Widening Logic
- Rewrite `best_common_type` to support widening.
  - *Input*: List of `TypeId`.
  - *Logic*:
      1. If all types are number literals -> return `number`.
      2. If all types are string literals -> return `string`.
      3. Otherwise, perform existing subtype reduction (keep supertypes, discard subtypes).
      4. Return Union of remaining types.

### Phase 4: Integration with Operations
- Update `src/solver/operations.rs`:
  - In `constrain_types_impl`, when `target` is an inference variable:
    ```rust
    // Old
    ctx.add_lower_bound(var, source);
    // New
    ctx.add_candidate(var, source, InferencePriority::Argument);
    ```

---

## Migration Strategy

To avoid breaking the build, we will modify `src/solver/infer.rs` in place.

1. **Step 1**: Modify `infer.rs`. Replace `ConstraintSet` usage with `InferenceInfo`.
2. **Step 2**: Update `ena` impl.
   ```rust
   impl UnifyValue for InferenceInfo {
       type Error = NoError;
       fn unify_values(a: &Self, b: &Self) -> Result<Self, NoError> {
           let mut merged = a.clone();
           merged.candidates.extend(b.candidates.clone());
           merged.upper_bounds.extend(b.upper_bounds.clone());
           Ok(merged)
       }
   }
   ```
3. **Step 3**: Update `resolve_with_constraints`.
   - Collect all candidates from the root variable.
   - Apply `widen_candidates`.
   - Apply `best_common_type`.
   - Check against `upper_bounds`.

---

## Test Cases

Create `src/solver/tests/inference_candidates_tests.rs`:

```rust
#[test]
fn test_infer_union_from_disjoint_primitives() {
    // f<T>(a: T, b: T) called with f(1, 'a')
    // Should infer: number | string
    let (db, mut ctx) = setup_inference();
    let t = ctx.fresh_type_param(db.intern_string("T"));
    
    ctx.add_candidate(t, TypeId::NUMBER, InferencePriority::Argument);
    ctx.add_candidate(t, TypeId::STRING, InferencePriority::Argument);
    
    let result = ctx.resolve_with_constraints(t).unwrap();
    
    // Verify result is Union(number, string)
    assert!(is_union_of(&db, result, &[TypeId::NUMBER, TypeId::STRING]));
}

#[test]
fn test_infer_widening_literals() {
    // f<T>(a: T, b: T) called with f(1, 2)
    // Should infer: number (not 1 | 2)
    let (db, mut ctx) = setup_inference();
    let t = ctx.fresh_type_param(db.intern_string("T"));
    
    let lit_1 = db.literal_number(1.0);
    let lit_2 = db.literal_number(2.0);
    
    ctx.add_candidate(t, lit_1, InferencePriority::Argument);
    ctx.add_candidate(t, lit_2, InferencePriority::Argument);
    
    let result = ctx.resolve_with_constraints(t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_infer_subtype_reduction() {
    // f<T>(a: T, b: T) called with f(Dog, Animal)
    // Should infer: Animal
    // Assuming Dog <: Animal
    // ... setup types ...
    // Result should be Animal
}
```

---

## Execution Plan

1. **Modify `src/solver/infer.rs`**:
   - Define `InferenceCandidate` and `InferenceInfo`.
   - Replace `ConstraintSet` with `InferenceInfo`.
   - Implement `UnifyValue` for `InferenceInfo` (merging logic).
2. **Implement Widening**:
   - Add `widen_candidates` function in `infer.rs`.
   - Update `best_common_type` to use widening.
3. **Update `src/solver/operations.rs`**:
   - Update `constrain_types` to use `add_candidate`.
4. **Verify**:
   - Run `cargo nextest run` to ensure no regressions.
   - Add the specific test case for `f(1, 'a')`.

---

## Acceptance Criteria

- [ ] `InferenceInfo` replaces `ConstraintSet`
- [ ] Candidate collection works for all inference scenarios
- [ ] Widening correctly converts literals to primitives
- [ ] BCT correctly reduces candidates to union
- [ ] Test case `f(1, 'a')` infers `string | number`
- [ ] Conformance tests pass with no regressions
