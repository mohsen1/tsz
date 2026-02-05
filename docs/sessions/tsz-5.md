# Session TSZ-5: Multi-Pass Generic Inference & Contextual Typing

**Started**: 2026-02-05
**Status**: 🔄 Starting - Planning Phase
**Focus**: Implement multi-pass inference to fix complex nested generic type inference
**Blocker**: None - Ready to start implementation

## Summary

This session implements **Multi-Pass Inference** to fix a critical bug where complex nested generic functions fail to infer type parameters correctly.

### Problem Statement

**Current Bug**: Complex nested inference fails
```typescript
function map<T, U>(arr: T[], f: (x: T) => U): U[] {
    return arr.map(f);
}

const result = map([1, 2, 3], x => x.toString());
// Error: Property 'toString' does not exist on type 'T'
// Expected: T = number, U = string
```

**Root Cause**: When checking `arr.map(f)`:
- `arr` has type `T[]` where `T` is still a TypeParameter (not yet resolved)
- The compiler tries to look up the `map` method signature on Array<T>
- But `T` is not resolved yet, causing the callback parameter to have type `error`

### Solution: Multi-Pass Inference

TypeScript uses "Inference Rounds" (per Gemini Flash 2026-02-05):
1. **Round 1 (Non-contextual)**: Infer from arguments that don't depend on context (like `[1, 2, 3]`)
2. **Fixing**: "Fix" (resolve) the type variables that have enough information
3. **Round 2 (Contextual)**: Use the fixed types to provide contextual types to remaining arguments (like the lambda)

## Implementation Plan

### Priority 1: Refactor `resolve_generic_call_inner` (src/solver/operations.rs)

**Task**: Split the single argument loop into two distinct passes

**Current Code** (lines 680-745):
```rust
// Single loop processes all arguments
for (i, &arg_type) in arg_types.iter().enumerate() {
    // Collect constraints from all arguments at once
    self.constrain_types(...);
}
```

**New Code** (two passes):
```rust
// Round 1: Non-contextual arguments
for (i, &arg_type) in arg_types.iter().enumerate() {
    if !self.is_contextually_sensitive(arg_type) {
        // Process arrays, primitives, non-lambda types
        let target_type = self.param_type_for_arg_index(...)?;
        self.constrain_types(&mut infer_ctx, &var_map, arg_type, target_type, 
                            InferencePriority::NakedTypeVariable);
    }
}

// Fixing: Resolve variables with enough information
infer_ctx.strengthen_constraints()?;

// Round 2: Contextual arguments
for (i, &arg_type) in arg_types.iter().enumerate() {
    if self.is_contextually_sensitive(arg_type) {
        // Process lambdas, object literals with contextual types
        let current_subst = self.get_current_substitution(&infer_ctx, &var_map);
        let target_type = self.param_type_for_arg_index(...)?;
        let contextual_target = instantiate_type(self.interner, target_type, &current_subst);
        
        // Re-check lambda with contextual_target
        self.constrain_types(&mut infer_ctx, &var_map, arg_type, contextual_target,
                            InferencePriority::ReturnType);
    }
}
```

### Priority 2: Implement Contextual Sensitivity Detection

**Task**: Create utility to detect contextually sensitive types

**Location**: `src/solver/visitor.rs` or `src/solver/operations.rs`

**Logic**:
- Use visitor pattern (NOT manual TypeKey matching per Gemini)
- Detect: Function expressions, callables, object literals
- Return true if type contains contextual elements

**Signature**:
```rust
fn is_contextually_sensitive(&self, type_id: TypeId) -> bool {
    // Use visitor to check if type contains:
    // - Function types / Callables
    // - Object literals (freshness)
    // - Unresolved type parameters in sensitive positions
}
```

### Priority 3: Enhance InferenceContext (src/solver/infer.rs)

**Task**: Expose "fixing" mechanism for partial variable resolution

**Changes Needed**:
1. Modify `strengthen_constraints` (line 1345) to be callable multiple times
2. Ensure `resolve_with_constraints` (line 1045) can return partial results
3. Implement "fixed" state for variables (Candidate → Fixed → Finalized)

**Key Functions**:
- `fix_variables(&mut self, vars: Vec<InferenceVar>)` - Fix specific variables
- `is_fixed(&self, var: InferenceVar) -> bool` - Check if variable is fixed
- `get_current_substitution(&self) -> TypeSubstitution` - Get current best types

### Priority 4: Integration with Checker (src/checker/expr.rs)

**Task**: Ensure Checker provides contextual type to Solver

**Location**: `src/checker/call_checker.rs` or `src/checker/state_type_resolution.rs`

**Logic**:
- Identify when an argument is a lambda/function expression
- Pass "delayed" check flag to Solver
- Allow Solver to request re-check with contextual type

## Success Criteria

- [ ] `map([1, 2, 3], x => x.toString())` infers `T = number, U = string`
- [ ] `filter(arr, x => x > 0)` correctly infers predicate return type
- [ ] Array methods with callbacks work correctly
- [ ] No regressions in simple inference tests
- [ ] All 3 tests in `generic_inference_manual.rs` still pass

## Architectural Considerations (from Gemini)

### Solver/Checker Boundary
- **Solver (WHAT)**: Handles *when* to fix variables and *how* to merge constraints
- **Checker (WHERE)**: Identifies lambda arguments and passes "delayed" check to Solver

### Visitor Pattern Requirement
- **DO NOT** manually match on `TypeKey` to find lambdas
- **USE** visitor pattern in `src/solver/visitor.rs`
- May need to add `contains_contextual_element` method to visitor

### Inference Priorities (src/solver/types.rs)
- **Priority 1**: Naked type variables (e.g., `x: T`)
- **Priority 32**: Return types (contextual inference)
- **Deferred**: Function expressions/lambdas
- Multi-pass logic must respect these levels

### Fixing Logic (src/solver/infer.rs)
- "Fixing" moves variable from "Candidate" → "Resolved" state
- Once fixed, subsequent passes treat it as concrete type
- Still validate new constraints don't violate fixed type

## MANDATORY Gemini Workflow (per AGENTS.md)

### Question 1 (PRE-implementation) - REQUIRED
Before modifying `src/solver/operations.rs`:

```bash
./scripts/ask-gemini.mjs --include=src/solver/operations.rs --include=src/solver/infer.rs "
I am starting tsz-5 to implement Multi-Pass Inference.
I plan to refactor resolve_generic_call_inner to split argument processing into two passes:
1) Round 1: Process non-contextual arguments (arrays, primitives)
2) Fixing: Call strengthen_constraints to resolve variables
3) Round 2: Process contextual arguments (lambdas) with fixed types

Questions:
1) Is this the right approach? Where exactly should I split the loop?
2) How should I implement the 'fixing' mechanism in InferenceContext?
3) Are there specific pitfalls in infer.rs I should avoid?
4) How do I ensure fixed variables aren't overridden in Round 2?
"
```

### Question 2 (POST-implementation) - REQUIRED
After implementing the changes:

```bash
./scripts/ask-gemini.mjs --pro --include=src/solver/operations.rs --include=src/solver/infer.rs "
I implemented Multi-Pass Inference in resolve_generic_call_inner.

Changes:
[PASTE CODE OR DESCRIBE CHANGES]

Please review:
1) Is the two-pass logic correct for TypeScript?
2) Did I miss any edge cases?
3) Are there bugs in my fixing mechanism?
Be specific if it's wrong - tell me exactly what to fix.
"
```

## Related Sessions
- **tsz-2**: Coinductive Subtyping (Recursive Types) - COMPLETED
- **tsz-4**: Strict Null Checks & Lawyer Layer - COMPLETED
- **tsz-2 (appendix)**: Generic Type Inference Investigation - COMPLETED (discovered inference already implemented, found complex inference bug)

## Session History
Created 2026-02-05 following completion of Generic Type Inference Investigation in tsz-2.
