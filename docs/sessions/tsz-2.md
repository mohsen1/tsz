# Session tsz-2: Advanced Type Evaluation & Inference

**Started**: 2026-02-04
**Status**: 🟡 Active (Phase 1: Conditional Types)
**Previous**: BCT and Intersection Reduction (COMPLETED 2026-02-04)

## Session Redefinition (2026-02-04)

### COMPLETED WORK: BCT and Intersection Reduction

**Previous session accomplishments** (2026-02-04):
1. ✅ **Intersection Reduction** - Recursive evaluation for meta-types
2. ✅ **BCT for Intersections** - Extract common members from intersection types
3. ✅ **Lazy Type Support** - BCT works with Lazy(DefId) classes
4. ✅ **Literal Widening** - Array literals like [1, 2] infer as number[]
5. ✅ **Intersection Sorting Fix** - Preserve callable order for overload resolution

**Test Results**: All 18 BCT tests pass, no regressions

---

## NEW FOCUS: Advanced Type Evaluation

### Problem Statement

The Solver (the "WHAT" of type checking) lacks two of TypeScript's most powerful features:
1. **Conditional Types** - `T extends U ? X : Y`
2. **Mapped Types** - `{ [K in keyof T]: U }`

These features are core to modern TypeScript's type system. Without them:
- Standard library types like `Exclude`, `Extract`, `Partial`, `Required` don't work
- Complex narrowing scenarios fail (narrowing often involves conditional types)
- Conformance tests remain blocked (many use conditional/mapped types)

### Why This Matters Now

1. **North Star Alignment**: Solver is the "central type computation engine"
2. **Unblocks tsz-3 (CFA)**: Complex narrowing requires conditional type evaluation
3. **Unblocks tsz-4 (Emit)**: TypePrinter needs evaluated types for .d.ts files
4. **High Impact**: Biggest boost to conformance pass rate

---

## Implementation Plan

### Phase 1: Nominal Subtyping Implementation (HIGH PRIORITY)

#### Task 1: Add Visibility Enum and ParentId to PropertyInfo ✅ COMPLETED (2026-02-04)
**Status**: COMPLETE

**Commits**:
- `d0b548766`: feat(solver): add Visibility enum and parent_id to PropertyInfo
- `dfc2ea9e4`: fix: remove visibility/parent_id from FunctionShape and CallSignature

**What Was Implemented**:
1. Added `PropertyVisibility` enum (Public, Private, Protected) to `src/solver/types.rs`
2. Added `visibility: Visibility` field to `PropertyInfo`
3. Added `parent_id: Option<SymbolId>` field to `PropertyInfo`
4. Updated all 50+ PropertyInfo construction sites across the codebase
5. All properties default to `Visibility::Public` with `parent_id: None`

**Next Steps**: Tasks 2-4 will implement the logic to use these fields

#### Task 2: Update Lowering Logic to Populate Visibility 🔄 IN PROGRESS (50% Complete)
**Files**: `src/solver/lower.rs`, `src/checker/type_checking_queries.rs`, `src/checker/class_type.rs`

**Status**: Partially Complete

**Completed (2026-02-04)**:
- ✅ Added `get_visibility_from_modifiers()` to `src/checker/type_checking_queries.rs`
- ✅ Added `get_visibility_from_modifiers()` to `src/solver/lower.rs`
- ✅ Updated `lower_type_element()` in lower.rs to use visibility (type literals only)

**Remaining Work**:
- ⏳ Update 8 PropertyInfo construction sites in `src/checker/class_type.rs`:
  - Lines: 199 (properties), 323 (constructor params), 398, 431, 452 (private brand), 1084, 1236, 1269
- ⏳ For each PropertyInfo construction:
  1. Call `self.get_visibility_from_modifiers(&member.modifiers)`
  2. Set `parent_id: current_sym` (class symbol, available at line 101)
  3. Keep `visibility: Visibility::Public` for private brand (line 452)

**Pattern to Apply**:
```rust
// Before:
PropertyInfo {
    name,
    type_id,
    write_type: type_id,
    optional,
    readonly,
    is_method: false,
    visibility: Visibility::Public,  // Replace this
    parent_id: None,                 // Replace this
}

// After:
let visibility = self.get_visibility_from_modifiers(&prop.modifiers);
PropertyInfo {
    name,
    type_id,
    write_type: type_id,
    optional,
    readonly,
    is_method: false,
    visibility,
    parent_id: current_sym,
}
```

**Commits**:
- `ec7a3e06b`: feat(solver): add visibility detection helpers for nominal subtyping

#### Task 3: Implement Property Compatibility Checking
**File**: `src/solver/subtype_rules/objects.rs`

**Function**: `check_property_compatibility()`

**Requirements**:
1. If `target.visibility` is `Private` or `Protected`:
   - Source MUST have matching `parent_id` (nominal check)
   - Return `SubtypeResult::False` if mismatch
2. If `target.visibility` is `Public`:
   - Standard structural compatibility
3. Handle `any` propagation (Lawyer layer handles this)

#### Task 4: Handle Inheritance and Overriding
**File**: `src/solver/class_hierarchy.rs`

**Function**: `merge_properties()`

**Requirements**:
1. When derived class overrides base property:
   - Update `parent_id` to derived class symbol
   - Preserve visibility from base (private stays private)
2. Handle protected member inheritance correctly

---

### Phase 2: Conditional Types (LOWER PRIORITY)

#### Task 5: Conditional Type Evaluation ✅ ALREADY IMPLEMENTED
**File**: `src/solver/evaluate_rules/conditional.rs`

**Discovery (2026-02-04)**:
Conditional type evaluation is **ALREADY FULLY IMPLEMENTED** in `evaluate_rules/conditional.rs`!

**What's Already Implemented**:
1. **Basic evaluation**: `T extends U ? X : Y` ✅
2. **Distributive behavior**: `(A | B) extends U ? X : Y` ✅
3. **Recursion protection**: Tail-recursion elimination up to 1000 iterations ✅
4. **`infer` keyword**: Full support with type substitution ✅
5. **`any` handling**: Returns union of both branches ✅
6. **Type parameter substitution**: Via `TypeSubstitution` ✅

**Key Features**:
- Tail-recursion elimination for patterns like `type Loop<T> = T extends [...infer R] ? Loop<R> : never`
- `infer` type inference with constraints
- Proper handling of distributive vs non-distributive conditionals
- Deferred evaluation for unresolved type parameters

**Status**: Task 2 COMPLETE (already implemented)

#### Task 3: Test and Verify Conditional Types
**File**: `src/solver/infer.rs`

**Function to implement**: `infer_from_conditional_type()`

**Requirements**:
1. Pattern matching: `T extends (infer U)[] ? U : never`
2. Extract type variables from extends clause
3. Return inferred type or constraint
4. Handle multiple `infer` declarations

**Examples**:
```typescript
type UnpackedArray<T> = T extends (infer U)[] ? U : T;
type T0 = UnpackedArray<number[]>; // number
type T1 = UnpackedArray<string>; // string
```

---

### Phase 2: Mapped Types (LOWER PRIORITY)

#### Task 4: Implement Mapped Type Evaluation
**File**: `src/solver/evaluate.rs`

**Function to implement**: `evaluate_mapped()`

**Requirements**:
1. **Key iteration**: Iterate over `keyof T`
2. **Property mapping**: Apply type transformation to each property
3. **Modifier handling**:
   - `?` optional modifier
   - `readonly` modifier
   - `-?` and `-readonly` removal modifiers

**Examples**:
```typescript
type Partial<T> = { [P in keyof T]?: T[P] };
type Readonly<T> = { readonly [P in keyof T]: T[P] };
```

---

## Success Criteria

1. **Conditional Types**:
   - [ ] Basic `T extends U ? X : Y` works
   - [ ] Distributive over unions works
   - [ ] `infer` keyword works for simple cases
   - [ ] Standard library types (`Exclude`, `Extract`) work

2. **Mapped Types** (Phase 2):
   - [ ] Basic `[K in keyof T]: U` works
   - [ ] Modifiers (`?`, `readonly`) work
   - [ ] Modifier removal (`-?`, `-readonly`) works

3. **Conformance**:
   - [ ] Significant improvement in pass rate
   - [ ] No regressions in existing tests

---

## MANDATORY: Two-Question Rule

⚠️ **Before implementing**, use `tsz-gemini` (Question 1):
```bash
./scripts/ask-gemini.mjs --include=src/solver "I need to implement Conditional Types (T extends U ? X : Y).
1) Where should evaluation logic live (evaluate.rs)?
2) How to handle distributive behavior over unions?
3) How to prevent infinite recursion in nested conditionals?
4) What about 'infer' keyword - separate task or same implementation?"
```

⚠️ **After implementing**, use `tsz-gemini --pro` (Question 2):
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver "I implemented conditional type evaluation.
[PASTE CODE]
Please review: 1) Is this correct for TypeScript? 2) Did I miss edge cases?
3) Are there type system bugs? Be specific."
```

---

## Session History

- 2026-02-04: Started as "Intersection Reduction and Advanced Type Operations"
- 2026-02-04: **COMPLETED** BCT, Intersection Reduction, Literal Widening
- 2026-02-04: **FIXED** Intersection sorting bug (preserve callable order)
- 2026-02-04: **REDEFINED** to "Advanced Type Evaluation & Inference"
- 2026-02-04: New focus - Conditional Types and Mapped Types

---

## Completed Commits (History)

- `7bf0f0fc6`: Intersection Reduction (evaluate_intersection, evaluate_union)
- `7dfee5155`: BCT for Intersections + Lazy Support
- `c3d5d36d0`: Literal Widening for BCT
- `f84d65411`: Fix intersection sorting - preserve callable order

---

## Complexity: HIGH

**Why High**:
- Conditional types are the most complex feature in TypeScript's type system
- `infer` keyword requires pattern matching and unification
- Distributive behavior requires careful union handling
- Infinite recursion is a real risk

**Risk**: Changes to evaluation logic can cause regressions across all type operations.

**Mitigation**: Follow Two-Question Rule strictly. All changes must be reviewed by Gemini Pro.
