# End-to-End Timeline: One File from Text to Output, and the Driving Layer

This is the spine document. Every other internals chapter explains one
subsystem in depth; this one follows a single source file from the moment the
process starts to the moment a diagnostic is printed or a `.js` file is
written, naming the real entry-point function that runs at each step. It then
zooms out to the *driving layer* — the code in `crates/tsz-cli`,
`crates/tsz-core`, `crates/tsz-lsp`, and `crates/tsz-wasm` that constructs a
program, orders files, loads libs, parallelizes work, aggregates diagnostics,
and chooses an exit code. The compiler kernel (scanner, parser, binder,
checker, solver, emitter) is owned by other crates; the driving layer is what
turns that kernel into `tsc`-compatible behavior.

The pipeline itself is described stage-by-stage in
[front-end-scanner-parser](front-end-scanner-parser.md), [binder](binder.md),
[checker-context-and-state](checker-context-and-state.md), the `solver-*`
chapters, and [emitter](emitter.md). Here we care about *who calls whom, in
what order, and where the boundaries are*.

## Owns / Must not own

The driving layer (`tsz-cli` driver + `tsz-core` parallel/config/resolution)
owns:

- CLI argument parsing, command selection, and `tsc` exit-code policy.
- `tsconfig.json` discovery, `extends` resolution, and compiler-option merge.
- Root-file discovery and the breadth-first import-graph walk that determines
  *file ordering* (and therefore global `SymbolId` assignment).
- Lib-file selection, loading, and binding.
- Parallelism policy: when to use Rayon, how wide, and when to fall back to
  sequential checking for determinism.
- Program construction (`MergedProgram`), incremental caches, and
  diagnostic aggregation/sorting/filtering before rendering.

The driving layer must **not** own:

- Type semantics. It never runs relation, inference, instantiation,
  evaluation, or narrowing kernels; those belong to the solver. The driver
  hands the checker a `&dyn QueryDatabase` and reads back `Diagnostic`s.
- Diagnostic *meaning*. Codes, messages, and structured reasons originate in
  the checker/solver; the driver only collects, dedups, sorts, and filters
  them to match `tsc`'s program-level ordering and suppression rules.
- AST shape decisions. The parser owns the grammar; the binder owns symbols
  and the flow skeleton.

## The two halves: kernel pipeline vs. driving layer

```text
                        DRIVING LAYER (tsz-cli / tsz-core)
  ┌──────────────────────────────────────────────────────────────────────┐
  │ main → select_command → run_compile → driver::compile                 │
  │            → compile_inner (the orchestrator)                          │
  │   config load → discover roots → BFS import walk → resolve libs        │
  │   → build_program → collect_diagnostics → emit → exit code             │
  └───────────────┬──────────────────────────────────────────────────────┘
                  │ calls into, per file
                  ▼
                        KERNEL PIPELINE (per source file)
   text ─ScannerState─▶ tokens ─ParserState─▶ AST(NodeArena)
       ─BinderState─▶ symbols+scopes+flow ─(merge)─▶ MergedProgram
       ─CheckerState─▶ asks solver via QueryDatabase ─▶ Vec<Diagnostic>
       ─emitter─▶ .js / .d.ts / .map
```

The kernel is per-file and (mostly) parallel; the driving layer is
program-wide and sequences the kernel calls.

## Driver module map

| Path | Role |
| --- | --- |
| `crates/tsz-cli/src/bin/tsz.rs` | Process entry: `main`, `select_command`, batch mode, `--build`/`--showConfig`/`--listFilesOnly` dispatch. |
| `crates/tsz-cli/src/bin/tsz/run.rs` | `run_compile`: invokes the driver, renders diagnostics, writes trace/file lists, sets the exit code. |
| `crates/tsz-cli/src/driver/core.rs` | `compile`, `compile_project`, `compile_with_cache*`, `build_program_with_cache`, `CompilationResult`, `CompilationCache`. |
| `crates/tsz-cli/src/driver/core_diagnostics.rs` | `compile_inner` — the full orchestration body and program-level diagnostic filtering. |
| `crates/tsz-cli/src/driver/sources.rs` | `read_source_files` (the BFS import walk), config/tsconfig resolution, file discovery. |
| `crates/tsz-cli/src/driver/check.rs` | `collect_diagnostics_with_source_resolutions`, `ProgramContext` assembly, parallel/sequential checking policy. |
| `crates/tsz-cli/src/driver/check_file.rs` | `run_check_on_existing_checker` / `check_file_for_parallel`: per-file checker invocation. |
| `crates/tsz-cli/src/driver/emit.rs` | `emit_outputs`, `write_outputs`, output-directory normalization. |
| `crates/tsz-core/src/parallel/` | `parse_files_parallel`, `parse_and_bind_parallel_with_libs_and_target`, `merge_bind_results`, `MergedProgram`, lib loading, residency stats, Rayon pool policy. |
| `crates/tsz-core/src/config/` | `resolve_compiler_options`, lib resolution, `extends` chains. |
| `crates/tsz-core/src/module_resolver/` | `tsc`-style module specifier resolution consumed by the BFS walk and the checker. |
| `crates/tsz-lsp/src/project/` | Editor-mode `Project`: reuses parse/bind/merge and `CheckerState` with persistent caches. |
| `crates/tsz-wasm/src/wasm_api/program.rs` | Browser `TsProgram`: `ensure_compiled` calls the same `parse_and_bind`/`merge_bind_results`. |

## The orchestrator: `compile_inner`

Almost every CLI path converges on one function:
`compile_inner` in `crates/tsz-cli/src/driver/core_diagnostics.rs` (called
through the thin wrappers `compile`, `compile_project`, `compile_with_cache`,
`compile_with_cache_and_changes` in `core.rs`). It is the single place where
the whole timeline is sequenced. Reading it top to bottom is the fastest way
to understand the pipeline; the rest of this document annotates it.

Its signature carries everything the orchestration needs:

```rust
pub(super) fn compile_inner(
    args: &CliArgs,
    cwd: &Path,
    mut cache: Option<&mut CompilationCache>,   // watch/incremental reuse
    changed_paths: Option<&[PathBuf]>,           // incremental: what changed
    forced_dirty_paths: Option<&FxHashSet<PathBuf>>,
    explicit_config_path: Option<&Path>,         // --build mode
) -> Result<CompilationResult>
```

The early body is *config and policy*, not types: resolve the tsconfig path
(`resolve_tsconfig_path`), load it with diagnostics
(`load_config_with_diagnostics`), resolve compiler options
(`resolve_compiler_options`), apply CLI overrides
(`apply_cli_overrides_with_config_options`), and bail out early for the many
`tsc` config-error families (TS5103, TS5102, TS5110, TS18003, etc.) with a
`CompilationResult` that carries only `config_diagnostics`. These early returns
are why `compile_inner` is long: `tsc` parity demands that a malformed config
*stops before checking* and reports exactly the config-level errors.

## Walk-through: a single file, `src/app.ts`

Take the smallest interesting input: one file, no tsconfig.

```ts
// src/app.ts
const x: number = "hello";
```

### Step 0 — process entry and command selection

`main` (`crates/tsz-cli/src/bin/tsz.rs`) initializes tracing, preprocesses
argv (`preprocess_args`), parses into `CliArgs` via `clap`, and — for
project-sized or multi-file work — re-enters on a large-stack thread
(`THREAD_STACK_SIZE_BYTES`, 64 MB) because deeply nested conditional/mapped
type evaluation consumes real stack frames even with logical recursion guards.
`select_command` normalizes args (e.g. promoting a lone directory positional to
`--project`, rejecting `tsconfig`-only or `--build`-only flags) and returns
`Command::Compile`. `run_compile` (`run.rs`) then calls `driver::compile(args,
cwd)`.

### Step 1 — config and root discovery

In `compile_inner`: with no tsconfig present, `resolve_tsconfig_path` returns
`None`, `resolved` is built from CLI overrides only, and `build_discovery_options`
+ `discover_ts_files` (`crates/tsz-cli/src/fs.rs`) turn the explicit
`src/app.ts` into the initial `file_paths`. `collect_type_root_files` adds any
`@types` packages. The result is `root_file_paths = [src/app.ts]`.

### Step 2 — the BFS import walk (`read_source_files`)

`read_source_files` (`crates/tsz-cli/src/driver/sources.rs`) is where *file
ordering* is decided, and ordering is load-bearing. It runs a
**level-synchronous breadth-first search** over the import graph:

1. Seed a `VecDeque` (`pending`) with the canonicalized root paths, recording a
   monotonically increasing `discovery_order` for each first-seen path.
2. For each BFS level, read file bodies in parallel (Rayon), then run the
   *serial* resolution phase that mutates `module_resolver`, `seen`, and
   `pending` in the original pop order. Each file's text is scanned for import
   specifiers, which are resolved through `resolve_module_specifier`
   (`crates/tsz-core/src/module_resolver/`); newly discovered targets are
   appended to `pending`.
3. Dependency lists per file are kept in *source-import order* via
   `push_unique_dep`.

The reason this matters: the merge phase assigns global `SymbolId`s in file
order, so a stable discovery order keeps `SymbolId` assignment stable for an
unchanged graph — which is exactly what the incremental `CompilationCache`
relies on (its doc comments call this out explicitly). For our single file
there are no imports, so the BFS produces just `[src/app.ts]`.

`read_source_files` returns a `SourceReadResult` carrying `sources`,
`dependencies`, `module_resolutions`, and the various resolution-error vectors
(used to synthesize TS2307/TS2688/TS1453/TS1490 later). Binary files are split
off here and get TS1490.

### Step 3 — lib selection and loading

`resolve_effective_lib_paths` computes which `lib.*.d.ts` files apply: the
target-default libs plus any `/// <reference lib="..." />` directives
(expanded transitively by `resolve_lib_files_with_options_transitive`), minus
`--noLib`. For `const x: number`, `number` resolves to the `Number` interface
in `lib.es5.d.ts`, so the default lib set must be loaded.

`parallel::load_lib_files_for_binding_strict` (in
`crates/tsz-core/src/parallel/core/parse_and_libs.rs`) loads them. It is
heavily optimized: embedded lib content (`crate::embedded_libs`) avoids disk
I/O entirely, and a disk-backed snapshot cache
(`crates/tsz-core/src/parallel/lib_snapshot.rs`, `try_load_many` /
`try_store_many`) can skip parse+bind by deserializing already-bound lib state.
On a miss, each lib is parsed (`ParserState::parse_source_file`) and bound
(`BinderState::bind_source_file`), largest file first so Rayon work-stealing
starts `dom.d.ts` early. The result is `Vec<Arc<LibFile>>`.

A second, *checker-facing* clone of the libs is started on a background thread
(`load_checker_libs` via `clone_lib_files_for_checker`) because the
binding-phase libs are mutated during declaration merging, while the checker
needs clean lib binders. This is the `tsz-checker-lib-clone` thread spawned in
`compile_inner`.

### Step 4 — parse + bind + merge → `MergedProgram`

With no incremental cache, `compile_inner` calls
`parallel::parse_and_bind_parallel_with_libs_and_target(compile_inputs,
&lib_files, target)` then wraps `parallel::merge_bind_results(bind_results)` in
an `Arc`. With a cache it goes through `build_program_with_cache` instead (see
*Caches and invariants*).

Per file, the parallel bind path runs the kernel:

- **Scan + parse.** `ParserState::new(file_name, source_text)` then
  `parse_source_file` (`crates/tsz-parser/src/parser/state_statements.rs`).
  This drives the scanner, parses statements into a `NodeArena`, caches comment
  ranges, folds scanner diagnostics (e.g. TS1185 conflict markers) into
  `parse_diagnostics`, sorts them in `tsc`'s `compareDiagnostics` order, and
  builds the `SourceFileData` root node. `parser.into_parts()` yields
  `(NodeArena, Vec<ParseDiagnostic>)`. `.json` inputs bypass the grammar
  entirely via `synthesize_json_source_file` (running only strict-JSON
  validation), matching `tsc`'s dedicated JSON path.
- **Bind.** `BinderState::new()` then `bind_source_file(&arena, source_file)`
  (`crates/tsz-binder/src/state/core.rs`). The binder resets its per-file stack
  guard, clears resolution caches, pre-sizes `node_symbols`/`node_flow`,
  builds the persistent scope tree, assigns `SymbolId`s, and constructs the
  control-flow skeleton (`flow_nodes`, `node_flow`). It computes *no types*.
- The per-file result is a `BindResult` (defined in
  `crates/tsz-core/src/parallel/core/parse_and_libs.rs`) — a wide struct of
  `Arc`-shared maps (`symbols`, `file_locals`, `node_symbols`, `scopes`,
  `flow_nodes`, `semantic_defs`, etc.) designed so the merge and later per-file
  binder reconstruction are atomic `Arc::clone`s rather than deep copies.

`merge_bind_results` / `merge_bind_results_ref` (`merge_support.rs`) combine
the per-file `BindResult`s into a single `MergedProgram`: it remaps each file's
local `SymbolId`s into a global `SymbolArena`, builds program-wide `globals`,
`module_exports`, `reexports`, `declaration_arenas`, `cross_file_node_symbols`,
and — critically — the shared `TypeInterner` and a `DefinitionStore`
pre-seeded with `DefId`s for every top-level `semantic_def`. The
`MergedProgram` is the single semantic universe the checker reads.

For our file, the `MergedProgram` has one `BoundFile` for `src/app.ts` plus the
lib globals, with `x` bound to a `SymbolId` and `number`/`Number` reachable
through the lib binders.

### Step 5 — assemble `ProgramContext` and check

`collect_diagnostics_with_source_resolutions` (`crates/tsz-cli/src/driver/check.rs`)
assembles a `tsz::checker::context::ProgramContext` from the `MergedProgram`:
shared `Arc`s for lib contexts, all arenas, all binders, resolved-module
indices, reexport/augmentation indices, and the shared `DefinitionStore`. It
calls `build_global_indices` (or the fingerprint-aware
`build_global_indices_if_changed` when a skeleton index exists) and
`build_global_symbol_file_index`. It installs a shared `DefinitionStore` so all
parallel checkers allocate `DefId`s from one globally-unique sequence —
without this, independent `DefId` streams would collide via `TypeData::Lazy(DefId)`
interning.

Then it checks each file. For our single file the *sequential reused-checker*
path is chosen (tiny batches avoid Rayon overhead and concurrent-interning
nondeterminism). Per file it ends up in `run_check_on_existing_checker`
(`check_file.rs`):

1. Build a per-file `BinderState` from the `BoundFile` (sharing the merged
   `Arc` maps) via `create_binder_from_bound_file`.
2. Construct a `CheckerState` (`CheckerState::with_options` /
   `new_with_shared_def_store`, `crates/tsz-checker/src/state/state.rs`). The
   checker receives `&dyn QueryDatabase` — a `QueryCache` wrapping the program's
   `TypeInterner` (`crates/tsz-solver/src/caches/db.rs`, `query_cache.rs`).
   **This handle is the only door between checker and solver.** The checker asks
   semantic questions through it; it does not run kernels itself.
3. `tsz::checker::reset_stack_overflow_flag()`, then
   `checker.check_source_file(file.source_file)`
   (`crates/tsz-checker/src/state/state_checking/source_file.rs`). This walks
   the AST, resolves heritage interface bodies, then checks every statement.
   When it reaches `const x: number = "hello"`, it asks the solver whether
   `"hello"` is assignable to `number` through the shared
   `query_boundaries/assignability` gateway
   ([checker-assignability-gateway](checker-assignability-gateway.md)); the
   solver returns a structured failure reason
   ([solver-relations](solver-relations.md)), and the checker maps it to
   **TS2322 `Type 'string' is not assignable to type 'number'.`**, anchored at
   the initializer span.
4. The checker's `ctx.diagnostics` are drained
   (`std::mem::take(&mut checker.ctx.diagnostics)`) and post-processed
   (`post_process_checker_diagnostics`): JS grammar filtering,
   `@ts-expect-error`/`@ts-ignore` suppression, and syntax-error gating.

The diagnostic flows back up as a `Vec<Diagnostic>`. Parse diagnostics are
folded in too (converted to TS8xxx for JS files, kept as-is for TS).

### Step 6 — program-level filtering and sorting

Back in `compile_inner`, the per-file diagnostics are joined with
`config_diagnostics`, `binary_file_diagnostics`, and `type_file_diagnostics`,
then run through the program-level rules that make tsz match `tsc`'s
*whole-program* behavior, not just per-file behavior:

- JS-only-syntactic gate: if any `TS8xxx` JS-syntactic diagnostic fires, every
  other file loses its semantic diagnostics (mirrors `tsc`'s
  `emitFilesAndReportErrors` running `getSyntacticDiagnostics` first).
- TS2304 suppression near TS8xxx grammar errors.
- Deprecation-diagnostic priority (TS5107/TS5101 vs. grammar errors).

Finally `diagnostics.sort_by(|l, r| l.compare(r))` puts them in `tsc`'s
canonical order (by file, then position, then code).

### Step 7 — emit (skipped here) and exit code

`should_emit = !(no_emit || (no_emit_on_error && has_error))`. Our program has
a TS2322 error and the default is *not* `--noEmitOnError`, so `tsc` *would*
still emit `app.js`. `emit_outputs` (`crates/tsz-cli/src/driver/emit.rs`) runs
the emitter ([emitter](emitter.md)) to produce `OutputFile`s; `write_outputs`
writes them unless a declaration-emit-blocking diagnostic (TS9007–TS9039,
TS4020, TS6200) suppresses the `.d.ts`. The emitter performs **no** semantic
validation — it consumes the AST plus checked `TypeCache` summaries.

`compile_inner` returns a `CompilationResult`. `run_compile` (`run.rs`) renders
diagnostics through `Reporter` and picks the exit code:
`EXIT_SUCCESS` (0) when no errors, `EXIT_DIAGNOSTICS_OUTPUTS_GENERATED` (2)
when errors exist but output was generated (or `--noEmit`), and
`EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED` (1) when `--noEmitOnError` suppressed output.
For our file: TS2322 printed, `app.js` written, exit code 2.

## Cross-file walk-through: two files

```ts
// src/util.ts
export const greeting = "hi";
// src/app.ts
import { greeting } from "./util";
const n: number = greeting;
```

The differences from the single-file trace:

- **BFS ordering.** `read_source_files` seeds `pending` with `src/app.ts`,
  reads it, scans `import { greeting } from "./util"`, resolves `./util` to
  `src/util.ts`, and appends it. Discovery order is `app.ts`, then `util.ts`;
  the merge assigns global `SymbolId`s in that order.
- **Merge wiring.** `merge_bind_results` records `util.ts`'s exported
  `greeting` symbol in `module_exports` keyed by file path, and the importer's
  binder is bridged to it via `propagate_module_export_maps` using the
  pre-computed `resolved_module_paths` (no filesystem calls during checking).
- **Parallel vs. sequential checking.** With more than one file and no
  order-sensitive global lib, the driver may use the Rayon
  `par_iter` fresh-checker path (`check_file_for_parallel`), each file with its
  own `CheckerState` and `QueryCache` but a shared `TypeInterner` (a thread-safe
  `DashMap`) and shared `DefinitionStore`. A `SharedQueryCache` deduplicates
  cross-file evaluations. Small projects stay sequential for determinism.
- **The error.** Checking `app.ts`, `greeting`'s type resolves cross-file to
  the string literal `"hi"`; assigning it to `number` yields TS2322 again, this
  time with the cross-file symbol resolved through `ProgramContext`.

## Caches and invariants

The driving layer carries several caches whose invalidation rules are part of
`tsc` parity. Their owners and keys:

| Cache | Owner | Key | Invalidation |
| --- | --- | --- | --- |
| `CompilationCache` (`type_caches`, `bind_cache`, `dependencies`, `diagnostics`, `export_hashes`, `import_symbol_ids`) | `crates/tsz-cli/src/driver/core.rs` | `PathBuf` | Watch/incremental: `invalidate_paths`, `invalidate_paths_with_dependents_symbols` (transitive via `reverse_dependencies`). |
| `cached_merged_program` | `CompilationCache` | whole program | Fast path: when no file re-parsed (`dirty_paths` empty) and file count unchanged, returns the cached `Arc<MergedProgram>` directly, skipping the O(total\_symbols) merge. Replaced whenever any file is dirty or the file count changes. |
| `bind_cache` (per-file `BindResult` + content hash) | `CompilationCache` | `PathBuf` | `build_program_with_cache` re-parses only files whose `hash_text_with_language_version` differs; unchanged files reuse their `BindResult`. Pruned to the current file set each build. |
| `BuildInfo` (`.tsbuildinfo`) | `crates/tsz-cli/src/incremental.rs` | on disk | Loaded when `--incremental`; seeds a `CompilationCache` (`build_info_to_compilation_cache`); saved after a successful no-error build. Preserves `latestChangedDtsFile` across no-emit saves, matching `tsc`. |
| lib snapshot cache | `crates/tsz-core/src/parallel/lib_snapshot.rs` | `(file_name, content_hash)` | Content-hash keyed; a hit skips parse+bind of lib files. Disable with `TSZ_LIB_CACHE`. |
| shared `TypeInterner` | `MergedProgram` | `TypeKey` → `TypeId` | Lives for the program; thread-safe `DashMap`. Batch mode clears thread-locals between projects (`clear_batch_iteration_state`). |
| shared `DefinitionStore` | `ProgramContext` | `DefId` | One per program; guarantees globally-unique `DefId` allocation so `Lazy(DefId)` interning never collides across parallel checkers. |

Invariants the driving layer must preserve:

- **Discovery order is stable for an unchanged graph.** The BFS in
  `read_source_files` and the cached-rebuild replay both produce identical
  `discovery_order`, so global `SymbolId` assignment is deterministic. The
  `CompilationCache::dependencies` doc comment pins this.
- **Single semantic universe.** All parallel checkers share one
  `TypeInterner` and one `DefinitionStore`; the driver never spins up a second
  type universe.
- **Worker isolation in batch mode.** `run_batch_mode` (`tsz.rs`) calls
  `clear_batch_iteration_state` between compilations to drop thread-local
  construction caches, subtype state, checker thread-locals, and the resolver's
  path-existence caches — a reused worker must not read stale `TypeData` for a
  `TypeId` reused by a fresh interner, nor a stale `is_file` answer.

## Parallelism policy

Parallelism is *policy*, decided by the driver, not the kernel:

- **Parse/bind** are embarrassingly parallel: `parse_files_parallel` and
  `parse_and_bind_parallel*` use Rayon (`maybe_parallel_into!`). On WASM
  (`target_arch = "wasm32"`) these degrade to sequential iteration to avoid
  oversubscription against host worker threads.
- **The Rayon pool** is initialized lazily and once (`ensure_rayon_global_pool`,
  `RAYON_POOL_INIT: Once`) with the 64 MB worker stack. Small workloads use a
  scoped narrower pool (`run_with_rayon_pool_for_work_items`,
  `SMALL_WORKLOAD_RAYON_MAX_ITEMS = 32`, `SMALL_WORKLOAD_RAYON_THREADS = 4`) so
  high-core machines don't pay full-width startup cost on tiny app projects.
- **Checking** chooses between sequential reused-checker and parallel
  fresh-checker paths in `check.rs` based on file count and whether an
  order-sensitive global lib (e.g. DOM/webworker globals) is present.
  Determinism wins: tiny batches and order-sensitive lib sets force sequential
  so concurrent type interning cannot produce schedule-dependent diagnostics.
  Escape hatches (`TSZ_EXPERIMENT_FORCE_PARALLEL_CHECK*`) exist only for repro
  work.

## How LSP and WASM reuse the same pipeline

The kernel and `tsz-core` parallel layer are the shared substrate; LSP and
WASM are *different drivers* over the same `parse → bind → merge → check`
steps. They do **not** reimplement type algorithms.

- **WASM** (`crates/tsz-wasm/src/wasm_api/program.rs`): `TsProgram::ensure_compiled`
  calls `parse_and_bind_parallel_with_libs` (or `parse_and_bind_parallel`) then
  `merge_bind_results`, storing the `MergedProgram`. `get_semantic_diagnostics`
  / `get_type_checker` then run `CheckerState` over it, exactly like the CLI —
  the only WASM-specific work is byte↔UTF-16 offset conversion for Monaco.
- **LSP** (`crates/tsz-lsp/src/project/`): a long-lived `Project` keeps the
  bound program and per-file caches resident, and `project_file.rs` calls
  `checker.check_source_file(self.root)` on a `CheckerState` constructed with a
  shared `DefinitionStore` (`with_cache_and_shared_def_store`). When a file's
  cached check is still valid, the diagnostic *pull* model
  (`project/diagnostic_pull.rs`) returns cached diagnostics instead of
  re-running `check_source_file`. Edits invalidate via the same dependency
  graph the CLI uses.

Because all four front ends (CLI, batch, LSP, WASM) funnel through
`parse_and_bind_parallel*` + `merge_bind_results` + `CheckerState`, a fix in
the kernel is automatically shared; the drivers differ only in *scheduling,
caching, and output*.

## `--build` and project references

`handle_build` (`tsz.rs`) loads a `ProjectReferenceGraph`, validates reference
constraints (TS6306/TS6310/TS6202), topologically sorts via `build_order`, and
calls `driver::compile_project` (→ `compile_inner` with an
`explicit_config_path`) per project in dependency order, skipping up-to-date
projects unless `--force`. Each project still runs the full single-program
timeline above; the build layer only sequences whole programs and accumulates
exit-code state across them.

## Edge cases and tsc parity

The driving layer is where many *whole-program* `tsc` quirks live — the kernel
cannot see them because they depend on the program as a whole:

- **No inputs.** Empty `file_paths` with no explicit `files`/`references`
  yields TS18003 (`no_input_diagnostics_for_config`); a references-only
  solution-style root is silent, matching `tsc`.
- **Config stops the world.** Fatal config diagnostics (TS5103, removed-option
  TS5102, TS5110, TS5090) return before any file is read, so no follow-on
  semantic noise leaks.
- **`--noCheck` / parse-only.** `compile_inner` has short-circuit arms that run
  only `parse_files_parallel` + grammar diagnostics (and isolated-declaration
  TS9007–TS9039 when `--isolatedDeclarations`), skipping `ProgramContext`
  entirely.
- **`--skipLibCheck` on a pure `.d.ts` no-emit project.** A dedicated arm
  avoids loading default libs and binding declaration files, emitting only
  parse/config/type-reference diagnostics.
- **Binary files.** Detected during `read_source_files`; only TS1490 survives
  (`binary_file_names_to_suppress` strips the cascading TS1127 false positives
  that UTF-16-as-UTF-8 parsing would produce).
- **JS-only-syntactic gate.** Any `TS8xxx` from checked JS suppresses *all*
  semantic diagnostics program-wide, mirroring `tsc`'s syntactic-first phase
  ordering — a per-file checker cannot enforce this; the driver does.
- **Declaration-emit blocking.** TS9007–TS9039, TS4020, and TS6200 block the
  corresponding `.d.ts` output (or all of them, for the augments-cannot-be-
  serialized case) even though JS still emits.
- **Exit codes.** `run_compile` distinguishes outputs-generated (2) from
  outputs-skipped (1) exactly the way `tsc`'s `ExitStatus` enum does, including
  the `--noEmit`-selects-2 subtlety driven by `result.no_emit`.

## Where to go next

- The front-end steps: [front-end-scanner-parser](front-end-scanner-parser.md)
  and [binder](binder.md).
- The checker walk that `check_source_file` drives:
  [checker-context-and-state](checker-context-and-state.md),
  [checker-flow-and-narrowing](checker-flow-and-narrowing.md),
  [checker-declarations-modules](checker-declarations-modules.md),
  [checker-classes](checker-classes.md),
  [checker-calls-signatures-generics](checker-calls-signatures-generics.md),
  [checker-jsx-properties-accessors-enums](checker-jsx-properties-accessors-enums.md).
- The query boundary the checker uses to reach the solver:
  [checker-assignability-gateway](checker-assignability-gateway.md) and
  [checker-error-reporter-diagnostics](checker-error-reporter-diagnostics.md).
- The solver kernels behind the `QueryDatabase`: [solver-relations](solver-relations.md),
  [solver-inference](solver-inference.md),
  [solver-instantiation](solver-instantiation.md),
  [solver-evaluation](solver-evaluation.md),
  [solver-narrowing](solver-narrowing.md),
  [solver-operations](solver-operations.md),
  [solver-types-intern-def](solver-types-intern-def.md),
  [solver-caches-objects-contextual-compat](solver-caches-objects-contextual-compat.md).
- The final stage: [emitter](emitter.md).
