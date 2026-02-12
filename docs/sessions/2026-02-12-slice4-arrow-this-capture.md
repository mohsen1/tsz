# Slice 4: Arrow Function `this` Capture Fix

## Session: 2026-02-12

### Problem

Our ES5 arrow function transform uses IIFE parameter passing:
```javascript
// Our current output (WRONG):
function Greeter() {
    foo((function (_this) {
        return function () {
            bar((function (_this) {
                return function () {
                    var x = _this;
                };
            })(_this));
        };
    })(this));
}
```

TypeScript uses lexical capture with `var _this = this;` at function start:
```javascript
// Expected output (tsc):
function Greeter() {
    var _this = this;
    foo(function () {
        bar(function () {
            var x = _this;
        });
    });
}
```

### Architecture Analysis

TypeScript's approach:
1. Emit `var _this = this;` ONCE at the START of the containing function/constructor/method
2. Convert arrows to plain `function () {}` (no IIFE wrappers)
3. Substitute `this` → `_this` inside arrow bodies

Current tsz architecture:
- `lowering_pass.rs` tracks `this_capture_level` and marks `SubstituteThis` directives ✅
- `emit_arrow_function_es5` wraps arrows in IIFE `(function (_this) { ... })(this)` ❌
- Substitution works but wrapper pattern is wrong ❌

### Implementation Plan

#### Step 1: Simplify Arrow Emission (Remove IIFE Wrapper)

**File**: `crates/tsz-emitter/src/emitter/es5_helpers.rs`

Current `emit_arrow_function_es5` (lines 706-828):
- Wraps in IIFE when `captures_any && !use_class_alias_capture`
- Has complex logic for simple vs non-simple captures

**Changes needed**:
- Remove all IIFE wrapper logic (lines 720-734, 808-827)
- Always emit plain `function () {}` for arrows
- Keep async function handling
- Keep parameter transform logic
- Simplify: remove `is_simple_capture` special case

#### Step 2: Track Containing Functions

**File**: `crates/tsz-emitter/src/lowering_pass.rs`

Add to `LoweringPass` struct (around line 89):
```rust
/// Stack of containing function/constructor/method nodes
/// Used to mark which functions need `var _this = this;` prologue
containing_functions: Vec<NodeIndex>,
```

Modify `visit_arrow_function` (line 1307):
- When arrow captures `this`, mark the top of `containing_functions` stack
- Track which functions need `_this` capture

Add push/pop in:
- `visit_function_declaration`
- `visit_function_expression`
- `visit_constructor` (already visits body, just needs stack tracking)
- `visit_method`

#### Step 3: Add Directive for Functions Needing Capture

**File**: `crates/tsz-emitter/src/transform_context.rs`

Add after `SubstituteArguments` (around line 262):
```rust
/// Mark function/constructor/method that needs `var _this = this;` at body start
/// because it contains arrow functions that capture `this`.
///
/// Example:
/// ```typescript
/// function outer() {
///     const f = () => this.x;
/// }
/// ```
///
/// Becomes:
/// ```javascript
/// function outer() {
///     var _this = this;  // ← Emitted by this directive
///     var f = function () { return _this.x; };
/// }
/// ```
FunctionNeedsThisCapture {
    function_node: NodeIndex,
},
```

Also add corresponding `EmitDirective` variant in `emitter/mod.rs`.

#### Step 4: Emit `var _this = this;` Prologue

**File**: `crates/tsz-emitter/src/emitter/functions.rs`

Add new function:
```rust
pub(super) fn emit_block_with_this_capture_prologue(
    &mut self,
    block_idx: NodeIndex,
    also_has_param_prologue: bool,
) {
    // Similar to emit_block_with_param_prologue
    // Emits: var _this = this;
}
```

Modify block emission in:
- `emit_method_es5` (already calls `emit_block_with_param_prologue`)
- `emit_function_expression_es5_params`
- Constructor emission in `es5_helpers.rs`

Check for `FunctionNeedsThisCapture` directive and emit prologue.

### Test Cases

#### Simple Case
```typescript
function outer() {
    const arrow = () => this;
}
```

Expected:
```javascript
function outer() {
    var _this = this;
    var arrow = function () { return _this; };
}
```

#### Nested Arrows
```typescript
function outer() {
    foo(() => {
        bar(() => {
            var x = this;
        });
    });
}
```

Expected:
```javascript
function outer() {
    var _this = this;
    foo(function () {
        bar(function () {
            var x = _this;
        });
    });
}
```

#### Constructor with Super
```typescript
class RegisteredUser extends User {
    constructor() {
        super();
        var x = () => super.sayHello();
    }
}
```

Expected (from TypeScript baseline):
```javascript
function RegisteredUser() {
    var _this = _super.call(this) || this;
    var x = function () { return _super.prototype.sayHello.call(_this); };
    return _this;
}
```

Note: Constructor already has `var _this = ...` from super call, so prologue logic needs to handle this case.

### Files Modified

1. `crates/tsz-emitter/src/transform_context.rs` - Add `FunctionNeedsThisCapture` directive
2. `crates/tsz-emitter/src/lowering_pass.rs` - Track containing functions, mark when needed
3. `crates/tsz-emitter/src/emitter/mod.rs` - Add `EmitDirective::FunctionNeedsThisCapture`
4. `crates/tsz-emitter/src/emitter/es5_helpers.rs` - Simplify `emit_arrow_function_es5`, remove IIFE
5. `crates/tsz-emitter/src/emitter/functions.rs` - Emit `var _this = this;` prologue

### Edge Cases to Handle

1. **Constructors with super**: Already have `var _this = _super.call(this) || this;`
   - Don't emit duplicate `var _this = this;`
   - Super call creates the _this binding

2. **Nested functions**: Each function scope needs its own `_this` if it has arrows
   - But TypeScript doesn't rename - uses same `_this` name
   - Shadowing is intentional

3. **Static class members**: Already handled via `class_alias`
   - Keep that logic working

4. **Arguments capture**: Similar pattern needed for `_arguments`
   - Can be done in parallel or as follow-up

### Next Steps

1. Complete Step 1: Simplify `emit_arrow_function_es5` to remove IIFE wrapper
2. Build and test: `cargo build --release -p tsz-cli`
3. Test with simple case: `.target/release/tsz --noCheck --noLib --target es5 tmp/simple-this-test.ts`
4. Implement Steps 2-4 for complete fix
5. Run emit tests: `./scripts/emit/run.sh --js-only --filter="this" --max=20`
6. Verify variable shadowing tests pass (ES5For-of15, ES5For-of16, etc.)

### Related Issues

This also relates to Slice 4's variable shadowing issues in for-of loops:
- Tests like `ES5For-of15` expect `v_1` for shadowed variables
- Need to ensure variable renaming logic works with new `_this` capture approach

### References

- TypeScript baseline: `TypeScript/tests/baselines/reference/badThisBinding.js`
- TypeScript baseline: `TypeScript/tests/baselines/reference/superInLambdas.js`
- Current implementation: `crates/tsz-emitter/src/emitter/es5_helpers.rs:706`
- Lowering pass: `crates/tsz-emitter/src/lowering_pass.rs:1307`
