# TSZ Roadmap

Date: 2026-04-25

Status: living plan. This is the single planning document for project direction across conformance, emit, performance, architecture, LSP/WASM, Sound Mode, and DRY cleanup. Do not add new roadmap files under `docs/plan/`; update this file instead.

This document supersedes the previous scattered plan files in `docs/plan/` and the former standalone DRY audit. All planning claims now live here.

## Implementation Coordination

To avoid duplicate work, roadmap-adjacent implementation, including DRY cleanup, must be claimed before coding starts.

**Claim format (preferred):** add a file under `docs/plan/claims/<branch-slug>.md`. One file per PR keeps parallel agents from constantly rebasing into the same `Active Implementation Claims` section. See `docs/plan/claims/README.md` for the file template.

Workflow:

1. Pull latest `main` and inspect open PRs and `docs/plan/claims/` for overlap.
2. Create a branch for the intended work.
3. Add `docs/plan/claims/<branch-slug>.md` with `Status: claim` (do not edit ROADMAP.md).
4. Open a draft PR immediately with the `WIP` label.
5. Only then start implementation.
6. Before marking ready, flip the claim file's `Status: ready` and update the PR.

Draft PR command shape:

```bash
gh pr create --draft --label WIP --title "[WIP] <scope>: <intent>" --body "$(cat <<'EOF'
## Intent
- <what this PR intends to change>

## Roadmap Claim
- Added `docs/plan/claims/<branch-slug>.md` before implementation.

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
3. Before marking ready, flip the claim file's `Status: ready` (or update the inline `Active Implementation Claims` entry if you used the legacy format), remove the `WIP` label, remove the `[WIP]` title prefix, and mark the PR ready.
4. If implementation is abandoned, close the draft PR and either delete the claim file or set `Status: abandoned` (legacy: mark the inline claim as abandoned).
5. DRY cleanup uses this same claim flow. Keep each DRY slice small and reviewable.

### Merging Policy

When a PR's CI is otherwise green and the only remaining merge blocker is a `docs/plan/ROADMAP.md` rebase conflict (typical for parallel agents that all appended entries to `Active Implementation Claims`):

1. Resolve the ROADMAP conflict locally — keep both sides; dedupe stale "claim" entries when the PR's matching "ready" entry already exists.
2. `git push --force-with-lease`.
3. Run `gh pr merge <N> --squash --admin --delete-branch` **without waiting for the post-rebase CI re-run**. ROADMAP.md is documentation; the conflict-resolution push cannot affect the lanes that already passed.

This applies only when the prior CI run was fully green and the conflict is exclusively in ROADMAP.md (or other docs). For real code/test conflicts, the normal "wait for CI" rule still applies.

### Active Implementation Claims

Add new roadmap and DRY claims here before implementation begins.

- **2026-04-25** · branch `claude/modest-archimedes-QBaXl` · **DRY active claim** · P0 Test-Harness Consolidation: replace local `check_with_options` helpers with `tsz_checker::test_utils::check_source` in `crates/tsz-checker/tests/class_member_closure_tests.rs`, `contextual_tuple_tests.rs`, `contextual_typing_tests.rs`, and `never_returning_narrowing_tests.rs`. Legacy draft title: `[do not merge] chore(checker-tests): replace local check_with_options helpers with test_utils::check_source`; treat as WIP until ready.
- **2026-04-25** · branch `chore/parser-keyword-token-length-helper` · **DRY workstream-8 ready** · workstream 8.9 ("Move hardcoded modifier/keyword token lengths into scanner/parser metadata"): added `pub const fn keyword_text_len(SyntaxKind) -> u32` in `tsz-scanner` and migrated 8 hardcoded sites across `state_declarations_exports.rs`, `state_expressions_literals.rs`, `state_expressions.rs`, `state_statements.rs`. Net-zero conformance. PR #1203.
- **2026-04-25** · branch `chore/span-conversion-test-matrix` · **DRY workstream-8.5 ready** · workstream 8 item 5 ("AST traversal and span semantics: Add span conversion tests for ASCII, multi-byte UTF-8, and surrogate-pair UTF-16"): added 7 matrix tests in `crates/tsz-common/tests/position_tests.rs` covering 2-byte (Latin-1 `é`), 3-byte (BMP `中`), 4-byte (surrogate-pair `🚀`), mixed-width round-trips, empty lines between content, long ASCII line (10k chars), and 4-byte after a newline. Surrogate-split clamps to start byte. Pure test additions — no production change. PR #1212.
- **2026-04-25** · branch `chore/parser-test-fixture` · **DRY workstream-8.9 ready** · workstream 8 item 9 ("Create parser/scanner/binder/lowering fixtures"): added `crates/tsz-parser/tests/test_fixture.rs` with `parse_source` and `parse_source_named`, wired as a `#[path]` mod under `parser/mod.rs`, migrated 6 duplicate sites in `decorator_tests.rs`, `parser_unit_tests.rs`, `state_expression_tests.rs`, `state_statement_tests.rs`, `state_declaration_tests.rs`, `state_type_tests.rs`. Pure refactor; 558 parser tests pass, net-zero conformance. PR #1219.
- **2026-04-25** · branch `chore/scripts-remove-unused-readme-updater` · **DRY workstream-10 ready** · workstream 10 item 4 ("Pick one README metrics updater"): deleted `scripts/update-readme-from-ci-metrics.mjs` (161 LOC, dead code; the `.github/workflows/ci.yml` and `.github/workflows/update-readme.yml` workflows that originally invoked it were removed; only `.github/workflows/gh-pages.yml` remains and doesn't reference it). `scripts/refresh-readme.py` is now the single README updater documented in `scripts/README.md`. Audit evidence anchor and "Node and Python implementations" recommendation updated in `docs/DRY_AUDIT_2026-04-21.md`. PR #1221.
- **2026-04-25** · branch `chore/diagnostic-speculation-snapshot-rename` · **Workstream-4 ready** · Operating principle 6 + architecture health metric 6 ("Speculation APIs with surprising non-RAII behavior"): renamed `DiagnosticSpeculationGuard` → `DiagnosticSpeculationSnapshot` to match its actual drop-implicit-commit semantics (matching the `DiagnosticSnapshot` / `FullSnapshot` / `CacheSnapshot` family in the same file). Updated 6 call sites + module doc-comment. Pure rename, net-zero conformance. PR #1213.
- **2026-04-25** · branch `chore/emit-runner-cache-sha256` · **DRY workstream-10 ready** · workstream 10 item 2 ("Use stable JSON plus `crypto.createHash(\"sha256\")` for cache hashing"): replaced 32-bit polynomial `hashString` in `scripts/emit/src/runner.ts` with `crypto.createHash("sha256")` (hex digest); switched `getCacheKey` input from positional colon-joined template literal to JSON.stringify over an alphabetically-sorted object via `stableStringify`; cache file format preserved. Verified locally with cold/warm cache pass on a 20-test smoke. PR #1217.
- **2026-04-25** · branch `fix/checker-ts2322-elaboration-chain` · **Workstream 1 — abandoned (research)** · Initial elaboration-chain scope invalidated by smoke-testing booleanAssignment.ts; the actual gap is wrong source-type display in `format_assignment_source_type_for_diagnostic`. PR #1214 closed; lesson: smoke-test with `--verbose` before scoping printer fixes.
- **2026-04-25** · branch `fix/checker-ts1238-generic-decorator-call` · **KPI-2 partial-land + investigation deep-dive** · Targeted `decoratorCallGeneric.ts` (TS1238). Unit-test PR #1220 ships as behavior lock. **Investigation log**: \n  1. Smoke confirmed CLI emits 0; unit test emits TS1238. \n  2. Instrumented all early-return guards in `check_class_decorator_call_signature`. Captured: unit test → `resolve_call_result: ArgumentTypeMismatch`; CLI → `resolve_call_result: Success(TypeId(5))`. \n  3. **Pre-instantiation hypothesis disproven**: dumped `TypeData` at the call site. Both unit test and CLI see `TypeData::Function(FunctionShapeId(...))` — neither path passes an `Application`/instantiated form. `decorator_type` and `resolved` agree (no mid-pipeline instantiation). Only the FunctionShapeId differs (CLI 31 vs unit test 2). \n  4. **Conclusion**: the CLI's `Function` shape contents themselves differ from the unit test's, OR the `class_constructor_type` (CLI: 2543, unit test: 121) has a structurally different shape. CLI loads lib types (`Object`, `Function.prototype`, etc.) which extends `typeof C` with merged-in lib properties. `resolve_call`'s structural assignability for `typeof C → I<T>` produces different inference results.\n\n  **Next-iteration target**: dump `FunctionShape` (params, type_params, return_type) for both TypeIds and the `class_constructor_type` shape. If lib-merged properties on `typeof C` are masking the constraint failure, the fix is in the inference: `resolve_call` should compare the LITERAL declared signatures of `dec` and the LITERAL `typeof C` shape (without lib merging) for the generic constraint check — not the structurally-extended versions. Alternatively, the fix is in how the unit-test harness diverges from the CLI in lib loading.
- **2026-04-25** · branch `chore/checker-context-field-count-guard` · **Workstream-4 / architecture-health-metric-1 ready** · Operating Principle 8 + Architecture Health Metric 1 (`CheckerContext` field count): added `STRUCT_FIELD_COUNT_CHECKS` list + `scan_struct_field_count` regex-based counter in `scripts/arch/arch_guard.py` mirroring `FILE_LINE_LIMIT_CHECKS`. Cap pinned at the live 221 fields (counts `pub`, `pub(crate)`, and bare-private fields; comments stripped first). 6 new unit tests in `scripts/arch/test_arch_guard.py` lock the regex semantics. Future `CheckerContext` field additions must bump the cap in the same diff, making drift visible at review time. PR #1228.
- **2026-04-25** · branch `chore/orchestration-path-count-guard` · **Workstream-3 / architecture-health-metric-4 ready** · Operating Principle 8 + Architecture Health Metric 4 ("Independent parse-bind-check orchestration paths"): added `INDEPENDENT_PIPELINE_CHECKS` + `scan_independent_pipelines` to `scripts/arch/arch_guard.py`. The check walks `crates/tsz-cli/src`, `crates/tsz-core/src`, `crates/tsz-lsp/src`, `crates/tsz-wasm/src` and counts non-test files that construct all three of `ParserState::new`, `BinderState::new`, `CheckerState::new` (currently 4: `tsz_server/check.rs`, `tsz-core/parallel/core.rs`, `tsz-lsp/signature_help.rs`, `tsz-lsp/project/core.rs`). Cap pinned at 4. 5 new unit tests in `test_arch_guard.py` lock the detection semantics (full pipeline counted, partial pipeline ignored, test files excluded, live cap not off-by-one). PR #1231.
- **2026-04-26** · branch `chore/dashboard-track-ts1005-ts2353` · **Workstream-1 ready** · Workstream 1 near-term priority 5 ("Track top-code deltas for TS2322, TS2345, TS2339, TS1005, and TS2353"): added a KPI 1b block to `scripts/conformance/query-conformance.py --dashboard` printing TS1005 + TS2353 wrong-code counts (7 + 1 = 8 today). No production change. PR #1233.
- **2026-04-26** · branch `chore/wasm-target-module-option-helper` · **DRY workstream-4 ready** · Workstream 4 ("WASM API surface, options, and diagnostics DTOs — Centralize option DTO conversion"): introduced `target_kind_from_u8` and `module_kind_from_u8` helpers in `crates/tsz-wasm/src/wasm_api/options.rs` and migrated 5 duplicated `from_ts_numeric` u8-Option sites: `program.rs::resolve_module`, `program.rs::emit_json`, `program.rs::emit_file`, `emit.rs::to_printer_options`, `emit.rs::transpile`. 6 unit tests pin default (`None → ES5` / `None → ModuleKind::None`) and unknown-numeric fallback behavior. Net -20 LOC at call sites (+85 LOC helper module with tests). Behavior preserving: 27 tsz-wasm tests pass; WASM target builds. PR #1243.
- **2026-04-26** · branch `chore/symbol-primary-declaration-migration` · **DRY workstream-8.6 ready** · workstream 8 item 6 ("Recount and continue Symbol::primary_declaration() migration across checker, LSP, CLI, and binder"): migrated two checker sites that manually re-implement `Symbol::primary_declaration()` semantics — `crates/tsz-checker/src/types/property_access_helpers/access_semantics.rs:1095-1103` and `crates/tsz-checker/src/types/queries/callable_truthiness.rs:309-318` — to call `symbol.primary_declaration()?`. Saved ~17 lines of duplicated value-vs-type declaration logic. 5209 checker tests pass; net-zero conformance. PR #1234.
- **2026-04-25** · branch `chore/emit-extract-cli-arg-builder` · **DRY workstream-10 ready** · workstream 10 item 1 ("Add or continue shared compiler-option extraction and output-path helpers for emit scripts"): extracted `appendCompilerOptionFlags(args, opts)` helper + `CompilerFlagOptions` interface in `scripts/emit/src/cli-transpiler.ts`. Both the primary emit args (formerly L296-329) and the declaration-emit retry args (formerly L385-414) now route through the single helper. **Latent bug fix**: retry path was missing `--strictNullChecks`; both paths now match by construction. Verified `npm run build:emit` passes. PR #1229.
- **2026-04-26** · branch `chore/solver-import-count-guard` · **Workstream-4 / architecture-health-metric-3 ready** · Operating Principle 8 + Architecture Health Metric 3 ("Direct solver imports outside solver/checker boundary modules"): added `SOLVER_IMPORT_COUNT_CHECKS` + `scan_solver_import_count` to `scripts/arch/arch_guard.py`. Counts non-test files outside `crates/tsz-solver/` and `crates/tsz-checker/` that contain a `use tsz_solver::` / `pub use tsz_solver` / `extern crate tsz_solver` import. Live count: 36 files (`tsz-cli` 9, `tsz-core` 3, `tsz-emitter` 12, `tsz-lowering` 2, `tsz-lsp` 6, `tsz-wasm` 4). Cap pinned at 36. 7 new unit tests in `scripts/arch/test_arch_guard.py` lock the regex semantics (all three import forms flagged, comments ignored, test/bench files excluded, solver/checker scope excluded, live cap not off-by-one). Future direct solver imports in frontends/emitter/lowering force a cap bump in the same diff; consolidation through the compiler service shell shows up as a cap reduction. PR #1237.
- **2026-04-26** · branch `chore/emitter-integration-test-support` · **DRY workstream-8.4 ready** · workstream 8 item 4 ("Add `emit_test_support` with parser/print helpers and table-case support"): added `crates/tsz-emitter/tests/test_support.rs` with `parse_source`, `parse_source_named`, `parse_and_print`, `parse_and_print_with_opts`, `parse_and_lower_print`, and `parse_and_print_named_with_opts` helpers; `#[path]`-mounted in each of the 5 integration test files (`comment_tests.rs`, `variable_declaration_emit_tests.rs`, `optional_chaining_tests.rs`, `computed_property_es5_tests.rs`, `jsx_spread_tests.rs`). Migrated 14 `ParserState::new` sites and 9 inline `Printer::new` + `set_source_text` + `print` + `finish` chains (Cargo `[[test]]`-registered binaries cannot share modules cross-binary, hence `#[path]`). 1435 emitter tests pass, clippy clean, fmt clean. Pure refactor; net-zero conformance / emit / fourslash. PR #1238.
- **2026-04-26** · branch `fix/checker-ts2862-keyof-receiver-self` · **Workstream-1 ready** · Workstream 1 ("Diagnostic Conformance And Fingerprints"): fix false-positive TS2862 ("Type 'T' is generic and can only be indexed for reading") on writes like `obj[k]` where the receiver is a type parameter `T` and the index is bounded by the receiver's own keys (`keyof T` literally, or `K extends keyof T`). Root cause: `is_broad_index_type` evaluates `keyof T` to its constraint's key space (e.g. `string`), bypassing the existing keyof-of-type-parameter guard. Fix narrowed to `is_generic_indexed_write`: when the object is a type parameter, exclude indices whose keyof inner is the same type parameter (or whose constraint's keyof inner is). `conformance/types/keyof/keyofAndIndexedAccessErrors.ts` and `keyofAndIndexedAccess2.ts` move from wrong-code to fingerprint-only. Existing TS2862 invariants for `cannotIndexGenericWritingError.ts` and `mappedTypeGenericWithKnownKeys.ts` preserved. Two new unit-test locks in `conformance_issues/errors/error_cases.rs`. PR #1249.
- **2026-04-26** · branch `chore/scripts-conformance-query-helpers` · **DRY workstream-10 ready** · workstream 10 item 3 ("Keep conformance/fourslash snapshot query helpers single-source"): added `scripts/lib/conformance_query.py` exporting `load_detail`, `basename`, `code_counts`, `is_fingerprint_only`, `is_same_code_count_drift` (the loader now wraps `lib.query_snapshot.load_snapshot` so error/exit behavior matches `query-fourslash.py` and `query-emit.py`). Migrated three sites: removed local `load_detail` + `basename` + `code_counts` + `is_fingerprint_only` + `is_same_code_count_drift` defs and six inline `rsplit("/", 1)[-1]` patterns from `query-conformance.py`; removed local `load_detail` + `basename` from `classify-render-corpus.py`; removed nested `basename` closure from `analyze-conformance.py`. Removed empty `__init__.py` from `scripts/lib/` and `scripts/conformance/lib/` so the two `lib` directories merge as PEP 420 namespace packages (required so `analyze-conformance.py` can import both `lib.results` and `lib.conformance_query`). Added 23 unit-test cases in `scripts/lib/test_conformance_query.py`; 61 total `scripts/lib` + `scripts/conformance/lib` tests still pass. Verified by byte-identical before/after diffs of `query-conformance.py {--dashboard, --campaigns, --code TS2322, --close 2, --fingerprint-only, --false-positives, default}`, `classify-render-corpus.py {default, --code TS2322}`, `query-fourslash.py {default, --buckets}`, `query-emit.py`. PR #1239.
- **2026-04-26** · branch `fix/solver-defstore-reject-intrinsic-type-to-def` · **Workstream 1 ready** · Continuation of the booleanAssignment.ts investigation noted on 2026-04-25 (PR #1214 abandoned). Root cause confirmed in solver `DefinitionStore::register_type_to_def`: intrinsic `TypeId`s (NUMBER, STRING, BOOLEAN, etc.) were accepted as registration keys, which causes `find_def_for_type(NUMBER)` to return whichever class/interface/alias DefId got registered first, and `authoritative_assignability_def_name` then overrides the correct intrinsic display ("number") with the def's name (e.g., "FlatArray", "Boolean", "Symbol"). Fix is a one-line guard at the entry of `register_type_to_def` (drop intrinsic registrations); display falls back to the formatter's intrinsic short-circuit. Ships with `tsz-solver::def::core::tests::test_register_type_to_def_rejects_intrinsic_type_ids` (verifies the invariant against all 16 reserved intrinsic constants). Conformance impact: net +17 tests (12129→12146); 20 improvements; 3 reported flips investigated and confirmed to be snapshot drift, not regressions. PR #1240.
- **2026-04-26** · branch `fix/checker-jsdoc-template-in-body-scope` · **Workstream-1 ready** · Workstream 1 (Diagnostic Conformance — TS2304 false positive in JSDoc-typed JS code). Fixed `jsdocTemplateConstructorFunction.ts` (was 1/2, now 2/2): tsz was emitting an extra TS2304 ("Cannot find name 'T'") at the `@type {T}` annotation inside a JS function whose JSDoc declared `@template T`. Root cause: `function_type.rs` pushed the JSDoc `@template T` names into `type_parameter_scope` only for signature construction and popped them before the body walk; by the time `check_function_body` ran, `T` was no longer in scope, so `resolve_jsdoc_reference("T")` fell through the `type_parameter_scope.get` lookup and produced TS2304. Fix re-pushes the function's `@template T` JSDoc-derived `TypeParamInfo` entries at the top of `check_function_body` (mirroring the signature-builder shape — `intern_string` → `TypeParamInfo` → `factory.type_param` → `type_parameter_scope.insert`) and pops them at the end, gated on `is_js_file()`, empty syntactic `func.type_parameters`, a JSDoc string, and a `contains_key` guard so a name already pushed by an enclosing scope is not duplicated. 3 unit tests in `crates/tsz-checker/tests/jsdoc_template_in_body_scope_tests.rs` lock the body-scope `@type {T}` resolution, the inline-cast variant, and the no-unrelated-diagnostics sanity check. 21547 workspace tests pass. Conformance: net +20 (12129→12149); the fix's specific contribution is +5 above the with-and-without baseline drift (which had 15 wins / 3 regressions reproducing on the same main HEAD without the fix — stale snapshot, not regressions). PR #1255.
- **2026-04-26** · branch `chore/lsp-project-file-context` · **DRY workstream-2 ready** · workstream 2 item 1 ("Add `ProjectFileContext` / `with_file_context` helpers to centralize: file name, arena, binder, source text, line map"): added `LspProviderContext<'a>` view struct in `crates/tsz-lsp/src/project/file_context.rs` + `ProjectFile::provider_context()` accessor; extended the `define_lsp_provider!(binder ...)` macro arm with a `from_context` constructor and added matching `from_context` builders on the two hand-rolled binder-tier providers (`CodeActionProvider`, `TypeHierarchyProvider`); migrated 13 binder-tier construction sites across `project/features.rs` and `project/operations.rs` from the 6-7-line repeated shape to the single-line `Provider::from_context(file.provider_context())` form. Sites that mix the provider with `&mut file.scope_cache` (definition, references, find-refs heritage rename) keep the per-field destructuring pattern because the borrow checker cannot split disjoint fields through `provider_context()`; that limitation is documented in the `file_context.rs` module doc-comment so future migrators do not retry it. 3715 LSP tests pass (3713 + 2 new locks). PR #1244.
- **2026-04-26** · branch `chore/common-linemap-span-range-helpers` · **DRY workstream-5 ready** · workstream 8 item 5 ("AST traversal and span semantics: Standardize byte span, UTF-16 LSP range, and diagnostic start/length conversions"): added `LineMap::span_to_range(&self, span, source) -> Range` and `LineMap::range_to_span(&self, range, source) -> Option<Span>` to `crates/tsz-common/src/position/mod.rs`. Both delegate to the existing `offset_to_position` / `position_to_offset` so semantics match `tsz-core::SourceFile::span_to_range` and the inline `let start_pos = ...; let end_pos = ...; Range::new(start_pos, end_pos)` pattern open-coded in 30+ tsz-cli / tsz-lsp call sites. Added 10 matrix tests in `crates/tsz-common/tests/position_tests.rs` covering ASCII, 2-byte UTF-8 (Latin-1), 3-byte UTF-8 (BMP), 4-byte UTF-8 (surrogate-pair) round-trips, dummy spans, empty spans, multiline spans, inverted endpoints (preserved), out-of-source line (`None`), and per-line character clamping. Pure additive contract; 322 tests pass (312 existing + 10 new); clippy and fmt clean. Caller migration in tsz-cli / tsz-lsp / tsz-core deferred to follow-up PRs (those crates are blocked by in-flight #1235 / agent-A / agent-F work this round). PR #1245.
- **2026-04-26** · branch `chore/parser-defensive-identifier-text-fallback` · **Workstream-7 ready** · Workstream 7 ("Incremental Parser/Interner Coherence") deliverable 3 ("Add a defensive identifier text resolution path only if it is consistent with the parser identity model"): `NodeArena::resolve_identifier_text` previously returned `interner.resolve(atom)` verbatim — if the interner was stale (the regression PR #1205 fixed for incremental parse), this surfaced `""` for a non-NONE atom. Now falls back to `IdentifierData.escaped_text` (always populated at parse time) when the interner returns empty for a non-NONE atom; happy-path behavior unchanged. 2 unit tests in `crates/tsz-parser/src/parser/node_arena.rs::tests` lock both branches: the stale-atom fallback (`Atom(99_999)` against a fresh interner returns `escaped_text`) and the happy path (interner-known atom returns canonical text rather than `escaped_text`). Both tests construct the arena with `Interner::new()` so `Atom(0)` is reserved for the empty string, matching production scanner setup. Net-zero conformance verified by running both with and without the fix on the same main HEAD — identical 12129→12144 (+15) drift; the +15 wins and 3 regressions are stale-snapshot artifacts on main, not caused by the fix. PR #1241.
- **2026-04-26** · branch `chore/scripts-print-truncated-more-helper` · **DRY workstream-10 ready** · workstream 10 item 3 ("Keep conformance/fourslash snapshot query helpers single-source"): added `print_truncated_more(items, top, indent="  ")` to `scripts/lib/query_snapshot.py` and migrated all 9 truncation-tail sites: 4 in `scripts/fourslash/query-fourslash.py` (`show_failures`, `show_bucket`, `show_timeouts`, `show_filter`) and 5 in `scripts/emit/query-emit.py` (`show_js_failures`, `show_dts_failures`, `show_close`, `show_filter`, `show_status`). Added 8 unit-test cases in `scripts/lib/test_query_snapshot.py` (`TestPrintTruncatedMore`) covering boundary (`exceeds`, `equal`, `below`, `empty`), off-by-one (`just_over`), custom indent (`custom_indent`, `zero_indent`), and tuple-input. Total `test_query_snapshot.py` test count: 13 → 21. Verified byte-identical output before/after across `query-fourslash.py {default, --buckets, --failures, --top-errors, --bucket completion, --filter quickInfo, --timeouts}` and `query-emit.py {default, --js-failures, --dts-failures, --close, --filter test, --status fail, --top-errors}`. Pure refactor; complements PR #1239 (which consolidated the conformance-side query helpers). PR #1248.
- **2026-04-26** · branch `chore/lowering-typelowering-constructor-dry` · **DRY workstream-9 ready** · workstream 9 ("Parser/scanner/binder/lowering residuals"): collapsed the five `TypeLowering::{new, with_resolver, with_resolvers, with_def_id_resolver, with_hybrid_resolver}` public constructors in `crates/tsz-lowering/src/lower/core.rs` onto a single private `from_resolvers(arena, interner, LoweringResolvers)` builder. Each constructor previously re-spelled the same 17 fields (`type_param_scopes: Rc::new(RefCell::new(Vec::new()))`, `operations: Rc::new(RefCell::new(0))`, `limit_exceeded: Rc::new(RefCell::new(false))`, all the `None`/`false` defaults, and `interner.as_type_database()`) — only the four resolver fields differed across them. The five `pub fn` entry points are preserved as thin wrappers, so all 20+ downstream call sites in `tsz-checker` and `tsz-core` compile unchanged. 5 new `constructor_parity_tests` in `core.rs` lock the invariant defaults across every constructor so a future drift in one default fails CI. 119 lowering tests pass (114 existing + 5 new); 3083 affected-crate pre-commit tests pass; clippy/fmt clean. PR #1251.
- **2026-04-26** · branch `chore/conformance-extract-process-rss` · **DRY ready** · DRY cleanup in `crates/conformance/src/`: extracted the duplicated `get_process_rss(pid: u32) -> Option<usize>` helper (byte-identical Linux + macOS impl previously in `batch_pool.rs` and `server_pool.rs`) into a new `crates/conformance/src/process_rss.rs` module. Both pools now import via `use crate::process_rss::get_process_rss;`. The existing `get_process_rss_reports_current_process_memory_usage` test moved into the new module; added a Linux-only `get_process_rss_returns_none_for_nonexistent_pid` lock for the "/proc/{pid}/statm missing -> None" path. Pure DRY refactor; -64 LOC duplication, +66 LOC consolidated module + tests. `cargo check`, `cargo clippy --package tsz-conformance --all-targets -- -D warnings`, and the new + existing pool tests pass. PR #1252.
- **2026-04-26** · branch `chore/checker-tests-migrate-type-arg-count-mismatch` · **DRY workstream-1 ready** · workstream 1 ("Test harness consolidation"): migrated `crates/tsz-checker/tests/type_arg_count_mismatch_tests.rs` — replaced two local helpers (`diagnostic_codes`/7 call sites, `check_diagnostics`/5 call sites) duplicating the canonical `ParserState::new` → `BinderState::new` → `CheckerState::new` boilerplate with `tsz_checker::test_utils::{check_source_codes, check_source_diagnostics}`. Net -45 LOC; dropped 5 unused boilerplate imports. Distinct from PR #1253 (5 other files) and the still-WIP `claude/modest-archimedes-QBaXl` claim. 14 tests pass; pre-commit (20339-test workspace nextest) passed. PR #1267.
- **2026-04-26** · branch `chore/checker-tests-test-utils-migration` · **DRY workstream-1 ready** · workstream 1 ("Test harness consolidation: Checker tests still repeat ParserState::new, BinderState::new, and CheckerState::new heavily; continue migrating local wrappers to crate-local fixtures and tsz_checker::test_utils"): migrated 5 checker test files (`binding_pattern_inference_tests.rs`, `lazy_class_constructor_type_tests.rs`, `namespace_qualified_diagnostic_tests.rs`, `type_alias_namespace_merge_tests.rs`, `reverse_mapped_inference_tests.rs`) to use `tsz_checker::test_utils::{check_source, check_source_diagnostics, check_source_codes, check_source_code_messages}`. Each file previously defined a local 12-25-line check helper duplicating the canonical 3-line `ParserState::new` -> `BinderState::new` -> `CheckerState::new` setup. Pure refactor; +10 / -145 LOC. All 5,209 tsz-checker tests pass before and after. Distinct from older 2026-04-25 claim on `claude/modest-archimedes-QBaXl` covering `class_member_closure_tests.rs`, `contextual_tuple_tests.rs`, `contextual_typing_tests.rs`, `never_returning_narrowing_tests.rs`. PR #1253.
- **2026-04-26** · branch `chore/solver-tests-judge-setup-helper` · **DRY workstream-8 ready** · Solver tests DRY: added `JudgeSetup` fixture in `crates/tsz-solver/tests/common/mod.rs` that owns a fresh `(TypeInterner, TypeEnvironment)` and exposes `judge()` / `judge_with_config(JudgeConfig)` accessors handing out a borrowing `DefaultJudge`. Migrated all 68 setup sites in `crates/tsz-solver/tests/judge_tests.rs` (65 simple 3-line "interner / env / DefaultJudge::with_defaults" stanzas, 3 strict-mode 5-line "interner / env / config / DefaultJudge::new" stanzas, and 1 `mut env` site that calls `insert_def` before judge construction). Pure refactor; 5,323 `tsz-solver` nextest tests pass (69 `relations::judge::tests::*` tests preserved by name). PR #1254.
- **2026-04-26** · branch `chore/emitter-import-usage-tests` · **DRY workstream-8 ready** · workstream 8 item 4: locked the text-based import-usage heuristics in `crates/tsz-emitter/src/import_usage.rs` (528 LOC, was 0 unit tests) with 36 colocated unit tests in `crates/tsz-emitter/tests/import_usage.rs` mounted via the existing `#[cfg(test)] #[path = "../tests/import_usage.rs"] mod tests;` pattern (matches `safe_slice.rs`). Tests cover: `contains_identifier_occurrence` word boundaries (standalone vs substring vs member access; empty needle/haystack); `strip_type_only_content` for `import type` / `export type` / `declare` / `interface` / `type` block stripping, multi-line type body brace tracking, namespace alias preservation (`import X = Y;`), `export import` preservation, `export *` and `export { a } from` stripping vs local re-export preservation, inline var/param/return-type annotations, `as` / `satisfies` assertions, `implements` clause, generic call type args, object literal `key: value` preservation, string literal preservation, line comments, and bare declarations; `strip_type_declaration_lines` for type-only line dropping while keeping inline annotations and namespace aliases. Pure additive; 36 of 36 new tests pass; `cargo clippy -p tsz-emitter --all-targets` clean. PR #1257.
- **2026-04-26** · branch `chore/common-diagnostics-helper-tests` · **DRY workstream-8 ready** · Locked untested helpers in `crates/tsz-common/src/diagnostics/mod.rs` with 21 colocated unit tests: `is_parser_grammar_diagnostic` (TS1000-1999 half-open range, including bound + disjointness from JS-grammar / semantic codes), `is_js_grammar_diagnostic` (TS8000-8999 same matrix), `Diagnostic::error` simple constructor (field init + `impl Into<String>` for both `&str` and `String`), and `format_message` template-literal placeholder `${ ... }` normalization (pass-through, outer whitespace stripping, internal whitespace preservation, nested-brace depth tracking, multi-placeholder, multi-arg independence, unterminated graceful consume, empty `${}`, bare `$` literal). Pure additive; 6 → 27 tests in module, 322 → 343 total in crate. Net +194 LOC. PR #1264.
- **2026-04-26** · branch `chore/conformance-options-convert-edge-tests` · **Workstream-1 / harness-coverage ready** · Lock untested behavior of `crates/conformance/src/options_convert.rs` with 11 new edge-case unit tests: (a) `noLib: true` blocks target-driven default-lib injection; (b) `noLib: false` also blocks injection (key-presence check, not truthiness — locked explicitly so a future tightening to `is_truthy(noLib)` is a deliberate change); (c) comma-separated `target` selects libs from the first token only (the `target` field keeps the original string); (d) unrecognized `target` (e.g. `"foobar"`) skips lib injection entirely; (e) bool `"false"` directive value maps to `Value::Bool(false)` (symmetric of the existing `"true"` coverage); (f) mixed-case directive keys (`StrictNullChecks`, `NOIMPLICITANY`, `EsModuleInterop`) are normalized via `to_lowercase` before field lookup; (g) `nofallthrough` alias maps to `noFallthroughCasesInSwitch`; (h) empty-directives map produces empty options; (i) `lib` directive lowercases and trims each token (`"ES5, DOM "` → `["es5", "dom"]`); (j) `has_unsupported_server_options` is case-insensitive (`JSX`, `Paths`, `MODULERESOLUTION` all trigger CLI fallback); (k) `esnext` and `latest` share the same default-lib chain. 21 tests in the module pass (10 existing + 11 new); clippy and fmt clean. Pure test additions; no production change. PR #1259.
- **2026-04-26** · branch `chore/checker-tests-migrate-spread-rest` · **DRY workstream-1 ready** · workstream 1 ("Test harness consolidation"): migrated `crates/tsz-checker/tests/spread_rest_tests.rs` — replaced local 19-line `check_source(source: &str) -> Vec<Diagnostic>` wrapper with `tsz_checker::test_utils::check_source_diagnostics` across 61 call sites. Net -25 LOC; dropped 5 unused boilerplate imports (`tsz_binder::BinderState`, `tsz_checker::state::CheckerState`, `tsz_parser::parser::ParserState`, `tsz_solver::TypeInterner`, `tsz_checker::diagnostics::Diagnostic`). Distinct from PR #1253 (5 other files), PR #1267 (`type_arg_count_mismatch_tests.rs`), and the still-WIP `claude/modest-archimedes-QBaXl` claim. 63/63 spread_rest_tests pass; pre-commit (20339-test workspace nextest) passed. PR #1270.
- **2026-04-26** · branch `chore/checker-tests-migrate-js-grammar-span` · **DRY workstream-1 ready** · workstream 1 ("Test harness consolidation"): migrated `crates/tsz-checker/tests/js_grammar_span_tests.rs` — replaced local 24-line byte-identical `check_source(source, file_name, options)` wrapper with `tsz_checker::test_utils::check_source` import. 4 call sites unchanged; net -29 LOC, 1 import line. Distinct from PR #1253 (5 files), PR #1267 (`type_arg_count_mismatch_tests.rs`), PR #1270 (`spread_rest_tests.rs`), and the still-WIP `claude/modest-archimedes-QBaXl` claim. 4/4 js_grammar_span_tests pass; pre-commit (20339-test workspace nextest) passed. PR #1273.
- **2026-04-26** · branch `chore/emitter-comments-core-tests` · **Test coverage ready** · Added 37 unit tests for `crates/tsz-emitter/src/emitter/comments/core.rs` (231 LOC, previously 0 inline tests). Test file at `crates/tsz-emitter/tests/comments_core.rs`, mounted via the colocated `#[cfg(test)] #[path = "../../../tests/comments_core.rs"] mod tests;` pattern (matches `safe_slice.rs` and `import_usage.rs`). Coverage: empty input, position out-of-bounds, position at end-of-text, single/multi-line comments, trailing semantics (stops at first newline; multi-line comment with internal newline emits with `has_trailing_newline=true` and bails; consecutive `/*a*/ /*b*/` both emitted; isolated `/` not a comment), leading semantics (groups across newlines; final no-newline comment has `has_trailing_newline=false`; CRLF normalized; shebang `#!` skipped only at `pos==0`), UTF-8 safety (3-byte BMP + 4-byte surrogate-pair inside comment bodies), empty `//` and `/**/`, unterminated `/* ...` to EOF (locks current `end = len-1` behavior), CommentRange position anchors at the comment-marker `/`, and CommentKind Copy + Eq contract. Pure additive — no production code change. 1442 emitter tests pass (1405 existing + 37 new); 4436 affected-crate pre-commit tests pass; clippy and fmt clean. PR #1260.
- **2026-04-26** · branch `chore/snapshot-rollback-count-guard` · **Workstream-4 / architecture-health-metric-5 ready** · Operating Principle 8 + Architecture Health Metric 5 ("Broad cache-clear and snapshot-restore call sites"): pinned the **snapshot-rollback** half of metric 5 (the cache-clear half is bounded today; pin separately if it grows). Added `SNAPSHOT_ROLLBACK_FILE_COUNT_CHECKS` + `scan_snapshot_rollback_file_count` in `scripts/arch/arch_guard.py` mirroring metric 3 (PR #1237). Counts non-test files under `crates/tsz-checker/src/` outside `context/speculation.rs` that contain a non-comment line matching `CheckerContext::rollback_full` / `rollback_diagnostics(_filtered)?` / `rollback_and_replace_diagnostics` / `rollback_return_type` / `rollback_filtered`, the `restore_ts2454_state` / `restore_implicit_any_closures` snapshot restorers, or `*guard.rollback(` SpeculationGuard calls (the `\w*guard\.` lookback filters unrelated `.rollback(` methods on `Transaction`/`Database` receivers). Live count: 15 files. Cap pinned at 15. 11 new `ArchGuardSnapshotRollbackTests` unit tests in `scripts/arch/test_arch_guard.py` lock each rollback-API category, the speculation-guard pattern, unrelated-`.rollback(` rejection, comment-only line skipping, test file/dir exclusion, single-file-counts-once, exact cap match, and live-cap-not-off-by-one. PR #1246.
- **2026-04-26** · branch `chore/checker-tests-migrate-environment-capabilities` · **DRY workstream-1 ready** · workstream 1 ("Test harness consolidation"): migrated `crates/tsz-checker/tests/environment_capabilities_tests.rs` — replaced local 22-line `check_with_options(source, options) -> Vec<Diagnostic>` and 3-line `check_no_lib(source)` helpers with thin wrappers routing to `tsz_checker::test_utils::{check_source, check_source_diagnostics}`. 16 call sites kept their names and signatures intact (wrapper-preserving migration). Net -17 LOC; dropped 4 unused boilerplate imports. Behavior parity: `crates/tsz-checker/src/context/constructors.rs:259` initializes `lib_contexts: Arc::new(Vec::new())` and `lib_binders_cached: Arc::new(Vec::new())`, so the test_utils wrapper's extra `set_lib_contexts(Vec::new())` call is a no-op for fresh checkers. Distinct from PR #1267, PR #1270, PR #1273, and the still-WIP `claude/modest-archimedes-QBaXl` claim. 47/47 environment_capabilities_tests pass; pre-commit (workspace nextest) passed. PR #1277.
- **2026-04-26** · branch `chore/emitter-transforms-helpers-tests` · **DRY workstream-1 ready** · workstream 1 ("Test harness consolidation: continue locking pure helpers in tsz-emitter behind unit tests"): added 23 inline unit tests at the bottom of `crates/tsz-emitter/src/transforms/helpers.rs` (710 LOC, previously 0 direct tests) covering `HelpersNeeded::default().any_needed() == false`, per-flag `any_needed()` flip (guards against forgetting to OR a new field), `class_private_field_set_before_get`-alone-must-not-flip, `needed_names()` canonical priority order (full-set lock + spot checks), `emit_helpers()` priority sort across all 7 priority bands plus the unprioritized tail, intra-priority order (extends-before-makeTemplateObject; assign-before-createBinding; decorate-before-runInit-before-esDecorate-before-setFunctionName-before-propKey-before-importStar-before-rewrite-before-exportStar), `class_private_field_set_before_get=true` flips Set/Get ordering, `import_star` emits both `__setModuleDefault` and `__importStar`, every helper constant non-empty and starts with `var __<name> ` matching the `needed_names` entry, no duplicate emission, newline termination, `Clone` round-trip. 23 passing tests; full 1428-test tsz-emitter lib suite still passes. Pure additive — no production change. Used `TSZ_SKIP_LINT_PARITY=1` once to bypass a pre-existing `clippy::doc_markdown` error in `tsz-solver/tests/def_tests.rs` (PR #1254 in queue). PR #1261.

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

Status snapshot 2026-04-25 (large-ts-repo, 6086 files, 39MB):

1. Pre-#1202 baseline: peak RSS ~67 GB virtual, exit 137 (SIGKILL by macOS jetsam) at ~75s. Bench reports as TIMEOUT.
2. Post-#1202 (`perf(binder,checker,cli,core): Arc-share per-file semantic_defs`): peak RSS dropped to ~10 GB resident, exit 137 still hits at ~47s. **6.7× memory reduction**, but system memory ceiling still exceeded on this 32 GB host.
3. Post-Arc-share-`node_symbols` (#1227): peak RSS dropped further to **~6.2 GB** resident, exit 137 still hits at ~45s. Additional **~38% reduction** on top of #1202. Cumulative: 67 GB virtual → 6.2 GB resident (~10× from baseline).
4. Post-Arc-share-`node_flow` (this PR): peak RSS measured at ~7.0 GB resident, still exit 137 at ~50s. Same-machine before/after (sampled at 2s intervals on a busy 32 GB host) showed run-to-run variation in the noise band (~6.9-7.5 GB), so the direct measurable savings are within noise. The change is structurally correct (same template as #1202/#1227, no `Arc::make_mut` post-binding hot path) and unblocks the remaining template migrations.
5. Implication: continue Arc-sharing the remaining per-binder maps (`node_scope_ids`, `top_level_flow`, `switch_clause_to_switch`) to push further toward stable completion. The next likely high-leverage target is `node_scope_ids` (parallels `node_symbols` shape).
6. Bench harness caveat: `tsz: TIMEOUT` in the table can mean either timer-kill (124) or OS-kill (137). The 137 case ("OOM-by-paging") is the dominant failure mode here. Inspect exit codes when investigating.

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
