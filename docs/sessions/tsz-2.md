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

### Phase 1: Conditional Types (HIGH PRIORITY)

#### Task 1: Verify Nominal Subtyping First
**File**: `src/solver/subtype.rs`

**Known Issue from Previous Session**:
> "is_subtype_of treats ObjectWithIndex types structurally instead of nominally"
> Example: `is_subtype_of(Cat, Dog)` returns `true` (WRONG!)

**Action**: Verify `is_subtype_of` correctly uses `check_nominal_inheritance` for class-based types.

**Test**:
```typescript
class A { private x: string; }
class B { private x: string; }
const a: A = new B(); // Should error: Type 'B' is not assignable to type 'A'
```

#### Task 2: Implement Conditional Type Evaluation
**File**: `src/solver/evaluate.rs`

**Function to implement**: `evaluate_conditional()`

**Requirements**:
1. **Basic evaluation**: `T extends U ? X : Y`
   - Check subtype: `is_subtype_of(T, U)`
   - Return `X` if true, `Y` if false

2. **Distributive behavior**:
   - `(A | B) extends U ? X : Y` → `(A extends U ? X : Y) | (B extends U ? X : Y)`
   - Distribute over unions in the `extends` clause

3. **Recursion protection**:
   - Detect infinite recursion in nested conditionals
   - Use cycle detection similar to other evaluation functions

**Edge Cases**:
- `any`/`unknown` special handling
- Lazy type resolution in extends clause
- Type parameter substitution

#### Task 3: Implement `infer` Keyword
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
