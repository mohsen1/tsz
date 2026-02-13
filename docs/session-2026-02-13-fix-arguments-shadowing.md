# Session 2026-02-13: Fix Arguments Variable Shadowing

## Summary
Fixed TS2339 false positive when local `arguments` variable shadows built-in IArguments in function scope.

## Result
- **Conformance tests 100-199**: 89% → 91% (fixed 2 tests)
- **Unit tests**: 368/368 passing ✓

## Problem
We incorrectly emitted TS2339 ("Property doesn't exist") when accessing properties on a local `arguments` variable that shadowed the built-in IArguments object.

### Example Test Case
```javascript
class A {
	constructor(foo = {}) {
		const arguments = this.arguments;  // Local variable shadows IArguments
		this.bar = arguments.bar;  // Should use local variable type, not IArguments
	}

	get arguments() {
		return { bar: {} };
	}
}
```

**Expected**: Only TS1210 (strict mode error)
**Actual (before fix)**: TS1210 + TS2339 (property 'bar' doesn't exist on IArguments)

## Root Cause
In two locations, we checked for the identifier name "arguments" and returned IArguments type **before** checking for local variable shadowing:

1. `type_computation_complex.rs:1563` - Reading identifiers
2. `type_computation.rs:601` - Writing to identifiers

The code always returned IArguments when inside a regular function body, ignoring local declarations.

## Solution
Added logic to check if "arguments" is declared locally within the current function scope:

1. Resolve the identifier symbol
2. Get the enclosing function for both the reference and the declaration
3. If declared in the same function, use the local variable type
4. Otherwise, fall back to built-in IArguments

This correctly handles:
- ✓ Local `const arguments = ...` in the same function (shadows IArguments)
- ✓ Outer scope `arguments` variables (don't shadow IArguments)
- ✓ Parameters named `arguments` (shadow IArguments)

## Files Modified
- `crates/tsz-checker/src/type_computation_complex.rs` - Fixed read path
- `crates/tsz-checker/src/type_computation.rs` - Fixed write path

## Tests Fixed
1. `argumentsReferenceInConstructor4_Js.ts` - Target test with local shadowing
2. `argumentsBindsToFunctionScopeArgumentList.ts` - Global arguments variable

## Tests Verified
- ✓ Local shadowing: `const arguments = this.arguments;` in constructor
- ✓ Global variable: `var arguments = 10;` at top level doesn't shadow in function
- ✓ Both read and write paths

## Remaining Work
9 tests still failing in slice 100-199:
- TS2339 false positives: 2 tests (const enums, AMD modules)
- TS2345 false positives: 2 tests
- Other issues: 5 tests

Next highest-impact fix: TS2488 for arguments Symbol.iterator (could fix 2 tests).
