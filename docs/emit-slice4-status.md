# Emit Testing - Slice 4 Status Report
**Date:** 2026-02-12
**Slice:** 4 - Helper functions and this capture

## Current Status

### Test Pass Rates
- **ES5For-of tests**: 70% (14/20 passing)
- **Arrow function tests**: 25% (4/16 passing)
- **Overall sample (200 tests)**: 68.2%

### Work Completed ✅

**1. downlevelIteration Support**
- Implemented parsing of `downlevelIteration` compiler option from test file comments
- Tests like ES5For-of33 now correctly emit `__values` helper
- Commit: `1fdd6bbcf`

## Remaining Issues for Slice 4

### 1. Variable Shadowing/Renaming 🔴 HIGH PRIORITY

**Problem:** When for-of loops have nested scopes with same variable names, we don't add suffixes to avoid conflicts.

**Example (ES5For-of15):**
```typescript
for (let v of []) {
    for (const v of []) {  // Inner v shadows outer v
        var x = v;
    }
}
```

**Expected output:**
```javascript
for (var _i = 0, _a = []; _i < _a.length; _i++) {
    var v = _a[_i];
    for (var _b = 0, _c = []; _b < _c.length; _b++) {
        var v_1 = _c[_b];  // Renamed with _1 suffix
        var x = v_1;
    }
}
```

**Our output:**
```javascript
// Same but uses `var v` in both loops (conflict)
```

**Affected tests:**
- ES5For-of15, ES5For-of16, ES5For-of17
- ES5For-of19 (function scope variation)
- ES5For-of24 (variable shadowing in loop body)

**Implementation needed:**
1. Track variable names declared in current function scope
2. When emitting for-of variable binding, check if name already used
3. Add suffix (_1, _2, etc.) if shadowing detected
4. Handle both loop initializer and loop body declarations

**Code location:** `crates/tsz-emitter/src/emitter/es5_bindings.rs:1778`
- Currently just `self.emit(decl.name)` without shadowing check

### 2. This Capture Pattern 🔴 HIGH PRIORITY

**Problem:** We use IIFE pattern for this capture instead of simple `var _this = this` declaration.

**Example:**
```typescript
class MyClass {
    method() {
        var arrow = () => this;
    }
}
```

**Expected output:**
```javascript
MyClass.prototype.method = function () {
    var _this = this;  // Simple declaration at function start
    var arrow = function () { return _this; };
};
```

**Our output:**
```javascript
MyClass.prototype.method = function () {
    var arrow = (function (_this) {  // IIFE wrapper
        return function () { return _this; };
    })(this);
};
```

**Affected tests:**
- arrowFunctionContexts
- arrowFunctionExpressions
- abstractPropertyInConstructor
- APISample_jsdoc
- Many more (arrow function tests have 25% pass rate)

**Implementation needed:**
1. Detect if function contains arrow functions that use `this`
2. Emit `var _this = this;` at start of function
3. Transform arrow function bodies to use `_this` instead of `this`
4. Don't use IIFE wrapper

**Code location:**
- `crates/tsz-emitter/src/lowering_pass.rs` - this capture detection
- `crates/tsz-emitter/src/emitter/expressions.rs` - arrow function emission

### 3. Temp Variable Naming in Function Scopes 🟡 MEDIUM PRIORITY

**Problem:** Temp variable names don't reset in new function scopes.

**Example (ES5For-of19):**
```typescript
for (let v of []) {
    function foo() {
        for (let v of []) {}  // New function scope
    }
}
```

**Expected:**
- Outer loop: `_i`, `_a`
- Inner function loop: `_i`, `_a` (can reuse in new scope)

**Our output:**
- Outer loop: `_i`, `_a`
- Inner function loop: `_b`, `_c` (doesn't reset counter)

**Implementation needed:**
1. Reset temp variable counter when entering function scope
2. Use scope stack to save/restore naming state
3. Infrastructure partially exists: `temp_scope_stack`

**Code location:** `crates/tsz-emitter/src/emitter/mod.rs:257-258`

## Architecture Notes

### Existing Infrastructure

**Printer struct fields relevant to slice 4:**
```rust
pub struct Printer<'a> {
    /// All identifiers in file (for collision detection)
    file_identifiers: FxHashSet<String>,

    /// Generated temp names (_a, _b, _c, etc.)
    generated_temp_names: FxHashSet<String>,

    /// Stack for function scope temp naming
    temp_scope_stack: Vec<(u32, FxHashSet<String>, bool)>,

    /// First for-of gets special _i name
    first_for_of_emitted: bool,

    /// Hoisted temps for assignment destructuring
    hoisted_assignment_temps: Vec<String>,
}
```

### Transform Directives

The lowering pass already marks arrow functions for this capture:
- `TransformDirective::SubstituteThis` marks nodes to replace `this`
- Currently generates IIFE pattern
- Need to change to emit `var _this = this` at function level

## Impact Analysis

### Variable Shadowing Fix
- **Estimated tests fixed:** 5-10
- **Complexity:** High (need scope tracking)
- **Risk:** Medium (new infrastructure)

### This Capture Pattern Fix
- **Estimated tests fixed:** 20-40
- **Complexity:** Medium (change existing pattern)
- **Risk:** Medium (changes arrow function emit)

### Temp Variable Naming Fix
- **Estimated tests fixed:** 3-5
- **Complexity:** Low (use existing scope stack)
- **Risk:** Low (infrastructure exists)

## Recommended Approach

### Phase 1: Temp Variable Naming (Quick Win)
1. Implement scope stack push/pop in function emission
2. Reset temp counter when entering function
3. Test with ES5For-of19

### Phase 2: This Capture Pattern
1. Change arrow function emission from IIFE to simple function
2. Detect functions containing arrow functions with `this`
3. Emit `var _this = this;` at function start
4. Replace `this` with `_this` in arrow bodies
5. Test with arrowFunctionContexts, arrowFunctionExpressions

### Phase 3: Variable Shadowing
1. Track declared variable names in current function scope
2. Check for conflicts when emitting for-of bindings
3. Add _1, _2 suffixes when needed
4. Test with ES5For-of15-17, ES5For-of24

## Other Slice 4 Issues

### Helper Functions Status
- ✅ `__values` - Working (after downlevelIteration fix)
- ✅ `__extends` - Working
- ✅ `__spreadArray` - Working
- ⚠️ `__read` - Need to verify for destructuring

### Minor Issues
- Formatting: space before `catch` and `finally` (slice 2)
- Object literal parentheses in arrow returns (slice 2)

---

**Last Updated:** 2026-02-12
**Next Steps:** Implement Phase 1 (temp variable naming) as quick win
