# Editor and Browser Surfaces: the LSP Language Service and the WASM API

## Orientation

The pipeline doc [`end-to-end-timeline.md`](end-to-end-timeline.md) ends its
cross-file walk-through by noting that the CLI, `--build`, the LSP language
service, and the WASM bindings all reuse the same kernel. The driver docs
[`driver-incremental-and-watch.md`](driver-incremental-and-watch.md) and
[`driver-project-references-and-build-mode.md`](driver-project-references-and-build-mode.md)
cover the batch and watch drivers. This page fills the remaining boundary gap:
the two *read-mostly editor surfaces* that sit on top of the kernel without
owning any type algorithm of their own — the `tsz-lsp` crate (the language
service: hover, completions, navigation, rename, diagnostics, code actions,
highlighting, signature help, the fourslash harness, and the multi-file
`Project` container) and the `tsz-wasm` crate's `wasm_api` module (the
TypeScript-compatible `TsProgram` / `TsTypeChecker` / `TsSourceFile` /
`TsLanguageService` shims plus single-file `transpileModule`).

The defining property of both surfaces is that they are *projections* of the
existing scanner → parser → binder → checker → solver pipeline at a *cursor
position*. They convert an editor position (line/character or byte offset) into
an AST `NodeIndex`, resolve that node to a `SymbolId` or a `TypeId`, ask the
checker/solver for the semantic answer, and serialize the answer to the
editor's wire format (LSP JSON, tsserver `CompletionEntry`, Monaco markers).
They never run a relation, an inference round, an instantiation, or an
evaluation kernel themselves — every such call lands in `CheckerState` (which
delegates to the solver). The architectural rule is the same one the checker
obeys, applied one level further out: **the editor surfaces ask, they do not
compute.**

## Owns / Must not own

| Owns | Must not own |
| --- | --- |
| Position ↔ offset translation (`LineMap`), node-at-offset lookup, symbol-query-node backtracking | The type universe — they hold a borrowed `&TypeInterner`, never mutate type identity directly |
| Constructing a per-request `CheckerState` (with a reusable `TypeCache`) and reading its answers | Relation/inference/instantiation/evaluation/narrowing kernels — those stay in solver behind `CheckerState` |
| Symbol-usage resolution via the lightweight `ScopeWalker` (binder maps *declarations*, LSP needs *usages*) | Re-binding semantics — the walker reconstructs scope chains but never creates symbols |
| Multi-file orchestration: the `Project` container, shared `Arc<TypeInterner>` / `Arc<DefinitionStore>`, file indices, eviction | Module-resolution algorithm (see [`module-resolution-engine.md`](module-resolution-engine.md)) — `Project` consumes resolution results |
| Caches that make repeat cursor queries cheap: `ScopeCache`, per-file `TypeCache`, `cached_diagnostics` with result-id pull model | Diagnostic *production* — they convert `CheckerContext::diagnostics`, they do not invent codes or suppress them |
| Wire-format serialization (LSP `TextEdit`/`Location`/`CompletionItem`, tsserver `kindModifiers`, UTF-16 offsets, source maps) | Emit policy — `transpileModule` drives `LoweringPass` + `Printer` but adds no semantic validation |
| The fourslash test harness's marker DSL | Marker awareness inside any production provider — providers must behave identically for user-typed and marker-annotated text |

## Crate and module map

### `tsz-lsp` (the language service)

The crate root [`crates/tsz-lsp/src/lib.rs`](../../../crates/tsz-lsp/src/lib.rs)
declares one module per feature family and re-exports the public provider
structs. `tsz-core` re-exports the whole crate as `tsz::lsp` (see
`crates/tsz-core/src/lib.rs`, `pub use tsz_lsp as lsp;`), which is how
`tsz-wasm` reaches it.

| Module path | Role | Provider tier |
| --- | --- | --- |
| `provider_macro/mod.rs` | `define_lsp_provider!` macro: the `minimal` / `binder` / `full` field-and-constructor template every provider expands | — |
| `resolver/core.rs` | `ScopeWalker` + `ScopeCache`: resolve identifier *usages* to `SymbolId` on demand | — |
| `hover/` (`core.rs`, `contextual.rs`, `format.rs`, `jsdoc_format.rs`) | `HoverProvider`: quickinfo display string, kind, `kindModifiers`, JSDoc | `full` |
| `completions/` (`core.rs`, `member.rs`, `filters.rs`, `string_literals.rs`, `import_paths.rs`, `context.rs`, `postfix.rs`) | `Completions`: scope, member, string-literal, auto-import, postfix completions | `full` |
| `signature_help/` (`trigger.rs`, `phases.rs`, `shapes.rs`, `selection.rs`, `display.rs`, `docs.rs`, `contextual.rs`) | `SignatureHelpProvider`: active call site, overload candidates, active parameter | `full` |
| `navigation/` (`definition.rs`, `declaration.rs`, `type_definition.rs`, `implementation.rs`, `references.rs`, `source_definition.rs`) | go-to providers + `FindReferences` | mostly `binder` |
| `rename/` (`core.rs`, `linked_editing.rs`, `file_rename.rs`) | `RenameProvider`, `LinkedEditingProvider`, `FileRenameProvider`, `WorkspaceEdit` | `binder` / `minimal` |
| `highlighting/` (`document.rs`, `semantic_tokens.rs`) | `DocumentHighlightProvider`, `SemanticTokensProvider` | `binder` |
| `hierarchy/` (`call_hierarchy.rs`, `type_hierarchy.rs`) | `CallHierarchyProvider`, `TypeHierarchyProvider` | `binder` |
| `classify/mod.rs` | shared `classify_symbol_flags` / `kind_modifiers` / `variable_decl_kind` — one owner for symbol→presentation mapping | — |
| `diagnostics/` | `LspDiagnostic`, severities, the pull-model report DTOs | — |
| `code_actions/` (≈27 `code_action_*.rs` modules) | `CodeActionProvider`, `CodeFixRegistry`, quick fixes, refactors, import management | mixed |
| `symbols/`, `editor_ranges/`, `editor_decorations/`, `document_links/`, `formatting.rs` | document symbols, folding/selection ranges, code lens/inlay hints/document colors, links, formatting | mostly `minimal` |
| `project/` (`core.rs`, `core/project_file.rs`, `features.rs`, `operations.rs`, `diagnostic_pull.rs`, `eviction.rs`, `file_context.rs`) | the multi-file container: `Project`, `ProjectFile`, feature dispatch, pull diagnostics, eviction | — |
| `fourslash.rs`, `fourslash_variants.rs`, `fourslash/parsing.rs` | the marker-DSL test harness (test-only; production providers may not depend on it) | — |

### `tsz-wasm` (`wasm_api`)

The crate root [`crates/tsz-wasm/src/lib.rs`](../../../crates/tsz-wasm/src/lib.rs)
is a `cdylib` that `pub use tsz::*` (so wasm-bindgen picks up every
`#[wasm_bindgen]` export in the core crate) and adds the `wasm_api`
compatibility layer.
[`crates/tsz-wasm/src/wasm_api/mod.rs`](../../../crates/tsz-wasm/src/wasm_api/mod.rs)
documents the handle-based design: objects live in Rust, JS holds `u32`
handles.

| Module path | Role |
| --- | --- |
| `wasm_api/program.rs` | `TsProgram` (`createProgram` equivalent), `TsCompilerOptions` DTO, `ensure_compiled`, syntactic + semantic diagnostic collection |
| `wasm_api/type_checker.rs` | `TsTypeChecker`: `typeToString`, type predicates (`isUnionType`, `isArrayType`, …), intrinsic-type getters, type flags |
| `wasm_api/language_service.rs` | `TsLanguageService`: the single-file IDE shim that drives the `tsz-lsp` providers directly |
| `wasm_api/source_file.rs` | `TsSourceFile`: lazy-parsed AST access, file metadata |
| `wasm_api/emit.rs` | `transpileModule` / `transpile`: single-file JS (and optional `.d.ts`) emit, source maps |
| `wasm_api/diagnostics.rs` | `TsDiagnostic` DTO, `formatTsDiagnostic`, colored-context formatting |
| `wasm_api/ast.rs`, `types.rs`, `enums.rs`, `options.rs`, `utilities.rs` | `TsNode`/`TsType`/`TsSymbol` shims, enum DTOs, option converters |

## The provider tier macro

Almost every LSP feature is a struct generated by `define_lsp_provider!` in
[`crates/tsz-lsp/src/provider_macro/mod.rs`](../../../crates/tsz-lsp/src/provider_macro/mod.rs).
The macro encodes the *dependency budget* of a feature as one of three tiers,
which is the single clearest statement of the architectural boundary in the
crate: the more semantic work a feature needs, the more of the pipeline it has
to borrow.

```
minimal  ── arena, line_map, source_text
             (AST-only: folding, selection ranges, document symbols,
              document links, linked editing, file rename)

binder   ── arena, binder, line_map, file_name, source_text
             (symbol identity, no types: go-to-definition, references,
              rename, highlighting, call/type hierarchy, code lens)

full     ── arena, binder, line_map, interner, source_text, file_name,
             strict, sound_mode, checker_options, lib_contexts
             (type-aware: hover, completions, signature help)
```

The `full` arm is the boundary-critical one. It generates the only path by
which a provider may reach the type system, and it funnels every such path
through `CheckerState`:

- `checker_options()` derives a `tsz_checker::context::CheckerOptions` from the
  provider's `strict` / `sound_mode` flags (or an explicit override).
- `apply_lib_contexts()` installs `LibContext`s into the freshly built checker.
- `checker_with_cache(type_cache)` builds a `tsz_checker::CheckerState` via
  `CheckerState::with_cache(...)` when a `TypeCache` exists, or
  `CheckerState::new(...)` otherwise, then applies lib contexts.

A `full`-tier provider therefore never touches a `TypeInterner` method that
constructs or relates types; it holds `interner: &'a TypeInterner` only so it
can hand the borrow to `CheckerState`, and it asks the checker high-level
questions like `get_type_of_node`, `get_type_of_symbol`, and `format_type`.
This is the same discipline the checker itself follows toward the solver, just
moved one ring outward. The `minimal` and `binder` tiers structurally *cannot*
do type work — they do not even hold the interner — which is why navigation,
rename, and highlighting are answered purely from binder symbols.

## Resolving a cursor to a symbol: the `ScopeWalker`

The binder records a `node_symbols` map from *declaration* nodes to `SymbolId`,
but an editor cursor usually sits on a *usage* — an identifier reference, a
property name, a specifier. The binder doc
[`binder.md`](binder.md) explains why the binder deliberately does not resolve
references (that is name resolution, which interleaves with type semantics).
The LSP layer therefore reconstructs scope chains on demand with a lightweight
walker in
[`crates/tsz-lsp/src/resolver/core.rs`](../../../crates/tsz-lsp/src/resolver/core.rs).

`ScopeWalker` (`fn new`, `fn resolve_node`, `fn resolve_node_cached`) mimics the
binder's scope logic but only to *look up* a name, never to declare one. It
keeps a `scope_stack: Vec<SymbolTable>`, a parallel
`function_scope_indices: Vec<usize>` (so `var`-hoisting resolution can find the
nearest function scope), and three recursive tree walks (`walk_to_node`,
`walk_for_scope`, `collect_references`) that share a `tree_walk_depth: u32`
counter.

That counter is the crate's recursion guard. The comment explains the subtlety:
`stacker::remaining_stack()` reports the *current segment's* ~2 MB headroom
inside a `maybe_grow` closure and so never detects runaway chaining (e.g.
`a.b.c.d…` thousands deep, or a pathological alias chain). Instead the walker
increments `tree_walk_depth` on entry and decrements on exit, and once it
exceeds `const TREE_WALK_MAX_DEPTH: u32 = 4096` it sets
`ref_walk_stack_tripped = true`; all subsequent recursive calls return
immediately. The walker is ephemeral — one per operation call — so no reset is
needed. Stack growth itself uses `stacker::maybe_grow(256 * 1024, 2 * 1024 * 1024, …)`.

### The `ScopeCache`

`ScopeCache` is `FxHashMap<u32, Vec<SymbolTable>>` — keyed by node id, valued by
the reconstructed scope chain. Across a burst of cursor requests in the same
file (hover, then signature help, then completions, all near the same point),
the walker can serve the chain from the cache instead of re-walking from the
root. `ScopeCacheStats { hits, misses }` is threaded through the cached
resolution path and recorded into `ProjectPerformance` so the residency and
benchmark work tracked in the project memory has a real hit-rate signal. The
cache lives on the owning `ProjectFile` (`scope_cache: ScopeCache`) and is
cleared whenever the file is re-parsed.

## Walk-through 1: hover over `const x = 42;`

Trace `getQuickInfoAtPosition` (or `Project::get_hover`) for the cursor on `x`
in `const x = 42;`. The real call chain is in
[`crates/tsz-lsp/src/hover/core.rs`](../../../crates/tsz-lsp/src/hover/core.rs),
`HoverProvider::get_hover_internal`:

```
position (line, character)
  │  HoverProvider::get_hover
  ▼
LineMap::position_to_offset(position, source_text)            ── byte offset
  │
  ▼
utils::find_node_at_or_before_offset(arena, offset, src)      ── NodeIndex on `x`
  │   (keyword fast-paths: ThisKeyword → hover_for_this_keyword,
  │    SuperKeyword → hover_for_super_keyword)
  │   utils::is_symbol_query_node gate; backtrack via
  │   find_symbol_query_node_at_or_before for comment/edge offsets
  ▼
ScopeWalker::resolve_node(root, node_idx)  (or _cached)       ── SymbolId for x
  │   binder.symbols.get(symbol_id)                            ── Symbol
  ▼
self.checker_with_cache(type_cache)                           ── CheckerState
  │   find_best_declaration(symbol, node_idx)
  │   checker.get_type_of_node(decl)  /  get_type_of_symbol    ── TypeId  (← solver)
  │   checker.format_type(type_id)                             ── "42" or "number"
  ▼
*type_cache = Some(checker.extract_cache())                   ── persist TypeCache
  │   get_tsserver_kind / get_kind_modifiers / build_display_string
  ▼
HoverInfo { contents, range, display_string: "const x: number",
            kind: "const", kind_modifiers, documentation, tags }
```

Several parity-driven details live in this method. For variables
(`FUNCTION_SCOPED_VARIABLE` / `BLOCK_SCOPED_VARIABLE`) the provider asks
`get_type_of_node(decl_node_idx)` rather than `get_type_of_symbol`, because the
declaration node carries the flow-narrowed/initializer view tsserver shows. If
the variable has an explicit annotation, `variable_declaration_annotation_text`
substitutes the *written* text. For a `const` it deliberately keeps the checker
result (`const c = 0` displays as `0`, not `number`) and only falls back to the
initializer type for `let`/`var` or when the checker returned `"error"`/empty.
The kind string and `kindModifiers` come from the shared `classify` module so
hover, completions, document symbols, and rename never drift on how a symbol is
labeled.

Crucially, the type itself is produced entirely by `CheckerState` →
[`solver-types-intern-def.md`](solver-types-intern-def.md) and rendered by the
solver's `TypeFormatter` (`checker.format_type`). The hover provider chooses
*which* type to ask for and *how to label* the symbol; it never widens, narrows,
or relates a type to make the string come out right. See
[`checker-type-of-symbol-and-symbol-types.md`](checker-type-of-symbol-and-symbol-types.md)
for what `get_type_of_symbol` actually does.

## Walk-through 2: member completions after a dot

`obj.` triggers member completions. The dispatch path
(`Completions::get_completions` →
[`crates/tsz-lsp/src/completions/member.rs`](../../../crates/tsz-lsp/src/completions/member.rs))
is the clearest example of a provider asking the checker for a *type* and then
reading its *shape* through solver query helpers rather than open-coding type
logic:

1. `make_checker(cache_ref)` builds a `CheckerState` (mirroring the macro's
   `checker_with_cache`, reusing the file's `TypeCache`).
2. `checker.get_type_of_node(expr_idx)` returns the `TypeId` of the
   left-hand-side expression — the solver computes it.
3. The provider enumerates the members of that `TypeId` using solver-owned
   structural helpers (`tsz_solver::objects::apparent_primitive_members`,
   `ApparentMemberKind`, the `visitor` traversal, `Visibility`) — not by
   pattern-matching raw `TypeData`. For primitive receivers it falls back to
   `apparent_*_members` so `"".|` offers `String.prototype` members exactly as
   tsc does.
4. Each member is rendered into a `CompletionItem` with `kind`, `kindModifiers`,
   `sortText`, and a `detail` produced by `checker.format_type(member_type)`.
5. The updated `TypeCache` is written back so the next request reuses it.

The completion `sortText` constants in
[`crates/tsz-lsp/src/completions/mod.rs`](../../../crates/tsz-lsp/src/completions/mod.rs)
(`sort_priority::LOCATION_PRIORITY = "11"`, `LOCAL_DECLARATION = "10"`,
`GLOBALS_OR_KEYWORDS = "15"`, `AUTO_IMPORT = "16"`, and the
`deprecated`/`object_literal_property`/`sort_below` transforms) are copied
verbatim from TypeScript's `ts.Completions.SortText` enum so the editor orders
the list identically to tsserver. The `CompletionItem` field set
(`hasAction`, `source`, `sourceDisplay`, `replacementSpan`,
`additionalTextEdits`, `isPackageJsonImport`, opaque `data` for lazy resolve)
mirrors the tsserver `CompletionEntry` protocol field-for-field.

## Walk-through 3: signature help inside a call

For `foo(|)` the trigger logic in `signature_help/trigger.rs` locates the
containing call site, the callee, and the active parameter index from token
structure (using parser helpers like `count_top_level_commas`,
`find_incomplete_paren_call`, `find_incomplete_angle_call`,
`has_comma_between_offsets`). The candidate phase
(`signature_help/shapes.rs`, `phases.rs`) then asks the checker for the callee's
`TypeId` and reads its call signatures through solver shapes
(`tsz_solver::FunctionShape`, `ParamInfo`, `apparent_intrinsic_kind`,
`TypePredicateTarget`). Overload selection (`selection.rs`) scores argument
types against each candidate. The display phase (`display.rs`,
`apply_type_param_substitution`) substitutes inferred type arguments into the
rendered label. As with hover and completions, every type comes from the
checker/solver; the provider owns only the *call-site geometry* (which call,
which argument slot) and the *presentation*. For receivers like
`"".charAt(|)`, `crate::intrinsic_params` supplies parameter specs that match
the lib `.d.ts` signatures without re-deriving them.

## Multi-file orchestration: the `Project` container

Single-file providers borrow an arena, a binder, and a line map. Real editors
work across files: go-to-definition jumps into another module, auto-import adds
an import statement, rename edits every referencing file. The `Project`
container in
[`crates/tsz-lsp/src/project/core.rs`](../../../crates/tsz-lsp/src/project/core.rs)
owns the cross-file state that the kernel needs to behave like one program.

`Project` holds `files: FxHashMap<String, ProjectFile>` plus the two shared
handles that make cross-file type identity work:

- `type_interner: Arc<TypeInterner>` — every `ProjectFile` shares it, so
  `TypeId`s are globally unique across files and cross-file comparisons are O(1)
  identity checks rather than structural re-matching.
- `definition_store: Arc<DefinitionStore>` — the single global `DefId` →
  `TypeId` resolver. Sharing it is a prerequisite for cross-file `Lazy(DefId)`
  references to resolve, and it is only sound *because* the interner is shared
  (one type universe). See
  [`solver-types-intern-def.md`](solver-types-intern-def.md) and
  [`checker-context-and-state.md`](checker-context-and-state.md) for the
  `DefId`/`Lazy` contract.

Supporting state includes a `DependencyGraph`, a `SymbolIndex` (for workspace
symbols and auto-import candidates), a `FileIdAllocator` (stable per-file `u32`
indices used as `DefinitionStore` file provenance), a
`SkeletonFingerprintCache` of export signatures, an `open_files` set (never
evicted), a `focused_file` hint, and a `diagnostics_generation: u64` barrier
(see Caches below).

### `ProjectFile`: a self-contained per-file analysis unit

[`crates/tsz-lsp/src/project/core/project_file.rs`](../../../crates/tsz-lsp/src/project/core/project_file.rs)
defines `ProjectFile`, which owns a `ParserState` (and its arena), a
`BinderState`, a `LineMap`, the shared `Arc<TypeInterner>`, an optional shared
`Arc<DefinitionStore>`, and — critically — the per-file caches: an
`Option<TypeCache>`, a `ScopeCache`, and `cached_diagnostics`. It also stores
`export_signature` (a position-independent fingerprint of the file's public
API), a `content_hash`, a `file_idx`, and a `last_accessed: Instant` for
eviction.

`ProjectFile::provider_context()` returns an `LspProviderContext` (defined in
[`crates/tsz-lsp/src/project/file_context.rs`](../../../crates/tsz-lsp/src/project/file_context.rs)),
a `Copy` borrowed view bundling the five binder-tier inputs (`arena`, `binder`,
`line_map`, `file_name`, `source_text`). Feature dispatch builds binder-tier
providers via `Provider::from_context(ctx)` instead of repeating five accessors
at every call site — the shape matches the `binder` arm of the macro exactly.
(Sites that also need `&mut file.scope_cache` keep the flat per-field
destructuring so the borrow checker can track disjoint fields.)

The type-aware feature methods on `ProjectFile`
(`get_hover_with_stats`, `get_signature_help_with_stats`,
`get_completions_with_stats`) all follow the same shape: build the `full`-tier
provider via `with_strict(...)`, then call `*_with_scope_cache(root, position,
&mut self.type_cache, &mut self.scope_cache, scope_stats)`. Because the
`TypeCache` and `ScopeCache` are threaded by `&mut`, the cost of building the
type view is paid once and reused across the burst of requests the editor fires
at a single cursor.

### Diagnostics: `compute_diagnostics` and the shared checker

`ProjectFile::compute_diagnostics`
(project_file.rs) is where the editor surface runs the *whole-file* checker.
It builds a `CheckerOptions` from `self.strict`, creates a `QueryCache` over the
shared interner, and constructs the checker through one of four constructors
chosen by `(self.type_cache.take(), &self.definition_store)`:

```
(Some(cache), Some(def_store)) → CheckerState::with_cache_and_shared_def_store
(Some(cache), None)            → CheckerState::with_cache
(None,        Some(def_store)) → CheckerState::new_with_shared_def_store
(None,        None)            → CheckerState::new
```

The shared-`DefinitionStore` constructors are what make cross-file diagnostics
correct inside a `Project`: a `Lazy(DefId)` reference to a type declared in
another file resolves through the one global store. It then calls
`checker.check_source_file(self.root)`, maps each
`checker.ctx.diagnostics` entry through `convert_diagnostic` into an
`LspDiagnostic` (UTF-8 byte spans translated to editor positions via the
`LineMap`), writes back the `TypeCache` via `checker.extract_cache()`, clears
`diagnostics_dirty`, and stores `cached_diagnostics`. The checker is the same
`CheckerState` the CLI uses; the LSP layer adds only the cache plumbing and the
position/format conversion. See
[`checker-error-reporter-diagnostics.md`](checker-error-reporter-diagnostics.md)
for how the diagnostics themselves are produced and ordered.

## Caches and invariants

The editor surfaces add several caches on top of the kernel's own caches (see
[`solver-caches-objects-contextual-compat.md`](solver-caches-objects-contextual-compat.md)).
Each has an explicit invalidation contract.

| Cache | Owner | Keyed by | Invalidation |
| --- | --- | --- | --- |
| `ScopeCache` (`FxHashMap<u32, Vec<SymbolTable>>`) | `ProjectFile.scope_cache` | node id | `scope_cache.clear()` on re-parse / `reset_analysis_state` |
| `TypeCache` | `ProjectFile.type_cache` (`Option`) | checker-internal type keys | extracted/reinstalled per request; dropped (`= None`) on re-parse; preserved across feature calls within a file version |
| `cached_diagnostics` (+ `diagnostics_result_id`, `diagnostics_generation`) | `ProjectFile` | file version | served only while the stamped generation matches `Project.diagnostics_generation`; cleared by own edits |
| shared `Arc<TypeInterner>` | `Project` | content-addressed | never reset per-file; lives for the project's lifetime |
| shared `Arc<DefinitionStore>` | `Project` | `DefId` (+ `file_idx` provenance) | `invalidate_file(file_idx)` before re-binding a changed file |
| `export_signature` / `SkeletonFingerprintCache` | `ProjectFile` / `Project` | `file_idx` | updated on every `set_file`/`update_file`; gates dependent invalidation |
| `SymbolIndex` | `Project` | file name | re-indexed per `set_file` |

Key invariants:

- **The interner is never reset when a file changes.** The comment on
  `reset_analysis_state` is explicit: the `type_interner` is shared and may hold
  `TypeId`s referenced by other files, so on a file change only the *per-file*
  caches (`type_cache`, `scope_cache`) are invalidated, forcing recomputation
  against the still-valid shared interner. Resetting the interner would
  invalidate every other file's `TypeId`s.

- **`DefId`s carry file provenance.** Each `ProjectFile` gets a stable
  `file_idx` from `Project.file_id_allocator`; the binder stamps
  `decl_file_idx` with it, and every `DefinitionInfo` the checker registers
  carries it. When a file is replaced, `definition_store.invalidate_file(idx)`
  removes exactly that file's `DefId`s before the re-bind registers fresh ones
  under the *same* index — so dependents that re-resolve see the new
  definitions.

- **`set_file` is content-hash gated.** `Project::set_file` first compares
  `hash_source_content(&source_text)` against the existing file's
  `content_hash` and returns immediately on a match — a no-op `didOpen` on an
  already-loaded file (or a `didSave` without changes) skips re-parse, re-bind,
  re-index, and invalidation entirely.

- **Cross-file invalidation is coarse and generation-stamped.** Because the
  dependency graph keys edges by raw import specifiers and cannot enumerate
  every dependent affected by an *inferred* export type, `set_file` and
  `update_file` call `invalidate_all_cached_diagnostics()`, which is O(1): it
  bumps `Project.diagnostics_generation` instead of walking files. A
  `ProjectFile`'s `cached_diagnostics` is served only while its stamped
  `diagnostics_generation` matches the project's — so a change in file A can
  never leave file B serving stale diagnostics, without paying a per-file scan.

## The pull-model diagnostics protocol

[`crates/tsz-lsp/src/project/diagnostic_pull.rs`](../../../crates/tsz-lsp/src/project/diagnostic_pull.rs)
implements LSP `textDocument/diagnostic` and `workspace/diagnostic`. The
result-id mechanism lets the editor avoid re-receiving unchanged diagnostics:

- Every recompute assigns a fresh, monotonically increasing
  `diagnostics_result_id` to `cached_diagnostics` — never reused, so a
  client-provided `previousResultId` can only match the exact recompute that
  produced it.
- `get_document_diagnostics_pull(file, previous_result_id)`: if the client sent
  a `previous_result_id`, the cache is still valid (generation matches), and the
  stored result id equals the client's, it returns an **`Unchanged`** report and
  **runs no checking**. Otherwise it returns a **`Full`** report — served from a
  valid cache without rechecking, or recomputed with a fresh id.
- `get_workspace_diagnostics_with_previous` iterates files in sorted order,
  applying the same per-file logic and tagging each `WorkspaceDiagnosticReportItem`
  as `Unchanged` or `Full`.

This is the same diagnostic data the CLI prints, projected into the
editor's incremental pull protocol; the LSP layer adds the result-id identity
and the generation check, not the diagnostics.

## The WASM surface

### `TsLanguageService`: the single-file IDE shim

[`crates/tsz-wasm/src/wasm_api/language_service.rs`](../../../crates/tsz-wasm/src/wasm_api/language_service.rs)
is the most direct demonstration that the editor surface owns no type
algorithm. `TsLanguageService::new` parses (`ParserState::parse_source_file`),
builds a `LineMap`, binds (`BinderState::bind_source_file`), and holds a
per-file `TypeInterner`. Each exported method instantiates the *same* `tsz-lsp`
provider the native LSP server uses, calls it, and serializes the result to
JSON:

| WASM method (JS name) | Provider used | Output |
| --- | --- | --- |
| `getCompletionsAtPosition` | `Completions::new_with_types` | JSON array of `{label, kind, detail, documentation}` |
| `getQuickInfoAtPosition` | `HoverProvider::new` + `get_hover` | tsserver `QuickInfo` with display parts and `textSpan` |
| `getDefinitionAtPosition` | `GoToDefinition::new` | JSON array of `{fileName, textSpan}` |
| `getReferencesAtPosition` | `FindReferences::new` | JSON array of reference entries |
| `updateSource` | re-parse + re-bind + rebuild `LineMap` | — |

The `CompletionItemKind` → numeric LSP-kind mapping (`Variable`/`Const`/`Let` →
6, `Function` → 3, `Class` → 7, `Method` → 2, `Property` → 10, …) lives in this
file because it is wire-format translation, not semantics. The provider produces
the semantic kind; the shim maps it to the LSP enum the browser expects.

### `TsProgram`: the batch program shim

[`crates/tsz-wasm/src/wasm_api/program.rs`](../../../crates/tsz-wasm/src/wasm_api/program.rs)
mirrors TypeScript's `Program`. `TsCompilerOptions` is a `serde` DTO that
delegates `strict`-family resolution to the shared `WasmCompilerOptions` owner
in tsz-core (so this surface cannot drift from the rest of the WASM API, per
issue #13117) and only contributes its non-strict default and the surface-only
options (`declaration`, `checkJs`, `allowJs`, `noResolve`) plus the numeric
`u8` `target`/`module` DTO encoding.

`ensure_compiled()` reuses the core *parallel* driver rather than re-running the
pipeline by hand: `parse_and_bind_parallel` (or
`parse_and_bind_parallel_with_libs` when lib files were added via `addLibFile`),
then `merge_bind_results` into a `MergedProgram`. `collect_semantic_diagnostics`
calls `check_files_parallel(merged, &checker_options, &self.lib_files)` — the
same `MergedProgram`/`check_files_parallel` machinery the parallel CLI driver
uses (see [`end-to-end-timeline.md`](end-to-end-timeline.md)). The shim's only
additions are option DTO conversion, cache invalidation on
`setCompilerOptions`/`addSourceFile`, and **byte → UTF-16 offset conversion**:
`byte_offset_to_utf16` / `byte_length_to_utf16` count `c.len_utf16()` across the
prefix, because the Rust pipeline uses UTF-8 byte offsets but JavaScript/Monaco
expect UTF-16 code-unit offsets (identical for ASCII, divergent for em dashes,
emoji, CJK).

### `TsTypeChecker`: predicates over an interner

[`crates/tsz-wasm/src/wasm_api/type_checker.rs`](../../../crates/tsz-wasm/src/wasm_api/type_checker.rs)
holds only `interner: Arc<TypeInterner>`. Its implemented methods are
*structural reads* delegated to solver helpers: `typeToString` →
`TypeFormatter::format`; `isUnionType`/`isIntersectionType`/`isTypeParameter`/
`isArrayType`/`isTupleType`/`isNullableType` → the matching free functions in
`tsz_solver` and `tsz_solver::ts_type_flags`; `getTypeFlags` →
`type_id_ts_flags`; and a `define_checker_type_getters!` macro that exposes the
intrinsic `TypeId`s (`getAnyType` → `TypeId::ANY`, `getStringType` →
`TypeId::STRING`, …). The node-keyed methods (`getTypeAtLocation`,
`getSymbolAtLocation`, `getDeclaredTypeOfSymbol`, `getPropertiesOfType`, …) are
documented placeholders returning `TypeId::ANY` / `u32::MAX` / empty — the
node→type bridge that a full handle-resolving implementation would need is not
wired through this DTO. The *real* node-keyed type queries an editor needs are
served by `TsLanguageService`, which goes through the `tsz-lsp` providers and a
real `CheckerState`. This split keeps the rule intact: where the WASM checker
shim answers, it answers by reading the interner/solver, never by computing.

### `transpileModule`: single-file emit, no semantic validation

[`crates/tsz-wasm/src/wasm_api/emit.rs`](../../../crates/tsz-wasm/src/wasm_api/emit.rs)
implements tsc's `transpileModule`/`transpile`. `compile_transpile_source`
parses, runs the emit pipeline — `LoweringPass::new(&arena, &ctx).run(root_idx)`
then `Printer::with_transforms_and_options(...).emit(root_idx)` — and optionally
generates a source map. There is **no checker call** on the emit path:
`transpileModule` is type-stripping transpilation, exactly like tsc's, so the
emit surface adds no semantic validation and performs no output surgery. The
file owns only wire-level concerns: output-extension mapping
(`.mts`→`.mjs`, `.cts`→`.cjs`, `.ts`/`.tsx`→`.js` via
`js_output_name_for_source_map`), the `export {};` empty-module preservation
(`preserve_empty_module_output`), and `sourceMappingURL` placement (inline
base64 vs external `.map`, inline winning to match tsc). Optional `.d.ts` output
is gated behind the `dts` cargo feature and the `DeclarationEmitter`. The actual
transforms are owned by the emitter — see [`emitter.md`](emitter.md) and
[`emitter-async-generator-decorators-modules.md`](emitter-async-generator-decorators-modules.md).

## The fourslash harness

[`crates/tsz-lsp/src/fourslash.rs`](../../../crates/tsz-lsp/src/fourslash.rs)
is a *test harness only*, and its module doc states the architectural boundary
emphatically: production provider modules **must not** import from, depend on,
or inspect anything in this module. They must not branch on marker-like
comments, recognize marker names, or reorder results to match fourslash
expectations.

The harness owns all knowledge of the `/*name*/` marker DSL. `parse_markers`
(in `fourslash/parsing.rs`) translates a marker-annotated source string into (1)
a *cleaned* source with marker comments stripped, and (2) a set of `Marker`
positions as plain `(file, line, character, offset)` tuples in cleaned
coordinates. Multi-file tests use `parse_multi_file` with `// @filename:`
directives. Every request a fourslash test makes is then converted to the
*ordinary* LSP inputs — file URI, document text, cursor offset/range — and
driven through a real `Project`, so the providers behave exactly as they would
for a user-typed file opened in an editor. The fluent assertion helpers
(`DefinitionResult::expect_at_marker`, hover's
`expect_display_string_contains`, etc.) check the provider's real output. This
boundary is what lets fourslash conformance be a *parity floor* (the `hold`
roadmap goal) rather than a set of behaviors the providers special-case.

## Edge cases and tsc parity

- **Symbol-query backtracking.** A cursor in trailing whitespace, inside a
  comment, or just past a token must still answer about the *nearest preceding
  symbol*, matching tsserver. Hover and navigation gate on
  `utils::is_symbol_query_node` and backtrack via
  `find_symbol_query_node_at_or_before` when the literal offset lands on a
  non-symbol node (`is_comment_context`, `should_backtrack_to_previous_symbol`).
  Navigation additionally rejects a backtracked node unless it ends at or very
  near the cursor, so `1;|` does not jump back to an `x` several tokens earlier.

- **`this` / `super` keyword hover.** These are handled before symbol-query
  filtering (`hover_for_this_keyword`, `hover_for_super_keyword`) because they
  are not ordinary identifier symbols but still have quickinfo in tsserver.

- **`Foo.#prop` in a type position.** Private members are invalid in type
  references and tsserver reports no quickinfo; hover suppresses via
  `is_private_identifier_in_type_context` to match.

- **`const` literal preservation in hover.** `const c = 0` displays as `0`, not
  `number` — the provider keeps the checker's literal `TypeId` and only widens
  via the initializer text for `let`/`var`, mirroring tsc's display.

- **Primitive member completions.** `"".|` and `(0).|` offer
  `String.prototype` / `Number.prototype` members via the solver's
  `apparent_*_members` helpers, not by special-casing the literal — the same
  apparent-type logic the checker uses for property access (see
  [`checker-jsx-properties-accessors-enums.md`](checker-jsx-properties-accessors-enums.md)).

- **Completion ordering.** `sortText` values are copied from TypeScript's
  `SortText` enum so the editor list order is byte-identical to tsserver,
  including the `z`-prefixed deprecated sort and null-byte object-literal
  tiebreakers.

- **UTF-16 offsets at the WASM boundary.** Multi-byte source (em dashes, emoji,
  CJK) makes byte offsets diverge from the UTF-16 offsets JS/Monaco expect;
  `TsProgram` converts both diagnostic `start` and `length`, and `TsSourceFile`
  saturates byte lengths to `u32::MAX` rather than wrapping (issue #4778), which
  preserves the `end >= pos` invariant.

- **Empty-module emit.** A module-syntax file that transpiles to nothing emits
  `export {};` (non-CommonJS) so the output is still a module, matching tsc's
  `transpileModule`.

## How this surface stays a read layer

Three structural facts keep both surfaces honest:

1. **The tier macro gates type access.** Only `full`-tier providers hold the
   `interner`, and the only way they use it is to build a `CheckerState`. There
   is no provider field or constructor that hands a provider the raw means to
   relate, infer, instantiate, or evaluate a type.

2. **All semantic answers route through `CheckerState`.** `get_type_of_node`,
   `get_type_of_symbol`, `format_type`, and `check_source_file` are the entire
   vocabulary the surfaces use; each delegates into the solver. The surfaces
   read solver *query helpers* (`apparent_primitive_members`, `FunctionShape`,
   the `visitor`, the `is_*_type` predicates) for shape decisions and never
   pattern-match raw `TypeData` or construct a `TypeKey`.

3. **The shared interner + def store make multi-file identity the kernel's
   job.** The `Project` provides one `TypeInterner` and one `DefinitionStore`;
   cross-file correctness then falls out of the solver's `DefId`/`Lazy`
   resolution, not out of any LSP-specific type reconstruction.

Where to go next: [`checker-context-and-state.md`](checker-context-and-state.md)
for the `CheckerState` these surfaces build per request;
[`solver-caches-objects-contextual-compat.md`](solver-caches-objects-contextual-compat.md)
for the `QueryCache`/`TypeCache` they thread;
[`module-resolution-engine.md`](module-resolution-engine.md) for how `Project`
resolves the imports its cross-file features depend on; and
[`driver-incremental-and-watch.md`](driver-incremental-and-watch.md) for the
batch/watch counterpart to the `Project` container.
