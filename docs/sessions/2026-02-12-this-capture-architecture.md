# This Capture Architecture Issue

## Problem

Arrow functions that capture `this` are currently emitted with IIFE wrappers per function, but TypeScript emits a single `var _this = this;` declaration at the scope level.

### Expected (TypeScript)
```javascript
var _this = this;
var f1 = function () { _this.age = 10; };
var f2 = function (x) { _this.name = x; };
```

### Actual (Our Compiler)
```javascript
var f1 = (function (_this) { return function () { _this.age = 10; }; })(this);
var f2 = (function (_this) { return function (x) { _this.name = x; }; })(this);
```

## Root Cause

Our current implementation in `emit_arrow_function_es5()` (crates/tsz-emitter/src/emitter/es5_helpers.rs:706) wraps each arrow function individually in an IIFE when it captures `this`.

## Required Changes

To match TypeScript's behavior:

1. **Scope-Level Detection**: Detect at the module/function scope level if any arrow functions capture `this`

2. **Hoisted Declaration**: Emit `var _this = this;` once at the top of that scope

3. **Simplified Arrow Emission**: Convert arrow functions to regular functions without IIFE wrappers
   - No `(function (_this) { ... })(this)` wrapper
   - Just emit `function () { _this.age = 10; }`

4. **This Substitution**: Ensure `this` references in arrow bodies are substituted with `_this`
   - This might already be handled via SubstituteThis directives

## Implementation Strategy

1. **In LoweringPass**:
   - Track `needs_this_capture` at scope level (source file, function, etc.)
   - Mark scopes that need the `var _this = this;` declaration

2. **In Emitter**:
   - Add logic to emit `var _this = this;` at the top of scopes that need it
   - Modify `emit_arrow_function_es5()` to:
     - Skip IIFE wrapper when at scope with hoisted `_this`
     - Just emit as regular function
     - Rely on SubstituteThis directives for `this` -> `_this` conversion

3. **Scope Management**:
   - Need to track which scope level has the `_this` declaration
   - Nested arrow functions should use the outer `_this`
   - Nested regular functions would need their own `_this` if they have arrow functions

## Test Case

`TypeScript/tests/cases/conformance/es6/arrowFunction/emitArrowFunctionThisCapturing.ts`

Currently fails with +6/-7 lines difference.

## Priority

Medium-High. This affects:
- Test: emitArrowFunctionThisCapturing
- General arrow function ES5 compatibility
- Code size and performance (IIFEs are larger and slower)

## Notes

- The current IIFE approach is functionally correct but differs from TypeScript's output
- May need architectural changes to how we track scope-level state
- Consider whether this should be done in lowering_pass.rs or emitter layer
