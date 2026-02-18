# Emitter Work: Architecture, Target Pipeline & Roadmap

## Current Architecture

### Two-Phase Pipeline

The emitter uses a two-phase approach:

1. **LoweringPass** — read-only AST walk that produces `TransformDirective`s stored in a `TransformContext`
2. **Printer** — walks the AST again, checks each node for directives, applies transforms or emits natively

Key files:
- `crates/tsz-emitter/src/lowering_pass.rs` — Phase 1 analysis
- `crates/tsz-emitter/src/emitter/mod.rs` — Phase 2 emission (Printer)
- `crates/tsz-emitter/src/transform_context.rs` — `TransformDirective` enum + `TransformContext` map
- `crates/tsz-emitter/src/emit_context.rs` — `EmitContext` with target flags

### IR-Based Transforms

Modern transforms produce `IRNode` intermediate representations, then `IRPrinter` converts IR → JS text. This cleanly decouples transform logic from string generation.

Migrated to IR:
- `class_es5` / `class_es5_ir.rs` — class → IIFE
- `enum_es5` / `enum_es5_ir.rs` — enum → IIFE + reverse mapping
- `async_es5` / `async_es5_ir.rs` — async/await → `__awaiter` + `__generator`
- `namespace_es5` / `namespace_es5_ir.rs` — namespace → IIFE
- `destructuring_es5.rs` — destructuring → assignment sequences
- `spread_es5.rs` — spread → `__spreadArray` helpers

Still embedded in Printer (no separate IR):
- Arrow function emission (captures `_this`, `_arguments`)
- Block scoping (`let`/`const` → `var`)

### ScriptTarget Enum

Defined in `crates/tsz-common/src/common.rs`:

```
ES3=0, ES5=1, ES2015=2, ES2016=3, ES2017=4, ES2018=5,
ES2019=6, ES2020=7, ES2021=8, ES2022=9, ES2023=10,
ES2024=11, ES2025=12, ESNext=99
```

Helper methods: `supports_es2015()`, `supports_es2017()`, `supports_es2018()`, `supports_es2020()`, `supports_es2022()`, `is_es5()`.

### Runtime Helpers

`HelpersNeeded` in `transforms/helpers.rs` tracks which `tslib` helpers are required:
`__extends`, `__assign`, `__rest`, `__awaiter`, `__generator`, `__values`, `__read`, `__spread`, `__spreadArray`, `__exportStar`, `__importDefault`, `__importStar`, `__classPrivateFieldGet/Set/In`, `__makeTemplateObject`, etc.

---

## The Gap: Binary Switch Instead of Pipeline

### Current Behavior

The emitter effectively uses a **binary switch**, not a per-level pipeline:

| Target | Behavior |
|--------|----------|
| ES5/ES3 | Full downleveling (classes, arrows, let/const, for-of, templates, spread, destructuring, async) |
| ES2015/ES2016 | Only async/await lowering |
| ES2017+ | No transforms (pass-through) |

The core gate is `ctx.target_es5: bool` with one additional check for `needs_async_lowering` (target < ES2017). Features from ES2018–ES2025 are emitted as-is regardless of target.

### TSC's Chain (What We Should Match)

TSC applies transforms in a sequential chain, each lowering one version level:

```
Source AST
  → transformTypeScript    (strip types, transform enums/namespaces)
  → transformClassFields   (if target < ES2022)
  → transformES2021        (if target < ES2021)
  → transformES2020        (if target < ES2020)
  → transformES2019        (if target < ES2019)
  → transformES2018        (if target < ES2018)
  → transformES2017        (if target < ES2017)
  → transformES2015        (if target < ES2015)
  → transformES5           (if target ≤ ES3)
  → Output JS
```

Each transform only lowers features from one ES version to the previous version.

---

## Missing Per-Level Transforms

### ES2016 → ES2015

| Feature | Transform | Complexity |
|---------|-----------|------------|
| Exponentiation `**` | → `Math.pow(base, exp)` | Trivial |

### ES2018 → ES2017

| Feature | Transform | Complexity |
|---------|-----------|------------|
| Async iterators (`for await...of`) | → `__asyncGenerator` / `__asyncValues` helpers | Moderate |
| Object rest properties (`const {a, ...rest} = obj`) | → `__rest(obj, ["a"])` | Moderate |
| Object spread properties (`{...obj, a: 1}`) | → `Object.assign({}, obj, {a: 1})` | Moderate |
| RegExp named capture groups | → runtime polyfill (or leave as-is) | Low priority |
| RegExp dotAll flag `/s` | → rewrite pattern (or leave as-is) | Low priority |

### ES2019 → ES2018

| Feature | Transform | Complexity |
|---------|-----------|------------|
| Optional catch binding (`catch {}`) | → `catch (_unused) {}` | Trivial |

### ES2020 → ES2019

| Feature | Transform | Complexity |
|---------|-----------|------------|
| Optional chaining `?.` | → ternary chains: `a?.b` → `a === null \|\| a === void 0 ? void 0 : a.b` | Moderate-High |
| Nullish coalescing `??` | → `a !== null && a !== void 0 ? a : b` | Moderate |
| `globalThis` | → platform-specific polyfill | Low priority |

### ES2021 → ES2020

| Feature | Transform | Complexity |
|---------|-----------|------------|
| Logical assignment `??=` | → `a ?? (a = b)` | Simple |
| Logical assignment `\|\|=` | → `a \|\| (a = b)` | Simple |
| Logical assignment `&&=` | → `a && (a = b)` | Simple |
| Numeric separators `1_000` | → `1000` (strip underscores) | Trivial |

### ES2022 → ES2021

| Feature | Transform | Complexity |
|---------|-----------|------------|
| Class public fields | → constructor assignment or `Object.defineProperty` | High |
| Class private fields `#field` | → `WeakMap` + `__classPrivateFieldGet/Set` | High |
| Class private methods `#method()` | → `WeakSet` + helper | High |
| Class static blocks `static {}` | → IIFE after class | Moderate |
| `#field in obj` | → `WeakMap.prototype.has.call(weakmap, obj)` | Moderate |
| Top-level await | Affects module format, not syntax lowering | Module-level |
| RegExp `/d` flag | → strip flag (lose indices info) | Low priority |

### ES2024 → ES2023

| Feature | Transform | Complexity |
|---------|-----------|------------|
| Decorators (stage 3) | → `__esDecorate` / `__runInitializers` helpers | Very High |
| `Array.groupBy` | Runtime, not syntax | N/A |

### ES2025 → ES2024

| Feature | Transform | Complexity |
|---------|-----------|------------|
| `using` / `await using` (explicit resource management) | → `try/finally` with `[Symbol.dispose]()` calls | High |
| RegExp duplicate named groups | Runtime support, not syntax | N/A |
| `Set` methods | Runtime, not syntax | N/A |

---

## Implementation Strategy

### Approach: Expand the Single-Pass Model

**Design constraint: single pass for speed.** TSC's multi-pass chain (one transform per ES level) is architecturally clean but slow — each pass re-walks the entire AST. Our single-pass LoweringPass + directive model exists because we want to be fast. We keep it.

The strategy is to expand the existing single LoweringPass walk with per-feature target checks, not add separate passes:

1. **Add per-feature target checks** in LoweringPass (not just `target_es5`)
2. **Add new `TransformDirective` variants** for each feature (e.g., `ES2020OptionalChain`, `ES2021LogicalAssignment`)
3. **Produce IR nodes** for each transform (follow the existing `*_ir.rs` pattern)
4. **Gate on target level** using existing `ScriptTarget` helpers, adding new ones as needed

One walk, many directives. The LoweringPass already handles ES5 classes, arrows, async, destructuring, spread, block scoping, and modules in a single traversal — adding more feature checks to the same walk is straightforward and maintains O(n) AST traversal.

### Concrete Steps

1. Add `supports_es2016()`, `supports_es2019()`, `supports_es2021()`, `supports_es2023()`, `supports_es2024()`, `supports_es2025()` helpers to `ScriptTarget`
2. Replace the boolean `target_es5` with a richer set of feature flags in `EmitContext`, e.g.:
   ```
   needs_es2020_lowering: bool,   // optional chaining, nullish coalescing
   needs_es2021_lowering: bool,   // logical assignment
   needs_es2022_lowering: bool,   // class fields, private names
   ```
3. For each feature, add a `TransformDirective` variant + IR transform + IR printer support
4. Wire through LoweringPass → Printer following the existing pattern

### Priority Order

Ranked by real-world usage frequency and user demand:

| Priority | Feature | Target | Impact |
|----------|---------|--------|--------|
| P0 | Optional chaining `?.` | ES2020 | Very high — most requested downlevel |
| P0 | Nullish coalescing `??` | ES2020 | Very high — paired with `?.` |
| P1 | Class fields (public + private) | ES2022 | High — ubiquitous in modern TS |
| P1 | Static blocks | ES2022 | High — paired with class fields |
| P2 | Logical assignment | ES2021 | Moderate — simple transform |
| P2 | Exponentiation `**` | ES2016 | Low effort — trivial transform |
| P3 | Object rest/spread | ES2018 | Moderate — already have spread infra |
| P3 | Async iterators | ES2018 | Moderate — already have async infra |
| P3 | Optional catch binding | ES2019 | Trivial |
| P4 | `using`/`await using` | ES2025 | Important for future — growing adoption |
| P4 | Decorators | ES2024 | Very complex — defer until stable |

---

## Abstraction Opportunities

### 1. Generic List Emitter (~50+ call sites)

Parenthesized/bracketed lists with separators are copy-pasted everywhere (parameters, arguments, type args, array elements, tuple members). A `emit_list(items, options)` helper with `ListFormat` flags (matching TSC's internal pattern) would consolidate these.

### 2. Keyword + Content Builder (~180+ sites)

Pattern: `write_keyword("export")` → `write_space()` → `write_keyword("default")` → `write_space()` → emit content. A chainable builder or macro would reduce boilerplate.

### 3. Unified Modifier Emitter (3 duplicate functions)

`emit_decorators_and_modifiers`, `emit_class_element_modifiers`, and similar functions share near-identical iteration logic. Unify into `emit_modifiers(node, filter)`.

### 4. Optional Element Helper (~80+ sites)

`if !idx.is_none() { self.emit_node(idx); }` repeated for optional AST children. An `emit_optional(idx)` or `emit_with_prefix(prefix, idx)` would clean this up.

### 5. Block Emission Standardizer

Open brace → indent → emit statements → dedent → close brace. Repeated for function bodies, class bodies, namespace bodies, module bodies, enum bodies. A single `emit_block(body, callback)` would unify these.

---

## File Inventory

### Transform Files (`crates/tsz-emitter/src/transforms/`)

| File | Purpose | Pattern |
|------|---------|---------|
| `mod.rs` | Re-exports, documentation | — |
| `ir.rs` | `IRNode` type definitions | Data |
| `ir_printer.rs` | IR → JS string emission | Printer |
| `helpers.rs` | `HelpersNeeded` + helper injection | Infra |
| `class_es5.rs` | Class transform entry | Wrapper |
| `class_es5_ir.rs` | Class → IIFE IR generation | IR |
| `enum_es5.rs` | Enum transform entry | Wrapper |
| `enum_es5_ir.rs` | Enum → IIFE IR generation | IR |
| `async_es5.rs` | Async transform entry | Wrapper |
| `async_es5_ir.rs` | Async → `__awaiter` IR generation | IR |
| `namespace_es5.rs` | Namespace transform entry | Wrapper |
| `namespace_es5_ir.rs` | Namespace → IIFE IR generation | IR |
| `destructuring_es5.rs` | Destructuring lowering | IR |
| `spread_es5.rs` | Spread lowering | IR |
| `arrow_es5.rs` | Arrow analysis (no emit) | Analysis |
| `block_scoping_es5.rs` | Block scope analysis (no emit) | Analysis |
| `es5.rs` | General ES5 IR helpers | IR |
| `emit_utils.rs` | Emission utilities | Utility |
| `emitter.rs` | Misc emitter utilities | Utility |
| `module_commonjs.rs` | CommonJS helpers | Module |
| `module_commonjs_ir.rs` | CommonJS IR generation | IR |
| `private_fields_es5.rs` | Private field transform | IR |

### Key Non-Transform Files

| File | LOC | Purpose |
|------|-----|---------|
| `emitter/mod.rs` | ~2,800 | Printer main dispatch |
| `emitter/expressions.rs` | ~1,800 | Expression emission |
| `emitter/declarations.rs` | ~2,050 | Declaration emission |
| `emitter/statements.rs` | ~1,200 | Statement emission |
| `emitter/types.rs` | ~1,500 | Type annotation emission |
| `lowering_pass.rs` | ~1,200 | LoweringPass AST walker |
| `emit_context.rs` | ~700 | EmitContext + target flags |
| `transform_context.rs` | ~320 | TransformDirective enum |
| `source_writer.rs` | ~400 | Low-level string/sourcemap output |
| `es5_bindings.rs` | ~3,900 | ES5 binding patterns (destructuring, parameters) |

Total emitter: ~47,000 LOC across 40+ files.
