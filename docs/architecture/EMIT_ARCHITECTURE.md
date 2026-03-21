# Emitter Architecture

**Version**: 1.0
**Status**: Binding Target Architecture
**Last Updated**: March 2026

---

## 1. Purpose

This document defines the architecture of the tsz emitter subsystem (`tsz-emitter`).

The emitter is the **OUTPUT** layer of the compiler pipeline. It consumes checked semantic products and produces JavaScript, declaration files (`.d.ts`), and source maps. It must not perform semantic validation, type reasoning, or late semantic discovery.

The emitter's architectural contract is simple:

> Transform checked AST into output text. All semantic questions were already answered upstream.

---

## 2. The Emitter in the Pipeline

```text
scanner -> parser -> binder -> checker -> solver -> emitter
                                                    ├── lowering (Phase 1)
                                                    │   └── produces TransformContext
                                                    ├── printer (Phase 2)
                                                    │   └── walks AST + directives -> JS
                                                    └── declaration emitter
                                                        └── produces .d.ts from AST + type cache
```

The emitter receives:

- A read-only `NodeArena` (AST) from the parser.
- `PrinterOptions` derived from compiler configuration.
- Optional `TypeCache` from the checker (for type-only import elision).
- Optional `TypeCacheView` / `TypeInterner` / `BinderState` (for declaration emit).

The emitter produces:

- JavaScript source text.
- Declaration file (`.d.ts`) text.
- Source map JSON.
- Emit diagnostics (declaration emit only).

---

## 3. Two-Phase Emission Model

The emitter uses a two-phase architecture that separates analysis from output generation. This is the central design decision of the emitter.

### 3.1 Phase 1: Lowering Pass

`LoweringPass` performs a read-only walk of the AST and produces a `TransformContext` — a map from `NodeIndex` to `TransformDirective`.

```text
LoweringPass::new(arena, ctx).run(source_file) -> TransformContext
```

The lowering pass decides **what** needs to be transformed (ES5 classes, enums, namespaces, CommonJS exports, arrow functions, async/await, destructuring, etc.) without producing any output text. It records its decisions as lightweight directive values.

**Key properties:**

- Read-only AST access. No mutation.
- O(n) single pass with recursion depth limits (`MAX_AST_DEPTH = 500`).
- Stateful tracking for scoping (namespace depth, this-capture, class context).
- Declaration merging detection via `declared_names`.
- Output is a `HashMap<NodeIndex, TransformDirective>` — no intermediate AST copies.

### 3.2 Phase 2: Print Pass

`Printer` walks the AST and checks the `TransformContext` before emitting each node. If a directive exists, it dispatches to the appropriate transform emitter; otherwise it emits the node's literal representation.

```text
Printer::with_transforms_and_options(arena, transforms, options).emit(root)
```

**Key properties:**

- Single-pass AST traversal guided by directives.
- Delegates to specialized sub-emitters by concern (expressions, statements, declarations, types, JSX, modules, comments).
- For IR-migrated transforms, delegates to `IRPrinter` for the transformed subtree.
- Writes through `SourceWriter` for source map tracking.

### 3.3 Why Two Phases

Separating analysis from emission provides:

1. **Testability**: Lowering decisions are testable without parsing output strings.
2. **DOD compliance**: The read-only AST is never mutated; transforms are a side-channel.
3. **Composability**: Directives can chain (`CommonJSExport` wrapping `ES5Class`).
4. **Clarity**: The printer's job is text generation, not semantic analysis.

---

## 4. Transform Directive Model

Since the AST is read-only (Data-Oriented Design), the emitter cannot annotate or mutate AST nodes. Instead, it uses a **projection layer**: the `TransformContext` maps node indices to `TransformDirective` variants that override default emission.

### 4.1 Directive Variants

| Directive | Purpose |
|---|---|
| `Identity` | Emit node as-is (explicit no-op) |
| `ES5Class` | Class declaration -> IIFE pattern |
| `ES5ClassExpression` | Class expression -> IIFE expression |
| `ES5Namespace` | Namespace -> IIFE with merge detection |
| `ES5Enum` | Enum -> IIFE with value computation |
| `CommonJSExport` | Wrap with `exports.X = X` assignment |
| `ES5ArrowFunction` | Arrow -> function expression with this/arguments capture |
| `SubstituteThis` | Replace `this` with captured variable |
| `SubstituteArguments` | Replace `arguments` with captured variable |
| `BlockScopeVariable` | `let`/`const` -> `var` with rename |
| `ES5AsyncFunction` | Async -> `__awaiter`/`__generator` pattern |
| `ES5DestructuringAssignment` | Destructuring -> temp variable assignments |
| `ES5SpreadArgument` | Spread -> `__spreadArray`/`.concat()` |
| `ES5PrivateFieldAccess` | Private field -> `__classPrivateFieldGet/Set` |
| `InsertThisCapture` | Insert `var _this = this` at function body start |
| `Chain` | Compose multiple directives sequentially |

### 4.2 Directive Lookup

During emission, the printer checks for a directive via `TransformContext::get(node_index)`. This is an O(1) hash map lookup. The vast majority of nodes have no directive and emit literally.

---

## 5. IR (Intermediate Representation)

Complex transforms (classes, enums, namespaces, async/await, CommonJS modules) produce an `IRNode` tree instead of emitting strings directly. The IR is a tree-structured JavaScript AST that the `IRPrinter` converts to text.

### 5.1 Why IR

String concatenation in transforms is fragile, hard to test, and prevents formatting consistency. The IR layer provides:

- **Structural correctness**: IR nodes enforce valid JavaScript syntax.
- **Testability**: IR trees can be inspected without string matching.
- **Formatting consistency**: `IRPrinter` applies indentation and whitespace uniformly.
- **Future extensibility**: Minification or pretty-print modes only need to change `IRPrinter`.

### 5.2 IR Node Categories

The `IRNode` enum covers:

- **Literals**: numeric, string, boolean, null, undefined.
- **Identifiers**: names, `this` (with capture flag), `super`.
- **Expressions**: binary, unary, call, new, property access, element access, conditional, comma, array literal, spread, assignment, typeof, void, delete, in, instanceof, yield, await, tagged template.
- **Statements**: expression statement, variable declaration, return, if/else, for, for-in, for-of, while, do-while, switch/case, break, continue, throw, try/catch/finally, with, labeled, debugger, empty.
- **Declarations**: function, class (with full member support), arrow function.
- **Patterns**: object literal, computed property, shorthand property, getter/setter.
- **Structural**: block, sequence, IIFE, `ASTRef` (delegate back to AST printer for subtrees).

### 5.3 ASTRef: Bridging IR and AST

The `ASTRef(NodeIndex)` variant allows IR trees to reference original AST subtrees that don't need transformation. This avoids duplicating the entire AST into IR — only transformed portions use IR nodes, while unchanged subtrees delegate back to the AST printer.

---

## 6. Declaration Emit (`.d.ts`)

`DeclarationEmitter` produces TypeScript declaration files from checked AST and type information.

### 6.1 Inputs

- `NodeArena` (AST).
- `TypeCacheView` (node types, symbol types, DefId-to-symbol mapping).
- `TypeInterner` (for printing resolved types).
- `BinderState` (for symbol resolution and scope analysis).
- `UsageAnalyzer` output (which symbols are exported, type-vs-value usage).
- Import plan (precomputed module specifiers and aliases).

### 6.2 Architecture

Declaration emit is architecturally separate from JS emit. It has its own:

- Writer and indentation state.
- Source map tracking.
- Import elision and generation logic.
- Namespace and scope handling.
- Overload signature deduplication.

### 6.3 Boundary Rule

Declaration emit should prefer **precomputed semantic summaries** over late semantic rediscovery. When declaration emit requires large amounts of fresh semantic work, the semantic boundary is too weak upstream (see `BOUNDARIES.md` Section 13).

The `TypeCacheView` abstraction decouples the emitter from checker internals while accepting the minimal cache data needed:

```rust
pub struct TypeCacheView {
    pub node_types: FxHashMap<u32, TypeId>,
    pub symbol_types: FxHashMap<SymbolId, TypeId>,
    pub def_to_symbol: FxHashMap<DefId, SymbolId>,
}
```

---

## 7. Output Layer

### 7.1 SourceWriter

All text output flows through `SourceWriter`, which manages:

- Output buffer accumulation.
- Line/column tracking for source maps.
- Lazy indentation (indent applied on first write after newline).
- Source map generation via `SourceMapGenerator`.

The separation of text writing from AST traversal ensures that source maps are accurate for both direct AST emission and IR-based transform emission.

### 7.2 Print API

The high-level output API provides:

- `print_to_string()`: Simple AST-to-string for testing.
- `print_with_options()`: Configurable emission.
- `emit_to_writer()`: Write to external buffer.

### 7.3 Source Maps

Source maps track the mapping from output positions to original source positions. Both the AST printer and IR printer emit source map entries through `SourceWriter`, ensuring transformed code maps back to its original TypeScript source.

---

## 8. Module System

### 8.1 Module Transforms

The emitter handles module system transformations:

- **CommonJS**: `import/export` -> `require()`/`exports.X` assignments.
- **ESModule**: Preserve or minimal transform.
- **Bundling**: `--outFile` concatenation with module wrapping.

CommonJS transformation uses the directive model: `CommonJSExport` directives wrap declarations with export assignments. Module-level concerns (require generation, exports object) are handled in `module_commonjs_ir.rs`.

### 8.2 Import Elision

Type-only imports are elided based on `TypeCache.type_only_nodes` from the checker. The `import_usage.rs` module provides text-based heuristics for detecting value vs. type-only usage when full checker information is unavailable.

---

## 9. Emitter Context

### 9.1 EmitContext

`EmitContext` centralizes transform-specific state that the printer and lowering pass share:

- `PrinterOptions`: Full compiler configuration.
- `EmitFlags`: Per-scope emission flags (async, generator, this-capture, etc.).
- `ArrowTransformState`: Arrow function this/arguments capture tracking.
- `DestructuringState`: Destructuring transform scratch state.
- `CommonJSState`: Module-level CommonJS tracking.
- `BlockScopeState`: Block scoping let/const-to-var tracking.
- `PrivateFieldState`: Private class field WeakMap tracking.

### 9.2 PrinterOptions

`PrinterOptions` carries the full emission configuration (~160 fields), including:

- Target ECMAScript version.
- Module system.
- JSX mode and factory configuration.
- Quote style, semicolons, newline style.
- Decorator and enum preservation settings.
- Helper import configuration.
- Type-only node elision sets.

---

## 10. Boundary Rules

### 10.1 What the Emitter Owns

- JavaScript text generation from AST and directives.
- Declaration file text generation from AST and type summaries.
- Source map generation.
- Transform analysis (lowering) and IR production.
- Emit diagnostics (declaration emit accessibility and visibility errors).

### 10.2 What the Emitter Must Not Own

- **Semantic validation**: No type checking, no assignability, no compatibility.
- **Type reasoning**: No inference, no evaluation, no relation computation.
- **Late semantic discovery**: No semantic questions that should have been answered by checker/solver.
- **Checker internals**: No direct access to checker state beyond `TypeCache`/`TypeCacheView`.

### 10.3 Dependency Direction

```text
tsz-emitter depends on:
  tsz-parser   (NodeArena, NodeIndex, SyntaxKind)
  tsz-scanner  (SyntaxKind, tokens)
  tsz-common   (diagnostics, source maps, shared types)
  tsz-binder   (BinderState, SymbolId — declaration emit only)
  tsz-solver   (TypeId, TypeInterner, DefId — declaration emit only)

tsz-emitter does NOT depend on:
  tsz-checker  (no checker internals; TypeCache passed as data)
```

### 10.4 Consumer Direction

```text
tsz-cli       -> tsz-emitter (driver/emit.rs orchestrates emission)
tsz-core      -> tsz-emitter (re-exports public API)
tsz-checker   does NOT import tsz-emitter
```

---

## 11. Transform Migration Status

Transforms are migrating from direct string emission to the IR model.

### 11.1 IR-Migrated (Target Architecture)

| Transform | Files |
|---|---|
| ES5 Classes | `class_es5.rs`, `class_es5_ir.rs`, `class_es5_ir_members.rs` |
| ES5 Enums | `enum_es5.rs`, `enum_es5_ir.rs` |
| ES5 Namespaces | `namespace_es5.rs`, `namespace_es5_ir.rs` |
| Async/Await | `async_es5.rs`, `async_es5_ir.rs`, `async_es5_ir_convert.rs` |
| CommonJS Modules | `module_commonjs.rs`, `module_commonjs_ir.rs` |

### 11.2 Analysis-Only (No IR Needed)

| Transform | File |
|---|---|
| Arrow Functions | `arrow_es5.rs` |
| Block Scoping | `block_scoping_es5.rs` |
| Private Fields | `private_fields_es5.rs` |
| Destructuring | `destructuring_es5.rs` |
| Spread | `spread_es5.rs` |

### 11.3 Migration Direction

New complex transforms should produce IR trees, not concatenate strings. Simple transforms that only rearrange or rename existing AST text (arrows, block scoping) can use directives without IR.

---

## 12. Enum Subsystem

The emitter contains a self-contained enum subsystem:

- `EnumChecker`: Validates enum declarations (duplicate members, computed initializers).
- `EnumEvaluator`: Computes enum member values (constant folding, auto-increment).
- `EnumTransformer`: Produces IR for enum IIFE patterns.

This subsystem is emitter-owned because enum value computation is an output concern (the emitted IIFE needs concrete values), not a type-checking concern.

---

## 13. Performance Considerations

### 13.1 Allocation Strategy

- AST is shared via `&NodeArena` reference — no cloning.
- Directives stored in a flat `FxHashMap<NodeIndex, TransformDirective>`.
- IR nodes use `Box<IRNode>` and `Vec<IRNode>` — heap-allocated but short-lived per-transform.
- `SourceWriter` uses a single growing `String` buffer.
- `Cow<'static, str>` used in IR for zero-cost static strings.

### 13.2 Parallelism

File-level parallelism is supported. Each file's emission is independent:

- Separate `Printer` per file.
- Separate `LoweringPass` per file.
- Separate `DeclarationEmitter` per file.
- No shared mutable state between files.

The CLI driver uses `rayon` for parallel file emission.

### 13.3 Hot Path Optimization

- Directive lookup is O(1) hash map access.
- Most nodes have no directive — fast path is "no lookup hit, emit literally."
- String interning (via `Atom`) reduces allocation in identifier emission.
- Source map entries use pre-computed byte-offset-to-line/column tables.

---

## 14. Testing Strategy

### 14.1 Test Levels

- **Unit tests**: Individual transform IR output, enum evaluation, import elision.
- **E2E tests**: Full TypeScript -> JavaScript round-trip (`es5_transforms_e2e.rs`).
- **Conformance**: Emitter correctness validated against TypeScript compiler output.
- **Snapshot tests**: Declaration emit output compared against expected `.d.ts` files.

### 14.2 Test Organization

Tests live in `crates/tsz-emitter/tests/` (~40 test files), organized by transform concern. The two-phase model makes unit testing transforms straightforward: test lowering decisions independently, then test IR output independently, then test full emission end-to-end.

---

## 15. Review Checklist

For any non-trivial emitter PR, ask:

1. Does this emit output based on checked facts, or does it discover semantics?
2. Does this use the directive/IR model, or does it add ad-hoc string concatenation?
3. Does declaration emit use precomputed summaries, or does it re-derive type information?
4. Does this maintain the Phase 1 / Phase 2 separation?
5. Does this keep the emitter free of checker/solver internal dependencies?
6. Does this preserve file-level parallelism (no shared mutable state)?
7. Does this maintain source map accuracy through `SourceWriter`?

---

## 16. Anti-Goals

The following are explicitly not the target architecture:

1. Performing type checking or semantic validation in the emitter.
2. Importing checker internals for semantic decisions during emit.
3. Building new transforms with direct string concatenation instead of IR.
4. Re-deriving semantic facts that should have been computed upstream.
5. Sharing mutable state across file emission boundaries.
6. Making the emitter responsible for diagnostic messages about type errors.
7. Letting declaration emit grow into a shadow type checker.

---

## 17. Final Rule

The emitter's job is mechanical transformation of checked input to textual output. When the emitter needs to make semantic decisions, the fix is upstream — in the checker, solver, or binder — not in the emitter itself.
