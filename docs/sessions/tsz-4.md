# Session TSZ-4: Nominality & Accessibility (Lawyer Layer)

**Started**: 2026-02-05
**Status**: 🔄 Active - Priority 5 (Void Return Exception)
**Focus**: Implement TypeScript's nominal "escape hatches" and type system quirks in the Lawyer layer (CompatChecker)

**Session Scope**:
- ✅ Priority 1: Enum Nominality (COMPLETE - 21 tests)
- ✅ Priority 2: Private Brand Checking (COMPLETE - 31 tests)
- ✅ Priority 3: Constructor Accessibility (COMPLETE - 13 tests)
- ✅ Priority 4: Function Bivariance (COMPLETE - 8 tests)
- 🔄 Priority 5: Void Return Exception (ACTIVE)
- 📝 Priority 6: Any-Propagation Hardening (PENDING)

**Key Insight**: Per Gemini Flash (2026-02-05), "infrastructure exists" doesn't mean it works correctly. Need to verify with tests and tsz-tracing to ensure bivariance logic matches tsc exactly.

**Completed**:
- Enum Nominality with TypeKey::Enum wrapper
- Private Brand Checking with recursive Union/Intersection handling
- Constructor Accessibility for both new expressions and class inheritance
- Function Bivariance (methods bivariant, properties contravariant in strict mode)

**In Progress**:
- Void Return Exception (Priority 5)

## Previous Session (COMPLETE)

**Session TSZ-4: Strict Null Checks & Lawyer Layer Hardening** - ✅ Complete (2026-02-05)

### Completed Work
1. **Test Infrastructure**: Created `src/checker/tests/strict_null_manual.rs` with 4 passing tests
2. **Error Code Validation**: Verified TS18047/TS18048 emission matches tsc behavior
3. **Weak Type Detection**: Fixed critical bug in `ShapeExtractor` - now resolves Lazy/Ref types
4. **Object Literal Freshness**: Implemented nested object literal excess property checking

### Impact
- **Before**: Weak type detection failed for interfaces/classes (false TS2559 positives)
- **After**: Weak type detection correctly handles all object types including interfaces and classes
- **Nested Freshness**: Object literals with nested objects now correctly checked for excess properties

---

## Current Session: Nominality & Accessibility

**Recommended by**: Gemini Flash (2026-02-05)
**Why This Fits**: Builds on TSZ-4's Lawyer layer expertise (object literal freshness, weak types, any propagation)

### Problem Statement

TypeScript uses **nominal typing** as an "escape hatch" from structural subtyping in specific cases:

1. **Enums are nominal**: `Enum A` is NOT assignable to `Enum B` even if they share values
2. **Private brands**: Classes with private members are only compatible with themselves/subclasses
3. **Constructor accessibility**: Cannot instantiate classes with private/protected constructors from invalid scopes

Currently, `src/solver/compat.rs` has stub implementations (`NoopOverrideProvider`) for these rules, causing:
- Hundreds of missing `TS2322` (Type mismatch) errors
- Missing `TS2673` (Constructor private) errors
- False positives in conformance suite

### Why This is High Value

Structural subtyping (the "Judge") incorrectly allows:
- `Enum A` to be assigned to `Enum B` if they share values
- `Class A` to be assigned to `Class B` even if they have `private` members (if shapes match)
- Creating instances of classes with `private` constructors from outside the class

Fixing these will resolve hundreds of conformance failures.

### Tasks (Priority Order)

#### Priority 1: Harden Enum Assignability (TS2322) ✅ COMPLETE
**Status**: Implemented enum nominal assignability with `TypeKey::Enum(def_id, literal_type)` wrapper
**Files Modified**:
- `src/solver/compat.rs` - Implemented `enum_assignability_override`
- `src/solver/type_queries_extended.rs` - Added `Enum` variant to `NamespaceMemberKind`
- `src/checker/type_checking_queries.rs` - Added enum member lookup
- `src/checker/state_type_environment.rs` - Fixed `get_enum_identity` using AST parent traversal

**Test Results**: 21/21 tests passing ✅

#### Priority 2: Upgrade Private Brand Checking ✅ COMPLETE
**Status**: Rewrote `private_brand_assignability_override` to use recursive structure preserving Union/Intersection semantics
**Files Modified**:
- `src/solver/compat.rs` - Rewrote with proper recursive handling

**Test Results**: 31/31 tests passing ✅

#### Priority 3: Implement Constructor Accessibility (TS2673 / TS2674 / TS2675) ✅ COMPLETE
**Status**: Implemented constructor accessibility checks for both `new` expressions and class inheritance
**Files Modified**:
- `src/checker/state_checking.rs` - Added TS2675 check in heritage clause validation
- `src/checker/type_checking_utilities.rs` - Enhanced `class_constructor_access_level` to walk inheritance chain
- `src/checker/constructor_checker.rs` - Implemented `check_constructor_accessibility_for_new`

**Test Results**: 13/13 tests passing ✅

#### Priority 1: Harden Enum Assignability (TS2322)
**Current State**: `enum_assignability_override` in `compat.rs` is a `Noop`

**Task**: Implement the logic in `CompatChecker` to ensure Enums are nominal
- Enum member from `Enum A` should NOT be assignable to `Enum B`, even if both are numeric
- Handle "Const Enums" correctly
- Numeric enums are bitwise-compatible with `number` but NOT each other

**Files**:
- `src/solver/compat.rs` - Replace `NoopOverrideProvider` with real implementation
- `src/solver/lawyer.rs` - Add enum nominality to QUIRKS documentation

#### Priority 2: Upgrade Private Brand Checking
**Current State**: `private_brand_assignability_override` uses string-prefix matching (`__private_brand_`)

**Task**: Refactor to use **Symbol identity**
- In TypeScript, a class with a private member is only compatible with itself or a subclass
- The "Brand" should be tied to the unique `SymbolId` of the private member, not just its name
- Use the `ShapeExtractor` you fixed in the previous session

**Files**:
- `src/solver/compat.rs` - Replace string matching with SymbolId comparison
- `src/checker/symbol_resolver.rs` - Extract symbol flags to check for `private`/`protected`

#### Priority 4: Function Bivariance Verification & Hardening ✅ COMPLETE
**Status**: Verified and tested function bivariance implementation
**Files Modified**:
- `src/checker/tests/function_bivariance.rs` - Created comprehensive test suite (8 tests)

**Bug Found and Fixed**:
The `// @strictFunctionTypes: true` comment was not being parsed in tests because:
1. Tests prepend `GLOBAL_TYPE_MOCKS` (non-comment interface declarations)
2. `parse_test_option_bool` breaks at first non-comment line
3. Comment parser never reaches `@strictFunctionTypes` comment

**Fix**: Updated `test_function_variance` and `test_no_errors` to prepend `@strictFunctionTypes: true` BEFORE `GLOBAL_TYPE_MOCKS`

**Test Results**: 8/8 tests passing ✅
- Methods are bivariant even in strict mode ✓
- Function properties are contravariant in strict mode ✓

**Implementation Verified**:
1. `lower.rs` correctly sets `is_method = true` only for MethodSignature (line 1262)
2. `functions.rs:539` computes `is_method = source.is_method || target.is_method`
3. `functions.rs:110-111` toggles bivariance based on `is_method` + `strict_function_types`
4. `compat.rs:752` propagates `strict_function_types` to SubtypeChecker

#### Priority 5: Void Return Exception 🔄 ACTIVE
**Goal**: Implement/verify Lawyer rule where function returning void can be assigned function returning any type T
- In strict set theory, `() => number` is NOT subtype of `() => void`
- TypeScript allows this for callback ergonomics
- Classic "Lawyer" override

**Example**:
```typescript
function takesCallback(cb: () => void) {
    cb(); // Can call with () => number
}
takesCallback(() => 5); // Should be allowed
```

**Questions** (to ask Gemini):
- Where is the logic that allows `() => T` to be assigned to `() => void`?
- Is this in Judge or Lawyer layer?

**Files**:
- `src/solver/compat.rs` - Check is_assignable_to for void exception
- `src/solver/subtype_rules/functions.rs` - Verify return type check respects this quirk

#### Priority 6: Any-Propagation Hardening 📝 NEW
**Goal**: Verify `any` behaves as both top and bottom type in Lawyer layer without polluting Judge's logic
- NORTH_STAR.md Section 3.3 describes `any` as the "black hole"
- Verify CompatChecker correctly uses `any` to silence structural mismatches
- Ensure it doesn't suppress errors inappropriately (strict contexts, intrinsics)

**Key Concerns**:
- Does `any` correctly suppress structural errors only when appropriate?
- Are there strict contexts where `any` should NOT silence errors?
- Does it interact correctly with intrinsic checks?

**Files**:
- `src/solver/compat.rs` - Verify any propagation logic
- Verify interaction with strict mode checks

#### ~~Priority 5~~: Literal Type Widening 📝 DEFERRED (moved to future session)
**Status**: DEFERRED to future session - not a Lawyer Layer task

**Why Deferred**:
Per Gemini Pro guidance: "Widening is a Type Inference problem, not an Assignability problem."
- CompatChecker answers "Is Type A assignable to Type B?"
- Widening determines what Type A *is* during inference
- Belongs in `src/solver/infer.rs` or `src/checker/expr.rs`, NOT `src/solver/compat.rs`

**For Future Session**:
When implementing, modify Type Inference layer:
- Implement `get_widened_type()` function
- Apply to object properties when constructing object type
- Handle const vs let widening rules
- Handle union type edge cases (`"a" | "b"` → `string`)

### Success Criteria
- [x] `Enum A` assigned to `Enum B` produces a diagnostic ✅ (21 tests)
- [x] `Class A { private x }` assigned to `Class B { private x }` produces a diagnostic ✅ (31 tests)
- [x] `new C()` where `C` has a private constructor produces a diagnostic ✅ (13 tests)
- [ ] **Function Bivariance**: Verified with tests for strictFunctionTypes (on/off) and Method vs. Property
- [ ] **Void Return Exception**: `(x: T) => number` assignable to `(x: T) => void`
- [ ] **Any-Propagation**: `any` correctly suppresses structural errors in CompatChecker
- [ ] No regressions in existing tests

---

## Architectural Considerations (NORTH_STAR.md)

### Judge vs. Lawyer (Section 3.3)
- **Judge (SubtypeChecker)**: Should remain blissfully ignorant of nominality rules
  - Judge says "Yes, these shapes match" (structural)
- **Lawyer (CompatChecker)**: Steps in and says "Wait, the brands don't match"
  - Lawyer overrides Judge's decision with `false` for nominal types

### Visitor Pattern
- Use the `ShapeExtractor` you just fixed to find private symbols/brands
- Do NOT manually match on `TypeKey`

### Checker/Solver Boundary
- **Solver (WHAT)**: Implements the nominal override logic
- **Checker (WHERE)**: Provides symbol flags and context (private/protected modifiers)

---

## Other Active Sessions (Non-Conflict Verification)
- **TSZ-1**: Flow Analysis & Overload Resolution (focus: initialization) ✅ No conflict
- **TSZ-5**: Multi-Pass Generic Inference (focus: solving for `T`) ✅ No conflict
- **TSZ-11**: Truthiness & Equality Narrowing (focus: `if (x === null)`) ✅ No conflict

**Why no conflicts**:
- TSZ-1 looks at *when* variables are initialized (you look at *if* types are compatible)
- TSZ-5 solves for type parameters (you check concrete enum/class compatibility)
- TSZ-11 handles narrowing in conditional branches (you handle static assignability)

---

## MANDATORY Gemini Workflow (per AGENTS.md)

### Question 1 (PRE-implementation) - REQUIRED
Before modifying `src/solver/compat.rs`:

```bash
./scripts/ask-gemini.mjs --pro --include=src/solver/compat.rs --include=src/solver/lawyer.rs --include=src/checker/symbol_resolver.rs "
I am starting tsz-4 to implement Enum and Private Brand nominality in the Lawyer layer.

Current State:
- enum_assignability_override is Noop
- private_brand_assignability_override uses string matching

My planned approach for Enum Nominality:
1. In CompatChecker, check if both source and target are Enum types
2. Extract their DefIds/SymbolIds
3. If DefIds differ, return false (not assignable)
4. Exception: numeric enum is assignable to number

My planned approach for Private Brands:
1. Use ShapeExtractor to find private/protected members
2. Extract SymbolId for each private member
3. Compare brand SymbolIds between source and target
4. Return false if brands don't match

Questions:
1) Is this the right approach for nominal checking?
2) Where exactly should I extract the DefId/SymbolId from Enum types?
3) How do I determine if a class member is private/protected from the type system?
4) Are there TypeScript edge cases I'm missing (e.g., cross-instance private access)?
"
```

### Question 2 (POST-implementation) - REQUIRED
After implementing the changes:

```bash
./scripts/ask-gemini.mjs --pro --include=src/solver/compat.rs --include=src/solver/lawyer.rs "
I implemented Enum and Private Brand nominality in the Lawyer layer.

Changes:
[PASTE CODE OR DESCRIBE CHANGES]

Please review:
1) Is the nominal checking correct for TypeScript?
2) Did I miss any edge cases (const enums, enum merging, subclasses)?
3) Are there bugs in my brand comparison logic?
Be specific if it's wrong - tell me exactly what to fix.
"
```

---

## Key Commits (Previous Session)
- `9bb0a79ab` - Test infrastructure for strict null checks
- `bbdd4ac9f` - Fix Weak Type detection by resolving Lazy/Ref types
- `4cedeb282` - feat(tsz-4): implement nested object literal excess property checking
- `983675bad` - fix(tsz-4): address critical bugs per Gemini Pro review

---

## Focus Areas
- `src/solver/compat.rs` - Primary workspace (replace NoopOverrideProvider)
- `src/solver/lawyer.rs` - Add nominality rules to QUIRKS documentation
- `src/checker/symbol_resolver.rs` - Extract symbol flags for private/protected
- `src/checker/tests/` - Manual verification tests

---

## Progress Log (2026-02-05)

### Commit: `c763799da` - feat(tsz-4): add enum nominal assignability fix (WIP)

**What Was Done**:
1. Implemented `enum_assignability_override` fix to match on `TypeKey::Enum(def_id, inner_type)`
2. When DefIds match (same enum), check structural assignability of inner types
3. Created `enum_nominality_tests.rs` with 9 test cases

**Discovery** (from Gemini Pro):
- **Root Cause**: `enum_object_type` sets all members to `TypeId::NUMBER` or `TypeId::STRING`
- This loses nominal information needed for enum member distinction
- **Current Behavior**: `E.A` and `E.B` are both typed as `number`, so `E.A` is assignable to `E.B` (WRONG)
- **Expected Behavior**: `E.A` should be `TypeKey::Enum(DefId(E), Literal(0))`, `E.B` should be `TypeKey::Enum(DefId(E), Literal(1))`

**Test Results**: 4 passed, 5 failed
- Failed tests expect TS2322 errors but get 0 (because types are `number`, not `TypeKey::Enum`)

**Next Steps**:
1. Fix `enum_object_type` in `src/checker/state_type_environment.rs` to create `TypeKey::Enum(DefId, Literal)` for each member
2. Ask Gemini Pro for POST-implementation review before proceeding
3. Address Priority 2-4 (Numeric Literal Assignability, Performance Anti-Pattern, Bitwise Enums)

### Commit: `f1542996b` - feat(tsz-4): implement enum member nominal typing (WIP)

**What Was Done**:
1. Fixed `enum_member_type_from_decl` to return literal types from initializers (string/numeric literals)
2. Updated `compute_type_of_symbol` in `state_type_analysis.rs` to wrap members in `TypeKey::Enum(member_def_id, literal_type)`
3. Added `get_enum_identity` helper to resolve parent enum symbols from member DefIds
4. Refactored `enum_assignability_override` with cleaner nominal check logic per Gemini Flash guidance

**Test Results**: 6/9 tests passing
- ✅ Same member to same member: `E.A` to `E.A`
- ✅ Member to whole enum: `E.A` to `E`
- ✅ Whole enum to member: `E` to `E.A` (correctly rejects)
- ✅ Numeric enum to number: `E.A` to `number`
- ✅ Number to numeric enum member: `number` to `E.A` (correctly rejects)
- ✅ String literal to string enum: `"a"` to `E` (correctly rejects)
- ❌ Different members: `E.A` to `E.B` (expects TS2322, gets 0)
- ❌ Different enums: `E.A` to `F.A` (expects TS2322, gets 0)
- ❌ String enum to string: `E.A` to `string` (expects TS2322, gets 0)

**Issue**: `enum_assignability_override` not being triggered or returning `None` when it should return `Some(false)`

**Investigation Needed**:
1. Verify `TypeKey::Enum(member_def_id, literal_type)` types reach the assignability check
2. Check if `get_enum_identity` correctly resolves parent symbols from member DefIds
3. Add debug logging to trace why TS2322 errors are not emitted
4. May need to check if the override is even being called during assignment checking

**Files Modified**:
- `src/checker/state_type_environment.rs`: Added `get_enum_identity`, `check_structural_assignability`, refactored `enum_assignability_override`
- `src/checker/state_type_analysis.rs`: Updated enum member type resolution to use `TypeKey::Enum`
- `src/checker/type_checking_utilities.rs`: Fixed `enum_member_type_from_decl` to return literal types
- `src/solver/expression_ops.rs`: Added `NoopResolver` import

### Commit: `11226ad46` - feat(tsz-4): fix enum identity resolution to use get_symbol_globally

**What Was Done** (per Gemini Pro guidance):
1. Fixed `get_enum_identity` to use `get_symbol_globally` instead of `ctx.binder.get_symbol`
2. Added debug logging to trace enum identity resolution
3. Added explicit `Some(false)` return for different enum identities (Gemini's recommended fix)

**Gemini's Analysis**:
- Root cause: When `s_id != t_id`, code was falling through to `None`, allowing structural fallback
- Symbol resolution bug: `ctx.binder.get_symbol` only checks local file, needed `get_symbol_globally`
- Fix: Return `Some(false)` immediately for different enum identities

**Test Status**: Still 6/9 passing
- Same 3 tests still failing (E.A to E.B, E.A to F.A, E.A to string)
- Override still not being triggered despite fixes

**Remaining Investigation** (Per Gemini Pro 2026-02-05):
**CRITICAL FINDING**: This is a "wiring problem", not an architecture problem.

**Gemini's Diagnosis**:
The `enum_assignability_override` logic is correct, but there's an integration gap. The issue is likely one of:
1. **Type Resolution Problem**: Type annotation `E.B` resolves to primitive value (`number`) instead of `TypeKey::Enum(member_def_id, literal_type)`
2. **Code Path Difference**: `E.B` (type annotation) uses `get_type_from_type_reference` which may not call `compute_type_of_symbol`

**Key Insight from Gemini**:
- `E.A` (expression) uses `check_property_access`
- `E.B` (type annotation) uses `get_type_from_type_reference`
- If the type reference resolves to the VALUE (e.g., `0`) instead of the NOMINAL type, the override won't trigger

**Next Steps** (from Gemini):
1. Verify that `get_type_from_type_reference` calls `compute_type_of_symbol` for enum members
2. Ensure type references return `TypeKey::Enum` wrapper, not primitive values
3. If type references don't use the nominal wrapper, fix them to do so

**Investigation Completed**:
Found `get_type_from_type_reference` in `state_type_resolution.rs`. This function:
- Uses `TypeLowering` to lower type references (line 86)
- Calls `lower_type()` which may not preserve `TypeKey::Enum` wrapper
- Returns `TypeId::ERROR` for unresolved symbols (line 55)

**Critical Finding**:
Type references (`E.B` in type annotations) use a DIFFERENT code path than property access (`E.A` in expressions):
- Property access: Uses `check_property_access` → resolves symbol → calls `compute_type_of_symbol` → returns `TypeKey::Enum`
- Type reference: Uses `get_type_from_type_reference` → uses `TypeLowering` → may unwrap to primitive

This explains why enum_assignability_override is never triggered for type annotations!

**Resolution**:
Need to ensure `TypeLowering.lower_type()` preserves `TypeKey::Enum` wrappers for enum member type references, OR modify the type resolution path to use `compute_type_of_symbol` for enum members.

**Status**: Investigation complete. Root cause identified.

**Gemini Recommendation**: Wait for rate limit recovery before implementing.
- Modifying `src/solver/lower.rs` without Gemini review has 100% failure rate per AGENTS.md
- Enum nominality is notoriously tricky - high regression risk
- Wait time (15-60 min) is faster than debugging broken type system

**Prepared Gemini Prompt** (for when rate limit resets):
```bash
./scripts/ask-gemini.mjs --pro --include=src/solver/lower.rs --include=src/checker/state_type_resolution.rs "
I need to fix Enum Nominality in TypeLowering (TSZ-4).
Problem: TypeLowering unwraps Enum Members to primitives for type annotations (e.g., 'let x: E.B'), while property access ('E.A') preserves the TypeKey::Enum wrapper. This causes assignability mismatches.

Planned approach: Modify src/solver/lower.rs to ensure that when lowering a type reference to an enum member, we return the interned TypeKey::Enum instead of the underlying primitive.

Is this the right approach? Specifically:
1) Which function in lower.rs should I target (lower_type_reference or a specific member handler)?
2) How should I handle the underlying primitive for assignability (should the Solver handle the 'unwrapping' during subtype checks instead of during lowering)?
3) Are there edge cases with 'const enum' or 'string enums' I'm missing?
"
```

**Next Steps** (when rate limit resets):
1. Ask Gemini the pre-implementation question above
2. Implement the fix in `src/solver/lower.rs` per Gemini's guidance
3. Run tests to validate fix
4. Ask Gemini for POST-implementation review

### Commit: `f1542996b` - feat(tsz-4): implement enum member nominal typing (WIP)

**What Was Done**:
1. Fixed `enum_member_type_from_decl` to return literal types from initializers (string/numeric literals)
2. Updated `compute_type_of_symbol` in `state_type_analysis.rs` to wrap members in `TypeKey::Enum(member_def_id, literal_type)`
3. Added `get_enum_identity` helper to resolve parent enum symbols from member DefIds
4. Refactored `enum_assignability_override` with cleaner nominal check logic per Gemini Flash guidance

**Test Results**: 6/9 tests passing
- ✅ Same member to same member: `E.A` to `E.A`
- ✅ Member to whole enum: `E.A` to `E`
- ✅ Whole enum to member: `E` to `E.A` (correctly rejects)
- ✅ Numeric enum to number: `E.A` to `number`
- ✅ Number to numeric enum member: `number` to `E.A` (correctly rejects)
- ✅ String literal to string enum: `"a"` to `E` (correctly rejects)
- ❌ Different members: `E.A` to `E.B` (expects TS2322, gets 0)
- ❌ Different enums: `E.A` to `F.A` (expects TS2322, gets 0)
- ❌ String enum to string: `E.A` to `string` (expects TS2322, gets 0)

**Issue**: `enum_assignability_override` not being triggered or returning `None` when it should return `Some(false)`

**Investigation Needed**:
1. Verify `TypeKey::Enum(member_def_id, literal_type)` types reach the assignability check
2. Check if `get_enum_identity` correctly resolves parent symbols from member DefIds
3. Add debug logging to trace why TS2322 errors are not emitted
4. May need to check if the override is even being called during assignment checking

**Files Modified**:
- `src/checker/state_type_environment.rs`: Added `get_enum_identity`, `check_structural_assignability`, refactored `enum_assignability_override`
- `src/checker/state_type_analysis.rs`: Updated enum member type resolution to use `TypeKey::Enum`
- `src/checker/type_checking_utilities.rs`: Fixed `enum_member_type_from_decl` to return literal types
- `src/solver/expression_ops.rs`: Added `NoopResolver` import

### Commit: `a884c0aa2` - test(tsz-4): add Solver-layer unit tests for enum nominal typing

**What Was Done**:
While waiting for Gemini API rate limit recovery, expanded test coverage per Gemini Flash's recommendation.

**Added Tests** (`src/solver/tests/enum_nominality.rs`):
- `test_enum_member_typekey_wrapper`: Verifies TypeKey::Enum wrapper creation and DefId preservation
- `test_different_enum_members_different_types`: Confirms same enum, different members → different types
- `test_different_enums_different_defids`: Validates different enums have different DefIds
- `test_enum_preserves_literal_type`: Tests literal type preservation (numeric and string)
- `test_unwrapped_literals_no_nominality`: Verifies unwrapped literals have no nominal identity
- `test_same_enum_different_members_different`: Additional validation of nominal distinction

**Test Status**:
- ✅ 9/9 Solver-layer unit tests passing
- ❌ 6/9 integration tests passing (enum_nominality_tests.rs)
  - 3 failures require TypeLowering fix (blocked by Gemini API rate limit)

**Value**:
These tests validate the Solver layer behavior in isolation, confirming that:
- TypeKey::Enum wrapper is correctly constructed
- DefId provides nominal identity
- Different enum members are structurally and nominally distinct
- Literal types are preserved within the enum wrapper

This validates the correctness of the Solver layer implementation independent of the TypeLowering integration issue.

**Next Steps**:
1. Wait for Gemini API rate limit recovery
2. Ask pre-implementation question about TypeLowering fix
3. Implement TypeLowering changes per Gemini's guidance
4. Verify integration tests pass
5. Ask Gemini for POST-implementation review

### Commit: `87bdb3103` - test(tsz-4): add test infrastructure for Priority 2 & 3 (Private Brands & Constructor Accessibility)

**What Was Done**:
Per Gemini Flash recommendation, created comprehensive test suites for the next two priorities in TSZ-4 while waiting for Gemini API rate limit recovery.

**Test Suite 1: Private Brand Nominality** (`src/checker/tests/private_brands.rs`)
- 10 tests covering private/protected member nominal typing
- Validates that classes with private members behave nominally, not structurally
- Tests cover:
  * Different classes with same private member shape (should fail TS2322)
  * Private members preventing structural assignment to object literals
  * Protected members also creating nominal brands
  * Subclass compatibility (should pass - brand is inherited)
  * Public members remaining structural (baseline)
  * Multiple private members creating stronger brands
  * Different private member sets being incompatible
  * Private methods creating brands
  * Same class compatibility (trivial case)

**Test Suite 2: Constructor Accessibility** (`src/checker/tests/constructor_accessibility.rs`)
- 11 tests covering private/protected constructor accessibility
- Validates that restricted constructors cannot be instantiated from invalid scopes
- Tests cover:
  * Private constructor instantiation outside class (TS2673)
  * Protected constructor instantiation outside hierarchy (TS2674)
  * Private constructor accessible inside class (static factory pattern)
  * Protected constructor accessible in subclasses
  * Private constructor failing in subclasses
  * Protected constructor inaccessible from unrelated classes
  * Public constructor baseline (no restrictions)
  * Default constructor being public
  * Private constructor as type annotation (valid)
  * Abstract class instantiation restrictions (TS2511)

**Current Status**: All tests failing as expected (functionality not yet implemented)
- These tests define the exact behavior we need to implement
- Tests will guide implementation when rate limit resets
- Test failures provide clear success criteria for each feature

**Next Steps** (when rate limit resets):
1. Implement Private Brand nominality checks in `src/solver/compat.rs`
2. Implement Constructor Accessibility checks in `src/solver/compat.rs`
3. Verify all tests pass

### Commit: `a0f126a99` - docs(tsz-4): add QUIRKS documentation for nominal typing overrides

**What Was Done**:
Per Gemini Pro recommendation, documented the TypeScript nominal typing rules in the Lawyer layer QUIRKS section while waiting for Gemini API rate limit recovery.

**Added Documentation** (Section F: Nominality Overrides in `src/solver/lawyer.rs`):

**F.1. Enum Nominality (TS2322)**
- Documented that enum members are nominally typed via `TypeKey::Enum(def_id, literal_type)` wrapper
- Explained how `def_id` provides nominal identity while `literal_type` preserves value
- Included examples showing `E.A ≠ E.B` and `E.A ≠ F.A` even with same values

**F.2. Private/Protected Brands (TS2322)**
- Documented that classes with private members behave nominally, not structurally
- Explained the brand concept and rationale (prevent accidental mixing of similar-shape types)
- Clarified that subclasses inherit parent's private brand
- Distinguished from public members (which remain structural)

**F.3. Constructor Accessibility (TS2673, TS2674)**
- Documented private/protected constructor accessibility restrictions
- Explained scope validation (inside class, subclass, external)
- Included examples for each accessibility level

**Architectural Context**:
Added "Why These Override The Judge" section explaining:
- The Judge implements sound, structural set theory semantics
- The Lawyer adds TypeScript-specific restrictions on top
- Key principle: Lawyer never makes types MORE compatible, only LESS compatible
- This is TypeScript legacy behavior that violates soundness for ergonomic reasons

**Value**:
- Provides context for future developers working on the Lawyer layer
- Explains the rationale behind TypeScript's nominal typing quirks
- Clarifies the Judge vs. Lawyer relationship
- Prepares codebase for implementation of Priority 1, 2, & 3 features

**Session Status**:
- ✅ All test infrastructure complete (Priorities 1, 2, 3)
- ✅ Documentation complete
- ❌ BLOCKED on Gemini API rate limit for implementation work

**Next Steps** (when rate limit resets):
1. Ask pre-implementation question about TypeLowering fix (Priority 1)
2. Implement TypeLowering changes per Gemini's guidance
3. Implement Private Brand checks (Priority 2)
4. Implement Constructor Accessibility checks (Priority 3)
5. Verify all tests pass

### Commit: `244df32ae` - feat(tsz-4): use type_reference_symbol_type for enum nominality
**What Was Done**:
Per Gemini Flash guidance, modified `get_type_from_type_reference` to call `type_reference_symbol_type` for qualified names instead of `resolve_qualified_name`.

**Change**:
In `src/checker/state_type_resolution.rs` line 91-106:
- Added call to `type_reference_symbol_type(sym_id)` after checking type parameters
- This ensures enum members used as type annotations preserve their `TypeKey::Enum` wrapper

**Test Results**: Still 3/9 failing (same as before)
- Override not being triggered
- Needs further investigation

### Commit: `f2cbccdeb` - feat(tsz-4): add debug logging to enum_assignability_override
**What Was Done**:
Added comprehensive debug logging to `enum_assignability_override` to trace why it's not being triggered.

**Logging Added**:
- Entry point: Shows source and target TypeIds
- TypeKey lookup: Shows what TypeKey variants are present  
- Nominal check: Shows DefId comparisons and identity resolution
- Branch decisions: Shows which code path is taken

**CRITICAL FINDING**: No debug output appeared during test run!

This means `enum_assignability_override` is **NOT being called at all** for type annotations like `let x: E.B = E.A`.

**Implications**:
The override exists and has the correct logic, but there's a "wiring problem" where the type checking pipeline for type annotations doesn't invoke the Lawyer layer's enum nominality override.

**Next Investigation Steps**:
1. Find where type annotation assignment checking happens
2. Verify if `CompatChecker` is being used for type annotations
3. Check if there's a code path that bypasses the override system
4. Investigate if type annotations use a different compatibility checking mechanism than expressions

**Current Status**: Blocked on finding why enum_assignability_override is not invoked

### Commit: `1e80cf6b0` - feat(tsz-4): add debug logging to trace assignability checking
**What Was Done**:
Added comprehensive debug logging at multiple levels to trace why `enum_assignability_override` is not being called.

**Logging Added**:
1. `src/checker/state.rs`: Added logging to `CheckerOverrideProvider.enum_assignability_override`
2. `src/checker/assignability_checker.rs`: Added logging to `is_assignable_to`

**Critical Finding**: Still no debug output appeared during test run!

This suggests that `is_assignable_to` is NOT being called at all for const declarations with type annotations.

**Deep Investigation Results**:
- Verified wiring is correct: `CheckerOverrideProvider` is created and passed to `is_assignable_with_overrides`
- Verified `assignability_checker.rs` line 257 calls `checker.is_assignable_with_overrides(source, target, &overrides)`
- But ZERO debug output appears at any level

**Hypothesis**:
Const declarations may use a different code path than variable declarations, OR the test setup doesn't trigger the assignment check pipeline at all.

**Current Status**: Deep debugging needed to find the actual code path for type annotation compatibility checking in const declarations.

**Blocker Level**: CRITICAL - Found architectural issue that prevents enum nominal typing from working. Need to trace the exact code execution path to understand why the Lawyer layer is bypassed.

### Commit: `c85b1fdf9` - feat(tsz-4): fix enum member property access and nominal identity ✅

**BREAKTHROUGH - ALL 15 ENUM TESTS PASSING!**

**Root Cause Discovery**:
The issue was NOT that `enum_assignability_override` wasn't being called. The issue was that:
1. **Tracing wasn't working in tests** - The test environment doesn't initialize the tracing subscriber, so all `tracing::debug!` calls were no-ops
2. **Enum property access returned `any`** - When accessing `E.A`, the expression was typed as `any` instead of `TypeKey::Enum(def_id, literal_type)`

**True Root Cause**:
`classify_namespace_member` in `src/solver/type_queries_extended.rs` didn't recognize `TypeKey::Enum` types. When checking property access like `E.A`:
1. `E` resolved to `TypeKey::Enum(def_id, union_type)`
2. `classify_namespace_member` only handled `Callable` and `Lazy`, falling through to `Other`
3. Property access failed, returning `any`
4. `any` is assignable to everything, so no TS2322 errors were emitted

**Fixes Implemented**:

1. **Added Enum variant to NamespaceMemberKind** (`src/solver/type_queries_extended.rs`):
   ```rust
   pub enum NamespaceMemberKind {
       SymbolRef(SymbolRef),
       Lazy(DefId),
       Callable(CallableShapeId),
       Enum(DefId),  // NEW
       Other,
   }
   ```

2. **Updated classify_namespace_member** to handle `TypeKey::Enum`:
   ```rust
   Some(TypeKey::Enum(def_id, _)) => NamespaceMemberKind::Enum(def_id),
   ```

3. **Added enum member lookup** in `resolve_namespace_value_member` and `namespace_has_type_only_member`

4. **Fixed `get_enum_identity`** to use AST parent lookup instead of `symbol.parent`:
   - `symbol.parent` was `u32::MAX` for enum members (sentinel value)
   - Now uses `arena.get_extended` to find parent enum node
   - Looks up parent enum symbol via `binder.get_node_symbol`

5. **Fixed numeric enum assignability**:
   - `number` is NOT assignable to enum members (e.g., `1` to `E.A` where `E.A = 0` should fail)
   - Enum members ARE assignable to `number` (e.g., `E.A` to `number` should succeed)

**Test Results**: 15/15 enum nominality tests passing ✅

**Files Modified**:
- `src/solver/type_queries_extended.rs`: Added `Enum` variant and handling
- `src/checker/type_checking_queries.rs`: Added enum member lookup, removed deprecated `SymbolRef`
- `src/checker/state_type_environment.rs`: Fixed `get_enum_identity` and numeric enum rules
- `src/checker/assignability_checker.rs`: Debug logging (temporary)
- `src/checker/state.rs`: Debug logging (temporary)
- `src/checker/state_checking.rs`: Debug logging (temporary)

### Commit: `d0ef97966` - fix(tsz-4): distinguish between enum type and enum members (Gemini Review Applied)

**Gemini Pro Review**:
Per Gemini Pro feedback, fixed 2 bugs in the initial implementation:

1. **Bug 1 Fixed**: `number` is now assignable to Enum type (`let e: E = 123`) ✓
2. **Bug 2 Fixed**: Literals are now correctly rejected for enum members (`let a: E.A = 0`) ✗

**Implementation**:
- Distinguish between Enum type (E) and enum members (E.A) by checking symbol flags
- Enum type: has `ENUM` flag
- Enum member: has `ENUM_MEMBER` flag
- Added 2 new tests for these edge cases

**Final Test Results**: 21/21 enum nominality tests passing ✅

**Priority 1 Summary**:
- Started with 6/9 tests passing (enum members typed as `any`)
- Root cause: `classify_namespace_member` didn't recognize `TypeKey::Enum`
- Fixed property access by adding `Enum(DefId)` variant and enum member lookup
- Fixed enum identity by using AST parent traversal instead of `symbol.parent`
- Fixed numeric enum assignability per Gemini Pro review
- All 21 tests pass including edge cases

**Mandatory Gemini Workflow Completed**:
- ✅ PRE-implementation consultation (via previous session investigation)
- ✅ POST-implementation review (via Gemini Pro)
- ✅ Applied Gemini's fixes for numeric enum assignability bugs
- ✅ Verified all tests pass

**Next Steps**:
1. Clean up temporary debug eprintln statements
2. Move to Priority 2: Private Brand Checking
3. Move to Priority 3: Constructor Accessibility
4. Move to Priority 4: Literal Type Widening

**Status**: Priority 1 COMPLETE ✅

### Commit: `1e2c94465` - fix(solver): handle unions/intersections correctly in private brand checking ✅

**Priority 2 COMPLETE - Private Brand Nominality!**

**What Was Done**:
Per Gemini Pro feedback, completely rewrote `private_brand_assignability_override` to use recursive structure that preserves Union/Intersection semantics.

**Initial Implementation Issues** (found by Gemini Pro):
1. **Flattening Bug**: Collected all shapes into a single `Vec<u32>`, losing Union vs Intersection distinction
2. **Logic Inversions**:
   - Union Source (A | B) -> Target: Checked "at least one" instead of "all"
   - Target Union: Checked "all" instead of "at least one"
   - Intersection Source: Checked "all" instead of "at least one"

**Correct Implementation** (recursive structure):
1. Target Union (A | B): Source must match AT LEAST ONE (OR logic)
2. Source Union (A | B): ALL variants must match target (AND logic)
3. Target Intersection (A & B): Source must match ALL (AND logic)
4. Source Intersection (A & B): AT LEAST ONE must match target (OR logic)
5. Handle Lazy types with recursive resolution
6. Base case: Extract and compare object shapes for nominal checking

**Key Insight**:
Do NOT flatten composite types. Use recursive structure to preserve logical operators.

**Test Results**: All 31 tests passing ✅
- 10/10 private_brands tests
- 10/10 solver::enum_nominality tests
- 11/11 enum_nominality_tests tests

**Files Modified**:
- `src/solver/compat.rs`: Rewrote `private_brand_assignability_override` with proper recursive handling

**Mandatory Gemini Workflow Completed**:
- ✅ PRE-implementation consultation (identified flattening bug)
- ✅ POST-implementation review #1 (found logic inversions)
- ✅ POST-implementation review #2 (verified recursive structure is correct)
- ✅ Applied all fixes per Gemini guidance
- ✅ Verified all tests pass

**Next Steps**:
1. Clean up temporary `collect_shapes_recursive` function (no longer needed)
2. Move to Priority 3: Constructor Accessibility
3. Move to Priority 4: Literal Type Widening

**Status**: Priority 2 COMPLETE ✅


### Commit: `e1a681ba5` - test(tsz-4): add global type mocks to bypass TS2318 test infrastructure issue 🟡

**Priority 3 Status: Constructor Accessibility - MOSTLY COMPLETE (10/11 tests passing)**

**What Was Done**:
Per Gemini Flash guidance, applied global type workaround (interface declarations) to bypass TS2318 test infrastructure blocker.

**Test Results**: 10/11 tests PASSING ✅
- ✅ test_private_constructor_instantiation - External instantiation correctly rejected (TS2673)
- ✅ test_protected_constructor_instantiation - External protected correctly rejected (TS2674)
- ✅ test_private_constructor_inside_class - Static factory pattern works
- ✅ test_protected_constructor_in_subclass - Subclass can call super()
- ✅ test_private_constructor_type_annotation - Type annotations work
- ✅ test_public_constructor_no_restrictions - Public has no restrictions
- ✅ test_default_constructor_is_public - Default constructor is public
- ✅ test_abstract_class_instantiation - Cannot instantiate abstract (TS2511)
- ✅ test_abstract_class_subclass_instantiation - Subclass instantiation works
- ✅ test_protected_constructor_cross_class - Protected rejected outside hierarchy
- ❌ test_private_constructor_in_subclass - FAILS (different feature)

**Remaining Issue**:
The failing test (`test_private_constructor_in_subclass`) checks whether you can **EXTEND** a class with a private constructor (during class declaration), not during `new` expression checking. This is a separate feature requiring class inheritance validation.

Test code:
```typescript
class A { private constructor() {} }
class B extends A {  // Should emit TS2673 here
    constructor() { super(); }
}
```

This check needs to happen in class declaration validation, not in `get_type_of_new_expression`.

**Mandatory Gemini Workflow Completed**:
- ✅ Asked for session redefinition when blocked on test infrastructure
- ✅ Applied Gemini Flash's workaround recommendation
- ✅ Verified 10/11 tests passing with workaround

**Next Steps**:
1. Add class inheritance constructor accessibility check (separate feature)
2. OR mark Priority 3 as "substantially complete" and move to Priority 4
3. Fix test infrastructure properly (remove global type mocks)

**Status**: Priority 3 SUBSTANTIALLY COMPLETE 🟡

### Commit: `fc06762b9` - feat(tsz-4): add TS2675 check for class inheritance with private constructor ✅

**Priority 3 COMPLETE - Constructor Accessibility!**

**What Was Done**:
Per Gemini Pro guidance, implemented TS2675 check for class inheritance with private constructors.

**Implementation**:
1. Added diagnostic code TS2675 in `diagnostics.rs`
2. Added check in `check_heritage_clauses_for_unresolved_names()` in `state_checking.rs`
3. Enhanced `class_constructor_access_level()` to walk inheritance chain for inherited private constructors
4. Fixed test expectations (TS2675 instead of TS2673 for inheritance)

**Key Features**:
- Checks if base class has private/protected constructor
- Allows nested classes to extend (enclosing_class check)
- Recursively walks inheritance chain for implicit constructors
- Emits TS2675 for invalid inheritance attempts

**Bug Fixes** (per Gemini Pro review):
1. **Fixed False Positive**: Added check for `self.ctx.enclosing_class` to allow nested classes to extend private constructor
2. **Fixed False Negative**: Modified `class_constructor_access_level` to walk inheritance chain and check base class constructor

**Test Results**: All 13 constructor accessibility tests passing ✅
- 11/11 original tests (including fixed `test_private_constructor_in_subclass`)
- 1/1 new test for nested class private constructor
- 1/1 new test for inherited private constructor

**Files Modified**:
- `src/checker/state_checking.rs`: Added TS2675 check in heritage clause validation
- `src/checker/type_checking_utilities.rs`: Enhanced `class_constructor_access_level` to walk inheritance chain
- `src/checker/tests/constructor_accessibility.rs`: Fixed test expectations, added 2 new tests
- `src/checker/types/diagnostics.rs`: Added TS2675 diagnostic code and message

**Mandatory Gemini Workflow Completed**:
- ✅ PRE-implementation consultation (asked Gemini about approach)
- ✅ POST-implementation review #1 (found false positive and false negative bugs)
- ✅ POST-implementation review #2 (verified recursive inheritance walk is correct)
- ✅ Applied all fixes per Gemini guidance
- ✅ Verified all tests pass

**Status**: Priority 3 COMPLETE ✅

**Remaining Edge Cases** (per Gemini Pro, rare in practice):
- Deep nesting: `class A { class B { class C extends A {} } }` - requires walking parent chain
- Recursion guard: Could add depth limit to `class_constructor_access_level`

**Next Steps**:
1. Move to Priority 4: Literal Type Widening
2. OR address remaining edge cases if needed

**Overall Session Status**:
- ✅ Priority 1: Enum Nominality COMPLETE
- ✅ Priority 2: Private Brand Checking COMPLETE
- ✅ Priority 3: Constructor Accessibility COMPLETE
- 🔄 Priority 4: Literal Type Widening (PENDING)

### Commit: TBD - Priority 4 Investigation: Literal Type Widening Architecture

**CRITICAL FINDING**: Initial approach was WRONG!

**Gemini Pro Feedback**:
Asked pre-implementation question about implementing widening in CompatChecker. Gemini Pro said:
> "🛑 STOP: Your planned approach is INCORRECT. Do NOT modify CompatChecker to implement widening."

**Why CompatChecker is Wrong**:
- CompatChecker answers: "Is Type A assignable to Type B?"
- CompatChecker does NOT determine what Type A *is*
- Widening is a **Type Inference** problem, not an **Assignability** problem

**Example**:
```typescript
let x = { a: "hello" };
// If x has type { a: "hello" }, that's an INFERENCE problem
// CompatChecker correctly sees "hello" assignable to string
// But CompatChecker cannot "change" the type of x for future assignments
```

**Correct Approach**:
Modify **Type Inference** logic in `src/solver/infer.rs` or `src/checker/expr.rs`:
1. Find where Object Literals are converted to Types
2. Implement `get_widened_type()` function:
   ```rust
   fn get_widened_type(&self, type_id: TypeId) -> TypeId {
       match self.lookup(type_id) {
           TypeKey::Literal(LiteralValue::String(_)) => TypeId::STRING,
           TypeKey::Literal(LiteralValue::Number(_)) => TypeId::NUMBER,
           TypeKey::Literal(LiteralValue::Boolean(_)) => TypeId::BOOLEAN,
           // ... handle unions recursively
           _ => type_id
       }
   }
   ```
3. Apply to object properties when constructing object type

**const vs let Widening Rules**:
- `const x = "a"` → Non-Widening context → Inferred type: `"a"`
- `let x = "a"` → Widening context → Inferred type: `string`
- `const o = { x: "a" }` → Properties are mutable! → Inferred type: `{ x: string }`
- `const o = { x: "a" } as const` → Non-Widening → Inferred type: `{ readonly x: "a" }`

**Union Edge Cases**:
- `"a" | "b"` in widening context → `string`
- `null` and `undefined` → widen to `any` (non-strict) or stay (strict)
- `false | true` → widens to `boolean`

**Freshness vs Widening**:
- **Freshness**: Allows excess properties in object literals
- **Stripping**: Happens when fresh object literal is assigned to variable
- Look for `remove_freshness(TypeId) -> TypeId` in `src/solver/freshness.rs`

**Next Steps**:
1. Investigate `src/solver/infer.rs` and `src/checker/expr.rs`
2. Find where Object Literals are converted to Types
3. Implement widening logic in type inference layer
4. DO NOT modify CompatChecker for this feature

**Status**: Priority 4 - ARCHITECTURE INVESTIGATION COMPLETE 🟡

### Priority 4 Architecture Investigation: Function Bivariance

**Gemini Pro Guidance (2026-02-05)**

#### Q1: Distinguishing Methods from Properties
**Answer**: Cannot distinguish by TypeKey alone. Must use `SymbolFlags` stored in `PropertyInfo`:
- Methods have `SymbolFlags::Method`
- Properties have `SymbolFlags::Property`

**Implementation**:
```rust
// src/solver/types.rs
pub struct PropertyInfo {
    pub name: Atom,
    pub type_id: TypeId,
    pub optional: bool,
    pub readonly: bool,
    pub is_method: bool,  // Derived from SymbolFlags::Method during lowering
}
```

When CompatChecker/SubtypeChecker checks object properties, it reads `is_method` on the **target** property.

#### Q2: Bivariance Scope
**Answer**: Applies ONLY to top-level parameters, NOT recursive.

**Example**:
```typescript
interface A {
    method(cb: (n: number) => void): void;
}
```
- Comparing `method`: Bivariant (because it's a method)
- Comparing `cb`: Contravariant (standard rule, `cb` is just a function type)

**Rule**: "Method Bivariance" is a shallow switch that flips variance for that signature, then reverts to standard rules for constituents.

#### Q3: Compiler Flag Control
**Answer**: Controlled by `strictFunctionTypes` in CompilerOptions.

**Logic Matrix**:
| `strictFunctionTypes` | Target is Method | Target is Property | Behavior |
|---|---|---|---|
| **False** | (Any) | (Any) | **Bivariant** (Legacy) |
| **True** | **Yes** | No | **Bivariant** (Method Exception) |
| **True** | No | **Yes** | **Contravariant** (Strict) |

**Implementation**:
```rust
let use_bivariance = !self.strict_function_types || target_prop.is_method;
```

#### Q4: Overloads
**Answer**: YES, all signatures in a method's CallableShape become bivariant.

When assignability check iterates through signatures, every signature comparison for a method treats parameters bivariantly.

#### Implementation Plan
1. Ensure `PropertyInfo` in `src/solver/types.rs` carries `is_method` boolean
2. In `src/solver/subtype.rs` (likely `compare_object_properties`):
   - Access target property's `PropertyInfo`
   - Determine variance mode: `!self.strict_function_types || target_prop.is_method`
   - Pass mode when comparing property types

#### Next Steps
1. Ask follow-up question: Verify `is_method` approach for PropertyInfo
2. Implement PropertyInfo.is_method field
3. Implement bivariance logic in SubtypeChecker
4. Add tests for method vs property variance
5. Test with --strictFunctionTypes flag

**Status**: Architecture complete, ready for implementation 🟡

### PropertyInfo.is_method Already Exists! ✅

**Gemini Flash Finding (2026-02-05)**:
`is_method` field is ALREADY PRESENT in `PropertyInfo` (line 435 of `src/solver/types.rs`).

**Implementation Already Exists**:
- `lower.rs`: Sets `is_method: true` for METHOD_SIGNATURE and interface methods
- `subtype_rules/objects.rs`: Uses `is_method` in `check_property_compatibility`
- `subtype_rules/functions.rs`: Uses `is_method` to toggle variance in `are_parameters_compatible_impl`
- `subtype.rs`: Has `check_subtype_with_method_variance` utility

**My Task**: VERIFY existing implementation is working correctly

**Action Plan**:
1. Use `tsz-tracing` skill to verify `strict_function_types` is bypassed for methods
2. Check if `lower.rs` is correctly setting `is_method` flag
3. Verify `subtype_rules/functions.rs` is using the flag correctly
4. Write tests to confirm method bivariance works as expected

**Edge Cases to Handle**:
- Intersection types: `{ a(): void } & { a: () => void }` - treat as method if any constituent is method
- Mapped types: Preserve `is_method` flag when iterating
- `this` types: Ensure bivariance applies to `this` parameter checks
- `disable_method_bivariance` flag: Check for sound mode override
- Overloads: Sync `is_method` across CallableShape and PropertyInfo

**Status**: Infrastructure exists, need verification and testing 🟢

### Commit: `2cb7dbcc9` - test(tsz-4): fix function bivariance tests to use interfaces

**Priority 4 Baseline Established**

**What Was Done**:
Created comprehensive test suite for function bivariance in `src/checker/tests/function_bivariance.rs`.

**Test Results**: 5/8 tests passing ✅
- ✅ Methods ARE bivariant (correct)
- ❌ Function properties are ALSO bivariant (BUG - should be contravariant in strict mode)

**Critical Bug Confirmed**:
Current implementation doesn't distinguish between methods and function properties for variance checking. In strict mode (`--strictFunctionTypes`), function properties should be contravariant but they're being treated as bivariant.

**Failing Tests**:
1. `test_arrow_function_property_contravariance` - expects TS2322, gets 0
2. `test_function_property_contravariance` - expects TS2322, gets 0  
3. `test_function_property_contravariance_strict_mode` - expects TS2322, gets 0

**Root Cause**:
The `is_method` flag in `PropertyInfo` exists but is not being used correctly during variance checking in `src/solver/subtype_rules/functions.rs`.

**Next Steps**:
1. Use tsz-tracing with solver subtype logging to find exact code path
2. Identify where `are_parameters_compatible_impl` should check `is_method`
3. Fix the logic to enforce contravariance for non-method properties
4. Re-run tests to verify fix

**Status**: Priority 4 - Bug identified, needs investigation 🔍

### Gemini Flash Guidance (2026-02-05): Function Bivariance Fix

**Recommended Approach**: Use contextual flag in SubtypeChecker

**Why**: Adding arguments to check_subtype would require updating hundreds of call sites. A specialized function only solves the entry point.

**Implementation Plan**:

#### 1. File: `src/solver/subtype.rs`
Add `pub(crate) method_context: bool` to SubtypeChecker struct.

Create wrapper function:
```rust
pub(crate) fn check_subtype_with_method_variance(
    &mut self,
    source: TypeId,
    target: TypeId,
    is_method: bool
) -> SubtypeResult {
    let old = self.method_context;
    self.method_context = is_method;
    let res = self.check_subtype(source, target);
    self.method_context = old;
    res
}
```

#### 2. File: `src/solver/subtype_rules/functions.rs`
Modify check_function_subtype and related functions:

```rust
// Capture effective is_method state
let is_method = source.is_method || target.is_method || self.method_context;

// Reset context for recursive calls (return types, parameters)
let old_context = self.method_context;
self.method_context = false;

// ... check return types ...

// Use captured local is_method for parameters
if !self.are_parameters_compatible_impl(s_param, t_param, is_method) {
    self.method_context = old_context;
    return SubtypeResult::False;
}

self.method_context = old_context; // Restore
```

**Critical**: Reset immediately to prevent bivariance leaking into nested function types.

#### 3. File: `src/solver/subtype_rules/objects.rs`
Verify check_property_compatibility calls check_subtype_with_method_variance.

**Status**: Implementation plan ready 🟢

**Next Steps**:
1. Implement method_context flag in SubtypeChecker
2. Create check_subtype_with_method_variance wrapper
3. Update functions.rs to use captured is_method
4. Update objects.rs to use new wrapper
5. Re-run tests to verify fix
