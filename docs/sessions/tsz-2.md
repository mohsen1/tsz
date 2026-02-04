# Session tsz-2

## Current Work

**Task**: Module Resolution and Lib Loading (TS2307, TS2318)

**Target Issues**:
- TS2307 (Cannot find module) - 2,139 false negatives
- TS2318 (Cannot find global type) - 3,386 false negatives

**Rationale**: These are "Environment" errors. If tsz fails to load the standard library or imports, it lacks the definitions required to perform accurate type checking. Fixing these False Negatives will likely reduce the count of False Positives (like TS2304 and TS2322).

**Starting Point**: Asking Gemini for guidance on module resolution architecture and where to start investigating.

---

## Previous Work

**Completed**: TS2322 (Type not assignable) - Accessor Type Compatibility False Positives ✅

**Specific Issue**: `accessors_spec_section-4.5_inference.ts`
- TSC expects: NO errors (empty array)
- tsz was reporting: 4 TS2322 errors (false positives)
- **Now FIXED**: tsz correctly reports 0 errors

**Test Case**:
```typescript
class A { }
class B extends A { }

class LanguageSpec_section_4_5_inference {
    public set InferredGetterFromSetterAnnotation(a: A) { }
    public get InferredGetterFromSetterAnnotation() { return new B(); }
}
```

## Resolution Summary

### Root Causes Identified and Fixed:

**1. Missing Nominal Typing for Empty Classes**
- Empty classes A and B were both getting `Object(ObjectShapeId(0))` - the same type
- This broke nominal typing where each class should have a distinct type
- **Fix**: Set `symbol` field in `ObjectShape` for ALL class instance types (src/checker/class_type.rs:888-918)
- Now each class gets a distinct `ObjectWithIndex` type with its unique symbol

**2. Type Annotations Resolving to Constructor Types**
- When processing `a: A` where A is a class, the type was resolving to the constructor type (Callable) instead of instance type (Object)
- In type position, class references should return instance types
- `typeof A` should return constructor types
- **Fix**: Added `resolve_type_annotation` helper (src/checker/type_checking_queries.rs:21-65)
- Detects direct class Lazy references and extracts instance type from construct signatures

### Changes Made:

1. **src/checker/class_type.rs**:
   - Changed instance type construction to use `object_with_index` with symbol set for all classes
   - Both branches (with/without index signatures) now set `symbol: current_sym`

2. **src/checker/type_checking_queries.rs**:
   - Added `resolve_type_annotation()` helper function
   - Checks if type annotation is a direct Lazy reference to a CLASS symbol
   - Extracts instance type from constructor type's construct signatures
   - Preserves constructor types for `typeof` expressions and type aliases

### Test Results:
- ✅ `test_accessor_type_compatibility_inheritance_no_error` - PASSES
- The getter returning `new B()` (B extends A) is now correctly assignable to setter taking `A`

### Known Issues (Pre-existing, not introduced by this fix):
- `test_abstract_constructor_assignability` - Shows Object prototype type in error messages
- `test_abstract_mixin_intersection_ts2339` - Similar issues with property access
- These appear to be pre-existing issues related to type formatting/error messages



The issue is likely in how `new B()` gets the instance type:
1. `get_type_of_new_expression()` (line 23 in type_computation_complex.rs)
2. Should call `get_construct_signature_return_type()` to get B's instance type
3. Something is going wrong - returning Object prototype instead of B

**Next Steps**:
1. Debug `new B()` expression resolution
2. Check if construct signature return type is correct for class B
3. Verify that `get_class_instance_type()` is being called correctly
4. May need to trace through Lazy type resolution for class symbols

### What I've Learned

**Conformance System Architecture:**
- Two-phase testing: (1) Generate TSC cache, (2) Run tsz and compare
- Cache file: `tsc-cache-full.json` (2.3MB, 12,399 entries)
- Test directory: `TypeScript/tests/cases/` (compiler, conformance, fourslash, etc.)
- Runner: Rust-based with tokio parallel execution (16 workers default)

**Current Test Results (50 tests sample):**
- Pass rate: 32% (16/50)
- Top error mismatches:
  - TS1005: missing=11 ("{0}" expected - Parser error recovery)
  - TS2695: missing=10 (Namespace no exported member - Module resolution)
  - TS1068: missing=1 (Continuation statement not within loop)
  - TS2307: missing=1 (Cannot find module)
  - TS2511: missing=1 (Cannot create instance of abstract class)

**Key Conformance Gaps (from docs/walkthrough/07-gaps-summary.md):**

False Positives (we report, TSC doesn't):
1. TS2322 (11,773x) - Type not assignable
2. TS2694 (3,104x) - Namespace no exported member
3. TS1005 (2,703x) - '{0}' expected
4. TS2304 (2,045x) - Cannot find name
5. TS2571 (1,681x) - Object is 'unknown'
6. TS2339 (1,520x) - Property doesn't exist
7. TS2300 (1,424x) - Duplicate identifier

False Negatives (TSC reports, we don't):
1. TS2318 (3,386x) - Cannot find global type
2. TS2307 (2,139x) - Cannot find module
3. TS2488 (1,749x) - Must have Symbol.iterator
4. TS2583 (706x) - Change target library?
5. TS18050 (680x) - Value cannot be used here

### Conformance Improvement Opportunities

Based on the gaps and error frequencies, here are the highest-impact areas:

**1. Parser Error Recovery (TS1005)**
- Impact: 2,703 false positives
- Files: `src/parser/`, especially `state.rs`
- Issue: Parser doesn't recover from syntax errors as well as TSC

**2. Module Resolution & Lib Loading (TS2307, TS2318, TS2694, TS2695)**
- Impact: ~8,000+ test failures
- Files: `src/binder/state.rs`, lib loading infrastructure
- Issue: Module resolution, namespace exports, lib.d.ts loading incomplete

**3. Symbol Resolution (TS2304)**
- Impact: 2,045 false positives
- Files: `src/binder/state.rs`, `src/checker/symbol_resolver.rs`
- Issue: Symbol lookup and resolution incomplete

**4. Subtype Checking (TS2322)**
- Impact: 11,773 false positives
- Files: `src/solver/subtype*.rs`
- Issue: Type assignability rules not matching TSC

**5. Object Types (TS2339, TS2571)**
- Impact: 3,201 false positives
- Files: `src/solver/`, `src/checker/type_checking.rs`
- Issue: Property access, type narrowing, `unknown` type handling

### Gemini Recommendation

**Priority: Module Resolution (TS2307) and Lib Loading (TS2318)**

While TS2322 has the highest count (11,773), it's a "symptom" error. The root causes are often upstream: if the compiler cannot find a module or a global type (like `Promise` or `Array`), the resulting types become `error` or `any`, causing cascading assignability failures.

**Focus on False Negatives first:**
- TS2307 (Cannot find module) - 2,139x
- TS2318 (Cannot find global type) - 3,386x

These are "Environment" errors. If tsz fails to load the standard library or imports, it lacks the definitions required to perform accurate type checking. Fixing these False Negatives will likely reduce the count of False Positives (like TS2304 and TS2322).

**Files to Read (in order):**
1. `src/binder/state.rs` - Look for `resolve_import_with_reexports()`, depends on pre-populated `module_exports`
2. `src/checker/state.rs` - Check how the checker initializes the global scope
3. `src/binder/mod.rs` - Review the `Symbol` struct to see how exports are stored

**Dependencies:**
- TS2322 (Assignability) depends on TS2318/TS2307
- TS2694 (Namespace exports) depends on TS2307
- TS2304 (Cannot find name) depends on TS2318

**Action Plan:**
1. Create a reproduction test case that imports a file and fails with TS2307
2. Investigate `src/binder/state.rs` to implement basic module resolution logic
3. Verify if `lib.d.ts` symbols are being loaded into the root scope

---

## History (Last 20)

### 2025-02-04: FIXED TS2322 accessor false positive with class inheritance

**Root Causes Fixed**:
1. **Nominal typing for empty classes**: Empty classes A and B were both getting `Object(ObjectShapeId(0))` - the same type. Fixed by setting `symbol` field in `ObjectShape` for ALL class instance types.

2. **Type annotation resolution**: Class references in type position (e.g., `a: A`) were resolving to constructor types instead of instance types. Fixed by adding `resolve_type_annotation()` helper that detects direct class Lazy references and extracts instance type from construct signatures.

**Files Modified**:
- `src/checker/class_type.rs`: Set symbol in ObjectShape for all class instance types
- `src/checker/type_checking_queries.rs`: Added resolve_type_annotation helper for accessor type checking

**Test Results**:
- ✅ `test_accessor_type_compatibility_inheritance_no_error` now PASSES
- `new B()` where `B extends A` now correctly returns B's instance type
- Getter returning B is correctly assignable to setter taking A

**Technical Details**:
- Used Gemini to understand TypeScript's nominal typing system
- Learned that ObjectShape.symbol field exists for this exact purpose
- Discovered distinction between type position (`a: A`) and type query (`typeof A`)
- Created targeted fix that only affects direct class Lazy references, not type aliases

### 2025-02-03: Investigated TS2322 accessor false positive with class inheritance

**Test Added**: `test_accessor_type_compatibility_inheritance_no_error`
- Confirms bug: `new B()` where `B extends A` returns Object prototype type instead of B
- Error message shows getter returns `{ isPrototypeOf, propertyIsEnumerable, ... }` (Object)

**Investigation Deep Dive**:
1. Found `classify_for_new_expression()` missing Lazy type case
2. Added fix to handle `TypeKey::Lazy(def_id)`
3. Fix didn't work - reverted
4. Verified construct signatures correctly use `instance_type` as return type
5. Verified `compute_type_of_symbol()` returns constructor type for CLASS symbols

**Root Cause**: Still unknown - bug is deep in type resolution chain for `new` expressions with class inheritance. Requires tracing through Lazy type resolution and CallEvaluator.

**Files Investigated**:
- `src/solver/type_queries_extended.rs` - classify_for_new_expression
- `src/checker/type_computation_complex.rs` - get_type_of_new_expression
- `src/checker/class_type.rs` - get_class_instance_type, get_class_constructor_type
- `src/checker/state_type_analysis.rs` - get_type_of_symbol, compute_type_of_symbol

---

## Punted Todos

*No punted items*
