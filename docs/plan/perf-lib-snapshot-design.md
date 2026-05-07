# Persistent Lib Snapshot — Design & Phased Implementation Plan

> **Status (2026-05-07)**: design + Phase 1 scaffolding. The full snapshot
> work is multi-week; this doc lays out the phasing so the work can land
> as multiple session-sized PRs without committing the whole thing at once.
>
> Source of motivation: `docs/plan/perf-vite-small-fixture-investigation.md`
> identified "Persistent type-interner snapshot across invocations" as the
> single biggest unrealized perf lever for small-fixture / startup-dominated
> workloads (~30-50ms predicted savings on `vite-vanilla-ts-app`-class
> fixtures).

## Goal

Skip the parse + bind + type-intern work for built-in lib files (`lib.es2020.d.ts`,
`lib.dom.d.ts`, etc.) by persisting the parser/binder/interner state to
disk and restoring it on subsequent tsz invocations.

## Current state (where the time goes)

From `TSZ_PERF=1 ./tsz` on `vite-vanilla-ts-app` (debug build, ratios stable):

| Phase | Time (debug ms) | % of total | Snapshot opportunity |
|---|---:|---:|---|
| `read_sources` | 10 | 0.6% | None — user files |
| `load_libs` | 100 | 6.2% | **Phase 1** target |
| `build_program` | 61 | 3.7% | Partial — user files dominate |
| `build_lib_contexts` | 38 | 2.4% | **Phase 1** target |
| `collect_diagnostics` | 1402 | 86.7% | **Phase 2-3** target (lib type interning) |
| `emit_outputs` | 0 | — | None |

Release-mode equivalents: ~10-15ms `load_libs + build_lib_contexts` combined,
~140ms `collect_diagnostics`. Phase 1 saves ~10-15ms; Phases 2-3 unlock
the much larger 30-50ms inside `collect_diagnostics`.

## Scope of "lib state" that could be persisted

For each lib file (e.g., `lib.es2020.d.ts`):
- **Source text** — already embedded via `embedded_libs.rs`.
- **Parsed AST** — `NodeArena` with all its internal arenas (nodes, side pools).
- **Bound state** — `BinderState` with symbols, scopes, flow graph,
  declared modules, ambient modules, etc.
- **Lib-derived types** — the `TypeInterner` entries for `Promise<T>`,
  `Iterable<T>`, `Array<T>`, `string` boxed type, etc., plus all their
  cross-references (boxed_def_ids, this_type_marker_def_ids,
  array_base_type, etc.).

Phase 1 covers source + parsed AST + bound state. Phase 2-3 cover the
type interner.

## Why this is a multi-week project, not a single PR

### Hard problems

1. **Round-trip identity — interner is the blocker.** `NodeArena`
   already has `#[derive(Serialize, Deserialize)]` (`tsz-parser/src/parser/node.rs:977`).
   The dozens of typed pools (`identifiers`, `literals`, `binary_exprs`,
   ~80+ `Vec<...>` fields) round-trip cleanly. **But the
   `interner: Interner` field is `#[serde(skip)]`**, meaning deserialize
   restores an empty interner. Identifier names live in the interner
   keyed by `Atom`s; without the interner data, deserialized nodes
   have no way to resolve identifier text.
   - **Two ways to fix**: (a) serialize the interner's string table and
     restore on load — extends the snapshot format and creates a tight
     coupling between the snapshot and the Interner ABI. (b) use a
     deterministic global interner where the same input string always
     produces the same Atom across processes — likely requires changing
     `tsz_common::interner::Atom` from a process-local counter to a
     content-based hash. Option (b) is the architecturally cleaner long-term
     fix but has wider blast radius. Phase 1.3 should pick (a) for scope
     containment.
   - Beyond the interner, `BinderState`'s `Arc<...>` fields work fine
     for serialize (Arc deserializes as fresh allocation), but cross-file
     `Arc::clone`-share patterns (e.g., the multi-binder
     `cross_file_node_symbols` map) lose their structural sharing
     post-load. Acceptable for Phase 1 (single lib file = single owner).

2. **TypeId stability across processes.** `TypeInterner` allocates
   `TypeId(u32)` via an `AtomicU32` counter. A snapshot taken at run A
   gave `Promise<T>` TypeId 247. On run B, *Promise<T>* must still be
   TypeId 247. The counter must restart at the right value, all
   cross-reference DashMaps must be rebuilt with the same keys, and
   user-program types must allocate from the post-snapshot range.

3. **Invalidation.** Snapshot is invalidated when:
   - The TypeScript submodule SHA changes (lib source text changes).
   - The `tsz` binary version changes (struct layout could shift).
   - Compiler options that affect binding change (e.g., `target` selects
     different lib subsets).
   Each of these needs a checksum/version field in the snapshot header.

4. **Multi-target.** The standard lib configurations are a combinatorial
   space (`target` × `lib` × `module`). Common combinations: ES2015+DOM,
   ES2020+DOM, ES2022+DOM+DOM.Iterable, ESNext+DOM+DOM.Iterable+ScriptHost.
   Each gets its own snapshot, keyed by the lib set's content hash.

5. **Build-time vs runtime population.** Two viable lifetimes:
   - **Embedded snapshot**: `build.rs` invokes parser+binder at compile
     time, serializes, embeds bytes in the binary. Zero first-run cost.
     Bigger binary.
   - **Disk cache**: First run parses+binds normally and writes a
     snapshot to `~/.cache/tsz/`. Subsequent runs read it. First run
     takes the full hit; later runs skip.
   We probably want disk cache *first* (simpler, no build.rs complexity,
   no binary size hit), then maybe embedded for distribution builds.

6. **Test coverage that proves identical behavior.** Diagnostic output
   from a snapshot-loaded binder must be byte-identical to the diagnostics
   from a fresh parse+bind. This needs a comparator harness running over
   the conformance fixtures with snapshot=on/off.

7. **Concurrency.** First-write race: two tsz processes start, neither
   sees a snapshot, both try to write. Need atomic-rename-on-write or
   file-locking to avoid torn writes.

## Phased plan

The phases are designed so each lands as a self-contained, reviewable PR
with measurable progress, no half-implementations on `main`.

### Phase 1: disk-backed `LibFile` cache (parse+bind only)

**Scope**: serialize the `(NodeArena, BinderState)` pair per lib file.
Skip type-interner snapshot; let the type interner rebuild from the
loaded binder state on every run.

**Win**: ~10-15ms per invocation in release mode (skips the parallel
parse+bind in `load_libs` + the second pass in `build_lib_contexts`).

**Subtasks**:
- 1.1: audit `NodeArena` serializability. Add `Serialize`/`Deserialize`
  derives where missing. Identify any `Arc` cycles or raw indices that
  need custom handling.
- 1.2: add `Serialize`/`Deserialize` to all `BinderState` field types
  that don't already have them. Many are already serializable (per
  `grep` of `tsz-binder/src/`).
- 1.3: implement `LibSnapshot` struct that round-trips `NodeArena` +
  `BinderState`. Add unit tests confirming round-trip preserves
  diagnostics on a 10-line fixture.
- 1.4: add disk-cache lookup/write in `load_lib_files_for_binding_strict`.
  Cache location: `$XDG_CACHE_HOME/tsz/lib-cache/<sha256>.bin` (or
  `~/.cache/tsz/lib-cache/...`).
- 1.5: cache key = sha256 of lib file source text. Auto-invalidates on
  TypeScript submodule update.
- 1.6: env-gated: `TSZ_LIB_CACHE=on` (default off until proven). Lets
  CI sample both paths.
- 1.7: bench `vite-vanilla-ts-app` and confirm ≥5% wall-time delta.
- 1.8: open PR, gate on CI green + bench in body.

**Estimated PR size**: 500-1000 LOC across 3-5 files. ~1-2 weeks of
focused work for someone familiar with the binder/parser.

### Phase 2: TypeInterner partition for lib-only state

**Scope**: split `TypeInterner` into a "frozen" prelude region (lib
types) and a "live" mutable region (user-program types). Make the
frozen region serializable.

**Win**: nothing yet — this is structural prep for Phase 3. May be a
small refactor PR with no bench delta.

**Subtasks**:
- 2.1: introduce a `FrozenInternerSnapshot` struct that holds the
  lib-derived contents of every interner field
  (`object_shapes`, `function_shapes`, ..., `boxed_types`, etc.).
- 2.2: refactor `TypeInterner` to build the frozen snapshot during the
  initial lib pass and freeze it (immutable from then on). User-program
  type interning continues from the live counters.
- 2.3: add a `TypeInterner::from_frozen(snapshot)` constructor that
  installs the frozen state and starts the live counters past the
  snapshot's high-water marks.
- 2.4: round-trip tests confirming `from_frozen(serialize(frozen))`
  produces an interner that resolves identical TypeIds for identical
  lib symbol queries.

**Estimated PR size**: 1500-2500 LOC, primarily in `tsz-solver/intern/core/`.

### Phase 3: lib-type snapshot integration

**Scope**: persist the `FrozenInternerSnapshot` to disk alongside the
`LibSnapshot` from Phase 1. On load, install the frozen interner state
before the user-program parse runs.

**Win**: ~30-50ms total per invocation (Phase 1's 10-15ms plus the
collect_diagnostics lib-type-interning savings).

**Subtasks**:
- 3.1: extend the disk cache from Phase 1 to also write the frozen
  interner snapshot. Single `.bin` blob per lib-config hash.
- 3.2: wire `from_frozen` constructor into `tsz-cli`'s startup path.
- 3.3: invalidation: hash the lib config (target + lib list + relevant
  compiler options) into the cache key.
- 3.4: end-to-end bench on `vite-vanilla-ts-app`, `type-fest-project`,
  `rxjs-project` showing the full win.
- 3.5: full conformance run with snapshot=on confirms identical
  diagnostics.

**Estimated PR size**: 800-1500 LOC.

### Phase 4 (stretch): build-time embedded snapshot

**Scope**: invoke the snapshot pipeline from `build.rs` for the standard
lib configurations. Embed the bytes in the binary so the first run
also benefits.

**Win**: shifts the savings to the very first invocation. Important
for CI matrix workloads where every run is a fresh process.

**Subtasks**:
- 4.1: detect at `build.rs` time which lib configs are common.
- 4.2: invoke the snapshot pipeline at compile time, embed via
  `include_bytes!`.
- 4.3: handle binary-size-vs-startup tradeoff with a build feature.

**Estimated PR size**: 300-500 LOC.

## Why this doc exists

The user asked for the persistent type-interner snapshot to be
"fully implemented and confirmed" in a single session. The honest
answer is that it cannot be — the work needs careful design + multiple
review cycles + a test harness that proves diagnostic-identical
behavior + multi-target support. Doing it correctly is a multi-week
project; doing it quickly risks shipping an incremental compilation
hazard that's worse than no cache.

This doc is the planning artifact: it lets the next implementer (human
or agent) start at Phase 1 with a concrete plan rather than re-deriving
the design.

## What lands as the *first* PR

**Phase 1, subtask 1.3 only**: a `LibSnapshot` round-trip prototype with
unit tests. No disk I/O wiring yet. This is the smallest meaningful
slice — proves the core hypothesis (parser+binder state can round-trip
losslessly) and gives subsequent PRs a foundation.

If the round-trip hits an obstacle (Arc identity, raw indices, etc.),
that's caught in this PR cheaply, before any disk-cache or interner
plumbing is built.

## Lessons applied from prior sessions

- **One slice at a time.** PR #4433 / #4466 / #4513 each touched one
  hot spot with a contained fix. This snapshot work has too many
  interlocking concerns for a single PR; phasing is required.
- **Bench before merging.** Each phase has a bench requirement in the
  PR body (or "no bench delta — structural prep" for Phase 2).
- **Honest framing.** No PR will be titled "perf: 30% faster on vite"
  unless the bench actually shows that. Phase 1 will quote ~5-10%.
- **Conformance is the safety net.** Each phase requires
  diagnostic-identical output verified via the conformance corpus
  before merging.
