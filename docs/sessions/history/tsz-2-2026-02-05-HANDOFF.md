# Session TSZ-5: Discriminant Narrowing Robustness

**Started**: 2026-02-05
**Status**: 🔄 IN PROGRESS
**Focus**: Fix critical bugs in discriminant narrowing implementation

## Problem Statement

Recent implementation of discriminant narrowing (commit f2d4ae5d5) had **3 critical bugs** identified by Gemini Pro:
1. **Reversed subtype check** - Asked `is_subtype_of(property_type, literal)` instead of `is_subtype_of(literal, property_type)`
2. **Missing type resolution** - Didn't handle `Lazy`/`Ref`/`Intersection` types
3. **Broken for optional properties** - Failed on `{ prop?: "a" | "b" }` cases

## Goal

Harden discriminant narrowing in `src/solver/narrowing.rs` to handle full complexity of `tsc` narrowing behavior.

## Focus Areas

### 1. Optional Property Discriminants
- Narrowing on unions where discriminant is an optional property
- Example: `{ prop?: "a" | "b" }`
- Must handle `undefined` in the union correctly

### 2. Type Resolution
- Properly resolve `Lazy`/`Ref`/`Intersection` types before narrowing
- Ensure subtype checks work on resolved types, not wrappers

### 3. In Operator Narrowing
- Discriminant narrowing via `in` operator
- Example: `"a" in obj` where obj has optional property `a`

### 4. Instanceof Narrowing
- Discriminant narrowing via `instanceof` operator
- Class constructor discriminants

### 5. Intersection Type Discriminants
- Handle `Intersection` types as discriminants
- Resolve all members of intersection for checking

## Files to Modify

- `src/solver/narrowing.rs` - Main narrowing logic
- `src/solver/subtype.rs` - Relation foundation (may need fixes)
- Test files in `src/solver/tests/` or `src/checker/tests/`

## Gemini Pro Guidance (2026-02-05)

### Validated Approach

**Key Functions to Modify:**
1. `NarrowingContext::resolve_type` - Handle Ref/Lazy/Intersection wrapper types
2. `NarrowingContext::get_type_at_path` - Pass resolver to PropertyAccessEvaluator
3. `NarrowingContext::narrow_by_discriminant` - Use classify_for_union_members
4. `NarrowingContext::narrow_by_excluding_discriminant` - Fix subtype direction

### Implementation Steps

**Step A: Unified Resolution**
- Modify `resolve_type` to handle `TypeKey::Ref` by mapping to `DefId` or legacy `resolve_ref`
- Ensure all wrapper types are resolved before narrowing checks

**Step B: Fix Property Access (Line 261 TODO)**
- Update `PropertyAccessEvaluator::new()` to accept `self.resolver`
- Critical for looking up properties on Lazy aliases and complex inheritance
- Optional properties already return `PossiblyNullOrUndefined` - wrap with `undefined` union

**Step C: Correct Subtype Logic**
- Positive Narrowing (`===`): `is_subtype_of(literal_value, property_type)` ✅
- Negative Narrowing (`!==`): `is_subtype_of(property_type, excluded_value)` ✅
- Direction matters! Double-check: `is_subtype_of(narrower, candidate)`

**Step D: Intersection Handling**
- Positive: If ANY part matches, whole intersection matches
- Negative: If ANY part matches excluded value, exclude entire intersection

### Edge Cases

1. **Nested Paths**: `x.payload.type === "a"` - recurse correctly
2. **Optional Discriminants**: `{ type?: "a" } | { type: "b" }`
3. **any/unknown in Unions**: Preserve `any` as it could satisfy any discriminant
4. **Unit Tuples**: Ensure tuple element narrowing works

### Potential Pitfalls

1. **Fuel Exhaustion**: Counter needs to be high enough (100 is usually fine)
2. **Missing classify_for_union_members**: Always use classifier, not `union_list_id` directly
3. **Direction Reversal**: Easy to flip source/target - verify carefully
4. **Resolver Availability**: Check if `Option<&dyn TypeResolver>` is `None`

## Progress

- [ ] Read current narrowing.rs implementation
- [ ] Identify line 261 TODO in get_type_at_path
- [ ] Implement unified resolution for Ref/Lazy/Intersection types
- [ ] Fix PropertyAccessEvaluator resolver passing
- [ ] Verify and fix subtype direction logic
- [ ] Test with existing discriminant narrowing tests
- [ ] Ask Gemini Question 2 for implementation review

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
