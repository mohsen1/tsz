# Emit 100% Pass Rate — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Achieve 100% pass rate for both JS emit (currently 76.6%) and DTS emit (currently 53.7%) tests.

**Architecture:** Transform-Heavy → Sweep strategy. Implement missing language transforms (JSX, private fields, rest, decorators) to unblock test groups, fix module detection as systemic root cause, improve DTS type inference in parallel, then sweep comment/formatting diffs. All transforms live in `crates/tsz-emitter/src/transforms/`, DTS in `declaration_emitter/`, module detection split between emitter and CLI.

**Tech Stack:** Rust (tsz-emitter crate), TypeScript (emit test runner), Python (analysis scripts). Tests run via `./scripts/emit/run.sh`.

---

## Measurement Commands

```bash
# Build (required before test runs)
CARGO_TARGET_DIR=.target cargo build --profile dist-fast -p tsz-cli --bin tsz

# JS emit tests (full)
TSZ_BIN=.target/dist-fast/tsz ./scripts/emit/run.sh --js-only --skip-build

# DTS emit tests (full)
TSZ_BIN=.target/dist-fast/tsz ./scripts/emit/run.sh --dts-only --skip-build

# Targeted test (fast, seconds)
TSZ_BIN=.target/dist-fast/tsz ./scripts/emit/run.sh --filter "pattern" --verbose --skip-build

# Unit tests
cargo test -p tsz-emitter
```

---

## Phase 1: JSX Transform (95 exclusive JS tests)

### Task 1.1: Add JSX options to PrinterOptions

**Files:**
- Modify: `crates/tsz-emitter/src/emitter/core.rs` (PrinterOptions struct, ~line 19-52)
- Modify: `crates/tsz-emitter/src/context/emit.rs` (EmitContext)

**Step 1: Add JSX fields to PrinterOptions**

In `crates/tsz-emitter/src/emitter/core.rs`, add to the `PrinterOptions` struct:

```rust
pub jsx: JsxEmit,
pub jsx_factory: Option<String>,
pub jsx_fragment_factory: Option<String>,
pub jsx_import_source: Option<String>,
```

Add the `JsxEmit` enum (or import from `tsz-common`):

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum JsxEmit {
    #[default]
    Preserve = 0,
    React = 1,
    ReactJsx = 2,
    ReactJsxDev = 3,
    ReactNative = 4,
}
```

**Step 2: Wire JSX options from CLI to Printer**

In `crates/tsz-cli/src/driver/emit.rs`, pass JSX options when constructing `PrinterOptions`:

```rust
let printer_options = PrinterOptions {
    // ... existing fields ...
    jsx: context.options.jsx.unwrap_or_default(),
    jsx_factory: context.options.jsx_factory.clone(),
    jsx_fragment_factory: context.options.jsx_fragment_factory.clone(),
    jsx_import_source: context.options.jsx_import_source.clone(),
};
```

**Step 3: Run tests to verify nothing breaks**

```bash
cargo test -p tsz-emitter
TSZ_BIN=.target/dist-fast/tsz ./scripts/emit/run.sh --js-only --skip-build --max=100
```

Expected: Same pass count as before (no regression).

**Step 4: Commit**

```bash
git add crates/tsz-emitter/src/emitter/core.rs crates/tsz-cli/src/driver/emit.rs crates/tsz-emitter/src/context/emit.rs
git commit -m "feat(emitter): add JSX options to PrinterOptions"
```

---

### Task 1.2: Implement `jsx=react` classic transform

**Files:**
- Modify: `crates/tsz-emitter/src/emitter/jsx.rs` (main transform logic)
- Modify: `crates/tsz-emitter/src/emitter/core.rs` (dispatch routing)
- Test: `crates/tsz-emitter/tests/jsx_transform.rs` (new test file)

**Step 1: Write failing tests**

Create `crates/tsz-emitter/tests/jsx_transform.rs`:

```rust
// Test: JSX element → React.createElement
// Input: <div />
// Expected: React.createElement("div", null)

// Test: JSX element with props
// Input: <div id="test" />
// Expected: React.createElement("div", { id: "test" })

// Test: JSX element with children
// Input: <div>hello</div>
// Expected: React.createElement("div", null, "hello")

// Test: JSX fragment
// Input: <>hello</>
// Expected: React.createElement(React.Fragment, null, "hello")

// Test: JSX with spread props
// Input: <div {...props} />
// Expected: React.createElement("div", props)

// Test: JSX component (uppercase)
// Input: <MyComponent />
// Expected: React.createElement(MyComponent, null)

// Test: Custom factory
// Input: <div /> with jsxFactory="h"
// Expected: h("div", null)
```

**Step 2: Run tests to verify they fail**

```bash
cargo test -p tsz-emitter jsx_transform -- --nocapture
```

Expected: FAIL (transform not implemented).

**Step 3: Implement the JSX classic transform**

In `crates/tsz-emitter/src/emitter/jsx.rs`, add transform methods:

The core logic: when `self.ctx.options.jsx == JsxEmit::React`:
- `emit_jsx_element` → call `emit_jsx_element_as_create_element`
- `emit_jsx_self_closing_element` → call `emit_jsx_self_closing_as_create_element`
- `emit_jsx_fragment` → call `emit_jsx_fragment_as_create_element`

Each converts to: `factory(tag, props, ...children)` where:
- `factory` = `self.ctx.options.jsx_factory.as_deref().unwrap_or("React.createElement")`
- `tag` = `"div"` for intrinsic elements (lowercase), `ComponentName` for components (uppercase)
- `props` = `null` if no attributes, else object literal from attributes
- `children` = remaining arguments after props

Key patterns to handle:
1. **Intrinsic elements** (lowercase tag): emit tag name as string literal
2. **Component elements** (uppercase/dotted): emit tag name as identifier
3. **Props → object literal**: `{ key: value, key2: value2 }`
4. **Spread attributes**: `Object.assign({}, props, { key: value })`
5. **Children**: string text → string literal, expressions → expression, nested JSX → recursive
6. **Fragment**: use `React.Fragment` or custom fragment factory
7. **Key prop**: tsc passes `key` inside the props object for classic transform

**Step 4: Route JSX dispatch in core.rs**

In the emit dispatcher (core.rs ~line 1052), check `self.ctx.options.jsx`:

```rust
k if k == syntax_kind_ext::JSX_ELEMENT => {
    if self.ctx.options.jsx == JsxEmit::React {
        self.emit_jsx_element_as_create_element(node)
    } else {
        self.emit_jsx_element(node)
    }
},
```

**Step 5: Run tests and iterate**

```bash
cargo test -p tsz-emitter jsx_transform -- --nocapture
TSZ_BIN=.target/dist-fast/tsz ./scripts/emit/run.sh --filter "jsx" --js-only --verbose --skip-build
```

Expected: Unit tests pass. Targeted emit tests show improvement.

**Step 6: Commit**

```bash
git commit -m "feat(emitter): implement jsx=react classic transform"
```

---

### Task 1.3: Implement `jsx=react-jsx` automatic transform

**Files:**
- Modify: `crates/tsz-emitter/src/emitter/jsx.rs`
- Modify: `crates/tsz-emitter/src/emitter/source_file.rs` (auto-import injection)

**What**: When `jsx=react-jsx`, transform JSX to `_jsx()` / `_jsxs()` calls and auto-inject the import at file top:
- `import { jsx as _jsx, jsxs as _jsxs, Fragment as _Fragment } from "react/jsx-runtime"`
- Or for CJS: `const jsx_runtime_1 = require("react/jsx-runtime");` then `(0, jsx_runtime_1.jsx)(...)`

Key differences from classic transform:
- Uses `_jsx` / `_jsxs` instead of `createElement`
- `_jsxs` is used when element has multiple children (array)
- `_jsx` is used for 0 or 1 child
- Children go inside the props object as `{ children: ... }` not as extra arguments
- `key` is extracted from props and passed as third argument to `_jsx`
- Auto-import from `{jsxImportSource}/jsx-runtime`

**Testing:**
```bash
TSZ_BIN=.target/dist-fast/tsz ./scripts/emit/run.sh --filter "reactJsx\|react-jsx\|jsxImportSource" --js-only --verbose --skip-build
```

---

## Phase 2: Private Fields Access Transform (30 exclusive JS tests)

### Task 2.1: Transform `this.#field` reads to `__classPrivateFieldGet`

**Files:**
- Modify: `crates/tsz-emitter/src/transforms/class_es5_ast_to_ir.rs` (~line 669-697, `convert_property_access`)
- Test: `crates/tsz-emitter/tests/private_fields_es5.rs`

**What**: Currently `convert_property_access` does not detect private identifiers. When the property name is a `PrivateIdentifier`, convert to `PrivateFieldGet` IR node instead of `PropertyAccess`.

**Step 1: Write failing test**

Add to `tests/private_fields_es5.rs` or `tests/class_es5.rs`:
```rust
// Input: class C { #x = 1; get() { return this.#x; } }
// Expected in output: __classPrivateFieldGet(this, _C_x, "f")
```

**Step 2: Implement in `convert_property_access`**

After getting the name, check `is_private_identifier(self.arena, access.name_or_argument)`. If true, look up the WeakMap name from `self.private_fields` and emit `IRNode::PrivateFieldGet`.

**Step 3: Run targeted tests**

```bash
TSZ_BIN=.target/dist-fast/tsz ./scripts/emit/run.sh --filter "privateField\|privateName\|classPrivate" --js-only --verbose --skip-build
```

---

### Task 2.2: Transform `this.#field = value` writes to `__classPrivateFieldSet`

**Files:**
- Modify: `crates/tsz-emitter/src/transforms/class_es5_ast_to_ir.rs` (assignment expression handling)

**What**: In assignment expressions where the LHS is a property access with a private identifier, emit `PrivateFieldSet` IR node instead of regular assignment.

---

### Task 2.3: Transform `#field in obj` to `__classPrivateFieldIn`

**Files:**
- Modify: `crates/tsz-emitter/src/transforms/class_es5_ast_to_ir.rs` (binary expression handling)
- Modify: `crates/tsz-emitter/src/transforms/ir.rs` (add `PrivateFieldIn` IR node if missing)
- Modify: `crates/tsz-emitter/src/transforms/ir_printer.rs` (print the new node)

---

## Phase 3: `__rest` Helper Gaps (54 exclusive JS tests)

### Task 3.1: Audit and fix `__rest` emission in variable destructuring

**Files:**
- Modify: `crates/tsz-emitter/src/emitter/es5/bindings_patterns.rs`
- Modify: `crates/tsz-emitter/src/lowering/helpers.rs` (detection)
- Modify: `crates/tsz-emitter/src/lowering/core.rs` (marking)

**What**: The `__rest` helper is already defined and detection exists. The gap is likely in:
1. Variable-level object rest (not just function parameters)
2. Nested destructuring patterns with rest
3. Assignment destructuring with rest
4. For-of destructuring with rest

**Step 1: Run targeted tests to find specific failures**

```bash
TSZ_BIN=.target/dist-fast/tsz ./scripts/emit/run.sh --filter "rest\|spread\|destructur" --js-only --verbose --skip-build --max=50
```

**Step 2: Categorize failures and fix each pattern**

Each sub-pattern likely needs rest detection wired into the lowering pass AND the rest exclude list correctly computed in the emitter.

---

## Phase 4: Module Detection Fix (~200+ exclusive JS tests)

### Task 4.1: Fix `.mjs`/`.mts` extension detection in emit test runner

**Files:**
- Modify: `scripts/emit/src/cli-transpiler.ts`
- Modify: `crates/tsz-cli/src/driver/emit.rs`

**What**: The CLI already has `implied_resolution_mode_for_file()` in `driver/resolution.rs` that checks `.mts`/`.mjs` extensions. But the emit test runner may not pass the file extension correctly, or the per-file override in `emit.rs` (lines 72-82) may not fire for all test configurations.

**Step 1: Run targeted tests to identify module misdetection**

```bash
TSZ_BIN=.target/dist-fast/tsz ./scripts/emit/run.sh --filter "nodeModules\|moduleDetection" --js-only --verbose --skip-build --max=50
```

**Step 2: Fix extension-to-module-kind mapping**

Ensure `.mjs`/`.mts` → ESNext and `.cjs`/`.cts` → CommonJS in all code paths.

---

### Task 4.2: Fix `moduleDetection=auto` for ESM indicators

**Files:**
- Modify: `crates/tsz-emitter/src/emitter/module_emission/core.rs`
- Modify: `crates/tsz-emitter/src/lowering/helpers.rs`

**What**: When `moduleDetection=auto`, tsc detects ESM from:
- Top-level `import`/`export` statements (already handled)
- `import.meta` usage (partially handled)
- Top-level `await` (NOT handled)
- Package.json `"type": "module"` scope (handled in CLI, not in emitter)

Fix: Add top-level `await` detection in `file_is_module()` and `should_emit_es_module_marker()`.

---

### Task 4.3: Fix extra `__esModule` / `"use strict"` / CJS helper over-emission

**Files:**
- Modify: `crates/tsz-emitter/src/emitter/source_file.rs`
- Modify: `crates/tsz-emitter/src/emitter/module_emission/core.rs`

**What**: When a file IS an ES module but we treat it as CJS, we over-emit:
- `"use strict";` (redundant in ESM)
- `Object.defineProperty(exports, "__esModule", { value: true });`
- `__createBinding` / `__importStar` helpers

Fix: Improve `is_es_module_output` detection so ESM files don't get CJS treatment.

---

## Phase 5: DTS Command Crash Fixes (44 DTS tests)

### Task 5.1: Fix DTS test runner binary path

**Files:**
- Modify: `scripts/emit/src/cli-transpiler.ts`

**What**: Some DTS tests use the wrong binary path (`.target/release/tsz` instead of the `TSZ_BIN` env var). Ensure all test invocations use the configured binary.

**Step 1: Verify the issue**

```bash
TSZ_BIN=.target/dist-fast/tsz ./scripts/emit/run.sh --dts-only --skip-build --filter "jsDeclarations" --verbose 2>&1 | head -30
```

**Step 2: Fix binary path propagation**

In `cli-transpiler.ts`, ensure `this.tszBin` is used consistently for ALL invocations, including DTS-specific runs.

---

### Task 5.2: Fix `--allowJs --declaration` panics

**Files:**
- Modify: `crates/tsz-cli/src/driver/emit.rs`
- Modify: `crates/tsz-emitter/src/declaration_emitter/core.rs`

**What**: `--allowJs` with `--declaration` on `.js` inputs causes the CLI to crash. Investigate and fix the specific panic paths.

```bash
.target/dist-fast/tsz --declaration --allowJs --alwaysStrict true --esModuleInterop --target es2015 --module commonjs /tmp/test.js 2>&1
```

---

## Phase 6: DTS Type Inference Improvements (151 DTS tests)

### Task 6.1: Improve initializer type inference in declaration emitter

**Files:**
- Modify: `crates/tsz-emitter/src/declaration_emitter/helpers.rs` (~line 1297, `infer_fallback_type_text`)

**What**: Currently handles ~6 patterns (numeric/string/boolean/null/undefined literals, simple objects). Add:
- Array literals → `type[]` inference
- Template literals → `string`
- Binary ops on known types → result type
- `new ClassName()` → `ClassName`
- Function calls with known return types

**Step 1: Run DTS tests and categorize `any` fallbacks**

```bash
TSZ_BIN=.target/dist-fast/tsz ./scripts/emit/run.sh --dts-only --skip-build --verbose 2>&1 | grep -B5 ": any" | head -100
```

---

### Task 6.2: Improve TypePrinter for complex types

**Files:**
- Modify: `crates/tsz-emitter/src/declaration_emitter/helpers.rs` (print_type_id)
- Modify: `crates/tsz-emitter/src/declaration_emitter/type_emission.rs`

**What**: When `type_interner` is available but the type is complex (union, intersection, mapped, conditional), ensure full serialization rather than falling back to `any`.

---

## Phase 7: DTS Export/Import Handling (305 DTS tests)

### Task 7.1: Add `declare module` wrapping for ambient modules

**Files:**
- Modify: `crates/tsz-emitter/src/declaration_emitter/core.rs`
- Modify: `crates/tsz-emitter/src/declaration_emitter/exports.rs`

**What**: When multiple source files contribute to a DTS output (e.g., AMD bundles), tsc wraps each file's declarations in `declare module "name" { ... }`. The tsz DTS emitter emits them flat.

---

### Task 7.2: Fix re-export and namespace export patterns

**Files:**
- Modify: `crates/tsz-emitter/src/declaration_emitter/exports.rs`

**What**: Handle:
- `export * from "module"` → proper re-export in DTS
- `export { name } from "module"` → named re-exports
- `export = namespace` → CommonJS-style export assignments
- Namespace merging in declaration files

---

## Phase 8: Decorator Metadata (`__metadata`) (38 exclusive JS tests)

### Task 8.1: Implement `emitDecoratorMetadata` support

**Files:**
- Modify: `crates/tsz-emitter/src/transforms/class_es5_ir.rs`
- Modify: `crates/tsz-emitter/src/transforms/helpers.rs` (add METADATA_HELPER)
- Modify: `crates/tsz-emitter/src/lowering/helpers.rs` (detection)

**What**: When `emitDecoratorMetadata=true` and `experimentalDecorators=true`, emit:
```javascript
__metadata("design:type", Function),
__metadata("design:paramtypes", [String, Number]),
__metadata("design:returntype", void 0)
```

These appear inside `__decorate([...], ...)` calls, alongside `__param(N, decorator)` entries.

Key: Requires type serialization — converting TypeScript types to runtime type references:
- `string` → `String`
- `number` → `Number`
- `boolean` → `Boolean`
- `void` → `void 0`
- Class types → class identifier
- Interfaces → `Object`
- Arrays → `Array`
- Function types → `Function`

---

## Phase 9: Comment Handling (835 exclusive JS tests)

### Task 9.1: Fix trailing comment association with next-sibling cap

**Files:**
- Modify: `crates/tsz-emitter/src/emitter/statements.rs`
- Modify: `crates/tsz-emitter/src/emitter/source_file.rs`
- Modify: `crates/tsz-emitter/src/emitter/comments/comment_helpers.rs`

**What**: When emitting trailing comments for a statement, the scanner can overshoot past the next sibling statement and steal its comments. Fix: pass `next_sibling.pos` as an upper bound to `emit_trailing_comments`.

**Step 1: Run targeted tests**

```bash
TSZ_BIN=.target/dist-fast/tsz ./scripts/emit/run.sh --filter "comment\|Comments" --js-only --verbose --skip-build --max=50
```

---

### Task 9.2: Fix comment preservation through ES5 class transforms

**Files:**
- Modify: `crates/tsz-emitter/src/transforms/class_es5_ir.rs`
- Modify: `crates/tsz-emitter/src/transforms/ir_printer.rs`

**What**: Comments above class members are dropped during ES5 IIFE lowering. Need to track and re-emit comments from original source positions in the IR output.

---

### Task 9.3: Fix erased-declaration comment boundaries

**Files:**
- Modify: `crates/tsz-emitter/src/emitter/source_file.rs`

**What**: When type-only declarations (interfaces, type aliases) are erased, their surrounding comments need proper handling:
- Comments BEFORE erased decl: preserve as standalone
- Comments AFTER erased decl on same line: assign to next non-erased statement
- Comments BETWEEN consecutive erased decls: skip entirely

---

## Phase 10: Export/Import Pattern Fixes (307 JS tests)

### Task 10.1: Fix CJS `exports.X` reference rewriting

**Files:**
- Modify: `crates/tsz-emitter/src/emitter/module_emission/exports.rs`
- Modify: `crates/tsz-emitter/src/emitter/expressions/identifier.rs`

**What**: When a variable is exported via `exports.X = ...` in CommonJS, references to that variable within the same module should also use `exports.X` rather than the bare local name. Requires tracking which variables are exported.

---

### Task 10.2: Fix anonymous default export naming

**Files:**
- Modify: `crates/tsz-emitter/src/emitter/module_emission/exports.rs`
- Modify: `crates/tsz-emitter/src/emitter/declarations/class.rs`

**What**: tsc names anonymous `export default class` as `default_1`, `default_2`, etc. tsz uses `_a_default` or leaves unnamed. Implement sequential `default_N` naming.

---

### Task 10.3: Fix `export {}` sentinel logic

**Files:**
- Modify: `crates/tsz-emitter/src/emitter/source_file.rs`

**What**: `export {};` should be emitted when all imports/exports were type-only erased and the file needs to remain a module. Currently over/under-emitted in various cases.

---

## Phase 11: Expression/Statement Formatting (456 JS tests)

### Task 11.1: Fix semicolon edge cases (118 tests)

**Files:**
- Modify: `crates/tsz-emitter/src/emitter/statements.rs`
- Modify: `crates/tsz-emitter/src/emitter/declarations/class_members.rs`

**What**: Various patterns where semicolons are over/under-emitted:
- Missing semicolons after class member declarations
- Extra semicolons after transformed statements
- Missing ASI semicolons in ES5 lowered output

---

### Task 11.2: Fix empty body formatting `{ }` vs `{}` (60 tests)

**Files:**
- Modify: `crates/tsz-emitter/src/emitter/statements.rs`

**What**: tsc always normalizes empty blocks to `{ }` (with space) for most contexts. tsz sometimes emits `{}`. Ensure consistent `{ }` formatting.

---

### Task 11.3: Fix parenthesization edge cases (28+ tests)

**Files:**
- Modify: `crates/tsz-emitter/src/emitter/expressions/binary.rs`
- Modify: `crates/tsz-emitter/src/emitter/expressions/unary.rs`

**What**: Various operator contexts where parentheses are missing or extra. Audit by running:

```bash
TSZ_BIN=.target/dist-fast/tsz ./scripts/emit/run.sh --filter "paren\|precedence" --js-only --verbose --skip-build
```

---

## Phase 12: TC39 Decorators (`__esDecorate`) (217 tests, 6 exclusive)

### Task 12.1: Add TC39 decorator helper definitions

**Files:**
- Modify: `crates/tsz-emitter/src/transforms/helpers.rs`

**What**: Add the 4 TC39 decorator helper constants:
- `ES_DECORATE_HELPER` (~35 lines, from tsc's `emitHelpers.ts`)
- `RUN_INITIALIZERS_HELPER` (~7 lines)
- `PROP_KEY_HELPER` (~5 lines)
- `SET_FUNCTION_NAME_HELPER` (~5 lines)

Add fields to `HelpersNeeded`:
```rust
pub es_decorate: bool,
pub run_initializers: bool,
pub prop_key: bool,
pub set_function_name: bool,
```

---

### Task 12.2: Implement TC39 decorator transform for class declarations

**Files:**
- Create: `crates/tsz-emitter/src/transforms/es_decorators.rs`
- Modify: `crates/tsz-emitter/src/transforms/mod.rs`
- Modify: `crates/tsz-emitter/src/lowering/core.rs`

**What**: This is the largest single task. The TC39 decorator transform:

1. Converts `@dec class C { @dec method() {} }` to:
   ```javascript
   let C = (() => {
       let _classDecorators = [dec];
       let _classDescriptor;
       let _classExtraInitializers = [];
       let _classThis;
       let _instanceExtraInitializers = [];
       let _method_decorators;
       var C = class C {
           constructor() {
               __runInitializers(this, _instanceExtraInitializers);
           }
           method() {}
       };
       // ... __esDecorate calls for each member ...
       // ... __esDecorate call for the class ...
       return C = _classThis;
   })();
   ```

2. Each decorated member gets an `__esDecorate(...)` call
3. The class itself gets an `__esDecorate(...)` call if decorated
4. Initializers are collected and run via `__runInitializers`

**Approach**: Create new transform file, model after tsc's `esDecorators.ts`.

---

### Task 12.3: Wire TC39 decorator detection into lowering pass

**Files:**
- Modify: `crates/tsz-emitter/src/lowering/core.rs`
- Modify: `crates/tsz-emitter/src/lowering/helpers.rs`

**What**: Detect when a class has non-legacy decorators (i.e., `legacy_decorators=false` and decorators present) and set:
- `helpers.es_decorate = true`
- `helpers.run_initializers = true`
- `helpers.set_function_name = true` (if needed)
- Create appropriate transform directives

---

## Phase 13: `__awaiter`/`__generator` Improvements (28 exclusive JS tests)

### Task 13.1: Fix async parameter default hoisting

**Files:**
- Modify: `crates/tsz-emitter/src/transforms/async_es5*.rs`

**What**: When async functions have default parameters (`async function f(a = await expr)`), tsc hoists the defaults into the `__awaiter` wrapper differently than tsz.

---

### Task 13.2: Improve generator state machine

**Files:**
- Modify: `crates/tsz-emitter/src/transforms/async_es5*.rs`

**What**: Generator functions lowered to ES5 require a state machine pattern with `__generator`. Improve coverage of edge cases (try/catch/finally in generators, nested yields).

---

## Phase 14: Long Tail Cleanup

### Task 14.1: Fix `var`/`let`/`const` keyword matching (31 tests)

**What**: ES5 transform should convert `let`/`const` to `var`. Verify all paths do this.

### Task 14.2: Fix numeric literal normalization

**What**: tsc normalizes `0888` → `888`, `0777` → `511`, `1_000` → `1000`.

### Task 14.3: Fix optional chain continuation

**What**: `obj?.a.b++` should lower the full chain, not just `?.a`.

### Task 14.4: Fix `static {}` block transforms for ES2021-

**What**: Class static blocks need a transform for targets below ES2022.

---

## Progress Checkpoints

After each task, run the relevant test suite and record the pass count:

| Checkpoint | Expected JS | Expected DTS |
|------------|-------------|--------------|
| Baseline (current) | 10,290 (76.6%) | 783 (53.7%) |
| After Phase 1 (JSX) | ~10,385 (+95) | 783 |
| After Phase 2 (Private fields) | ~10,415 (+30) | 783 |
| After Phase 3 (__rest) | ~10,469 (+54) | 783 |
| After Phase 4 (Module detection) | ~10,669 (+200) | 783 |
| After Phase 5 (DTS crashes) | ~10,669 | ~827 (+44) |
| After Phase 6 (DTS inference) | ~10,669 | ~978 (+151) |
| After Phase 7 (DTS exports) | ~10,669 | ~1,283 (+305) |
| After Phase 8 (__metadata) | ~10,707 (+38) | ~1,283 |
| After Phase 9 (Comments) | ~11,542 (+835) | ~1,283 |
| After Phase 10 (Exports) | ~11,849 (+307) | ~1,283 |
| After Phase 11 (Formatting) | ~12,305 (+456) | ~1,283 |
| After Phase 12 (TC39 decorators) | ~12,522 (+217) | ~1,283 |
| After Phase 13 (Async/gen) | ~12,550 (+28) | ~1,283 |
| After Phase 14 (Long tail) | 13,427 (100%) | 1,457 (100%) |

Note: These are optimistic estimates. Some tests overlap categories, so actual gains will be lower per-phase but cumulative.
