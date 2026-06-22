# Emit Helpers and Decorator Metadata Serialization

The sibling document
[Async/Generator State Machines, Decorators, and Module-Format Wrapping in Emit](emitter-async-generator-decorators-modules.md)
names the runtime helpers (`__awaiter`, `__decorate`, `__esDecorate`, …) and
states that the emitter reproduces their exact source text and the legacy-vs-TC39
decorator split. It deliberately leaves three machines underspecified: (1) the
*helper library itself* — where the canonical helper source strings live, how a
transform *requests* a helper, how requests are deduplicated, and the precise
**first-use ordering** that decides the order helpers appear in the file; (2) the
**inline-vs-imported** decision (`--importHelpers`/`tslib`, `--noEmitHelpers`),
including the collision-aliasing of `import { __decorate as __decorate_1 }`; and
(3) **`emitDecoratorMetadata`** — the design-time type serializer that turns a
type *annotation* (a syntax node, never a solver type) into the runtime
value-expression `tsc` writes into `__metadata("design:type", T)`,
`"design:paramtypes"`, and `"design:returntype"`. This document fills exactly
those gaps and treats the decorator *call* emission (the `__decorate([...], C,
"k", null)` shape) as owned by the sibling.

The whole surface obeys one rule it shares with every other emit pass: it is a
string/IR builder driven by **AST shape and `PrinterOptions`**, never by the
[solver](solver-relations.md). The metadata type serializer in particular does
**not** ask the checker "what is the type of this property" — it walks the type
*annotation node* with a syntactic visitor (`Printer::serialize_type_for_metadata`),
which is precisely what `tsc`'s `serializeTypeNode` does and is why metadata under
inference-only members serializes to `Object`. Anything genuinely semantic
(reachability, legality of a decorator, whether an import is type-only) was
already decided upstream by the [checker](checker-declarations-modules.md); emit
only lowers.

---

## Owns / Must not own

| Owns | Must not own |
| --- | --- |
| The canonical helper source-text constants (`EXTENDS_HELPER`, `DECORATE_HELPER`, `METADATA_HELPER`, `ES_DECORATE_HELPER`, …) in `transforms/helpers.rs`. | Generating helper source at runtime or templating it. The strings are fixed `tsc` boilerplate, byte-for-byte. |
| The helper **request** model: `HelpersNeeded` flags + `helpers_mut()`, the `mark_*` request methods, and the `unprioritized_order` first-use list. | Deciding *that* a transform is needed (that is the [lowering pass](emitter-async-generator-decorators-modules.md) reading AST shape). |
| Helper **dedup + ordering**: `emit_helpers` (inline) and `needed_names` (import), implementing `tsc`'s `compareEmitHelpers` priority tiers. | Re-ordering helpers by output surgery after the fact. Order is computed before any helper byte is written. |
| The inline-vs-`tslib` decision, the `import { … } from "tslib"` / `require("tslib")` synthesis, the per-file binding (`tslib_1`), and collision aliasing (`helper_import_aliases`). | Module *resolution* of `tslib`. Specifier resolution is [module-resolution-engine](module-resolution-engine.md). |
| `emitDecoratorMetadata` design-time serialization (`serialize_type_for_metadata`, `serialize_param_types_to_string`, the numeric-enum / async-Promise / union-collapse rules). | Reading a solver `TypeId`. The serializer is purely syntactic over the annotation node. |
| Keeping a metadata-referenced import alive against type-only elision (`name_appears_in_decorator_metadata_type`). | Deciding import elision policy in general; this is a targeted exception to it. |

---

## Module map

| Path | Role |
| --- | --- |
| `crates/tsz-emitter/src/transforms/helpers.rs` | The helper library. All `*_HELPER` source-text constants, the `HelpersNeeded` struct, `HelperEmitOrder`, the `mark_*` request methods, `emit_helpers` (inline text), `needed_names` (tslib import names). |
| `crates/tsz-emitter/src/context/transform.rs` | `TransformContext::helpers` storage and the `helpers()` / `helpers_mut()` accessors used by every requesting site. |
| `crates/tsz-emitter/src/lowering/decorator_helpers.rs` | Where TC39 decorator helpers are requested (`mark_tc39_decorator_helpers`: `es_decorate`, `run_initializers`, `prop_key`, `set_function_name`, ordering flags). |
| `crates/tsz-emitter/src/lowering/core.rs` | Where the legacy class-level `metadata` helper is requested for constructor paramtypes. |
| `crates/tsz-emitter/src/emitter/source_file/emit.rs` | The top-of-file emission of helpers: inline block vs `tslib` import vs `tslib` require, the collision aliasing loop, the hoist-position bookkeeping. |
| `crates/tsz-emitter/src/emitter/helpers.rs` | `Printer::write_helper` — resolves a helper call name to a bare name, an aliased name, or a `tslib_1.`-prefixed CJS member at the call site. |
| `crates/tsz-emitter/src/emitter/declarations/class/decorators.rs` | Legacy (`experimentalDecorators`) `__metadata` call emission **and** `serialize_type_for_metadata` (the design:type serializer) for the ES2015+ path. |
| `crates/tsz-emitter/src/transforms/class_es5_ir_helpers.rs` | The ES5-IR mirror of the serializer (`serialize_type_for_metadata`, `serialize_param_types`) used when the class is lowered to an IIFE. |
| `crates/tsz-emitter/src/transforms/class_es5_ir_decorators.rs` | ES5-IR `__metadata(...)` string assembly for the IIFE-class path. |
| `crates/tsz-emitter/src/import_usage.rs` | `name_appears_in_decorator_metadata_type` — the syntactic scan that keeps a metadata-referenced import from being elided as type-only. |
| `crates/tsz-emitter/src/emitter/module_wrapper/system_helpers.rs`, `wrapper_entry.rs` | AMD/UMD/System wrappers OR-merge `decorate`/`param`/`metadata` requests and emit interop helpers before the wrapper body. |

---

## The helper library: source text is fixed

Every helper is a `pub const … : &str` raw-string literal in
`crates/tsz-emitter/src/transforms/helpers.rs`. There is no codegen, no
templating, no per-file substitution of the body — the body is copied verbatim
into the output. Each one is the self-installing `var __x = (this && this.__x) ||
…;` form `tsc` uses, so the helper is idempotent across files concatenated into a
single script:

```rust
// transforms/helpers.rs
pub const DECORATE_HELPER: &str = r#"var __decorate = (this && this.__decorate) || function (decorators, target, key, desc) {
    var c = arguments.length, r = c < 3 ? target : desc === null ? desc = Object.getOwnPropertyDescriptor(target, key) : desc, d;
    if (typeof Reflect === "object" && typeof Reflect.decorate === "function") r = Reflect.decorate(decorators, target, key, desc);
    else for (var i = decorators.length - 1; i >= 0; i--) if (d = decorators[i]) r = (c < 3 ? d(r) : c > 3 ? d(target, key, r) : d(target, key)) || r;
    return c > 3 && r && Object.defineProperty(target, key, r), r;
};"#;

pub const METADATA_HELPER: &str = r#"var __metadata = (this && this.__metadata) || function (k, v) {
    if (typeof Reflect === "object" && typeof Reflect.metadata === "function") return Reflect.metadata(k, v);
};"#;
```

The full catalogue (each a constant of the same shape):

| Constant | Helper | First requested by |
| --- | --- | --- |
| `EXTENDS_HELPER` | `__extends` | ES5 class `extends` lowering |
| `ASSIGN_HELPER` | `__assign` | object spread below ES2018 |
| `REST_HELPER` | `__rest` | object destructuring rest below ES2018 |
| `DECORATE_HELPER` | `__decorate` | legacy decorators |
| `PARAM_HELPER` | `__param` | legacy parameter decorators |
| `METADATA_HELPER` | `__metadata` | `--emitDecoratorMetadata` |
| `AWAITER_HELPER` | `__awaiter` | `async`/`await` downleveling |
| `GENERATOR_HELPER` | `__generator` | generator downleveling |
| `VALUES_HELPER` / `READ_HELPER` | `__values` / `__read` | `for..of` / array destructuring below ES2015 |
| `SPREAD_ARRAY_HELPER` | `__spreadArray` | array/call spread below ES2015 |
| `AWAIT_HELPER` / `ASYNC_GENERATOR_HELPER` / `ASYNC_DELEGATOR_HELPER` / `ASYNC_VALUES_HELPER` | `__await` / `__asyncGenerator` / `__asyncDelegator` / `__asyncValues` | async generators / `for await` |
| `IMPORT_DEFAULT_HELPER` / `IMPORT_STAR_HELPER` | `__importDefault` / `__importStar` | `esModuleInterop` imports |
| `CREATE_BINDING_HELPER` / `SET_MODULE_DEFAULT_HELPER` / `EXPORT_STAR_HELPER` | `__createBinding` / `__setModuleDefault` / `__exportStar` | CJS re-export / namespace import |
| `MAKE_TEMPLATE_OBJECT_HELPER` | `__makeTemplateObject` | tagged templates below ES2015 |
| `CLASS_PRIVATE_FIELD_GET_HELPER` / `_SET_` / `_IN_` | `__classPrivateField{Get,Set,In}` | private fields below ES2022 |
| `ADD_DISPOSABLE_RESOURCE_HELPER` / `DISPOSE_RESOURCES_HELPER` | `__addDisposableResource` / `__disposeResources` | `using` / `await using` |
| `ES_DECORATE_HELPER` / `RUN_INITIALIZERS_HELPER` / `PROP_KEY_HELPER` / `SET_FUNCTION_NAME_HELPER` | `__esDecorate` / `__runInitializers` / `__propKey` / `__setFunctionName` | TC39 (standard) decorators |
| `REWRITE_RELATIVE_IMPORT_EXTENSION_HELPER` | `__rewriteRelativeImportExtension` | `--rewriteRelativeImportExtensions` |

Note `IMPORT_STAR_HELPER`'s body calls `__createBinding` and `__setModuleDefault`,
so requesting `import_star` transitively forces those two — handled at *emit* time
(see ordering) rather than by the request methods.

---

## Requesting a helper: `HelpersNeeded` and `mark_*`

Helper requests are accumulated on a single `HelpersNeeded` value stored in
`TransformContext` (`context/transform.rs`, field `helpers`, accessed through
`helpers()` / `helpers_mut()`; also surfaced on `EmitPlan` in `context/plan.rs`).
The struct is one `bool` per helper plus an **order list**:

```rust
// transforms/helpers.rs
#[derive(Default, Clone)]
pub struct HelpersNeeded {
    pub extends: bool, pub assign: bool, pub rest: bool,
    pub decorate: bool, pub param: bool, pub metadata: bool,
    pub awaiter: bool, pub generator: bool,
    pub es_decorate: bool, pub run_initializers: bool,
    pub run_initializers_before_es_decorate: bool,   // ordering tiebreak
    pub prop_key: bool, pub set_function_name: bool,
    pub class_private_field_set_before_get: bool,     // ordering tiebreak
    // … one flag per helper …
    pub unprioritized_order: Vec<HelperEmitOrder>,    // first-use order
}
```

Two kinds of write exist. **Direct flag writes** (`helpers.es_decorate = true`,
`helpers.metadata = true`) are used for helpers whose relative position is fixed
by a priority tier. **`mark_*` request methods** (`mark_rest`, `mark_spread_array`,
`mark_read`, `mark_values`, `mark_await_helper`, `mark_async_generator`,
`mark_async_delegator`, `mark_async_values`, `mark_import_default`,
`mark_class_private_field_{get,set,in}`) are used for the *unprioritized* helpers —
the ones `tsc` emits in **first-use order** rather than a fixed tier. Each
`mark_*` sets the bool **and** appends to `unprioritized_order` via
`remember_unprioritized`, which is idempotent (it skips if the helper is already
present), giving free dedup:

```rust
fn remember_unprioritized(&mut self, helper: HelperEmitOrder) {
    if !self.unprioritized_order.contains(&helper) {
        self.unprioritized_order.push(helper);
    }
}
pub fn mark_rest(&mut self) {
    self.rest = true;
    self.remember_unprioritized(HelperEmitOrder::Rest);
}
```

`mark_read` and `mark_async_values` are special: they *insert before* any already
recorded `SpreadArray` rather than appending, matching `tsc`'s ordering where
`__read` and `__asyncValues` precede `__spreadArray` even when requested later.

Requests happen during the lowering pass (so `emit.rs` can read a finished
`HelpersNeeded` without re-scanning the arena). For TC39 decorators the request
site is `lowering/decorator_helpers.rs::mark_tc39_decorator_helpers`:

```rust
let helpers = self.transforms.helpers_mut();
helpers.es_decorate = true;
helpers.run_initializers = true;
if has_decorated_method_or_accessor {
    helpers.run_initializers_before_es_decorate = true; // tsc request-order tiebreak
}
if needs_prop_key { helpers.prop_key = true; }
if needs_set_function_name || needs_class_set_fn_name { helpers.set_function_name = true; }
```

`run_initializers_before_es_decorate` is set exactly when the class has a
decorated method/getter/setter, because those members request the method
`__runInitializers` *while the element is processed*, before the class-level
`__esDecorate` call is built — so `tsc` emits `__runInitializers` first. Decorated
fields, auto-accessors, and bare class decorators do not, so they keep
`__esDecorate` first. This is a faithful reproduction of `tsc`'s request-order
dependence, not a heuristic. Legacy `metadata` for the class-level constructor
case is requested in `lowering/core.rs` (only when `legacy_decorators &&
emit_decorator_metadata && class has a class-level decorator && a constructor
exists).

---

## Dedup and ordering: `emit_helpers` and `needed_names`

The same `HelpersNeeded` drives two consumers, each emitting in `tsc`'s
`compareEmitHelpers` order. Inline emission is `emit_helpers(&HelpersNeeded) ->
String`; the tslib import-name list is `needed_names() -> Vec<&'static str>`. Both
implement the documented priority tiers (the doc-comment in `emit_helpers`):

```text
priority 0 : __extends, __makeTemplateObject
priority 1 : __assign, __createBinding, __setModuleDefault
priority 2 : __decorate, __esDecorate/__runInitializers, __propKey,
             __importStar, __exportStar
priority 3 : __metadata
priority 4 : __param
priority 5 : __awaiter
priority 6 : __generator
priority 7 : disposable helpers
no priority (last, first-use order): __await, __asyncGenerator, __asyncDelegator,
             __rest, __values, __read, __spreadArray, __asyncValues,
             __importDefault, __classPrivateField{Get,Set,In}
```

`emit_helpers` walks the tiers in order, pushing the helper's source-text constant
whenever its flag is set; dedup is structural (a flag is one bool, written once).
The unprioritized tail is emitted by `emit_unprioritized_helpers`, which first
walks `unprioritized_order` (the recorded first-use order) and then a fixed
`fallback_unprioritized_order`, guarding against double emission with an `emitted:
Vec<HelperEmitOrder>` set (`emit_unprioritized_helper` early-returns if
`emitted.contains(&helper)`). The fallback exists for code paths that set a bool
directly without going through `mark_*` (so `unprioritized_order` may be empty
even though a bool is set).

Two intra-tier tiebreaks are encoded as separate flags rather than position:

- `run_initializers_before_es_decorate` swaps the order of `__runInitializers` and
  `__esDecorate` within priority 2 (see the two mirrored `if` blocks around
  `RUN_INITIALIZERS_HELPER` in `emit_helpers`).
- `class_private_field_set_before_get` swaps `__classPrivateFieldSet` ahead of
  `__classPrivateFieldGet` when the first private-field operation in the file is a
  plain assignment (a `Set` before any `Get`). `emit_class_private_helpers` and
  `fallback_class_private_order` both consult it.

`__setModuleDefault` is not a `HelpersNeeded` flag — `emit_helpers` emits it
whenever `import_star` is set, because the star helper's body calls it. It is a
priority-1 helper in `tsc`'s `compareEmitHelpers` table
(`typescript:commonjscreatevalue`), so it is emitted in the priority-1 tier
(right after `__createBinding`), before any priority-2 helper — not bundled with
`__importStar`. `__setFunctionName` floats: it is emitted
right after the priority-6 block when paired with `es_decorate` (TC39), but with
the unprioritized helpers otherwise — `needed_names` mirrors this with the two
`set_function_name` pushes around `push_unprioritized_names`.

A regression test (`needed_names_priority_order_for_full_set`) locks the entire
canonical order with every flag set, so a missing, reordered, or duplicated entry
fails the build.

```text
 LoweringPass (Phase 1)                emit.rs top-of-file (Phase 2)
 ─────────────────────                 ──────────────────────────────
  mark_rest()  ─┐                        helpers = ctx.helpers().clone()
  decorate=true ├─► HelpersNeeded ─────►  if importHelpers && ESM:
  es_decorate=t │   { bools +                needed_names() → import{…}from"tslib"
  mark_read()  ─┘    unprioritized_order } elif importHelpers && CJS:
                                              var tslib_1 = require("tslib")
                                           else if !noEmitHelpers:
                                              emit_helpers() → inline var __x = …
```

---

## Inline vs imported: `--importHelpers`, `tslib`, `--noEmitHelpers`

The top-of-file decision lives in `emitter/source_file/emit.rs`. The emitted file
order is fixed: `"use strict"` → ESM jsx-import → ESM tslib-import → inline helpers
→ `__esModule` → CJS tslib-require → exports init. The three mutually exclusive
outcomes:

1. **Inline** (default, `!noEmitHelpers && !(importHelpers && is_file_module)`):
   `emit_helpers(&helpers)` is concatenated into the output as a block of `var __x
   = …;` declarations.
2. **ESM tslib import** (`importHelpers && !is_commonjs() && helpers.any_needed()`):
   `needed_names()` is sorted (`names.sort_unstable()`) and written as
   `import { __assign, __decorate, … } from "tslib";`.
3. **CJS tslib require** (`importHelpers && needs_tslib_binding`): a binding var
   (`var tslib_1 = require("tslib");`) is synthesized, and call sites are prefixed
   `tslib_1.__decorate(...)`.

`HelpersNeeded::any_needed()` gates whether *any* helper machinery runs — it ORs
every helper bool but deliberately **excludes** the two ordering-only flags
(`class_private_field_set_before_get` is bookkeeping; a unit test asserts it alone
does not flip `any_needed`, so the emitter does not install a tslib import for a
no-op state).

### Collision aliasing

When a helper name collides with a local file identifier (e.g. a `declare var
__decorate`), `tsc` imports it under an alias and uses the alias at every call
site. `emit.rs` reproduces this during the ESM-import loop: for each needed name,
if `self.file_identifiers.contains(name)`, it picks the first free
`__decorate_1`, `__decorate_2`, … (avoiding both file identifiers and
already-assigned aliases), writes `import { __decorate as __decorate_1 }`, and
records the mapping in `self.helper_import_aliases`:

```rust
let collides = self.file_identifiers.contains(name_str);
if collides {
    let mut suffix = 1u32;
    let mut alias = format!("{name_str}_{suffix}");
    while self.file_identifiers.contains(alias.as_str())
        || self.helper_import_aliases.values().any(|v| v == &alias) {
        suffix += 1; alias = format!("{name_str}_{suffix}");
    }
    self.write(" as "); self.write(&alias);
    self.helper_import_aliases.insert(name_str.to_string(), alias);
}
```

### Resolving a helper *call*

`Printer::write_helper` (`emitter/helpers.rs`) is the single chokepoint every
`__decorate`/`__metadata`/`__param`/`__awaiter` call site uses to print the
callee. It resolves three ways in priority order:

```rust
pub(super) fn write_helper(&mut self, name: &str) {
    if self.ctx.options.import_helpers && self.ctx.is_effectively_commonjs() {
        let binding = self.commonjs_tslib_import_binding.clone(); // "tslib_1"
        self.write(&binding); self.write("."); self.write(name);  // tslib_1.__decorate
        return;
    }
    if let Some(alias) = self.helper_import_aliases.get(name) {
        let alias_owned = alias.clone();
        self.write(&alias_owned);                                 // __decorate_1
        return;
    }
    self.write(name);                                             // __decorate
}
```

The CJS `tslib` binding is `tslib_1` by default
(`commonjs_tslib_import_binding`), reset per source file in
`prepare_source_file_emit_state` *unless* inside a module wrapper body (an outFile
second module rebinds it, e.g. `tslib_2`, and resetting would clobber it). AMD/UMD/
System wrappers OR-merge `decorate`/`param`/`metadata` requests into the wrapper's
`HelpersNeeded` (`module_wrapper/system_helpers.rs`, `wrapper_entry.rs`) and emit
interop helpers (`create_binding`, `import_star`, `import_default`) *before* the
wrapper body, so `emit.rs` suppresses them inside the body to avoid double
emission.

---

## Decorator metadata serialization (`--emitDecoratorMetadata`)

When `emit_decorator_metadata` is set and the file uses `experimentalDecorators`,
`tsc` emits design-time reflection calls alongside the legacy `__decorate` array.
For the ES2015+ path these are emitted from
`emitter/declarations/class/decorators.rs`; the ES5-IIFE path mirrors them through
`transforms/class_es5_ir_decorators.rs` using the shared serializer in
`class_es5_ir_helpers.rs`. The three keys and where each is produced:

| Member kind | Emitted calls | Function |
| --- | --- | --- |
| Decorated **property** | `__metadata("design:type", T)` | `emit_metadata_for_property` |
| Decorated **method** | `__metadata("design:type", Function)`, `__metadata("design:paramtypes", [...])`, `__metadata("design:returntype", R)` | `emit_metadata_for_method` |
| Decorated **accessor** (get/set pair) | `__metadata("design:type", T)`, `__metadata("design:paramtypes", [...])` | `emit_metadata_for_accessor` |
| Class-level decorator + constructor | `__metadata("design:paramtypes", [...])` | `emit_metadata_for_constructor_params` |

A method's `design:type` is **always `Function`** (a literal, not serialized). The
`design:returntype` for a method follows the async rule: if the method has an
explicit return annotation it is serialized; otherwise, if the method is `async`
and is **not** an async *generator* (`has_async_modifier && !has_generator_asterisk`,
computed in the per-member match), it serializes to the literal `Promise`; else
`void 0`:

```rust
} else if async_returns_promise {
    self.write_helper("__metadata");
    self.write("(\"design:returntype\", Promise)");
} else {
    self.write_helper("__metadata");
    self.write("(\"design:returntype\", void 0)");
}
```

Members are emitted **instance/prototype-first then static**, preserving source
order within each partition (`ordered_members`), matching `tsc`. Getter/setter
pairs collapse to a single `__decorate` call; the accessor `design:type` prefers
the setter's first parameter type, falling back to the getter's return type, else
`Object` (`accessor_metadata_strings`). The `this` parameter is skipped in
paramtypes (`serialize_param_types_to_string`), and a rest parameter
(`...args: T[]`) serializes its array *element* type, else `Object`
(`serialize_rest_param_element_type`).

### `serialize_type_for_metadata`: the type-node → value-expression map

The core is `Printer::serialize_type_for_metadata(type_idx)` — a **syntactic
visitor over the annotation node**, mirroring `tsc`'s `serializeTypeNode`. It never
consults the solver; the input is the `NodeIndex` of the written type annotation.
The mapping:

| Type-annotation node | Serialized value |
| --- | --- |
| `string` / `String`-ref | `String` |
| `number` / `number`-ref | `Number` |
| `boolean` | `Boolean` |
| `symbol` | `Symbol` |
| `bigint` | `BigInt` |
| `void` / `undefined` / `null` / `never` | `void 0` |
| `any` / `unknown` / `object` | `Object` |
| `T[]` (`ARRAY_TYPE`), tuple | `Array` |
| function/constructor type | `Function` |
| template-literal type | `String` |
| literal type | inferred from the literal (`"x"`→`String`, `1`→`Number`, `-1`→`Number`, `1n`→`BigInt`, `true`/`false`→`Boolean`, `null`→`void 0`) |
| intersection, this-type, indexed-access, mapped, type-query, infer, import, default `_` | `Object` |
| `readonly`/`keyof`/`unique` (`TYPE_OPERATOR`), `T?` (`OPTIONAL_TYPE`), parenthesized | unwrap and recurse |
| conditional type | serialize both branches; if equal, use it, else `Object` |
| union | see below |
| type reference | `serialize_type_reference_for_metadata` (see below) |

**Unions** (`serialize_type_for_metadata`, `UNION_TYPE` arm) filter out
`null`/`undefined`/`void`/`never` members — but only strip `null`/`undefined`/
`void` when `strictNullChecks` is **false** (under strict-null, those are
meaningful and kept; `never` is always stripped). After filtering: a single
remaining member is serialized directly; multiple members that all serialize to
the *same* non-`Object` value collapse to that value; an all-stripped union is
`void 0`; otherwise `Object`. This is what makes `string | null` → `String` (non-
strict) but `string | number` → `Object`.

**Type references** (`serialize_type_reference_for_metadata`) decompose the entity
name into dotted parts (`metadata_entity_name_parts`, recursing through
`QUALIFIED_NAME`). Several structural rules apply, none keyed on a printed string:

- **Generic type-parameter erasure**: if the root name is one of the class's type
  parameters (gathered into `metadata_class_type_params` in
  `emit_declaration.rs` when the class has type parameters and metadata is on),
  the reference serializes to `Object`. So `@dec foo(): T` → `design:returntype
  Object`.
- **Numeric-enum → `Number`**: if the reference's leading (or member-owning)
  segment resolves to a *declared enum in this file whose every member is numeric*
  (`metadata_reference_is_numeric_enum` → `is_declared_numeric_enum`), it
  serializes to `Number`. This recognizes bare `E`, member `E.A`, and — via the
  union "all members agree" rule — `E.B | E.C` and `E | number`. Numeric-vs-string
  membership is a **syntactic** test on member initializers
  (`metadata_enum_initializer_is_string`: string/template literals and `"x" + …`
  concatenations are string; numeric/bitwise/member-ref are numeric), the same
  split the ES5 enum transform uses — it does not depend on the printed type or
  solver evaluation.
- **Single-segment builtin names** map like keyword types
  (`serialize_identifier_type_reference_for_metadata`: `string`→`String`, etc.).
- **Otherwise the name is emitted as a value reference** (the dotted entity name),
  which is exactly why a metadata-referenced class/enum must survive import
  elision.

### Keeping metadata-referenced imports alive

Because a `design:type` reference becomes a *value* expression at runtime, the
import that owns the name must not be elided as type-only. The standard type-only
strip would remove it from the value-usage haystack, so `import_usage.rs` provides
`name_appears_in_decorator_metadata_type(source, ident_to_find)`: a coarse
syntactic scan of the *unstripped* source for `@ident` decorator patterns followed
by a class member whose type annotation references the name (including method
parameter lists with parameter decorators, which still emit method metadata even
when the method itself is undecorated). If any decorated-member metadata position
mentions the name, the import is preserved. This is consulted by the CJS/ESM
import emitters in `module_emission/imports.rs` under `emit_decorator_metadata`.

### Cross-file guards under `isolatedModules` / `noLib`

When a referenced name may not be a runtime value (e.g. a qualified reference into
a namespace, or, under `noLib && isolatedModules`, a name with no value
declaration), the serializer emits a defensive `typeof`-guarded expression instead
of a bare reference, via `metadata_fallback_entity` /
`serialize_metadata_fallback_entity`, allocating a hoisted temp:

```js
typeof (_a = X && X) === "function" ? _a : Object
```

`metadata_qualified_type_reference_requires_guard` and
`metadata_type_reference_requires_guard` decide when the guard is needed; CommonJS
named-import substitution (`commonjs_named_import_substitutions`) is applied to the
root segment first so the guard references the correct local binding.

---

## Walk-through: a decorated class under `--emitDecoratorMetadata` (ES2015+)

Input (`legacy_decorators`, `emitDecoratorMetadata`, ESM, target ES2017):

```ts
import { Service } from "./service";
@injectable()
class Repo {
  @inject() svc!: Service;
  @log() async find(id: number): Service { /* … */ }
}
```

1. **Lowering** (`lowering/core.rs`): the class has a class-level decorator and a
   member is decorated, so the legacy decorator path is selected. `helpers.decorate
   = true` and `helpers.param`/`helpers.metadata` are set as members are scanned;
   the class-level constructor-paramtypes branch sets `metadata = true` if a
   constructor exists. `emitDecoratorMetadata` also marks the `Service` import as
   metadata-referenced so it is not elided.
2. **Type-param capture** (`emit_declaration.rs`): `Repo` has no type parameters,
   so `metadata_class_type_params` stays `None` (a generic class would populate it
   so `T` serializes to `Object`).
3. **Member `__decorate` + metadata** (`decorators.rs`, instance-first order):
   - For `svc`: `__decorate([ inject(), __metadata("design:type", Service) ],
     Repo.prototype, "svc", void 0);` — `serialize_type_reference_for_metadata`
     sees `Service`, not a type-param, not a numeric enum, so it emits the value
     reference `Service`.
   - For `find`: `__decorate([ log(), __metadata("design:type", Function),
     __metadata("design:paramtypes", [Number]), __metadata("design:returntype",
     Service) ], Repo.prototype, "find", null);` — `id: number` → `Number`;
     `design:type` is the literal `Function`; the explicit return annotation
     `Service` is serialized (the async rule's `Promise` fallback only applies when
     there is **no** return annotation).
4. **Class-level assignment** (`emit_legacy_class_decorator_assignment`): `Repo =
   __decorate([ injectable() ], Repo);` (plus `__metadata("design:paramtypes",
   [...])` if a constructor exists).
5. **Helper emission** (`emit.rs`, ESM): `needed_names()` returns `["__decorate",
   "__metadata", "__param"?]` in tier order; since this is ESM without
   `importHelpers`, the inline `emit_helpers` block writes `var __decorate = …;
   var __metadata = …;` at the top. At each call site `write_helper("__metadata")`
   resolves to the bare `__metadata` (no alias, no `tslib_1.` prefix).

If the same file were compiled with `--importHelpers`, step 5 instead emits
`import { __decorate, __metadata } from "tslib";` (sorted), and if a local
`__metadata` existed it would become `import { __metadata as __metadata_1 }` with
`write_helper` printing `__metadata_1`.

---

## Caches and invariants

- **Single request accumulator, populated once.** `HelpersNeeded` is built during
  the lowering pass and read in `emit.rs` via `helpers_populated()` /
  `helpers().clone()`. The emit phase does **not** re-scan the arena for helper
  needs (a fallback to `HelpersNeeded::default()` exists for non-transforming
  emits but is documented as rare). This is the O(N)-avoidance invariant the
  comment in `emit.rs` calls out.
- **Idempotent requests.** `mark_*` dedups via `unprioritized_order.contains`;
  inline emission dedups via the `emitted: Vec<HelperEmitOrder>` set and per-bool
  single-write tiers. No helper text or import name appears twice regardless of how
  many sites requested it.
- **Order is computed, never patched.** Both `emit_helpers` and `needed_names`
  produce final order in one pass over the tiers + `unprioritized_order`; there is
  no post-hoc reordering of already-written bytes. The full-set ordering test pins
  it.
- **Per-file binding reset.** `commonjs_tslib_import_binding` resets to `tslib_1`
  in `prepare_source_file_emit_state`, *except* inside a module-wrapper body
  (outFile multi-module), preserving `tslib_2`/etc.
- **`helper_import_aliases` and `file_identifiers` are per-file.** Collision
  aliasing is recomputed each file from that file's identifier set, so an alias is
  only synthesized when a real local name shadows the helper.
- **The metadata serializer holds no cache.** It is a pure recursive walk of the
  annotation node. The only state it touches is `metadata_class_type_params` (the
  current class's type-parameter names, cleared after the class) and the file's
  enum declarations (re-scanned per reference in `is_declared_numeric_enum`).

---

## Edge cases and tsc parity

- **Method `design:type` is a constant `Function`**, not a serialized signature;
  only `design:paramtypes` and `design:returntype` are serialized.
- **Async return → `Promise`** only when there is no explicit return annotation and
  the method is `async` and **not** an async generator. An explicit annotation
  always wins; an async generator yields `void 0`.
- **Generic type parameters erase to `Object`** (`metadata_class_type_params`), so
  `T`, `U` never leak as value references.
- **Numeric enums serialize to `Number`** (bare, member, and homogeneous-union
  forms), driven by a syntactic numeric-vs-string membership test, not by the
  solver. A mixed enum or an enum with any string member is **not** treated as
  numeric and falls through to a value reference.
- **`strictNullChecks` changes union stripping**: `null`/`undefined`/`void` are
  kept in a union under strict-null (so the union usually collapses to `Object`),
  stripped otherwise (so `string | undefined` → `String`). `never` is always
  stripped.
- **`this` parameters and rest parameters**: `this` is dropped from paramtypes;
  `...args: T[]` serializes `T`'s element type, non-array rest → `Object`.
- **Imports kept alive for metadata**: a type referenced only inside a decorated
  member's annotation is preserved against type-only elision via
  `name_appears_in_decorator_metadata_type`, matching `tsc`'s value-position
  treatment of design-time types.
- **`noLib`/`isolatedModules` defensiveness**: qualified or value-uncertain
  references emit a `typeof (… ) === "function" ? _temp : Object` guard rather than
  a bare reference, to avoid a `ReferenceError` at runtime.
- **`--noEmitHelpers`**: no inline block and no `tslib` import — call sites still
  print `__decorate`/`__metadata` and rely on a project-provided global (the
  self-installing `(this && this.__x) || …` form makes this safe).
- **TC39 vs legacy split**: this document's `__metadata` serialization is the
  **legacy** (`experimentalDecorators`) reflection model. Standard (TC39)
  decorators do **not** emit `design:*` metadata; they request `__esDecorate` /
  `__runInitializers` / `__setFunctionName` / `__propKey` instead (see
  [Async/Generator State Machines, Decorators, and Module-Format Wrapping in Emit](emitter-async-generator-decorators-modules.md)
  for the call shape). The `run_initializers_before_es_decorate` and
  `class_private_field_set_before_get` flags exist solely to reproduce `tsc`'s
  request-order-dependent ordering within a tier.

---

## Related reading

- [Async/Generator State Machines, Decorators, and Module-Format Wrapping in Emit](emitter-async-generator-decorators-modules.md)
  — the decorator *call* emission (`__decorate([...])`, `__esDecorate`), async/
  generator helper *consumers*, and the module wrappers that OR-merge helper
  requests.
- [The Emitter: JS and Declaration Output](emitter.md) — the two-phase
  `LoweringPass` → `Printer` pipeline this surface plugs into.
- [Source Maps in Emit](emitter-source-maps.md) — the position-mapping machinery
  that runs alongside helper/metadata emission.
- [Declarations and Modules in the Checker](checker-declarations-modules.md) and
  [module-resolution-engine](module-resolution-engine.md) — where import
  elision policy and `tslib` resolution are actually decided.
- [The CLI Surface and Diagnostic Reporting](cli-surface-and-diagnostic-reporting.md)
  — where `--importHelpers`, `--noEmitHelpers`, `--emitDecoratorMetadata`, and
  `--experimentalDecorators` enter as `PrinterOptions`.
