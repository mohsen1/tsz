# tsz vs tsgo Performance Analysis & Architectural Plan

> Source: external compiler-perf expert review of the April 2026 perf session.
> Status as of plan creation: 4 PRs landed (#1618, #1619, #1623, #1626) with PR #1626 giving a measured −5.4% on subset3. None of those address the underlying architectural issue; they were point optimizations on top of the wrong topology.

## The diagnosis

We are not looking at "Rust is slower than Go." We are looking at a **compiler-state topology bug**.

The biggest structural mismatch is this: **tsz creates per-file checker worlds and then recursively creates child checker worlds for cross-file symbols**. tsgo uses a bounded checker pool, keeps type universes checker-local, and only lets diagnostics/displayable outputs cross checker boundaries.

The 200×+ gap on `large-ts-repo` is not 30 % of any one phase. It's multiplicative duplication — per-file lib symbol merge, per-delegation child checker construction, per-checker re-resolution of the same lib types, plus a global interner that is shared between all this duplicated work and bottlenecks parallelism.

## The architecture I should aim for

```
Global (program-wide, immutable after build):
  - intrinsic type IDs
  - canonical symbols / declarations / files
  - string atoms
  - lib metadata

Per checker (bounded pool of N workers, not per file):
  - type arena
  - structural type interner            (no merge across checkers)
  - relation cache
  - instantiation cache
  - apparent / indexed / mapped / conditional caches
  - flow / narrowing caches

Cross-file communication:
  - checker → checker only via stable SymbolId / DefId queries
  - never pass a TypeId from one checker's interner to another
  - only diagnostics and displayable outputs escape a checker
```

No periodic merge phase. A merge reintroduces contention and complexity. tsgo's comment in `program.go` implies the same: do not mix checker-local types across checkers; only diagnostics and displayable outputs escape.

## The first milestone is NOT "beat tsgo"

It is:

```
large-ts-repo:
  no single worker stuck for 95% of check phase
  no child-checker explosion
  no global interner contention plateau
  peak (same-metric) memory under 2–3 GB
  tsz < 30s
```

Once we are under 30 s, the remaining work becomes normal compiler performance engineering. Right now, the profile is still telling us the semantic-work graph is shaped wrong.

## Realistic time budget

```
706s -> 70s : remove catastrophic duplication
70s  -> 15s : checker-local interner/cache + no child checkers
15s  -> 5s  : resolver/VFS parallelism + shared lib/binder architecture
5s   -> 2.5s: match tsgo scheduling/memory locality
2.5s -> 1.2s: beat tsgo with better compactness, fewer allocations, better laziness, PGO
```

This is multiple rewrites, not a tuning pass.

## The PR sequence

Numbers are concrete, ordered. Do not skip ahead — earlier PRs unblock the data we need to drive later ones.

### PR 1 — Instrumentation (this branch)

Counters everywhere. No semantic changes. Behind a single env-flag (`TSZ_PERF_COUNTERS=1` or `--perf-counters`) so the production build doesn't pay the cost.

```
delegate_cross_arena_*           call count
delegate recursion               max depth, histogram of depths
CheckerState                     created count
CheckerState                     max-live count
copy_symbol_file_targets_to      call count
overlay entries                  copied per call (histogram)
overlay bytes                    copied per call (histogram)

compute_type_of_symbol           total calls
compute_type_of_symbol           unique SymbolId count
top SymbolIds                    by recomputation count

type_of_interface                calls
interface type cache             hit / miss
alias resolution                 hit / miss

TypeInterner:
  get_calls / insert_calls / hits / misses (totals)
  inserts_by_type_kind (histogram)
  bytes_by_type_kind (histogram)
  duplicate structural-hash hits (count)
  lock_wait_ns_by_shard (histogram, per shard)
  top 100 largest interned types (kind + structural summary + size)

Resolver:
  file_exists / dir_exists / read_dir / package_json call counts
  candidate paths per import (histogram)
  syscalls per resolved module (histogram)
  package_json cache hit / miss
```

Print at process exit when `--extendedDiagnostics` (or the new flag) is set. Then run on `large-ts-repo` to confirm where the bytes and calls go.

### PR 2 — Checker-local interner experiment

No merge phase. No cross-checker type mixing. Each per-file (eventually per-worker) checker gets its own structural interner. Only diagnostics / displayable outputs escape.

The compromise option, only if checker-local recomputation becomes too expensive, is "intern recipes globally, materialize locally":

```rust
enum TypeRecipe {
    SymbolDeclaredType(SymbolId),
    TypeReference { target: SymbolId, args: SmallVec<[TypeRecipeId; 4]> },
    IndexedAccess { object: TypeRecipeId, index: TypeRecipeId },
    // ...
}
```

But don't start there. Start with the simple split.

### PR 3 — No child-checker prototype

Introduce a cross-file `SymbolId` query API for `type_of_symbol` (initially backed by the existing code). Then at each `delegate_cross_arena_symbol_resolution` call site, replace the boxed child-checker construction with the query call. The query is memoized at the program (or per-file) level and returns a result that's safe for the caller to use *without* importing the child's type universe.

This is the structural fix. The 14 % of CPU in `copy_symbol_file_targets_to` clones disappears because the overlay is replaced with a query, not because the clone got cheaper.

### PR 4 — Shared lib / global binder

Stop merging lib symbols into every per-file binder. Use a lookup chain instead:

```
file locals -> module exports/imports -> program globals -> libs
```

The "merge" pattern duplicates `lib.d.ts` symbol tables 6086× on `large-ts-repo`. A lookup chain over an `Arc`'d shared lib binder is cheaper and has no fan-out.

### PR 5 — Overlay replacement

`cross_file_symbol_targets` is currently a per-checker `RefCell<FxHashMap<SymbolId, usize>>` that gets cloned on every cross-arena delegation. Replace with one of, in priority order:

1. Query-backed symbol target map (fits with PR 3's query API).
2. Parent/delta overlay: child holds a small delta and a parent pointer; flatten when delta exceeds a threshold.
3. Persistent HAMT only if lookup counts stay reasonable.
4. `Arc<HashMap> + make_mut` only if PR 1's instrumentation shows most children don't write. (Probably not — observed children do write — so this is the fallback, not the primary fix.)

### PR 6 — Resolver VFS cache + per-worker resolver state

Split resolver state:

```
ResolverShared:
  - immutable compiler options
  - canonical path functions
  - shared VFS cache:
      file_exists(path) / read_file(path) / read_dir(dir)
      package_json_info(dir) / realpath(path)
  - shared package-scope cache

ResolverWorker:
  - per-thread/per-worker resolver instance
  - local small caches
  - local scratch buffers
```

Then make `read_source_files` use a concurrent frontier:

```rust
while let Some(path) = queue.pop() {
    if seen.try_claim(path).is_err() { continue; }
    let source = fs.read_file_cached(path);
    let imports = scan_imports(source);
    for import in imports {
        let resolved = worker_resolver.resolve(import, path, &shared_vfs);
        queue.push(resolved);
    }
}
```

Record discovery index for deterministic ordering of final `SourceFile`s.

The biggest module-resolution win is **stop doing per-candidate syscalls**. Cache directory entries and package JSON by directory / package scope. tsgo's own perf history shows this: `microsoft/typescript-go#673` reports `ResolveModuleName` blocking during program creation and adds `sync.Map` caching for VFS `Stat`/`getEntries` for ~1.9× on Linux at GOMAXPROCS=16. Same shape applies here.

Don't make every resolver cache a `DashMap`. That turns one single-thread bottleneck into a many-thread lock bottleneck.

### PR 7 — Deterministic checker groups

Fixed checker pool of N workers. Weighted file assignment. Checker-local caches stay alive across the worker's batch of files.

This is the analogue of tsgo's `--checkers` flag. Compare:

```
tsgo --singleThreaded
tsgo --checkers 1 / 2 / 4 / 8

tsz single-thread
tsz current parallel
tsz parallel with global interner disabled/proxied
tsz with checker-local experimental interner
```

That comparison tells us how much of tsgo's win is parallelism versus single-thread throughput.

## What PR 1's data has to confirm before we commit to PR 2+

PR 1 lets us check the assumptions before each later PR pays for them:

- TypeInterner: are most inserts kind-X? Do shards bottleneck? What fraction is duplicate structural hashes?
- `compute_type_of_symbol`: what fraction of calls are unique SymbolIds vs recomputation?
- CheckerState: max live count and recursive depth.
- Resolver: candidate paths per import, syscalls per resolved module.
- Memory: bytes by type kind, top-N largest types, retained vs scratch.

If the data contradicts the diagnosis above, we revisit. PR 1 is the plan-changing PR.

## Profiling tools

```
CPU hot paths               samply, cargo flamegraph, Instruments Time Profiler
Off-CPU / blocked threads   Instruments Thread State Trace (macOS); perf sched / eBPF offcputime (Linux)
Lock contention             custom lock-wait histograms (PR 1); perf lock (Linux)
Resolver syscalls           fs_usage / DTrace (macOS); strace -c, perf trace, opensnoop / statsnoop (Linux)
Heap growth                 Instruments Allocations; heaptrack; DHAT; jemalloc/mimalloc stats
Cacheline / false sharing   perf c2c (Linux)
```

samply alone won't surface contention. PR 1's lock-wait histograms are how we see it without leaving macOS.

## Memory metric — normalize first

PR #1618 reports:

```
tsgo:  2.45s, 2.47 GB RSS / 16 MB peak footprint
tsz : 706s , 10.1 GB peak / 2.9 GB RSS
```

The "16 MB vs 10 GB" framing mixes RSS, live heap, allocator high-water, and platform "footprint." The gap is real, but the exact ratio needs a same-metric measurement. PR 1 includes a memory dumper that reports the same metric for both, and the bench wrapper records both.

## What this plan explicitly is NOT

- It is NOT "increase DashMap shards." That's a tuning pass and the data probably won't reward it more than ~1×.
- It is NOT "parallelize one more loop." Parallelization on top of the wrong topology gives ~5 % per attempt and plateaus.
- It is NOT "optimize the HashMap clone." That's a 14 % red flag, not the main target. The fix is to remove the overlay, not make the clone cheaper.

## Reference / further reading

- TypeScript 7 announcement (`--singleThreaded`, `--checkers`): https://devblogs.microsoft.com/typescript/announcing-typescript-7-0-beta/
- typescript-go program.go (checker-local types comment): https://github.com/microsoft/typescript-go/blob/main/internal/compiler/program.go
- typescript-go module-resolution caching issue: https://github.com/microsoft/typescript-go/issues/673
- rustc demand-driven query system: https://rustc-dev-guide.rust-lang.org/query.html
- rustc parallel compilation (worker-local arenas): https://rustc-dev-guide.rust-lang.org/parallel-rustc.html
- Salsa (incremental, demand-driven): https://github.com/salsa-rs/salsa
- Rust Performance Book — Profiling: https://nnethercote.github.io/perf-book/profiling.html
