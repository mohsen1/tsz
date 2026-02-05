# Session TSZ-5: Discriminant Narrowing Robustness

**Started**: 2026-02-05
**Status**: 🔄 READY FOR IMPLEMENTATION - Clean Handoff
**Focus**: Fix 3 critical bugs in discriminant narrowing implementation

## Problem Statement

Recent implementation of discriminant narrowing (commit f2d4ae5d5) had **3 critical bugs** identified by Gemini Pro code review:
1. **Reversed subtype check** - Asked `is_subtype_of(property_type, literal)` instead of `is_subtype_of(literal, property_type)`
2. **Missing type resolution** - Didn't handle `Lazy`/`Ref`/`Intersection` types
3. **Broken for optional properties** - Failed on `{ prop?: "a" | "b" }` cases

## Gemini Pro Guidance (Question 1 Response)

### Validated Approach

**Key Functions to Modify:**
1. `NarrowingContext::resolve_type` - Handle Ref/Lazy/Intersection wrapper types
2. `NarrowingContext::get_type_at_path` - Fix line 261 TODO (pass resolver to PropertyAccessEvaluator)
3. `NarrowingContext::narrow_by_discriminant` - Use classify_for_union_members
4. `NarrowingContext::narrow_by_excluding_discriminant` - Fix subtype direction

### Implementation Steps

#### Step A: Unified Resolution
Modify `resolve_type` to handle `TypeKey::Ref` by:
- Checking if it can be mapped to a `DefId` or resolved directly via legacy `resolve_ref`
- Ensure all wrapper types are resolved before narrowing checks
- **Pitfall**: Failing to resolve `Ref` types will cause narrowing to fail on interfaces or classes

#### Step B: Fix Property Access (Line 261 TODO)
Update `PropertyAccessEvaluator::new()` to accept `self.resolver`:
- Critical for looking up properties on `Lazy` aliases and complex inheritance
- Optional properties already return `PossiblyNullOrUndefined` - wrap with `undefined` union
- This is a TODO in the current code

#### Step C: Correct Subtype Logic
- **Positive Narrowing (`===`)**: `is_subtype_of(literal_value, property_type)` ✅
  - Reason: The literal must be a valid inhabitant of the property's type
- **Negative Narrowing (`!==`)**: `is_subtype_of(property_type, excluded_value)` ✅
  - Reason: Only exclude if property is guaranteed to be the excluded value
  - Example: If property is `string` and we exclude `"a"`, keep the member (could be `"b"`)

**CRITICAL**: Double-check direction - `is_subtype_of(narrower, candidate)`

#### Step D: Intersection Handling
- **Positive**: If ANY part of intersection matches discriminant, whole intersection matches
- **Negative**: If ANY part matches excluded value, exclude entire intersection

### Edge Cases

1. **Nested Paths**: `x.payload.type === "a"` - recurse correctly through each level
2. **Optional Discriminants**: `{ type?: "a" } | { type: "b" }` checking `x.type === "a"`
3. **any/unknown in Unions**: Preserve `any` as it could satisfy any discriminant
4. **Unit Tuples**: Ensure tuple element narrowing works correctly

### Potential Pitfalls

1. **Fuel Exhaustion**: Counter needs to be high enough (100 is usually fine)
2. **Missing classify_for_union_members**: Always use classifier, not `union_list_id` directly
3. **Direction Reversal**: Extremely easy to flip source/target - verify carefully
4. **Resolver Availability**: `NarrowingContext` has `Option<&dyn TypeResolver>` - check if `None`

## Mandatory Workflow (From AGENTS.md)

### ✅ Question 1: Approach Validation (COMPLETED)
Response received and documented above.

### ⏳ Question 2: Implementation Review (PENDING)
After implementing, MUST ask:
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver/narrowing \
  "I implemented discriminant narrowing fixes for optional properties and type resolution.

  Changes: [PASTE CODE OR DIFF]

  Please review: 1) Is this correct for TypeScript? 2) Did I miss edge cases?
  Be specific if it's wrong - tell me exactly what to fix."
```

## Files to Modify

1. **`src/solver/narrowing.rs`** - Main narrowing logic
   - `resolve_type()` function
   - `get_type_at_path()` function (line 261 TODO)
   - `narrow_by_discriminant()` function
   - `narrow_by_excluding_discriminant()` function

2. **`src/solver/operations_property.rs`** - Property access logic
   - Ensure `PropertyAccessEvaluator` is used correctly
   - May need to modify constructor to accept resolver

3. **`src/solver/subtype.rs`** - Reference for unified type resolution logic
   - May need fixes for resolution helpers

## Debugging Approach

Use `tsz-tracing` skill to understand current behavior:
```bash
TSZ_LOG="wasm::solver::narrowing=trace" TSZ_LOG_FORMAT=tree \
  cargo test test_name -- --nocapture 2>&1 | head -200
```

## Dependencies

- Session tsz-1: Core type relations (may need coordination)
- Session tsz-2: Complete (circular inference)
- Session tsz-3: Narrowing (different domain - control flow)
- Session tsz-4: Emitter (different domain)

## Why This Is Priority

Per Gemini Pro and AGENTS.md:
- **High Impact**: Core TypeScript feature used daily
- **Recent Bugs**: 3 critical bugs found in recent implementation
- **Conformance**: Essential for matching `tsc` behavior exactly
- **User Experience**: Breaks common patterns with optional properties

## Clean Handoff Strategy

This session follows the "Clean Handoff" pattern:
1. ✅ Comprehensive Gemini Pro guidance documented
2. ⏳ Failing tests to be created (Definition of Done)
3. ✅ All changes committed and pushed
4. 📋 Next agent ready to implement with fresh context

## Next Steps for Implementing Agent

1. Read this session file thoroughly
2. Read the Gemini Pro guidance above (Step A-D)
3. Find the specific functions mentioned in narrowing.rs
4. Implement fixes following the validated approach
5. MANDATORY: Ask Gemini Question 2 for implementation review
6. Fix any issues Gemini identifies
7. Test and commit
