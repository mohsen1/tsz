# Session TSZ-4: Nominality & Accessibility (Lawyer Layer)

**Started**: 2026-02-05
**Status**: 🔄 Active - In Progress (Enum Member Distinction)
**Focus**: Implement TypeScript's nominal "escape hatches" (Enums, Private/Protected members) and visibility constraints

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

#### Priority 3: Implement Constructor Accessibility (TS2673 / TS2674)
**Current State**: `constructor_accessibility_override` is a `Noop`

**Task**: Implement the check that prevents assigning a class with `private` or `protected` constructor
- Cannot assign class type to `new()` signature if constructor is private/protected
- Must respect scope (e.g., static methods can access private constructor)
- Emit `TS2673` or `TS2674` errors

**Files**:
- `src/solver/compat.rs` - Implement the override
- `src/checker/declarations.rs` - Integrate with class declaration checking

#### Priority 4: Literal Type Widening (Freshness Counterpart)
**Task**: Ensure that when a "fresh" object literal is assigned to a non-literal type, its internal literal types widen
- `"a"` widens to `string` when target requires it
- This is the INVERSE of the freshness work you just completed
- Ensures object literals don't have overly specific types after assignment

### Success Criteria
- [ ] `Enum A` assigned to `Enum B` produces a diagnostic
- [ ] `Class A { private x }` assigned to `Class B { private x }` produces a diagnostic
- [ ] `new C()` where `C` has a private constructor produces a diagnostic
- [ ] Literal types in object literals widen correctly after assignment
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
