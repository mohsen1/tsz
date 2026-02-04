# Session tsz-2: Type Metaprogramming & Solver Completion

**Started**: 2026-02-04
**Status**: 🟡 Investigation Complete (TypeEnvironment Registration Issue)
**Previous**: Nominal Subtyping (COMPLETED 2026-02-04)

## Session Redefinition #3 (2026-02-04)

### COMPLETED WORK: Nominal Subtyping ✅

**Phase 1 Complete** (2026-02-04):
1. ✅ **Task 1**: Visibility enum and parent_id added to PropertyInfo
2. ✅ **Task 2**: Lowering logic populates visibility from modifiers
3. ✅ **Task 3**: Property compatibility checking with nominal subtyping
4. ✅ **Task 4**: Inheritance and overriding with parent_id tracking

All 4 tasks complete. TypeScript classes now have proper nominal subtyping for private/protected members.

---

## NEW FOCUS: Type Metaprogramming Triad

### Problem Statement

The Solver (the "WHAT" of type checking) is missing critical type metaprogramming features required by the TypeScript Standard Library:
1. **Mapped Types** - `{ [K in keyof T]: U }` (MISSING - HIGH PRIORITY)
2. **Template Literal Types** - `` `${T}` `` (MISSING)
3. **Conditional Type Inference** - `infer` keyword verification (NEEDS TESTING)

Without these features:
- Standard library types like `Partial`, `Required`, `Pick`, `Omit` don't work
- Template literal type manipulation fails (core to modern TypeScript)
- Conformance tests for conditional/mapped types remain blocked

### Why This Matters Now

1. **North Star Alignment**: Solver should be feature-complete for type computations
2. **Unblocks Standard Library**: Cannot process `lib.d.ts` without mapped types
3. **High Conformance Impact**: Biggest boost to pass rate
4. **Solver-First Architecture**: Complete Solver before adding more Checker complexity

---

## Implementation Plan

### Phase 2: Type Metaprogramming Triad

#### Task 1: Verify & Stress-Test Conditional Type Inference
**File**: `src/solver/infer.rs`

**Function**: `infer_from_conditional_type()` (verify implementation)

**Requirements**:
1. Verify `infer` keyword works for pattern matching: `T extends (infer U)[] ? U : never`
2. Multiple `infer` declarations for same type variable
3. `infer` in function parameters vs. return types
4. Nested conditionals
5. Distributive behavior with union types

**Success Criteria**:
- All conditional type inference tests pass
- Standard library types like `Extract`, `Exclude` work correctly

---

#### Task 2: Implement Mapped Type Evaluation (HIGH PRIORITY)
**File**: `src/solver/evaluate_rules/mapped.rs` (create new file) or `src/solver/evaluate.rs`

**Function**: `evaluate_mapped()`

**Requirements**:
1. **Key iteration**: Iterate over `keyof T` (constraint type)
2. **Property mapping**: Apply type transformation to each property
3. **Modifier handling**:
   - `?` optional modifier (add)
   - `readonly` modifier (add)
   - `-?` optional removal
   - `-readonly` readonly removal
4. **Homomorphic mapped types**: Preserve structure when mapping over identity
5. **Template literal keys**: Handle `[K in keyof T as `${Prefix}${K}`]`

**Examples**:
```typescript
type Partial<T> = { [P in keyof T]?: T[P] };
type Readonly<T> = { readonly [P in keyof T]: T[P] };
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
```

**Key Decision**: Create separate file or add to evaluate.rs?
- Create `evaluate_rules/mapped.rs` if complex (>200 lines)
- Add to `evaluate.rs` if simple (<200 lines)

---

#### Task 3: Template Literal Types (LOWER PRIORITY)
**File**: `src/solver/types.rs` (TypeKey variant) and `src/solver/evaluate_rules/template.rs`

**Requirements**:
1. String concatenation: `` `${A}${B}` ``
2. Type inference within templates
3. Union types in templates: `` `${A | B}` ``
4. Template literal type constraints

**Note**: Can defer this if conformance push is higher priority.

---

#### Task 4: Conformance Push
**Goal**: Use new Solver capabilities to unblock test suites

**Actions**:
1. Run `./scripts/conformance.sh --filter=conditional`
2. Run `./scripts/conformance.sh --filter=mapped`
3. Fix failures with Solver improvements
4. Measure conformance pass rate improvement

---

## Investigation: TypeEnvironment Registration Issue (2026-02-04)

### Problem Discovery
Conformance audit revealed mapped types at 32.1% pass rate (18/56 tests).

**Root Cause**: Type aliases from lib.d.ts (like `Partial<T>`) are not being properly registered in TypeEnvironment, causing mapped type evaluation to fail.

**Trace Evidence**: `Partial<Foo>` resolves to `TypeId(3)` (Unknown) instead of the mapped type structure.

### Investigation Process

1. **Verified evaluate_mapped exists** - Found 442-line implementation in `src/solver/evaluate_rules/mapped.rs`
2. **Traced test failure** - Found that `resolve_lazy()` returns `None` for type alias DefIds
3. **Identified missing link** - Type alias bodies not stored in DefinitionInfo.body field

### Implementation Attempt

**Changes Made** to `src/checker/state_type_analysis.rs`:
1. Added `definition_store.set_body(def_id, alias_type)` after computing type alias body
2. Added Lazy type return for recursive type aliases to prevent infinite recursion

**Result**: No conformance improvement - still 32.1% pass rate

**Gemini Pro Analysis**:
- Issue is deeper than expected - possibly in circular dependency handling
- Type alias lowering pipeline requires comprehensive understanding
- Multiple code paths may be overwriting or clearing the body

### Technical Details Discovered

**Correct Registration Sequence** (from Gemini):
1. Create DefId: `get_or_create_def_id(sym_id)`
2. Register type params: `insert_def_type_params(def_id, params)`
3. Store body: `definition_store.set_body(def_id, alias_type)`
4. Return Lazy type for recursive aliases
5. Register in TypeEnvironment: `insert_def_with_params(def_id, result, params)`

**Key Files**:
- `src/checker/state_type_analysis.rs` - `compute_type_of_symbol` (line 1336)
- `src/solver/lower.rs` - TypeLowering bridge
- `src/solver/db.rs` - TypeResolver implementation
- `src/solver/subtype.rs` - TypeEnvironment (line 357: `insert_def_with_params`)

### Status
**Investigation complete** but **fix insufficient**. This is a **deep architectural issue** requiring extensive archaeology of:
- Binder → Checker → Solver data flow
- Type alias lowering pipeline integration
- Lazy type evaluation chain
- Circular dependency resolution

**Recommendation**: This issue is well-documented but requires significant investment to resolve. Consider session priorities before continuing.

---

## MANDATORY: Two-Question Rule

⚠️ **Before implementing Task 2 (Mapped Types), ask Gemini:**
```bash
./scripts/ask-gemini.mjs --include=src/solver "I am starting Task 2: Mapped Type Evaluation.
Problem: Need to implement { [K in keyof T]: U } with modifier handling.
Planned Approach:
1. Create evaluate_mapped in src/solver/evaluate.rs
2. Iterate over keys of constraint
3. Apply TypeSubstitution to value type U
4. Handle Optional/Readonly flags

Questions:
1. Should I create separate file in evaluate_rules/?
2. How does Solver handle homomorphic mapped type optimization?
3. Are there pitfalls with recursive mapped types?"
```

⚠️ **After implementing, ask Gemini Pro for review.**

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

#### Task 2: Update Lowering Logic to Populate Visibility ✅ COMPLETED (2026-02-04)
**Files**: `src/solver/lower.rs`, `src/checker/type_checking_queries.rs`, `src/checker/class_type.rs`

**Status**: COMPLETE

**Commits**:
- `ec7a3e06b`: feat(solver): add visibility detection helpers for nominal subtyping
- `43fd74dbf`: feat(solver): complete Task 2 - populate visibility for all class members

**What Was Implemented**:

1. **Helper Functions Added**:
   - `get_visibility_from_modifiers()` in `src/checker/type_checking_queries.rs`
   - `get_visibility_from_modifiers()` in `src/solver/lower.rs`

2. **Refactored Aggregates** (Gemini's recommended approach):
   - Added `visibility: Visibility` field to `MethodAggregate`
   - Added `visibility: Visibility` field to `AccessorAggregate`
   - Updated in both `get_class_instance_type_inner` and `get_class_constructor_type_inner`
   - All members of a class share the same `parent_id` (class symbol)

3. **Updated All PropertyInfo Sites**:
   - ✅ Class properties: Uses `get_visibility_from_modifiers(&prop.modifiers)`
   - ✅ Constructor parameters: Uses `get_visibility_from_modifiers(&param.modifiers)`
   - ✅ Accessor-based properties: Uses stored `accessor.visibility`
   - ✅ Method-based properties: Uses stored `method.visibility`
   - ✅ All use `parent_id: current_sym` (class symbol)

4. **Result**: All class members now have proper visibility and parent_id populated from AST modifiers

**Next**: Task 3 - Implement nominal compatibility checking in subtype_rules/objects.rs

#### Task 3: Implement Property Compatibility Checking ✅ COMPLETED (2026-02-04)
**File**: `src/solver/subtype_rules/objects.rs`

**Status**: COMPLETE

**Commits**:
- `ac1e4432f`: feat(solver): implement nominal subtyping for private/protected properties

**What Was Implemented**:

1. **`check_property_compatibility` function**:
   - Added nominal check: non-public target requires matching `parent_id`
   - Added visibility guard: public target cannot be satisfied by private/protected source
   - This prevents "private slot leakage" - private members can't satisfy public requirements

2. **`check_object_with_index_to_object` function**:
   - Added same visibility guards as check_property_compatibility
   - Ensures nominal checking works for index signatures too

3. **`check_missing_property_against_index_signatures` function**:
   - Added guard: index signatures cannot satisfy private/protected properties
   - This is correct because index signatures are always public

4. **`check_object_subtype` function**:
   - Added check: missing private/protected properties are rejected, even if optional
   - Private/protected properties must be nominally present in the source

**Critical Bugs Found and Fixed by Gemini Pro**:
1. **Bug 1**: Original code allowed assigning private source to public target (TypeScript forbids this)
   - Fixed by adding `else if source.visibility != Visibility::Public` check
2. **Bug 2**: Original code allowed missing private optional properties (TypeScript requires them)
   - Fixed by checking visibility before optional property logic

**Examples of Correct Behavior**:
```typescript
// ✅ Correct: Private requires same declaration
class A { private x = 1; }
class B { private x = 1; }
const a: A = new B(); // ERROR: different private declarations

// ✅ Correct: Cannot assign private to public
class C { private x = 1; }
interface I { x: number; }
const c: C = new C();
const i: I = c; // ERROR: property 'x' is private in type 'C' but not in type 'I'
```

**Next**: Task 4 - Handle Inheritance and Overriding in class_hierarchy.rs

#### Task 4: Handle Inheritance and Overriding ✅ COMPLETED (2026-02-04)
**File**: `src/solver/class_hierarchy.rs`

**Status**: COMPLETE

**Commits**:
- `8bb483b73`: feat(solver): implement visibility-aware inheritance in class_hierarchy

**What Was Implemented**:

1. **`merge_properties` function**:
   - Added `current_class: SymbolId` parameter for parent_id tracking
   - ALL base properties (including private) are inherited
   - Private members are inherited but not accessible (Checker enforces access control)
   - `parent_id` updated to `current_class` for all own/overriding members
   - Updated test to use new function signature

2. **Key Insight from Gemini Pro**:
   - Initial implementation filtered out private base properties
   - **Critical Bug**: In TypeScript, private members ARE inherited (just not accessible)
   - This is required for subtyping: `class B extends A` must structurally contain all of A's properties for `B <: A` assignability
   - Filtering out private members would break: `let a: A = new B()`

3. **Correct Behavior**:
   ```typescript
   class A { private x = 1; }
   class B extends A {}  // B inherits private x (but can't access it)
   const a: A = new B(); // ✅ OK: B is subtype of A (has x nominally)
   ```

**Phase 1 Complete**: All four tasks for nominal subtyping implementation are now complete!

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
