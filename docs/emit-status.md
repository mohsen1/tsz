# Emit Test Status - Slice 4 (Helper Functions + this capture)

## Progress Update

### ✅ Completed: __read Helper Emission
- Fixed detection of binding patterns in for-of initializers  
- Properly checks VARIABLE_DECLARATION_LIST → declarations → binding patterns
- Sets `helpers.read = true` when both `target_es5` and `downlevel_iteration` enabled
- Helper is now correctly emitted in output

**Commit**: fix(emit): emit __read helper for for-of destructuring with downlevelIteration

### ❌ Remaining: Destructuring Lowering
The __read helper is emitted, but destructuring patterns are NOT yet lowered.

**Current output**:
```javascript
var __read = (this && this.__read) || function (o, n) { ... };
...
for (var _b = __values([2, 3]), _c = _b.next(); !_c.done; _c = _b.next()) {
    var [a = 0, b = 1] = _c.value;  // ❌ NOT LOWERED
    ...
}
```

**Expected output**:
```javascript
var __read = (this && this.__read) || function (o, n) { ... };
...
for (var _b = __values([2, 3]), _c = _b.next(); !_c.done; _c = _b.next()) {
    var _d = __read(_c.value, 2), _e = _d[0], a = _e === void 0 ? 0 : _e, _f = _d[1], b = _f === void 0 ? 1 : _f;  // ✅ LOWERED
    ...
}
```

### Implementation Notes

The transformation happens in `crates/tsz-emitter/src/emitter/es5_bindings.rs:1639` in function `emit_for_of_value_binding_iterator_es5()`.

Current code at line 1664:
```rust
self.emit(decl.name);  // Emits binding pattern as-is
self.write(" = ");
self.write(result_name);
self.write(".value");
```

Needs to become:
1. Check if `decl.name` is an ARRAY_BINDING_PATTERN or OBJECT_BINDING_PATTERN
2. If yes, emit lowered destructuring using __read:
   - `var _d = __read(_c.value, N)` where N is element count
   - Then emit individual bindings: `_e = _d[0], a = _e === void 0 ? 0 : _e, ...`
3. If no, emit as current (simple identifier)

### Test Impact
- ES5For-of36: Helper emitted ✅, destructuring NOT lowered ❌  
- ES5For-of37: Similar pattern
- Other for-of tests with destructuring: Partial progress

### Next Steps
1. Implement `emit_es5_destructuring_with_read()` helper
2. Handle array binding patterns with defaults
3. Handle object binding patterns  
4. Handle nested patterns
5. Test with all ES5For-of test cases

