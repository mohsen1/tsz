# Session Summary: Conformance Tests 100-199 (2026-02-13)

## Final Status
**Pass Rate: 89/100 (89%)**

## Major Achievement: Fixed JavaScript Type Checking

### The Problem
The `--checkJs` CLI flag was being parsed but never applied to compiler options. This meant JavaScript files were **never type-checked**, even when explicitly requested with `--allowJs --checkJs`.

### The Fix (Commit d9ebb5e5e)
Added 3 lines to `apply_cli_overrides()` in `crates/tsz-cli/src/driver.rs:3478`:

```rust
if args.check_js {
    options.check_js = true;
}
```

### Impact
✅ **Enabled**: JavaScript type checking now works  
✅ **Enabled**: TS1210 detection (arguments shadowing in class constructors)  
✅ **Unblocked**: 2-3 JavaScript-related conformance tests  
⚠️ **Trade-off**: Pass rate went 90% → 89% (exposed pre-existing bugs)

## Analysis of 11 Failing Tests

### By Category
- **False Positives (7 tests)**: We emit errors TSC doesn't
- **All Missing (1 test)**: We don't emit errors TSC does
- **Wrong Codes (3 tests)**: Both have errors, different codes
- **Close (2 tests)**: Differ by only 1-2 error codes

### By Error Code Impact
**Highest Impact (fix helps multiple tests):**
1. **TS2339** - Incorrectly emitted in 3 tests
   - amdModuleConstEnumUsage.ts - Const enum member access
   - amdLikeInputDeclarationEmit.ts - AMD module resolution
   - argumentsReferenceInConstructor4_Js.ts - JS constructor properties

2. **TS2322** - Incorrectly emitted in 2 tests
   - ambientClassDeclarationWithExtends.ts - Ambient classes with namespace merging
   - amdDeclarationEmitNoExtraDeclare.ts - Class mixin patterns

3. **TS2345** - Incorrectly emitted in 2 tests
   - amdDeclarationEmitNoExtraDeclare.ts - Mixin constructor arguments
   - anonClassDeclarationEmitIsAnon.ts - Class expression patterns

### Root Causes Identified

#### 1. JavaScript Constructor Property Assignment
**Tests Affected:** argumentsReferenceInConstructor4_Js.ts

TSZ emits TS2339 for properties dynamically added in JavaScript constructors:
```javascript
class A {
  constructor(foo = {}) {
    this.foo = foo;  // TS2339: Property 'foo' does not exist on type 'A'
  }
}
```

In JavaScript, this is valid - properties can be added dynamically. TSC with `--checkJs` infers these properties from assignments.

**Root Cause:** The checker doesn't recognize JSDoc-annotated property assignments in constructors as valid property declarations.

#### 2. Arguments Symbol.iterator Type Inference  
**Tests Affected:** argumentsObjectIterator02_ES6.ts

TSZ incorrectly infers `blah()` as `AbstractRange<any>` instead of `ArrayIterator<any>`:
```typescript
let blah = arguments[Symbol.iterator];
for (let arg of blah()) {  // TS2488: AbstractRange<any> lacks Symbol.iterator
```

**Root Cause:** Property access with `Symbol.iterator` returns wrong type. Should be `() => ArrayIterator<any>`, but resolves to DOM type `AbstractRange<any>`.

#### 3. Super Property Access Visibility
**Tests Affected:** argumentsReferenceInConstructor3_Js.ts

TSZ emits TS2340 on valid super property access:
```javascript
class B extends A {
  constructor() {
    super.arguments.foo;  // TS2340: Only public/protected accessible via super
  }
}
```

Where `arguments` is a public getter in class A.

**Root Cause:** Visibility checking for super property access doesn't recognize getters as public members.

#### 4. Const Enum Member Resolution
**Tests Affected:** amdModuleConstEnumUsage.ts, amdLikeInputDeclarationEmit.ts

TSZ can't resolve imported const enum members or fails with module resolution errors.

**Root Cause:** Const enum inlining or module resolution issues with AMD modules.

## Next Steps (Prioritized by ROI)

### High Impact (2-3 tests each)
1. **Fix JavaScript constructor property inference** (+1 test)
   - Location: `crates/tsz-checker/src/` - property assignment handling in JS files
   - Enable JSDoc-inferred properties from constructor assignments
   
2. **Fix TS2339 false positives** (+2-3 tests)
   - Investigate const enum member resolution
   - Fix AMD module property access

### Medium Impact (1 test each)
3. **Fix super property visibility check** (+1 test)
   - Location: `crates/tsz-checker/src/` - super property access validation
   - Recognize public getters/setters as accessible via super

4. **Fix arguments Symbol.iterator type** (+1 test)
   - Location: `crates/tsz-checker/src/type_computation.rs` - property access type resolution
   - Ensure `arguments[Symbol.iterator]` resolves correctly

### Low Priority (parser issues)
5. **Fix TS1434 vs TS2304 in ambiguous generic context**
   - Parser-level issue with `<<T>` operator/generic disambiguation

## Documentation Created
- `docs/2026-02-13-conformance-investigation.md` - Initial investigation
- `docs/2026-02-13-checkjs-fix.md` - JavaScript checking fix details
- `docs/2026-02-13-session-summary.md` - This document

## Commits
- `f59560a5e` - docs: comprehensive investigation (90% pass rate analysis)
- `d9ebb5e5e` - fix(cli): apply --checkJs flag from CLI arguments
- `b59ee74e7` - docs: document checkJs fix and JavaScript checking issues
- `[this commit]` - docs: session summary

## Key Insights

1. **Enabling features can expose bugs**: The --checkJs fix was correct and necessary, but it exposed pre-existing type checker bugs that were hidden when JS files weren't being checked.

2. **False positives are high impact**: Fixing false positive errors (especially TS2339) affects multiple tests and improves user experience.

3. **JavaScript type checking needs work**: Several issues with JSDoc, property inference, and type annotations in JavaScript files.

4. **Root causes are deep**: Most remaining failures require non-trivial fixes in the type checker core, not simple one-liners.

## Recommendations

To reach 95% pass rate (95/100):
1. Fix JavaScript constructor property inference (+1 test)
2. Fix 2 TS2339 const enum/module issues (+2 tests) 
3. Fix super property visibility (+1 test)
4. Fix arguments iterator type (+1 test)

Total: +5 tests → 94/100 (94%)

Alternative if some prove too difficult:
- Focus on the "low-hanging fruit" false positives
- Each simple fix = +1 test
- Need 6 fixes to reach 95/100
