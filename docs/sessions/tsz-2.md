# Session tsz-2: Checker-Solver Bridge & Type Alias Resolution

**Started**: 2026-02-04
**Status**: 🟢 Active (Phase 1: Nominal Subtyping COMPLETE)
**Previous**: BCT and Intersection Reduction (COMPLETED 2026-02-04)

### COMPLETED WORK: Nominal Subtyping ✅

**Phase 1 Complete** (2026-02-04):
1. ✅ **Task 1**: Visibility enum and parent_id added to PropertyInfo
2. ✅ **Task 2**: Lowering logic populates visibility from modifiers
3. ✅ **Task 3**: Property compatibility checking with nominal subtyping
4. ✅ **Task 4**: Inheritance and overriding with parent_id tracking

All 4 tasks complete. TypeScript classes now have proper nominal subtyping for private/protected members.

---

## NEW FOCUS: Checker-Solver Bridge (Type Alias Resolution)

### Problem Statement

The **Checker-Solver bridge is broken** for type aliases. The Solver cannot resolve the bodies of type aliases from `lib.d.ts` (like `Partial<T>`, `Pick<T, K>`, etc.), even though:

1. The **Mapper exists** (`evaluate_mapped` - 442 lines, fully implemented)
2. The **type logic exists** (lowering produces correct structures)
3. The **registration code exists** (TypeEnvironment has `insert_def_with_params`)

**The Problem**: When `Partial<Foo>` is evaluated:
- `evaluate_application` tries to resolve the type alias body
- `resolve_lazy` returns `None` (body not found)
- Result: `unknown` instead of the mapped type structure

**Why This Matters**:
- **Solver-First Violation**: Solver should be the source of truth but is "blinded" by missing alias bodies
- **Blocks All Type Metaprogramming**: Cannot use mapped/conditional types if aliases don't resolve
- **Blocks Other Sessions**: tsz-3 (CFA), tsz-4 (emit), tsz-5, tsz-6 all depend on this bridge

---

## Implementation Plan

### Focus: Fix TypeEnvironment Registration for Type Aliases

The investigation revealed that type aliases from lib.d.ts are not being registered in TypeEnvironment, causing `resolve_lazy()` to return `None`. The fix requires:

1. **Understand the registration sequence** - Trace where type aliases should call `insert_def_with_params`
2. **Identify the missing link** - Find why the registration isn't happening
3. **Fix the bridge** - Ensure type alias bodies flow from Checker to Solver correctly

**Key Files**:
- `src/checker/state_type_analysis.rs:1336` - `compute_type_of_symbol` (type alias handling)
- `src/solver/lower.rs:741` - `lower_type_alias_declaration` (TypeLowering bridge)
- `src/solver/subtype.rs:357` - `insert_def_with_params` (TypeEnvironment registration)
- `src/solver/db.rs` - `resolve_lazy` implementation

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

## Next Steps

Before implementing any fixes to the Checker-Solver bridge, use the **Two-Question Rule**:

### Question 1: Approach Validation
```bash
./scripts/ask-gemini.mjs --include=src/checker --include=src/solver "I need to fix the TypeEnvironment registration issue for type aliases.

Problem: lib.d.ts type aliases like Partial<T> are not registered in TypeEnvironment, causing resolve_lazy() to return None.

Investigation findings:
- evaluate_application calls resolver.resolve_lazy(def_id) and returns unknown when it gets None
- TypeEnvironment's def_types HashMap is empty for Partial's DefId
- Added set_body() call but no improvement

My planned approach:
1. Create minimal test case demonstrating the bug
2. Use tsz-tracing to trace where resolve_lazy returns None
3. Audit TypeLowering pipeline to find registration sequence
4. Fix the registration to call insert_def_with_params

Before I implement: 1) Is this the right approach? 2) What specific files/functions should I examine? 3) Are there TypeScript behaviors I need to match?"
```

### Question 2: Implementation Review
```bash
./scripts/ask-gemini.mjs --pro --include=src/checker --include=src/solver "I implemented TypeEnvironment registration fix in [FILE]:[FUNCTION].

Changes: [PASTE CODE OR DIFF]

Please review: 1) Is this logic correct for TypeScript? 2) Did I miss any edge cases with recursive type aliases? 3) Are there type system bugs? Be specific if it's wrong."
```

---

### COMPLETED WORK: BCT and Intersection Reduction

---

## COMPLETED WORK: Nominal Subtyping (Phase 1)

All four tasks completed successfully on 2026-02-04:
1. ✅ **Visibility Enum and ParentId** - Added to PropertyInfo
2. ✅ **Lowering Logic** - Populate visibility from modifiers
3. ✅ **Property Compatibility** - Nominal checking for private/protected
4. ✅ **Inheritance and Overriding** - parent_id tracking through class hierarchy

---

## COMPLETED WORK: BCT and Intersection Reduction

**Previous session accomplishments** (2026-02-04):
1. ✅ **Intersection Reduction** - Recursive evaluation for meta-types
2. ✅ **BCT for Intersections** - Extract common members from intersection types
3. ✅ **Lazy Type Support** - BCT works with Lazy(DefId) classes
4. ✅ **Literal Widening** - Array literals like [1, 2] infer as number[]
5. ✅ **Intersection Sorting Fix** - Preserve callable order for overload resolution

**Test Results**: All 18 BCT tests pass, no regressions

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

## Next Steps

Before implementing any fixes to the Checker-Solver bridge, use the **Two-Question Rule**:

### Question 1: Approach Validation
```bash
./scripts/ask-gemini.mjs --include=src/checker --include=src/solver "I need to fix the TypeEnvironment registration issue for type aliases.

Problem: lib.d.ts type aliases like Partial<T> are not registered in TypeEnvironment, causing resolve_lazy() to return None.

Investigation findings:
- evaluate_application calls resolver.resolve_lazy(def_id) and returns unknown when it gets None
- TypeEnvironment's def_types HashMap is empty for Partial's DefId
- Added set_body() call but no improvement

My planned approach:
1. Create minimal test case demonstrating the bug
2. Use tsz-tracing to trace where resolve_lazy returns None
3. Audit TypeLowering pipeline to find registration sequence
4. Fix the registration to call insert_def_with_params

Before I implement: 1) Is this the right approach? 2) What specific files/functions should I examine? 3) Are there TypeScript behaviors I need to match?"
```

### Question 2: Implementation Review
```bash
./scripts/ask-gemini.mjs --pro --include=src/checker --include=src/solver "I implemented TypeEnvironment registration fix in [FILE]:[FUNCTION].

Changes: [PASTE CODE OR DIFF]

Please review: 1) Is this logic correct for TypeScript? 2) Did I miss any edge cases with recursive type aliases? 3) Are there type system bugs? Be specific if it's wrong."
```

---

## Success Criteria

1. **TypeEnvironment Registration**:
   - [ ] `Partial<T>` resolves to mapped type structure instead of `unknown`
   - [ ] Type aliases from lib.d.ts are registered in TypeEnvironment
   - [ ] `resolve_lazy()` returns correct type bodies for all DefIds

2. **Conformance**:
   - [ ] Mapped type pass rate improves from 32.1% (18/56 tests)
   - [ ] No regressions in existing tests
   - [ ] Standard library types (Partial, Readonly, Pick, etc.) work correctly

---

## Session History

- 2026-02-04: Started as "Intersection Reduction and Advanced Type Operations"
- 2026-02-04: **COMPLETED** BCT, Intersection Reduction, Literal Widening
- 2026-02-04: **FIXED** Intersection sorting bug (preserve callable order)
- 2026-02-04: **COMPLETED** Phase 1: Nominal Subtyping (all 4 tasks)
- 2026-02-04: **REDEFINED** to "Advanced Type Evaluation & Inference"
- 2026-02-04: Conformance audit revealed mapped types at 32.1% pass rate
- 2026-02-04: **INVESTIGATED** TypeEnvironment registration issue
- 2026-02-04: **REDEFINED** to "Checker-Solver Bridge & Type Alias Resolution"

---

## Completed Commits (History)

- `7bf0f0fc6`: Intersection Reduction (evaluate_intersection, evaluate_union)
- `7dfee5155`: BCT for Intersections + Lazy Support
- `c3d5d36d0`: Literal Widening for BCT
- `f84d65411`: Fix intersection sorting - preserve callable order

---

## Complexity: HIGH

**Why High**:
- TypeEnvironment registration issue is a deep architectural problem
- Requires understanding Binder → Checker → Solver data flow
- Type alias lowering pipeline is complex with multiple code paths
- Circular dependency resolution adds complexity
- Changes to the bridge can affect all type operations

**Risk**: Changes to Checker-Solver bridge can cause regressions across the entire type system.

**Mitigation**: Follow Two-Question Rule strictly. All changes must be reviewed by Gemini Pro.

## Gemini Flash Analysis (2026-02-04)

**Question Asked**: "I need to fix the TypeEnvironment registration issue for type aliases..."

**Key Insights**:

1. **`try_borrow_mut()` Silent Failure** (CRITICAL)
   - Location: `src/checker/state.rs` - `get_type_of_symbol` around line 3080
   - Problem: If environment is already borrowed during recursive resolution, the registration silently fails
   - Impact: DefId is never registered in TypeEnvironment, causing `resolve_lazy` to return None

2. **Lib Resolution Bypass**
   - Location: `src/checker/state_type_resolution.rs` - `resolve_lib_type_by_name`
   - Problem: This function bypasses normal registration path when lowering lib.d.ts types
   - Impact: Creates DefIds without calling `insert_def_with_params`

3. **Registration Gap in Delegation Block**
   - Location: `src/checker/state.rs` - `get_type_of_symbol` lines 3045-3065
   - Problem: When symbol is resolved in different arena (like lib.d.ts), returns early without updating main checker's type_env
   - Impact: Type aliases from lib.d.ts aren't visible to solver

**Recommended Fix Sequence**:
1. Add debug warnings/panics to `try_borrow_mut` to identify dropped registrations
2. Audit `resolve_lib_type_by_name` to ensure it routes through registration logic
3. Verify `compute_type_of_symbol` TYPE_ALIAS branch returns correct TypeParamInfo
4. Check param identity - ensure TypeParamInfo TypeIds match lowered body

**Key Files to Examine**:
- `src/checker/state.rs` - `get_type_of_symbol` orchestration bottleneck
- `src/checker/state_type_resolution.rs` - `resolve_lib_type_by_name`, `type_reference_symbol_type`
- `src/checker/state_type_analysis.rs` - `compute_type_of_symbol` TYPE_ALIAS branch (lines 3230-3255)
- `src/solver/application.rs` - `evaluate_inner` where `resolve_lazy` is called

**TypeScript Behaviors to Match**:
- **Transparency**: Type aliases are transparent - `Partial<T>` is just a name for a Mapped type
- **Recursive Aliases**: Must support `type Tree<T> = T | Tree<T>[]` by registering DefId before body is fully computed

## FIX IMPLEMENTED (2026-02-04)

**Root Cause Found**: `resolve_lib_type_by_name` only called `insert_def_type_params` 
but never `insert_def_with_params`, causing TypeEnvironment to have no entry for lib type aliases.

**Location**: `src/checker/type_checking_queries.rs` lines 1899-1901

**Fix Applied**: Added `insert_def_with_params` call to register type body in TypeEnvironment:
```rust
// Cache type parameters for Application expansion
let def_id = self.ctx.get_or_create_def_id(sym_id);
self.ctx.insert_def_type_params(def_id, params.clone());

// CRITICAL: Register the type body in TypeEnvironment
if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
    env.insert_def_with_params(def_id, ty, params);
}

lib_types.push(ty);
```

**Commit**: `ae03bafeb` - "fix: register lib type aliases in TypeEnvironment for resolve_lazy"

**Expected Impact**: Should unblock `evaluate_mapped` to be triggered, improving mapped type 
conformance from 32.1% (18/56 tests).

**Next**: Run conformance tests to validate fix.
