# TSZ Robustness Audit — 2026-04-26

This document catalogs **silent-repair / patch-over-it** anti-patterns identified
by a keyword-based source review. The worst offenders are not the code paths
explicitly labeled `hack`; they are the places where the code silently repairs
missing state, bypasses known-broken APIs, returns `any`, or tells clients
"success" with empty results.

Each entry below identifies precise file/line anchors, the failure mode, and a
suggested **small, behavior-preserving** PR that addresses it. PRs land in the
priority order documented under [Sequencing](#sequencing).

This is a durable architecture/quality artifact, not roadmap status; per
`docs/plan/ROADMAP.md` rules, individual landing claims for each PR live in
`Active Implementation Claims` of the living roadmap and link back here.

## Background

Keyword smoke test across non-test Rust sources:

```text
fallback        ~1728 hits
suppress        ~1536 hits
legacy           ~478 hits
try_borrow_mut     57 hits
temporary          66 hits
hardcoded          18 hits
stub                8 hits
workaround          4 hits
hack                2 hits
```

These counts alone are not bad — TypeScript compatibility legitimately requires
substantial recovery logic. The audit triages which hits represent real
robustness debt vs. justified compatibility behavior, and proposes per-item
fixes that preserve TypeScript-parity semantics while removing fragile silent
recovery paths.

## Audit findings

Items are ordered by **risk** (1 = most dangerous). Each entry has a tracker
status emoji at the start of the title:

- ⏳ pending (no PR yet)
- 🚧 in flight (PR open, not yet merged)
- ✅ landed
- ❌ abandoned

### 1. ✅ Silent `try_borrow_mut` failures during semantic registration

**Files**

- `crates/tsz-checker/src/context/def_mapping.rs:451-459, 463-474, 493-500, 510-516, 550-565`
- `crates/tsz-checker/src/state/state_checking/source_file.rs:87-94`

**Failure mode** Many "register in both environments" helpers do:

```rust
if let Ok(mut env) = self.type_env.try_borrow_mut() {
    env.insert_def(def_id, body);
}
if let Ok(mut env) = self.type_environment.try_borrow_mut() {
    env.insert_def(def_id, body);
}
```

No failure path, no deferred write, no invariant failure, often no logging. If
the borrow fails, a required semantic registration silently disappears. The
comments at `def_mapping.rs:431-433` confirm this is not theoretical: missing
entries happen when `insert_def_kind` fails and `TypeEnvironment::get_def_kind`
falls back to `DefinitionStore`. Worse, `source_file.rs:87-94` clones one
mutable environment over another to repair missed writes.

**Fix plan**

- **PR #A** (this audit, item 1a): replace the silent
  `if let Ok(mut env) = ...try_borrow_mut()` pattern in `def_mapping.rs` with
  a `must_borrow_mut(name)` helper that panics in debug + emits a structured
  error metric in release. Keep behavior identical when the borrow succeeds.
  This makes the failure visible without changing the architectural shape.
- **PR #A** (item 1b, follow-up): redesign the dual-environment write path so
  registration is transactional: queue writes when borrowing isn't possible,
  apply at scope boundaries. Delete the `source_file.rs:87-94` env-snapshot
  clone-over.
- Add a `tsz-checker` invariant test that asserts the two environments converge
  after a representative file's check completes.

**Verification** Targeted nextest in `tsz-checker`; smoke-conformance run on
files that exercise recursive `Lazy(DefId)` resolution.

---

### 2. ⏳ Checker bypasses a known binder bug

**Files**

- `crates/tsz-checker/src/symbols/symbol_resolver.rs:458-462, 728-732`

**Failure mode** Comments explicitly say "the binder's method has a bug" and
the checker reaches into `lib_contexts.file_locals` directly to bypass it.
This creates two symbol-resolution truths (binder vs. checker fallback);
shadowing, merged symbols, and DefId mapping can diverge.

**Fix plan**

- **PR #B**: fix the binder's lib-lookup behavior in
  `crates/tsz-binder/...`. Add binder-level regression tests that lock the
  fixed semantic.
- Remove the checker-side `lib_contexts.file_locals` bypass and rerun
  conformance to confirm no regressions.

**Verification** Binder unit tests + targeted conformance on tests that today
depend on the bypass.

---

### 3. ⏳ Dual type environments synced by patches

**Files**

- `crates/tsz-checker/src/context/def_mapping.rs:871-917` (`register_resolved_type`)

**Failure mode** Both `type_env` and `type_environment` are mutable
representations of the same semantic facts. Many call sites must remember
which to write to, and helper functions still recover via silent
`try_borrow_mut`. Comments admit prior versions wrote to only one environment,
breaking `resolve_lazy(DefId)`.

**Fix plan**

- **PR #C** (after PR #A): collapse the two environments into one
  authoritative semantic environment with read-only snapshots for flow
  analysis. If two views must persist, synchronize via a versioned snapshot
  with explicit authority instead of dual writes.

**Verification** Single-environment must pass full unit tests + conformance.

---

### 4. ✅ "Largest `SymbolId` wins" for lib symbol canonicalization

**Files**

- `crates/tsz-checker/src/context/def_mapping.rs:299-346` (`canonical_lib_sym_id`)

**Failure mode** Heuristic that depends on allocator behavior:

```rust
if let Some(sym_id) = binder.file_locals.get(name) && sym_id.0 > best.0 {
    best = sym_id;
}
```

If allocation order changes or an unrelated symbol gets a larger ID, the
wrong identity is chosen. Especially dangerous for the `DefId` migration.

**Fix plan**

- **PR #D**: replace heuristic canonicalization with an explicit stable key
  `(name, declaring file, stable location, merged-binder provenance)` or ask
  `DefinitionStore` / binder for the canonical identity directly. Add tests
  that lock the canonicalization rule.

**Verification** Targeted lib-merge tests + conformance smoke on lib-heavy
tests (`enumAssignmentCompat`, `arrayToLocaleString*`, etc.).

---

### 5. ✅ Speculation guard says rollback-on-drop, actually implicit-commits

**Files**

- `crates/tsz-checker/src/context/speculation.rs:1-11, 352-360, 418-422`

**Failure mode** Module preamble promises "rollback-on-drop"; the type's
docstring says the same; the implementation has no `Drop` impl and "dropping
without an explicit call means keep the speculative diagnostics (implicit
commit)." A future caller can reasonably assume RAII rollback and silently
keep speculative diagnostics. (This was partially addressed by `PR #1213`'s
`DiagnosticSpeculationGuard → DiagnosticSpeculationSnapshot` rename;
remaining inconsistencies in surrounding docs and any remaining "guard"
language need cleanup.)

**Fix plan**

- **PR #E**: align all surrounding doc-comments, README/architecture refs,
  and call-site language to the actual snapshot semantics. Or, if the team
  wants true RAII, build a real transaction API: `with_diagnostic_speculation(|ctx| -> SpeculationOutcome)`.
  Pick one and remove the contradictory language.

**Verification** Speculation rollback tests; doc-style review.

---

### 6. ⏳ LSP signature help is a source-text mini-parser

**Files**

- `crates/tsz-lsp/src/signature_help.rs:199-224, 1320-1345, 1719-1748, 1902-2054`

**Failure mode** Backward byte-scanning for `(` / `<`, manual brace/bracket
counting, regex-style interface scanning via `format!("interface {name}")`.
Wrong for comments, strings, overloaded members, computed names, Unicode
identifiers, declared interfaces, namespace-qualified names, etc.

**Fix plan**

- **PR #F**: extract a small `IncompleteCallContext` / `IncompleteCodeQuery`
  service (probably in `tsz-parser` or a new crate) that takes
  `(source, position)` and returns a structured trigger. Migrate
  `signature_help.rs` to call it. Existing fragile text scanning becomes the
  service implementation under one roof, easier to test and replace.

**Verification** Existing fourslash signature-help tests + new unit tests for
the service.

---

### 7. ⏳ LSP completions fall back to source slicing for type detail

**Files**

- `crates/tsz-lsp/src/completions/member.rs:716-720, 1574-1640`

**Failure mode** Member completions slice raw source for method signatures
when checker resolution fails. UI display becomes partly semantic, partly
text-guessed. Long-term: every TS syntax feature must be re-discovered by
text slicing.

**Fix plan**

- **PR #G**: expose a semantic `display_signature_for_declaration_node` API
  from compiler services. Migrate `completions/member.rs` to call it. Keep a
  centralized token-based fallback only for incomplete code, behind a tiny
  service.

**Verification** Fourslash completion tests + targeted unit tests for the
service.

---

### 8. ⏳ Tsserver handlers return success for stubs / empty bodies

**Files**

- `crates/tsz-cli/src/bin/tsz_server/main.rs:977-983, 1005-1020`
- `crates/tsz-cli/src/bin/tsz_server/handlers_diagnostics.rs:445-452`
- `crates/tsz-cli/src/bin/tsz_server/handlers_code_fixes.rs:1094, 3678`

**Failure mode** Unsupported commands return `success: true` with empty
bodies. Clients can't distinguish "feature produced no result" from "feature
not implemented" from "feature failed internally."

**Fix plan**

- **PR #H**: return `success: false` with a capability/error reason for
  unimplemented commands, OR advertise capabilities accurately so clients
  don't dispatch to stubs.

**Verification** Server handler unit tests; existing tsserver integration
tests; protocol-conformance check if any.

---

### 9. ⏳ Diagnostic suppression is scattered through mutable checker flags

**Files**

- `crates/tsz-checker/src/context/mod.rs:627-661, 791-833`

**Failure mode** Dense cluster of ambient booleans (`has_parse_errors`,
`has_syntax_parse_errors`, `is_in_ambient_declaration_file`,
`skip_flow_narrowing`, `skip_callable_type_param_suppression`, …) that change
checker behavior globally. Risk: state leakage from a missing restore, a
nested call, or a speculative path.

**Fix plan**

- **PR #I**: introduce a `DiagnosticPolicy` / `ParseHealth` value passed
  through diagnostic emission, instead of context-global booleans. Migrate
  one suppression at a time (the parse-error cluster first, since it's
  semantically a single concept).

**Verification** Conformance net-zero per migrated suppression; targeted
unit tests for the new `DiagnosticPolicy` API.

---

### 10. ⏳ Frequent `TypeId::ANY` fallbacks hide missing semantics

**Files**

- `crates/tsz-checker/src/dispatch.rs:374-376, 557-558, 1876-1878`
- `crates/tsz-checker/src/dispatch_yield.rs:184-185, 639-641`

**Failure mode** Returning `any` from "safe fallback" sites masks
incompleteness — once a type becomes `any`, downstream errors disappear.
Distinguishable cases (parse-recovery error, unresolved-semantic error, real
TS `any`) collapse to one type.

**Fix plan**

- **PR #J**: introduce sentinel-but-not-`any` types for the parse-recovery
  and unresolved-semantic cases. They should suppress cascades narrowly but
  not pass assignability everywhere. Migrate the dispatch sites one cluster
  at a time.

**Verification** Conformance net-zero per migration; targeted regression
tests on the specific fallback paths.

---

### 11. ⏳ Flow analysis has its own "fallback type from syntax" subsystem

**Files**

- `crates/tsz-checker/src/flow/control_flow/assignment_fallback.rs` (entire
  module)

**Failure mode** A partial second type resolver. If the main checker and
fallback resolver disagree, narrowing becomes inconsistent.

**Fix plan**

- **PR #K**: route fallback through the same semantic service as the main
  checker. If fallback is needed for cycle breaking, mark results approximate
  and avoid committing them as normal type facts.

**Verification** Flow-narrowing tests + conformance smoke on
control-flow-heavy tests.

---

### 12. ✅ Parser recovery placeholders are indistinguishable from real empty identifiers

**Files**

- `crates/tsz-parser/src/parser/state.rs:875-1008, 2140-2171, 2204-2207, 2687-2703`

**Failure mode** `create_missing_expression` builds an `Identifier` with
`Atom::NONE` and empty `escaped_text`. Later phases can't distinguish a
recovery placeholder from a real empty identifier without checking side
channels.

**Fix plan**

- **PR #L**: give synthetic recovery nodes an explicit syntax kind (e.g.
  `MissingExpression`) or a flag bit, then have downstream consumers check
  for it.

**Verification** Parser unit tests + checker tests that depend on the
distinction.

---

### 13. ✅ Identifier/interner fallback leaks into LSP/navigation

**Files**

- `crates/tsz-lsp/src/navigation/implementation.rs:41-45`
- `crates/tsz-lsp/src/rename/core.rs:299-307`

**Failure mode** LSP code bypasses `get_identifier_text` because the
interner transfer happens through `into_arena()` but not `get_arena()`.
Comments explicitly explain the workaround. Symptomatic of inconsistent
identifier-text ownership at the arena API. (`PR #1205` partially addressed
the underlying coherence for incremental parse but the LSP-side bypasses
remain.)

**Fix plan**

- **PR #M**: make identifier text resolution total at the `NodeArena` API
  (always either resolve via interner OR fall back to `escaped_text`) so LSP
  callers don't need to bypass it. Delete the bypass workarounds.

**Verification** LSP rename and implementation tests + targeted unit tests
on `NodeArena::resolve_identifier_text` for edge cases.

---

### 14. ⏳ Module resolution has parallel legacy and new paths

**Files**

- `crates/tsz-checker/src/module_resolution.rs:380-382, 424-427, 538-551, 578-583`

**Failure mode** Newer resolver and legacy map path coexist. Different
frontends may resolve modules differently. Compatibility shim
(`module_specifier_candidates`) blurs raw vs. extension-stripped semantics.

**Fix plan**

- **PR #N**: collapse to one resolver entrypoint, one normalized key model,
  one cache. Keep compatibility behavior only at the boundary where
  TypeScript semantics require it.

**Verification** Conformance Node lane + import-resolution tests.

---

### 15. ✅ Hardcoded apparent members for primitives/Function

**Files**

- `crates/tsz-solver/src/operations/property_helpers.rs:958-977, 1273-1281`

**Failure mode** Hardcoded `apply`, `call`, `bind`, `toString`, `name`,
`length`, `prototype`, `arguments`, `caller` on `Function` types. May be
necessary for no-lib bootstrap, but should be tightly gated. Lib augmentations
or target-specific lib differences could be masked.

**Fix plan**

- **PR #O**: gate hardcoded apparent members behind explicit
  no-lib/bootstrap mode. Track usage via a counter so drift is visible.

**Verification** Conformance no-lib tests + lib-augmentation tests.

---

### 16. ✅ Public editor actions advertise unimplemented behavior

**Files**

- `crates/tsz-lsp/src/code_actions/code_action_editor_features.rs:125-134`

**Failure mode** "Add Missing Imports" is unconditionally advertised with
`edit: None` — UI shows an action that does nothing.

**Fix plan**

- **PR #P** (smallest possible PR): only advertise the action when import
  candidates exist, or mark it disabled with an explicit reason if the
  protocol/client supports that.

**Verification** Code-action LSP tests.

---

### 17. ⏳ Crate-wide lint suppression masks cleanup debt

**Files**

- `crates/tsz-checker/src/lib.rs:16-26`

**Failure mode** Crate-wide `#![allow(dead_code)]` lets old migration paths
stay alive indefinitely. Other allows (`redundant_clone`, `unnecessary_map_or`)
hide perf and stylistic debt.

**Fix plan**

- **PR #Q**: remove broad `#![allow(...)]`. Re-add targeted `#[allow(...)]`
  on the specific items that justify it. Likely produces ~20-100 spot fixes
  to be split across follow-up small PRs (one cluster per `#[allow]` removal).

**Verification** `cargo check -p tsz-checker --tests` + clippy clean.

---

## The "lazy patch" top 5

If only five items are landed, fix these in this order:

1. **Item 2** (#B): Checker directly bypasses a known binder bug
   (`crates/tsz-checker/src/symbols/symbol_resolver.rs:458-462, 728-732`)
2. **Item 1** (#A): Semantic registrations silently disappear on RefCell
   borrow failure (`crates/tsz-checker/src/context/def_mapping.rs:451-565`)
3. **Item 3** (#C): One environment is cloned over another to repair missed
   writes (`crates/tsz-checker/src/state/state_checking/source_file.rs:87-94`)
4. **Item 4** (#D): Lib symbol canonicalization chooses the largest numeric
   `SymbolId` (`crates/tsz-checker/src/context/def_mapping.rs:299-346`)
5. **Item 5** (#E): Speculation guard documentation says rollback-on-drop,
   implementation says implicit commit
   (`crates/tsz-checker/src/context/speculation.rs:1-11, 352-360, 418-422`)

These five are the most dangerous. Style, lint cleanup, and the
broader-scope items (#K, #N, #Q) come after.

## Sequencing

```text
Wave 1 (correctness, urgent):
  #B  ->  #A  ->  #C  ->  #D  ->  #E

Wave 2 (architectural cleanups behind #A/#C):
  #I  ->  #J  ->  #K  ->  #L  ->  #M

Wave 3 (UX honesty):
  #H  ->  #P

Wave 4 (LSP semantic surface):
  #F  ->  #G

Wave 5 (longer-running cleanups):
  #N  ->  #O  ->  #Q
```

Each PR is small, behavior-preserving (except where explicitly fixing a
behavior bug), and ships with a regression test that locks the new
invariant.

## How to use this document

1. When you start an item, claim it in `docs/plan/ROADMAP.md` ->
   `Active Implementation Claims` with a timestamped entry that links here.
2. Open a draft `WIP` PR per CLAUDE.md.
3. Implement the smallest PR that addresses the item without scope-creep.
4. Update the entry's status emoji from ⏳ → 🚧 → ✅ on the corresponding
   PR ready/merge.
5. If a fix invalidates the audit's hypothesis, update this document with
   the corrected analysis instead of leaving stale notes.

## Landing log

- ✅ #5 (#E) speculation guard doc coherence — PR #1364 (merged 2026-04-26)
- ✅ #4 (#D) canonical_lib_sym_id heuristic instrumentation — PR #1371
  (merged 2026-04-26). Behavior preserved; structured trace event surfaces the
  rare case where the heuristic chooses a non-input SymbolId. Full
  redesign with stable key tracked under follow-up.
- ✅ #16 (#P) "Add Missing Imports" stub no longer advertised — PR #1375
  (merged 2026-04-26).
- ✅ #1 (#A) silent `try_borrow_mut` failures — tracing variant merged via
  PR #1369 (2026-04-26). Redesign to transactional dual-env writes is a
  follow-up tracked separately.
- ✅ #13 (#M) LSP identifier text bypass — PR #1378 (merged 2026-04-26)
  delegates to a total `resolve_identifier_text` on `NodeArena`; downstream
  LSP rename / implementation no longer reach into raw arena state.
- ✅ #15 (#O) hardcoded `Function` apparent-member fallback —
  PR #1507 (merged 2026-04-26) adds a `tracing::trace!` event when the
  no-lib hardcoded ladder fires (`apply`/`call`/`bind`/`toString`/`name`/
  `length`/`prototype`/`arguments`/`caller`). Behaviour preserved; surfaces
  drift when the boxed `Function` interface fails to provide the property.
- ✅ #12 (#L) parser recovery placeholders distinguishable from real empty
  identifiers — already addressed by `NodeArena::is_missing_recovery_identifier`
  (see `crates/tsz-parser/src/parser/node_access.rs:169`). Codifies the
  `escaped_text.is_empty() && atom == Atom::NONE` invariant into a stable
  API. Inline call-site migration is incremental and tracked separately.
