# TSZ Roadmap

Date: 2026-04-25

Status: living plan. This is the single planning document for project direction across conformance, emit, performance, architecture, LSP/WASM, Sound Mode, and DRY cleanup. Do not add new roadmap files under `docs/plan/`; update this file instead.

This document supersedes the previous scattered plan files in `docs/plan/` and the former standalone DRY audit. All planning claims now live here.

## Implementation Coordination

To avoid duplicate work, roadmap-adjacent implementation, including DRY cleanup, must be claimed before coding starts.

Workflow:

1. Pull latest `main` and inspect open PRs.
2. Create a branch for the intended work.
3. Make a minimal roadmap edit in the section below, describing the intent, scope, branch, and draft PR title.
4. Open a draft PR immediately with the `WIP` label.
5. Only then start implementation.

Draft PR command shape:

```bash
gh pr create --draft --label WIP --title "[WIP] <scope>: <intent>" --body "$(cat <<'EOF'
## Intent
- <what this PR intends to change>

## Roadmap Claim
- Updated `docs/plan/ROADMAP.md` before implementation.

## Planned Scope
- <files/systems expected to change>

## Verification Plan
- <targeted tests / conformance / emit / bench plan>
EOF
)"
```

Rules:

1. A WIP PR is a coordination claim, not a merge candidate.
2. Never merge a branch while its PR is draft, labeled `WIP`, titled with `[WIP]`, or otherwise described as WIP.
3. Before marking ready, update this roadmap with the actual status, remove the `WIP` label, remove the `[WIP]` title prefix, and mark the PR ready.
4. If implementation is abandoned, close the draft PR and remove or mark the roadmap claim as abandoned.
5. DRY cleanup uses this same claim flow. Keep each DRY slice small and reviewable.

### Active Implementation Claims

Add new roadmap and DRY claims here before implementation begins.

- **2026-04-25** · branch `claude/modest-archimedes-QBaXl` · **DRY active claim** · P0 Test-Harness Consolidation: replace local `check_with_options` helpers with `tsz_checker::test_utils::check_source` in `crates/tsz-checker/tests/class_member_closure_tests.rs`, `contextual_tuple_tests.rs`, `contextual_typing_tests.rs`, and `never_returning_narrowing_tests.rs`. Legacy draft title: `[do not merge] chore(checker-tests): replace local check_with_options helpers with test_utils::check_source`; treat as WIP until ready.
- **2026-04-25** · branch `chore/parser-keyword-token-length-helper` · **DRY workstream-8 ready** · workstream 8.9 ("Move hardcoded modifier/keyword token lengths into scanner/parser metadata"): added `pub const fn keyword_text_len(SyntaxKind) -> u32` in `tsz-scanner` and migrated 8 hardcoded sites across `state_declarations_exports.rs`, `state_expressions_literals.rs`, `state_expressions.rs`, `state_statements.rs`. Net-zero conformance. PR #1203.
- **2026-04-25** · branch `chore/span-conversion-test-matrix` · **DRY workstream-8.5 ready** · workstream 8 item 5 ("AST traversal and span semantics: Add span conversion tests for ASCII, multi-byte UTF-8, and surrogate-pair UTF-16"): added 7 matrix tests in `crates/tsz-common/tests/position_tests.rs` covering 2-byte (Latin-1 `é`), 3-byte (BMP `中`), 4-byte (surrogate-pair `🚀`), mixed-width round-trips, empty lines between content, long ASCII line (10k chars), and 4-byte after a newline. Surrogate-split clamps to start byte. Pure test additions — no production change. PR #1212.
- **2026-04-25** · branch `chore/parser-test-fixture` · **DRY workstream-8.9 ready** · workstream 8 item 9 ("Create parser/scanner/binder/lowering fixtures"): added `crates/tsz-parser/tests/test_fixture.rs` with `parse_source` and `parse_source_named`, wired as a `#[path]` mod under `parser/mod.rs`, migrated 6 duplicate sites in `decorator_tests.rs`, `parser_unit_tests.rs`, `state_expression_tests.rs`, `state_statement_tests.rs`, `state_declaration_tests.rs`, `state_type_tests.rs`. Pure refactor; 558 parser tests pass, net-zero conformance. PR #1219.
- **2026-04-25** · branch `chore/scanner-test-fixture` · **DRY workstream-8.9 active claim** · workstream 8 item 9 ("Create parser/scanner/binder/lowering fixtures") for the scanner: added `crates/tsz-scanner/tests/common/mod.rs` with `make_scanner(&str) -> ScannerState`, and migrated the 23 `ScannerState::new(source, true)` sites in `scanner_impl_tests.rs` to use it. Pure test consolidation — no production change. Mirrors the parser fixture shipped in PR #1219. Other scanner test files (`regex_unicode_tests.rs`, `scanner_comprehensive_tests.rs`) intentionally deferred to keep the slice small and reviewable. Draft PR title: `[WIP] chore(scanner/tests): add make_scanner fixture; migrate scanner_impl_tests`.

## How To Keep This Current

Agents and contributors should update this roadmap in the same PR as meaningful work when the work changes direction, status, metrics, risks, sequencing, or the active backlog.

Update this file when:

1. A PR lands or opens that changes a phase's status.
2. A measured metric changes: conformance, JS emit, declaration emit, fourslash, large-repo runtime/RSS, benchmark wins/losses, or failure-bucket counts.
3. A plan assumption is falsified by code review, profiling, conformance, CI, or implementation.
4. A new architecture direction is chosen or an old direction is abandoned.
5. A workstream produces a reusable design constraint that future agents must not rediscover.
6. You are about to create another plan document under `docs/plan/`.

Rules:

1. Keep this document concise enough to read before work starts.
2. Put durable design contracts in `docs/architecture/` or `docs/specs/` only when they are not roadmap/status material, and link them from this file.
3. Use the DRY Cleanup workstream below for DRY work; do not create a separate audit document.
4. Delete or rewrite stale notes instead of appending contradictory history.
5. Prefer dated status notes when a number may go stale.
6. If a PR intentionally does not update this roadmap, the PR description should make that clear when the work is roadmap-adjacent.

## Current Public Metrics

Source: `README.md` on 2026-04-25.

| Surface | Current |
| --- | ---: |
| Diagnostic conformance | `96.4%` (`12,129 / 12,582`) |
| JavaScript emit | `91.1%` (`12,325 / 13,526`) |
| Declaration emit | `76.5%` (`1,276 / 1,668`) |
| Fourslash / language service | `100.0%` rounded (`6,561 / 6,562`) |

Primary project goal: match `tsc` behavior closely enough to serve as a drop-in checker, emitter, and language service while becoming faster and more scalable on real repositories.

## Current Strategic Read

The top-level compiler phase split is sound:

```text
scanner -> parser -> binder -> checker/solver -> emitter
                                  -> LSP / CLI / WASM frontends
```

The implementation is transitional. The biggest risks are boundary drift and excessive mutable state, not the existence of the current crates.

Highest-risk areas:

1. `CheckerContext` is still a giant mutable coordination object.
2. CLI, LSP, WASM, and `tsz-core` still provide too many compiler front doors.
3. Checker, solver, and LSP boundaries are porous.
4. Speculation and cache invalidation still rely on caller discipline.
5. Large-repo performance is blocked by file-centric residency and per-file reconstruction.
6. Declaration emit pass rate depends on better upstream semantic summaries.
7. Incremental parser/interner coherence needs a targeted correctness check.

## Operating Principles

1. Conformance work and architecture work should reinforce each other. Prefer fixes that reduce the number of semantic paths.
2. Emit pass-rate work must not turn the emitter into a shadow checker. JS emit stays syntax plus transform policy; declaration emit consumes semantic summaries.
3. Large-repo performance moves from file residency toward stable semantic identity, skeleton indexes, and bounded arena retention.
4. CLI, LSP, WASM, and public facade crates should converge on one compiler service API.
5. Checker state should be split by lifetime before attempting broad purity.
6. Speculation should be transaction-safe: guard means rollback-on-drop unless committed; snapshot means explicit restore/commit.
7. Caches are architecture, not incidental optimization. Their keys, scope, invalidation, and rollback behavior must be explicit.
8. New architecture metrics should be cheap to count repeatedly so drift is visible.

## Architecture Health Metrics

Track these over time:

1. `CheckerContext` field count.
2. Frontend crates directly depending on parser/binder/checker/solver/emitter internals.
3. Direct solver imports outside solver/checker boundary modules.
4. Independent parse-bind-check orchestration paths.
5. Broad cache-clear and snapshot-restore call sites.
6. Speculation APIs with surprising non-RAII behavior.
7. LSP/WASM semantic features implemented outside the compiler service layer.
8. Declaration emit paths that rederive semantic facts instead of consuming summaries.

## Active Workstreams

### 1. Diagnostic Conformance And Fingerprints

Goal: make remaining diagnostic mismatches boring, measurable, and routed through shared semantic paths.

Current status:

1. Overall conformance is high enough that fingerprint quality, count, anchor, and high-volume semantic-code correctness now dominate.
2. Recent commits on `main` are mostly checker/solver/parser fixes around diagnostic display, optional parameter handling, literal display, parser recovery, and declaration emit.
3. The previous fingerprint plan's latest detailed snapshot was `96.2%` with `345` fingerprint-only failures; refresh this bucket from current artifacts before using the number for prioritization.

Near-term priorities:

1. Keep pair-aware type display as a checker-side finalization step, not a global solver formatter rewrite.
2. Separate diagnostic work into:
   - message rendering,
   - count and suppression policy,
   - anchor/location policy,
   - parser recovery.
3. Route high-volume `TS2322`, `TS2345`, and `TS2339` fixes through central assignability/property query boundaries.
4. Keep parser diagnostics separate from checker rendering.
5. Track top-code deltas for `TS2322`, `TS2345`, `TS2339`, `TS1005`, and `TS2353`.

Do not:

1. Globally rewrite `format_type_for_assignability_message` without a measured failure bucket.
2. Hide semantic behavior inside `DiagnosticRenderRequest`.
3. Fix under-count or over-count failures in display code.
4. Add checker-local relation heuristics when the root cause belongs in solver or query boundaries.

Exit criteria:

1. Fingerprint-only failures are no longer the dominant conformance bucket.
2. The Big3 diagnostic deltas shrink through shared relation/rendering paths.
3. Parser diagnostic fixes are measured independently from checker formatting fixes.

### 2. JS Emit And Declaration Emit Pass Rate

Goal: make emit pass-rate work as concrete and triaged as diagnostic conformance.

Current status:

1. JS emit is at `91.1%`.
2. Declaration emit is at `76.5%` and is the largest public pass-rate gap.
3. The emitter architecture is clear: lowering/directives/IR/printing for JS emit, and a separate declaration emitter that should consume semantic summaries.

JS emit priorities:

1. Keep JS baseline comparison stable and visible in CI metrics.
2. Bucket failures by transform family:
   - modules / CommonJS,
   - classes,
   - async/generator,
   - destructuring,
   - spread/rest,
   - private fields,
   - JSX,
   - source maps, if measured.
3. Fix high-volume transform failures through the lowering/directive/IR model.
4. Avoid new direct string-concatenation transforms for complex rewrites.

Declaration emit priorities:

1. Bucket declaration emit failures separately from JS emit.
2. Identify which failures need upstream semantic summaries rather than emitter-local rediscovery.
3. Define a narrow declaration emit semantic view for:
   - exported symbol identity,
   - public type display,
   - type-only/value usage,
   - import/export naming,
   - accessibility and visibility diagnostics.
4. Move declaration emit toward that view instead of broad binder/solver reach-through.

Architecture guardrails:

1. JS emit should not depend on the full solver for syntax utilities or transform helpers.
2. Declaration emit may depend on semantic views, but not broad checker/solver internals.
3. Syntax helpers such as operator classification belong in shared syntax/common APIs, not in semantic solver internals.

Exit criteria:

1. JS emit and declaration emit have separate top-failure reports.
2. Emit fixes are tied to transform families and measured against pass rate.
3. Declaration emit has a documented semantic input contract narrower than "whatever checker/solver state is reachable".

### 3. Compiler Service Front Door

Goal: stop CLI, LSP, WASM, and `tsz-core` from becoming separate compiler drivers.

Problem:

Frontends can currently reach the implementation through both a facade path and direct internal crate dependencies. That creates multiple parse-bind-check-emit flows and weakens incremental correctness.

Target shape:

```text
tsz-compiler-services
  project/session model
  parse/bind/check orchestration
  diagnostics
  emit API
  semantic query API
  incremental invalidation

tsz-cli
  command adapter

tsz-lsp
  protocol adapter

tsz-wasm
  JS/WASM adapter

tsz-core or tsz
  optional public facade, no WASM/LSP ownership
```

Required service operations:

1. `load_files`
2. `parse`
3. `bind`
4. `check`
5. `emit_js`
6. `emit_dts`
7. `diagnostics`
8. `quick_info`
9. `completion_items`
10. `signature_help`
11. `definition`
12. `references`

First useful PRs:

1. Add a service API shell that wraps the existing CLI/core pipeline with no behavior change.
2. Route one WASM multi-file check path through it.
3. Route one LSP semantic feature through it after file context is explicit.
4. Remove WASM and LSP concerns from `tsz-core`, preserving compatibility facades only where needed.

Exit criteria:

1. There is one blessed parse-bind-check path.
2. Frontends no longer construct independent checker/solver pipelines for ordinary operations.
3. `tsz-core` no longer owns WASM/LSP-specific behavior.

### 4. Checker State, Requests, And Speculation

Goal: make checker state understandable by lifetime and safe for caching, speculation, and reuse.

Problem:

`CheckerContext` mixes project state, file state, request state, scratch state, diagnostics, caches, speculation, class/function/module state, contextual typing state, JS/JSX state, recursion guards, `DefId` migration state, and binder/solver interop state.

Target grouping:

```rust
CheckerContext {
    project: ProjectSemanticState,
    file: FileCheckState,
    request: CheckRequestState,
    scratch: CheckScratch,
    diagnostics: DiagnosticSink,
    caches: CheckerCaches,
    speculation: SpeculationState,
}
```

Near-term priorities:

1. Extract `DiagnosticSink` and make speculative diagnostics transactional.
2. Extract `CheckerCaches` without changing semantics.
3. Extract a small `CheckRequestState` for contextual typing and relation request data currently passed implicitly.
4. Move request-sensitive meaning into explicit request structs.
5. Keep solver-owned facts out of checker scratch state.

Speculation policy:

1. RAII guard means rollback on drop unless explicitly committed.
2. Snapshot means caller must explicitly restore or commit.
3. Avoid names that imply rollback safety when dropping preserves speculative state.

Exit criteria:

1. Checker state explains lifetime and rollback behavior directly.
2. Speculative diagnostics cannot leak through a missed explicit method call.
3. New checker code has an obvious home for project, file, request, scratch, diagnostics, caches, and speculation state.

### 5. Stable Identity, Skeletons, And Large-Repo Residency

Goal: finish the large-repo architecture pivot from file-resident execution to stable semantic identity and bounded residency.

Current status:

1. Stable identity work has landed in multiple steps, including `StableLocation` and syntax rehydration helpers.
2. Several skeleton consumers have already moved to skeleton-derived indexes.
3. Some skeleton projections are unsafe when they depend on post-merge symbol identity; do not reopen failed file-local projections without correcting that design.
4. Large-repo work has shown that OOM/timeout behavior is memory-residency driven, not simply CPU-bound.
5. `instantiate_type` cache infrastructure has been designed and partially implemented through `QueryCache`/`QueryDatabase`; entry-point wiring and shared-cache decisions must preserve the design constraints below.

Non-negotiable identity rules:

1. `NodeIndex` is a local traversal coordinate, not cross-file semantic identity.
2. Binder/skeleton should own stable declaration locations and topological facts.
3. Checker should rehydrate syntax only when it truly needs source traversal.
4. Cross-file semantic reuse should be keyed by stable semantic identity.

`instantiate_type` cache constraints:

1. Cache hooks belong on `QueryDatabase`, not `TypeDatabase`.
2. Do not intern substitutions on `TypeInterner`; `QueryCache::clear()` and size accounting must own cache state.
3. Preserve leaf fast paths before cache-key construction.
4. `substitute_this_type` must not skip caching just because the substitution is empty; skip only when substitution is empty and `this_type` is absent.
5. Do not compare `TypeId`s across distinct `TypeInterner`s in tests.
6. Avoid `instantiate_generic` double-caching unless overlap with `application_eval_cache` is explicitly addressed.

Large-repo priorities:

1. Continue moving safe consumers from full binders/arenas to skeleton indexes.
2. Continue eliminating per-file deep clones of program-wide or file-owned maps where they block finishing large repo at all.
3. Measure peak RSS and timeout status after each residency change.
4. Treat Arc-sharing as Phase 0 plumbing, not the final architecture.
5. Move toward bounded user arena residency only after stable identity and skeleton consumers are proven.

Exit criteria:

1. Large repo finishes without OOM/timeout.
2. Cross-file lookups increasingly answer from stable identity and skeleton indexes.
3. Full AST/binder residency becomes a fallback, not the architecture.

### 6. LSP And WASM As Service Clients

Goal: keep editor and browser APIs from duplicating compiler semantics.

LSP target:

```rust
service.quick_info(file, position)
service.signature_help(file, position)
service.completion_items(file, position)
service.definition(file, position)
service.references(file, position)
service.diagnostics(file)
```

WASM target:

```rust
program.ensure_compiled_and_checked()
program.emit_js(...)
program.emit_dts(...)
program.diagnostics(...)
program.semantic_query(...)
```

Near-term priorities:

1. LSP becomes a protocol adapter over semantic service responses.
2. WASM keeps a stable JS-facing API while delegating implementation to one compiler service surface.
3. Diagnostic DTOs use one explicit shape and one span policy.
4. Semantic presentation models are shared before conversion to LSP/WASM DTOs.

Exit criteria:

1. Hover/completion/signature-help logic is not duplicated across checker, LSP, and WASM.
2. WASM no longer has parallel parser/program/checker implementations competing with core.
3. LSP request handling mostly maps protocol inputs to service queries and service outputs to protocol DTOs.

### 7. Incremental Parser And Interner Coherence

Goal: remove a concrete identity-risk bug from incremental parsing.

Concern:

Full parsing updates the parser arena's interner after parsing. Incremental parsing from an offset resets scanner text and parses a suffix into the existing arena, but must also keep scanner and arena interner state coherent.

Risk:

If the scanner interns a new identifier during incremental parse and the arena still has an older cloned interner, later identifier text resolution can be stale or empty. That can corrupt binding, exports, LSP results, and incremental diagnostics.

Deliverables:

1. Add a regression test where incremental parsing introduces a new identifier not present in the initial interner.
2. Update arena interner state after incremental parse, or move scanner/arena to a shared coherent interner handle.
3. Add a defensive identifier text resolution path only if it is consistent with the parser identity model.

Exit criteria:

1. Incremental parse, binder-visible identifier text, and LSP-visible names remain coherent after suffix edits.

### 8. DRY Cleanup

Goal: reduce repeated compiler setup, repeated semantic plumbing, and duplicated helper logic without mixing unrelated work into oversized PRs.

Status:

1. Former standalone audit date: 2026-04-21; last validation pass: 2026-04-25.
2. Scope covered the full workspace: Rust crates, conformance tooling, WASM bindings, CLI/server code, LSP code, tests, and scripts.
3. The audit prioritized repeated behavior that can drift semantically over cosmetic duplication.
4. Exact line numbers from the original audit are evidence anchors, not durable references. Recount call sites before starting any helper sweep.

DRY loop rules:

1. Pick one small refactor slice per PR.
2. Check current repo and GitHub state before claiming work:
   - `git fetch origin`
   - `git status --short`
   - `gh pr list --state open --limit 100 --json number,title,headRefName,url,body`
   - `gh pr list --state merged --limit 100 --json number,title,mergedAt,url`
3. Search open and recently merged PRs by planned symbols and paths, not just broad section names.
4. Claim the slice in `Active Implementation Claims`, open a draft PR with the `WIP` label, then begin implementation.
5. Prefer one helper or fixture plus a small representative migration, one crate-local consolidation with tests, one bug-shaped finding with a targeted regression test, or one script/helper extraction with callers migrated in the same area.
6. Preserve behavior unless the selected item is explicitly a bug fix.
7. Add or update tests that lock the shared invariant, not just the migrated call sites.
8. End successful iterations with `scripts/session/verify-all.sh`.
9. Before marking ready, update this roadmap with landed status, remove `WIP`, remove any `[WIP]` / `[do not merge]` title prefix, and mark the PR ready.
10. If unrelated dirty files are present, stop and resolve the handoff instead of silently omitting them.

Landed since the original audit:

1. Compiler option / lib metadata: canonical target/module/moduleResolution parse/display/numeric round-trips in common/core; script-side enum conversions landed.
2. Checker test harness: `check_source_code_messages` added to `tsz-checker::test_utils`; 10 checker test files migrated; test utilities exported for integration tests.
3. Conformance runner: diagnostic comparison collapsed into `compare_diagnostics`; server-mode fingerprint skip fixed when one side has no fingerprints.
4. WASM/API: option/lib cache, code-action context, program DTOs, transform context, core utility exports, program/parser wrappers, and shared target/module deserialization landed.
5. AST traversal/span: parser accessor child double-push fixed.
6. Common/checker/solver/emitter/CLI/LSP: multiple helper sweeps landed for numeric parsing, assignment operator helpers, symbol declaration helpers, `Symbol::has_any_flags`, query cache constructors, diagnostic collection, solver constraint helpers, substitution helpers, identity-check-mode scoping, emitter numeric helpers, build-clean path handling, and code-lens reference counts.

Highest priority remaining DRY work:

1. Test harness consolidation:
   - Checker tests still repeat `ParserState::new`, `BinderState::new`, and `CheckerState::new` heavily.
   - Continue migrating local wrappers to crate-local fixtures and `tsz_checker::test_utils`.
   - Add or strengthen `CheckerFixture`, `ProjectFixture`, `ServerFixture`, `EmitterFixture`, `SolverFixture`, and `ModuleResolutionFixture` where useful.
   - Convert high-volume repeated tests to table-driven loops when failure names remain readable.
2. LSP provider context and reference occurrence model:
   - Add `ProjectFileContext` / `with_file_context` helpers to centralize file name, arena, binder, source text, line map, cache, touch, and timing behavior.
   - Introduce a shared `CursorTarget` resolver for definition, type definition, hover, rename, references, highlights, and code lens.
   - Make a single `ReferenceCollector` return rich occurrences with file, range, node, symbol/file identity, access kind, and declaration/reference classification.
   - Move file rename range/module specifier replacement into `rename/file_rename.rs`.
3. Conformance runner backend consolidation:
   - Define a `RunnerBackend` trait for local, batch, and server execution.
   - Share process-pool lifecycle, timeout, RSS, restart, test discovery, and cache discovery code.
   - Keep Python result parsing/diffing/code extraction single-source.
4. WASM API surface, options, and diagnostics DTOs:
   - Pick one canonical WASM implementation surface and treat the other as a compatibility facade.
   - Centralize option DTO conversion, diagnostic DTO conversion, `ensure_compiled_and_checked`, and emit-from-arena behavior.
   - Use one explicit byte/UTF-16 span policy.
5. AST traversal and span semantics:
   - Make node traversal APIs easier than local recursion.
   - Standardize byte span, UTF-16 LSP range, and diagnostic start/length conversions.
   - Add span conversion tests for ASCII, multi-byte UTF-8, and surrogate-pair UTF-16.
   - Group mutable per-file binder state so reset behavior is not field-by-field.
6. Checker residuals:
   - Recount and continue `Symbol::primary_declaration()` migration across checker, LSP, CLI, and binder.
   - Add helpers for cross-file child checker lifecycle, heritage/member iteration, CommonJS export LHS classification, and common assignability diagnostic builders.
   - Consolidate checker tests around one fixture and diagnostic assertion DSL.
7. Solver residuals:
   - Finish remaining rest-parameter constraint helpers, temporary type predicate annotation state, object/index constraint handling, subtype/assignability cache path deduplication, and solver test builders.
8. Emitter residuals:
   - Introduce shared transform context with temp allocator, helper request tracking, and module/import/export classification.
   - Replace repeated visitor skeletons with traversal adapters.
   - Audit module temp naming where paths may always select `{module}_1`.
   - Add `emit_test_support` with parser/print helpers and table-case support.
9. Parser/scanner/binder/lowering residuals:
   - Migrate local walkers to shared AST traversal infrastructure.
   - Move hardcoded modifier/keyword token lengths into scanner/parser metadata.
   - Create parser/scanner/binder/lowering fixtures.
10. Scripts:
   - Add or continue shared compiler-option extraction and output-path helpers for emit scripts.
   - Use stable JSON plus `crypto.createHash("sha256")` for cache hashing.
   - Keep conformance/fourslash snapshot query helpers single-source.
   - Pick one README metrics updater or make both consume one declarative suite config.

Bug-shaped findings still worth triage:

1. `tsz-lsp` heritage index cleanup may still leave stale `sub_to_bases` relationships on file removal.
2. `tsz-emitter` CommonJS/module IR temp naming may always choose `{module}_1` in some paths.

Verification:

1. Prefer behavior locks before broad replacement.
2. Run targeted crate tests for the touched helper plus `scripts/session/verify-all.sh` before marking ready.
3. For option/diagnostic/span work, add matrix tests that make the shared invariant explicit.
4. For status-sensitive helper sweeps, recount call sites before and after the PR.

### 9. Sound Mode

Goal: keep Sound Mode honest, narrow, and compatible with the architecture while it remains experimental.

Current status:

1. Sound Mode is partially implemented as a project-wide boolean.
2. Method bivariance tightening is live through relation policy.
3. `any` handling is partial; top-level `any` remains too permissive for the target contract.
4. Sticky freshness is currently active under sound mode but should be treated as pedantic, not part of the first stable core contract.
5. Dedicated public TSZ sound diagnostics, code-aware suppressions, report-only behavior, and declaration-boundary projection are not yet complete.

First stable target:

1. Scope only user-authored `.ts`, `.tsx`, `.mts`, and `.cts` implementation code.
2. Keep `.d.ts`, generated libs, third-party declarations, JS, and JSDoc out of the first stable scope by default.
3. Ban explicit `any` in sound-scoped user source.
4. Disable method bivariance in sound-scoped assignability.
5. Imply `useUnknownInCatchVariables`, `noUncheckedIndexedAccess`, and `exactOptionalPropertyTypes`.
6. Emit dedicated TSZ sound diagnostics.
7. Add code-aware `@tsz-unsound` with required reasons and stale-suppression checking.
8. Treat declaration files as trust boundaries, but do not promise general quarantine in the first stable release.

Boundary and overlay direction:

1. Write a precise boundary projection design before implementing general declaration-origin `any` quarantine.
2. Prefer projected sound views for arbitrary declaration inputs.
3. Use curated internal overlays only for small, high-value surfaces such as `JSON.parse(): unknown` or `Response.json(): Promise<unknown>`.
4. Keep projected semantic caches separate from persistent overlay object caches.
5. If persistent overlays are built, use content-addressed objects with manifest validation and atomic publish; packages and project declarations may share one object store but must have distinct subject identities.

Exit criteria for first stable Sound Mode:

1. CLI and server/editor paths both honor sound mode.
2. The documented scope exactly matches live behavior.
3. Non-sound mode parity is unaffected.
4. Dedicated diagnostics and suppressions work for direct sound diagnostics.
5. Boundary projection remains explicitly later or experimental.

## Recommended Sequencing

1. Continue focused diagnostic conformance fixes while current fingerprint momentum is active.
2. Add emit pass-rate triage reports and choose JS/declaration emit fixes by bucket.
3. Start the compiler service shell without moving all frontends at once.
4. Fix speculation transaction semantics before broad checker state work.
5. Split `CheckerContext` by lifetime in non-behavioral batches.
6. Continue skeleton/stable-identity migrations that directly reduce large-repo residency.
7. Move one WASM path and one LSP path to the compiler service API as proof points.
8. Fix incremental parser/interner coherence with a targeted regression test.
9. Keep Sound Mode work limited to the first stable scope until diagnostics, suppressions, and policy cache correctness are real.
10. Run DRY cleanup as small WIP-claimed PRs that reduce future conformance, emit, or architecture work.

## Definition Of Done

This roadmap is succeeding when:

1. Conformance and emit pass-rate work move the public metrics every week.
2. The number of independent compiler orchestration paths goes down.
3. `CheckerContext` field count and ambient mutable request state go down.
4. LSP and WASM imports of checker/solver internals go down.
5. Speculation APIs are mechanically safe.
6. Large-repo runs finish without OOM and then become faster.
7. Declaration emit relies on upstream semantic summaries instead of late semantic rediscovery.
8. Sound Mode remains a narrow, honest product surface instead of a grab bag of half-wired checks.
9. DRY cleanup reduces repeated behavior without creating broad, risky refactor PRs.
