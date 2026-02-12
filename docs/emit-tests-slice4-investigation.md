# Emit Tests - Slice 4 Investigation
## Session: 2026-02-12

### Slice 4 Assignment
**Helper functions + this capture**
- `__values`, `__read`, `__spread` helpers not emitted for ES5
- `_this = this` capture for arrow functions inside methods
- Regex literals preservation

### Test Results (ES5For-of filters)

Ran 51 tests, found two main failure categories:

#### 1. Variable Renaming (7 failures)
**Tests**: ES5For-of15, ES5For-of16, ES5For-of17, ES5For-of19, ES5For-of20, ES5For-of24, ES5For-of31

**Issue**: TypeScript adds `_1`, `_2`, `_3` suffixes to shadowed variables, we don't.

**Example** (ES5For-of15):
```typescript
for (var v of []) {
    v;
    for (var v of []) {  // Inner v shadows outer v
        var x = v;
    }
}
```

**Expected**:
```javascript
for (var _i = 0, _a = []; _i < _a.length; _i++) {
    var v = _a[_i];
    v;
    for (var _b = 0, _c = []; _b < _c.length; _b++) {
        var v_1 = _c[_b];  // ← _1 suffix for shadowed variable
        var x = v_1;
    }
}
```

**We emit**:
```javascript
for (var _i = 0, _a = []; _i < _a.length; _i++) {
    var v = _a[_i];
    v;
    for (var _b = 0, _c = []; _b < _c.length; _b++) {
        var v = _c[_b];  // ← No suffix!
        var x = v;
    }
}
```

**Impact**: 7 tests (all minor variants of the same issue)

**Fix Location**: Variable declaration lowering in `crates/tsz-emitter/src/lowering_pass.rs`

**Complexity**: Medium - needs scope tracking to detect shadowing

---

#### 2. __values Helper Missing (1 failure, HIGH PRIORITY)
**Test**: ES5For-of33

**Issue**: When `downlevelIteration: true`, TypeScript emits the `__values` helper and uses iterator protocol with try-catch-finally. We emit a simple for loop.

**Example**:
```typescript
//@downlevelIteration: true
for (var v of ['a', 'b', 'c']) {
    console.log(v);
}
```

**Expected** (91 lines!):
```javascript
var __values = (this && this.__values) || function(o) {
    var s = typeof Symbol === "function" && Symbol.iterator, m = s && o[s], i = 0;
    if (m) return m.call(o);
    if (o && typeof o.length === "number") return {
        next: function () {
            if (o && i >= o.length) o = void 0;
            return { value: o && o[i++], done: !o };
        }
    };
    throw new TypeError(s ? "Object is not iterable." : "Symbol.iterator is not defined.");
};
var e_1, _a;
try {
    for (var _b = __values(['a', 'b', 'c']), _c = _b.next(); !_c.done; _c = _b.next()) {
        var v = _c.value;
        console.log(v);
    }
}
catch (e_1_1) { e_1 = { error: e_1_1 }; }
finally {
    try {
        if (_c && !_c.done && (_a = _b.return)) _a.call(_b);
    }
    finally { if (e_1) throw e.error; }
}
```

**We emit** (4 lines):
```javascript
for (var _i = 0, _a = ['a', 'b', 'c']; _i < _a.length; _i++) {
    var v = _a[_i];
    console.log(v);
}
```

**Impact**: All downlevelIteration tests (affects proper iterator protocol support)

**Fix Required**:
1. ✅ Helper is already defined: `VALUES_HELPER` in `crates/tsz-emitter/src/transforms/helpers.rs:108`
2. ✅ HelpersNeeded.values field exists: `crates/tsz-emitter/src/transforms/helpers.rs:236`
3. ✅ Transform directive exists: `TransformDirective::ES5ForOf` in `crates/tsz-emitter/src/transform_context.rs:164`
4. ✅ Lowering pass creates directive: `crates/tsz-emitter/src/lowering_pass.rs:663` (when downlevelIteration enabled)
5. ❌ **MISSING**: Emit logic that applies the ES5ForOf transformation

**Root Cause**: The `emit_for_of_statement` function in `crates/tsz-emitter/src/emitter/statements.rs:405` never checks for the `ES5ForOf` directive. It always emits the for-of statement literally.

**Complexity**: **HIGH**
- Must emit try-catch-finally structure
- Must generate temporary variables (e_1, _a, _b, _c)
- Must handle Symbol.iterator protocol
- Must match TypeScript's exact variable naming conventions

---

### Code Architecture Findings

#### Transform Pipeline
1. **Phase 1 - Lowering Pass** (`lowering_pass.rs`):
   - Walks AST
   - Creates `TransformDirective::ES5ForOf` at line 663
   - Stores directives in `TransformContext`

2. **Phase 2 - Emit** (`emitter/statements.rs`):
   - `emit_for_of_statement` at line 405
   - **BUG**: Never checks `TransformContext` for directives!
   - Just emits for-of literally

3. **Helpers** (`transforms/helpers.rs`):
   - `VALUES_HELPER` constant defined (line 108)
   - `HelpersNeeded` struct with `.values` field (line 236)
   - Helper emission happens somewhere (need to find where)

#### Key Files

| File | Purpose | Status |
|------|---------|--------|
| `crates/tsz-emitter/src/lowering_pass.rs:663` | Creates ES5ForOf directive | ✅ Working |
| `crates/tsz-emitter/src/emitter/statements.rs:405` | Emits for-of statement | ❌ Broken |
| `crates/tsz-emitter/src/transforms/helpers.rs:108` | __values helper code | ✅ Exists |
| `crates/tsz-emitter/src/transform_context.rs:164` | ES5ForOf directive type | ✅ Exists |

---

### Implementation Plan

#### Quick Win: Fix Variable Renaming (Estimated: 2-3 hours)
1. Add scope tracking to detect variable shadowing
2. Generate `_1`, `_2`, `_3` suffixes for shadowed variables
3. Apply consistently across for-of, destructuring, etc.
4. Test with ES5For-of15-17, 19-20, 24, 31

#### Complex: Implement ES5ForOf Transform (Estimated: 6-8 hours)
1. **Check for directive** in `emit_for_of_statement`:
   ```rust
   if let Some(TransformDirective::ES5ForOf { for_of_node }) = self.transforms.get(idx) {
       self.emit_for_of_downlevel(node);
       return;
   }
   ```

2. **Implement `emit_for_of_downlevel`**:
   - Generate temp variable names (e_1, _a, _b, _c)
   - Emit try-catch-finally structure
   - Use `__values(expression)` for iterable
   - Emit `for (var _b = __values(arr), _c = _b.next(); !_c.done; _c = _b.next())`
   - Handle iterator return in finally block

3. **Set helper flag**:
   ```rust
   self.helpers_needed.values = true;
   ```

4. **Verify helper emission**:
   - Find where `VALUES_HELPER` is emitted at file top
   - Ensure it triggers when `helpers_needed.values == true`

5. **Test comprehensively**:
   - ES5For-of33 (basic case)
   - Nested for-of loops
   - for-of with destructuring
   - for-await-of statements

---

### Test Coverage

**Total ES5For-of tests**: 51
- **Passing**: 43 (84.3%)
- **Failing**: 8 (15.7%)
  - Variable renaming: 7 tests
  - __values helper: 1 test (but critical)

**Quick wins**: Fixing variable renaming = 7 tests
**High impact**: Fixing __values = Proper downlevelIteration support (many more tests likely affected)

---

### References

- TypeScript Handbook: [Iteration protocols](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Iteration_protocols)
- TypeScript Compiler: [transformGenerators.ts](https://github.com/microsoft/TypeScript/blob/main/src/compiler/transformers/generators.ts)
- Emit test baselines: `TypeScript/tests/baselines/reference/ES5For-of*.js`

---

### Next Steps

1. **Immediate**: Document findings (this file)
2. **Session priority**: Fix variable renaming (achievable in this session)
3. **Next session**: Implement ES5ForOf transformation (complex, needs dedicated focus)
4. **Also investigate**:
   - `_this = this` capture issues
   - `__read`, `__spread` helpers
   - Regex literal preservation

---

### Session Status
- **Time spent**: ~1 hour
- **Tests run**: 51 ES5For-of tests
- **Root cause identified**: ✅ Yes (ES5ForOf directive not applied)
- **Code ready**: ❌ No (needs implementation)
- **Documentation**: ✅ Complete
