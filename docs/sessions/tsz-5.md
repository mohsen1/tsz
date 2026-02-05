# Session TSZ-5: Multi-Pass Generic Inference & Contextual Typing

**Started**: 2026-02-05
**Status**: 🔄 Partially Implemented - Infrastructure Complete, Integration In Progress
**Focus**: Implement multi-pass inference to fix complex nested generic type inference
**Last Updated**: 2026-02-05

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

## Implementation Status

### ✅ Complete Components

1. **Contextual Sensitivity Detection** (`src/solver/operations.rs:1373-1475`)
   - `is_contextually_sensitive()` helper to detect lambdas, callables, and object literals
   - Uses visitor pattern to recursively check types
   - Correctly identifies function types, callable types, unions, intersections, etc.

2. **Variable Fixing Mechanism** (`src/solver/infer.rs:3016-3109`)
   - `fix_current_variables()` method on `InferenceContext`
   - Resolves variables with candidates after Round 1
   - Sets `resolved` field to prevent Round 2 from overriding
   - `get_current_substitution()` returns current best types for all variables

3. **Two-Pass Argument Processing** (`src/solver/operations.rs:682-839`)
   - Round 1: Processes non-contextual arguments (arrays, primitives)
   - Fixing: Calls `fix_current_variables()` to resolve variables
   - Round 2: Processes contextual arguments (lambdas) with fixed types
   - Creates contextual target types by instantiating with current substitution

4. **TypeSubstitution API Enhancement** (`src/solver/instantiate.rs:96-102`)
   - Added `map()` method to expose internal substitution map
   - Enables building new substitutions from existing ones

### ⚠️ Known Limitations

**Current Issue**: Lambda type checker doesn't use contextual target types

The multi-pass inference infrastructure is in place, but there's a critical gap:
- Round 2 computes the contextual target type (e.g., `(x: number) => U`)
- However, the lambda type checker doesn't receive/use this contextual type
- Lambda parameters still have the original TypeParameter types

**Example Test**:
```typescript
function process<T, U>(value: T, callback: (x: T) => U): U {
    return callback(value);
}

const result = process(42, x => x.toString());
// Current: x has type T (TypeParameter)
// Expected: x should have type number (from T = number in Round 1)
```

**Error**: `Property 'toString' does not exist on type 'T'`

### 🔧 Remaining Work

**Required**: Integrate contextual target types with lambda type checking

1. **Modify lambda type checker** (`src/checker/expr.rs` or similar):
   - Accept contextual target type parameter
   - Use contextual type to infer lambda parameter types
   - Ensure lambda body checking uses inferred parameter types

2. **Pass contextual type from Solver to Checker**:
   - Modify `resolve_generic_call_inner` to return contextual types
   - Or add a separate mechanism to provide contextual types to Checker

3. **Handle nested generics**:
   - The `arr.map(f)` case requires method resolution on generic types
   - This is a separate issue from lambda contextual typing
   - May require additional work in property access resolution

## Implementation Plan

### Priority 1: Refactor `resolve_generic_call_inner` (src/solver/operations.rs) ✅ COMPLETE

**Task**: Split the single argument loop into two distinct passes

**Status**: Complete
- Round 1 processes non-contextual arguments
- Fixing resolves variables with candidates
- Round 2 processes contextual arguments with fixed types


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

## Next Steps

### ✅ COMPLETED: Gemini Pro Review
Asked Gemini Pro for guidance on lambda integration (2026-02-05). Key findings:

**Root Cause**: The current approach has the Solver doing multi-pass inference **after** the Checker has already computed all argument types. This is too late - the Checker needs to do two-pass argument checking itself.

**Correct Strategy** (from Gemini Pro):

The **Checker** must implement two-pass argument collection:
1. **Pass 1**: Check non-contextual arguments (primitives, objects) to get concrete types
2. **Partial Inference**: Ask Solver to infer type params based ONLY on Pass 1 args
3. **Pass 2**: Use inferred types to construct contextual types for lambdas, then check them

**Files to modify**:
- `src/solver/operations.rs`: Add `infer_contextual_parameter_type()` method to `CallEvaluator`
- `src/checker/call_checker.rs`: Modify call checking to do two-pass argument collection
- `src/checker/state.rs`: Add `is_contextually_sensitive_node()` helper

**Key insight**: Lambda checking happens in `src/checker/function_type.rs::get_type_of_function`, which looks at `self.ctx.contextual_type`. By setting this to the inferred signature (e.g., `(x: number) => U`), the lambda will correctly infer `x: number`.

### Priority 1: Implement Two-Pass Argument Checking in Checker

**Step 1: Add Solver API** (`src/solver/operations.rs`)
```rust
pub fn infer_contextual_parameter_type(
    &mut self,
    func: &FunctionShape,
    known_args: &[(usize, TypeId)], // (index, type) of arguments checked so far
    target_param_index: usize,      // The parameter index we need context for
) -> TypeId
```

**Step 2: Modify Call Checker** (`src/checker/call_checker.rs`)
- Separate arguments into "easy" (non-lambda) and "hard" (lambda)
- Check easy args first
- Call `infer_contextual_parameter_type` for each deferred arg
- Set `ctx.contextual_type` and check lambdas

**Step 3: Add Helper** (`src/checker/state.rs`)
```rust
pub fn is_contextually_sensitive_node(&self, idx: NodeIndex) -> bool
```

### Priority 2: Test and Validate
- Create comprehensive tests for multi-pass inference
- Verify no regressions in existing tests
- Document edge cases and limitations

### Priority 3: Method Resolution on Generic Types
The `arr.map(f)` case requires additional work to handle method calls on generic types before type parameters are resolved. This is a separate issue from lambda contextual typing.

---

## Implementation TODOs (Redefined by Gemini Flash 2026-02-05)

### Phase 1: Checker Orchestration (The "Split")

#### TODO 1.1: Implement Contextual Sensitivity Detection
**File**: `src/checker/expr.rs`
**Function**: `is_contextually_sensitive(node: NodeIndex) -> bool`
**Description**: Identifies expressions whose type depends on target type (lambdas, object literals)
**Status**: ⏳ Pending

#### TODO 1.2: Refactor Call Expression Checking
**File**: `src/checker/expr.rs`
**Function**: `check_call_expression`
**Description**: Two-pass argument check:
- Pass 1: Check non-contextual arguments
- Intermediate: Call Solver for partial inference
- Pass 2: Check contextual arguments with inferred signature
**Status**: ⏳ Pending
**Dependencies**: TODO 1.1, TODO 2.1

### Phase 2: Solver API Enhancement (The "Engine")

#### TODO 2.1: Expose Partial Inference API
**File**: `src/solver/mod.rs`
**Function**: `trait Solver { fn infer_type_parameters(...) }`
**Description**: API for partial inference based on subset of arguments
**Status**: ⏳ Pending

#### TODO 2.2: Support Partial Type Variable Fixing
**File**: `src/solver/infer.rs`
**Function**: `InferenceContext::infer_from_argument_types`
**Description**: Infer type variables from arguments while preserving state
**Status**: ⏳ Pending

### Phase 3: Contextual Type Propagation

#### TODO 3.1: Verify Expression Checker Accepts Contextual Type
**File**: `src/checker/expr.rs`
**Function**: `check_expression`
**Description**: Verify optional `contextual_type` parameter exists
**Status**: ⏳ Pending

#### TODO 3.2: Extract Lambda Parameter Types
**File**: `src/checker/expr.rs`
**Function**: `check_arrow_function`
**Description**: Extract parameter types from partially inferred signature
**Status**: ⏳ Pending
**Dependencies**: TODO 1.2, TODO 2.2

### Test Success Criteria

#### Test 1: Basic Array Map (North Star)
```typescript
declare function map<T, U>(arr: T[], callback: (arg: T) => U): U[];
const result = map([1, 2, 3], x => x.toString());
// Expected: T=number, U=string, result: string[]
```

#### Test 2: Nested Generics
```typescript
declare function pipe<A, B, C>(val: A, f1: (a: A) => B, f2: (b: B) => C): C;
const res = pipe(42, x => x.toString(), s => s.length);
// Expected: A=number, B=string, C=number
```

#### Test 3: Object Literal Context
```typescript
declare function handle<T>(config: { data: T, process: (t: T) => void }): void;
handle({ data: "hello", process: t => t.toUpperCase() });
// Expected: T=string
```

## Dependencies

1. **TODO 1.1** → **TODO 1.2**
2. **TODO 2.1** → **TODO 1.2**
3. **TODO 1.2** + **TODO 2.2** → **TODO 3.2**

## MANDATORY Gemini Workflow (Per AGENTS.md)

**Question 1 (Approach)** - Before implementing TODO 1.2:
```bash
./scripts/ask-gemini.mjs --include=src/checker/expr.rs --include=src/solver/infer.rs "
I am about to split check_call_expression into two passes in src/checker/expr.rs.
How should I handle object literals containing both values and lambdas?
Should the whole object be deferred to Pass 2?
"
```

**Question 2 (Review)** - After implementing TODO 2.2:
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver/infer.rs "
I implemented InferenceContext::infer_from_argument_types for two-pass inference.
Please review: Does it correctly preserve state between two calls from the Checker?
"
```
