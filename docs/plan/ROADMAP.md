# TSZ Roadmap

Status: clean-slate rewrite. This is the single living execution plan.

## North Star

TSZ must produce the same project result as the pinned TypeScript 7 compiler,
then become materially faster without changing that result.

The eventual target is:

- exact diagnostic, inference, narrowing, module-resolution, emit, and language-service compatibility;
- every required and canary project measured with its real dependency graph;
- at least 3x the throughput of `tsgo` on every project that is already correct;
- one TypeScript-compatible semantics. Sound Mode no longer exists.

This is a restart, not a migration of the old checker. The parent checkpoint is
`2770da88d4` (2026-08-20). Git history is the rollback mechanism.

## Why The Reset Exists

The retired implementation accumulated approximately:

- 1.37 million non-test-ish Rust lines across twelve compiler crates;
- another 1.27 million lines of embedded and crate-root Rust tests;
- about 127 cache-like structures and 111 `TSZ_*` behavior knobs;
- a 258-field checker context, two mutable type environments, and mirrored writes;
- 105 files and 649 direct Sound Mode markers.

Its last checked-in conformance dashboard was 11,667 / 12,043 runnable cases
(96.9%), with 376 failures. Required/canary dashboards also overstated real
coverage: some rows were absent or gray, and some fixtures replaced dependency
types with 105 stub modules containing 719 `any` members.

The decisive semantic lesson is not “hash every declaration” or “add one SCC
fixpoint.” Both ideas were tested and did not explain the canaries. TSZ forced
deferred forms into concrete copies before inference and relation; TypeScript 7
often relates or infers over the symbolic forms directly and bounds recursion
by the identity of the active comparison. The rewrite starts from that rule.

## What Survives

The validation perimeter is product infrastructure and must remain usable:

- `scripts/conformance/**` and the pinned TypeScript 7 oracle/cache;
- `scripts/emit/**` and JS/DTS baselines;
- `scripts/fourslash/**` and its framed server adapter;
- `scripts/bench/**`, `scripts/perf/**`, project compile guards, and all 69 row definitions;
- safe-run, timeout, immutable-binary measurement, provenance, and artifact checks;
- black-box tests and fixtures under `crates/*/tests/**`, plus hashed inline-test
  fragments under `tests/legacy-internal/**`, as a disabled porting corpus;
- Criterion benchmark sources under `crates/tsz-core/benches/**`;
- generated diagnostic/localization data and TypeScript lib assets;
- the behavioral specs under `docs/specs/`, except the removed Sound Mode catalog.

Historical results remain evidence. They are never relabeled as rewrite
results, and the rewrite never claims progress by lowering the old floor.

## Replacement Shape

The Rust workspace starts with three functional packages:

```text
tsz-core
├── syntax       scanner, parser, immutable syntax storage
├── program      config, source files, module graph, stable declaration index
├── checker      binding, types, inference, relations, flow, diagnostics
├── emit         JavaScript and declaration output
└── service      the sole compiler/project/language-service facade

tsz-cli ────────> tsz-core
  binaries: tsz, tsz-server, tsz-lsp, try-tsz

tsz-conformance
  external-process oracle harness; no compiler-internal dependency
```

`tsz-wasm` returns only after the service API is stable enough to expose. The
website is a Node project, not a dummy Rust workspace member. Internal phases
are modules first; a new crate boundary requires measured compile-time or API
isolation evidence.

## Semantic Rules

1. **Port behavior, do not redesign TypeScript.** The pinned TypeScript 7.0.2
   implementation and oracle define compatibility. Prefer a recognizable,
   source-linked port of its algorithm over a novel solver abstraction.
2. **Deferred forms are first-class.** References/applications, indexed access,
   `keyof`, conditionals, mapped types, and inference placeholders remain
   symbolic until the owning operation genuinely needs a concrete view.
3. **One checker, one type universe.** `TypeId` is meaningful only inside the
   checker that created it. Cross-session products contain declarations,
   diagnostics, text, or stable summaries—never raw type handles.
4. **Identity is explicit, not content-elected.** A declaration receives one
   program-owned identity. Nominal declaration identity and structural type
   equality are separate questions.
5. **Completion is typed.** Semantic work returns `Complete`, `Deferred`,
   `Cycle`, or `Limit`. An incomplete computation may not silently become
   `any`, `unknown`, `error`, or a definitive cached answer.
6. **Construction does not over-reduce.** Union reduction policy belongs to the
   semantic call site. Interning alone must not subtype-reduce a union.
7. **Diagnostics follow facts.** Relations return structured reasons;
   diagnostics select codes, messages, spans, and related information after the
   semantic outcome is known.
8. **Emit consumes syntax and checked summaries.** It does not re-check the
   program or patch already-rendered output.
9. **Capabilities have one owner.** Syntax retains authored facts. One immutable,
   typed analysis per program/options snapshot derives claims keyed by operation
   or product and program/file/node scope. Checker, public emit/printers, every
   service, and exit-status selection reuse it; phases do not mirror policy.
10. **Completion is dependency-closed and stays local.** An incomplete producer
    defers every dependent demand before definitive absence/relation diagnostics,
    while independent declarations continue checking. Whole-program suppression
    is only for uncertainty that is structurally program-global.
11. **Evaluation has one session identity.** Forcing and recursion use one
    canonical checker-session key schema. Demand-scoped frames and typed budget
    axes remain distinct, but required-type and display do not create independent
    identity universes or eager subtree prewalks. Traversal depth is not reused as
    evaluator fuel, and incomplete operands stop owner materialization immediately.
12. **Caches prove purity.** A cache key names every input that can change the
   answer. Incomplete/speculative results do not enter definitive caches.
   Version epochs and whole-cache clears are not substitute inputs.
13. **Determinism precedes parallelism.** The reference path is single-threaded.
   A parallel stage graduates only after repeated, file-order, cold/warm, and
   thread-count diagnostic-set agreement.
14. **Freshness is typed once.** Literal freshness and regular structural type
   are distinct semantic facts. Mutable observation points consume the shared
   widening query; diagnostic display provenance does not decide assignability.
15. **Validation never repairs the product.** Canonical harnesses consume only
   TSZ output from the original invocation. Oracle answers, altered retries,
   output surgery, omitted rows, and fixture stubs cannot count as parity or
   performance evidence.

## Execution Milestones

### R0 — Delete and prove a vertical slice

Goal: `green` foundation.

- remove every legacy compiler `src/` implementation and old-layout guard;
- remove Sound Mode from code, CLI, config, API, tests, docs, and website;
- replace the workspace with the small graph above;
- retain the oracle, test, emit, fourslash, project, and performance harnesses;
- implement a fresh end-to-end slice: source -> scan -> parse -> bind -> check -> diagnostic/emit;
- preserve the `tsz`, `tsz-server`, `tsz-lsp`, and `try-tsz` process contracts;
- add seed parity cases for declarations, literal widening, annotations,
  assignment, function calls/returns, object properties, unions, and JS emit;
- prove identical output over repeated runs and reversed root-file order;
- keep new hand-written compiler code below 15,000 physical lines.

The first PR opens only after the R0 conviction gate below passes.

### R1 — Syntax, config, and project truth

Goal: `hold` the frontend before semantics expand.

- port the TypeScript 7 scanner/parser recovery behavior by grammar family;
- generate `SyntaxKind`, option, diagnostic, and lib metadata from pinned sources;
- make `tsconfig`, root-file order, directives, libraries, and module resolution
  agree before broad type-checking work;
- reach exact scanner/parser diagnostics on their conformance campaigns;
- run parse-only throughput and peak-RSS comparisons against `tsgo`.

### R2 — Ordinary TypeScript semantics

Goal: `green` on ordinary projects.

- declarations, merging, scopes, aliases, functions, classes, objects, arrays,
  tuples, unions/intersections, generics, contextual typing, and flow;
- exact structured diagnostic reasons and display provenance;
- no semantic cache until the uncached result is stable and its request is typed;
- recover required project rows one family at a time with real dependencies.

### R3 — Deferred and recursive type semantics

Goal: turn every canary into a trustworthy result.

- conditional, distributive, mapped, remapped, template, indexed, `keyof`, and
  `infer` behavior through symbolic operations before materialization;
- explicit recursion-identity accounting and completion states;
- advanced inference and relation algorithms ported against adjacent matrices;
- all 69 rows visible, reproducible, and evaluated against an explicit oracle domain;
- zero `any`-stub fixtures in rows used for compatibility claims.

### R4 — Emit, incremental service, LSP, and WASM

Goal: `hold` complete product behavior.

- exact JS and DTS emit, source maps, and transform scheduling;
- incremental invalidation through declaration/API dependencies;
- stable service API used by CLI, framed server, LSP, and WASM;
- fourslash parity and bounded project residency.

### R5 — Speed after correctness

Goal: `fast`.

- every timed row must already be green;
- optimize repeated semantic operations, allocation, locality, and safe concurrency;
- report CPU, wall time, peak RSS, diagnostic hash, binary hash, fixture hash,
  and oracle version together;
- reach zero rows below the eventual 3x `tsgo` target.

## R0 Conviction Gate

Open the reset PR only when all of these are true on the exact head:

1. no legacy compiler source or Sound Mode decision surface remains;
2. `cargo fmt --all --check`, `cargo check --workspace`, Clippy, and new unit tests pass;
3. all four native binaries build and their `--help`/protocol smoke tests pass;
4. the seed oracle matrix has exact codes, spans, messages, exit status, and emit;
5. the retained conformance and emit runners can launch the new binary on narrow filters;
6. ten repeated runs and both root-file orders produce the same diagnostic fingerprint;
7. the architecture/context audit passes and hand-written compiler files stay below 2,000 lines;
   architecture ratchets for capability policy, whole-program suppression,
   force call sites/depth resets, recursion constructors, required-type prepasses,
   checker collections, and near-cap central modules do not grow;
   mirrored `ProductCapabilities`/`SourceUnit` product-policy booleans are removed
   and all product consumers use one typed capability analysis;
8. the PR reports the deletion/retention manifest, measured LOC, known unsupported surface,
   and the exact commands used—without implying broad compatibility.

## Scoring During The Rewrite

Keep three distinct records:

- **legacy checkpoint**: frozen parent metrics, never used as the new floor;
- **rewrite capability**: supported grammar/semantic families and their exact seed tests;
- **full-corpus observation**: conformance/emit/fourslash/project results, including
  unsupported and crashed cases rather than filtering them away.

R0 and R1 gates are capability gates. Once a family is declared supported, its
exact result is a monotonic floor. Broad percentage is informational until the
compiler can parse and bind the full corpus.

## The Four Goals

The repository still uses the four PR goal labels:

- `green`: project results match TypeScript 7;
- `fast`: green results become at least 3x faster than `tsgo`;
- `grow`: add real, dependency-complete projects after required rows are green;
- `hold`: never regress a capability already declared supported.

Until R3, most implementation PRs are `green`; guardrail and parity-floor PRs
are `hold`. Speed claims on red, yellow, gray, or stubbed rows do not count.

## Working Rules

- The reported test is a witness; implement the upstream structural rule.
- No behavior keyed by fixture path, user spelling, source snippets, or rendered types.
- Preserve unsupported outcomes honestly; do not manufacture success with `any`.
- Use the pinned TypeScript 7.0.2 oracle, including its declared threading mode.
- Run focused local suites; CI owns full conformance, emit, fourslash, and project matrices.
- Compare artifacts by stable row identity and exact diagnostic/product payload;
  unchanged totals or status matrices do not establish parity.
- Record provenance and exact commands in every PR.
- Git history is the archive. Do not keep the deleted implementation in a new source directory.

## Definition Of Done

The experiment succeeds when TypeScript 7 and TSZ return the same result on the
full validation perimeter, every real project row is reproducible and green,
and every green timed row is at least 3x faster than `tsgo`. If the architecture
cannot make steady, measured progress toward those gates without recreating the
retired complexity, the experiment has failed and the project should stop.
