# TS2339 Investigation Summary

## Date: January 29, 2026

## Issue
**TS2339: Property does not exist on type** - 2482x extra errors in full conformance (top extra error)

## Root Cause Analysis

### Architecture
Property access for built-in types is handled in two ways:

1. **Primitive types** (string, number, boolean, bigint, symbol):
   - Handled in `src/solver/apparent.rs` with hardcoded method lists
   - Function: `apparent_primitive_member_kind()`
   - Called from: `src/solver/operations.rs:resolve_primitive_property()`

2. **Array/Tuple types**:
   - Handled in `src/solver/operations.rs` with hardcoded method list
   - Function: `resolve_array_property()` (lines 3158-3427)
   - Implements: length, map, filter, push, pop, includes, reduce, forEach, join, etc.

3. **Object types**:
   - Handled via object shape lookup in type database
   - Falls back to `apparent_object_member_kind()` for base Object methods

### Potential Sources of TS2339 Errors

1. **Hardcoded lists are incomplete**
   - `apparent.rs` has hardcoded lists for primitive methods
   - If a method is missing from the list, TS2339 is emitted
   - Risk: Typos, missing newly added methods, version mismatches

2. **Union/Intersection types**
   - When property access on `A | B`, both types must have the property
   - When property access on `A & B`, property can be in either type
   - Complex interactions with type narrowing

3. **Generic types**
   - `Array<T>` properties work correctly
   - But what about `Map<K, V>`, `Set<T>`, user-defined generics?
   - Need to check if generic type property access is handled correctly

4. **lib version compatibility**
   - TypeScript hides methods based on `--lib` version
   - TSZ might show all methods regardless of lib setting
   - OR TSZ might be missing methods that depend on specific lib versions

### Array Methods Implemented

✅ **Properties**: `length`

✅ **Methods returning arrays**: concat, filter, flat, flatMap, map, reverse, toReversed, slice, sort, toSorted, splice, toSpliced, with

✅ **Methods returning element**: at, find, findLast, pop, shift

✅ **Methods returning boolean**: every, includes, some

✅ **Methods returning number**: findIndex, findLastIndex, indexOf, lastIndexOf, push, unshift

✅ **Methods returning string**: join, toLocaleString, toString

✅ **Methods returning void**: forEach

✅ **Iteration methods**: entries, keys, values

✅ **Methods returning other**: copyWithin, fill, reduce, reduceRight

### Potential Missing Methods

Looking at `src/solver/operations.rs:3158-3427`, the implementation looks comprehensive. However, TS2339 errors might come from:

1. **Property access on union types** - e.g., `(string | number).toString()`
2. **Property access on intersection types** - e.g., `(A & B).method()`
3. **Property access on type parameters** - e.g., `function foo<T>(x: T) { x.method(); }`
4. **Property access on generic types** - e.g., `Map<K, V>.get()`
5. **Property access on `any` or `unknown`**
6. **Dynamic property access** - e.g., `obj["computed"]`

## Recommended Investigation Steps

1. **Profile the conformance failures**
   ```bash
   ./conformance/run.sh --max=100 > output.txt 2>&1
   grep "TS2339" output.txt | head -20
   ```

2. **Find specific failing tests**
   - Run single test files to see exact TS2339 errors
   - Compare TypeScript vs TSZ output

3. **Add missing methods** (if any)
   - Compare against TypeScript's lib.d.ts
   - Add to `apparent.rs` or `operations.rs`

4. **Fix union/intersection property access**
   - Check if `resolve_property_access` handles unions correctly
   - Check if it handles intersections correctly

5. **Fix generic type property access**
   - Ensure `evaluate_type()` is called before property access on generics
   - Check Application type handling (line 2812)

## Files Involved

- `src/solver/apparent.rs` - Primitive type properties (hardcoded lists)
- `src/solver/operations.rs:3158-3427` - Array/tuple properties (hardcoded)
- `src/solver/operations.rs:2790-2900` - Property access resolution entry point
- `src/solver/intern.rs` - Type interning and evaluation
- `src/checker/type_checking.rs` - TS2339 error emission

## Priority

**HIGH** - 2482x extra errors (most frequent error by far)

## Complexity

**HIGH** - Deep architectural issue touching core type system

## Estimated Effort

2-3 days of focused work to:
1. Profile and categorize failures
2. Add missing methods (if any)
3. Fix union/intersection handling
4. Fix generic type handling
5. Test and validate improvements
