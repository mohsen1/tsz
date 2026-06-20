# Emitter: JS Emit, Transforms, Lowering, Declaration Emit, and Source Maps

The emitter is the last stage of the pipeline
(`scanner -> parser -> binder -> checker -> solver -> emitter`). It turns a
parsed `NodeArena` AST into JavaScript text, `.d.ts` declaration text, and
optional Source Map v3 JSON. Its single contract is *output parity with `tsc`*:
the byte-for-byte downleveled JavaScript, the helper ordering, the temp-name
sequence, the `.d.ts` surface, and the source-map segments must match what
TypeScript emits. The emitter is a pure formatter — it never re-runs the type
system. Every semantic answer it needs (inferred types for `.d.ts`, import
value-usage facts, const-enum values) is computed earlier and handed in as
read-only data.

The crate is `tsz-emitter` (`crates/tsz-emitter/src`). It is organized into a
two-phase JS pipeline (a read-only **lowering pass** that produces transform
directives, then a **print pass** that consumes them), a tree of feature
emitters under `emitter/`, a family of string/IR transforms under `transforms/`,
and a self-contained **declaration emitter** under `declaration_emitter/`
(gated behind the `dts` cargo feature). Output and source-map bookkeeping live
in `output/`. This document traces real code paths through those modules and
names the functions that run. It is the middle-tier companion to
[binder](binder.md), [end-to-end-timeline](end-to-end-timeline.md), and the
checker/solver internals docs.

> Note on the name "lowering": inside `tsz-emitter` the **emit** lowering pass
> is `crate::lowering::LoweringPass` (AST -> transform directives). There is a
> separate workspace crate `tsz-lowering` whose `lower` module is the
> *type-system* bridge (AST type nodes -> `TypeId`); it is not part of the emit
> pipeline and is not covered here.

---

## Owns / Must not own

**The emitter owns:**

- AST-to-JavaScript text generation for every `SyntaxKind`, dispatched through
  `Printer::emit_node` / `Printer::emit_node_by_kind`
  (`emitter/core.rs`).
- Target downleveling: ES5/ES2015+ class/arrow/`for-of`/spread/template/optional-chain/nullish
  lowering, async/await and generator lowering, decorator lowering (legacy
  `__decorate` and TC39 `__esDecorate`), and `using`/disposable lowering.
- Module-format output: `CommonJS`, `AMD`, `System`, `UMD`, ES6/`Preserve`,
  and the Node16/NodeNext format resolution.
- Runtime-helper scheduling and emission (`__extends`, `__awaiter`,
  `__generator`, `__spreadArray`, `__classPrivateFieldGet`, ...), including
  first-use ordering and `--importHelpers` (tslib) rewrites.
- Temp/hoist planning: the file-scoped `_a`, `_b`, ... generated-name sequence,
  reservation against user identifiers, and prologue/var hoisting.
- Declaration emit (`.d.ts`): printing the public type surface from
  checker-produced type caches, import elision, and portability checks.
- Output buffering, indentation, UTF-16 column tracking, and Source Map v3
  segment generation (`output/source_writer.rs`,
  `tsz_common::source_map`).

**It must not own** (hard architecture rules):

- **Semantic validation.** The emitter emits no diagnostics and runs no
  relation/inference/evaluation. `.d.ts` types come from a precomputed
  `TypeCacheView`; the emitter looks them up, it does not compute them.
- **Output surgery to encode policy.** Transforms produce the right output
  structurally; the emitter does not post-process already-emitted text to patch
  in semantics. (It *does* do mechanical buffer edits — `insert_at`,
  `truncate`, `undo_last_write_line` — for hoist injection and comment
  placement, which are formatting, not semantics.)
- **Type construction or printer-output predicates.** The declaration emitter
  reads `TypeId`s and prints them; it must not read its own rendered string back
  as a semantic decision input.

---

## Where the emitter sits in the pipeline

```text
 parser ──► NodeArena (read-only AST, arenas, 16-byte nodes)
                 │
   checker/solver├─► TypeCacheView (node_types, symbol_types, def_types …)   [for .d.ts]
                 ├─► ImportValueUsageFacts (binder-backed import elision)     [for JS]
                 │
                 ▼
   ┌──────────────────────── tsz-emitter ────────────────────────┐
   │ Phase 1  LoweringPass.run_plan(root) ──► EmitPlan            │
   │            (read-only walk; emits TransformDirective map)    │
   │ Phase 2  Printer (with EmitPlan) ──► SourceWriter ──► String │
   │            DeclarationEmitter ──► .d.ts String + map         │
   └─────────────────────────────────────────────────────────────┘
                 │
                 ▼
        CLI / WASM / project driver writes .js / .d.ts / .js.map
```

The CLI driver wires it together in `crates/tsz-cli/src/driver/emit.rs`:
`tsz::lowering::LoweringPass::new(&file.arena, &ctx).run_plan(file.source_file)`
builds the plan, `Printer::with_emit_plan_and_options(...)` constructs the
printer, `printer.emit(file.source_file)` runs the print pass, and
`printer.take_output()` / `printer.generate_source_map_json()` produce the
artifacts. Declarations go through `DeclarationEmitter::with_shared_type_info(...)`
(or `DeclarationEmitter::new` for the binderless transpile path) and
`emitter.emit(file.source_file)`. The WASM path mirrors this in
`crates/tsz-core/src/api/wasm/parser.rs`.

---

## Module map

| Path | Role |
| --- | --- |
| `output/source_writer.rs` | `SourceWriter`: the text buffer. Indentation, UTF-16 column tracking, source-map mapping calls, `LineMap` for O(log n) offset→(line,col). |
| `output/printer.rs` | High-level `Printer`/`PrintOptions` convenience wrapper, `print_to_string`, `lower_and_print`, `StreamingPrinter`. |
| `context/transform.rs` | `TransformDirective` enum, `TransformContext` directive map, `HelpersNeeded` snapshot, `this_capture_scopes`. |
| `context/plan.rs` | `EmitPlan` (file-level typed plan: target facts, module, transforms, helpers, temp/hoist/export/region slots) and `EmitPlanBuilder`. |
| `context/target_facts.rs` | `EmitTargetFacts`: per-`ScriptTarget` feature gates (`needs_es2022_lowering`, `needs_async_lowering`, `supports_using_declarations`, …). |
| `context/emit.rs` | `EmitContext`: live transform state (`EmitFlags`, `DestructuringState`, `BlockScopeState`, `PrivateFieldState`, module state). |
| `lowering/` | Phase 1 `LoweringPass`: read-only AST walk producing directives + helper requests. |
| `emitter/core.rs` | The big `Printer<'a>` struct, recursion guard, `emit_node`, `emit_node_by_kind` dispatch. |
| `emitter/source_file/emit.rs` | `emit_source_file`: prologue (`"use strict"`, shebang), helper block, module wrapping. |
| `emitter/transform_dispatch.rs` | `apply_transform`: turns each `TransformDirective` into emitted text. |
| `emitter/expressions/`, `statements/`, `declarations/`, `functions*`, `literals/`, `jsx/`, `module_emission/`, `module_wrapper/`, `es5/` | Feature emitters, by concern. |
| `emitter/helpers.rs` | `write_helper`, `make_unique_name*` temp/hoist name allocation. |
| `transforms/ir.rs`, `ir_printer*.rs` | `IRNode` tree + `IRPrinter`: string IR used by ES5 class / namespace / async / generator lowering. |
| `transforms/class_es5*`, `async_es5*`, `enum_es5*`, `namespace_es5*`, `module_commonjs*`, `spread_es5.rs`, `destructuring_es5.rs`, `private_fields_es5.rs`, `es_decorators*` | The heavy ES5/feature transforms. |
| `transforms/helpers.rs` | Helper source-text constants (`EXTENDS_HELPER`, `AWAITER_HELPER`, …) and the `HelpersNeeded` struct + first-use ordering. |
| `passes/import_value_usage.rs` | Binder-backed per-import-binding value-usage facts for JS import elision. |
| `declaration_emitter/` | The `.d.ts` emitter (gated by feature `dts`): `DeclarationEmitter`, type printing, import rewrites, portability. |
| `type_cache_view.rs` | `TypeCacheView`: read-only projection of checker type caches consumed by the declaration emitter. |

---

## Phase 1: the lowering pass (`crate::lowering::LoweringPass`)

`LoweringPass` (`lowering/core.rs`) is a **read-only** depth-first walk over the
`NodeArena`. It computes nothing about types. Its only outputs are:

1. a `TransformContext` mapping `NodeIndex -> TransformDirective`, and
2. a `HelpersNeeded` snapshot of which runtime helpers the file will need.

`LoweringPass::run_plan(source_file)` calls `run(source_file)` and wraps the
result in an `EmitPlan` via `EmitPlanBuilder` (`lowering/core.rs`). `run`
does: `init_module_state`, push the source-file as the top-level `_this` capture
scope when `target_es5`, `self.visit(source_file)`, `maybe_wrap_module`, and
`mark_helpers_populated`.

The walk threads scope context as plain fields: `this_capture_level`,
`arguments_capture_level`, `in_constructor`, `in_static_context`,
`current_class_is_derived`, `in_es5_class`, a `namespace_depth`, and stacks
`enclosing_function_bodies` / `enclosing_capture_names` (the scope that needs
`var _this = this;`). Recursion is bounded by `MAX_AST_DEPTH = 500`,
`MAX_QUALIFIED_NAME_DEPTH = 100`, and `MAX_BINDING_PATTERN_DEPTH = 100`.

### Why a projection layer instead of mutating the AST

The `NodeArena` AST is read-only data-oriented storage (16-byte `Node`s, arena
indices). Transforms cannot rewrite nodes in place. Instead, lowering produces
lightweight **directives** that tell the print pass to emit a node differently
than its literal AST shape. The `TransformContext` doc comment
(`context/transform.rs`) calls this the "Projection Layer": AST stays
read-only, transforms are testable in isolation, and composition is expressed by
`TransformDirective::Chain` and `CommonJSExport { inner, .. }` rather than nested
node rewrites.

### The directive vocabulary

`TransformDirective` (`context/transform.rs`) is the contract between the two
phases. The key variants:

| Directive | Meaning |
| --- | --- |
| `Identity` | Emit as-is. |
| `ES5Class { class_node, heritage }` / `ES5ClassExpression` | Class -> IIFE (`var Foo = (function () { ... }())`). |
| `ES5Namespace { should_declare_var }` / `ES5Enum` | Namespace/enum -> IIFE. |
| `ES5GeneratorFunction`, `ES5AsyncFunction` | Generator -> `__generator`; async -> `__awaiter`. |
| `ES5ArrowFunction { captures_this, captures_arguments, class_alias }` | Arrow -> `function`. |
| `ES5ForOf`, `ES5ObjectLiteral`, `ES5ArrayLiteral`, `ES5CallSpread`, `ES5NewSpread`, `ES5VariableDeclarationList`, `ES5FunctionParameters`, `ES5TemplateLiteral` | Iteration/spread/destructuring/default-param/template downlevel. |
| `SubstituteThis { capture_name }`, `SubstituteArguments` | Replace `this`/`arguments` reads inside captured arrows. |
| `ES5SuperCall` | `super(...)` -> `_super.call(this, ...)`. |
| `CommonJSExport { names, is_default, inner }`, `CommonJSExportDefaultExpr`, `CommonJSExportDefaultClassES5` | Wrap exported declarations with `exports.X = X`. |
| `ModuleWrapper { format, dependencies }` | Wrap the whole file for AMD/System/UMD. |
| `TC39Decorators { class_node, function_name }` | TC39 (standard) decorator IIFE wrapper. |
| `Chain(Vec<Self>)` | Apply transforms in order (e.g. ES5 class **and** CommonJS export). |

The `SubstituteThis`/`SubstituteArguments` directives are attached to individual
`Identifier` nodes, which is why `emit_node_by_kind` special-cases identifiers
that carry directives (`emitter/core.rs`): inside a `this`-capturing arrow,
the `this` token is rewritten to the capture name (`_this`) and `arguments` to
`_arguments`.

---

## Phase 2: the print pass (`Printer`)

`Printer<'a>` (`emitter/core.rs`) is a very large struct: it holds the
`&NodeArena`, the `SourceWriter`, the live `EmitContext`, the `TransformContext`
(via the `EmitPlan`), and a long list of per-emit scratch fields (pending class
field inits, namespace export-fold state, temp-name pools, comment cursors,
optional-chain receiver splice offsets, …). The print pass is a recursive AST
walk.

### Dispatch

The single dispatch entry is `Printer::emit_node(node, idx)`
(`emitter/core.rs`):

```text
emit_node(node, idx):
  emit_recursion_depth += 1
  if depth > MAX_EMIT_RECURSION_DEPTH (10_000):
      write "/* emit recursion limit exceeded */"; return
  has_transform = !transforms.is_empty()
                  && kind_may_have_transform(node.kind)   // cheap kind prefilter
                  && transforms.has_transform(idx)
  queue_source_mapping(node)                              // remember source pos
  if has_transform: apply_transform(node, idx)            // directive path
  else:             emit_node_by_kind(node, idx, kind)    // literal path
  emit_recursion_depth -= 1
```

`kind_may_have_transform` is a `const fn` over `node.kind` that filters to the
handful of kinds that can carry a directive (source file, class, module, enum,
function/arrow, variable statement/list, `for-of`, object/array literal, call,
new, tagged/template). This avoids a hash lookup for the overwhelming majority
of nodes (identifiers, operators) that never have directives.

`emit_node_by_kind` (`emitter/core.rs`) is the literal-emit switch: a large
`match kind` mapping each `SyntaxKind` to a feature emitter
(`emit_identifier`, `emit_numeric_literal`, `emit_string_literal`,
`emit_class_declaration`, `emit_function_declaration`, and so on across the
`emitter/` submodules). The depth guard `MAX_EMIT_RECURSION_DEPTH = 10_000` is
sized for valid deeply-nested left-associative expression chains (the binder's
binary-expression stress fixture) rather than only pathological inputs.

### `apply_transform`

`apply_transform(node, idx)` (`emitter/transform_dispatch.rs`) is the directive
path. It looks the directive up, clones it for emit, and `match`es it. For
`ES5Class` it: registers the ES5 class binding name, tries a fast
`render_simple_tc39_decorated_class_es5` path, collects leading comments, builds
a `ClassES5Emitter`, calls `emit_es5_class_output` to get a `String` plus offset
mappings, then splices that into the main writer via
`write_with_offset_mappings`, and finally advances the comment cursor past the
class body (`skip_comments_for_erased_node`). `ES5Namespace`/`ES5Enum`/async/
generator follow the same pattern: a sub-emitter renders the IR to a string with
relative mappings, and the `Printer` splices the string and rebased mappings
into the live output.

---

## Transforms and lowering, by feature

The downlevel transforms split into two styles:

- **Inline string emitters** that write directly to `SourceWriter` while the
  print pass walks the AST (most expression-level downlevel:
  optional-chain, nullish, spread, destructuring, template).
- **IR-based emitters** that build an `IRNode` tree and render it through an
  `IRPrinter` to a detached `String` + relative source-map `Mapping`s, which the
  `Printer` then splices in. Class, namespace, enum, async, and generator
  lowering use this style because they reorder and synthesize statements.

### The string IR (`transforms/ir.rs`, `IRNode` / `IRPrinter`)

`IRNode` (`transforms/ir.rs`) is a small JavaScript-construct tree:
`NumericLiteral`, `StringLiteral`, `Identifier`, `This { captured }`, `Super`,
`RuntimeHelper(name)`, `CallExpr`, etc. The transform analyses the AST once and
emits an `IRNode` tree; `IRPrinter::emit(node)` (`transforms/ir_printer.rs`)
walks the tree and produces text. The IR keeps transform logic testable apart
from string formatting and lets the printer apply indentation and source maps
consistently. The IR carries its own source-map machinery
(`transforms/ir_printer_source_map.rs`) so an ES5 class body still maps back to
the original source.

### ES5 class lowering walk-through

Input:

```typescript
class Point extends Base {
  x = 1;
  constructor(y) { super(); this.y = y; }
}
```

1. **Lowering** (`LoweringPass::visit_class_declaration`) records
   `TransformDirective::ES5Class { class_node, heritage: Some(Base) }`, sets
   `helpers.extends = true` (because of the heritage clause), and records the
   `_this`/`_super` capture facts.
2. **Print** reaches the class node; `emit_node` sees the directive and calls
   `apply_transform` -> the `ES5Class` arm.
3. The arm builds a `ClassES5Emitter` (`transforms/class_es5_ir.rs`), which
   converts the class AST to IR (`class_es5_ast_to_ir*`), then renders the IIFE
   shape: `var Point = (function (_super) { __extends(Point, _super); function
   Point(y) { _super.call(this, ...); this.x = 1; this.y = y; } return Point;
   }(Base));`.
4. **Field-init order:** instance field initializers are interleaved into the
   constructor body in **source order** relative to other fields, after the
   `super()` call. The collection step (`collect_class_es6_field_inits` in
   `emitter/declarations/class/emit_es6_field_inits.rs`) records each field as a
   `FieldInit` tuple carrying `member_node.pos` as the source-order key; the
   constructor synthesis emits `this.x = 1` at the field's original position.
   This matches `tsc`'s ordering exactly, which is observable when a field
   initializer reads a constructor parameter or an earlier field.
5. The IIFE string carries relative mappings; `write_with_offset_mappings`
   rebases them onto the current writer line/column and splices the text.

### Optional-chain downlevel walk-through

Input: `obj?.a.b?.(args)` at an ES target below ES2020
(`needs_es2020_lowering`).

The leaf optional-chain emitter (`emitter/expressions/access.rs`) writes the
guard `obj === void 0 ? void 0 : obj.a.b` and records
`optional_chain_sync_tail_start = Some(writer.len())` — the byte offset just
after the `: ` of the guard. A consuming optional call that needs the chain's
`this` receiver (here, `this = obj.a`) splices its `(_t = <tail>)` capture
*inside* the guard at that offset, so a short-circuit yields `void 0` instead of
dereferencing it. The `EmitFlags::optional_chain_needs_parens` flag, set by
prefix/postfix-unary and conditional-condition emitters, decides whether the
lowered ternary is parenthesized so that e.g. `o?.a++` lowers to
`(o === null || o === void 0 ? void 0 : o.a)++`. The same flag family drives
nullish-coalescing parenthesization (`nullish_coalescing_needs_parens` in
`emitter/expressions/binary_downlevel.rs`).

### Spread / `for-of` / destructuring

Spread arguments lower to `.apply` with `__spreadArray`
(`transforms/spread_es5.rs`): `foo(...arr, 1, 2)` becomes
`foo.apply(void 0, __spreadArray(__spreadArray([], arr, false), [1, 2], false))`.
`new Foo(...args)` uses the `Foo.bind.apply(...)` form. `for-of` lowering
(directive `ES5ForOf`) emits the array fast-path by default, or, with
`downlevel_iteration`, the full iterator protocol via `__values`/`__read`.
Destructuring lowering (`ES5VariableDeclarationList`, `transforms/destructuring_es5.rs`)
introduces temps from the file's `_a`, `_b`, ... sequence.

### Async / generator / `yield*`

`ES5AsyncFunction` lowers to the `__awaiter` + `__generator` envelope;
`ES5GeneratorFunction` to `__generator`. The body is converted to a state
machine in the `async_es5_ir*` family (`transforms/async_es5_ir_statements.rs`
and siblings). `yield*` (delegation) is opcode `5` (`yield**`,
`opcodes::YIELD_STAR`) and wraps its operand in `__values(x)` so the runtime
drives the delegate's iterator protocol; plain `yield` is opcode `4`
(`async_es5_ir_statements.rs`). The generator-state variable name is chosen by
`IRPrinter::generator_state_name_for_hoisted` (`ir_printer_generator_state.rs`),
which picks the next `_a..`-style name not already consumed by hoisted temps and
skips the reserved `_i` (index 8) and `_n` (index 13) slots, matching `tsc`'s
`TempFlags` allocator.

### Decorators

Two decorator models, selected by `PrinterOptions`: `legacy_decorators`
(experimental) lowers to `__decorate`/`__param`/`__metadata`
(`transforms/es_decorators*` and the legacy helpers in `transforms/helpers.rs`),
and the default TC39/standard model (directive `TC39Decorators`) lowers to the
`__esDecorate`/`__runInitializers`/`__setFunctionName` IIFE wrapper.
`emit_decorator_metadata` additionally requests the `metadata` helper and the
type-serialization path (`metadata_class_type_params` on the `Printer`).

---

## Module-format output

Module format is `PrinterOptions::module` (a `ModuleKind`). `emit_source_file`
(`emitter/source_file/emit.rs`) first resolves the *effective* format:

- `auto_detect_module` promotes `ModuleKind::None` to `CommonJS` when the file
  has import/export syntax (`file_is_module`).
- `Node16`/`Node18`/`Node20`/`NodeNext` resolve to `ESNext` for `.mts`/`.mjs`
  and `CommonJS` otherwise (by file extension).

`CommonJS` lowering lives in `transforms/module_commonjs*` and
`emitter/module_emission/` (imports, named/`default`/`export =` exports, live
re-exports). `AMD`, `System`, and `UMD` are whole-file wrappers driven by
`TransformDirective::ModuleWrapper` and emitted by `emitter/module_wrapper/`
(`system_emit`, `amd_bundle`, top-level-await and live-binding handling for
System). ES6/`Preserve` keeps native `import`/`export` and only elides
type-only bindings.

### Import elision

Whether an `import { x }` binding survives JS emit is *value-usage*: it survives
only if `x` is referenced in a value position. The accurate answer comes from
the binder via the `passes/import_value_usage.rs` pass, which computes
`ImportValueUsageFacts` once per file from `BinderState` symbol resolution plus a
syntactic classification of each identifier reference (only provably-erased
positions — type annotations, `typeof` queries, interface/type-alias bodies,
`implements`, ambient declarations — count as non-value; anything unresolved
conservatively counts as a value use, so the failure mode is over-preserving,
never wrongly eliding). These facts are threaded through
`PrinterOptions::import_usage_facts` and consulted at the elision sites in
`emitter/module_emission/imports.rs` and `lowering/import_usage.rs`. Binderless
callers (transpile-style emit) leave the facts `None` and fall back to the
conservative text heuristics in `crate::import_usage`.

---

## Helper scheduling

Runtime helpers are tracked in `HelpersNeeded` (`transforms/helpers.rs`): a
struct of booleans (`extends`, `assign`, `rest`, `awaiter`, `generator`,
`values`, `read`, `spread_array`, `class_private_field_get/set`, `es_decorate`,
`run_initializers`, `import_default`, `import_star`, …). Lowering and the print
pass set these flags as they discover features. The helper *source text* is a set
of constants in the same file (`EXTENDS_HELPER`, `ASSIGN_HELPER`,
`AWAITER_HELPER`, `GENERATOR_HELPER`, `VALUES_HELPER`, …), each emitted as the
`var __helper = (this && this.__helper) || function ...` guarded form `tsc` uses.

`HelpersNeeded` does not merely record *which* helpers are needed; it records
*order*. `tsc` emits helpers in **first-use order**, and the struct preserves
that with an `unprioritized_order: Vec<HelperEmitOrder>` plus targeted ordering
hooks:

- `mark_read` inserts `Read` *before* `SpreadArray` if spread was seen first,
  because `__read` must precede `__spreadArray` when both appear.
- `mark_async_values` similarly orders `AsyncValues` relative to `SpreadArray`.
- `class_private_field_set_before_get` flips Get/Set ordering when the first
  private-field operation is a plain assignment.
- `run_initializers_before_es_decorate` keeps member-decorator helper order
  stable for the same-priority case.

`write_helper(name)` (`emitter/helpers.rs`) emits a helper *call site*. With
`--importHelpers` under an effectively-CommonJS module it prefixes the per-file
tslib binding (`tslib_1.__awaiter`); under ESM the helper is imported directly so
no prefix is needed; if the helper name collided with a local identifier it
emits the renamed alias from `helper_import_aliases` (`__decorate_1`).

---

## Temp and hoist planning

Generated temporary names are file-scoped and follow `tsc`'s exact sequence.
`generate_fresh_temp_name` (`emitter/helpers.rs`) produces `_a, _b, _c, ..., _z,
_0, _1, ...` from a single counter (`DestructuringState::temp_var_counter`),
**skipping** counter 8 (`_i`) and 13 (`_n`) because `tsc` reserves those for
dedicated `TempFlags` (`_i` for loop indices, `_n`). Each candidate is checked
against `file_identifiers` (every identifier text in the source, collected once
at `emit_source_file` start — the analogue of `tsc`'s `sourceFile.identifiers`),
`generated_temp_names`, and `reserved_nested_temp_names`, so a temp never
shadows a user binding.

Allocation is layered:

- `make_unique_name` / `make_unique_name_fresh`: ordinary temps; the former
  drains a `preallocated_temp_names` queue first so that names reserved up front
  keep their lower ordinals.
- `make_unique_name_from_base(base)`: `base_1`, `base_2`, ... (used for named
  things like capture variables, reserving in the `BlockScopeState`).
- `make_unique_name_hoisted_assignment` / `..._hoisted_value` / `..._file_hoisted`:
  temps that must be declared in a hoisted `var _a, _b;` list rather than at use
  site; these record into `hoisted_assignment_temps` /
  `hoisted_assignment_value_temps` so the prologue declaration is emitted with the
  right names in the right order.

Function-scope boundaries save/restore the whole temp state. `push_temp_scope`
snapshots the counter, generated-name set, preallocated queues, and hoist pools
into a `TempScopeState` and resets the counter to 0; `pop_temp_scope` restores
them (`emitter/helpers.rs`). This makes per-function `_a` reuse match `tsc`,
which numbers temps per emit scope.

Hoist injection itself is a mechanical buffer edit: `SourceWriter::insert_at`
splices an inline `var _a; ` into a single-line body, and `insert_line_at`
inserts a full hoisted line and calls `shift_generated_lines` on the source map
so existing mappings move down (`output/source_writer.rs`,
`tsz_common::source_map`).

---

## The declaration emitter (`.d.ts`)

`DeclarationEmitter` (`declaration_emitter/core/mod.rs`, behind the `dts`
cargo feature) emits `.d.ts` text. Its entry point is `emit(source_file)`. It is
a separate emitter from the JS `Printer` — it produces only type-surface output
and erases all bodies.

Its single semantic dependency is the read-only `TypeCacheView`
(`type_cache_view.rs`):

```rust
pub struct TypeCacheView {
    pub node_types: FxHashMap<u32, TypeId>,
    pub symbol_types: FxHashMap<SymbolId, TypeId>,
    pub def_to_symbol: FxHashMap<DefId, SymbolId>,
    pub def_types: FxHashMap<u32, TypeId>,           // DefId.0 -> resolved body TypeId
    pub def_type_params: FxHashMap<u32, Vec<TypeParamInfo>>,
    pub boxed_types: FxHashMap<IntrinsicKind, TypeId>,
    // ...
    pub def_to_name: FxHashMap<DefId, String>,
}
```

This is a *projection* of checker-produced caches: inferred node/symbol types,
the `DefId -> TypeId` resolution from `TypeEnvironment`, and well-known symbol
names. The declaration emitter looks these up to print a function's inferred
return type or a variable's inferred type; it does **not** re-run inference,
relations, or evaluation. The `def_types`/`def_type_params` entries let it
evaluate cross-file type-alias applications referenced as `Lazy(DefId)` *by
looking up the already-resolved body*, consistent with the architecture rule
that the checker stabilizes `DefId` and `TypeEnvironment` resolves `DefId ->
TypeId`. The emitter is `Arc`-shared and read-only after construction, so per-file
and scratch declaration emitters reference one program-produced view rather than
deep-cloning cache maps.

Beyond type printing, the declaration emitter owns `.d.ts`-specific concerns
none of which involve semantic validation:

- **Import elision and rewriting** (`declaration_emitter/core/import_rewrites.rs`,
  `helpers/default_import_alias_rewrite.rs`): drop type-only imports, rewrite
  module specifiers, generate required imports for foreign symbols.
- **Public-API surface filtering** (`emit_public_api_only`,
  `usage_analyzer/public_surface.rs`): only emit declarations reachable from the
  exported surface.
- **Portability checks** (`helpers/portability_*`): the TS2883-style checks for
  references to symbols that are not portably nameable from the output file.
- **JS/JSDoc-driven declaration emit** (`core/js_emit*.rs`,
  `helpers/jsdoc*.rs`): inferring declarations for `.js` inputs with JSDoc.
- **Recursive-expansion limits** that match `tsc`: `MAX_RECURSIVE_EXPANSION = 10`
  visible levels for object-shaped returns and
  `MAX_RECURSIVE_INTERSECTION_EXPANSION = 5` for intersection-callable returns,
  emitting the exact `ELIDED_ANY = "/*elided*/ any"` string when the limit is
  reached (constants in `lib.rs`). These reproduce `tsc`'s observable
  truncation, not an internal recursion guard.

---

## Output buffer and source maps

`SourceWriter` (`output/source_writer.rs`) is the only thing that touches the
output `String`. The `Printer` and `DeclarationEmitter` delegate all text writes
to it. It tracks `line`, `column`, `indent_level`, and lazy indentation
(`at_line_start`). Crucially, **column counting uses UTF-16 code units**, not
bytes — `raw_write` uses `memchr` for newline scanning with an ASCII fast path
and a `len_utf16()` slow path — because Source Map v3 columns and `tsc`'s scanner
line map are UTF-16 based.

Writes come in two flavors:

- `write` / `write_char` / `write_usize`: "syntax glue" with no source mapping
  (delimiters, keywords, spacing).
- `write_node` / `write_node_with_name` / `write_node_with_end`: text derived
  from a source node; these call into the optional `SourceMapGenerator` to add a
  mapping from the current generated `(line, column)` to the node's original
  `(line, column)`. `write_node_with_end` adds a second end-of-token mapping that
  `tsc` emits for single-character tokens (`;`, `{`, `}`).
  `write_node_with_name` registers an entry in the source-map `names` table for
  identifiers.

Original positions come from `LineMap` (`output/source_writer.rs`), a thin
adapter over `tsz_common::position::LineMap` that maps a byte offset to a
UTF-16 `(line, column)` in O(log n) via binary search over a precomputed line
table. The `Printer` builds it once when source text is set (field
`line_map: Option<LineMap<'a>>`); without it, computing positions would be
O(pos) per call and O(n^2) per file.

`SourceMapGenerator` (`tsz_common/src/source_map/mod.rs`) accumulates `Mapping`s
and `generate_json` sorts them by generated position, VLQ-encodes the segment
deltas into the `mappings` string, and serializes a Source Map v3 `SourceMap`
(`version: 3`, `sources`, `sourcesContent`, `names`, `mappings`). It panics
rather than silently saturate if any segment value would overflow `i32`, because
a saturated VLQ segment would be syntactically valid but semantically wrong.

### Splicing transform output into the map

IR-based transforms (ES5 class, async, …) render to a detached `String` with
*relative* mappings. The `Printer` rebases those when splicing:

- `add_offset_mappings(base_line, base_column, mappings)` shifts a block of
  mappings by the current splice point (column offset applies only to line 0).
- `inline_capture_from` + `add_inline_capture_mappings` capture a scratch
  sub-render with its own relative `SourceMapGenerator` (cloned name table) and
  merge it back, syncing the names table.
- `shift_generated_lines` adjusts every mapping at/after a line when a hoisted
  line is inserted after emit.

This is how an ES5 class IIFE — whose statements were reordered and synthesized —
still maps each emitted token back to the source it came from.

---

## Caches and invariants

- **`EmitPlan` snapshots helpers up front.** `EmitPlan::from_transforms`
  (`context/plan.rs`) clones the lowering pass's `HelpersNeeded` into the plan,
  so the print pass starts from the helper set lowering already discovered and
  only adds to it. `EmitPlan` is the explicit boundary that will absorb the temp/
  hoist/export/region scheduling currently still discovered while printing
  (the `EmitTempPlan`/`EmitHoistPlan`/`EmitExportPlan`/`EmitRegionPlan` slots are
  the typed homes for that migration).
- **`file_identifiers` is collected once** at `emit_source_file` start and is the
  invariant backstop that no generated temp ever shadows a user identifier.
- **`generated_temp_names` is monotonic within a temp scope** and saved/restored
  at function boundaries; the counter resets to 0 on entering a new scope so
  per-function `_a` reuse matches `tsc`.
- **`comment_emit_idx`** is a single monotonically-advancing cursor into the
  file's collected comments, shared across `emit_source_file`, `emit_block`, and
  the transform arms, so a comment is emitted exactly once even when a node is
  erased or relocated by a transform (`skip_comments_for_erased_node`).
- **`TypeCacheView` is read-only and `Arc`-shared** across the per-file and
  scratch declaration emitters; the declaration emitter never mutates it and
  never recomputes a type it can look up.
- **Recursion is bounded everywhere:** `MAX_EMIT_RECURSION_DEPTH = 10_000` in the
  print pass, `MAX_AST_DEPTH = 500` (plus the 100-deep qualified-name and
  binding-pattern guards) in lowering, and the `MAX_RECURSIVE_EXPANSION` /
  `MAX_RECURSIVE_INTERSECTION_EXPANSION` `tsc`-parity limits in `.d.ts` type
  printing.
- **Debug-only delimiter balance check:** `SourceWriter` tracks open/close
  delimiters written through `write_open_delimiter`/`write_close_delimiter` and
  `debug_assert!`s on `take_output` that nothing was left unbalanced.

---

## Edge cases and `tsc` parity

- **`"use strict"` placement.** `emit_source_file` emits it before comments and
  helpers, only for CommonJS *modules* (script files with no import/export do not
  get it), inside the `define()` body for AMD/UMD, when `always_strict` is set on
  non-ESM output, and never for ES-module output (ESM is implicitly strict) or
  when the source already has a `"use strict"` prologue directive. `.cts`/`.cjs`
  files whose module was overridden from ESM to CJS suppress it
  (`suppress_use_strict`).
- **Temp-name skips.** `_i` and `_n` are skipped in the generated sequence
  because `tsc` reserves them; the generator-state name picker skips them too.
- **First `for-of` uses `_i`.** The first `for-of` in a scope uses the reserved
  `_i` index name (`first_for_of_emitted`), matching `tsc`.
- **Const-enum inlining.** A pre-pass (`collect_const_enum_values`) inlines
  member access like `Direction.Up` to its literal value with a `/* Direction.Up */`
  comment. `--preserveConstEnums` keeps the *declaration* but still inlines uses;
  `--isolatedModules` / `--verbatimModuleSyntax` set `no_const_enum_inlining` and
  disable inlining entirely (cross-file const enums cannot be inlined).
- **Static no-init field erasure.** A `static x;` with no initializer is erased
  unless `useDefineForClassFields` is set (`static_no_init_field_is_erased`),
  matching `tsc`.
- **Instance-field init order** is source order relative to other fields and
  after `super()`, observable when an initializer references a constructor
  parameter or earlier field.
- **Optional-chain receiver capture** splices `(_t = tail)` *inside* the guard so
  a short-circuited chain never dereferences `void 0`.
- **Helper first-use ordering** is preserved (`__read` before `__spreadArray`,
  private-field Set-before-Get when the first op is assignment), because `tsc`'s
  helper emit order is observable in baselines.
- **`.d.ts` recursive truncation** emits the exact `/*elided*/ any` text at
  `tsc`'s visible depth limits rather than an internal sentinel.

These behaviors are all *structural*: they are decided from target facts, module
kind, binder/checker-provided facts, and AST shape — never from a fixture name or
by reading the emitter's own rendered output back as a predicate.

---

## See also

- [binder](binder.md) — symbols, scopes, and the value-usage facts the JS
  import-elision pass consumes.
- [checker-declarations-modules](checker-declarations-modules.md) and
  [solver-types-intern-def](solver-types-intern-def.md) — where the `TypeId`s
  and `DefId` resolutions in `TypeCacheView` come from.
- [solver-instantiation](solver-instantiation.md) and
  [solver-evaluation](solver-evaluation.md) — the type computation the
  declaration emitter consumes but never repeats.
- [end-to-end-timeline](end-to-end-timeline.md) — where emit sits in the full
  compile.
