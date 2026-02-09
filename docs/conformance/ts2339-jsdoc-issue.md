# TS2339 False Positives in JavaScript Files with JSDoc

## Issue

TSZ incorrectly reports TS2339 "Property does not exist" errors for dynamic property assignments in JavaScript files when they have JSDoc type annotations.

## Example

```javascript
// @filename: /a.js
class B {
    m(foo = {}) {
        /**
         * @type object
         */
        this.x = foo;  // ❌ ERROR: TS2339: Property 'x' does not exist on type 'B'
    }
}
```

**Expected:** No error (JSDoc annotation allows dynamic property)  
**Actual:** TS2339 error

## TypeScript Behavior

In JavaScript files (`@allowJs: true`), TypeScript allows:
1. Properties to be added via assignment on `this`
2. JSDoc `@type` annotations to specify the property type
3. Dynamic property creation without prior declaration

## Root Cause

TSZ property access checking doesn't handle JavaScript-specific semantics:
- Not recognizing JavaScript file context
- Not checking for JSDoc type annotations before property assignment
- Treating JS files same as TS files (which require declared properties)

## Affected Tests

- `argumentsReferenceInMethod1_Js.ts`
- `argumentsReferenceInMethod3_Js.ts`  
- `argumentsReferenceInMethod5_Js.ts`
- Other `*_Js.ts` tests with dynamic properties

## Fix Requirements

1. **Detect JavaScript context**: Check if file is `.js` with `@allowJs`
2. **Handle JSDoc annotations**: Parse and respect `@type` comments
3. **Allow dynamic properties**: Don't report TS2339 for `this.property` assignments in JS files with JSDoc types
4. **Property type inference**: Use JSDoc type annotation for the property

## Code Locations

Likely in:
- `crates/tsz-checker/src/expr.rs` - Property access checking
- `crates/tsz-checker/src/assignment_checker.rs` - Assignment validation
- JSDoc parsing infrastructure

## Priority

**Medium** - Affects multiple JS test files, but less common than TS-only issues.

## Related

- Part of broader TS2339 false positive pattern (29+ extra errors)
- JavaScript/JSDoc support may need comprehensive review
