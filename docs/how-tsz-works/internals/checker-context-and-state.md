# Checker Context, Session State, and the Whole-File Checking Lifecycle

This chapter explains how the tsz checker is *wired together*: the giant
`CheckerContext` state object, the thin `CheckerState` orchestration shell, the
two `TypeEnvironment` instances, the per-file checking lifecycle (
`build_type_environment` -> `prepare_source_file_for_checking` ->
`check_source_file`), how the driver builds and feeds one checker per file, how
diagnostics are queued and deduplicated, and the `DefId` stabilization plus the
per-`TypeId` `env_eval_cache` invalidation discipline that keeps cached
evaluations consistent.

The checker's job is **orchestration**: walk the AST, build source context,
record diagnostics with source locations, drive the flow graph, and *ask the
solver for semantic answers*. It is not allowed to run relation/inference/
instantiation kernels itself, pattern-match raw `TypeData`/`TypeKey`, intern
types directly, or read printer output as a predicate. The structures described
here are the plumbing that makes those queries possible while staying inside
that boundary. For the algorithms behind the answers, see
[solver-relations](solver-relations.md), [solver-inference](solver-inference.md),
[solver-instantiation](solver-instantiation.md),
[solver-evaluation](solver-evaluation.md), and
[solver-types-intern-def](solver-types-intern-def.md).

## Owns / Must not own

| Owns | Must not own |
| --- | --- |
| `CheckerContext`: per-file/session caches, diagnostic queue, both `TypeEnvironment`s, `DefId <-> SymbolId` maps, fuel counters, recursion guards | Relation/inference/instantiation/evaluation kernels (those live in `tsz-solver`) |
| `CheckerState`: AST walk order, statement/declaration dispatch, source spans | Raw `TypeKey` construction or `TypeData` shape matching (route through `query_boundaries`) |
| `DefId` stabilization (`get_or_create_def_id`) and dual-env registration (`register_*_in_envs`) | The authoritative `DefId -> TypeId` resolution algorithm — that is `TypeEnvironment::resolve_lazy` / `DefinitionStore` |
| Diagnostic dedup, queuing, and source-file post-processing | Deciding *which* relation failed and *why* (`query_boundaries/assignability`, see [checker-assignability-gateway](checker-assignability-gateway.md)) |
| Per-file fuel reset and cross-file cache invalidation policy | Type interning and structural identity (`TypeInterner` / `QueryDatabase` in the solver) |

## Where the code lives

| Path | Role |
| --- | --- |
| `crates/tsz-checker/src/lib.rs` | Crate root: module wiring, domain re-exports (`state`, `dispatch`, `flow_*`, `query_boundaries`), `diagnostics` re-export gateway |
| `crates/tsz-checker/src/context/mod.rs` | Definitions of `CheckerContext<'a>` and the emit-side `TypeCache`; field inventory (~2000 lines of fields) |
| `crates/tsz-checker/src/context/core.rs` | `impl` blocks for `CheckerContext`/`TypeCache`: symbol-file resolution, invalidation |
| `crates/tsz-checker/src/context/constructors.rs` | `CheckerContext::new`, `new_with_shared_def_store`, `with_cache` |
| `crates/tsz-checker/src/context/typing_request.rs` | `TypingRequest`, `FlowIntent`, `ContextualOrigin` |
| `crates/tsz-checker/src/context/def_mapping.rs` | `get_or_create_def_id`, `register_*_in_envs` dual-env registration |
| `crates/tsz-checker/src/context/env_eval_cache.rs` | `env_eval_cache` invalidation: `cache_env_eval_result`, `clear_type_evaluation_caches_for_def`, `invalidate_env_eval_reachable_from` |
| `crates/tsz-checker/src/context/resolver.rs` | `impl TypeResolver for CheckerContext` — the sanctioned solver callback adapter |
| `crates/tsz-checker/src/context/diagnostic_push.rs` | `error`, `push_diagnostic`, dedup keys |
| `crates/tsz-checker/src/context/program_context.rs` | `ProgramContext` and `apply_to` — program-stable shared state installed per file |
| `crates/tsz-checker/src/context/speculation.rs` | Snapshot/rollback transactions for overload/inference probes |
| `crates/tsz-checker/src/context/caches.rs` | `CowCache`, `NodeTypeCache`, `SymbolTypeCache`, the assignability memos |
| `crates/tsz-checker/src/context/file_session_reset.rs` | `reset_for_next_file` — the file-session reset boundary |
| `crates/tsz-checker/src/context/lifetime_shells.rs` | Lifetime-class shell types (`WorkerContext`, `FileSession`, `SpeculationScope`, `LspPersistentCache`) |
| `crates/tsz-checker/src/state/state.rs` | `CheckerState<'a>` orchestration shell and constructors |
| `crates/tsz-checker/src/state/state_checking/source_file.rs` | `prepare_source_file_for_checking`, `check_source_file` — the per-file lifecycle |
| `crates/tsz-checker/src/state/type_environment/core.rs` | `build_type_environment` |
| `crates/tsz-checker/src/state/cache_invalidation.rs` | Cross-file/session cache invalidation logic |
| `crates/tsz-solver/src/def/resolver.rs` | `TypeEnvironment` struct, `TypeResolver` trait, `resolve_lazy` |

## The two layers: `CheckerState` and `CheckerContext`

The public checker entry type is `CheckerState<'a>`
(`crates/tsz-checker/src/state/state.rs`). It is intentionally thin — it is one
field:

```rust
pub struct CheckerState<'a> {
    pub ctx: CheckerContext<'a>,
}
```

All AST-walking methods (`check_source_file`, `check_statement`,
`build_type_environment`, ...) are `impl CheckerState`, while all *state* (every
cache, both envs, fuel counters, diagnostics) lives on `CheckerContext<'a>`.
This split lets specialized checkers borrow the context mutably while the walk
methods stay on the shell. The lib.rs doc comment names it precisely: *"the thin
checker is the unified checker pipeline; `CheckerState` is an alias to the thin
checker."*

`CheckerContext<'a>` borrows the immutable inputs by reference and owns
everything mutable:

| Field | Type | Meaning |
| --- | --- | --- |
| `arena` | `&'a NodeArena` | The parser's syntax-only AST (see [front-end-scanner-parser](front-end-scanner-parser.md)) |
| `binder` | `&'a BinderState` | Symbols, scopes, flow skeleton (see [binder](binder.md)) |
| `types` | `&'a dyn QueryDatabase` | The solver's interner + memoized query cache |
| `compiler_options` | `CheckerOptions` | Effective compiler flags |
| `capabilities` | `EnvironmentCapabilities` | Precomputed lib/config/feature matrix for diagnostic routing |
| `type_env` | `RefCell<TypeEnvironment>` | Evaluator env — authoritative `DefId -> TypeId` |
| `type_environment` | `RefCell<TypeEnvironment>` | Flow-analyzer env (a deliberate second `RefCell`) |
| `definition_store` | `Arc<DefinitionStore>` | Shared `DefId` namespace + bodies |
| `symbol_to_def` / `def_to_symbol` | `RefCell<FxHashMap<...>>` | Local `DefId` <-> `SymbolId` maps |
| `node_types` | `NodeTypeCache` | Dense flat-vec node -> `TypeId` cache |
| `symbol_types` / `symbol_instance_types` | `SymbolTypeCache` | Dense symbol -> `TypeId` caches |
| `env_eval_cache` | `RefCell<FxHashMap<TypeId, EnvEvalCacheEntry>>` | Memoized `evaluate_type_with_env` results |
| `diagnostics` | `Vec<Diagnostic>` | The output queue for this file |
| `eval_session` | `Rc<EvaluationSession>` | Solver-side instantiation fuel/depth, shared by `Rc` |
| `type_resolution_fuel` | `Cell<u32>` | Per-file resolution-op budget |
| `current_file_idx` | `usize` | Index of the file currently being checked |

`CheckerContext` implements `TypeResolver` (the solver callback trait), so the
solver can call *back* into the checker to resolve a `TypeData::Lazy(DefId)`
during evaluation. That adapter is the single sanctioned point of solver-API
contact in `crates/tsz-checker/src/context/resolver.rs`.

```
 driver (CLI/LSP)                  CheckerState (thin shell)
      │                                   │ ctx
      │ CheckerState::new(...)            ▼
      ├──────────────────────────►  CheckerContext<'a>  ──implements──► TypeResolver
      │                              ├─ &arena  &binder  &types(QueryDatabase)
      │ check_source_file(root)      ├─ type_env / type_environment (2× TypeEnvironment)
      ├──────────────────────────►   ├─ definition_store: Arc<DefinitionStore>
      │                              ├─ caches: node_types, symbol_types, env_eval_cache, ...
      │ take(ctx.diagnostics)        └─ diagnostics: Vec<Diagnostic>
      ◄──────────────────────────         │
                                          │ resolve_lazy(DefId) ──────► solver evaluation
                                          ◄────────────────────────────  asks back for bodies
```

## Compiler options and capabilities plumbing

`compiler_options: CheckerOptions` (re-exported from
`tsz_common::checker_options`) carries the effective flags. The driver computes
them once and threads them into `CheckerState::new(arena, binder, types,
file_name, compiler_options)`. A few per-file booleans are derived from them and
hoisted onto the context so hot checks do not re-derive policy:
`no_implicit_override`, `report_unresolved_imports`, `file_is_esm`, and the
`EnvironmentCapabilities` matrix (`capabilities`), which centralizes lib/config
queries for diagnostic routing.

Conformance fixtures embed pragmas like `// @strict: false`. Those are *not*
user-facing directives, so the context guards them behind
`allow_source_file_test_pragmas` (default `false`). Only when
`CheckerState::enable_source_file_test_pragmas` was called does
`prepare_source_file_for_checking` invoke `resolve_compiler_options_from_source`
to honor the pragma. Normal CLI/LSP/project checking leaves options unchanged —
this is the anti-hardcoding guarantee that source text cannot mutate real
compiler options.

## The two `TypeEnvironment`s and `DefId` resolution

The semantic source of truth for `DefId -> TypeId` is `TypeEnvironment`, defined
in `crates/tsz-solver/src/def/resolver.rs`. The context holds **two** of them in
separate `RefCell`s, and this is deliberate:

- `type_env` is the **evaluator** env — the authoritative one. The
  evaluator/state/`types`/assignability paths resolve symbols and expand
  `Application` types through it.
- `type_environment` is the **flow-analyzer** env. `FlowAnalyzer::from_ctx`
  borrows it (via `with_type_environment`) and holds that borrow live while
  narrowing reads types. It also carries the legacy `SymbolRef`-keyed entries
  the evaluator env never uses.

Why two cells rather than one? While the flow analyzer holds
`type_environment` borrowed, the evaluator must still be able to
`try_borrow_mut` `type_env` to publish freshly resolved `DefId -> TypeId`
bodies. A single `RefCell` would make that mutable borrow fail and silently drop
writes (the field doc on `type_environment` spells this out).

`TypeEnvironment` is a bundle of `FxHashMap`s keyed by the raw `u32` inside a
`DefId` / `SymbolRef`:

| Map | Purpose |
| --- | --- |
| `def_types` | `DefId -> TypeId` resolved body (type-position) |
| `def_type_params` | `DefId -> Vec<TypeParamInfo>` for generic aliases |
| `class_instance_types` | class `DefId -> instance TypeId` (so `resolve_lazy` returns the *instance* in type position) |
| `def_kinds` | `DefId -> DefKind` (Interface/Class/Enum/`TypeAlias`) |
| `symbol_to_def` / `def_to_symbol` | bridge to `InheritanceGraph` and `Ref -> Lazy` migration |
| `typeof_value_types` | merged interface+value symbol -> VALUE-space type for `typeof X` |
| `definition_store` | `Option<Arc<DefinitionStore>>` fallback when a local map is incomplete |
| `generation` | monotonic revision; `generation()` adds the shared store's generation |

`impl TypeResolver for TypeEnvironment` is the resolution kernel. `resolve_lazy`
returns the class instance type when present, then `get_def`, then a
`raw_symbol_fallback_def` redirect for "zombie" `Lazy(DefId(N))` whose numeric
value is actually a raw `SymbolId`. `resolve_type_query` instead returns the
VALUE-space type (constructor for classes, the `var`'s type for merged
interface+value symbols). The `generation` counter feeds
`resolver_generation()`, which narrowing and relation caches embed so a later
resolve returning a different type invalidates dependent cached verdicts.

### `DefId` stabilization

Semantic references are `TypeData::Lazy(DefId)`. The checker stabilizes the
`DefId`; the `TypeEnvironment` resolves `DefId -> TypeId`. `DefId` allocation is
`CheckerContext::get_or_create_def_id` (`def_mapping.rs`), which uses a layered
lookup so the *same* raw `SymbolId(u32)` across different binders never collapses
to one `DefId`:

1. **Local cache fast path** (`symbol_to_def: RefCell<FxHashMap<SymbolId,
   DefId>>`): O(1), no locking. The hit is *validated* against the authoritative
   file index and name to reject a stale or cross-file-colliding cache entry.
2. **Authoritative index** (`DefinitionStore::lookup_by_symbol(sym_id.0,
   file_idx)`): O(1) `DashMap` keyed by `(symbol_id, file_idx)`, which
   disambiguates the same raw `SymbolId` across binders.
3. **Create**: build `DefinitionInfo`, register in both the store and the local
   index.

Because raw `SymbolId` values are reused across binders, the file index is
load-bearing. `resolve_symbol_file_index_stable` (`context/core.rs`) prefers the
order-independent *declaring* file (`global_symbol_file_index`, immutable) and
only falls back to the monotonically-growing dynamic overlay
(`cross_file_symbol_targets`) for symbols with no statically-known declaring
file. This is what keeps cross-file alias / `export =` resolution
order-independent (refs #7574, #12148), with
`TSZ_DISABLE_ORDER_INDEP_RESOLUTION=1` as the A/B kill-switch.

### Dual-env registration

Once a body is resolved, the checker must publish it to *both* envs and the
shared store atomically. That is the `register_*_in_envs` family in
`def_mapping.rs`:

- `register_def_in_envs(def_id, body)` — non-generic body.
- `register_def_with_params_in_envs(def_id, body, params)` — generic, body and
  params published to the `DefinitionStore` in **one** write so a concurrent
  reader never sees a body without its params.
- `register_class_instance_in_envs`, `register_class_extends_in_envs`,
  `register_def_kind_in_envs`, `register_augmented_def_in_envs`, etc.

Each routes through `register_in_envs(DeferredFlowEnvWrite::...)`. The write hits
`type_env` directly and *mirrors* into `type_environment`. If the mirror loses
the `RefCell` borrow race (because the flow-analyzer env is already borrowed
during recursive resolution), the operation is **reified** into
`deferred_flow_env_writes` and replayed later rather than dropped. The
symmetric `deferred_eval_env_writes` queue does the same for the authoritative
`type_env` side — a dropped write there used to collapse a class-instance / def
body to `never` for every later consumer (the xstate `Actor` `this` collapse).
Both queues are flushed at file-preparation time. `debug_assert_eq!` guards
enforce that the evaluator and flow queues are empty before statements run, and
the reconcile step asserts that the flow env is not missing evaluator-env
entries after deferred replay.

## The per-file checking lifecycle

`CheckerState::check_source_file(root_idx)` is the per-file entry point
(`state_checking/source_file.rs`). It runs in two phases: a setup phase
(`prepare_source_file_for_checking`) and the statement-checking phase.

```
check_source_file(root_idx)
  │
  ├─ begin_file_inference_placeholders()        deterministic __infer_* witness names
  ├─ prepare_source_file_for_checking(root_idx) ── Option<NodeIndex>
  │     ├─ resolve_compiler_options_from_source  (only under test pragmas)
  │     ├─ has_ts_nocheck_pragma? → bail (None)
  │     ├─ clear application_symbols_resolved / _resolution_set
  │     ├─ reset_global_resolution_fuel()
  │     ├─ register_function_def_ids_early()
  │     ├─ warm_local_caches_from_shared_store()  OR  pre_populate_def_ids_from_binder / _lib_binders
  │     ├─ resolve_cross_batch_heritage()         (unless store fully populated)
  │     ├─ build_type_environment()               ◄── the big one
  │     ├─ ensure_both_envs_have_definition_store()
  │     ├─ flush_deferred_eval_env_writes()
  │     ├─ flush_deferred_flow_env_writes(); reconcile_flow_and_evaluator_envs()
  │     ├─ register_boxed_types()                 (String/Number/Boolean from lib)
  │     ├─ reset: type_resolution_fuel, eval_session instantiation fuel, depth_exceeded,
  │     │        global resolution fuel, stack-overflow flag, solver stack frames
  │     └─ is_checking_statements = true
  │
  ├─ publish_heritage_interface_bodies(statements)   (non-.d.ts only)
  ├─ for stmt in sf.statements.nodes:  check_statement(stmt)   ◄── main walk
  ├─ recheck_deferred_implicit_any_closures()
  ├─ check_isolated_declarations / function impls / export assignment / ...
  ├─ check_cross_file_circular_type_aliases()        (post-statement, cross-file)
  ├─ check_duplicate_identifiers / unused declarations / triple-slash / ...
  └─ (diagnostics now sit in ctx.diagnostics, ready for the driver to drain)
```

### `build_type_environment`

`build_type_environment` (`type_environment/core.rs`) resets the session's
instantiation fuel and the interner's evaluation fuel (matching tsc, which
resets `instantiationCount` per checked source element), collects the file's
**user** symbols only (`binder.node_symbols`) — lib symbols are resolved lazily
on demand to avoid the O(N) cost of eagerly materializing ~2000 lib symbols per
file — sorts them so type-defining symbols (functions, classes, interfaces, type
aliases, enums, modules) are processed before variables/parameters/properties,
then drives each through `get_type_of_symbol -> compute_type_of_symbol ->
register_def_in_envs`. When a shared `DefinitionStore` already holds a resolved
body for a symbol (computed by a parallel file checker), the loop **seeds**
`symbol_types` and the env from the store instead of recomputing — the
cross-file reuse that makes ts-toolbelt-style fan-out tractable.

### The statement walk and `TypingRequest`

The statement loop calls `check_statement(stmt_idx)` per top-level statement.
Inside, expression typing is request-driven. A `TypingRequest`
(`context/typing_request.rs`) is the explicit replacement for what used to be
ambient mutable globals on the context:

```rust
pub struct TypingRequest {
    pub contextual_type: Option<TypeId>,   // expected type
    pub origin: ContextualOrigin,          // Normal vs Assertion
    pub flow: FlowIntent,                  // Read vs Write (skip narrowing)
}
```

- `FlowIntent::Write` means "this is an assignment target; use the declared
  (pre-narrowed) type" — e.g. `foo[x] = 1` after `if (foo[x] === undefined)`
  needs `number | undefined`, not `undefined`.
- `ContextualOrigin::Assertion` (`as T`, `<T>expr`, JSDoc `@type`) suppresses
  checking the function body's return type against the contextual type; only
  TS2352 fires at the assertion site.

Callers build a request (`TypingRequest::with_contextual_type(expected)`,
`for_assertion`, `for_write_context`, or the builder chain) and pass it to the
request-first entry points instead of save/restoring fields. The architecture
contract tests in `tests/architecture_contract_tests.rs` enforce that migrated
files no longer assign `ctx.contextual_type`,
`ctx.contextual_type_is_assertion`, or `ctx.skip_flow_narrowing` directly. See
[checker-flow-and-narrowing](checker-flow-and-narrowing.md) for how `FlowIntent`
interacts with narrowing.

## Caches and invariants

The context carries a large family of caches. They fall into ownership classes,
documented field-by-field in
`docs/architecture/CHECKER_CONTEXT_CACHE_OWNERSHIP.md` and tracked by the
manifest at `context/checker_context_lifetimes.toml`. The lifetime-class shells
in `context/lifetime_shells.rs` (`WorkerContext`, `FileSession`,
`SpeculationScope`, `LspPersistentCache`) name the eventual destinations; they
are intentionally empty today, holding the architecture target while fields are
migrated incrementally.

### Identity handles and dense caches

The canonical identity handles are `TypeId(u32)`, `SymbolId(u32)`,
`FlowNodeId(u32)`, and `Atom(u32)` — all O(1) to compare. The hottest caches
exploit this with dense flat-vec storage rather than `FxHashMap`:

- `node_types: NodeTypeCache` — node `NodeIndex -> TypeId`, O(1) by node index.
- `symbol_types` / `symbol_instance_types: SymbolTypeCache` — symbol -> `TypeId`,
  O(1) by symbol index. `symbol_types` holds *constructor*/value-position types;
  `symbol_instance_types` holds class *instance* types.
- `narrowable_identifier_cache` — a dense 1-byte-per-node cache; pure over AST
  structure, so it never needs invalidation.

### `CowCache`: O(1) snapshots

Many caches are wrapped in `CowCache<T>` (`context/caches.rs`), an O(1)-cloneable
copy-on-write wrapper (`Arc<T>` + `Arc::make_mut` on first write). Speculation
snapshots and child-checker construction used to deep-clone whole maps; `CowCache`
makes the clone an `Arc` bump, and the deep copy is paid at most once per
diverging holder — never for snapshots dropped or rolled back without writes.
Isolation semantics are identical to a deep clone because every writer goes
through `Arc::make_mut`.

### The `env_eval_cache` and its invalidation

`env_eval_cache: RefCell<FxHashMap<TypeId, EnvEvalCacheEntry>>` memoizes
`evaluate_type_with_env` results (recursive mapped/conditional/`Application`
expansion), keyed by `TypeId`. Each entry stores the result plus a
`depth_exceeded` bit so a follow-up validation pass can still surface TS2589 from
a cache hit. The cache helpers live in `context/env_eval_cache.rs`:

| Helper | Effect |
| --- | --- |
| `cache_env_eval_result(ty, result, depth_exceeded)` | top-level result memo (the only correctness-relevant cache) |
| `lookup_env_eval_cache(ty)` | O(1) read |
| `invalidate_env_eval_for(ty)` | drop exactly one entry (minimal, no scan) |
| `invalidate_env_eval_reachable_from(ty)` | drop the entry for `ty` plus every entry whose key **or** result is a structural sub-term of `ty` |
| `clear_type_evaluation_caches_for_def(def_id)` | drop every entry whose key or result *mentions* `def_id`, and the matching narrowing `resolve_cache` / `contextual_resolve_cache` entries |
| `clear_env_eval_cache()` | global flush |

The intermediate seed/persist path (`env_eval_cache_seed_entries` /
`persist_env_eval_cache_entries`) is a **speed-only** memo that pre-populates a
fresh evaluator's per-run cache. Because each `evaluate_type_with_env_impl` call
re-marshals the whole growing cache (`O(cache_size)` per call, `O(N^2)` across a
file), it is gated by a soft cap (`ENV_EVAL_SEED_PERSIST_SOFT_CAP = 256`) with
the `TSZ_DISABLE_ENV_EVAL_SEED_CAP` kill-switch to prove byte-identical
diagnostics with the cap on vs. off. Persistence is also skipped for declaration
files (react16.d.ts-class graphs generate huge transient volumes), and guards
against union->non-union "cache poisoning" where an `Application` whose `DefId`
was not yet resolved would bypass union-member checking.

#### Targeted per-`TypeId` env-eval invalidation (#13991)

The general invalidation discipline keeps the env-eval cache consistent without
the `O(N^2)` sweeps that a naive flush-on-every-write would cause. When a
definition body is **registered**, `register_def_in_envs` /
`register_def_with_params_in_envs` decide whether to sweep:

- **First publication (`None -> Some`)**: no sweep. The solver refuses to
  persist application/closed-eval results computed while a def had no resolvable
  body (`mark_unresolved_def_seen`), so no cached entry can reference the def
  yet. Sweeping on every first registration would be `O(env_eval_cache)` per def,
  `O(N^2)` across a file of `N` aliases.
- **Every rewrite (`Some(old) -> Some(new)`)**, including an `A -> B -> A`
  re-publication, or any params change: invalidate through
  `clear_type_evaluation_caches_for_def`. Entries may have been populated while
  the intervening body was active, so a previously published body is not a safe
  shortcut. Env-eval entries are removed through the reverse `DefId` dependency
  index; the narrowing-cache structural scans remain gated on real rewrites.
  The cheap def-keyed `invalidate_application_eval_cache_for_def` also runs on
  body or parameter changes.

The `invalidate_env_eval_reachable_from(type_id)` variant exists because
re-evaluating a type under a different resolution mode (e.g. after a speculative
bounded verdict) is not enough to invalidate just its top-level result: the
first pass also cached results for the sub-terms it walked. It collects the
reachable set once (`collect_referenced_types`, which includes the root) and
drops every cache entry whose key or result lands in that set — `O(reachable) +
O(cache)` rather than re-walking per entry.

### Structural-walk memos

`lazy_def_ids_cache` and `type_queries_cache` (both
`RefCell<FxHashMap<TypeId, Rc<[...]>>>`) memoize the pure structural walks
`collect_lazy_def_ids` and `collect_type_queries` over the *immutable* interned
type structure. They are pure speed memos — identical to recomputing on demand —
returning `Rc` slices for cheap clone-on-hit. `type_position_resolution_cache`
(keyed by `(arena pointer, node index)`) memoizes context-free type-position
identifier resolution but deliberately **never** caches the enclosing-type-
parameter fast path, because the same lexical node can bind to different type-
parameter symbols across instantiation/return contexts.

### Fuel and recursion guards

| Guard | Type | Bounds |
| --- | --- | --- |
| `type_resolution_fuel` | `Cell<u32>` | per-file resolution-op budget; reset to `MAX_TYPE_RESOLUTION_OPS` after setup |
| `eval_session` | `Rc<EvaluationSession>` | solver-side instantiation fuel/depth (shared by `Rc`) |
| `depth_exceeded` | `Cell<bool>` | sticky flag for TS2589 surfacing |
| `application_eval_set` / `mapped_eval_set` | `FxHashSet<TypeId>` | recursion guards for application / mapped-type evaluation |
| `type_resolution_visiting` | `FxHashSet<CanonicalAppKey>` | guards `evaluate_type_with_resolution`; keyed by `CanonicalAppKey` (not raw `TypeId`) to collapse import-alias variants |
| `MAX_SYMBOL_RESOLUTION_DEPTH` | `const u32 = 50` | nested `get_type_of_symbol` depth cap (matches `MAX_INSTANTIATION_DEPTH`) |
| `CROSS_ARENA_DEPTH` / `CROSS_ARENA_BAILOUT_EPOCH` | thread-locals | cross-arena delegation depth cap and bailout epoch (`state.rs`) |

Setup work (type-environment prewarming) can spend the per-file budget or trip
the stack breaker while probing large lib-facing types; that is why
`prepare_source_file_for_checking` *re-resets* `type_resolution_fuel`,
instantiation fuel, `depth_exceeded`, the global resolution fuel, the
stack-overflow flag, and the solver's RAII-balanced cross-operation frame
breaker *after* setup, so user-visible diagnostics in the statement pass start
from a clean budget. The `CROSS_ARENA_BAILOUT_EPOCH` counter (`state.rs`) lets a
delegating resolution detect that a depth-cap bailout occurred in its subtree
and refuse to persist the transiently-incomplete result as authoritative — the
checker mirror of the solver's `unresolved_def_seen` discipline.

### File-session reset boundary

`reset_for_next_file` (`context/file_session_reset.rs`) is the API for reusing
one `CheckerContext` across files in a sequential session-reuse path. It clears
the high cross-file-leak-risk fields: diagnostic buffers and indices (file-local
positions), `NodeIndex`-keyed caches (`request_node_types`,
`class_instance_type_cache`, `class_constructor_type_cache` — raw `NodeIndex`
collides across files), resolution stacks, implicit-any closure sets,
class-checking sets, no-overload call nodes, and the depth counters. It
deliberately *retains* `SymbolId`-keyed caches and `Atom`/string-keyed lib
caches, whose stable identity makes them correct to keep. The current default
driver path constructs a fresh checker per file rather than calling this; the
helper is the boundary API for the future reuse path.

## Diagnostics: queue, dedup, and source locations

The checker is the owner of source locations and diagnostics. Every error goes
through `CheckerContext::error(start, length, message, code)`
(`context/diagnostic_push.rs`), which:

1. Calls `reconcile_name_resolution_precedence` to suppress a lower-precedence
   name-resolution diagnostic when a higher one already won at that span.
2. Computes a dedup key — normally `(start, code)`, but a documented set of codes
   (TS2374, TS2411, TS2413, TS2416, TS2430, TS2536/7/8, TS4094, ...) use
   `(start ^ message_hash, code)` so several genuinely-distinct messages can
   coexist at one span (e.g. one property failing against both a string and a
   number index signature).
3. Skips if `diagnostic_indices.emitted` already contains the key; otherwise
   inserts it, updates auxiliary indices, and pushes a `Diagnostic::error` onto
   `ctx.diagnostics` with the file name attached.

`push_diagnostic` is the same path for pre-built diagnostics with the same dedup
discipline and a related-information tiebreaker for key collisions. The
`diagnostics_discarded` flag marks transient cross-arena delegation child
checkers whose diagnostics are never surfaced; when set, the expensive
*presentation* work (`explain_failure`, type formatting, related-info chains) is
skipped, but the code and span are still recorded so internal counting/dedup
predicates keep working. Crucially, that flag *never* changes which checks run or
what types are computed.

Some diagnostics must survive speculative rollback (they were discovered inside a
speculative call-checker context that truncates diagnostics on rollback). Those
are buffered on the context — `deferred_ts2454_errors`,
`deferred_jsx_import_source_error` — and flushed at the end of
`check_source_file`. For the structured-reason -> diagnostic pipeline behind
assignability errors, see
[checker-assignability-gateway](checker-assignability-gateway.md) and
[checker-error-reporter-diagnostics](checker-error-reporter-diagnostics.md).

## Speculation: snapshot / rollback

Overload resolution, return-type inference, and contextual-typing probes must not
leak committed checker state. `context/speculation.rs` provides the transaction
boundary. The snapshot/holder types use **explicit-action** semantics, *not*
RAII: `Drop` is intentionally not implemented (the `CheckerContext` is not
reachable from `Drop`, and many sites legitimately want to keep speculative
output). The convention is encoded in the type names — `DiagnosticSnapshot`,
`FullSnapshot`, `CacheSnapshot` — and readers must call `rollback()` /
`rollback_filtered()` themselves when discarding. `CowCache` makes these
snapshots cheap: a snapshot is an `Arc` bump, and the copy is paid only if the
speculative branch actually writes. This module is pure checker orchestration —
it manages diagnostic/cache state, not type algorithms; the solver is not
involved.

## How the driver drives the program

The CLI driver (`crates/tsz-cli/src/driver/check_file.rs`) owns the program-level
loop. Module resolution, binding, and the shared `DefinitionStore` are built
once; then each file is checked. The program-stable state is carried by a
`ProgramContext` (`context/program_context.rs`) and installed per checker via
`ProgramContext::apply_to(&mut ctx)`, which `Arc`-clones the shared lib contexts,
all arenas, all binders, the global symbol/file/module indices, the shared
`DefinitionStore`, and the module-specifier display maps. `apply_to` is the
expensive per-file setup that the experimental pooled/reuse paths amortize.

A simplified per-file flow inside the parallel checker:

```
for each file_idx:
  binder        = bound program for file_idx
  checker       = CheckerState::with_options_deferred_def_store(arena, binder, types, name, options)
  program_context.apply_to(&mut checker.ctx)          // install Arc-shared program state
  configure_checker_per_file(...)                     // per-file flags: file_is_esm, report_unresolved_imports, ...
  if !no_check || (no_check && emit_declarations):
      reset_stack_overflow_flag()
      checker.check_source_file(file.source_file)      // ◄── the lifecycle above
      diags = take(checker.ctx.diagnostics)            // drain
      post_process_checker_diagnostics(diags, ...)     // JS filtering, syntax-error suppression, dedup
  type_cache = extract_type_cache.then(|| checker.extract_cache())   // for emit / incremental
```

The `with_options_deferred_def_store` + `apply_to` ordering matters:
`apply_to` installs the **project-wide shared** `DefinitionStore` (so
`is_fully_populated()` reflects program-wide state) before the expensive
semantic-def prepopulation gate runs. Because every per-file checker shares the
same `Arc<DefinitionStore>`, a body resolved by one file's checker is visible to
every other file's checker — the foundation of cross-file `DefId` identity and
the `build_type_environment` seeding step.

Per-file `TypeCache` extraction (`extract_cache`) is gated on
`extract_type_cache`: the emit pipeline (JS / `.d.ts`) and incremental reuse
consume it, but a `--noCheck`-without-`--declaration` run has no consumer, so
extracting it for every one of N files would pin several hash maps per file
(observed ~10 GB RSS on a 6000-file repo). The emit-side `TypeCache`
(`context/mod.rs`) is the *snapshot* shape — `symbol_types`, `node_types`,
`def_to_symbol`, `def_to_name`, `def_types`, `def_type_params`, `boxed_*`,
`class_instance_type_*` — with `merge` (accumulate across files for declaration
emit) and `invalidate_symbols` (dependency-graph-driven invalidation) helpers.
See [emitter](emitter.md) for the consumer side and
[end-to-end-timeline](end-to-end-timeline.md) for the full program timeline.

## Edge cases and tsc parity

- **`// @ts-nocheck`**: `prepare_source_file_for_checking` returns `None` and the
  whole file is skipped (no semantic diagnostics), matching tsc.
- **`.d.ts` ambient context**: in a declaration file the entire file is ambient
  (`is_in_ambient_declaration_file = true`); top-level non-declaration statements
  draw TS1036/TS1046, and bulk env-eval persistence is skipped for ambient
  declaration graphs to avoid the recursive `contains_infer_types` scan cost.
- **Per-statement fuel**: tsc resets `instantiationCount` per checked source
  element. tsz mirrors this by resetting per-statement instantiation and
  lazy-resolution budgets inside `StatementChecker::check_with_request`, so heavy
  work in one statement cannot starve the next (#12144, #10677, #10683).
- **TS2563 ("module body too large for control-flow analysis")**: tsz creates
  more flow nodes per expression than tsc (optional chains spawn multiple
  branch/join nodes), so the literal node-count threshold is unreliable;
  `check_source_file` uses a top-level-statement heuristic
  (`MAX_TOP_LEVEL_STATEMENTS = 5000`) instead of a raw flow-node count until tsc's
  runtime depth check is implemented in narrowing.
- **`typeof X` on merged interface+value symbols**: type-position references want
  the instance type, but `typeof` wants the value/constructor type. The split is
  handled by `resolve_lazy` (instance) vs `resolve_type_query` /
  `typeof_value_types` (value), so `typeof Date` gives `DateConstructor`, not the
  `Date` instance.
- **Cross-file circular type aliases (TS2456)**: detected *post*-statement in
  `check_cross_file_circular_type_aliases`, because cross-file `DefId` bodies are
  not all present during the initial `build_type_environment` pass; same-file
  cycles are caught inline in `compute_type_of_symbol`.
- **Order-independent alias resolution**: writes that pin a symbol's owning file
  prefer the immutable declaring-file index over the dynamic overlay, so the same
  `(file, symbol)` resolves identically regardless of file processing order
  (#7574, #12148).
- **Deferred-write replay over silent drop**: a dual-env registration that loses
  the `RefCell` borrow race is reified and replayed rather than dropped, which is
  what keeps class-instance bodies from collapsing to `never` for later consumers.

## Cross-references

- [front-end-scanner-parser](front-end-scanner-parser.md) and
  [binder](binder.md) for the `arena` and `binder` inputs.
- [checker-flow-and-narrowing](checker-flow-and-narrowing.md) for how
  `FlowIntent` and the flow-analyzer `type_environment` drive narrowing.
- [checker-declarations-modules](checker-declarations-modules.md),
  [checker-classes](checker-classes.md),
  [checker-calls-signatures-generics](checker-calls-signatures-generics.md),
  [checker-jsx-properties-accessors-enums](checker-jsx-properties-accessors-enums.md)
  for the specialized checks the statement walk dispatches into.
- [checker-assignability-gateway](checker-assignability-gateway.md) and
  [checker-error-reporter-diagnostics](checker-error-reporter-diagnostics.md) for
  the diagnostic pipeline that consumes structured relation reasons.
- [solver-types-intern-def](solver-types-intern-def.md),
  [solver-evaluation](solver-evaluation.md),
  [solver-instantiation](solver-instantiation.md), and
  [solver-caches-objects-contextual-compat](solver-caches-objects-contextual-compat.md)
  for the `TypeId`/`DefId` universe and the evaluation kernels the context
  resolves into.
- [end-to-end-timeline](end-to-end-timeline.md) for the program-wide build order.
