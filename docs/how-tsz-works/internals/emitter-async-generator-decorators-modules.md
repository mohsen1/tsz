# Async/Generator State Machines, Decorators, and Module-Format Wrapping in Emit

The [emitter](emitter.md) overview names the three heaviest emit transforms in a
single sentence each: async/await and generator downleveling, the legacy and
TC39 decorator helpers, and the AMD/UMD/System module wrappers. This document
fills that gap. It goes inside the `crates/tsz-emitter/src/transforms` and
`crates/tsz-emitter/src/emitter/module_wrapper` machinery that those summaries
gesture at, naming the real transform passes, the IR opcodes, the state-machine
label algorithm, the decorator emitter's `static { }`-vs-IIFE split, and the
System live-binding protocol. Everything here is downstream of the two-phase
pipeline the parent doc describes (`LoweringPass` produces `TransformDirective`s,
`Printer` consumes them); this doc assumes you have read that and dives into the
sub-emitters the directive arms delegate to.

These transforms share one non-negotiable contract: **byte-for-byte output
parity with `tsc`**. The `__awaiter`/`__generator` runtime-helper texts, the
numeric generator opcodes (`[4 /*yield*/, x]`), the `_a.label` switch shape, the
`__esDecorate` call argument order, the System `setters`/`execute` object — all
of it is observable in `tsc`'s emit baselines, so the emitter reproduces the
exact strings and exact ordering. None of these passes run the type system: they
are AST-shape-driven string/IR builders. They are the most string-literal-heavy
code in the compiler precisely because the target output is itself fixed
boilerplate.

---

## Owns / Must not own

| Owns | Must not own |
| --- | --- |
| Async/await and generator downleveling to the `__generator` state machine (`async_es5_ir*`). | Any relation/inference/evaluation. These transforms never ask the solver a question; they read AST shape only. |
| The exact `__awaiter`/`__generator`/`__await`/`__asyncGenerator` runtime-helper source text (`transforms/helpers.rs`). | Deciding *whether* code is reachable or well-typed. The checker already validated; emit just lowers. |
| Generator opcode assignment (`[4 /*yield*/]`, `[5 /*yield**/]`, `[2 /*return*/]`, `[3 /*break*/]`, try-region `.trys.push`). | Output surgery to encode semantics. (Mechanical buffer splices for hoists/comments are allowed; semantic patches are not.) |
| Legacy `__decorate`/`__param`/`__metadata` emission and TC39 `__esDecorate`/`__runInitializers`/`__setFunctionName`/`__propKey` emission. | Choosing the decorator *model*. That is a `PrinterOptions::legacy_decorators` flag the driver sets; the emitter only reacts to it. |
| Module-format wrapping: CommonJS lowering, and the AMD/UMD/System whole-file wrappers including System live bindings and top-level-await detection. | Module *resolution*. Specifier-to-file mapping is [module-resolution-engine](module-resolution-engine.md); the emitter only formats the lowered output. |
| Runtime-helper first-use ordering and `--importHelpers` (tslib) prefixing/aliasing. | Emitting diagnostics. Decorator/async legality is checked upstream. |

---

## Where these passes sit

```text
 LoweringPass.run_plan ──► TransformDirective map  (per NodeIndex)
   │   ES5AsyncFunction / ES5GeneratorFunction      (async / generator bodies)
   │   TC39Decorators { class_node, function_name } (standard decorators)
   │   ES5Class (carries legacy __decorate emission) (experimentalDecorators)
   │   ModuleWrapper { format, dependencies }        (AMD / UMD / System whole file)
   ▼
 Printer::apply_transform  (emitter/transform_dispatch.rs)
   ├─ ES5AsyncFunction  ─► emit_async_function_es5  ─► AsyncES5Emitter ─► IRPrinter
   │                                                   (async_es5_ir* state machine)
   ├─ TC39Decorators    ─► TC39DecoratorEmitter        (es_decorators*)
   ├─ ES5Class          ─► ClassES5Emitter             (class_es5_ir* incl. legacy __decorate)
   └─ ModuleWrapper     ─► emit_module_wrapper         (emitter/module_wrapper/*)
                              ├─ emit_amd_wrapper
                              ├─ emit_umd_wrapper
                              └─ emit_system_wrapper
```

Module-format selection happens in two places. The *whole-file* AMD/UMD/System
wrapper is decided in lowering by `LoweringPass::maybe_wrap_module`
(`lowering/helpers.rs`), which maps `ModuleKind::{AMD,System,UMD}` to a
`ModuleFormat` (`context/transform.rs`) and attaches a
`TransformDirective::ModuleWrapper` to the source-file node. `CommonJS` and
`ES6`/`Preserve` are *not* whole-file wrappers — CommonJS lowers
import/export statements individually (`transforms/module_commonjs*` plus
`emitter/module_emission/`), and ES6 keeps native syntax. The async/generator
and decorator decisions happen per function/class node during the lowering walk.

---

## Part 1 — Async/await and generator state machines

### When the state machine is built at all

The full `__generator` state-machine downlevel is **not** unconditional for
async functions. The gate is `EmitTargetFacts` (`context/target_facts.rs`):

| Target | async function | generator (`function*`) | async generator (`async function*`) |
| --- | --- | --- | --- |
| ES5 | `__awaiter` + full `__generator` state machine | full `__generator` state machine | `__asyncGenerator` + state machine |
| ES2015 / ES2016 | `__awaiter` wraps a **native** `function*` (no state machine) | native (no transform) | `__asyncGenerator` + state machine |
| ES2017 (`supports_es2017`) | native `async`/`await` (no transform) | native | `__asyncGenerator` + state machine (until ES2018) |
| ES2018+ | native | native | native |

The selection lives in `LoweringPass` (`lowering/core.rs`, e.g.
`commonjs_default_export_function_directive` and the function-expression /
declaration visitors). The relevant facts are `needs_async_lowering`
(`!supports_es2017`) and `needs_es2018_lowering` (`!supports_es2018`). The key
parity point: at ES2015/ES2016 an async function only needs `__awaiter` because
generators are native, so `mark_async_helpers()` runs but the body is *not*
fed through `async_es5_ir*`. Only `target_es5` (or async-generators below
ES2018) gets the explicit `[op, value]` state machine. A bare `function*` at ES5
takes `TransformDirective::ES5GeneratorFunction` and the same state machine in
*generator mode*.

### The runtime contract: `__generator` opcodes

The state machine talks to the `__generator` helper (`GENERATOR_HELPER` in
`transforms/helpers.rs`) through a small integer opcode protocol. The opcodes are
constants in `transforms/async_es5_ir_opcodes.rs`, matching `tsc` exactly:

| Opcode | Const | Meaning | Emitted shape |
| --- | --- | --- | --- |
| 0 | `NEXT` | resume | (driver-internal) |
| 1 | `THROW` | throw | (driver-internal) |
| 2 | `RETURN` | complete | `return [2 /*return*/, value]` |
| 3 | `BREAK` | jump to label | `return [3 /*break*/, N]` |
| 4 | `YIELD` | suspend (await/yield) | `return [4 /*yield*/, operand]` |
| 5 | `YIELD_STAR` | delegate | `return [5 /*yield**/, __values(x)]` |
| 6 | `CATCH` | enter catch | (try-region) |
| 7 | `END_FINALLY` | leave finally | `return [7 /*endfinally*/]` |

`IRNode::GeneratorOp { opcode, value, comment }` is printed by
`IRPrinter::emit_node` (`transforms/ir_printer.rs`) as `[N /*comment*/, value]`.
The resumed value of a suspension is read back through `IRNode::GeneratorSent`,
which prints `<state>.sent()` (`transforms/ir_printer.rs`, around the
`GeneratorSent` arm), and the current switch selector is `IRNode::GeneratorLabel`
printing `<state>.label`. The `<state>` name (`_a`, `_b`, …) is the generator
state variable, discussed below.

### The transformer and its state

`AsyncES5Transformer` (`transforms/async_es5_ir.rs`) is the analysis half. It is
a large `&NodeArena`-borrowing struct split across ~25 sibling modules
(`async_es5_ir_statements.rs`, `_control.rs`, `_for_of.rs`, `_for_await.rs`,
`_try_statement.rs`, `_disposables.rs`, `_destructuring.rs`, …). Its mode flags
decide what counts as a suspension:

- `generator_mode` — look for `YieldExpression` instead of `AwaitExpression`.
- `async_generator_mode` — both `await` and `yield` suspend; `await x` wraps in
  `__await(x)` and `yield x` feeds `__asyncGenerator`.

The transformer's transient state is `AsyncTransformState`
(`transforms/async_es5_ir_state.rs`): `label_counter`, `in_async_body`,
`has_await`, plus `captures_arguments`/`arguments_capture_name` (a body that
reads `arguments` is rewritten to `arguments_1`, declared by the caller as
`var arguments_1 = arguments;`). `AsyncTransformState::next_label()` is the
monotonic label allocator that drives the switch cases.

`AsyncES5Emitter` (`transforms/async_es5.rs`) is the thin printing half: it
calls the transformer to build an `IRNode`, then runs `IRPrinter` over it to get
a detached `String` plus relative source-map `Mapping`s, which the `Printer`
splices in via `write_with_offset_mappings`.

### Discovery: what suspends

Before building cases, the transformer scans the body for suspension points.
The discovery predicates live in `transforms/async_es5_ir_discovery.rs`:
`suspension_kind()` returns `AWAIT_EXPRESSION` or `YIELD_EXPRESSION` by mode;
`is_suspension_expression`, `body_contains_await`, `contains_await_recursive`,
`contains_for_await_recursive`, and `contains_array_spread_recursive` walk the
subtree. Every walk **stops at nested function/method/class boundaries** so an
inner `async function` never flags its enclosing scope as containing an await.
This is the structural rule that makes nested async functions lower
independently.

### Case-splitting: the heart of the state machine

`build_generator_cases` (`transforms/async_es5_ir_cases.rs`) is the entry point.
It resets the label counter, plans catch-binding temps and body-level helpers,
then walks statements accumulating IR into `current_statements` under
`current_label`. The split happens in
`process_await_expression_with_trailing_comment`
(`transforms/async_es5_ir_statements.rs`). On each suspension it:

1. pushes `return [4 /*yield*/, operand]` (or `[5, __values(x)]` for `yield*`)
   into `current_statements`;
2. closes the current case: `cases.push(IRGeneratorCase { label: *current_label, statements: take(current_statements) })`;
3. advances `*current_label = self.state.next_label()`.

Code *after* the await resumes in the new case and reads the awaited value with
`_a.sent()`. `build_generator_cases` finishes by appending the trailing case
(`return [2 /*return*/]` if the body did not already end in a return).

The operand transform is mode-sensitive (same function):

- plain `await x` → operand is `x` lowered to IR;
- `await x` in **async-generator** mode → `__await(x)`;
- `yield* x` in generator mode → `__values(x)` with opcode 5 (so the runtime
  drives the delegate's iterator protocol);
- a bare generator `yield;` → `[4 /*yield*/]` with no operand.

### Walk-through: `async function f() { const x = await g(); return x + 1; }` at ES5

1. **Lowering** (`lowering/core.rs`) sees `func.is_async`, `target_es5`, no
   asterisk → `mark_async_helpers()` (sets `helpers.awaiter` and
   `helpers.generator`) and emits `TransformDirective::ES5AsyncFunction`.
2. **Print** reaches `f`; `apply_transform` routes the directive to
   `emit_async_function_es5` (`emitter/functions.rs` →
   `emitter/es5/helpers_async.rs`).
3. `emit_async_function_es5_body` constructs an `AsyncES5Emitter`, configures
   module kind / `downlevel_iteration` / tslib prefix / source-map capture, and
   (when the body hoists no function declarations) calls `emit_awaiter_call`.
4. `emit_awaiter_call` (`transforms/async_es5.rs`) builds
   `AsyncES5Transformer::transform_generator_body_skipping`, extracts the
   directive prologue and hoisted `var` groups, and wraps the result in
   `IRNode::AwaiterCall { this_arg, generator_body, … }`.
5. `IRPrinter::emit_awaiter_call_node` (`transforms/ir_printer_class_emit.rs`)
   prints:

   ```javascript
   return __awaiter(this, void 0, void 0, function () {
       var x;
       return __generator(this, function (_a) {
           switch (_a.label) {
               case 0: return [4 /*yield*/, g()];
               case 1:
                   x = _a.sent();
                   return [2 /*return*/, x + 1];
           }
       });
   });
   ```

   `emit_generator_body_node` prints the `__generator` envelope and the
   `switch (_a.label)` head; `_a` is the generator state name (see below). The
   `var x;` is a hoisted-var group lifted out of the cases (local declarations
   are hoisted to the awaiter callback so they survive across the suspension
   boundary). `x = _a.sent()` is `GeneratorSent`.

### Generator state name allocation (`_a` vs hoisted temps)

The `__generator` callback parameter (`_a`) must not collide with temps the
state machine hoists. `IRPrinter::generator_state_name_for_hoisted`
(`transforms/ir_printer_generator_state.rs`) scans the hoisted var names, finds
the highest `_a..`-style index used, and picks the next free slot. Two slots are
permanently skipped: index 8 (`_i`) and index 13 (`_n`), because `tsc` reserves
those for dedicated `TempFlags` (loop index / `_n`). So a body that already
hoisted `_a` gets `_b` as its state name; this matches `tsc`'s temp allocator.
`rename_colliding_outer_generator_state` (same file) is a post-pass that renames
the state var when it would shadow the generator's own `this` arg — it rewrites
only `<state>.`-prefixed property accesses inside the `__generator` body,
skipping nested function ranges, so an unrelated `_a` in a nested closure is left
alone.

### Try/catch/finally regions

A `try` inside an async body becomes a `_.trys` region rather than native
`try`/`catch` (the helper's `step` driver interprets the region table). The IR
nodes `GeneratorTryPush`, `GeneratorTryPushCatch`, and `GeneratorTryPushFinally`
(`transforms/ir_printer.rs`) print `<state>.trys.push([start, catch, finally, end])`
with the appropriate slots blank. The region labels are filled in late by
`patch_try_region_placeholders` (`transforms/async_es5_ir_try_region.rs`), which
rewrites placeholder `GeneratorOp`s once all labels in the region are known.
`END_FINALLY` (opcode 7) emits `return [7 /*endfinally*/]`.

### Catch-binding renaming: a file-wide ordinal

A subtle parity rule: when a `catch (e)` lives inside an async body, `tsc`
renames the binding to `e_1`, `e_2`, … and the suffix counter **does not reset
across function boundaries** — it is file-wide. `fresh_catch_binding_temp`
(`transforms/async_es5_ir_names.rs`) implements this with
`catch_binding_ordinals`, a `RefCell<FxHashMap<String, u32>>` threaded through
`emit_async_function_es5_body` via `set_catch_binding_ordinals` /
`take_catch_binding_ordinals` so the count survives between functions in the same
file. `plan_catch_binding_temps` (`transforms/async_es5_ir_cases.rs`) reserves
the temp before the body is converted, so nested async expressions continue the
numbering after the outer body's catch names.

### `for await...of`

`process_for_await_statement_in_async`
(`transforms/async_es5_ir_for_await.rs`) lowers `for await (const x of src)` by
wrapping the iterable in `__asyncValues(src)` (line ~206) and driving
`iterator.next()` / `iterator.return()` through awaited suspensions inside the
state machine. The body-level helper request happens in
`plan_body_level_helpers` (`async_es5_ir_cases.rs`), which calls
`mark_async_values()` when `contains_for_await_recursive` is true and
`mark_spread_array()` when an array spread appears inside the awaited body.

### Async helper texts

The helper source text is fixed and matches `tsc`'s tslib verbatim
(`transforms/helpers.rs`): `AWAITER_HELPER`, `GENERATOR_HELPER` (with the full
`step` driver and the `case 4`/`case 5`/`case 7` opcode interpreter),
`AWAIT_HELPER`, `ASYNC_GENERATOR_HELPER`, `ASYNC_DELEGATOR_HELPER`,
`ASYNC_VALUES_HELPER`, and `VALUES_HELPER`. Each is emitted in the guarded
`var __x = (this && this.__x) || function ...` form. The emitter never
synthesizes these; it picks the constant.

---

## Part 2 — Decorators

tsz emits two completely different decorator lowerings, selected by
`PrinterOptions::legacy_decorators` (the `--experimentalDecorators` flag). The
driver sets the flag; lowering and the printer react to it. They produce
different helpers and different code shapes.

### Legacy (`--experimentalDecorators`): `__decorate` / `__param` / `__metadata`

Legacy decoration is folded into ES5/ES2015 class emission. The `ES5Class`
directive carries it, and `ClassES5Emitter::set_legacy_decorators`
(`transforms/class_es5_ir.rs`) toggles the path. Member-decorator emission lives
in `transforms/class_es5_ir_decorators.rs`:

- A decorated member emits `__decorate([dec1, dec2, ...], target, "name", desc)`
  after the class body (`emit` site around the `__decorate` raw-string builder).
  The decorator array, target (`ClassName.prototype` or `ClassName`), member
  name, and property descriptor are formatted with the exact indentation `tsc`
  uses (continuation lines at `indent_base + 2`).
- A class decorator emits `ClassName = __decorate([dec1, ...], ClassName)`
  (`emit_class_decorator_ir`). Constructor parameter decorators are folded into
  the *class-level* `__decorate` array as `__param(index, dec)` entries, between
  the class decorators and any `__metadata` entries.
- With `emitDecoratorMetadata`, `__metadata("design:type", T)`,
  `"design:paramtypes"`, and `"design:returntype"` entries are appended. The
  type is serialized by `serialize_type_for_metadata` /
  `serialize_param_types` — a *syntactic* serialization of the type annotation to
  a runtime constructor reference (`Function`, `Promise`, `void 0`, …), not a
  solver query.

The helper texts are `DECORATE_HELPER`, `PARAM_HELPER`, `METADATA_HELPER`
(`transforms/helpers.rs`), again verbatim tslib. `DECORATE_HELPER` is the
`Reflect.decorate`-or-fallback form; the iteration is right-to-left
(`for (var i = decorators.length - 1; i >= 0; i--)`), which is why decorator
*application order* is reversed from source order.

### TC39 / standard (default): `__esDecorate` / `__runInitializers`

The standard decorator model is a whole-class transform with its own directive,
`TransformDirective::TC39Decorators { class_node, function_name }`. The printer
arm `emit_tc39_decorators` (`emitter/transform_dispatch.rs`) builds a
`TC39DecoratorEmitter` (`transforms/es_decorators.rs`) and, critically, chooses
the **emit shape by target**:

```rust
emitter.set_use_static_blocks(!self.ctx.needs_es2022_lowering);
```

- **ES2022+** (`use_static_blocks = true`): decoration runs inside a
  `static { ... }` initializer block in the class body, using `this` as the
  class reference.
- **ES2015–ES2021**: decoration runs in an IIFE wrapper with comma expressions
  around the class expression, using a generated class alias (`_classThis`).
- **ES5**: there is a dedicated fast path
  `render_simple_tc39_decorated_class_es5` for the common
  single-decorated-class shape (called both from `apply_transform`'s `ES5Class`
  arm and from `emit_tc39_decorators`).

`emit_decorator_application` (`transforms/es_decorators_application.rs`) is the
core. For each decorated member, in `decorator_application_order`, it emits an
`__esDecorate` call; then a class-level `__esDecorate(null, _classDescriptor = { value: _classThis }, _classDecorators, { kind: "class", name: ..., metadata: _metadata }, null, _classExtraInitializers)`
followed by `ClassName = _classThis = _classDescriptor.value`. The
`Symbol.metadata` plumbing (`_metadata = typeof Symbol === "function" && Symbol.metadata ? Object.create(...) : void 0`)
and the `__runInitializers(ctor, _staticExtraInitializers)` /
`__runInitializers(ctor, _classExtraInitializers)` calls are emitted in `tsc`'s
exact order. The helper texts are `ES_DECORATE_HELPER`, `RUN_INITIALIZERS_HELPER`,
`PROP_KEY_HELPER`, `SET_FUNCTION_NAME_HELPER` (`transforms/helpers.rs`).

### Helper-marking and ordering for TC39

`LoweringPass::mark_tc39_decorator_helpers` (`lowering/decorator_helpers.rs`)
decides which TC39 helpers a class needs and, importantly, their *first-use
order*. It always sets `es_decorate` and `run_initializers`, then:

- sets `run_initializers_before_es_decorate = true` **iff** the class has a
  decorated method, getter, or setter (`class_has_decorated_method_or_accessor`).
  Those members request the method extra-initializers `__runInitializers` before
  the class-level `__esDecorate` is built, so `tsc` emits `__runInitializers`
  first. Decorated fields, auto-accessors, and bare class decorators keep
  `__esDecorate` first. This is a pure ordering parity rule.
- sets `prop_key` when a decorated member has a computed key
  (`class_has_computed_decorated_member`).
- sets `set_function_name` for private decorated members or for class decorators
  under ES5/ES2022-lowering / anonymous / static-private-method / static
  auto-accessor shapes.
- requests `__classPrivateFieldGet/Set/In` for decorated static private members
  via `decorated_static_private_member_helper_needs`.

`is_tc39_decorated_anonymous_class_expression` (same file) detects the anonymous
decorated class-expression case that needs special binding handling
(`default_1` → display name `default`).

### Walk-through: `@sealed class C { @log m() {} }` at ES2022 (default decorators)

1. **Lowering** sets the `TC39Decorators` directive on `C` and calls
   `mark_tc39_decorator_helpers`: `es_decorate = true`, `run_initializers = true`,
   and `run_initializers_before_es_decorate = true` (because `m` is a decorated
   method).
2. **Print** routes to `emit_tc39_decorators` →
   `set_use_static_blocks(true)` (ES2022 is not `needs_es2022_lowering`).
3. The `TC39DecoratorEmitter` emits the class with a `static { }` block that
   declares `_metadata`, calls `__esDecorate(this, null, _m_decorators, { kind: "method", name: "m", static: false, private: false, access: {...}, metadata: _metadata }, null, _instanceExtraInitializers)`,
   then the class-level `__esDecorate(null, _classDescriptor = { value: this }, _classDecorators, { kind: "class", ... }, null, _classExtraInitializers)`,
   then `__runInitializers(this, _classExtraInitializers)`.

---

## Part 3 — Module-format wrapping

### CommonJS (statement-level lowering, not a wrapper)

`CommonJS` does not wrap the whole file. Each import/export statement lowers
individually (`transforms/module_commonjs.rs` and
`emitter/module_emission/`). Key emit functions in `module_commonjs.rs`:

- The module prologue is `Object.defineProperty(exports, "__esModule", { value: true });`
  (the formatted constant near the top of the file).
- `get_import_bindings` lowers an import clause: `import foo from "m"` →
  `var foo = m_1.default;`; `import * as ns from "m"` → `var ns = __importStar(m_1);`
  under `esModuleInterop`, else `var ns = m_1;`. Named imports
  (`import { a, b as c }`) emit **no** local var — call sites are rewritten to
  `m_1.a` property accesses, matching `tsc`.
- `emit_export_assignment` → `exports.foo = foo;`.
- `emit_reexport_property` → `Object.defineProperty(exports, "foo", { enumerable: true, get: function () { return m_1.foo; } });`
  for `export { foo } from "m"` live re-exports.
- `export =` (export-assignment) and default-export forms are handled by the
  categorized collectors (`collect_export_names_categorized`).

The interop helpers are `IMPORT_DEFAULT_HELPER`, `IMPORT_STAR_HELPER`,
`CREATE_BINDING_HELPER`, `SET_MODULE_DEFAULT_HELPER`, `EXPORT_STAR_HELPER`
(`transforms/helpers.rs`).

#### CommonJS live exports

A `export let x` (or `export var/const`) is a *live binding*: every mutation of
`x` must also update `exports.x`. `commonjs_live_exports.rs`
(`emitter/module_emission/`) implements this. `CjsLiveExportKind` classifies a
local name as `Inline` (all reads rewrite to `exports.x`), `Clause`
(`export { x as foo }`, local `x` plus `exports.foo`), or `NotExported`. The
context predicate `is_commonjs_live_export_context` is careful: it is **not** the
same as `is_effectively_commonjs()`, because that helper also returns true inside
AMD/UMD/System wrapper bodies, which use a different export protocol and must not
get `exports.x = ...` rewrites.

### AMD, UMD, System: whole-file wrappers

These three attach `TransformDirective::ModuleWrapper { format, dependencies }`
to the source file in lowering and are emitted by `emit_module_wrapper`
(`emitter/module_wrapper/wrapper_entry.rs`), which dispatches on `ModuleFormat`.

| File | Role |
| --- | --- |
| `module_wrapper/wrapper_entry.rs` | `emit_module_wrapper` dispatch + `emit_amd_wrapper`, `emit_umd_wrapper`, `emit_system_wrapper`. |
| `module_wrapper/system_emit.rs` | System `setters` / `execute` body emission. |
| `module_wrapper/system_live_exports.rs` (and `module_emission/system_live_exports.rs`) | System `exports_1(...)` live-binding rewrites. |
| `module_wrapper/system_hoist.rs` | hoisted `var` names and hoisted function declarations for System. |
| `module_wrapper/system_export_star.rs` | `exportStar_1` re-export helper + `exportedNames_1` exclusion map. |
| `module_wrapper/system_import_export_order.rs` | ordering of import substitutions and export registration. |
| `module_wrapper/system_top_level_await` (tests) + `system_execute_needs_async` | top-level-await detection. |

#### AMD

`emit_amd_wrapper` emits `define([deps], function (require, exports, ...) { ... })`.
Dependency order is fixed: `"require"`, `"exports"`, then **named** `amd-deps`,
then import value deps, then **unnamed** `amd-deps`, then side-effect deps.
`/// <reference />` and `/// <amd-dependency />` directives are emitted *before*
`define()` (outside the wrapper), matching `tsc`. The `define()` callback gets a
`"use strict";` prologue when the file is a module (`file_is_module`), and
`suppress_use_strict` is set so the inner body emitter does not double-emit it.

#### UMD

`emit_umd_wrapper` emits the UMD prelude
(`(function (factory) { if (typeof module === "object" ...) ... else if (typeof define === "function" && define.amd) ... })(function (require, exports, ...) { ... })`).
Its dependency ordering differs deliberately from AMD: **named amd-deps,
unnamed amd-deps, import deps, side-effect deps** (AMD interleaves import deps
before unnamed amd-deps). A `var __syncRequire = ...` line is added when the
source has a dynamic `import()` call.

#### System

`emit_system_wrapper` emits `System.register([deps], function (exports_1, context_1) { ... })`.
The structure is: a hoisted `var` list (from `collect_system_hoisted_names`,
minus hoisted function names), `var __moduleName = context_1 && context_1.id;`,
import substitution registration, local export bindings, hoisted function
declarations (emitted under a temporary `ModuleKind::CommonJS` so `import()`
inside them dispatches through the System branch), the `exportStar_1` helpers,
then the `return { setters: [...], execute: function () { ... } };` object.

System has three parity-critical behaviors:

1. **Live bindings via `exports_1`.** `system_live_exports.rs` rewrites mutations
   of exported locals to `exports_1("name", localExpr)`. An assignment
   `x = v` becomes `exports_1("x", x = v)`, and prefix/postfix unary mutations
   are wrapped similarly (`emit_system_live_export_assignment_expression`,
   `_prefix_unary`, `_postfix_unary`). The set of export names for a local comes
   from `system_export_names_for_local`.
2. **Top-level await → async `execute`.** `system_execute_needs_async`
   (`wrapper_entry.rs`) walks the top-level statements; if it finds a top-level
   `await`, an `await using` declaration, or a `for await...of` (whose
   downleveled form emits `await iterator.next()`), it emits
   `execute: async function () { ... }` instead of `execute: function () { ... }`.
   The walk stops at function-like boundaries, because a nested function opens
   its own async context.
3. **tslib dependency injection.** Under `--importHelpers`, `"tslib"` is injected
   as the first System dependency and an `Assign("tslib_1")` action is added when
   the source has no user `Assign` for tslib, so helper calls resolve through the
   wrapper-provided binding (`tslib_1.__decorate`, …).

---

## Caches and invariants

- **Helper first-use order is recorded, not just membership.** `HelpersNeeded`
  (`transforms/helpers.rs`) is a struct of booleans plus an
  `unprioritized_order: Vec<HelperEmitOrder>` and targeted ordering flags:
  `class_private_field_set_before_get`, `run_initializers_before_es_decorate`.
  `needed_names()` walks a fixed priority sequence and splices the unprioritized
  ones in their first-use order. The async helpers `__await`,
  `__asyncGenerator`, `__asyncDelegator`, `__values`, `__read`, `__spreadArray`,
  `__asyncValues` are all in the unprioritized lane (`HelperEmitOrder`), because
  their relative order is determined by which feature the file uses first
  (`mark_read` inserts `__read` before `__spreadArray`; `mark_async_values`
  orders `__asyncValues` relative to `__spreadArray`).
- **Generator state name is collision-checked against hoisted temps and skips
  `_i`/`_n`.** `generator_state_name_for_hoisted` is recomputed each time an
  awaiter/generator body is printed from that body's hoisted-var set; it is not a
  global counter.
- **Catch-binding ordinals are file-wide and threaded by move.** The
  `catch_binding_ordinals` map is taken out of the `Printer`
  (`next_catch_binding_ordinals`), handed to each `AsyncES5Emitter`, and taken
  back, so the `e_1`, `e_2`, … sequence never resets between functions.
- **`temp_var_counter` and disposable-env counters round-trip through the
  emitter.** `AsyncES5Emitter` exposes `temp_var_counter()` /
  `set_temp_var_counter`, `disposable_env_counter()`, and
  `dynamic_import_promise_counter()` so the parent `Printer` keeps the file-wide
  temp sequence monotonic across spliced sub-renders
  (`sync_es5_class_emitter_state` in `emitter/transform_dispatch.rs`).
- **Module-wrapper bodies temporarily mutate `ctx.options.module`.** System
  hoisted-function emission and CJS hoisted-function emission set
  `original_module_kind` and flip `module` to `CommonJS`/`None` for a scoped
  window, so `import()` and live-export decisions inside hoisted bodies dispatch
  correctly. `is_commonjs_live_export_context` reads
  `cjs_export_body_outer_module` to see through that window.
- **`__decorate` is hoisted before AMD/UMD wrappers.**
  `hoist_decorate_helper_before_wrapper` (`wrapper_entry.rs`) emits the
  `__decorate` helper *outside* the wrapper body and temporarily clears its flag,
  restoring it after, because `tsc` places that one helper at file top level.

---

## Edge cases and `tsc` parity

- **Native generators at ES2015/ES2016.** An async function at these targets
  gets only `__awaiter` wrapping a native `function*` body; the explicit
  `__generator` state machine is *not* emitted. Only ES5 (and async-generators
  below ES2018) get the opcode state machine. Misfiring this would change every
  async baseline at ES2015.
- **`yield*` uses opcode 5 and `__values`.** Delegation lowers to
  `return [5 /*yield**/, __values(x)]`, not `[4]`; the `__values` helper is
  requested by lowering.
- **Bare `yield;` vs `await;`.** A generator `yield;` with no operand lowers to
  `[4 /*yield*/]` (no value); a recovered empty `await;` keeps a historical empty
  operand. (`process_await_expression_with_trailing_comment`.)
- **`arguments` capture.** A body that reads `arguments` is rewritten to
  `arguments_1` and the caller emits `var arguments_1 = arguments;` before
  `return __awaiter(...)`, because the generator closure cannot see the outer
  `arguments`.
- **Decorator application order is reversed.** Both legacy `__decorate` and TC39
  `__esDecorate` iterate decorators last-to-first, matching the spec; source
  order is preserved in the *array*, application is reversed in the *helper*.
- **`__runInitializers` before `__esDecorate`** only when the class has a
  decorated method/getter/setter — fields, auto-accessors, and bare class
  decorators keep `__esDecorate` first (`run_initializers_before_es_decorate`).
- **TC39 shape is target-dependent:** `static { }` blocks at ES2022+, IIFE +
  comma expressions at ES2015–ES2021, a simple-class fast path at ES5
  (`render_simple_tc39_decorated_class_es5`).
- **AMD vs UMD dependency ordering differ** (import deps before unnamed amd-deps
  in AMD; after them in UMD) — a deliberate `tsc` quirk, not a bug.
- **System `execute` goes async only for genuine top-level await**, and the
  detector stops at nested function boundaries so a nested `await` does not
  force the wrapper async.
- **System live bindings rewrite mutations, not reads** —
  `x = v` → `exports_1("x", x = v)` — and this fires only in
  `is_system_live_export_context`, never in CJS/AMD/UMD bodies, which is why the
  CJS context predicate explicitly excludes the System case.
- **`"use strict"` lives *inside* the AMD `define()` / UMD factory callback**,
  not at file top, and `suppress_use_strict` prevents the inner body emitter from
  re-emitting it.
- **`--importHelpers` (tslib).** Helper call sites are prefixed
  (`tslib_1.__awaiter`) under effectively-CommonJS modules and aliased
  (`__awaiter_1`) on name collision; under System, `"tslib"` is injected as the
  first dependency with an `Assign` action. The `TslibHelperNaming`
  (`transforms/tslib_helper_naming.rs`) value carries the prefix/binding/alias
  state into each sub-emitter.

All of these are decided from target facts (`EmitTargetFacts`), module kind,
the `legacy_decorators` option, and AST shape — never from a fixture name and
never by reading the emitter's own rendered output back as a predicate.

---

## See also

- [emitter](emitter.md) — the two-phase pipeline, the `TransformDirective`
  vocabulary, `Printer::apply_transform`, temp/hoist planning, and source maps
  that this doc's transforms plug into.
- [binder](binder.md) — the symbol/flow facts and import value-usage data that
  module lowering consumes for elision.
- [checker-classes](checker-classes.md) and
  [checker-class-shape-construction](checker-class-shape-construction.md) — the
  class semantics validated before decorator/field lowering runs.
- [checker-declarations-modules](checker-declarations-modules.md) — module
  semantics (export/import binding) upstream of CommonJS/System lowering.
- [module-resolution-engine](module-resolution-engine.md) — how specifiers map
  to files; the emitter only formats the lowered module output.
- [driver-project-references-and-build-mode](driver-project-references-and-build-mode.md)
  and [driver-incremental-and-watch](driver-incremental-and-watch.md) — where
  emit sits in the larger build, including `outFile` concatenation of wrappers.
- [end-to-end-timeline](end-to-end-timeline.md) — where emit runs in the full
  compile.
