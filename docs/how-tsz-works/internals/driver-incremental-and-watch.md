# Incremental Build and Watch Mode

This doc fills the gap that [end-to-end-timeline](end-to-end-timeline.md) left
at the boundary between "one compile" and "many compiles over time." The
timeline doc traces a *single* invocation `scanner -> parser -> binder ->
checker -> solver -> emitter`. This doc covers what happens **across** rebuilds:
how `tsz` persists state between runs in a `.tsbuildinfo` file, how watch mode
detects a file edit and turns it into a minimal recompile, what survives in
memory inside a `CompilationCache`, and how a single edited file cascades to its
dependents through an export-signature comparison rather than a blind full
rebuild. It also covers the deeper kernel mechanism the driver leans on for
in-process reuse: `CheckerContext::switch_to_file`, the file-session reset that
makes one long-lived checker safe to re-target at the next file.

All of this lives in the **driving layer** (`crates/tsz-cli`), with one
kernel-side primitive in `crates/tsz-checker`. The driver owns *file ordering,
change detection, cache lifetime, and invalidation policy*; the checker owns
*what state is safe to retain when the active file changes*. The solver and
emitter are unaware of incrementality — they are re-run (or skipped) by the
driver, never partially mutated.

For the sibling that covers multi-project orchestration (`tsc --build`,
`references[]`, `.tsbuildinfo` per project), see
[driver-project-references-and-build-mode](driver-project-references-and-build-mode.md);
this doc is about the *single-project* incremental and watch loop. For how the
LSP reuses state across keystrokes (a different reuse path entirely), see
[lsp-and-wasm-surfaces](lsp-and-wasm-surfaces.md).

## Owns / Must not own

| Concern | Owner | Notes |
| --- | --- | --- |
| File-change detection, debounce, watch roots | `crates/tsz-cli/src/commands/watch.rs` | filesystem `notify` events, not type semantics |
| `.tsbuildinfo` format, load/save, file-version hashing | `crates/tsz-cli/src/project/incremental.rs` | serde JSON; content hashes only |
| In-memory cross-rebuild caches (`bind_cache`, `type_caches`, `export_hashes`, dependency graph) | `CompilationCache` in `crates/tsz-cli/src/driver/core.rs` | the live watch-session memory |
| Smart-invalidation work queue (which files to recheck) | `crates/tsz-cli/src/driver/check.rs` | export-hash cascade |
| Per-file checker re-targeting (in-process file-session reuse) | `CheckerContext::switch_to_file` in `crates/tsz-checker/src/context/file_session_reset.rs` | the kernel's "safe to reuse" contract |
| Export-signature fingerprint (the change predicate) | `tsz_lsp::export_signature` (`ExportSignature`, `ExportSignatureInput`) | shared by CLI and LSP for identical decisions |
| **Must not own:** relation/inference/evaluation kernels | — | the driver re-runs the checker/solver; it never patches types to fake incremental results |
| **Must not own:** diagnostic *semantics* | — | cached diagnostics are replayed verbatim; the driver never synthesizes a new TS-code |

## Two layers of incrementality

`tsz` has two independent reuse mechanisms that are easy to conflate. Keep them
separate:

1. **Cross-process / cross-rebuild (`.tsbuildinfo`)** — implemented in
   `incremental.rs`. State is serialized to disk between *separate* `tsz`
   invocations. Activated by `incremental: true` (or `--incremental`). Stores
   file content hashes, the dependency graph, and cached semantic diagnostics.
   This is what makes a cold `tsz` run faster when only one file changed since
   the last run.

2. **In-process (`CompilationCache`)** — implemented in `driver/core.rs`. A
   live, in-memory cache held across rebuilds *within a single watch session*.
   Holds `BindResult`s, per-file `TypeCache`s, the merged program, and the
   reverse-dependency graph. This is what makes the *N*th rebuild in
   `tsz --watch` fast.

Watch mode owns a `CompilationCache` for its whole lifetime (see
`WatchState::type_cache`). The `.tsbuildinfo` path is used by non-watch
incremental compiles and is *not* loaded when a `CompilationCache` is already
provided (the two are mutually exclusive in `compile_inner`, see below).

```
                tsz --incremental (no watch)        tsz --watch
                ----------------------------        -----------
  between runs: .tsbuildinfo on disk               (process exits, nothing kept)
  within run:   one full compile                   long-lived CompilationCache,
                                                    re-used every keystroke
```

## `.tsbuildinfo`: the on-disk format

`crates/tsz-cli/src/project/incremental.rs` defines the serialized format. The
root struct is `BuildInfo` (`#[serde(rename_all = "camelCase")]`), whose fields
mirror tsc's build-info shape closely enough to be human-recognizable:

| `BuildInfo` field | Purpose |
| --- | --- |
| `version` (`BUILD_INFO_VERSION = "0.1.0"`) | format-compat gate |
| `compiler_version` (`CARGO_PKG_VERSION`) | invalidate on `tsz` upgrade |
| `root_files` | the project's entry files |
| `file_infos: BTreeMap<String, FileInfo>` | per-file content `version` + export `signature` |
| `dependencies: BTreeMap<String, Vec<String>>` | `file -> files it imports`, in import order |
| `semantic_diagnostics_per_file` | cached `CachedDiagnostic`s replayed for unchanged files |
| `emit_signatures` | per-output JS/DTS/map hashes |
| `latest_changed_dts_file` | fast-invalidation hook for project references |
| `options: BuildInfoOptions` | target/module/declaration/strict — a change forces a full rebuild |
| `build_time` | wall-clock stamp |

`BuildInfo::load` (incremental.rs) is the compatibility gate. It returns
`Ok(None)` — *not* an error — when either `version != BUILD_INFO_VERSION` or
`compiler_version != env!("CARGO_PKG_VERSION")`. A returned `None` means
"start fresh": a `tsz` upgrade silently discards the old build info rather than
risk replaying stale hashes computed by a different hashing algorithm.

File versions are content hashes. `compute_file_version` (incremental.rs) reads
the file bytes and runs them through `std::collections::hash_map::DefaultHasher`,
formatting the `u64` as `{hash:016x}`. `compute_export_signature(&[String])` in
the same file is a *separate, simpler* hash over a flat list of export-name
strings — note this is **not** the same as the structural
`ExportSignature::from_input` used by the in-process cascade (covered below);
the disk format keeps a coarser export hash.

### Where the build-info file lands

`default_build_info_path(config_path, out_dir, root_dir)` (incremental.rs)
mirrors tsc's `getTsBuildInfoEmitOutputFilePath`:

- `outDir` + `rootDir`: resolve `outDir + relative(rootDir, config-extless)`.
  When the config sits *outside* `rootDir` (the common `tsconfig.json` at root,
  sources under `rootDir: "src"`), the relative path starts with `..` and
  collapses back outside `outDir` — exactly tsc's quirk.
- `outDir` only: `outDir/<config-name>.tsbuildinfo`.
- neither: alongside the config file.

`strip_json_extension` removes a trailing `.json`; `normalize_path` collapses
`..`/`.` syntactically via
`tsz_common::module_resolution::path_identity::normalize_segments` (no
filesystem access), with a `.` fallback so the `.tsbuildinfo` suffix is always
appended to a real path component. The actual gate for *whether* to read/write
build info is `get_build_info_path` in `driver/core.rs`: it returns `None`
unless `options.incremental` is set or `tsBuildInfoFile` is explicit; an
explicit `tsBuildInfoFile` overrides the computed default.

### `ChangeTracker`: disk-format diffing

`ChangeTracker` (incremental.rs) compares the current file set against a loaded
`BuildInfo`. `compute_changes` (and `compute_changes_with_base`, which
normalizes absolute paths against `base_dir` first) classifies each file into
`new_files`, `changed_files`, `deleted_files`, and the union `affected_files`.
For each `changed`/`deleted` file it expands `BuildInfo::get_dependents` (a
linear scan of the `dependencies` map filtering for entries that import the
file) and adds those dependents to `affected_files` if they still exist. This is
the disk-format analogue of the in-memory reverse-dependency cascade.

## `CompilationCache`: the live watch-session memory

`CompilationCache` (`crates/tsz-cli/src/driver/core.rs`, `#[derive(Default)]`)
is the in-process heart of incremental rebuilds. Its fields, each keyed by
absolute `PathBuf`:

| Field | What it caches | Cleared/invalidated by |
| --- | --- | --- |
| `type_caches: FxHashMap<PathBuf, TypeCache>` | the checker's per-file type results (symbol/node types, etc.) | `invalidate_paths*`, dependent cascade |
| `bind_cache: FxHashMap<PathBuf, BindCacheEntry>` | `BindResult` + its content `hash` | hash mismatch in `build_program_with_cache` |
| `dependencies: FxHashMap<PathBuf, Vec<PathBuf>>` | per-file imports, in **discovery order** | `update_dependencies` each build |
| `reverse_dependencies: FxHashMap<PathBuf, FxHashSet<PathBuf>>` | `dep -> {importers}` | rebuilt by `update_dependencies` |
| `diagnostics: FxHashMap<PathBuf, Vec<Diagnostic>>` | per-file cached diagnostics | replayed for unchanged files |
| `export_hashes: FxHashMap<PathBuf, u64>` | last `ExportSignature` per file | the change predicate |
| `import_symbol_ids` | per-importer `{dep -> [SymbolId]}` | granular symbol-level invalidation |
| `star_export_dependencies` | files reached via `export *` | granular invalidation special case |
| `outfile_bundle_dependencies` | `outFile` bundle membership | `update_dependencies` |
| `cached_merged_program: Option<Arc<MergedProgram>>` | the whole merged program | no-op fast path |
| `cached_file_count: usize` | file count when the merged program was cached | detects file add/remove |

`BindCacheEntry { hash: u64, bind_result: BindResult }` is the unit of bind
caching: the `hash` is `hash_text_with_language_version(text, target)`, so a
language-version change (e.g. `target` flips) invalidates every bind entry even
if file bytes are identical.

### Dependency ordering is load-bearing

The `dependencies` list is stored in **source-import (discovery) order**, and
`build_info_to_compilation_cache` preserves that order when restoring from disk.
This is not cosmetic: a cached project rebuild replays BFS discovery in the same
order as the original fresh build, which keeps global `SymbolId` assignment
stable across the merge. Reordering would shift symbol ids and produce
order-dependent divergence. See the doc comment on `dependencies` and the
restore loop in `build_info_to_compilation_cache`.

### Three invalidation strategies

`CompilationCache` exposes a graduated set of invalidation methods, from
coarsest to finest:

- `clear()` — drops everything, including `cached_merged_program` and
  `cached_file_count`. Used when watch detects a *config* change
  (`WatchState::needs_full_rebuild`).
- `invalidate_paths(paths)` — removes the named files' entries from every
  per-file map. No transitive expansion. This is the *first* pass in
  `compile_with_cache_and_changes`: the changed file itself is dropped so it
  gets re-parsed and re-bound.
- `invalidate_paths_with_dependents(paths)` — expands `paths` through
  `collect_dependents` (a BFS over `reverse_dependencies`) and drops *all*
  affected files wholesale.
- `invalidate_paths_with_dependents_symbols(paths)` — the **granular** path.
  For each affected dependent that is *not itself* one of the changed files, it
  tries to invalidate only the symbols that actually flow from the changed file
  (via `import_symbol_ids`), calling `TypeCache::invalidate_symbols(&roots)`
  instead of dropping the whole `TypeCache`. If the dependent reached the change
  through `export *` (`star_export_dependencies`), it conservatively clears only
  `node_types`; if there is no recorded symbol import at all, it drops the whole
  `TypeCache`. This is what
  `compile_with_cache_and_changes` uses for the dependent pass.

`collect_dependents` is the BFS engine shared by these methods: a `VecDeque`
worklist seeded with the changed paths, walking `reverse_dependencies` until no
new files are discovered, returning the transitive closure (including the seeds).

```
collect_dependents({b.ts}):
  reverse_dependencies = { b.ts -> {a.ts}, a.ts -> {main.ts} }
  worklist: [b.ts] -> visit -> push a.ts
            [a.ts] -> visit -> push main.ts
            [main.ts] -> visit -> (no importers)
  result: {b.ts, a.ts, main.ts}
```

### `TypeCache::invalidate_symbols`: symbol-level pruning

The finest grain lives in the checker crate:
`TypeCache::invalidate_symbols(&mut self, roots: &[SymbolId]) -> usize`
(`crates/tsz-checker/src/context/core.rs`). It builds a reverse map from the
`symbol_dependencies` graph (`symbol -> referenced symbols`), BFS-expands the
`roots` to every transitively-dependent symbol, then removes those symbols from
`symbol_types`, `symbol_instance_types`, and `symbol_dependencies`. It also
clears the node-keyed and class-keyed caches wholesale (`node_types`,
`class_instance_type_cache`, `class_constructor_type_cache`,
`class_instance_type_to_decl`) because those are keyed by `NodeIndex`/`TypeId`
and cannot be selectively pruned by symbol. It returns the count of affected
symbols. This is the boundary where the driver hands a *symbol set* to the
kernel and the kernel decides what type state survives.

## Watch mode: from a keystroke to a recompile

`crates/tsz-cli/src/commands/watch.rs` is the watch entry point, dispatched from
`tsz.rs` as `Command::Watch => watch::run(&args, &cwd)`. The control flow:

```
watch::run
  -> WatchState::new            (load tsconfig, compute watch roots + filters)
  -> print_watch_start          (TS6031 "Starting compilation in watch mode...")
  -> compile_and_report(None)   (full initial build)
  -> create_watcher             (notify native or polling)
  -> watcher.watch(root, Recursive) for each watch root
  -> loop:
       rx.recv_timeout(DEBOUNCE_TICK = 50ms)
         -> handle_event        (filter + debounce-record)
       debouncer.flush_ready(now)?
         -> print_watch_change  (TS6032 "File change detected...")
         -> compile_and_report(Some(changed_paths))
```

### `WatchState` and its collaborators

`WatchState` holds `base_dir`, `watch_roots`, a `WatchFilter`, a `Debouncer`,
and the long-lived `type_cache: CompilationCache`. `WatchState::new` loads the
project (`load_project_state`), resolves explicit `--files`, collects watch
roots (`collect_watch_roots` — the base dir plus the parent of each explicit
file), and computes the ignore set (`compute_ignore_dirs` — `DEFAULT_EXCLUDES`
plus `outDir`/`declarationDir`, plus any `--excludeDirectories`).

`WatchFilter::should_record(path)` is the predicate that decides whether a
filesystem event is worth acting on. In order: skip if the path is one we *just
emitted* (`last_emitted`, prevents emit-triggers-rebuild loops); always record
a config change (the `project_config` or any `tsconfig.json`); skip ignored
dirs, default-excluded dirs (`node_modules`, `bower_components`, ...),
`--excludeFiles`, and non-TS files; finally, when explicit `--files` are set,
only record those files. The `last_emitted` set is refreshed every rebuild via
`WatchState::update_emitted`, which also calls `debouncer.remove_paths` so a
fresh emit can't re-trigger a queued recompile.

### Debouncing

`Debouncer` (watch.rs) coalesces a burst of events (editors often write a file
several times). `record_at(now, path)` inserts into a `pending` set and stamps
`last_event_at`. `flush_ready(now)` returns `Some(drained_paths)` only once
`now - last_event_at >= delay` (`DEFAULT_DEBOUNCE = 200ms`) and `pending` is
non-empty. The main loop polls every `DEBOUNCE_TICK = 50ms`, so the worst-case
latency from last keystroke to recompile is roughly `delay + tick`. Polling
intervals for the polling watcher are tsc-matched constants
(`FIXED_POLLING_INTERVAL = 250ms`, etc., selected by `--watchFile`/
`--fallbackPolling` in `create_watcher`).

### Native vs polling watcher

`create_watcher` picks between a `RecommendedWatcher` (native OS FS events) and
a `PollWatcher`, wrapped in the `WatcherImpl` enum. `--watchFile` polling
strategies force `PollWatcher`; the default tries the native watcher and falls
back to polling on failure (printing a warning). Both feed a single
`mpsc::channel` of `notify::Result<Event>`.

### Full-rebuild vs incremental decision

`WatchState::compile_and_report` is where the two in-process paths diverge:

```rust
let needs_full_rebuild = changed_paths.is_some_and(|p| self.needs_full_rebuild(p));
if needs_full_rebuild { self.type_cache.clear(); }

let result = if needs_full_rebuild || changed_paths.is_none() {
    driver::compile_with_cache(args, cwd, &mut self.type_cache)
} else if let Some(changed) = changed_paths {
    driver::compile_with_cache_and_changes(args, cwd, &mut self.type_cache, changed)
} else { ... };
```

`needs_full_rebuild` is true when any changed path is the active config
(`is_config_path` — the explicit `--project` config, else any `tsconfig.json`).
A config edit can change *anything* (lib, target, include globs), so the cache
is `clear()`ed and a full `compile_with_cache` runs. Otherwise the changed
paths drive `compile_with_cache_and_changes`. The initial build (`changed_paths
== None`) also takes `compile_with_cache`, populating the cache for the first
time.

After compiling, the console is cleared with the ANSI sequence
`\x1B[2J\x1B[3J\x1B[H` (unless `--preserveWatchOutput`), diagnostics are
rendered via `Reporter`, emitted files are recorded into the filter's
`last_emitted`, and `print_watch_complete` prints TS6194 ("Found N errors.
Watching for file changes."). Timestamps use tsc's `h:mm:ss tt` 12-hour format
via `format_watch_timestamp` (libc `localtime_r` on Unix).

## The incremental compile path

`compile_with_cache_and_changes` (`driver/core.rs`) is the function watch mode
calls for an edit. It runs the compile in up to **two passes**, with an
export-signature comparison deciding whether the second pass is needed:

```
compile_with_cache_and_changes(changed_paths):
  1. canonicalize changed_paths
  2. snapshot old_hashes = export_hashes[changed]      (before invalidation)
  3. cache.invalidate_paths(changed)                   (drop changed-file entries only)
  4. PASS 1: compile_inner(changed_dirty = changed)    (re-check changed files)
  5. compare old_hashes vs new export_hashes[changed]:
        all equal  -> no dependents need rechecking -> return
        any changed -> compute dependents:
                         --assumeChangesOnlyAffectDirectDependencies ?
                           direct dependents only
                         : collect_dependents (transitive)
  6. cache.invalidate_paths_with_dependents_symbols(changed)  (granular)
  7. PASS 2: compile_inner(forced_dirty = dependents)  (re-check dependents)
  8. attach InvalidationSummary[] and return
```

Pass 1 re-parses, re-binds, and re-checks the *changed* files. Their new
`ExportSignature` is computed and stored into `export_hashes`. The crucial
parity-and-perf decision is step 5: **if a file's public API hash is unchanged,
its dependents keep their cached diagnostics and are never re-checked.** A
function-body edit, a comment, whitespace, or a private-symbol change does not
change `ExportSignature`, so dependents are skipped. Only a public-API change
(an exported type's shape, a new/removed export, a re-export retarget) triggers
pass 2.

The per-file `InvalidationSummary` (`tsz_lsp::export_signature`) records
`api_changed`, `old_signature`, `new_signature`, and `dependents_invalidated`
for each changed file. It is surfaced in the `CompilationResult` and used by
perf tooling / `--extendedDiagnostics`; it does not affect diagnostics output.

`--assumeChangesOnlyAffectDirectDependencies` (the tsc flag) narrows the
dependent set to one level (`reverse_dependencies` lookups, no transitive BFS),
trading soundness for speed exactly as tsc does.

### Inside `compile_inner`: cache threading

`compile_inner` (`driver/core_diagnostics.rs`) is the shared compile body for
*all* entry points (`compile`, `compile_with_cache`,
`compile_with_cache_and_changes`, `compile_project`). Its cache handling:

- If a `CompilationCache` was passed in (watch path), it is used directly.
- Else if `cache.is_none() && resolved.incremental`, it *loads*
  `.tsbuildinfo` from disk: `BuildInfo::load` -> `build_info_to_compilation_cache`
  produces a `local_cache`, and `should_save_build_info = true` schedules a
  save at the end. `prior_latest_changed_dts_file` is carried forward so a
  no-emit incremental save preserves `latestChangedDtsFile` (tsc parity).
- Else: no cache, full fresh compile.

This is the mutual exclusion: an explicit `CompilationCache` (watch) suppresses
the `.tsbuildinfo` read/write; `.tsbuildinfo` only activates for non-watch
incremental compiles.

The program is built by `build_program_with_cache(sources, cache, lib_files,
target)`:

1. For each source, compute `hash_text_with_language_version`. If
   `bind_cache[path].hash == hash`, the file is `cached_ok` and **not
   re-parsed**; otherwise it is marked dirty and queued for parse+bind.
2. Dirty files are parsed and bound in parallel
   (`parallel::parse_and_bind_parallel_with_libs_and_target`) and their fresh
   `BindResult`s are written back into `bind_cache`.
3. `bind_cache` is `retain`ed to the current path set (drops deleted files).
4. **No-op fast path:** if `nothing_to_parse` *and*
   `meta.len() == cache.cached_file_count` *and* a `cached_merged_program`
   exists, the cached `Arc<MergedProgram>` is returned via `Arc::clone` with an
   empty `dirty_paths` — the entire `O(total_symbols)` merge is skipped. This is
   what makes a repeated rebuild over an unchanged graph nearly free.
5. Otherwise `parallel::merge_bind_results_ref(&ordered)` re-merges (in the
   stable dependency order), and the result is cached as `cached_merged_program`
   with `cached_file_count = ordered.len()`.

After building the program, `update_import_symbol_ids` re-derives the
per-importer `import_symbol_ids` and `star_export_dependencies` maps used by the
granular symbol invalidation, and `update_dependencies` rebuilds both
`dependencies` and the `reverse_dependencies` index from the freshly resolved
import graph.

### The smart-invalidation work queue (checking phase)

The checking phase in `driver/check.rs` is where "which files actually get
type-checked" is decided. A `work_queue: VecDeque<usize>` and
`checked_files: FxHashSet<usize>` are seeded by walking every file: a file is
queued (`needs_check`) iff the cache has **no** `type_caches` entry for it.
Files whose `TypeCache` survived invalidation are *not* queued — their cached
diagnostics are replayed at the end.

For the cached/sequential path, the queue is re-ordered into
dependency-first order (`topological_file_order`) so a dependency's fresh
`export_hashes` entry is available before its dependent is checked. Then the
driver drains the queue. After checking each file it computes the file's new
signature with `compute_export_signature(program, file, file_idx)`
(`driver/check_utils.rs`, which builds an `ExportSignatureInput` from the merged
program and calls `ExportSignature::from_input`) and compares against the cached
`export_hashes`. The cascade:

```rust
c.type_caches.insert(file_path, checker.extract_cache());
c.diagnostics.insert(file_path, file_diagnostics.clone());
c.export_hashes.insert(file_path, new_hash);

if old_hash != Some(new_hash) {
    for dep_path in c.reverse_dependencies[&file_path] {
        if checked_files.insert(dep_idx) {
            work_queue.push_back(dep_idx);   // schedule dependent
            c.type_caches.remove(dep_path);  // force its recheck
            c.diagnostics.remove(dep_path);
        }
    }
}
```

So even *within a single compile pass*, a changed export signature pushes
dependents onto the same work queue and drops their cached results — a dynamic
cascade, not a precomputed set. Files never enqueued keep their cached
diagnostics, which are gathered at the end:

```rust
for file in &program.files {
    if let Some(cached) = c.diagnostics.get(&PathBuf::from(&file.file_name)) {
        diagnostics.extend(cached.clone());   // replay verbatim
    }
}
```

This is the parity guarantee: an unchanged file's diagnostics are the *exact
bytes* produced when it was last checked, never re-derived and never mutated.
Finally the cache is pruned to `used_paths` (`type_caches`, `diagnostics`,
`export_hashes` all `retain`ed) so deleted files leave no stale entries.

## File-session reuse: re-targeting one checker across files

The work queue above describes *which* files to check. Orthogonally, the driver
can check many files on a **single long-lived `CheckerState`** rather than
constructing a fresh checker per file. This amortizes the expensive
`ProgramContext::apply_to` setup (shared `DefinitionStore`, global symbol
indices, resolved-module maps, boxed-lib priming) across `files / N` files. The
kernel primitive that makes this safe is `CheckerContext::switch_to_file`
(`crates/tsz-checker/src/context/file_session_reset.rs`).

For the deeper structure of `CheckerContext` and what its many caches hold, see
[checker-context-and-state](checker-context-and-state.md); this section covers
only the *reset boundary* used by reuse.

### `reset_for_next_file` then `switch_to_file`

`switch_to_file(arena, binder, file_name, file_idx)` runs in two stages:

1. `reset_for_next_file()` drains **file-local** state that would otherwise leak
   into the next file: the diagnostic buffer and `diagnostic_indices`,
   `NodeIndex`-keyed caches (`request_node_types`, class instance/constructor
   caches), resolution stacks, depth counters (`call_depth`, `recursion_depth`,
   `instantiation_depth`, ...), reachability flags (`is_unreachable`,
   `label_stack`), and the per-thread resolution/enum memos
   (`reset_per_file_resolution_guards`). The symbol-resolution stack/set are
   `debug_assert!`'d empty rather than force-cleared — a non-empty stack at the
   boundary is a logic bug in the prior file's check, not a value worth hiding.

2. `switch_to_file` then re-points the `&'a arena` and `&'a binder` references at
   the next file, updates `current_file_idx`/`file_name`, rebuilds the
   `FlowGraph` from the new binder's flow nodes, and — critically — **clears the
   `SymbolId`-keyed caches**.

### Why `SymbolId`-keyed caches must be cleared on a binder swap

This is the subtle correctness rule the whole reuse path hinges on, documented
inline at length in `switch_to_file`. Each per-file `BinderState` allocates
`SymbolId`s starting from 0 (no `base_offset` in production binders). So
`SymbolId(N)` in the prior file's binder is a *different symbol* than
`SymbolId(N)` in the next file's binder. Holding the prior file's
`symbol_types[N]` across the swap would return the wrong type for the next
file's symbol. The fix is to reallocate the symbol caches fresh
(`SymbolTypeCache::with_capacity(binder.symbols.len())`) on every
`switch_to_file`, and to clear every cache whose **keys or values** carry
file-local `SymbolId`s: `symbol_to_def`/`def_to_symbol` (a stable-`DefId` key
whose *values* are still file-local symbols), the string-keyed
`symbol_name_candidates_cache` (a `"Leaf5" -> SymbolId` map whose values decode
against the wrong binder), `lib_delegation_cache`, `var_decl_types`, the
`NodeIndex`-keyed `member_access_info_cache`, and more.

The doc comment records the concrete witness: the T2.1.B wire-up PR
(`#5643`) on `monorepo-001` emitted 22% extra diagnostics, then residual
TS2820 spelling-suggestion divergence (`"leaf-5" → "leaf-4"` flag-off vs
`"leaf-4" → "leaf-2"` flag-on), precisely because stale `SymbolId`s resolved a
`Leaf5` reference to a different file's interface shape. After the swap,
`warm_local_caches_from_shared_store()` re-warms the now-empty `SymbolId`-keyed
caches from the **shared** `DefinitionStore` (resetting the `local_caches_warmed`
gate first), repopulating cross-file `DefId -> SymbolId` mappings the next
file's check assumes are present. Caches keyed by stable `DefId` whose *values*
are program-stable (`def_type_params`, `lib_type_resolution_cache`,
`shared_lib_type_cache`) are intentionally preserved — those are exactly the
allocations reuse exists to amortize.

### The driver wire-up and reuse policy

`check_files_sequentially_with_reuse` (`driver/check_file.rs`) implements the
loop: it lazily constructs one `CheckerState` on the first non-skipped file
(`with_options_deferred_def_store` + `apply_to` + `prime_boxed_types`), then on
every subsequent file calls `state.ctx.switch_to_file(...)` instead of
rebuilding. `check_files_in_parallel_chunks_with_reuse` runs the same loop per
rayon chunk; the bounded `TSZ_CHECKER_POOL` path distributes files across a
fixed pool of long-lived checkers by estimated cost.

Whether reuse is used at all is a *policy* decision in `driver/check.rs`,
deliberately conservative because reuse helps small projects but regresses at
scale:

| Condition | Reuse? |
| --- | --- |
| `TSZ_DISABLE_FILE_SESSION_REUSE` set | off (takes precedence) |
| `TSZ_FILE_SESSION_REUSE` set | on |
| JS/JSX workload present | off |
| `<= 32` files (`FILE_SESSION_REUSE_SMALL_PROJECT_MAX_FILES`) | on (default) |
| larger TS-only batch | off (default) |

`file_session_reuse_from_workload` encodes this. The comment block records the
measured scale cliff that drove the default off for large projects: reuse is
1.5x faster at 101 files but 3.9x–5.4x *slower* at 1k–5k files, and a 10k-file
synthetic only finishes with reuse off. Reuse is also gated `!extract_type_cache`
(`use_file_session_reuse = use_sequential_checking && !extract_type_cache &&
reuse_requested`): when emit or declaration emit needs the per-file `TypeCache`
extracted (`extract_type_cache = !no_emit || emit_declarations`), the reuse loop
can't hand back per-file caches and is skipped. The LSP server does **not** use
this driver path — it reuses state through the `tsz-lsp` `Project` API
(see [lsp-and-wasm-surfaces](lsp-and-wasm-surfaces.md)).

## A concrete walk-through

Project: `main.ts` imports `a.ts`, which imports `b.ts`.

```
b.ts:   export const SECRET = 42;
a.ts:   import { SECRET } from "./b"; export function f() { return SECRET; }
main.ts:import { f } from "./a"; console.log(f());
```

Watch starts. Initial build (`compile_with_cache(None)`):
`build_program_with_cache` parses+binds all three, merges them, caches each
`BindResult` (with content hash) and each `TypeCache`, records
`export_hashes[b.ts/a.ts/main.ts]`, and builds
`reverse_dependencies = { b.ts -> {a.ts}, a.ts -> {main.ts} }`.

### Edit 1 — change a function body only

Edit `a.ts` to `return SECRET + 1;`. The editor write fires a `notify` event;
`WatchFilter::should_record` accepts `a.ts` (it is a TS file, not emitted, not
excluded); `Debouncer` coalesces and after 200ms `flush_ready` returns
`[a.ts]`. `needs_full_rebuild` is false (not a config path), so
`compile_with_cache_and_changes(["a.ts"])` runs:

1. `old_hashes[a.ts]` snapshotted.
2. `invalidate_paths(["a.ts"])` drops `a.ts`'s bind/type/diag entries.
3. Pass 1: `build_program_with_cache` finds `b.ts` and `main.ts` `cached_ok`
   (hash unchanged, **not re-parsed**), re-parses+binds only `a.ts`, re-merges
   (the no-op fast path does *not* fire because `dirty_paths` is non-empty),
   re-checks `a.ts`. `f`'s *body* changed but its exported *signature*
   (`f: () => number`) did not, so `compute_export_signature(a.ts)` yields the
   same `u64`.
4. Step 5: `old_hash == new_hash` for `a.ts` -> `any_exports_changed = false`
   -> **return immediately**. `main.ts`'s cached diagnostics are replayed
   verbatim; it is never re-checked.

### Edit 2 — change a public export

Edit `b.ts` to `export const SECRET = "42";` (number -> string). Now:

1. Pass 1 re-checks `b.ts`. Its exported value's *type* changed, so
   `ExportSignature(b.ts)` differs from `old_hashes[b.ts]`.
2. `any_exports_changed = true`. Dependents are computed via
   `collect_dependents(["b.ts"])` = `{b.ts, a.ts, main.ts}`.
3. `invalidate_paths_with_dependents_symbols(["b.ts"])`: `a.ts` imported
   `SECRET` from `b.ts` (`import_symbol_ids[a.ts][b.ts] = [SymbolId(SECRET)]`),
   so only the `SECRET`-dependent symbols of `a.ts`'s `TypeCache` are pruned via
   `invalidate_symbols`, not the whole file.
4. Pass 2: `compile_inner(forced_dirty = {a.ts, main.ts})` re-checks the
   dependents. `a.ts`'s `f` now infers `() => string`; if any consumer asserted
   `number`, the fresh TS-error surfaces — identical to a full `tsc` rebuild.

### Edit 3 — change `tsconfig.json`

Editing the config makes `WatchFilter::should_record` accept it (config always
records) and `needs_full_rebuild` returns true. `type_cache.clear()` wipes the
entire `CompilationCache` (including `cached_merged_program`), and a full
`compile_with_cache` rebuilds from scratch — the only safe response to a
target/lib/include change.

## Caches and invariants

| Cache / field | Invariant | Invalidation trigger |
| --- | --- | --- |
| `bind_cache[path]` | valid iff `hash == hash_text_with_language_version(text, target)` | byte change or target/language-version change |
| `cached_merged_program` | reused only when `nothing_to_parse && file_count unchanged` | any dirty path or file add/remove |
| `export_hashes[path]` | last computed `ExportSignature::from_input` | recomputed after each file check |
| `type_caches[path]` | replayed only for files not in the work queue | dropped on import-signature change of a dependency |
| `diagnostics[path]` | replayed **verbatim** for unchanged files | dropped alongside `type_caches[path]` |
| `dependencies` order | source-import (discovery) order, preserved on disk restore | rebuilt by `update_dependencies` |
| `SymbolId`-keyed checker caches | only valid within one binder's symbol namespace | cleared on every `switch_to_file` |
| `.tsbuildinfo` | usable only if `version` **and** `compiler_version` match | `BuildInfo::load` returns `None` -> fresh build |

Key cross-cutting invariants:

- **Diagnostics are never re-synthesized for unchanged files.** They are stored
  and replayed byte-for-byte, so an incremental build's diagnostic stream for an
  unchanged file is identical to its last full check (the parity guarantee).
- **Symbol identity is binder-local.** Any cache whose key *or value* is a
  `SymbolId` is only valid for one `BinderState`; reuse across binders requires a
  full clear plus re-warm from the shared `DefinitionStore`.
- **A config change is always a full rebuild.** No attempt is made to
  incrementally honor a `target`/`lib`/`include` change.
- **The export signature is position-independent.** `ExportSignatureInput`
  carries no `NodeIndex`, `SymbolId`, or byte offsets — only names, flags, and
  structural relationships — so a whitespace/comment/reorder edit that shifts
  offsets but not the public API does not invalidate dependents.

## Edge cases and tsc parity

- **`compiler_version` bump invalidates `.tsbuildinfo`.** `BuildInfo::load`
  treats a `tsz` version mismatch as `Ok(None)` (fresh build), guarding against
  replaying hashes computed by a different algorithm.
- **No-emit incremental save preserves `latestChangedDtsFile`.** tsc seeds the
  new build info with the old value and only reassigns it when a declaration
  file is actually written; `compilation_cache_to_build_info` carries
  `prior_latest_changed_dts_file` forward for exactly this.
- **`tsBuildInfoFile` without `incremental` does not read/write build info.**
  `get_build_info_path` returns a path, but `compile_inner` only *loads* build
  info under `resolved.incremental`; a standalone `tsBuildInfoFile` is a path
  hint, not an activation switch.
- **`--assumeChangesOnlyAffectDirectDependencies`** narrows the dependent set to
  one level in `compile_with_cache_and_changes`, matching tsc's same-named flag.
- **Emit must not re-trigger a rebuild.** `WatchState::update_emitted` records
  emitted outputs into `WatchFilter::last_emitted` and removes them from the
  debouncer, so writing `.js`/`.d.ts` files does not loop. `compute_ignore_dirs`
  also excludes `outDir`/`declarationDir` from the watch.
- **`export *` dependents get coarse invalidation.** When a dependent reaches a
  changed file only through a wildcard re-export
  (`star_export_dependencies`), `invalidate_paths_with_dependents_symbols`
  cannot identify a precise imported-symbol set, so it conservatively clears the
  dependent's `node_types` (re-running expression checks) rather than dropping
  the whole `TypeCache`.
- **Watch timestamps match tsc.** `format_12h` reproduces tsc's `h:mm:ss tt`
  12-hour clock (12:00:00 AM at midnight, 12:00:00 PM at noon), and pretty mode
  wraps the timestamp in gray ANSI (`\x1b[90m...\x1b[0m`) like tsc v6.
- **File-session reuse never changes diagnostics.** The reuse path is a *perf*
  optimization gated behind the workload policy; `switch_to_file`'s clear-and-
  re-warm contract guarantees byte-identical output to the fresh-checker path
  (the explicit acceptance criterion in the `#5643` history). Where it cannot
  guarantee that (JS/JSX, emit needing `extract_type_cache`, large batches), it
  is disabled.

## Where to look next

- [end-to-end-timeline](end-to-end-timeline.md) — the single-compile pipeline
  this doc wraps in a rebuild loop.
- [driver-project-references-and-build-mode](driver-project-references-and-build-mode.md)
  — `tsc --build`, `references[]`, and per-project `.tsbuildinfo`.
- [module-resolution-engine](module-resolution-engine.md) — how the dependency
  graph that drives invalidation is discovered.
- [checker-context-and-state](checker-context-and-state.md) — the full
  `CheckerContext` cache inventory that `switch_to_file` resets.
- [checker-type-of-symbol-and-symbol-types](checker-type-of-symbol-and-symbol-types.md)
  — what lives in the `symbol_types` / `TypeCache` that reuse must keep coherent.
- [lsp-and-wasm-surfaces](lsp-and-wasm-surfaces.md) — the *other* state-reuse
  path (per-keystroke), which shares `ExportSignature` with this one.
