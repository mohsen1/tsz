# Project References and Build Mode

## Orientation

This document covers the **solution build** layer of tsz: how `tsconfig.json`
project references are loaded into a reference graph, how `--build` (a.k.a.
`-b`, "build mode") walks that graph in topological order, the
`composite`/`declaration`/`incremental` constraints that gate referenceable
projects, the `.tsbuildinfo` up-to-date check that lets build mode skip
unchanged projects, and the `extends` config-merge timeline that every loaded
project goes through. All of this lives in the **driving layer**
(`tsz-cli` and `tsz-core/config`) — it is program orchestration, not type
algorithm. The build driver never asks the solver anything; it constructs one
*program per project* and hands each to the ordinary compile path.

This is a Wave-2 deep dive. It assumes you have read the boundary-level
sibling [driver-incremental-and-watch](driver-incremental-and-watch.md) (which
owns the single-program incremental `CompilationCache`, export-signature
invalidation, and the watch loop) and goes one layer up to the *multi-project*
orchestration that build mode adds on top. Where this doc says "compile the
project," that is the single-program pipeline described in
[end-to-end-timeline](end-to-end-timeline.md). Config option fan-out that ends
up in the checker/printer is summarized here only where `composite` touches it;
the full option-resolution surface is shared with
[module-resolution-engine](module-resolution-engine.md). Type-level
declaration emit (the `.d.ts` whose freshness drives the up-to-date check) is
owned by the [emitter](emitter.md).

## Owns / Must not own

| Owns | Must not own |
| --- | --- |
| Loading the reference graph from a root `tsconfig.json` (`refs.rs`) | Type relations, inference, narrowing (solver) |
| Topological build order (Kahn's algorithm over the reference DAG) | Declaration `.d.ts` content (emitter) |
| Project-reference constraint diagnostics: `TS6306`, `TS6310`, `TS6202` | The single-program incremental cache (sibling doc) |
| `--build` flag handling: `--clean`, `--dry`, `--force`, `--verbose`, `--stopBuildOnErrors` | Module-resolution algorithm internals |
| `.tsbuildinfo` up-to-date checks across referenced project boundaries | Excess/freshness/variance compatibility rules |
| `extends` resolution + config merge (`config/extends.rs`) | Per-file flow graph / CFA |
| `composite -> declaration + incremental` option fan-out (`config/option_fanout.rs`) | Emitter helper scheduling / source maps |
| `${configDir}` template substitution; path anchoring of inherited options | Diagnostic *rendering* (owned by `Reporter`) |

## Module map

| Path | Role |
| --- | --- |
| `crates/tsz-cli/src/project/refs.rs` | Reference graph: `ProjectReferenceGraph`, `ResolvedProject`, load/order/validate |
| `crates/tsz-cli/src/project/mod.rs` | Re-exports `fs`, `incremental`, `refs` |
| `crates/tsz-cli/src/project/incremental.rs` | `BuildInfo` (`.tsbuildinfo`), `ChangeTracker`, `default_build_info_path` |
| `crates/tsz-cli/src/project/fs.rs` | `FileDiscoveryOptions`, `discover_ts_files` (glob walk for a project's inputs) |
| `crates/tsz-cli/src/commands/build.rs` | `is_project_up_to_date`, `get_build_info_path`, referenced-output freshness |
| `crates/tsz-cli/src/bin/tsz.rs` | `handle_build`, `handle_build_clean`, `handle_build_single_project` (the build driver) |
| `crates/tsz-cli/src/bin/tsz/arg_preprocess.rs` | `--build`-must-be-first ordering (`TS6369`/`TS5023`), build-mode flag remapping |
| `crates/tsz-cli/src/driver/core.rs` | `compile_project` — compile one project by config path |
| `crates/tsz-core/src/config/mod.rs` | `load_tsconfig`, `TsConfig`, `TsConfigReference`, `ExtendsValue`, the `extends` recursion |
| `crates/tsz-core/src/config/extends.rs` | `resolve_extends_path`, `merge_configs`, `${configDir}` and path anchoring |
| `crates/tsz-core/src/config/parse.rs` | Per-option config diagnostics: `TS6304`, `TS6379` (composite constraints) |
| `crates/tsz-core/src/config/option_fanout.rs` | `apply_composite_implications` (`composite -> declaration + incremental`) |
| `crates/tsz-core/src/config/resolved_options.rs` | `ResolvedCompilerOptions`, `composite`/`declaration` field resolution |

All build/refs/incremental modules are re-exported from `tsz-cli/src/lib.rs`
under stable names: `project::refs as project_refs`, `project::incremental`,
`project::fs`, and `commands::build as build`.

## The reference graph

A *solution* is a tree of `tsconfig.json` files connected by a `references`
array. Each entry is a `TsConfigReference { path, prepend }` in the core config
type (`config/mod.rs`); the CLI's richer `ProjectReference { path, prepend,
circular }` (`refs.rs`) additionally carries the non-standard `circular` flag
used for gradual-migration escape hatches. References are deserialized from
`tsconfig.json` with serde's `rename_all = "camelCase"`, so the on-disk
`{ "path": "./pkg", "prepend": true }` maps directly.

```
                tsconfig.json (root, "files": [], references: [...])
                 |          |
        references[0]   references[1]
                 v          v
            packages/core   packages/utils
                 |               |
            references[]    references[]
                 v               v
               shared           shared   <-- diamond: visited once
```

### `ProjectReferenceGraph`

`ProjectReferenceGraph` (`refs.rs`) is the central data structure. It is a
classic adjacency-list DAG keyed by a dense `ProjectId = usize`:

```rust
pub struct ProjectReferenceGraph {
    projects: Vec<ResolvedProject>,                // ProjectId -> project
    path_to_id: FxHashMap<PathBuf, ProjectId>,     // canonical config path -> id
    references: FxHashMap<ProjectId, Vec<ProjectId>>, // forward edges (a -> deps)
    dependents: FxHashMap<ProjectId, Vec<ProjectId>>, // reverse edges (dep -> a)
}
```

`path_to_id` is keyed by the **canonicalized** config path. Canonicalization is
load-bearing for correctness: a diamond reference graph (two parents pointing
at one shared leaf via different relative paths) must collapse to one node, and
the only stable identity for "the same project" is the resolved absolute path.

### `ProjectReferenceGraph::load`

`load(root_config_path)` (`refs.rs`) builds the whole graph from a single root
`tsconfig.json` via an explicit-stack worklist:

1. Canonicalize the root config path (`std::fs::canonicalize`, wrapped with a
   `with_context` so a bad root path yields a readable error).
2. Push it on a `stack: Vec<PathBuf>`.
3. Pop a path; if already in `visited` (an `FxHashSet<PathBuf>`), skip.
   Otherwise insert into `visited`, call `load_project(&config_path)`, and
   `add_project` it.
4. For each *valid* resolved reference of that project, push its config path on
   the stack if unvisited.
5. After the worklist drains, call `build_edges()` to wire `references` and
   `dependents` from each project's `resolved_references`.

The comment in the source calls this "BFS," but the structure is a `Vec`-backed
stack popped with `stack.pop()`, so traversal order is actually LIFO
(depth-first). The order only affects `ProjectId` assignment, not the final
build order — `build_order()` recomputes a deterministic ordering downstream.

### `load_project` — one project resolved

`load_project(config_path)` (`refs.rs`) is the per-node loader. It does two
distinct parses of the same file, which is the key thing to understand:

- `parse_tsconfig_with_references(&source)` — a *shallow* serde parse into
  `TsConfigWithReferences` (the local `references` array plus the flattened
  base `TsConfig`). This reads the raw `references` exactly as written.
- `load_tsconfig(config_path)` — the **full** `extends`-resolving loader from
  `config/mod.rs` (covered below). This is what supplies the *effective*
  compiler options after inheritance.

The split matters for parity: `references` are read from the *local* file only
(they are never inherited through `extends`, matching tsc — see
`merge_configs` below), while `composite`/`noEmit`/`outDir`/`declarationDir`
are read from the *effective* (post-`extends`) options. A referenced project
that sets `"composite": true` only in a base config it extends is still treated
as composite. This is exercised by `test_inherited_composite_satisfies_ts6306`
in `project_refs_tests.rs`.

`load_project` produces a `ResolvedProject`:

```rust
pub struct ResolvedProject {
    config_path: PathBuf,          // canonicalized
    root_dir: PathBuf,             // canonicalized parent of config
    config: TsConfigWithReferences,
    resolved_references: Vec<ResolvedProjectReference>,
    is_composite: bool,            // effective compilerOptions.composite
    no_emit: bool,                 // effective compilerOptions.noEmit
    declaration_dir: Option<PathBuf>, // root_dir.join(declarationDir)
    out_dir: Option<PathBuf>,      // root_dir.join(outDir)
}
```

### Reference path resolution

`resolve_single_reference(root_dir, reference)` (`refs.rs`) turns a
`reference.path` string into a `ResolvedProjectReference`. The rule mirrors
tsc's `resolveProjectReferencePath`:

| Input shape | Resolved config path |
| --- | --- |
| Absolute path | used as-is |
| Relative path to a **directory** | `<dir>/tsconfig.json` |
| Relative path ending in `.json` | the file itself |
| Relative path (no extension, not a dir) | `<path>/tsconfig.json` (assumed dir) |

The result is canonicalized when possible. Existence is checked: a missing
target yields `is_valid: false` with an `error` string `"Referenced project not
found: …"` rather than aborting the whole graph load. Invalid references are
simply skipped when building edges, so a broken reference degrades gracefully
instead of poisoning the build.

## Build order: Kahn's algorithm

`ProjectReferenceGraph::build_order()` (`refs.rs`) returns a
`Vec<ProjectId>` in **dependency-first** order — a referenced project always
appears before the project that references it, so its `.d.ts` outputs exist by
the time a consumer compiles.

```
detect_cycles()  ->  if any cycle, bail!("Circular project references …")
        |
        v
in_degree[id] = number of edges *into* id  (count of references pointing at id)
        |
        v
queue: BinaryHeap of all id with in_degree == 0   (roots / leaves of the DAG)
        |
        v
while queue.pop():
    order.push(node)
    for neighbor in references[node]:   // node's dependencies
        in_degree[neighbor] -= 1
        if in_degree[neighbor] == 0: queue.push(neighbor)
        |
        v
order.reverse()   // dependencies first
```

Two implementation details are worth calling out:

- The worklist is a `BinaryHeap<ProjectId>`, not a plain queue. Because
  `ProjectId` is `usize`, the heap pops the **highest** id first among ready
  nodes. This gives a deterministic tie-break, so two solutions with the same
  shape produce the same order across runs.
- `in_degree` is computed over `references` edges (forward edges from a project
  to its dependencies). The algorithm peels off projects whose dependents are
  already placed, builds the list "consumers first," then `order.reverse()`s it
  so dependencies lead. The driver then walks the reversed list in order.

### Cycle detection

`detect_cycles()` runs a recursive DFS (`detect_cycles_dfs`) with a `visited`
set and a `rec_stack` (the current recursion path). When DFS reaches a node
already on `rec_stack`, it slices the cycle out of the `path` vector and records
it. `build_order()` calls this first and `bail!`s with a human-readable
`A -> B -> C` chain if any cycle exists, so a cyclic solution never reaches the
topological sort. The same cycles feed `TS6202` in `validate()` (below).

## Constraint validation

`ProjectReferenceGraph::validate()` (`refs.rs`) returns a
`Vec<ProjectReferenceDiagnostic>` enforcing the three solution-level rules.
These are graph-shape constraints, distinct from the per-config-file option
constraints in `config/parse.rs`.

| Code | Constant | Condition |
| --- | --- | --- |
| `TS6306` | `REFERENCED_PROJECT_MUST_HAVE_SETTING_COMPOSITE_TRUE` | A referenced project has `is_composite == false` |
| `TS6310` | `REFERENCED_PROJECT_MAY_NOT_DISABLE_EMIT` | A referenced project has `no_emit == true` |
| `TS6202` | `PROJECT_REFERENCES_MAY_NOT_FORM_A_CIRCULAR_GRAPH_CYCLE_DETECTED` | Any cycle from `detect_cycles()` |

The codes come from `tsz_common::diagnostics::diagnostic_codes` — the driver
never hardcodes the numeric value, only the named constant. `validate()` is
called in `handle_build` (`tsz.rs`) *before* `build_order()`; any non-empty
diagnostic list prints `error TS<code>: <message>` to stdout and exits with
`EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED`. Because `TS6306`/`TS6310` use the *effective*
(post-`extends`) `composite`/`noEmit`, a base config that turns a project
composite satisfies `TS6306` even though the leaf config never names it.

### Per-config composite constraints (`TS6304` / `TS6379`)

A second family of constraints is enforced when *parsing one config file*, in
`config/parse.rs`, independent of the graph:

- `TS6304` `COMPOSITE_PROJECTS_MAY_NOT_DISABLE_DECLARATION_EMIT` — fires when
  `composite` is effectively enabled and `declaration` is explicitly `false`.
  Anchored at the `declaration` key.
- `TS6379` `COMPOSITE_PROJECTS_MAY_NOT_DISABLE_INCREMENTAL_COMPILATION` — fires
  when `composite` is enabled and `incremental` is explicitly `false`.
  Following tsc, this one is anchored at the enclosing `"compilerOptions"` key
  (the block that holds both interacting options), not at `incremental`.

These check the *explicit* value (`Some(Value::Bool(false))`), so an
unspecified `declaration`/`incremental` that gets fanned out to `true` by
`composite` does *not* trip them — only an explicit disable conflicts.

## `composite` option fan-out

`composite: true` is shorthand for several other options. The single owner of
those implications is `apply_composite_implications` in
`config/option_fanout.rs`:

```rust
// tsc 6.0.3 computedOptions: declaration = declaration || composite,
//                            incremental  = incremental  || composite
const fn apply_composite_implications(resolved: &mut ResolvedCompilerOptions) {
    if resolved.composite {
        resolved.emit_declarations = true;
        resolved.checker.emit_declarations = true;
        resolved.incremental = true;
    }
}
```

This is dispatched from `apply_non_strict_fanout`, which both emit-capable
engines (the CLI `driver/plan.rs` lane and the tsconfig `resolved_options.rs`
lane) call after their per-flag overrides. The module doc-comment in
`option_fanout.rs` is explicit about *why* it exists: the two lanes used to
re-encode these implications independently and drifted. Routing both through
one declarative table guarantees that `composite` implies declaration emit and
incremental identically no matter how the options arrived. The fan-out is
monotone (every rule is a `|| toward true`), so it is idempotent — a second call
is a no-op, asserted by `idempotent_second_call_is_a_no_op` in that module's
tests.

The net effect for a composite project: declaration emit is on (so `.d.ts`
outputs exist for consumers to type against) and incremental is on (so a
`.tsbuildinfo` is written, which build mode's up-to-date check reads).

## The build driver: `handle_build`

`handle_build(args, cwd)` in `tsz.rs` is the build-mode entry point. The
data-flow is:

```
handle_build
  |
  |-- resolve tsconfig path (args.project or cwd/tsconfig.json)
  |     missing -> println! TS5083, exit EXIT_DIAGNOSTICS_OUTPUTS_SKIPPED
  |
  |-- ProjectReferenceGraph::load(root)
  |     Err -> warn + handle_build_single_project (fallback, no references)
  |
  |-- graph.validate()  -> TS6306/TS6310/TS6202 -> print + exit if non-empty
  |
  |-- if args.clean -> handle_build_clean(graph)   (delete artifacts, return)
  |
  |-- build_order = graph.build_order()
  |     Err (cycle) -> println! Error + exit
  |
  |-- if args.dry -> print "would build … in order" + return
  |
  +-- for project_id in build_order:           // dependency-first
        project = graph.get_project(id)
        if !args.force && build::is_project_up_to_date(project, args):
            skipped_count += 1; continue
        result = driver::compile_project(args, project.root_dir, project.config_path)
        count errors; if errors and args.stop_build_on_errors -> exit
```

Each project is compiled by `driver::compile_project` (`driver/core.rs`), which
funnels into `compile_inner(args, cwd, None, None, None, Some(config_path))` —
the same single-program compile path used everywhere else, just pinned to a
specific `tsconfig.json`. **Build mode does not run a shared multi-project
program**; it runs N independent programs in dependency order. A consumer
project sees its dependency's outputs because the dependency was compiled first
and its `.d.ts`/`.tsbuildinfo` are now on disk.

### Build-mode flags

The build flags live on `CliArgs` (`commands/args.rs`) and are gated to build
mode only:

| Flag | Field | Effect |
| --- | --- | --- |
| `--verbose` | `build_verbose` | Per-project progress lines ("Building:", "Up to date:") |
| `--dry`/`-d` | `dry` | Print the would-build order and exit without compiling |
| `--force`/`-f` | `force` | Skip the up-to-date check; rebuild every project |
| `--clean` | `clean` | Delete `.tsbuildinfo` + `outDir`/`declarationDir`, then exit |
| `--stopBuildOnErrors` | `stop_build_on_errors` | Exit on the first project with errors |

Outside build mode these flags are rejected with `TS5093` ("Compiler option
'--X' may only be used with '--build'"), enforced by the loop at `tsz.rs:546`
which also checks `explicitly_disabled_bool_flags` so even `--force=false`
trips the gate.

### `--build` must be first

`arg_preprocess.rs` enforces tsc's positional rule that `--build`/`-b` must be
the *first* command-line argument. If `--build` (long form) appears non-first,
it rejects with `TS6369` ("Option '--build' must be the first command line
argument."); if `-b` (short form) appears non-first it rejects with `TS5023`
("Unknown compiler option '-b'."), matching tsc, which only recognizes the
short form in the leading slot. When `--build` *is* first, the preprocessor
remaps the short flags into their build-mode meanings: `-v -> --build-verbose`
(not `--version`), `-d -> --dry`, `-f -> --force`.

### `--clean`

`handle_build_clean(graph, verbose)` (`tsz.rs`) iterates every project in the
graph and deletes, for each:

1. The `.tsbuildinfo` file at `get_build_info_path(project)` (which honors an
   explicit `tsBuildInfoFile` or the `outDir`-relocated default — see below).
2. `project.out_dir` (recursively) if present and existing.
3. `project.declaration_dir` (recursively) if present and existing.

It reuses the `ResolvedProject`'s already-resolved absolute `out_dir`/
`declaration_dir` rather than re-running option resolution, so `--clean` removes
exactly the directories the build would write. The historical bug fixed here was
always writing the `.tsbuildinfo` next to the tsconfig, which missed the
`outDir`-relocated location.

## The up-to-date check

`is_project_up_to_date(project, args)` in `commands/build.rs` is the heart of
incremental *solution* builds — it decides whether a project can be skipped.
This is distinct from the single-program incremental cache (which decides which
*files within one program* to recheck — see
[driver-incremental-and-watch](driver-incremental-and-watch.md)). Here the
granularity is a whole project, and the inputs are `.tsbuildinfo` files plus
filesystem mtimes.

```
is_project_up_to_date(project)
  |
  |-- build_info_path = get_build_info_path(project)   // None -> false
  |-- if !exists -> false   (never built)
  |-- BuildInfo::load(path):
  |       Ok(None)  -> false  (version mismatch -> must rebuild)
  |       Err(_)    -> false  (corrupt -> rebuild)
  |       Ok(Some(bi)) -> continue
  |
  |-- discover_ts_files(FileDiscoveryOptions::from_tsconfig(...))   // current inputs
  |-- ChangeTracker::compute_changes_with_base(bi, current_files, root_dir)
  |       tracker.has_changes() -> false   (a source file changed/added/deleted)
  |
  +-- are_referenced_projects_uptodate(project, bi, args)
          for each reference:
            load referenced project; read its BuildInfo
            if its latest_changed_dts_file mtime >= our build_time -> false
```

### `BuildInfo` and version gating

`BuildInfo::load` (`incremental.rs`) returns `Ok(None)` — meaning "incompatible,
rebuild" — in two cases:

- `build_info.version != BUILD_INFO_VERSION` (the `.tsbuildinfo` *format*
  changed; the constant is `"0.1.0"`).
- `build_info.compiler_version != env!("CARGO_PKG_VERSION")` (the compiler that
  wrote it differs from this binary).

The compiler-version gate is deliberate: any change to the hashing algorithm or
internal compile logic ships a new `CARGO_PKG_VERSION`, which invalidates every
stored `.tsbuildinfo` and forces a clean recheck. This is the safe default —
build mode would rather rebuild than trust a hash from a different compiler.

### Source-change detection

`ChangeTracker` (`incremental.rs`) compares the project's *current* discovered
files against the `file_infos` map stored in the `BuildInfo`. The
`compute_changes_with_base` variant normalizes absolute discovered paths to
paths relative to `root_dir` (because the `.tsbuildinfo` stores relative paths)
and classifies each file:

- **new** — a current file not in `file_infos`.
- **deleted** — a `file_infos` key with no current file.
- **changed** — a current file whose `compute_file_version` (a content hash via
  `DefaultHasher`) differs from the stored `version`.

`tracker.has_changes()` is the OR of these three sets; any one of them means the
project is stale. Notably, build mode discovers inputs with
`FileDiscoveryOptions::from_tsconfig` against the *configured* `files`/`include`
(passing the project's `out_dir` so emitted output is excluded), so it never
treats an unlisted stray `.ts` as a new root — only the project's declared
inputs count.

### Cross-project freshness

`are_referenced_projects_uptodate` (`commands/build.rs`) is the part unique to
solution builds. For each reference it loads the referenced project, reads its
`.tsbuildinfo`, and compares the referenced project's
`latest_changed_dts_file` mtime against *our* recorded `build_time` (a Unix
second). The project is stale (must rebuild) when:

- the referenced project has no `.tsbuildinfo` (not built yet) — return `false`;
- the referenced project's `.tsbuildinfo` is version-incompatible — `false`;
- the referenced `.d.ts` recorded in `latest_changed_dts_file` cannot be
  `stat`ed (deleted/replaced/unreadable) — `false` (issue #4753: a missing
  recorded `.d.ts` must not be silently treated as fresh);
- `dts_timestamp >= build_info.build_time` — `false`.

The `>=` (not `>`) comparison is a deliberate parity choice documented in the
source against issue #4754: at one-second resolution, "ref finished a
millisecond before us" is indistinguishable from "a millisecond after," and in
that ambiguity the only safe option is to rebuild. A referenced project whose
`.d.ts` mtime lands in the *same* Unix second as our `build_time` forces a
parent rebuild. The regression test
`is_project_up_to_date_returns_false_when_referenced_dts_matches_build_time_at_second_resolution`
pins exactly this.

A referenced project that records *no* `latest_changed_dts_file` (e.g. it
emitted nothing whose declaration changed) does **not** force a rebuild — that
path is skipped entirely, asserted by
`is_project_up_to_date_allows_referenced_project_without_latest_changed_dts_file`.

### `.tsbuildinfo` location

`get_build_info_path(project)` (`commands/build.rs`) and the underlying
`default_build_info_path(config_path, out_dir, root_dir)` (`incremental.rs`)
mirror tsc's `getTsBuildInfoEmitOutputFilePath`:

| Config | `.tsbuildinfo` path |
| --- | --- |
| explicit `tsBuildInfoFile` (non-empty) | `root_dir.join(tsBuildInfoFile)` |
| `outDir` + `rootDir` set | `outDir + relative(rootDir, configExtless) + ".tsbuildinfo"` |
| `outDir` only | `outDir/<config-name>.tsbuildinfo` |
| neither | alongside the config file (`<config>.tsbuildinfo`) |

The `outDir + rootDir` case can resolve back *outside* `outDir` when the config
sits above `rootDir` (the common layout — `tsconfig.json` at the project root,
sources under `rootDir: "src"`), because `relative(rootDir, config)` then starts
with `..`. `normalize_path` (delegating to
`tsz_common::module_resolution::path_identity::normalize_segments`) collapses
those segments syntactically without touching the filesystem.

## The `extends` timeline

Every project loaded by `load_project` goes through `load_tsconfig` in
`config/mod.rs`, which resolves the full `extends` chain. The recursion is in
`load_tsconfig_inner` (and the diagnostic-carrying twin
`load_tsconfig_inner_with_diagnostics`):

```
load_tsconfig(path)
  config_dir = canonical(parent(path))     // the leaf/inheriting config's dir
  load_tsconfig_inner(path, visited={}, inherited=false, config_dir)
        |
        |-- canonicalize(path); if in `visited` -> bail! "extends cycle"
        |-- parse_tsconfig(source)
        |-- substitute_config_dir_templates(&config, config_dir)   // ${configDir}
        |-- anchor_inherited_path_options(&config, path)           // baseUrl/outDir/...
        |-- if inherited: anchor_inherited_root_selectors(&config, path)  // files/include
        |
        |-- for each extends entry (in order):
        |       base_path = resolve_extends_path(path, entry)?      // None -> skip (+TS6053)
        |       base = load_tsconfig_inner(base_path, visited, inherited=true, config_dir)
        |       accumulated = merge_configs(accumulated, base)      // later base wins
        |
        +-- config = merge_configs(accumulated, config)            // child wins over bases
            visited.remove(canonical)
```

### Ordering of the transforms

The order of the three transforms is deliberate and is the part most likely to
break parity if reshuffled:

1. **`${configDir}` first** (`substitute_config_dir_templates`,
   `config/extends.rs`). The TypeScript 5.5 `${configDir}` template always
   resolves against the *leaf* config's directory (`config_dir`), never the base
   that wrote it — that is the whole point of the feature. The same `config_dir`
   is threaded unchanged through the entire `extends` recursion. Substitution
   rewrites a leading `${configDir}` into an absolute, lexically-normalized
   path, and only the *leading* token is honored (a mid-string `${configDir}` is
   left literal, matching tsc). It applies to root selectors (`files`/`include`/
   `exclude`) and to path-shaped compiler options (`baseUrl`, `outDir`,
   `rootDir`, `declarationDir`, `outFile`, `tsBuildInfoFile`, `rootDirs`,
   `typeRoots`, and `paths` substitutions).

2. **Anchor inherited path options** (`anchor_inherited_path_options`). tsc
   resolves `baseUrl` (and friends) relative to the config file that *declares*
   them. When a child extends a base, the base's relative `baseUrl: "."` must
   stay anchored at the *base's* directory, not the child's. Anchoring rewrites
   each still-relative path option to an absolute path off the declaring
   config's parent. It runs *after* `${configDir}` substitution precisely so it
   only fires on the leftover relative paths (absolute values, including ones
   `${configDir}` already produced, are skipped).

3. **Anchor inherited root selectors** (`anchor_inherited_root_selectors`),
   **only when `inherited == true`** — i.e. only for *base* configs, never the
   leaf. A base config's `"include": ["./global.d.ts"]` must be re-anchored at
   the base's directory; the leaf's own selectors are already relative to the
   leaf. `lexically_normalize_selector` collapses `.`/`..` while preserving glob
   metacharacters (`**`, `*.ts`) — it deliberately does *not* use
   `std::fs::canonicalize` (which would hit the filesystem and destroy globs) or
   the canonical `normalize_segments` (which clamps `..` at the root). The
   regression here was a base `"./global.d.ts"` becoming the unmatchable glob
   `<dir>/./global.d.ts`, producing a false `TS18003`.

### `resolve_extends_path`

`resolve_extends_path(current_path, extends)` (`config/extends.rs`) mirrors
tsc's `getExtendsConfigPath`:

- **Relative/absolute** (`./base`, `../base.json`, `/abs`) — resolved against
  the declaring config's directory via `probe_extends_candidate`, which tries
  the path as written, then with `.json` appended when extensionless. No
  directory lookup for relative specifiers (a relative `extends` must name a
  file, like tsc).
- **Non-relative** (`@tsconfig/node20`, `shared-config`) — Node module
  resolution. First the package's `package.json` `"exports"` map
  (`resolve_package_extends_path`, with conditions `["types", "node", "import",
  "require", "default"]`), then a `node_modules` walk up through ancestor
  directories that honors an explicit subpath (`pkg/base.json`), an
  extensionless subpath (`pkg/recommended -> recommended.json`), and a bare
  package whose root holds a `tsconfig.json`.

An `extends` that resolves to nothing is *recoverable*: the diagnostic-free
`load_tsconfig_inner` simply skips the missing base; the diagnostic-carrying
`load_tsconfig_inner_with_diagnostics` emits `TS6053` (`FILE_NOT_FOUND`)
anchored at the specifier via `find_extends_specifier_span` and continues with
the remaining options. tsc behaves identically — a missing base is not fatal.

### `merge_configs` — child wins, references don't inherit

`merge_configs(base, child)` (`config/extends.rs`) overlays a child onto a base:

```rust
TsConfig {
    extends: None,
    compiler_options: merge(base.compiler_options, child.compiler_options),
    include: child.include.or(base.include),   // child wins, base is fallback
    exclude: child.exclude.or(base.exclude),
    files:   child.files.or(base.files),
    references: child.references,               // NOT inherited from the base
}
```

Two parity rules are encoded here:

- **Compiler options merge field-by-field** via the `merge_options!` macro,
  which does `child.field.or(base.field)` for every `Option` field — the child
  value wins when present, the base fills in everything the child left unset.
  This is a *deep* per-key merge, not a wholesale replace.
- **`references` are never inherited** — only `child.references` survives a
  merge. tsc reads `references` from the local config only; a base config's
  `references` array is dropped. This is asserted by
  `merge_configs_references_only_from_child` in `extends.rs` and is *why*
  `load_project` reads `references` from the shallow parse rather than the
  effective config.

An `extends` *array* (`"extends": ["./a.json", "./b.json"]`, tsc 5.0+, modeled
by `ExtendsValue::Array`) is applied left-to-right: each base is merged into an
accumulator, so later entries override earlier ones, and finally the local
config overrides the whole accumulated base.

## A concrete walk-through

Solution layout:

```
proj/
  tsconfig.json              { "files": [], "references": [{ "path": "./app" }] }
  tsconfig.base.json         { "compilerOptions": { "composite": true, "strict": true } }
  app/
    tsconfig.json            { "extends": "../tsconfig.base.json", "files": ["main.ts"],
                               "references": [{ "path": "../lib" }] }
    main.ts
  lib/
    tsconfig.json            { "extends": "../tsconfig.base.json", "files": ["index.ts"] }
    index.ts
```

Running `tsz --build proj/tsconfig.json` executes:

1. `arg_preprocess.rs` confirms `--build` is first; build-mode remapping is
   inert (no short flags here).
2. `tsz.rs` `parse_command` sees `args.build` and returns `Command::Build`;
   the dispatcher calls `handle_build`.
3. `ProjectReferenceGraph::load(proj/tsconfig.json)`:
   - canonicalize and `load_project` the root. Its `references` (shallow parse)
     name `./app`; `resolve_single_reference` resolves `app/` to
     `app/tsconfig.json`, valid.
   - worklist pushes `app/tsconfig.json`. `load_project` on it calls
     `load_tsconfig`, which resolves `extends: "../tsconfig.base.json"`:
     `resolve_extends_path` finds the base, `merge_configs` overlays the app's
     `files: ["main.ts"]` and `references: [../lib]` onto the base's
     `composite: true, strict: true`. The effective `composite` is `true`, so
     `is_composite = true`. The app's `references` (local only) name `../lib`.
   - worklist pushes `lib/tsconfig.json`; same `extends` merge makes it
     composite. `lib` has no references.
   - `build_edges` wires `root -> app -> lib`.
4. `graph.validate()`: every referenced project is composite (inherited from
   the base) and none set `noEmit`, so no `TS6306`/`TS6310`; no cycles, so no
   `TS6202`. Empty diagnostics — continue.
5. `graph.build_order()`: in-degrees are `lib: 1` (app references it),
   `app: 1` (root references it), `root: 0`. Kahn peels `root`, then `app`,
   then `lib`; after `order.reverse()` the build order is **`[lib, app, root]`**
   — dependencies first.
6. For `lib`: `is_project_up_to_date` finds no `lib/tsconfig.tsbuildinfo`
   (`get_build_info_path` -> `BuildInfo::load` -> file missing) and returns
   `false`. `compile_project(args, lib/, lib/tsconfig.json)` runs the single
   program; because `composite` fanned out to `declaration + incremental`, it
   emits `index.d.ts` and writes `lib/tsconfig.tsbuildinfo` recording the latest
   changed `.d.ts`.
7. For `app`: not up-to-date (no buildinfo). `compile_project` compiles
   `main.ts`; its import of `lib` type-checks against the `index.d.ts` just
   produced. Emits `main.d.ts`, writes `app/tsconfig.tsbuildinfo`.
8. For `root`: `files: []` with references is a *solution* config; it compiles
   trivially (its purpose is to aggregate the references).

A second `tsz --build` with no edits: `is_project_up_to_date(lib)` now loads
`lib/tsconfig.tsbuildinfo`, the version matches, `ChangeTracker` finds no source
changes, `lib` has no references — returns `true`, `lib` is skipped. For `app`,
the source is unchanged *and* `lib`'s `latest_changed_dts_file` mtime predates
`app`'s recorded `build_time`, so `are_referenced_projects_uptodate` passes —
`app` is skipped too. With `--verbose`, both print "✓ Up to date".

Editing `lib/index.ts`: `lib`'s `ChangeTracker` sees the content-hash change,
rebuilds, and re-stamps `lib/tsconfig.tsbuildinfo` with a fresh `build_time`
and a fresh `index.d.ts` mtime. On `app`, `are_referenced_projects_uptodate`
now sees `lib`'s `.d.ts` mtime `>=` `app`'s `build_time` and returns `false`,
so `app` rebuilds too. The change correctly propagates downstream.

## Caches and invariants

| Cache / state | Owner | Invalidation |
| --- | --- | --- |
| `path_to_id` (config path -> `ProjectId`) | `ProjectReferenceGraph` | rebuilt every `load`; keyed by canonical path so diamonds collapse |
| `visited` set during `load` | `ProjectReferenceGraph::load` | per-load; prevents re-loading a shared dependency |
| `visited` set during `extends` recursion | `load_tsconfig_inner` | per-load; entry is *removed* on exit so a config can be re-extended by siblings, but a re-entry within the same chain is a cycle (`bail!`) |
| `.tsbuildinfo` `BuildInfo` | persisted file | format `version` (`0.1.0`) + `compiler_version` gate; either mismatch -> `Ok(None)` -> rebuild |
| per-file `version` hash in `file_infos` | `BuildInfo` | content hash (`DefaultHasher`); mismatch marks the file changed |
| `latest_changed_dts_file` mtime | filesystem | compared `>=` against parent's `build_time`; missing file -> stale |
| `composite -> declaration/incremental` fan-out | `apply_non_strict_fanout` | recomputed per option resolution; idempotent (monotone `||`) |

Invariants:

- **Dependencies compile before consumers.** `build_order()` guarantees a
  referenced project precedes every project that references it, so a consumer
  always type-checks against fresh `.d.ts` from disk.
- **One project = one program.** Build mode runs N independent
  `compile_project` invocations; there is no shared cross-project type universe
  in build mode. Cross-project information flows only through emitted `.d.ts`
  files.
- **`references` are local, effective options are inherited.** `load_project`
  reads `references` from the shallow parse (never inherited) and
  `composite`/`noEmit`/`outDir` from the effective post-`extends` config.
- **Up-to-date is conservative.** Every uncertain branch (missing buildinfo,
  version mismatch, unreadable `.d.ts`, same-second mtime) returns `false`
  (rebuild). Build mode never risks serving a stale output.
- **The build driver never calls the solver.** It only orchestrates `extends`
  merges, graph ordering, freshness checks, and per-project `compile_project`
  calls.

## Edge cases and tsc parity

- **Diamond references** collapse to one node because `path_to_id` is keyed by
  the canonical config path and the `load` worklist checks `visited`. A shared
  leaf is loaded and built once.
- **Inherited `composite`** (set only in an extended base) still satisfies
  `TS6306` and still enables declaration/incremental emit, because `load_project`
  resolves the *effective* options through `load_tsconfig`.
- **Missing `extends` target** emits `TS6053` (`FILE_NOT_FOUND`) anchored at the
  specifier and continues with the local options — non-fatal, matching tsc.
- **`extends` cycle** is caught by the `visited` set in `load_tsconfig_inner`
  (`bail!("tsconfig extends cycle detected …")`); a `references` cycle is caught
  by `detect_cycles` and surfaces as `TS6202`.
- **Same-Unix-second `.d.ts`** forces a parent rebuild via the `>=` comparison
  (issue #4754) — tsz never assumes a same-second referenced output is older.
- **Deleted referenced `.d.ts`** that the buildinfo still records forces a
  rebuild rather than silently passing (issue #4753).
- **`${configDir}`** resolves against the leaf config's directory for the entire
  `extends` chain, and only when it leads the value (a mid-string occurrence is
  left literal, matching tsc).
- **`composite` + explicit `declaration: false`** is `TS6304`; `composite` +
  explicit `incremental: false` is `TS6379` (anchored at `compilerOptions`).
  An *unspecified* `declaration`/`incremental` that the fan-out sets to `true`
  does not conflict.
- **`--build` not first** -> `TS6369` (long form) / `TS5023` (`-b`); build-only
  flags outside build mode -> `TS5093`.
- **A references-only root** (`"files": []` with a non-empty `references[]`) is
  the canonical solution pattern; the driver suppresses `TS18003` ("no inputs")
  for it via the `has_project_references` check in
  `driver/core_diagnostics.rs`.
- **Graph load failure** falls back to `handle_build_single_project`, which
  compiles the root as an ordinary single program — a malformed solution still
  attempts a useful build instead of hard-failing.

## See also

- [driver-incremental-and-watch](driver-incremental-and-watch.md) — the
  single-program incremental cache, export-signature invalidation, and watch
  loop that build mode reuses per project.
- [end-to-end-timeline](end-to-end-timeline.md) — the scanner -> parser ->
  binder -> checker -> solver -> emitter pipeline each `compile_project` runs.
- [module-resolution-engine](module-resolution-engine.md) — how a consumer's
  import of a referenced project's module resolves.
- [emitter](emitter.md) — declaration (`.d.ts`) emit whose freshness drives the
  cross-project up-to-date check.
- [checker-declarations-modules](checker-declarations-modules.md) — module and
  declaration semantics consumed across project boundaries.
