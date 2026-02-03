# Session tsz-2

## Current Work

**Task**: TS2322 (Type not assignable) - Accessor Type Compatibility False Positives

**Specific Issue**: `accessors_spec_section-4.5_inference.ts`
- TSC expects: NO errors (empty array)
- tsz reports: 4 TS2322 errors (false positives)

**Test Case**:
```typescript
class A { }
class B extends A { }

class LanguageSpec_section_4_5_inference {
    public set InferredGetterFromSetterAnnotation(a: A) { }
    public get InferredGetterFromSetterAnnotation() { return new B(); }
}
```

**Expected Behavior**:
- Getter returns `B` (inferred from `new B()`)
- Setter takes `A`
- Since `B extends A`, `B <: A` should be TRUE
- Should NOT error

**Investigation Findings**:
1. Created unit test `test_accessor_type_compatibility_inheritance_no_error` to reproduce the issue
2. Test FAILS - confirms the bug exists
3. The error message shows: getter is returning Object prototype type instead of `B`:
   ```
   Type '{ isPrototypeOf: { (v: Object): boolean }; ... }' is not assignable to type 'A'.
   ```
4. The problem is NOT in the accessor compatibility check itself
5. The problem is in `new B()` type inference - it returns the wrong type

**Root Cause**:
`new B()` where `B extends A` is being typed as the Object prototype type instead of `B`. This means:
- `get_type_of_node(new B())` returns Object prototype type
- `infer_getter_return_type()` calls `get_type_of_node(return expression)`
- So getter_type = Object prototype type instead of B
- Object prototype is NOT assignable to A
- False positive TS2322 error

**Deep Investigation**:
Found the class instance type construction in `src/checker/class_type.rs`:
- Lines 848-869: Object prototype members are correctly added to class properties
- Lines 871-883: Final instance type is built
- This part is CORRECT - class B should have Object members

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

*No work history yet*

---

## Punted Todos

*No punted items*
