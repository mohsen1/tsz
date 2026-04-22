# Large-repo performance follow-up (2026-04-22)

Status: **open** — context dump for the next perf iteration.

This document captures what shipped during the 2026-04-22 perf sweep,
what the post-sweep `bench-vs-tsgo` run revealed, and what the
remaining high-leverage work looks like. It is intended as a read-me
for whoever picks up the campaign next, including fresh profiling data
from a nightly run on `large-ts-repo`.

## 1. What shipped on 2026-04-22

10 perf / perf-adjacent PRs landed in one day. All are in `main`.

### Parallelizations (4)
- **#810** `perf(cli): parallelize prepare_binders for large-repo scaling`
- **#815** `perf(cli): parallelize build_cross_file_binders`
- **#821** `perf(cli): parallelize collect_module_specifiers AST scan`
- **#824** `perf(cli): parallelize per_file_ts7016_diagnostics`

Each lifts a previously-sequential per-file startup pass to `par_iter()`.
Individually each is a small win; together they flatten the
pre-check phase on any repo with more than a handful of files.

### Algorithmic complexity (2)
- **#825** `perf(cli): O(N²)→O(N) — pre-bucket resolved_module_specifiers by file_idx`
  `check_file_for_parallel` was re-filtering the entire
  program-wide `resolved_module_specifiers` set per file; pre-bucket
  once, share via `Arc<Vec<FxHashSet<String>>>`. Bench-compare
  showed **-2.38% avg across parsed cases, 7 cases improved, 0
  regressed** — the cleanest single win of the batch.
- **#826** `perf(cli): O(N×M)→O(N+M) — symbol_file_targets via arena pointer hashmap`
  `symbol_file_targets` rebuilt itself via `all_arenas.iter().position(|a| Arc::ptr_eq(a, arena))`
  for every symbol — roughly 0.6–3 billion pointer compares at
  startup on a 6000-file project. Replaced with a one-shot
  `arena_ptr → file_idx` hashmap.

### Pre-sizing (2)
- **#828** `perf(cli): pre-size file_locals SymbolTable to avoid per-file rehashes`
- **#851** `perf(cli): pre-size program path maps in collect_diagnostics`

### Conformance fix required by the per-binder ProjectEnv migration (1)
- **#803** `fix(checker): finish per-binder alias_partners + declared_modules migration`
  Migrated 5 unmigrated direct reads of `binder.alias_partners` /
  `binder.declared_modules`; added `alias_partner_reverse`
  accessor; populated `global_declared_modules` from
  `program.declared_modules` when no skeleton exists. Fixed 2
  conformance regressions that the empty-out work had introduced.

### Bench tooling (1)
- **#817** `chore(bench): unblock large-ts-repo from bench-vs-tsgo (was silently SKIP'd)`
  The `tsc --noEmit -p` precheck tripped at 120 s on the 6086-file
  fixture, causing the entire `large-ts-repo` bench to be silently
  marked SKIP. Skipped the tsc precheck for `large-ts-repo` (we
  control the fixture); bumped tsz/tsgo precheck ceiling; capped
  hyperfine to 1 warmup + 3–5 runs at 300 s each to stay inside the
  CI budget. Without this the campaign's primary fixture wasn't
  even being measured.

## 2. `bench-vs-tsgo` after the sweep

Run: **2026-04-22 14:18 UTC**, local (Apple Silicon, Opus dev host).
Artifact: `artifacts/bench-vs-tsgo-20260422-161834.json`.

- **Score: tsz 56 · tsgo 14** across 70 comparable cases (76 total; 5 errors; 1 timeout).
- Every `tsz` win is in the 1.0×–2.3× range. The real-world fixture
  wins (`utility-types/aliases-and-guards.ts` 2.28×,
  `utility-types/mapped-types.ts` 1.80×) are the largest.
- **No case shows tsz at 2× tsgo except on utility-types/*,** which
  is far below the goal of "2× everywhere, especially large repos."

### Where tsgo still beats tsz

| case | lines | tsz_ms | tsgo_ms | factor |
|---|---:|---:|---:|---:|
| `ts-essentials/deep-readonly.ts` | 39 | 1552 | 94 | **16.56×** |
| `ts-essentials/paths.ts` | 101 | 698 | 96 | **7.27×** |
| `ts-essentials/deep-pick.ts` | 47 | 348 | 94 | **3.68×** |
| `BCT candidates=200` | 428 | 433 | 263 | 1.64× |
| `200 generic functions` | 4611 | 382 | 265 | 1.44× |
| `binaryArithmeticControlFlowGraphNotTooLarge.ts` | 1298 | 370 | 267 | 1.39× |
| `200 classes` | 9203 | 334 | 247 | 1.35× |
| `200 union members` | 491 | 296 | 241 | 1.23× |
| `100 generic functions` | 2311 | 284 | 250 | 1.13× |
| `ts-toolbelt/Any/Compute.ts` | 61 | 270 | 244 | 1.11× |
| `Mapped complex template keys=200` | 252 | 251 | 233 | 1.08× |
| `Mapped type keys=450` | 481 | 252 | 237 | 1.06× |
| `CFA branches=100` | 603 | 342 | 339 | 1.01× |
| `Constraint conflicts N=200` | 819 | 257 | 256 | 1.01× |

Two clear shapes:
1. **Deep recursive utility types** (`deep-readonly`, `paths`,
   `deep-pick`, `ts-toolbelt/Any/Compute`). These repeatedly
   evaluate the same conditional/mapped structures with different
   substitutions; tsgo hides the cost in its type cache, tsz
   redoes the work.
2. **Large-N generic stress** (`BCT candidates=200`,
   `200 generic functions`, `200 classes`). These push the solver
   through the same algorithm N times with slightly different
   inputs; tsz scales worse than tsgo by a constant factor.

### The large-repo blocker

| fixture | result |
|---|---|
| `large-ts-repo` (337 375 lines, 39 MB of .ts, 6086 files) | **tsz: timeout after 300 s** |

Verified outside the bench harness: `timeout 600 tsz --noEmit -p large-ts-repo/tsconfig.flat.json`
exited with **code 137** at ≈ 2 min — a SIGKILL, almost certainly
macOS jetsam (peak RSS over the available headroom on a 16 GB
system). `tsgo` on the same fixture sits at ~4.6 GB RSS / ~300 %
CPU and finishes inside 5 minutes on the same hardware.

Conclusion: **tsz is memory-bound on projects of this size, not
compute-bound.** Every one of today's PRs reduced either CPU work
or peak allocation, yet the cumulative savings still aren't enough
to keep the 6086-file project under the OOM ceiling. The campaign
goal ("2× faster than tsgo on large repos") is blocked at
"tsz must first *finish* large-repo without being killed."

## 3. Remaining high-leverage targets

All of the items below are bigger than a single same-day PR.
They are in rough priority order for moving `large-ts-repo` from
OOM-kill to "finishes at all," then "finishes faster than tsgo":

### 3.1. `binder.module_exports` consumer migration [memory, largest known]

Per-file `create_binder_from_bound_file_with_augmentations` still
does `module_exports: program.module_exports.clone()`. On the
large-ts-repo fixture this is a deep clone of the cross-file
merged exports table — potentially hundreds of MB — **into every
one of 6086 per-file binders**. The cross-file lookup binders
already set `module_exports: Default::default()`; the per-file
checking binders do not, because roughly 20 consumers still do
`binder.module_exports.get(key)` or `binder.module_exports.iter()`
directly.

- `ctx.module_exports_for_module(binder, key)` already exists and
  prefers the project-wide `program_module_exports` Arc.
- Migration: route all 20+ direct reads through the accessor,
  then set per-file `module_exports: Default::default()`.
- Known iter-consumers (expand at least these): `context/core.rs`
  (two spots), `context/mod.rs`, `declarations_module_helpers.rs`.
  The iter consumers may need a new `ctx.module_exports_iter`
  accessor to preserve semantics.

Size: mechanical but spread across many files. Risk: moderate
(similar pattern to #803, which had to fix 5 missed sites after
an incomplete migration).

### 3.2. `binder.declaration_arenas` per-file materialization [startup CPU + memory]

In `create_binder_from_bound_file_with_augmentations`:

```rust
let mut declaration_arenas: DeclarationArenaMap = program
    .declaration_arenas
    .iter()
    .filter_map(|(&(sym_id, decl_idx), arenas)| {
        let has_non_local_arena =
            arenas.iter().any(|arena| !Arc::ptr_eq(arena, &file.arena));
        has_non_local_arena.then(|| ((sym_id, decl_idx), arenas.clone()))
    })
    .collect();
```

Per-file this iterates the **program-wide** `declaration_arenas`
map. On a 6086-file project with ~100 K total declarations that
is ~600 M entry visits across all threads, each cloning a
`SmallVec<[Arc<NodeArena>; 1]>`. Critically: the filter keeps
~99 % of entries on a large project, so the filtering doesn't
even save much — the work is almost entirely duplicated.

Two possible fixes:
1. **Share via Arc.** Add
   `ctx.declaration_arenas_get(binder, sym_id, decl_idx)` that
   prefers a project-wide `Arc<DeclarationArenaMap>` on
   `ProjectEnv`. Empty the per-file map. ~20 consumer sites,
   similar to the module_exports migration.
2. **Change the field type.** Make
   `BinderState.declaration_arenas: Arc<DeclarationArenaMap>`.
   Mutations during binding use `Arc::make_mut` (zero-cost when
   refcount=1, which is always during binding). Reads go through
   `Deref`. Smaller consumer-side surface change but touches
   every binder mutation site in `tsz-binder` and the merge code
   in `tsz-core::parallel`.

I attempted option 1 as a foundation during the sweep and
backed out — adding the field without migrating consumers wins
nothing, and migrating consumers in one PR is a several-hour
change I didn't want to rush.

### 3.3. `instantiate_type` cross-call cache [compute, the utility-type blow-ups]

`TypeInstantiator` is constructed **per call** at every
`instantiate_type(interner, type_id, substitution)` entry point.
Its `visiting` cache is scoped to one invocation; two sibling
callers evaluating the same `(type_id, substitution)` redo the
full work.

For `DeepReadonly<T>` and friends this is the 16× gap above.
tsgo pays it once per unique shape; tsz pays it per call site.

Constraints:
- Cache key must be `(TypeId, Substitution)` where `Substitution`
  is content-hashable, not pointer-hashable — two substitutions
  with the same contents must hit the cache.
- Must respect the existing depth guards and error states so
  cache hits don't paper over an `ERROR` from a cycle.
- `QueryCache` already memoizes some evaluate paths but does
  **not** wrap `instantiate_type` — callers construct a fresh
  `TypeInstantiator` inside the solver crate directly. Either
  route these through `QueryCache` or add a dedicated
  `InstantiationCache` with the same `RefCell`/`Cell` pattern.

This is the single biggest single-file perf improvement
available. It is also the highest-risk: a wrong cache key
corrupts type identity across the whole pipeline.

### 3.4. BCT algorithm review [compute, 1.3–1.6× gap]

`BCT candidates=200`: 1.64× slower than tsgo. Likely the primary
vs union supertype detection in `inference/infer_bct.rs` doing
O(N²) work that tsgo shortcircuits. Needs profiling to confirm.

### 3.5. `lib_symbol_ids` Arc wrap [memory, cheap]

`program.lib_symbol_ids.clone()` is done per-file binder
construction (twice — cross-file + per-file). On large repos
`lib_symbol_ids` is a `FxHashSet<SymbolId>` with 10K+ entries;
deep clone × 12 K binder constructions is real allocation.

Change `BinderState.lib_symbol_ids: Arc<FxHashSet<SymbolId>>`.
Only 2 mutation sites in the binder crate
(`state/core.rs:271 .clear()`, `state/lib_merge.rs:376 .insert()`),
both during binding where refcount=1 so `Arc::make_mut` is free.
Smaller scope than the module_exports / declaration_arenas
migrations.

## 4. Concrete suggested next sequence

Assuming the next session starts with profiler data (e.g. `cargo
flamegraph --bin tsz -- --noEmit -p large-ts-repo/tsconfig.flat.json`):

1. **First:** chase the OOM. `dhat` / `heaptrack` a small run
   (say 1000 files) and confirm which allocations dominate. The
   suspects in priority order:
   - `program.module_exports.clone()` per-file binder (3.1 above)
   - `program.declaration_arenas`-derived per-file map (3.2)
   - `program.symbol_arenas`-derived per-file map
2. Ship the winner as its own PR. It's likely the module_exports
   migration.
3. **Second:** once large-ts-repo finishes at all, re-run
   `bench-vs-tsgo` with the full suite and establish a real
   large-repo time vs tsgo. Only then is "2× on large repos"
   measurable.
4. **Third:** tackle `instantiate_type` cross-call cache (3.3).
   This is the lever that closes the 16× utility-type gap.
5. BCT and other solver-level work as a follow-up pass.

## 5. Quick-reference bench state

Artifact: `artifacts/bench-vs-tsgo-20260422-161834.json`

- 76 cases run, 5 errors, 1 timeout
- tsz wins: 56; tsgo wins: 14
- Biggest tsz wins: `utility-types/aliases-and-guards.ts` (2.28×),
  `utility-types/mapped-types.ts` (1.80×)
- Biggest tsgo wins: `ts-essentials/deep-readonly.ts` (16.56×),
  `ts-essentials/paths.ts` (7.27×), `ts-essentials/deep-pick.ts` (3.68×)
- **`large-ts-repo`: tsz TIMEOUT / OOM** — the campaign's primary
  target is blocked on memory.

Don't treat the 56–14 win count as the headline. The headline is
the large-repo timeout and the three ts-essentials blow-ups.
