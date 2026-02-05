# Session TSZ-2: Circular Type Parameter Inference

**Started**: 2026-02-05
**Status**: 🔄 IN PROGRESS
**Focus**: Fix 5 failing circular `extends` tests in solver::infer

## Problem Statement

The current implementation fails to handle circular type parameter constraints (e.g., `T extends U, U extends T`) correctly. The solver uses simple iteration limits instead of proper coinductive (Greatest Fixed Point) resolution, causing inference to fail or produce incorrect results.

## Goal

Fix all 5 circular extends tests in `src/solver/infer.rs`:
1. `test_circular_extends_chain_with_endpoint_bound`
2. `test_circular_extends_three_way_with_one_lower_bound`
3. `test_circular_extends_with_literal_types`
4. `test_circular_extends_with_concrete_upper_and_lower`
5. `test_circular_extends_conflicting_lower_bounds`

## Gemini Pro Analysis

**Root Cause**: The `strengthen_constraints` function (lines 1208-1230 in `src/solver/infer.rs`) uses a simple iteration limit (`MAX_CONSTRAINT_ITERATIONS`) but doesn't implement true coinductive resolution for mutually dependent type parameters.

When cycles occur (e.g., `T extends U, U extends T`), the inference engine often defaults to `unknown` or triggers an `OccursCheck` error prematurely.

## Suggested Approach

Based on Gemini Pro's recommendation:

1. **Modify `strengthen_constraints`**: Implement dependency-graph-based resolution
   - Identify strongly connected components (SCCs) of type parameters
   - Parameters within the same SCC should be unified or resolved together

2. **Refine `occurs_in`**: Distinguish between:
   - Illegal recursion (type containing itself in a way that can't be lazily expanded)
   - Legal recursive constraints (F-bounded polymorphism like `T extends Comparable<T>`)

3. **Update `compute_constraint_result`**: When a cycle is detected, find the "least restrictive" type that satisfies the cycle by looking at the `extends` constraints

## Test Cases

### Test 1: Chain with Endpoint
```typescript
// T extends U, U extends V, V extends number
// Should resolve to number
```

### Test 2: Three-Way with Lower Bound
```typescript
// T extends U, U extends V, T extends string
// Should incorporate string constraint
```

### Test 3: Literal Types
```typescript
// Circular extends with literal type constraints
// Should preserve literal information
```

### Test 4: Concrete Upper and Lower
```typescript
// Both upper and lower bounds in a cycle
// Should find intersection or compatible type
```

### Test 5: Conflicting Lower Bounds
```typescript
// Multiple lower bounds in a cycle
// Should find best common type
```

## Code Locations

- `src/solver/infer.rs:330-374`: `expand_cyclic_upper_bound`
- `src/solver/infer.rs:1208-1230`: `strengthen_constraints`
- `src/solver/infer.rs:1232-1282`: `propagate_lower_bound` / `propagate_upper_bound`

## Potential Pitfalls

1. **Infinite Recursion**: Be extremely careful with `propagate_upper_bound`. If `T extends U` and `U extends T`, a naive propagator will bounce between them forever.

2. **Over-widening**: If circularity detection is too aggressive, you might resolve everything to `any` or `unknown`, violating the "Match tsc" goal.

## Dependencies

- Session tsz-1: Core type relations (must coordinate to avoid conflicts)
- Session tsz-3: Narrowing (no overlap - different domain)

## Progress Update (2026-02-05)

### Implemented Fixes (Per Gemini Pro Guidance)

1. **Modified `strengthen_constraints`** - Fixed-point iteration
   - Continues propagating until no new candidates added
   - Uses `changed` flag to detect stabilization

2. **Replaced `propagate_lower_bound/propagate_upper_bound`** with `propagate_candidates_to_upper`
   - Simplified propagation: candidates flow UP the extends chain
   - If T extends U (T <: U), then T's candidates are also U's candidates

3. **Removed `has_circular` special case** in `resolve_from_candidates`
   - Always filter by priority, even with circular candidates
   - High-priority direct candidates win over low-priority propagated ones

### Test Results

**PASSING (2/5):**
- ✅ test_circular_extends_chain_with_endpoint_bound
- ✅ test_circular_extends_three_way_with_one_lower_bound

**FAILING (3/5):**
- ❌ test_circular_extends_with_literal_types (BoundsViolation - needs cycle unification)
- ❌ test_circular_extends_conflicting_lower_bounds
- ❌ test_circular_extends_with_concrete_upper_and_lower

### Current Issue

The remaining failing tests involve TRUE cycles where type parameters need to unify and share candidates. The current implementation propagates candidates but doesn't fully unify type parameters in cycles.

Example: T extends U, U extends T with T.lower="hello", U.lower="world"
- Expected: Both T and U → STRING (unified)
- Actual: T → STRING, U → "world" (not unified, causes BoundsViolation)

### Next Steps

1. Investigate cycle detection and unification logic
2. Ask Gemini Pro: "How should type parameters in cycles be unified?"
3. Implement cycle unification (SCC detection + candidate merging)
4. Test and iterate

## Gemini Pro Algorithm (2026-02-05)

### Core Issues Identified

1. **Incorrect Fallback**: `resolve_with_constraints` defaults to upper bound when no candidates found. Should return `UNKNOWN` instead.
2. **Insufficient Propagation**: `strengthen_constraints` needs fixed-point iteration, not fixed count.
3. **Direction of Flow**: Candidates flow **up** the extends chain (T <: U means T's candidates are also U's candidates).

### Required Fixes

#### Fix 1: `compute_constraint_result` - Remove Upper Bound Fallback

**Location**: `src/solver/infer.rs`

```rust
let result = if !candidates.is_empty() {
    self.resolve_from_candidates(&candidates, is_const)
} else {
    // CRITICAL: Do NOT fall back to upper_bounds
    // If no lower bounds (candidates), inference failed - return UNKNOWN
    TypeId::UNKNOWN
};
```

**Why**: `T extends U` should NOT resolve `T = U` when T has no candidates. T should be UNKNOWN.

#### Fix 2: `strengthen_constraints` - Fixed-Point Propagation

```rust
pub fn strengthen_constraints(&mut self) -> Result<(), InferenceError> {
    let type_params: Vec<_> = self.type_params.clone();
    let mut changed = true;
    let mut iterations = 0;

    // Iterate to fixed point
    while changed && iterations < MAX_CONSTRAINT_ITERATIONS {
        changed = false;
        iterations += 1;

        for (name, var, _) in type_params.iter() {
            let root = self.table.find(*var);
            let info = self.table.probe_value(root).clone();

            // Propagate candidates UP the extends chain
            for &upper in info.upper_bounds.iter() {
                if self.propagate_candidates_to_upper(root, upper, *name)? {
                    changed = true;
                }
            }
        }
    }
    Ok(())
}
```

#### Fix 3: `propagate_candidates_to_upper` - New Helper

```rust
/// Propagates candidates from subtype to supertype
/// If var extends upper (var <: upper), then candidates of var are also candidates of upper
fn propagate_candidates_to_upper(
    &mut self,
    var_root: InferenceVar,
    upper: TypeId,
    exclude_param: Atom
) -> Result<bool, InferenceError> {
    // Check if upper is a type parameter we're inferring
    if let Some(TypeKey::TypeParameter(info)) = self.interner.lookup(upper) {
        if info.name != exclude_param {
            if let Some(upper_var) = self.find_type_param(info.name) {
                let upper_root = self.table.find(upper_var);

                // Don't propagate to self
                if var_root == upper_root {
                    return Ok(false);
                }

                // Get candidates from subtype (var)
                let var_candidates = self.table.probe_value(var_root).candidates.clone();

                // Add them to supertype (upper)
                for candidate in var_candidates {
                    if self.add_candidate_if_new(upper_root, candidate.type_id, InferencePriority::Circular) {
                        return Ok(true);
                    }
                }
            }
        }
    }
    Ok(false)
}
```

### Answers to Key Questions

1. **Chain vs Cycle Propagation**:
   - Chains (T extends U): Candidates flow UP. T's candidates become U's candidates.
   - Cycles (T extends U, U extends T): Candidates flow both ways, unifying them.

2. **Literal Type Preservation**:
   - Preserved: If `is_const` OR literal is the only candidate
   - Simplified: If not `const` and has multiple candidates
   - In cycles: `const` T keeps literal, non-const U widens to primitive

3. **Concrete Lower Bounds in Cycles**:
   - All params in cycle share candidates through propagation
   - Resolve from shared candidate set

## Notes

This is a deeper architectural challenge that requires understanding:
- Coinductive type systems (Greatest Fixed Point)
- Dependency graphs and Strongly Connected Components
- F-bounded polymorphism
