# TSZ Emit Gap Analysis — Definitive Report

**Date**: 2026-03-08
**Suite**: 13,527 test variants (TypeScript baseline comparisons)

## Current State

| Metric | Pass Rate | Pass / Total |
|---|---|---|
| **JS Emit** | **81.3%** | 11,003 / 13,526 |
| **DTS Emit** | **59.0%** | 873 / 1,480 |

### Pass Rates by Category

| Category | JS Pass Rate | JS Tests | DTS Pass Rate | DTS Tests |
|---|---|---|---|---|
| System module | 13.7% | 197 | 100% | 6 |
| Decorators | 23.2% | 642 | 100% | 3 |
| ES2017 target | 20.0% | 25 | — | — |
| JSX | 52.6% | 211 | 100% | 2 |
| ES5 target | 53.8% | 1,307 | 69.2% | 146 |
| ESNext target | 53.8% | 253 | 100% | 2 |
| Class | 57.4% | 1,478 | 56.4% | 94 |
| Async | 62.2% | 299 | 100% | 1 |
| Module | 66.6% | 1,742 | 64.1% | 412 |
| ES2015 target | 72.8% | 1,274 | 69.4% | 147 |
| Generator | 80.2% | 126 | 100% | 2 |
| Enum | 82.8% | 267 | 69.7% | 33 |
| Namespace | 82.7% | 208 | 90.4% | 73 |
| Declaration-focused | 66.7% | 1,812 | 50.3% | 511 |

---

## Deduplicated Root Causes — JS Emit (2,523 failures)

### Tier 1: Missing Features (~750 failures)

#### F1: `emitDecoratorMetadata` (`__metadata` helper) — ~290 failures
- **Status**: Completely unimplemented
- **Details**: Helper constant exists in `transforms/helpers.rs` as dead code. `helpers.metadata` flag is never set to `true`. Requires type serialization from checker/solver to generate `__metadata("design:type", String)`, `__metadata("design:paramtypes", [Object])`, `__metadata("design:returntype", Promise)`.
- **Key files**: `transforms/helpers.rs`, `lowering/core.rs`
- **Complexity**: High (requires type serialization from checker)

#### F2: `using` / `await using` declarations transform — ~87 failures
- **Status**: Missing
- **Details**: `__addDisposableResource`/`__disposeResources` pattern not generated. TC39 explicit resource management. Affects all targets. Many overlap with decorator tests (`usingDeclarationsWithESClassDecorators`).
- **Key files**: `transforms/`, `lowering/`
- **Complexity**: High

#### F3: `__param` helper for parameter decorators — ~50 failures
- **Status**: Completely unimplemented
- **Details**: `helpers.param` flag never set. All parameter-level decorators silently dropped. tsc emits `__param(0, inject(SomeService))` wrappers.
- **Key files**: `transforms/helpers.rs`, `emitter/declarations/class.rs`
- **Complexity**: Medium

#### F4: `importHelpers` / tslib mode — ~40 failures
- **Status**: Not supported
- **Details**: When `importHelpers: true`, tsc imports helpers from `tslib` (`const tslib_1 = require("tslib"); tslib_1.__awaiter(...)`) instead of emitting inline. Option is currently ignored.
- **Key files**: `emitter/source_file.rs`, `lowering/core.rs`
- **Complexity**: Low

#### F5: JSX `react-jsxdev` mode — ~50 failures
- **Status**: Not implemented
- **Details**: `jsxDEV()` with 6 arguments including source location info and `_jsxFileName` constant. Currently reuses `jsx()`/`jsxs()` codepath. Import source should be `jsx-dev-runtime` not `jsx-runtime`.
- **Key files**: `emitter/jsx.rs`
- **Complexity**: Low

#### F6: `@jsx`/`@jsxFrag` pragma processing — ~16 failures
- **Status**: Missing
- **Details**: File-level pragmas (`/** @jsx h */`, `/** @jsxFrag Fragment */`) ignored. Always falls back to compiler option factory.
- **Key files**: `emitter/jsx.rs`
- **Complexity**: Medium

#### F7: Async generator downlevel (`__asyncGenerator`/`__await`) — ~30 failures
- **Status**: Missing
- **Details**: Helpers not emitted for async generators when target < ESNext. `__asyncGenerator` and `__await` helpers needed.
- **Key files**: `transforms/async_es5.rs`
- **Complexity**: Medium

#### F8: Block scope loop capture IIFE (`_loop_1`) — ~25 failures
- **Status**: Missing
- **Details**: When closures capture loop variables in ES5, should generate `_loop_1 = function(i) { ... }` pattern. Not implemented.
- **Key files**: `transforms/block_scoping_es5.rs`
- **Complexity**: Medium

#### F9: Static `this` → class alias (`_a = C`) — ~30 failures
- **Status**: Missing
- **Details**: `this` inside static class members/blocks not replaced with class alias. Should generate `_a = ClassName` and use `_a.prop` instead of `this.prop`.
- **Key files**: `transforms/class_es5_ir.rs`
- **Complexity**: Medium

#### F10: Class expression static property IIFE wrapping — ~43 failures
- **Status**: Missing
- **Details**: When class expression has static properties, should wrap as `(_a = class C {}, _a.a = 1, _a)`. Currently emits class statement + separate assignments.
- **Key files**: `emitter/transform_dispatch.rs`
- **Complexity**: Medium

#### F11: `__rewriteRelativeImportExtension` for nodenext — ~10 failures
- **Status**: Missing
- **Details**: `.ts` → `.js` extension rewriting in dynamic import/require calls for nodenext module resolution.
- **Key files**: `emitter/module_emission/`
- **Complexity**: Low

### Tier 2: Partial Implementation Gaps (~800 failures)

#### G1: System module `execute()` body uses CJS export semantics — ~85 failures
- **Status**: Architectural issue
- **Details**: `module_wrapper.rs:907` switches to CommonJS mode for the body. Causes `exports.X = X` to leak instead of `exports_1("X", X)`. Missing: function hoisting before return block, variable mutation wrapping (`exports_1("x", (x++, x))`), export star, `import.meta` → `context_1.meta`, async execute for top-level await, named module registration.
- **Key files**: `emitter/module_wrapper.rs`
- **Complexity**: High

#### G2: Comment preservation & positioning — ~200 failures
- **Status**: Multiple sub-issues
- **Details**:
  - Comments inside type assertions/casts moved to wrong position
  - JSDoc in JS files sometimes stripped
  - JSX expression comments ejected from `createElement` calls
  - Parameter continuation indent wrong
  - Triple-slash directive spacing differs
- **Key files**: `emitter/comments/`, `emitter/jsx.rs`
- **Complexity**: Medium

#### G3: Node16/NodeNext helper emission conditions — ~100 failures
- **Status**: Conditions don't match tsc
- **Details**: `__createBinding`/`__importStar`/`__exportStar` helpers exist but emission conditions check only `ModuleKind::CommonJS`, missing `Node16`/`NodeNext`. Also missing `Promise.resolve().then(() => __importStar(require(...)))` for dynamic imports.
- **Key files**: `lowering/core.rs`, `emitter/module_emission/`
- **Complexity**: Low-Medium

#### G4: Async ES5 state machine gaps — ~80 failures
- **Status**: Incomplete
- **Details**: Complex control flow (if/else, switch, try/catch with await) in `__generator` emits raw source instead of state machine opcodes. Missing `arguments` capture, super access binding (`_super = Object.create(null, { foo: { get: () => super.foo } })`), custom promise type (`__awaiter(this, void 0, MyPromise, ...)`).
- **Key files**: `transforms/async_es5_ir.rs`
- **Complexity**: High

#### G5: Rest/default parameters not lowered in all contexts — ~40 failures
- **Status**: Context-dependent
- **Details**: Works standalone but not inside arrow functions, async functions, or class methods. Missing `_arguments` capture for arrow functions. Missing `if (z === void 0) { z = 10; }` in some contexts.
- **Key files**: `emitter/functions.rs`, `transforms/arrow_es5.rs`
- **Complexity**: Medium

#### G6: Computed property name decorator handling — ~30 failures
- **Status**: Silently drops decorators
- **Details**: `get_identifier_text_idx()` returns empty for computed names; `member_name.is_empty()` guard skips the entire `__decorate` call. Should extract string from `["name"]` or hoist to temp var.
- **Key files**: `emitter/declarations/class.rs:157-160`
- **Complexity**: Low

#### G7: JSX key + spread → createElement fallback — ~20 failures
- **Status**: Missing fallback
- **Details**: When key appears after spread in `react-jsx` mode, should fall back to classic `createElement` API with base package import. Currently continues using `jsx()` incorrectly.
- **Key files**: `emitter/jsx.rs`
- **Complexity**: Low

#### G8: Getter/setter decorator pair merging — ~15 failures
- **Status**: Not implemented
- **Details**: tsc merges get/set decorators into single `__decorate` call. tsz emits separate calls. Getter decorators should come first, then setter.
- **Key files**: `emitter/declarations/class.rs`
- **Complexity**: Low

#### G9: Decorated class self-reference alias (`C_1`) — ~20 failures
- **Status**: Not implemented
- **Details**: Missing temp alias for classes that reference themselves in static members after `__decorate` reassigns the class variable. Should generate `var C_1; let C = C_1 = class C { ... }`.
- **Key files**: `transforms/class_es5_ir.rs`
- **Complexity**: Medium

#### G10: Private field downlevel incomplete — ~25 failures
- **Status**: Partial
- **Details**: WeakMap emission partially works but accessor/method naming broken (emits `A.prototype. = function()` with empty name). Wrong arity for `__classPrivateFieldGet` (3 args instead of 4). Private static fields don't get class alias. Private method-to-WeakSet conversion missing.
- **Key files**: `transforms/private_fields_es5.rs`
- **Complexity**: Medium

### Tier 3: Parser & Formatting Differences (~400 failures)

#### P1: Parser recovery differences — ~80 failures
- Comma with missing operand: `(ANY, )` → `(ANY)` instead of preserving
- Cast recovery: `<T>(x), T` parsed differently than tsc
- Semicolon insertion differences

#### P2: Namespace/module variable binding — ~50 failures
- Type alias variables not emitted (`var R = N;` missing)
- Module merge collisions handled differently
- Ambient module detection in external modules differs

#### P3: Enum comment placement — ~30 failures
- Trailing comments positioned differently after enum members

#### P4: Import elision edge cases — ~40 failures
- `verbatimModuleSyntax` interaction gaps
- `importsNotUsedAsValues` not fully implemented

#### P5: Rest param format — ~15 failures
- `_a` vs `_i` variable name, missing braces, `- 0` inclusion for index 0

#### P6: Template literal caching — ~5 failures
- `__templateObject_XX` vs `templateObject_1` naming scheme

#### P7: Miscellaneous — ~80 failures
- Unicode escape sequences in identifiers not preserved
- Exponentiation `Math.pow` comment positioning
- JSX namespace quoting (`"a:b"` vs `a:b`)
- Various other edge cases

---

## Root Causes — DTS Emit (607 failures)

#### D1: Type cache not populated — ~100 failures
- Auto-accessors, getters, destructured bindings fall back to `any` instead of inferred type
- `get_node_type()` returns `None` → emitter falls back to `any`

#### D2: Anonymous class types not inlined — ~30 failures
- Emits `typeof LocalName` instead of structural type literal
- TypePrinter resolves to named reference that is local/not-exported

#### D3: Namespace visibility filtering — ~40 failures
- `export =` patterns cause namespace bodies to be stripped entirely
- Only `export = m` line emitted; `declare namespace m { ... }` blocks gone

#### D4: JSDoc/comment emission — ~30 failures
- Missing on signatures, accessors, call/construct signatures
- `emit_leading_jsdoc_comments` skips certain node types

#### D5: Generic type arguments dropped — ~15 failures
- Certain `TypeApplication` nodes don't emit type arguments
- `g<string>[]` → `g[]`

#### D6: Conditional/utility types not simplified — ~15 failures
- Emits unsimplified `Extract<Options, {k: "a"}>` instead of resolved form
- Solver/type printer not evaluating conditional types

#### D7: Base class expression synthesis — ~15 failures
- `extends import(undefined)` instead of synthesized `_base` variable
- Computed base class expressions not handled

#### D8: Auto-generated import statements — ~25 failures
- Cross-file type references missing required imports in .d.ts

#### D9: Enum member widening/values — ~20 failures
- Wrong enum type in .d.ts, const enum auto-increment not emitted

#### D10: Parameter indentation, multi-file output, misc — ~50 failures

#### D11: Overlap with JS failures — ~267 failures
- Decorators, using, class issues causing both JS+DTS fail

---

## Priority Roadmap to 100%

### Quick Wins (each ≤1 week, collectively ~600 tests)

| Priority | Fix | Tests Fixed | JS % After | DTS % After |
|---|---|---|---|---|
| QW1 | Wire `__param` helper (parameter decorators) | ~50 | 82.7% | — |
| QW2 | Implement JSX `react-jsxdev` mode | ~50 | 83.1% | — |
| QW3 | Fix Node16/NodeNext helper emission conditions | ~100 | 84.5% | — |
| QW4 | Implement `importHelpers`/tslib mode | ~40 | 85.2% | — |
| QW5 | Fix computed property decorator handling | ~30 | 85.7% | — |
| QW6 | Fix getter/setter decorator pair merging | ~15 | 86.0% | — |
| QW7 | Fix rest param format (`_i`, braces) | ~15 | 86.3% | — |
| QW8 | Populate type cache for auto-accessors/getters/destructured | ~100 | — | 65.8% |
| QW9 | Fix JSDoc comment emission on signatures | ~30 | — | 67.8% |

### Medium Effort (each 1-3 weeks, collectively ~700 tests)

| Priority | Fix | Tests Fixed |
|---|---|---|
| ME1 | Implement `emitDecoratorMetadata` (`__metadata`) | ~290 |
| ME2 | Fix System module `execute()` body export semantics | ~85 |
| ME3 | Fix comment preservation (cast, assertion, JSX expression) | ~200 |
| ME4 | Fix static `this` → class alias + class expression IIFE | ~73 |
| ME5 | Fix rest/default params in all contexts (arrow, async, class) | ~40 |
| ME6 | Implement `@jsx`/`@jsxFrag` pragma processing | ~16 |

### Hard Effort (each 3+ weeks, collectively ~700 tests)

| Priority | Fix | Tests Fixed |
|---|---|---|
| HE1 | Implement `using`/`await using` declarations transform | ~87 |
| HE2 | Complete async ES5 state machine (complex control flow) | ~80 |
| HE3 | Implement block scope loop capture IIFE | ~25 |
| HE4 | Fix parser recovery parity | ~80 |
| HE5 | Complete DTS type resolution (conditional types, generics, anonymous classes) | ~150 |
| HE6 | Complete async generator downlevel | ~30 |
| HE7 | Fix private field downlevel edge cases | ~25 |

### Milestones

| Target | Work Required | Cumulative Tests Fixed |
|---|---|---|
| **85%** JS | QW1-QW4 | ~240 |
| **90%** JS | QW1-QW7 + ME1-ME2 | ~675 |
| **95%** JS | + ME3-ME6 + HE1-HE2 | ~1,300 |
| **98%** JS | + HE3-HE7 + P1-P4 | ~1,900 |
| **100%** JS | + all remaining polish | ~2,523 |

### Single Highest-ROI Fix

**`emitDecoratorMetadata`** (`__metadata` helper) — implementing this one feature fixes ~290 JS tests (11.5% of all failures), moving JS emit from **81.3% → ~83.4%**. The helper constant already exists as dead code; the main work is implementing type serialization from the checker/solver.

### Reaching 95%+ JS Emit

Implementing **QW1-QW7 + ME1-ME4** (quick wins + medium effort, excluding the hardest items) would fix ~1,300 tests, bringing JS emit from **81.3% → ~91%**.

Adding **ME5-ME6 + HE1-HE2** would reach approximately **~95%**.

The remaining **~5%** (parser recovery, complex state machine edge cases, formatting polish) represents the long tail of diminishing returns.

---

## Emitter Source Structure Reference

| File | Lines | Purpose |
|---|---|---|
| `declaration_emitter/core.rs` | 2,302 | Main declaration emitter |
| `emitter/declarations/class.rs` | 1,983 | Class declaration emit + decorators |
| `transforms/ir_printer.rs` | 1,884 | IR-based transform printer |
| `emitter/module_emission/core.rs` | 1,859 | Module format emission |
| `transforms/class_es5_ir.rs` | 1,758 | ES5 class transform IR |
| `emitter/es5/bindings.rs` | 1,739 | ES5 binding patterns |
| `emitter/jsx.rs` | 1,626 | JSX emission |
| `declaration_emitter/exports.rs` | 1,612 | Declaration export emit |
| `emitter/expressions/core.rs` | 1,594 | Expression emission |
| `emitter/statements.rs` | 1,591 | Statement emission |
| `emitter/source_file.rs` | 1,560 | Source file orchestration |
| `lowering/core.rs` | 1,502 | Lowering pass (transform analysis) |
| `emitter/core.rs` | 1,450 | Core emitter |
| `emitter/transform_dispatch.rs` | 1,452 | Transform dispatch |
| `transforms/es5.rs` | 1,365 | ES5 transform entry |
| **Total** | **~72,154** | — |
