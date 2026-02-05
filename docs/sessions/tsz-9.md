# Session TSZ-9: Conditional Type Inference (`infer T`)

**Started**: 2026-02-05
**Status**: Active
**Goal**: Implement `infer` type parameter inference within conditional types

## Problem Statement

From NORTH_STAR.md:

TypeScript's conditional types support type parameter inference via the `infer` keyword:

```typescript
type ReturnType<T> = T extends (...args: any[]) => infer R ? R : any;
type T = ReturnType<() => string>; // T is string
```

The `infer R` declaration extracts the return type from the function type. This is critical for modern TypeScript libraries (Zod, TRPC, utility types) and requires sophisticated pattern matching within the Solver.

**Impact:**
- Blocks utility type implementations (ReturnType, Parameters, ThisParameterType, etc.)
- Prevents generic constraint inference in conditional types
- Critical for modern TypeScript ecosystem compatibility

## Technical Details

**Files**:
- `src/solver/infer.rs` - Type parameter inference logic
- `src/solver/evaluate.rs` - Conditional type evaluation
- `src/solver/subtype.rs` - Subtype checking for `extends` clause
- `src/solver/types.rs` - Type structures (ConditionalType, InferType)

**Root Cause**:
Conditional type evaluation needs to:
1. Check if `T extends U` (using subtype checker)
2. If true, extract inferred types from `infer` declarations in `V`
3. Substitute inferred types for type parameters in `V`
4. Handle contravariant positions (function parameters, `infer` in `extends` clause)
5. Handle multiple/overlapping `infer` declarations for the same type parameter

## Implementation Strategy

### Phase 1: Investigation (Pre-Implementation) ✅ COMPLETE

1. ✅ Read `docs/architecture/NORTH_STAR.md` sections on Conditional Types
2. ✅ Ask Gemini: "What's the correct approach for implementing `infer` in conditional types?"
3. ⏳ Review existing conditional type evaluation in `src/solver/evaluate_rules/`

**Gemini Guidance Summary** (Question 1 - Approach Validation):

**Discovery**: Much of the `infer` infrastructure already exists!
- `src/solver/evaluate_rules/infer_pattern.rs` - Pattern matching logic
- `src/solver/evaluate_rules/conditional.rs` - Conditional type evaluation
- `src/solver/instantiate.rs` - Type substitution

**Key Implementation Files**:
- `match_infer_pattern()` - Recursively walks source against pattern
- `bind_infer()` - Assigns discovered type to `infer` name
- `substitute_infer()` - Replaces `infer` placeholders with inferred types

**Main Gap to Fix**:
- **Contravariant Intersection Logic**: Multiple `infer` declarations in contravariant positions (function parameters) should produce **intersections**, not unions
- Need to add `polarity` flag to distinguish covariant vs contravariant positions
- Covariant → use `union2`
- Contravariant → use `intersection2`

**Edge Cases to Handle**:
- Multiple `infer` declarations for same type parameter
- Naked type parameters (distributivity)
- Recursive inference (tail recursion)
- `any` and `never` special cases
- Lazy/DefId resolution before matching

### Phase 2: Implementation (Current Phase)

1. ✅ TypeKey::Infer already exists in types.rs
2. ⏳ Review existing `match_infer_pattern` implementation
3. ⏳ Add `polarity` parameter for variance handling
4. ⏳ Fix contravariant intersection logic
5. ⏳ Handle Lazy/DefId resolution in pattern matching
6. ⏳ Test with utility types (ReturnType, Parameters, etc.)

### Phase 3: Validation
1. Write unit tests for `infer` extraction
2. Test with complex conditional types
3. Ask Gemini Pro to review implementation

## Success Criteria

- [ ] `type T = ReturnType<() => string>` evaluates to `string`
- [ ] `type P = Parameters<(a: number, b: string) => void>` evaluates to `[number, string]`
- [ ] `infer` in contravariant positions works correctly
- [ ] Multiple `infer` declarations for same parameter are handled
- [ ] Conditional types with generic constraints work

## Session History

*Created 2026-02-05 after completing TSZ-4 (Lawyer Layer Audit).*
*Renamed from TSZ-8 due to naming conflict with existing session.*

---

## Investigation Results (2026-02-05)

### Existing Implementation Found ✅

**File**: `src/solver/evaluate_rules/infer_pattern.rs` (1,085 lines)

**Key Functions**:
- `match_infer_pattern()` (line 845) - Main pattern matching entry point
- `bind_infer()` (line 286) - Bind inferred type with constraint checking
- `substitute_infer()` (line 28) - Replace infer placeholders with bindings
- `type_contains_infer()` (line 41) - Check if type contains infer

### Bugs Identified 🔍

**Bug #1: Always Uses Union for Multiple Infer Declarations**
- Location: Lines 886-888, 946-950
- Current code:
```rust
if let Some(existing) = merged.get_mut(&name) {
    if *existing != ty {
        *existing = self.interner().union2(*existing, ty); // ALWAYS UNION!
    }
}
```
- Problem: Should use `intersection2` for contravariant positions (function parameters)
- Impact: Incorrect type inference for overlapping `infer` declarations in function types

**Example that should fail**:
```typescript
type Bar<T> = T extends (x: infer U) => void | (x: infer U) => void ? U : never;
// Should produce intersection, but current code produces union
```

**Bug #2: No Polarity/Variance Tracking**
- The code has no way to know if it's in a covariant or contravariant position
- Function parameters are contravariant
- Function return types are covariant
- Need to add `polarity: bool` parameter to track this

### Implementation Plan

1. Add `polarity: bool` parameter to `match_infer_pattern()`
   - `true` = covariant (use union)
   - `false` = contravariant (use intersection)

2. Update all recursive calls to pass correct polarity:
   - Function parameters: `polarity = false`
   - Function return types: `polarity = true`
   - Object properties: `polarity = true`
   - Array elements: `polarity = true`
   - Tuple elements: `polarity = true`

3. Fix binding merge logic:
```rust
if polarity {
    *existing = self.interner().union2(*existing, ty); // Covariant
} else {
    *existing = self.interner().intersection2(*existing, ty); // Contravariant
}
```

### Files to Modify

1. `src/solver/evaluate_rules/infer_pattern.rs`
   - Add `polarity` parameter to `match_infer_pattern()`
   - Add `polarity` parameter to helper functions
   - Fix merge logic at 3 locations (lines 888, 949, etc.)

2. Tests needed for:
   - Multiple infer in function parameters (contravariant → intersection)
   - Multiple infer in return types (covariant → union)
   - Mixed polarity cases


---

## Gemini Pro Review (Question 2) - ✅ APPROVED ✅

**Verdict**: Implementation plan is CORRECT! Green light to proceed.

### Key Improvements from Gemini Pro

**1. Use Enum (not bool)**
```rust
pub enum InferencePolarity {
    Covariant,
    Contravariant,
}
```

**2. Additional Contravariant Positions**
- Function parameters ✅ (planned)
- Constructor parameters ✅ (add)
- Setters (write_type) ✅ (add)
- Methods: Treat as contravariant for inference

**3. Recursive Call Updates**
- `match_infer_function_pattern`: Params = Contravariant, Return = Covariant
- `match_infer_callable_pattern`: Params = Contravariant, Return = Covariant
- `match_infer_constructor_pattern`: Params = Contravariant
- Object/Array/Tuple: Preserve current polarity

**4. Merge Logic Fix**
```rust
*existing = match polarity {
    InferencePolarity::Covariant => self.interner().union2(*existing, ty),
    InferencePolarity::Contravariant => self.interner().intersection2(*existing, ty),
};
```

### Action Plan

1. ✅ Define InferencePolarity enum
2. ⏳ Update match_infer_pattern signature
3. ⏳ Update all recursive call sites
4. ⏳ Fix merge logic at 3 locations
5. ⏳ Test with examples

### Files to Modify

1. `src/solver/evaluate_rules/infer_pattern.rs` (main changes)
2. `src/solver/evaluate_rules/conditional.rs` (update call site)
3. Add tests for contravariant intersection


---

## Implementation Attempt - PAUSED

### Discovery: Large Refactoring Scope ⚠️

After starting implementation, discovered that updating `match_infer_pattern` signature requires:
- **20+ call sites** in infer_pattern.rs alone
- **Additional call sites** in conditional.rs and other files
- **High risk** of introducing bugs in critical type inference logic

### Changes Attempted

✅ Added InferencePolarity enum  
✅ Updated match_infer_pattern signature  
✅ Fixed 2 merge logic locations  
⏸️ PAUSED: Need to update 20+ call sites

### Better Approach Needed

Given the scope, need to ask Gemini about:
1. Should we use a different refactoring strategy?
2. Can we add a wrapper/helper to reduce call site changes?
3. Should we tackle this in smaller increments?
4. Is there a way to add the parameter with a default?

**Current Status**: Changes stashed, awaiting guidance on better approach.

**Next Step**: Ask Gemini for safer refactoring strategy.

