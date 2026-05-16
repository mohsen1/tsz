# TSZ LSP Roadmap

Status: **DRAFT** — first holistic LSP roadmap. Authored by agent
`opus-4-7-1m-m4-max-128g` on 2026-05-16.

This document is the LSP companion to `docs/plan/ROADMAP.md`. The top-level
roadmap parks LSP/WASM expansion as a back-burner workstream until the project
corpus gate is met (`ROADMAP.md` §Phase 1). This document is the deep technical
plan that **defines what "solid" means for the LSP** and **what must already be
true** when LSP graduates from back burner to a top-line track.

It does not change roadmap priorities. It captures the current LSP surface,
names the architectural risks that compound the longer they wait, and lays out
the tracks the LSP campaign will run when it activates.

---

## 0) Mission (LSP-specific)

> Match the editor experience of `tsserver` — same protocol, same answers,
> same latency envelope — under the same correctness contract `tsc` enforces.

The top-level mission is "match tsc behavior exactly." The LSP mission inherits
that and adds three editor-shaped invariants:

1. **Responsiveness**: hover/completion/diagnostic latency must stay within
   tsserver's envelope on the project corpus.
2. **Stability**: a request that supersedes another must cancel the old one;
   answers from before a `did_change` must never overwrite answers after it.
3. **Identity preservation**: every LSP response derives from the same
   solver-canonical `TypeId` universe the compiler uses. The LSP must never
   own a parallel type algorithm.

---

## 1) Current State (2026-05-16)

### Headline: the LSP is more mature than it looks

The LSP surface is approximately feature-complete against LSP 3.17. The gaps
that matter are architectural — concurrency, cancellation, configuration
reload, WASM portability, and module size — not method coverage.

### Crates and entry points

| Crate / binary | Path | LOC | Purpose |
|---|---|---|---|
| `tsz-lsp` (lib) | `crates/tsz-lsp/` | 62,083 | All LSP providers: hover, completions, navigation, rename, code actions, diagnostics, semantic tokens, hierarchy, etc. |
| `tsz_lsp` (bin) | `crates/tsz-cli/src/bin/tsz_lsp.rs` | ~3,114 | LSP-protocol server (Content-Length JSON-RPC over stdio). Spawned by the VS Code dev client. |
| `tsz_server` (bin) | `crates/tsz-cli/src/bin/tsz_server/` | 1,841 | tsserver-compatible binary. Supports the tsserver protocol *and* a legacy JSON-per-line protocol for fast conformance testing. |
| `tsz-wasm` (lib) | `crates/tsz-wasm/` | — | WASM bindings. Reuses `tsz::lsp::*` providers (hover, definition, completions, references, position). |
| VS Code dev client | `scripts/vscode-tsz-lsp/` | — | Minimal extension that launches `tsz_lsp` over stdio. Dev-only — not packaged for marketplace. |

### Module map (top-level `tsz-lsp`)

| Module | LOC | Notes |
|---|---|---|
| `project` | 12,789 | Multi-file state, document tracking, diagnostics, incremental updates, eviction, tsconfig |
| `code_actions` | 9,979 | 25+ code action generators (quickfixes, refactorings, extractions, imports, async/await) |
| `completions` | 7,308 | Member/property, auto-imports, postfix, import-path resolution, filtering/ranking |
| `signature_help` (single file) | 4,902 | **Largest single file in the crate** |
| `navigation` | 4,454 | Definition, type-definition, declaration, references, implementation, source-definition |
| `symbols` | 4,083 | Document symbols, workspace symbols, cross-file symbol index |
| `hover` | 3,317 | Hover info, contextual type display, JSDoc formatting |
| `hierarchy` | 2,740 | Call hierarchy + type hierarchy |
| `fourslash` (single file) | 2,256 | Editor-scenario test harness (test-only consumer) |
| `resolver` | 2,104 | Scope chain reconstruction, on-demand symbol resolution, scope caching |
| `highlighting` | 1,783 | Semantic tokens (full/range), document highlighting |
| `rename` | 1,582 | Rename, prepareRename, linked editing, file rename with import auto-update |
| `editor_decorations` | 1,036 | Code lens, inlay hints, document color, color presentation |
| `editor_ranges` | 840 | Folding ranges, selection ranges |
| `diagnostics` | 432 | LSP diagnostic conversion, severity mapping, related information |
| `export_signature` | 365 | Position-independent public API fingerprinting for smart cache invalidation |
| `jsdoc` | 329 | JSDoc extraction and formatting |
| `provider_macro` | 247 | Macro helpers to reduce provider boilerplate |
| `document_links` | 245 | Document link resolution |
| `dependency_graph` | 170 | Bidirectional import graph for incremental invalidation |
| `utils` | 368 | Position utilities, shared helpers |

Files over §12's 2,000-LOC suggestion: `signature_help.rs` (4,902),
`project/module_specifiers.rs` (3,507), `project/imports.rs` (3,234),
`symbols/document_symbols.rs` (2,881), `project/core.rs` (2,765),
`fourslash.rs` (2,256), `navigation/definition.rs` (2,121),
`hierarchy/call_hierarchy.rs` (2,087). Together ≈ 23,800 LOC of LSP logic in
8 files.

### LSP method coverage

Implemented (verified in handler dispatch + provider code):

- Lifecycle: `didOpen`, `didChange` (incremental and full), `didClose`,
  `didSave`
- Text-document semantics: `hover`, `completion` + `completionItem/resolve`,
  `definition`, `declaration`, `typeDefinition`, `references`,
  `implementation`, `documentSymbol`, `documentHighlight`, `signatureHelp`,
  `diagnostic` (LSP 3.17 pull model), `publishDiagnostics` (push)
- Refactoring: `rename`, `prepareRename`, `codeAction` (25+ kinds),
  `linkedEditingRange`, `formatting`, `rangeFormatting`, `onTypeFormatting`
- Editor extras: `semanticTokens/full`, `semanticTokens/range`,
  `foldingRange`, `selectionRange`, `inlayHint` + `inlayHint/resolve`,
  `codeLens` + `codeLens/resolve`, `documentLink`, `documentColor`,
  `colorPresentation`
- Hierarchy: `callHierarchy/{prepare,incomingCalls,outgoingCalls}`,
  `typeHierarchy/{prepare,supertypes,subtypes}`
- Workspace: `workspace/symbol`, `workspace/diagnostic`,
  `workspace/willRenameFiles`, `workspace/didRenameFiles`,
  `workspace/didChangeConfiguration`, `workspace/didChangeWatchedFiles`,
  `workspace/executeCommand`, `workspace/didChangeWorkspaceFolders`,
  `workspace/{will,did}{Create,Delete}Files`

Stubbed/no-op per LSP spec: `workspace/willCreateFiles`,
`workspace/willDeleteFiles` (intentionally return `null`).

**No major LSP 3.17 method is missing.**

### Project & document model

Project core (`project/core.rs`):

```rust
pub struct Project {
    files: FxHashMap<String, ProjectFile>,
    open_files: FxHashSet<String>,           // never evicted
    dependency_graph: DependencyGraph,       // bidirectional import edges
    symbol_index: SymbolIndex,               // cross-file symbols
    type_interner: Arc<TypeInterner>,        // single canonical type universe
    definition_store: Arc<DefinitionStore>,  // shared DefId registry
    file_id_allocator: FileIdAllocator,
    fingerprint_cache: SkeletonFingerprintCache,  // export-signature snapshots
    workspace_roots: Vec<String>,
    tsconfig_settings: FxHashMap<String, TsConfigSettings>,
}
```

Per-file (`ProjectFile`): `root: NodeIndex`, `parser: ParserState`,
`binder: BinderState`, `line_map: LineMap`, `type_cache: Option<TypeCache>`,
`scope_cache: ScopeCache`, `content_hash: u64`, `last_accessed: Instant`,
shared `Arc<TypeInterner>` and `Arc<DefinitionStore>`.

Update path (`update_file()`):
1. Capture old `ExportSignature`.
2. Try incremental parse (`incremental_update_plan` →
   `apply_incremental_update`); fall back to full reparse.
3. Try incremental rebind; fall back to full rebind.
4. Recompute `ExportSignature`.
5. Use `DependencyGraph.get_affected_files()` (BFS over reverse deps) to
   invalidate transitively dependent files **only if** the export signature
   changed. This is the win: body edits don't invalidate dependents.

### Tests

- 34 test files, 3,631 tests across `crates/tsz-lsp/tests/`.
- Largest: `fourslash_tests.rs` (353), `project_tests.rs` (212),
  `completions_tests.rs` (158), `export_signature_tests.rs` (133),
  `symbol_index_tests.rs` (117), `signature_help_tests.rs` (107),
  `rename_tests.rs` (107), `hover_tests.rs` (107).
- No `#[ignore]`, no disabled tests.
- Fourslash harness (`crates/tsz-lsp/src/fourslash.rs`) provides a TS-style
  marker DSL (`/*name*/`). Strict boundary: markers are test-only metadata;
  LSP providers never see raw marker names.
- CI: LSP path changes trigger the full ready-for-review suite (conformance,
  emit, fourslash, WASM). No dedicated LSP e2e/integration job.

### Test gaps (concrete)

Handlers exist but **zero tests**:

- `textDocument/documentColor`
- `textDocument/colorPresentation`
- `textDocument/linkedEditingRange`
- `textDocument/fileRename` (custom)

Minimal tests:

- `textDocument/onTypeFormatting` — 3 tests inside 51 `formatting_tests`
  (range formatting dominates).

WASM:

- One incidental WASM-extension test in `document_links_tests.rs`.
- No WASM-target LSP integration tests in CI.

### Performance & incrementality posture (vs CLAUDE.md §14)

| Contract | Status | Evidence |
|---|---|---|
| Consumer/orchestrator over project state | ✅ | `Project` owns files/graph/interners; LSP features query it |
| Global type interning across files | ✅ | `Arc<TypeInterner>` shared by all files |
| Incremental updates via reverse deps | ✅ | `DependencyGraph` + export-signature diffing; selective invalidation |
| WASM-compatible | ⚠️ partial | `project/module_specifiers.rs` calls `std::fs::*` without universal `cfg(target_arch)` guards |

Other observations:

- **Single-threaded**, no `tokio::spawn`, no thread pools, no
  `CancellationToken` plumbed through providers. The `cancelled_requests`
  set is tracked in the dispatcher but features don't check it during long
  operations.
- **Content-hash short-circuit** in `set_file`: if hash unchanged, skip
  reparse and rebind entirely.
- **`web_time` crate** used for `Instant`/`Duration` — correct WASM-safe
  choice.
- **`ProjectPerformance`** records per-request latency and scope-cache
  hit/miss counts per request kind. Hooks exist; no p50/p99 dashboard or
  regression gate consumes them.
- **No `tsconfig.json` watch handler.** Caller must re-add files when
  tsconfig changes. Path remapping forces global re-check; no diffing logic.
- **No LSP-specific criterion benchmarks** in `crates/tsz-core/benches/`.

### GitHub work in flight

**Snapshot taken 2026-05-16. Verify before acting — issue/PR state moves
fast.**

| Bucket | Count | Notes |
|---|---:|---|
| Open issues labelled `lsp` | **100** | All created today (`#7406`–`#7505`) by an automated audit tool (`SparkStudioLSP`). Zero pre-existing labelled issues remain. |
| Closed issues labelled `lsp` | 3 | One-shot capability gaps closed in the last week (`#4758`, `#4735`, `#4744`). |
| Closed issues with `lsp` in title (broader) | 8 | Each maps 1:1 to a merged `fix(lsp): …` PR. |
| Open PRs touching LSP | **0** | No LSP-labelled, LSP-titled, "playground"-titled, or "server"-titled open PRs. |
| Merged PRs mentioning `lsp` in title, last 60d | 60 | Plus 26 with `fourslash in:title`, 76 with `server in:title`. |
| Recent commits touching LSP paths, last 60d | ~490 | `crates/tsz-lsp`, `crates/tsz-cli/src/bin/tsz_server`, `crates/tsz-wasm`. |

**The 100 open issues are one wave with one root-cause class.** All 100
were filed within ~10 minutes today against the playground
(`localhost:8080/playground/`) and hit builtin-symbol resolution:

| Symptom | Count |
|---|---:|
| Missing tuple member completions (`concat`, `map`, `values`, …) | 29 |
| Missing array member completions | 29 |
| Extra `Object.prototype` completions on string (`constructor`, `replaceAll`, …) | 15 |
| Missing function-object completions (`apply`, `toString`, …) | 9 |
| Signature help returns synthetic `(...args: any[])` on string methods | 7 |
| Extra `Object.prototype` completions on boolean | 6 |
| Extra `Object.prototype` completions on number | 4 |
| Extra `[Symbol.iterator]` on map | 1 |

These three symptoms (missing builtin array/tuple/function methods, extra
`Object.prototype` members leaked into primitives, synthetic signature
help) share one underlying mechanism: **how `tsz-lsp` resolves and
projects the builtin `lib.*.d.ts` symbol space for completion and
signature-help responses**. They are not 100 separate fixes — they are
one fix with 100 witnesses. **See Track L11** below.

**No active LSP work to coordinate with.** A new agent picking up the
audit class will not duplicate any in-flight PR. The most recent
substantive LSP PR was `#7190` ("fix(lsp): repair hover and rename edge
cases", merged 2026-05-15, agent `m5-pro-48g-gpt5`).

**Recent merged themes (60 days):** hover display correctness for
overloads and cache staleness, cross-file navigation (call hierarchy,
rename, references, organizeImports), transport/protocol fixes
(`workspace/applyEdit`, percent-encoded URIs, tsconfig include
patterns), a sustained DRY push on LSP test helpers (`parse_test_source`
consolidation series), WASM wrapper API alignment.

### Peer architectural lessons

Distilled from rust-analyzer, tsserver, and Volar.js (see References §8).
These are the patterns mature LSPs converge on; TSZ should plagiarize them
when activating tracks below.

- **Salsa-style demand-driven query graph.** Rust-analyzer's biggest
  documented IDE-perf win is the *stable layer above body/inference*: an
  `ItemTree` summary that body edits don't invalidate. The TSZ
  `ExportSignature` is the seed of this; the next step is a Salsa-shaped
  query interface so cache invalidation falls out of dependencies rather
  than being explicitly threaded.
- **Cancellation as revision-counter panic, not cooperative polling.**
  Rust-analyzer panics with a sentinel when a query observes a newer
  revision; the `ide` layer catches and surfaces `Result<T, Cancelled>`.
  Avoids the bug-prone "remember to call `ct.is_cancelled()` at every
  checkpoint" pattern.
- **Two-process syntactic/semantic split.** tsserver runs syntactic-only
  features (formatting, syntactic diagnostics, outline) in a *separate
  process* so the editor stays responsive when semantic work is heavy.
- **`LanguageServiceHost`-shaped boundary independent of LSP.** Both
  rust-analyzer and tsserver win by treating LSP as a thin adapter over a
  richer host-facing API. The same boundary makes WASM/CLI/embedders cheap.
- **`ProjectService` with configured > inferred precedence.** Three-tier
  model (configured > external > inferred) keeps single-file scratch
  buffers working without tsconfig but lets them auto-promote when one
  is found.
- **Project sleep / LRU.** Required for multi-tsconfig monorepos. Document
  the lifecycle (`open` → `active` → `idle` → `evicted`) as first-class
  rather than retrofit.

---

## 2) Activation Trigger

The LSP campaign is **not yet** the active workstream. Per top-level
`ROADMAP.md` §Phase 0:

> Keep broad display-provenance polish, generalized query-engine refactors,
> major incremental/perf rewrites, and LSP/WASM expansion on the back burner
> unless they unblock a named project row or release gate.

LSP graduates to active campaign when **all** of the following hold:

1. Project corpus gate is green or has a defined exception
   (`ROADMAP.md` §Phase 1).
2. Track 9 has produced the "one compiler service front door" semantic view
   that the LSP can consume (`ROADMAP.md` §Track 9 acceptance criterion 4).
3. A weekly regression budget exists for LSP responsiveness (currently
   absent).

Until then: maintain regressions; fix high-impact correctness issues; do
**not** start the architectural sub-tracks below as top-line work. Test gaps
listed in §1 (`documentColor`, `colorPresentation`, `linkedEditingRange`,
`fileRename`, `onTypeFormatting`) are exceptions — those are small,
behavior-stabilizing, and can land opportunistically per CLAUDE.md §20.2.

---

## 3) Tracks (post-activation)

Each track names: scope, justification, owner layer, exit criterion, and an
anti-pattern to avoid.

### Track L1 — Concurrency, cancellation, and request scoping

Scope: requests have a lifetime; superseded requests cancel at their next
yield point; long-running requests (workspace symbol search, find
references) never block hover/diagnostics.

Justification: currently single-threaded with no cancellation honored
during work. The `cancelled_requests` set exists in the dispatcher but no
provider checks it. A slow query on the wrong project freezes the editor.

Owner layer: `tsz-cli/src/bin/tsz_lsp` dispatcher + `tsz-lsp` provider
seams.

Approach options to evaluate before implementation:
- (a) Cooperative polling (`CancellationToken` plumbed everywhere).
- (b) Revision-counter panic-and-catch (rust-analyzer pattern).
- (c) Worker pool with abortable handles.

Exit:
- Every public LSP handler accepts (or can observe) a cancellation signal.
- A superseding `did_change` cancels in-flight queries on the same file
  within one event loop iteration.
- Hover/completion latency p99 measured under contention (Track L8).

Anti-pattern: spawning a tokio task that holds a `&mut Project` reference
across an await point.

### Track L2 — Decomposing the megamodules

Scope: split files over §12's 2,000-LOC suggestion into focused submodules.
Priority list (LOC desc):

| File | LOC | Likely split axis |
|---|---|---|
| `signature_help.rs` | 4,902 | extraction → formatting → resolution |
| `project/module_specifiers.rs` | 3,507 | node_modules vs paths vs relative vs baseUrl |
| `project/imports.rs` | 3,234 | discovery vs ranking vs resolution |
| `symbols/document_symbols.rs` | 2,881 | traversal vs decoration |
| `project/core.rs` | 2,765 | struct + lifecycle vs update path vs introspection |
| `fourslash.rs` | 2,256 | DSL parser vs harness vs assertion library |
| `navigation/definition.rs` | 2,121 | lookup vs disambiguation |
| `hierarchy/call_hierarchy.rs` | 2,087 | graph build vs query |

Owner layer: `tsz-lsp` internal refactoring; no behavior change.

Exit: no file in `tsz-lsp/src/` exceeds 2,000 LOC; module boundaries
documented in each module's `mod.rs` rustdoc.

Anti-pattern: alphabetic or arbitrary chunking. Splits must be by concern,
not by line count.

### Track L3 — tsconfig hot-reload and project lifecycle

Scope: `tsconfig.json` changes must reload incrementally. New three-tier
project model: **configured > external > inferred** (tsserver pattern).
Project sleep/eviction first-class.

Justification: current code requires the caller to re-add all files when
tsconfig changes; path remapping forces full re-check; no project lifecycle.

Owner layer: `tsz-lsp::project`.

Exit:
- `workspace/didChangeWatchedFiles` events for tsconfig trigger incremental
  re-resolve.
- File-open in a new directory creates an inferred project; tsconfig
  discovery auto-promotes to configured.
- Idle projects (no open files, no recent requests) evict to a documented
  budget.
- A 1000-file project edited for 30 minutes does not grow memory
  monotonically.

Anti-pattern: diffing rendered tsconfig text instead of the parsed
`TsConfigSettings`.

### Track L4 — Stable summary layer (ExportSignature → ItemTree)

Scope: generalize `ExportSignature` into a Salsa-style stable summary layer
that body edits cannot invalidate. Lift the "did the public API change?"
check from a fingerprint comparison into a query-graph dependency, so
downstream queries (cross-file inference, references) re-run only when
their inputs invalidate.

Justification: rust-analyzer's documented top-line IDE-perf win is exactly
this layer. TSZ has the seed (`ExportSignature`); the gap is a query
interface so cache invalidation is structural rather than threaded by
hand.

Owner layer: `tsz-solver` query boundary + `tsz-lsp::project`. Track 9 of
the top-level ROADMAP is the prerequisite ("one compiler service front
door").

Exit:
- A documented set of stable queries (e.g., `public_api(file)`,
  `module_resolution(file)`, `transitive_imports(file)`) with explicit
  invalidation inputs.
- Body-only edits do not invalidate any cross-file query result.
- Benchmark: 1-line edit inside a function body in `rxjs` re-runs zero
  cross-file inference.

Anti-pattern: introducing a parallel cache that drifts from the existing
`ExportSignature` or `DependencyGraph`.

### Track L5 — Test-coverage close-out and WASM gate

Scope: close the named test gaps and add a WASM LSP CI job.

Concrete deliverables:
- Add tests for `documentColor`, `colorPresentation`, `linkedEditingRange`,
  `fileRename` (each with at least two shape variants per CLAUDE.md §25).
- Expand `onTypeFormatting` tests with trigger-character scenarios.
- Add a WASM LSP integration job in CI that cross-compiles the LSP and
  runs a representative subset against a node host. Guard
  `module_specifiers.rs` `std::fs::*` calls with universal `cfg` so the
  build is reproducible.
- Consider a dedicated LSP e2e CI job to decouple LSP validation from the
  shared fourslash run.

Exit:
- Zero LSP handlers without at least one regression test.
- WASM LSP smoke job in CI; LSP regressions in WASM fail PR merge.

### Track L6 — Observability and latency budgets

Scope: every LSP handler emits a `tracing` span; per-method p50/p95/p99
visible in benchmark runs; regression gate fires when hover or completion
drifts.

Justification: instrumentation hooks exist (`ProjectPerformance` records
per-request latency) but no dashboard or gate consumes them.

Owner layer: `tsz-lsp` handlers + `tsz-cli/src/bin/tsz_lsp` dispatch +
`crates/tsz-lsp/benches/` (new directory).

Approach:
- Wrap each provider entry point in `tracing::debug_span!`.
- Add a criterion harness running representative requests against the
  project corpus.
- Publish a per-method latency table next to the conformance/emit
  dashboard.

Exit:
- Latency dashboard exists; CI publishes per-method numbers; a documented
  regression budget blocks merge.

Anti-pattern: `eprintln!` instrumentation (CLAUDE.md §19.6 denies
`clippy::print_stderr`). Use `tracing::trace!`/`debug!` per §19.7.

### Track L7 — Two-process syntactic/semantic split

Scope: factor purely-syntactic features (formatting, syntactic
diagnostics, folding, document symbols, semantic-tokens-light) so they can
run independently of the semantic checker. tsserver does this as a
separate process; TSZ can do it as a separate task pool first and a
separate process later if needed.

Justification: keeps the editor responsive during heavy checker work. The
existing single-threaded model means a 2-second `check_source_file` blocks
formatting requests on a different file.

Owner layer: `tsz-cli/src/bin/tsz_lsp` dispatcher.

Exit:
- Syntactic requests never await semantic work on the same project.
- Latency for formatting/folding measurably independent of checker load
  (Track L6 dashboard).

### Track L8 — tsserver protocol parity verification

Scope: `tsz_server` claims tsserver protocol compatibility. Verify with a
replay harness: a corpus of recorded tsserver session traces replays
against `tsz_server`; responses match modulo timestamps and identifiers.

Owner layer: `tsz-cli/src/bin/tsz_server/` handlers + new
`tests/tsserver-replay/` harness.

Exit:
- Replay corpus exists; ≥80% of representative tsserver requests produce
  byte-equivalent responses; the remaining ≤20% are documented deviations.

Anti-pattern: skipping a deviation by hardcoding "ignore field X for test
Y" — deviations must be documented as policy, not as fixture exceptions.

### Track L9 — WASM playground and browser LSP

Scope: ship a browser playground that runs hover/completion/diagnostics
in-browser via `tsz-wasm`. Use the playground as a public smoke test for
LSP regressions.

Justification: `tsz-wasm` already consumes `tsz::lsp::*` providers; the
plumbing exists. The gap is the user-facing host and the
`module_specifiers` WASM guards (Track L5).

Owner layer: `tsz-wasm` + a new website demo.

Exit: a published playground (e.g., `tsz.dev/playground`) demonstrates
hover, completion, and live diagnostics on user-pasted code.

### Track L10 — Builtin lib symbol projection (audit-batch root cause)

Scope: fix the underlying mechanism behind issues `#7406`–`#7505`. The
audit batch is one root cause with three surface symptoms:

1. **Missing builtin methods** on arrays, tuples, functions
   (e.g. `Array.prototype.concat`, `Function.prototype.apply`,
   `IterableIterator.prototype.values` from `lib.es*.d.ts`).
2. **Extra `Object.prototype` members** leaked into primitive completion
   lists for `string`/`number`/`boolean` (`constructor`, `replaceAll`,
   `toString` where they should not appear).
3. **Synthetic `(...args: any[])` signature help** on string methods
   (`slice`, `indexOf`, `startsWith`, `charAt`, `trim`, `toLowerCase`,
   `toUpperCase`).

These share one mechanism: how `tsz-lsp::completions` and
`tsz-lsp::signature_help` project the builtin `lib.*.d.ts` symbol space
into LSP responses. Per CLAUDE.md §26, the fix must address the
mechanism, not the 100 specific methods named in the issues.

Justification: largest active correctness class (100 issues from one
audit run); blocks playground credibility; high-leverage fix.

Owner layer: `tsz-lsp::completions` + `tsz-lsp::signature_help`,
consuming solver query boundaries (not constructing them).

Approach (state the structural rule before coding, per CLAUDE.md §26):

> "When a member-completion request resolves the receiver type to a
> primitive (`string`/`number`/`boolean`/tuple/`Function`), tsc returns
> the methods declared in the corresponding builtin interface
> (`String`/`Number`/`Boolean`/array-type interfaces/`Function`) plus the
> documented `Object.prototype` subset that is structurally inherited;
> this change makes tsz do the same."

At least three adjacent variants must be tested per §26: array vs.
tuple, string vs. boxed `String`, named function vs. arrow function.

Exit:
- The named symptom classes are closed (≥80 of the 100 issues
  auto-close via a single fix).
- The remaining issues are explicitly documented (e.g., legitimate
  divergences) or have a follow-up issue with a documented rationale.
- New tests cover ≥3 shape variants for each symptom class.

Anti-pattern: per-method allowlists/denylists. The fix must be
structural over the builtin symbol space, not "skip these specific names
on string."

### Track L11 — Editor distribution (deferred)

Scope: package the dev VS Code extension as a real extension shippable to
the marketplace. JetBrains plugin when justified.

Owner: humans, not agents. Out of scope until L1–L4 are green.

Exit: documented installation procedure that does not require building
from source.

---

## 4) Anti-Patterns Specific to the LSP Layer

These extend CLAUDE.md §25 (anti-hardcoding) and §26 (generalization gate)
with LSP-specific cases:

1. **Per-method ad-hoc caching.** A hover handler must not own its own
   per-file cache; cache at the project/solver level so
   hover/definition/completion share invalidation.
2. **String-matching tsserver protocol payloads.** Reasoning over rendered
   JSON to drive behavior is forbidden; route through structured
   `serde_json::Value` or typed structs.
3. **Single-fixture fourslash tests.** A fourslash test that only passes
   for the literal identifier names in the fixture is a §25 violation.
   Vary names (`T`/`K`, `P`/`X`, etc.).
4. **Single-test handler shortcuts.** A handler must not have an
   `if project_name == "X" { … }` branch.
5. **Bypassing the project model.** Direct construction of checker/solver
   state in a handler is a §4 violation (consumer layers consume; they do
   not construct).
6. **`std::fs::*` without `cfg(not(target_arch = "wasm32"))` guards.** Any
   new file-I/O in `tsz-lsp` must compile under WASM (either guarded or
   routed through a trait).

## 5) Success Metrics

When the LSP campaign concludes, the following should be true and
measurable:

| Metric | Target | Source |
|---|---|---|
| Hover p99 on `rxjs` project | within 2× of tsserver | Track L6 dashboard |
| Completion p99 on `rxjs` project | within 2× of tsserver | Track L6 dashboard |
| Diagnostic delta vs tsserver on project corpus | 0 (or documented exception) | ROADMAP project gate |
| Reverse-dep recheck count on 1-line body edit | 0 cross-file inference re-runs | Track L4 benchmark |
| Memory growth per 1000 edit cycles on rxjs | bounded; no monotonic leak | Track L3 |
| LSP fourslash gate pass rate | 100% | existing CI |
| tsserver replay parity | ≥80% | Track L8 |
| WASM LSP CI gate | green | Track L5 |
| WASM playground availability | yes | Track L9 |
| Modules >2000 LOC | 0 | Track L2 |
| Open `lsp`-labelled issues from 2026-05-16 audit batch | ≥80 of 100 closed by structural fix | Track L10 |

## 6) Coordination Notes

- **Agent identity.** Per CLAUDE.md §20.1, every PR body must include the
  AgentName. The author of this doc is `opus-4-7-1m-m4-max-128g`.
- **Multi-agent overlap.** The github-inventory subsection (when filled
  in) lists in-flight draft PRs. Future agents picking up an LSP track
  should claim it by opening a draft PR or commenting on an existing one
  before starting.
- **Stacked PRs.** Tracks L1, L2, and L4 will likely produce dependent
  PRs. Use stacked PRs (CLAUDE.md §20.1) rather than waiting.
- **Roadmap discipline.** This document is durable direction (CLAUDE.md
  §0). Update it when track scope, sequencing, or exit criteria change.
  Do not update it for routine PR status — that belongs in draft PR
  bodies.
- **Touching `module_specifiers.rs`, `imports.rs`, or `core.rs`.** These
  are the biggest files in the tree and likely targets for Track L2.
  Coordinate splits with any in-flight refactor of the same files to
  avoid merge conflicts.

## 7) Open Questions

1. Should `tsz-lsp` consume the (future) compiler service front door
   directly, or should `tsz_lsp`/`tsz_server` mediate? Probably the
   former, but confirm when Track 9 lands.
2. Does the tsserver-protocol path need to remain after LSP-protocol
   parity is reached? VS Code uses LSP; nvim, sublime, JetBrains via
   plugins also use LSP; tsserver protocol exists primarily because of
   historical VS Code internals. Decision: keep both until empirical
   evidence shows the tsserver path has no remaining users.
3. What does "tsserver replay parity" look like as a CI gate? Need a
   recording mechanism and a deterministic diff strategy. The legacy
   JSON-per-line protocol in `tsz_server` may be reusable as the
   recording target.
4. Cancellation strategy: (a) cooperative polling, (b) revision-counter
   panic-and-catch (rust-analyzer), (c) abortable worker handles. Pick
   one before starting L1.
5. Should fourslash gain a "shape variant generator" that auto-derives
   §25 anti-hardcoding test variants? Possibly out of scope.
6. Should the two-process split (L7) be a separate process or a separate
   task pool in the same process? Probably pool first, process later if
   warranted.

## 8) References

- `docs/plan/ROADMAP.md` — top-level roadmap (LSP/WASM positioning at
  lines 214, 489–520).
- `crates/tsz-lsp/src/project/core.rs` — `Project` struct, `update_file`,
  `set_file`, content-hash short-circuit.
- `crates/tsz-lsp/src/dependency_graph/mod.rs` — reverse-dep BFS.
- `crates/tsz-lsp/src/export_signature/` — public-API fingerprinting.
- `crates/tsz-lsp/src/fourslash.rs` — editor scenario harness.
- `crates/tsz-cli/src/bin/tsz_server/main.rs` — tsserver-compatible
  binary; protocol modes documented in module doc comment.
- `scripts/vscode-tsz-lsp/README.md` — dev extension setup.
- `crates/tsz-wasm/src/wasm_api/language_service.rs` — WASM consumer of
  LSP providers.
- rust-analyzer architecture:
  https://github.com/rust-lang/rust-analyzer/blob/master/docs/book/src/contributing/architecture.md
- tsserver wiki:
  https://github.com/microsoft/TypeScript/wiki/Standalone-Server-%28tsserver%29
- Volar.js:
  https://volarjs.dev/

---

*End of LSP_ROADMAP.md*
