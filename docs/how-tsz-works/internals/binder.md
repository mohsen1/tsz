# Binder: Symbols, Scopes, Hoisting, Flow Skeleton, and Module Graph

The binder (`crates/tsz-binder`) is the layer that turns a syntax-only AST into a
*declaration world*: a set of named `Symbol`s, a scope/container tree that says
where each name is visible, a control-flow skeleton of `FlowNode`s that later
narrowing rides on, and the import/export wiring that links files into a module
graph. It walks every node of one source file exactly once (`bind_node`) and
produces `BinderState` — a single serializable bag of arenas, hash maps, and
small caches that the checker reads but does not own.

The defining contract is **no type computation**. The binder never asks "is `A`
assignable to `B`", never instantiates a generic, never evaluates a conditional
or mapped type, and never constructs a solver `TypeId`. It records *structural
facts* (this node declares a symbol; this symbol has these flags; this name is
exported; this AST node was reached with this flow node active) and leaves every
semantic answer to the checker and solver. The closest the binder gets to
"semantics" is `SemanticDefEntry` — a flat, AST-derived description of a top-level
declaration's identity (kind, name, type-parameter arity, heritage names) that
the checker later converts into a solver `DefId`. Even there, the binder copies
strings out of the AST; it does not resolve them.

Owns / Must not own:

| Owns | Must not own |
| --- | --- |
| `SymbolId`/`Symbol` allocation, declaration merging, symbol flags | Type relations, inference, instantiation, evaluation |
| The scope tree (`Scope`, `ScopeId`, `ContainerKind`) and `node_scope_ids` | `TypeId`, `TypeData`, solver interning |
| Hoisting of `var` and (Annex B) function declarations | Property type lookup, signature resolution |
| The flow skeleton (`FlowNode`, `FlowNodeId`, antecedent edges) | Flow *narrowing* (the checker/solver walk these edges) |
| Import/export symbol wiring, re-export tables, augmentation tables | Module *type* resolution, declaration emit policy |
| `SemanticDefEntry` (AST-derived identity facts) | `DefId` creation (checker does this from the entries) |

Related reading: [front-end-scanner-parser](front-end-scanner-parser.md) (what
feeds the binder), [checker-context-and-state](checker-context-and-state.md) and
[checker-flow-and-narrowing](checker-flow-and-narrowing.md) (what consumes it),
[checker-declarations-modules](checker-declarations-modules.md) (module
resolution on top of the binder's tables), and
[solver-types-intern-def](solver-types-intern-def.md) (where `DefId` lives).

---

## 1. Crate layout and module map

The crate root (`crates/tsz-binder/src/lib.rs`) re-exports the public data types
and keeps the algorithm modules private. The directory split mirrors *concern*,
not node kind:

| Path | Role |
| --- | --- |
| `symbols.rs` | `Symbol`, `SymbolId`, `SymbolTable`, `SymbolArena`, `StableLocation`, `symbol_flags` |
| `scopes.rs` | `Scope`, `ScopeId`, `ContainerKind` (the persistent scope record) |
| `flow.rs` | `FlowNode`, `FlowNodeId`, `FlowNodeArena`, `flow_flags` |
| `state/mod.rs` | `BinderState` struct definition + every `Arc`-shared field + `SemanticDefEntry`/`SemanticDefKind` |
| `state/core.rs` | `BinderState` constructors, `reset`, `bind_source_file`, scope enter/exit, `stamp_file_idx` |
| `state/resolution.rs` | scope-walk identifier resolution, `find_enclosing_scope`, import-alias following |
| `state/flow_helpers.rs` | flow-node factories (`create_branch_label`, `create_flow_condition`, `add_antecedent`, `finish_flow_label`) |
| `state/export_surface.rs` | `ExportSurface` — structured export topology for emit/LSP |
| `state/declaration_summary.rs` | `DeclarationSummary` — declaration-emit facts boundary |
| `state/lib_merge.rs` | `merge_lib_contexts_into_binder` — fold lib symbols into a file's arena with remapped IDs |
| `state/core_jsdoc.rs` | JSDoc-driven declaration binding (`@import`, etc.) |
| `nodes/binding.rs` | the `bind_node` dispatch, `declare_symbol`, hoisting collection, `can_merge_flags` |
| `nodes/binding_scope.rs` | `enter_scope`/`exit_scope` wrappers (export/member capture at scope exit) |
| `nodes/flow_statements.rs` | flow construction for `if`/`while`/`for`/`return`/`break`/`continue` |
| `nodes/names.rs` | identifier extraction, modifier helpers, binding-pattern name collection |
| `binding/declaration.rs` | class/interface/function/enum/variable declaration binding, `SemanticDefDetails` |
| `binding/expression_flow.rs` | expression binding that produces flow edges (assignments, calls, `&&`/`||`/`??`) |
| `binding/accessors_flow.rs` | accessor binding, `record_flow`, `with_fresh_flow` (function-body flow reset) |
| `binding/semantic_defs.rs` | `record_semantic_def*` recorders for `SemanticDefEntry` |
| `binding/stack_guard.rs` | amortized stack-overflow breaker for the recursive walk |
| `binding/validation.rs` | post-bind sanity checks (`validate_symbol_table`), lib-merge diagnostics |
| `modules/binding.rs` | namespace/module declaration binding, augmentation routing, export population |
| `modules/import_export.rs` | import/export declaration binding, alias symbol creation |
| `modules/resolution_debug.rs` | `ModuleResolutionDebugger` (merge/declaration event log) |
| `lib_loader.rs` | `LibLoader`, `LibFile` (load and parse `lib.*.d.ts` from disk) |

No file exceeds the repo's 2000-line ceiling; `nodes/binding.rs` (~2018) and
`state/core.rs` (~1856) are the largest and are split by concern, not arbitrary
line budget.

---

## 2. Identity handles

The binder allocates two of the four canonical `u32` identity handles
(`SymbolId`, `FlowNodeId`); `ScopeId` is binder-internal, and `NodeIndex`/`Atom`
come from the parser/scanner. All are produced by the `define_id!` macro in
`crates/tsz-common`, which emits a `#[derive(Copy, Clone, ... Hash)] struct
Name(pub u32)` plus a `NONE` sentinel.

| Handle | Defined in | Sentinel | Allocator |
| --- | --- | --- | --- |
| `SymbolId(u32)` | `symbols.rs` | `max` (`u32::MAX`) | `SymbolArena::alloc` / `alloc_from` |
| `FlowNodeId(u32)` | `flow.rs` | `max` | `FlowNodeArena::alloc` |
| `ScopeId(u32)` | `scopes.rs` | `max` | `next_persistent_scope_id` (index into `scopes` vec) |

Important: `SymbolId`, `FlowNodeId`, and `ScopeId` all use `sentinel: max`, so
`NONE == u32::MAX` and ID `0` is a *valid* handle (the START flow node, the root
scope, the first symbol). This differs from `Atom` (`sentinel: zero`), where `0`
is the empty/none atom. The `SymbolArena` additionally carries a `base_offset`:
checker-local transient symbols are allocated in a separate arena with a high
`base_offset` so their IDs never collide with binder-allocated IDs, and
`SymbolArena::get` returns `None` for an ID below its `base_offset` (an ID from a
different arena).

`StableLocation { file_idx, pos, end }` (in `symbols.rs`) is a re-parse-stable
*alternative* to `NodeIndex`. Every `Symbol` keeps `stable_declarations` in
lockstep with `declarations` (the documented invariant `stable_declarations.len()
== declarations.len()`), and `stable_value_declaration` parallel to
`value_declaration`. During single-file binding these carry `file_idx == u32::MAX`
("unassigned"); the driver later promotes them via `stamp_file_idx`. They are
Phase-1 plumbing for arena-less cross-file identity; today consumers still read
the `NodeIndex` fields.

---

## 3. The Symbol model

```text
Symbol
├── flags: u32                  // symbol_flags::CLASS | INTERFACE | ...
├── escaped_name: String
├── declarations: Vec<NodeIndex>            // every declaring node
├── stable_declarations: Vec<StableLocation> // parallel, re-parse-safe
├── value_declaration: NodeIndex            // the value-side decl (var/func/class)
├── parent: SymbolId                        // containing namespace/class symbol
├── exports: Option<Box<SymbolTable>>       // namespace/module members
├── members: Option<Box<SymbolTable>>       // class/interface members
├── is_exported / is_type_only / is_umd_export: bool
├── decl_file_idx: u32                      // owning file (u32::MAX = single-file)
└── import_alias: Option<Box<ImportAliasData>>  // out-of-lined import payload
```

`symbol_flags` (in `symbols.rs`) is a direct port of TypeScript's `SymbolFlags`
bitset: `FUNCTION_SCOPED_VARIABLE` (`var`/param), `BLOCK_SCOPED_VARIABLE`
(`let`/`const`), `PROPERTY`, `FUNCTION`, `CLASS`, `INTERFACE`, `REGULAR_ENUM`,
`CONST_ENUM`, `VALUE_MODULE`, `NAMESPACE_MODULE`, `TYPE_ALIAS`, `ALIAS` (import
alias), `TYPE_PARAMETER`, accessors, and so on. Composite masks (`VALUE`, `TYPE`,
`NAMESPACE`, `MODULE`) and the per-kind `*_EXCLUDES` masks mirror tsc's
declaration-merge legality rules — e.g. `INTERFACE_EXCLUDES = TYPE & !INTERFACE &
!CLASS` encodes "an interface can merge with another interface or a class but not
with a type alias or enum". The Rust port adds explicit parentheses because Rust
binds `&` tighter than `|`, which the comment in `symbol_flags` calls out.

The import-alias payload is **out-of-lined** (`Box<ImportAliasData>`): since
fewer than ~5% of symbols are import aliases, the module specifier, renamed
export name, and `resolution-mode` override live behind a heap box that only
alias symbols allocate (PR #13072). Read through `import_module()`,
`import_name()`, `import_resolution_mode()`; write through `set_import_*`.

### SymbolArena allocation and the shared prefix

`SymbolArena::alloc(flags, name)` (in `symbols.rs`) computes the next ID as
`base_offset + len()`, pushes a fresh `Symbol`, and incrementally updates a
name-index for O(1) `find_by_name`. `alloc_from(&source)` clones an existing
symbol with a new ID (used when a lib symbol is folded into a local arena).

The arena has two storage halves: an immutable `shared_prefix: Arc<Vec<Symbol>>`
and a mutable `symbols: Arc<Vec<Symbol>>`. A user-file binder cloned from a
*premerged lib binder* keeps the entire lib symbol universe in `shared_prefix`
(zero-copy via the `Arc`) and appends file-local symbols into `symbols`.
`get(id)` indexes into the prefix when `idx < shared_prefix.len()`, otherwise into
`symbols`; `get_mut` calls `materialize_shared_prefix()` to copy-on-write the
prefix back only if a shared-prefix symbol is mutated. This is how thousands of
per-file binders avoid each deep-cloning the lib symbol set.

### SymbolTable: name and atom indexing

`SymbolTable` (in `symbols.rs`) maps names to `SymbolId`s and is the contents of
every scope. It carries **two** indexes:

- `symbols: Arc<FxHashMap<String, SymbolId>>` — authoritative, by escaped name.
- `atom_symbols: Arc<FxHashMap<(usize, AstAtom), SymbolId>>` — by
  `(arena_owner_key, parsed-identifier atom)`, a per-arena accelerator.

`AstAtom` values are arena-local, so the arena pointer is part of the key; a
table shared across files can never resolve a foreign atom. Lookups go through
`get_by_atom_or_name(atom_key, name)`: try the same-arena atom first, fall back to
the string. The string map is always authoritative — the atom side-index only
*accelerates* same-arena lookups and resolves to identical strings. Both maps are
`Arc`-wrapped, so cloning a table is an O(1) refcount bump and mutation routes
through `Arc::make_mut` (free at refcount 1). `merge_filtered_from` copies
retained entries (name keys plus atom side-keys) when promoting a namespace
scope's table into its export table at scope exit.

---

## 4. Scopes and the container tree

A `Scope` (in `scopes.rs`) is `{ parent: ScopeId, table: SymbolTable, kind:
ContainerKind, container_node: NodeIndex }`. `ContainerKind` is one of
`SourceFile`, `Function`, `Module` (namespace/module body), `Class`, or `Block`.
The key behavioral predicate is `is_function_scope()` — true for `SourceFile`,
`Function`, and `Module` — which marks the scopes where `var` hoisting lands.

Scopes form a *persistent* tree, not a traversal stack. They live in
`BinderState.scopes: Arc<Vec<Scope>>`, addressed by `ScopeId` (= index). The
mapping `node_scope_ids: Arc<FxHashMap<u32, ScopeId>>` records, for every node
that *creates* a scope, which `ScopeId` it owns. Because the parent link is
stored on the `Scope` itself, the checker can re-derive any node's lexical scope
*after* binding without replaying a stack — this is what makes checking
"stateless" with respect to traversal order.

`enter_persistent_scope_with_capacity` (in `state/core.rs`) allocates the next
`ScopeId` via `next_persistent_scope_id(scopes.len())` (which refuses to overflow
the `u32::MAX` sentinel), pushes a child `Scope` linked to `current_scope_id`,
records `node_scope_ids[node] = new_id`, and sets `current_scope_id`.
`exit_persistent_scope` simply follows the parent link. The public
`enter_scope`/`exit_scope` wrappers (in `nodes/binding_scope.rs`) delegate to
these and additionally **capture exports/members at exit**:

- For `ContainerKind::Module`, `exit_scope` filters the live scope table to the
  symbols that pass the export test (`is_exported`, `EXPORT_VALUE`, or an
  `export_all` ambient/`declare global` body) and stores them in the module
  symbol's `exports` table via `merge_filtered_from`.
- For `ContainerKind::Class`, it clones the live scope table into the class
  symbol's `members`.

`current_scope()` returns `&scopes[current_scope_id].table` (or a shared empty
table pre-bind). This is "the single live declaration table" — there is no
separate transient stack.

---

## 5. The binding walk: `bind_source_file`

`bind_source_file(arena, root)` (in `state/core.rs`) is the entry point. It runs
once per file and is structured as a small set of passes:

```text
bind_source_file(arena, root)
 ├─ reset_stack_overflow_flag()         // per-file breaker reset
 ├─ clear_resolution_caches()           // SymbolIds about to change
 ├─ snapshot pre-merged lib symbols out of file_locals
 ├─ pre-size node_symbols / node_flow / symbols / scopes from statement count
 ├─ enter_persistent_scope(SourceFile, root)   // ScopeId(0), the root scope
 ├─ seed root scope with pre-merged lib symbols
 ├─ alloc START flow node -> current_flow
 ├─ is_external_module = source_file_is_external_module(...) || .mts/.cts/...
 ├─ is_strict_scope = always_strict || "use strict" prologue
 ├─ PASS 1a: collect_hoisted_declarations(statements)
 ├─ PASS 1b: process_hoisted_functions(arena)      // declare hoisted funcs
 ├─ PASS 1c: process_hoisted_vars(arena)           // declare hoisted vars
 ├─ PASS 2:  for stmt in statements { bind_node(stmt); top_level_flow[stmt]=current_flow }
 ├─ bind_jsdoc_import_tags(...)
 ├─ resolve_deferred_export_assignment(...)         // forward-ref `export = X`
 ├─ resolve_deferred_named_exports(...)             // forward-ref `export { X }`
 ├─ populate_module_exports_from_file_symbols(...)
 ├─ file_locals = root scope table (+ merged-back lib symbols)
 └─ stamp_file_idx()  (if driver assigned file_idx)
```

The two-pass structure is the heart of hoisting parity: `var` and (in legacy
modes) `function` declarations are bound *before* the statement walk so that
references appearing textually earlier than the declaration still resolve.

`is_external_module` is decided structurally: a file is a module if it has any
top-level `import`/`export` (`source_file_is_external_module`) or has an
`.mts`/`.cts`/`.mjs`/`.cjs` extension (`is_module_file_extension`). This single
boolean governs whether top-level declarations seed the cross-file global scope
(see `Symbol::is_cross_file_global`).

### `bind_node` dispatch and the stack guard

`bind_node(arena, idx)` (in `nodes/binding.rs`) is the recursive workhorse. Each
call first checks the thread-local stack breaker:

```text
if stack_overflow_tripped() { return; }                  // already over budget
if should_probe_stack() && headroom_below(1 MiB) {       // probe every 64th call
    trip_stack_overflow(); return;
}
stacker::maybe_grow(256 KiB, 2 MiB, || bind_node_by_node_kind(...));
```

The breaker (in `binding/stack_guard.rs`) packs a tripped flag and a probe
counter into one `u16` thread-local. `measured_headroom_below(None, _) == false`
is deliberate: on targets like `wasm32` where `stacker::remaining_stack()` returns
`None`, an unmeasurable stack must *not* count as critically low, or the binder
would abort mid-file and silently drop later declarations (issue #13815). The
breaker is reset at the top of each `bind_source_file` so a pathological earlier
file does not permanently disable safety for the rest of the batch.

`bind_node_by_node_kind` is a large `match node.kind` over `syntax_kind_ext`
constants. Statement and declaration kinds dispatch to dedicated binders
(`bind_class_declaration`, `bind_function_declaration`,
`bind_if_statement`, ...); blocks push a `ContainerKind::Block` scope; bare
identifiers just `record_flow(idx)`.

---

## 6. Hoisting

Hoisting runs in PASS 1 and only ever touches *names and flags* — never types.

`collect_hoisted_declarations_impl` (in `nodes/binding.rs`) recursively scans the
statement list, descending into `Block`, `If`, `While`/`Do`, `For`/`ForIn`/
`ForOf`, `Try`, `Switch`, and `Labeled` statements to find `var` declarations
(always function-scoped) and function declarations. The `in_block` flag tracks
whether the scan has entered a nested block, because that changes function-
declaration scoping.

`var` hoisting (`collect_hoisted_var_decl`): for each `VariableDeclaration` whose
list flags are not `let`/`const`, the declared identifier name(s) — including all
identifiers in a binding pattern via `collect_binding_identifiers` — are pushed
onto `hoisted_vars`. `process_hoisted_vars` then declares each with
`FUNCTION_SCOPED_VARIABLE` before the main walk.

Function hoisting (Annex B parity): a `FunctionDeclaration` is **block-scoped**
(and therefore *not* hoisted) when it sits inside a nested block AND
(`is_external_module || is_strict_scope || target >= ES2015`). In a non-strict,
non-module ES3/ES5 script, block-nested functions hoist (Annex B). Hoisted
functions land in `hoisted_functions` and are declared with the `FUNCTION` flag
by `process_hoisted_functions` before the body walk. `collect_hoisted_from_node`
distinguishes a function *body* block (whose top-level functions are at function
scope) from a nested statement block (whose functions are block-scoped) by
inspecting the block's parent node kind.

---

## 7. Declaring and merging symbols: `declare_symbol`

`declare_symbol(arena, name, flags, declaration, is_exported)` (in
`nodes/binding.rs`) is the single funnel for symbol creation. It returns the
`SymbolId` and wires the declaration into `node_symbols[declaration]`, the current
scope table (with atom side-key), and — at source-file scope — `file_locals`.

Its branching encodes TypeScript's declaration-merge rules and several
shadowing exceptions. In order:

1. **Same-name symbol already in the current scope?** Then several sub-cases:
   - *Cross-function synthetic `arguments`*: a real `arguments` (param or `var`)
     in a different function must not merge into a prior function's synthetic
     `arguments` symbol, or the two functions collapse and yield a spurious
     TS2403. Detected by comparing container symbols. (`arguments` is a true
     language builtin, so this is keyed on the reserved name — an allowed
     exception to the anti-hardcoding rule.)
   - *Existing symbol is from a lib binder, not local*: allocate a fresh local
     symbol that shadows the lib one, and write it into `file_locals` so
     resolution finds the local.
   - *`should_shadow_lib`*: file-scope value declarations (class/function) shadow
     identically-named lib *value* symbols because tsc resolves file-scope
     declarations before globals. The exact set differs by module vs script mode
     and is narrowed to avoid clobbering the other namespace —
     `collect_preserved_lib_meaning` re-attaches the lib symbol's *other-namespace*
     declarations (e.g. keep lib `interface Array<T>` alive when a user
     `const Array = 1` shadows the value), preventing a spurious TS2749.
   - *Namespace-scope non-exported `var` vs exported member*: keep them distinct
     (tsc treats `export var Origin` and a later `var Origin` in another block as
     separate symbols).
   - *Two ALIAS declarations that cannot merge*: keep the later as a distinct
     symbol so it shadows in expression resolution while preserving duplicate
     diagnostics.
   - *Otherwise* compute `can_merge = can_merge_flags(existing, new)`. If
     mergeable, OR the flags into the existing symbol, optionally upgrade
     `value_declaration`, append the declaration, and reuse the existing
     `SymbolId`. The merge is recorded in the `ModuleResolutionDebugger`.

2. **Hoisted-`var` reuse**: a `FUNCTION_SCOPED_VARIABLE` declaration already
   recorded in `node_symbols` (from PASS 1) reuses that symbol instead of
   re-allocating, then re-exposes it in the current scope table only when that
   scope is the var's home function scope or the function's own body block — not
   a nested statement block (which would cause a later block-scoped declaration to
   collide and emit a spurious TS2300).

3. **Otherwise** allocate a brand-new symbol, set its parent to the current
   container symbol, record `value_declaration` when the flags carry `VALUE`, and
   expose it in the current scope (and `file_locals` at source-file scope, but
   *not* inside a module-augmentation body — those go through a separate
   augmentation channel).

`can_merge_flags` (in `nodes/binding.rs`) is a pure predicate mirroring tsc's
merge legality: interface+interface, class+interface, module+module,
module+(class|function|enum), function+function, function+class, etc.

---

## 8. The flow skeleton

The binder builds a **control-flow graph skeleton** but performs no narrowing.
A `FlowNode` (in `flow.rs`) is `{ flags: u32, id, antecedent: Vec<FlowNodeId>,
node: NodeIndex }`. `flow_flags` mirrors tsc's `FlowFlags`: `START`,
`BRANCH_LABEL`, `LOOP_LABEL`, `ASSIGNMENT`, `TRUE_CONDITION`/`FALSE_CONDITION`,
`SWITCH_CLAUSE`, `ARRAY_MUTATION`, `CALL`, `AWAIT_POINT`, `YIELD_POINT`, plus the
composite `LABEL` and `CONDITION`. `FlowNodeArena::alloc(flags)` pushes a node and
returns its index-`FlowNodeId`.

Two singletons anchor the graph: the `unreachable_flow` node (allocated with
`UNREACHABLE` in every constructor, before anything else) and a per-file `START`
node allocated at the top of `bind_source_file`. `current_flow` tracks the
"active" flow node as the walk proceeds; `record_flow(node)` stamps
`node_flow[node] = current_flow` so the checker can later ask "what flow was
active at this expression". `top_level_flow[stmt]` additionally records the flow
after each top-level statement (for incremental binding).

### Flow factories and `add_antecedent`

The factories live in `state/flow_helpers.rs`:

- `create_branch_label()` / `create_loop_label()` — empty merge/back-edge labels.
- `create_flow_condition(flags, antecedent, condition)` — a `TRUE_CONDITION` or
  `FALSE_CONDITION` node carrying the condition expression and one antecedent.
- `create_flow_assignment`/`create_flow_call`/`create_flow_array_mutation`/
  `create_flow_await_point`/`create_flow_yield_point` — all share
  `create_flow_node_with_node`, chaining `current_flow` as the antecedent.
- `add_antecedent(label, antecedent)` — **skips** an antecedent that is `NONE` or
  equals `unreachable_flow`, and dedupes. This skip is what makes dead branches
  drop out of merge points.
- `finish_flow_label(label)` — tsc's `finishFlowLabel`: a label with 0 reachable
  antecedents collapses to `unreachable_flow`, with exactly 1 collapses to that
  antecedent, with several stays itself. Then assigns the result to
  `current_flow`. Used so that e.g. an `if` whose both arms `return` leaves the
  post-`if` code correctly unreachable rather than treating an antecedent-less
  merge label as reachable (which would drop later narrowing).

### Statement flow construction

`nodes/flow_statements.rs` builds the structured edges. For `bind_if_statement`:

```text
        bind(condition)
        pre = current_flow
        true  = create_flow_condition(TRUE,  pre, condition)
        current_flow = true; bind(then); after_then = current_flow
        false = create_flow_condition(FALSE, pre, condition)
        [if else:] current_flow = false; bind(else); after_else = current_flow
        merge = create_branch_label()
        add_antecedent(merge, after_then)
        add_antecedent(merge, after_else)
        finish_flow_label(merge)            // NOT a bare current_flow = merge
```

Loops (`bind_while_or_do_statement`, `bind_for_statement`,
`bind_for_in_or_for_of_statement`) create a `LOOP_LABEL`, push `break_targets`
(post-loop branch label) and `continue_targets`, bind the body, add the back-edge
antecedent to the loop label, and pop the targets. `do/while` and `while` skip the
false-condition post-loop edge when the condition is syntactically `true`
(`is_syntactically_true_condition`, which sees through parentheses). `return`,
`break`, and `continue` (`bind_return_or_throw_statement`,
`bind_break_statement`, `bind_continue_statement`) push the current flow onto the
relevant target label and then set `current_flow = unreachable_flow`, so anything
textually after them is unreachable unless reached by a label antecedent.

Function bodies get a fresh flow via `with_fresh_flow`/`with_fresh_flow_inner`
(in `binding/accessors_flow.rs`): a new `START` node is allocated, and for
closures (arrow/function expressions) the START's antecedent points back at the
enclosing flow so outer `const`/`let` narrowing is preserved inside the closure.
`return_targets` is saved/cleared so a non-IIFE function's `return` does not
redirect into an enclosing IIFE's return label.

The binder owns these edges; the *narrowing* over them belongs to the checker and
solver. See [checker-flow-and-narrowing](checker-flow-and-narrowing.md) and
[solver-narrowing](solver-narrowing.md).

---

## 9. Modules, imports, exports, and augmentation

### Imports

`bind_import_declaration` (in `modules/import_export.rs`) records the module
specifier into `file_import_sources` and creates `ALIAS` symbols for each binding:

- Default import (`import X from "m"`): an `ALIAS` symbol whose `import_name` is
  `"default"`.
- Namespace import (`import * as ns from "m"`): an `ALIAS` whose `import_name` is
  `"*"` (so the printer can render `typeof import("m")`).
- Named imports (`import { foo as bar } from "m"`): one `ALIAS` per specifier;
  `import_name` holds the original export name. `is_type_only` is set from the
  clause-level or specifier-level `type` modifier.

Each alias records `import_module` (the specifier) so the checker can resolve it
cross-file. Crucially, the binder does **not** resolve the import to a target type
— it only stores the wiring. `resolve_import_if_needed` (in `state/resolution.rs`)
follows the alias to a `SymbolId` when the target file's exports are available,
but that runs as part of resolution, not type checking.

### Exports and re-exports

`BinderState` carries the cross-file export tables (all `Arc`-shared so per-file
binders share them):

| Field | Holds |
| --- | --- |
| `module_exports` | per-file name -> `SymbolTable` of exports |
| `reexports` | `export { x } from "m"`: `(file, name) -> (source_module, orig_name)` |
| `wildcard_reexports` | `export * from "m"`: `file -> Vec<(source_module, is_type_only)>` |
| `alias_partners` | links a `TYPE_ALIAS` symbol to its `export * as X` ALIAS partner |
| `shorthand_ambient_modules` | bodyless `declare module "*.json"` (imports resolve to `any`) |
| `declared_modules` | every ambient module specifier seen |

Forward references are repaired after the walk: `resolve_deferred_export_assignment`
re-runs `export = X` statements (so `export = React` before `declare namespace
React` works) and `resolve_deferred_named_exports` re-marks `export { Hash }`
when the named declaration appeared later.

### Namespaces, ambient modules, and augmentations

`bind_module_declaration` (in `modules/binding.rs`) handles `namespace`/`module`
declarations. The three shapes it must distinguish — all structurally, never by
matching a user name except the reserved `global` keyword:

1. **`declare global { ... }`** (`is_global_augmentation`): the body binds
   *in place* at the boundary scope, the boundary table is **snapshotted and
   restored** afterward, and declarations are recorded in the dedicated
   `global_augmentations` channel (and `file_locals` for namespaces). The restore
   prevents augmentation symbols from shadowing the original lib declarations they
   augment (e.g. an `interface HTMLElement` augmentation displacing lib's).

2. **`declare module "spec" { ... }`** that is a *module augmentation*
   (`is_potential_module_augmentation` in an external-module or nested-ambient
   context): also binds in place with a save/restore of the boundary table, under
   the `in_module_augmentation` flag and `current_augmented_module` specifier, and
   records entries into `module_augmentations` plus
   `augmentation_target_modules`. A bodyless form registers a
   `shorthand_ambient_module`. Augmentation symbols are kept out of `file_locals`
   so they never overwrite the original module's exports. Within one file,
   repeated augmentation declarations of the same `(spec, name)` merge with each
   other via `module_augmentation_symbols` but never with a non-augmentation
   file-scope symbol (issue #6164).

3. **A normal `namespace`/`module`**: a `VALUE_MODULE`/`NAMESPACE_MODULE` symbol
   with a `ContainerKind::Module` child scope, whose exports are captured at
   `exit_scope`.

The actual *type-level* merge of augmentations happens later in the checker; see
[checker-declarations-modules](checker-declarations-modules.md).

---

## 10. Lib merging

`merge_lib_contexts_into_binder(lib_contexts)` (in `state/lib_merge.rs`) folds
the `lib.*.d.ts` symbol universe into a file's own arena. It cannot just borrow
lib `SymbolId`s — IDs across independent lib binders collide — so it:

1. Clones each lib `Symbol` into the local arena with a fresh remapped ID
   (`alloc_from`), building a `(lib_binder_ptr, old_id) -> new_id` remap.
2. Remaps internal references (`parent`, `exports`, `members`) to the new IDs.
3. Updates `file_locals` to point at the new IDs and records every new ID in
   `lib_symbol_ids` (so `should_shadow_lib` and resolution know which symbols are
   lib-origin).
4. Tracks each declaration's owning arena in `declaration_arenas` (a symbol like
   `Array` is declared across several lib files, so each declaration keeps its
   own arena) and the reverse remap in `lib_symbol_reverse_remap`.

The documented concurrency contract: the merge mutates only `self`; lib contexts
are read immutably, so a shared read-only lib set held at refcount > 1 (e.g.
across rayon workers) is never copy-on-write poisoned. `lib_type_namespace`
records lib *type* symbols that a local *value*-only declaration blocked from
`file_locals`, so the checker's type-position resolver can still find them
(TypeScript keeps value and type namespaces separate).

`program_globals` is a deliberately lib-only fallback table consulted by the
explicit `get_global_type*` accessors after a `file_locals` miss; the scope-chain
resolvers (`resolve_identifier`, `resolve_name_with_filter`) intentionally do
*not* consult it, so a program-global never shadows a declaring file's own local
(e.g. a user `interface EventSource` must win over DOM's within its own file).

---

## 11. Name resolution

The binder also *answers* name-resolution queries (the checker calls these; it
does not re-walk scopes itself). `resolve_identifier(arena, node)` (in
`state/resolution.rs`) is the canonical path:

```text
1. cache hit? -> return  (resolved_identifier_cache, keyed (arena_ptr, node))
2. scope = find_enclosing_scope(node); walk parent chain:
       scope.table.get_by_atom_or_name(atom, name) -> resolve_import_if_needed -> done
3. resolve_parameter_fallback (for scope-less bound-state binders)
4. file_locals.get_by_atom_or_name(...)
5. each lib_binder.file_locals lookup
6. miss
```

Both hits and misses are cached. The scope walk is bounded by
`MAX_SCOPE_WALK_ITERATIONS = 10_000` as a defensive cap.

`find_enclosing_scope(arena, node)` walks AST parent pointers (via
`arena.get_extended(idx).parent`) until it hits a node in `node_scope_ids`,
falling back to the root scope `ScopeId(0)`. Two parity wrinkles:

- A `ComputedPropertyName` on the walk forces the enclosing *class member*
  function scope to be skipped, because computed property names evaluate in the
  class scope, not the method scope (so `T` inside `[foo<T>()]<T>()` resolves to
  the class's type parameter, not the method's).
- The walk is **memoized with path compression** only past
  `ENCLOSING_SCOPE_MEMO_THRESHOLD = 32` hops and only on the computed-property-free
  prefix. Real identifiers sit a few nodes from their scope and pay zero cache
  cost; deeply nested types like `A<A<A<...>>>` (otherwise O(depth^2) to resolve
  every reference) are restored to linear time. The memo only ever short-circuits
  a walk that would reach the same scope anyway, so the
  `TSZ_DISABLE_ENCLOSING_SCOPE_CACHE` kill-switch must yield byte-identical
  diagnostics.

---

## 12. SemanticDefEntry: binder-owned identity (the DefId on-ramp)

This is the binder's only brush with semantics, and even here it copies AST
strings, not types. During `declare_symbol`-adjacent binding,
`record_semantic_def_ext` (in `binding/semantic_defs.rs`) records a
`SemanticDefEntry` per top-level CLASS/INTERFACE/`TYPE_ALIAS`/ENUM/NAMESPACE/
FUNCTION/VARIABLE symbol into `semantic_defs: Arc<FxHashMap<SymbolId,
SemanticDefEntry>>`. Each entry carries `kind` (a `SemanticDefKind` that *mirrors*
solver `DefKind` but lives in the binder crate to avoid a dependency cycle),
escaped `name`, `file_id`, `span_start`, type-parameter `count`+`names`,
`is_exported`/`is_const`/`is_abstract`/`is_declare`/`is_global_augmentation`,
`enum_member_names`, `parent_namespace`, and split `extends_names`/
`implements_names` (only simple identifier or dotted heritage names — not types).

The checker converts these entries into solver `DefId` + `DefinitionInfo` during
construction (`pre_populate_definition_store`), so DefIds exist before the type
walk and hot checker paths do not invent identity on demand. `merge_cross_file`
accumulates heritage names, enum members, export visibility, and type-parameter
arity when the same symbol spans multiple files. The result feeds
[solver-types-intern-def](solver-types-intern-def.md) and the file-skeleton used
by the parallel pipeline (`tsz_core::parallel::skeleton::FileSkeleton`; the
binder's own `BinderFileSummary` is test-only).

The export-emit boundary `ExportSurface` (in `state/export_surface.rs`) and its
`DeclarationSummary` wrapper (in `state/declaration_summary.rs`) are the other
binder-owned facts: a position-independent snapshot of exported locals,
named/wildcard re-exports, augmentations, and overload grouping, derived purely
from binder symbol tables and AST structure — "no type computation", as the module
doc states. The emitter consumes these (see [emitter](emitter.md)).

---

## 13. Caches and invariants

The binder maintains four regenerable resolution caches, all
`CloneableRwLock<...>` and all `#[serde(skip)]` (regenerated lazily on first
access after a snapshot load):

| Cache | Key -> value | Purpose |
| --- | --- | --- |
| `resolved_export_cache` | `(module_spec, export_name) -> Option<SymbolId>` | barrel/re-export chain memo |
| `resolved_export_type_only_cache` | `(module_spec, export_name) -> Option<(SymbolId, bool)>` | type-only path, records `export type *` crossing |
| `resolved_identifier_cache` | `(arena_ptr, node) -> Option<SymbolId>` | hot scope-walk memo |
| `find_enclosing_scope_cache` | `(arena_ptr, node) -> ScopeId` | deep-nesting parent-walk memo |

`resolution_cache_statistics()` reports entry counts;
`clear_resolution_caches{,_shared}()` wipes all four. **Invalidation rule**: any
operation that changes `SymbolId` assignments — `bind_source_file`,
`merge_lib_contexts_into_binder`, `reset` — clears these caches first, so callers
never receive a stale ID. The two `_disabled()` env kill-switches
(`TSZ_DISABLE_REEXPORT_TYPE_ONLY_CACHE`, `TSZ_DISABLE_ENCLOSING_SCOPE_CACHE`)
exist to prove the caches are pure (byte-identical diagnostics with them off).

Structural invariants worth stating:

- `stable_declarations.len() == declarations.len()` and
  `stable_value_declaration` parallel to `value_declaration`, maintained by
  `add_declaration`/`set_value_declaration`.
- `SemanticDefEntry.type_param_names.len() == type_param_count as usize`.
- Most of `BinderState` is `Arc`-shared with `Arc::make_mut` mutation, so the
  per-file binders the CLI driver builds (cross-file lookup + per-file checking,
  ~2N for N files) share allocations and pay copy-on-write only on genuine
  divergence. During a single file's bind the refcount is 1, so `make_mut` is
  free.
- The `node` field of an empty merge `FlowNode` is `NodeIndex::NONE`;
  `add_antecedent` never wires `unreachable_flow` or `NONE` as an antecedent.

---

## 14. A worked example

```ts
var greet = sayHi();              // 1: var hoisted; ref to sayHi before decl OK
function sayHi() { return "hi"; } // 2: function hoisted
export class Greeter {            // 3: exported class
  msg = greet;
}
```

What runs:

1. `bind_source_file` enters the root `SourceFile` scope (`ScopeId(0)`),
   allocates the `START` flow node, and sets `is_external_module = true` (the file
   has `export`).
2. **PASS 1a** `collect_hoisted_declarations`: `greet` is pushed to
   `hoisted_vars`; `sayHi` is pushed to `hoisted_functions` (it is at the file
   scope, not block-nested, so it hoists even in module mode).
3. **PASS 1b** `process_hoisted_functions`: `declare_symbol("sayHi", FUNCTION,
   …)` allocates `SymbolId` for `sayHi`, records its `SemanticDefEntry`
   (`SemanticDefKind::Function`), and exposes it in the root scope.
4. **PASS 1c** `process_hoisted_vars`: `declare_symbol("greet",
   FUNCTION_SCOPED_VARIABLE, …)` allocates `greet`'s symbol and exposes it in the
   root scope. Now both names exist before any statement is bound.
5. **PASS 2** binds statement 1: `bind_variable_declaration` calls `record_flow`,
   then `bind_node` on the initializer `sayHi()`. The hoisted-`var` reuse branch
   in `declare_symbol` finds `greet` already in `node_symbols` and reuses it.
   Binding the call creates a `CALL` flow node; the identifier `sayHi`
   `record_flow`s the active flow. The checker can later
   `resolve_identifier(sayHi)` and find the hoisted symbol.
6. Binding statement 2 (`function sayHi`): the hoisted-function symbol is reused;
   `bind_function_declaration` enters a `ContainerKind::Function` scope, declares
   the synthetic `arguments` symbol, and binds the body under `with_fresh_flow`
   (a new `START`), then exits.
7. Binding statement 3 (`export class Greeter`): `bind_class_declaration` calls
   `declare_symbol("Greeter", CLASS, idx, is_exported=true)`, records a
   `SemanticDefKind::Class` entry (with `is_exported`, empty heritage), enters a
   `ContainerKind::Class` scope sized to the member count, binds member `msg`
   (a `PROPERTY` symbol whose initializer references `greet`), then `exit_scope`
   captures `{msg}` into `Greeter`'s `members` table.
8. After the walk, `file_locals` is set from the root scope table (so `greet`,
   `sayHi`, `Greeter` are file-visible), `populate_module_exports_from_file_symbols`
   records `Greeter` in `module_exports`, and `stamp_file_idx` finalizes the
   stable locations and `decl_file_idx` if the driver assigned a file index.

At no point did the binder compute that `sayHi`'s return type is `string` or that
`msg` is `string` — those are checker/solver answers. The binder produced only the
symbols, scopes, hoisting, flow nodes, and export wiring.

---

## 15. Edge cases and tsc parity

- **Annex B function hoisting**: block-nested function declarations hoist in
  non-strict, non-module ES3/ES5 scripts but are block-scoped under
  module/strict/ES2015+ (`collect_hoisted_declarations_impl`).
- **`arguments` cross-function isolation**: a real `arguments` declaration never
  merges into another function's synthetic `arguments`, avoiding spurious TS2403.
- **Lib value shadowing without losing the type namespace**: a user
  `const Array = 1` shadows lib `var Array` but `collect_preserved_lib_meaning`
  keeps lib `interface Array<T>` reachable so `let xs: Array<number>` does not
  emit a spurious TS2749; conversely `interface Symbol {}` (TYPE-only) keeps lib's
  VALUE-bearing `var Symbol` visible.
- **Module-augmentation isolation (issue #6164)**: same-name augmentation
  declarations across `declare module "X"` blocks merge with each other but never
  with a file-scope non-augmentation symbol; the boundary table is snapshotted and
  restored so the augmentation body never leaks into file scope.
- **`declare global` restore**: the global-augmentation body binds in place and
  restores the boundary table so it cannot displace the lib symbol it augments.
- **Forward-referenced exports**: `export = X` and `export { X }` appearing before
  their declarations are repaired by the deferred-resolution passes after the main
  walk.
- **Unreachable-after-`return` parity**: `finish_flow_label` collapses
  antecedent-less merge labels to `unreachable_flow` (matching tsc's
  `finishFlowLabel`), so an `if` whose arms both `return` correctly marks the
  following code unreachable and preserves later narrowing.
- **External-module global isolation (issue #12372)**: a module's top-level value
  exports do not seed the cross-file global scope, so a bare `Symbol` resolves to
  the global `SymbolConstructor`, not a transitively-installed package's exported
  `Symbol` — governed by `Symbol::is_cross_file_global`.
- **Stack-guard `None` headroom**: an unmeasurable stack (wasm32) is treated as
  "not low" so the binder never aborts mid-file and drops later declarations
  (issue #13815).

---

## See also

- [front-end-scanner-parser](front-end-scanner-parser.md) — the AST and arenas
  the binder walks.
- [checker-context-and-state](checker-context-and-state.md) — how the checker
  consumes `BinderState`.
- [checker-flow-and-narrowing](checker-flow-and-narrowing.md) /
  [solver-narrowing](solver-narrowing.md) — narrowing over the flow skeleton.
- [checker-declarations-modules](checker-declarations-modules.md) — module/
  augmentation type resolution on top of the binder's tables.
- [solver-types-intern-def](solver-types-intern-def.md) — where `SemanticDefEntry`
  becomes a solver `DefId`.
- [emitter](emitter.md) — consumer of `ExportSurface`/`DeclarationSummary`.
- [end-to-end-timeline](end-to-end-timeline.md) — where binding sits in the
  pipeline.
