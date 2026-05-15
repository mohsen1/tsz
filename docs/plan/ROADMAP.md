# TSZ Roadmap

Date: 2026-05-14

Status: single living roadmap. Keep durable architecture contracts in
`docs/architecture/`, behavior specs in `docs/specs/`, product docs in
`docs/site/`, and current execution strategy here. Do not use this file for
routine PR status. Update it only for durable changes to public metrics, release
gates, sequencing, architecture direction, active priorities, or assumptions
future work would otherwise inherit incorrectly.

## North Star

tsz must become a real-project-compatible TypeScript compiler:

> Same project result as `tsc`, substantially faster when it succeeds, with
> clear failure categorization when it does not.

The immediate project risk is no longer raw feature count. tsz is weak under
advanced TypeScript composition: recursive conditionals, mapped/key-remapped
types, template literals, `infer`, indexed access, contextual generic
instantiation, flow narrowing, relation variance, and cross-file lib/module
identity. These are semantic-substrate problems, so the roadmap is organized
as campaigns instead of isolated conformance picks.

## Current Public Metrics

Source: `README.md` on 2026-05-14.

| Surface | Current |
| --- | ---: |
| Diagnostic conformance | `100.0%` rounded (`12,581 / 12,582`) |
| JavaScript emit | `94.8%` (`12,820 / 13,530`) |
| Declaration emit | `91.7%` (`1,531 / 1,669`) |
| Fourslash / language service | `99.9%` (`6,558 / 6,562`) |

Conformance remains a hard regression gate. It is no longer the sole readiness
signal. The primary readiness signal for this phase is whether tsz can
successfully check real projects that `tsc` accepts. Rounded percentages are
communication aids only; release planning uses exact numerators, denominators,
and failure-family counts.

## Evidence From Current Audit

This section is intentionally short and current. Replace it when a fresher audit
changes the picture.

1. Recent `main` history is fix-heavy: the last 1000 commits sampled on
   2026-05-14 contained roughly `590` fixes, `210` chores, `100+`
   performance/benchmark commits, and many more checker-scoped changes than
   solver-scoped changes. That is a signal that parity work is still often
   landing as local repair instead of substrate consolidation.
2. Recent bug language is also diagnostic: `alias`, `display`, `recursive`,
   `suppress`, `skip`, and `guard` appear often in commit subjects. Some are
   correct fixes, but the pattern says relation policy, display policy, and
   recursion/fuel policy still need central ownership.
3. A removed `Comparable<number>` rewrite showed the failure mode clearly:
   hardcoded user names, source-text scans, printer-output decisions, and a
   synthesized type can pass one fingerprint while making the compiler less
   real. Treat that as the cautionary example for new work.
4. Current open PR state is noisy: many branches are draft or WIP, and some
   ready PRs still carry `WIP` labels. A ready PR with a `WIP` label is still
   not mergeable.
5. Emit is a concentrated architecture risk, not cleanup tail work. Public
   metrics show `710` JavaScript emit failures and `138` declaration emit
   failures in `scripts/emit/emit-detail.json`; active work is heavily skewed
   toward narrow emit/DTS fixes and helper splitting.
6. Declaration emit currently performs too much late semantic discovery:
   `TypeData` matching, direct type evaluation during printing, usage walks over
   inferred `TypeId`s, and text-based import usage heuristics. The target is a
   precomputed declaration/public-API summary, not a shadow checker.
7. Open non-tech-debt bugs are concentrated in semantic substrates, not random
   leaf features. A 2026-05-14 audit found `125` open bugs excluding
   `tech-debt`; the dominant families were relation/assignability/readonly
   (`~56`), type inference/contextual instantiation (`~43`), deferred type
   evaluation (`~48`), key-space/indexed access/property logic (`~28`),
   class/`this`/accessor compatibility (`~23`), flow narrowing (`~9`), and
   symbol/lib/module identity (`~13`). These are the tracks' root inputs.
8. The design response is **not** an architecture-first pause. Purpose-specific
   normalization, inference sessions, key-space algebra, diagnostic-capable
   relation results, solver-owned flow predicates, identity/provenance queries,
   and cache-key contracts should be introduced as just-in-time compatibility
   enablers inside the tracks below. Broad checker thinning, display rewrites,
   LSP/WASM expansion, and generalized query-engine refactors stay on the back
   burner until a release gate or bug family requires them.
9. Emit architecture is moving to a TS6+ direct-to-target lane: `ES2015` is the
   strategic floor, `ES3`/`ES5` and deprecated legacy module outputs are
   compatibility lanes, and already-emitted JS/DTS string rewrites are tracked
   as output-surgery debt by `scripts/emit/audit-output-surgery.py`.

## Coordination Model

GitHub is the coordination surface.

1. Pick a stable `AgentName` and include it in every PR body and substantive PR
   comment.
2. Check open draft PRs and recent merged PRs for overlap before starting.
3. A GitHub issue is optional. A draft PR with a clear title/body is enough to
   claim active work.
4. Open a draft PR early, even if it is initially empty or docs-only. Use the
   draft PR body to record scope, invariants, risks, and verification.
5. Do not create claim documents under `docs/plan/claims`; that system has been
   removed.
6. Long-running branches must periodically merge `main` and fix conflicts in
   their own PRs.
7. Agents coordinate through PR comments, review comments, and PR descriptions.
   Address other agents by `AgentName` when coordination matters.
8. Never merge work that is still draft, labeled `WIP`, titled with `[WIP]`, or
   described as not ready.
9. Treat `ready` plus a `WIP` label as WIP. Remove the label before merge.
10. When ready, remove `WIP` labeling/title text, update the PR body with final
   scope and verification, mark ready, and let heavy CI run.
11. If a track is abandoned, close the draft PR with the reason and any useful
    findings.

Draft PR body shape:

```markdown
## Agent
AgentName: <stable-name>

## Track
<benchmark blocker | semantic campaign | emit/dts | refactor>

## Invariant
When <structural condition>, `tsc` <does X>; this PR makes tsz do X through
<owning layer>.

## Scope
- <files/systems expected to change>

## Verification
- <targeted local commands or CI gates>

## Coordination Notes
- <overlap, dependencies, follow-ups>
```

## Work Intake Rules

Every non-trivial PR declares exactly one type:

1. **Benchmark blocker**: names the project and before/after failure class.
2. **Semantic campaign**: names the invariant and owning layer.
3. **Emit/DTS parity**: names the baseline family and confirms no checker
   regression.
4. **Refactor only**: proves behavior unchanged and names the future campaign it
   enables.

For checker/solver fixes, the PR body must include:

1. Structural rule, not one-test symptom.
2. Owning layer: solver/query boundary/checker orchestration.
3. Adjacent-case matrix when behavior changes.
4. Cache-enabled/cache-disabled or order-independence plan when the bug touches
   generic instantiation, aliases, globals, or relation/evaluation caches.
5. Project-corpus smoke plan when the subsystem affects Kysely, Zod,
   ts-toolbelt, type-fest, ts-essentials, or large repo.

For emit/DTS fixes, the PR body must include:

1. Failure family: JS transform family or DTS nameability/portability/JSDoc/type
   display family.
2. Output layer: direct AST print, lowering directive, IR plan, declaration
   summary, or parser recovery fact.
3. Why the fix does not add semantic validation or late semantic discovery in
   emitter code.
4. Baseline-style verification plan; fragment `contains` tests are smoke tests,
   not proof of parity.

Symptom-patch freeze:

1. No new diagnostic decisions from file names, source text snippets, rendered
   type strings, or single conformance test names.
2. Existing `rewrite_*_fingerprints` and source-text/display-string decisions
   are finite migration debt. New work should remove one, route around one
   through a structural query, or explicitly list the temporary shortcut in the
   PR body with owner and removal condition.
3. Query-boundary modules may expose domain classifiers. They should not become
   broad re-export barrels for checker-local semantic traversal.

## Phase 0: Stabilize The Runway

Near-term priority order:

1. Merge or close current active PRs into coherent campaign ownership; remove
   stale `WIP` labels before any ready merge.
2. Establish the project-corpus red/yellow/green dashboard and bug-family
   intake loop before judging speed. Every red/yellow row should name the
   semantic operation that owns the first blocker.
3. Fold substrate refactors into bug closure. A semantic bug may add a
   normalization query, inference-session boundary, key-space helper,
   `RelationDecision` path, flow predicate, identity query, or cache-key
   contract only when the reported family needs that substrate.
4. Freeze new symptom patches and start burning down existing fingerprint/source
   text/rendered-type rewrites.
5. Stop starting broad DTS cleanup unless it removes an emitter boundary
   violation, reduces ambient state, improves a release gate, or unblocks a
   named failure family.
6. Convert noisy planning state into draft PRs, PR comments, and this roadmap
   only when the update is durable enough to justify the shared-file conflict
   risk.
7. Add reduced benchmark failures to targeted tests as they are understood.
8. Keep broad display-provenance polish, generalized query-engine refactors,
   major incremental/perf rewrites, and LSP/WASM expansion on the back burner
   unless they unblock a named release gate.

## Phase 1: Project Corpus Gate

The benchmark dashboard must distinguish correctness from speed.

| Status | Meaning |
| --- | --- |
| Green | tsz and `tsc` both exit successfully with accepted diagnostic policy |
| Yellow | tsz exits but diagnostics differ |
| Red | tsz crashes, errors, OOMs, or times out |
| Gray | fixture or artifact is missing/incomplete |

Required project rows:

| Project | Current Strategic Read | Primary Owner Track | Exit Target |
| --- | --- | --- | --- |
| Kysely | contextual generics, guards, indexed/property access | Tracks 2, 3, 5, 6 | exit success |
| Zod | recursive conditionals, object guards, class/generic identity | Tracks 2, 3, 4, 7 | exit success |
| ts-toolbelt | recursive type evaluation pressure | Tracks 2, 3 | exit success |
| type-fest | broad mapped/conditional/key-space utility surface | Tracks 2, 3, 5 | exit success |
| ts-essentials | utility types plus recursive JSON shapes | Tracks 2, 3, 5 | exit success |
| large-ts-repo | residency/runtime/project graph stress | Tracks 1, 7, 10 | exit success without OOM/timeout |
| Next.js full project | module graph plus generated app dependencies | Tracks 1, 7, 9 | recorded green/yellow/red |

For every project row, capture:

1. exit code,
2. timeout/OOM/crash/diagnostic mismatch,
3. diagnostic status and first 20 diagnostic deltas grouped by subsystem,
4. JavaScript emit status when emit is in scope,
5. declaration emit status when DTS is in scope,
6. known checker/solver/emit/DTS blockers,
7. peak memory if measured,
8. number of files reached if available,
9. last successful phase: parse, bind, check, emit.

Speed is a secondary column until the row is green or explicitly out of scope.
Do not present a faster red project as a win without also naming the remaining
correctness blocker.

## Architecture Health Metrics

Track these as counters or periodic audit bullets. They are more useful than
subjective "cleanup" language.

1. `CheckerContext` field count, currently pinned at `234`, plus the number of
   checker `source_text.contains` / file-name / rendered-message
   diagnostic decisions.
2. Number of post-check `rewrite_*_fingerprints` passes still active.
3. Direct `is_assignable_to` call sites on `TS2322`/`TS2345`/`TS2416` paths
   that need both relation result and failure reason.
4. Checker modules consuming broad traversal primitives instead of
   domain-specific query-boundary classifiers.
5. Direct `TypeData` pattern matching outside solver/query-boundary internals.
6. Actual-lib alias admissions and allowlists that should become stable lib
   identity queries.
7. Emitter/DTS direct solver imports, direct type evaluation during printing,
   and `TypeData`/`lookup()` guardrail exceptions.
8. `Printer` and `DeclarationEmitter` ambient state fields, especially fields
   added for one transform or one baseline family.
9. Emitter/DTS tests that assert fragments instead of exact output or structured
   plan/summary facts.

## Ten Parallel Agent Tracks

These tracks are designed for 10 concurrent agents. Each track can own multiple
small PRs, but each PR should state one invariant and avoid duplicating another
track's active draft PR.

### Track 1: Compatibility Corpus, Dashboard, And Triage Gates

Scope: project benchmark harness, public benchmark reporting, fixture status,
`tsc` oracle comparison, diagnostic-delta extraction, reduction queue, and
bug-family intake.

Core invariant: correctness status is reported separately from speed; no speed
headline is meaningful for a project until correctness status is green or
explicitly out of scope.

Acceptance:

1. Dashboard rows exist for Kysely, Zod, ts-toolbelt, type-fest, ts-essentials,
   large-ts-repo, and Next.js full.
2. Failed rows include exit class, first diagnostic deltas, semantic owner
   family, and phase reached.
3. Benchmark reductions become owning-crate tests when root cause is known.
4. Every semantic PR that claims project-corpus impact names the row and bug
   family it moves.

### Track 2: Type Evaluator And Purpose-Specific Normalization

Scope: conditional types, mapped types, template literal types, `infer`,
distributivity, key remapping, indexed access, utility types, intrinsics, and
recursive evaluation.

Core invariant: deferred type operations are evaluated through solver-owned,
purpose-specific queries with memoization keyed by expression identity,
substitution environment, compatibility mode, normalization purpose, and
recursion/fuel state. There is no universal eager normal form.

Acceptance:

1. Reduced failures from ts-toolbelt, type-fest, ts-essentials, Kysely, and Zod
   move into focused tests.
2. Deferred/unresolved conditionals are represented explicitly rather than
   erased to `any` or `error`.
3. Checker-local evaluation shortcuts trend down.
4. Callers name why they normalize: relation input, property lookup, inference
   source/target, diagnostic display, or flow narrowing.

### Track 3: Inference Sessions, Instantiation, And Cache Contracts

Scope: generic call inference, constructor inference, overload inference,
contextual typing, class/mixin instantiation, `this` substitution, stale aliases,
and relation/evaluation/inference cache keys.

Core invariant: generic inference is a bounded solver-owned transaction:
collect constraints, solve by priority, commit substitutions, then discard
session state. Cache keys include every input that can change the answer:
substitution environment, relation/variance mode, compatibility mode, lib/module
context, fresh-literal state, `this` type, and relevant flow/request context.

Acceptance:

1. Cache-enabled and cache-disabled modes agree on targeted semantic tests.
2. Reordered declarations/files produce stable diagnostics.
3. Self-contradictory errors such as `T` not assignable to `T` are treated as
   cache/keying bugs until proven otherwise.
4. Same-checker-context repeated generic calls cannot leak inference state into
   later calls.
5. Instantiation cache comments, stats, and production behavior agree.

### Track 4: Relations, Variance, Call Signatures, And Class Compatibility

Scope: assignability, function parameter variance, callable interfaces, overload
implementation compatibility, `call`/`apply`/`bind`, method bivariance
exceptions, abstract construct signatures, class/`this`/accessor/super/mixin
compatibility, freshness/excess-property policy, and weak type detection.

Core invariant: `TS2322`, `TS2345`, `TS2394`, `TS2416`, and related relation
paths flow through one assignability/relation gateway: relation -> structured
reason -> diagnostic rendering. Class-like compatibility is a typed
compatibility surface, not accidental object-shape comparison.

Acceptance:

1. Variance mode is explicit in relation context.
2. Bivariant and `any` propagation exceptions live in compatibility policy, not
   scattered call-site flags.
3. Callable interface assignment does not fall back to property comparison when
   `tsc` would compare signatures.
4. `TS2322`/`TS2345`/`TS2394`/`TS2416` paths that need relation plus failure
   reason use `RelationRequest`/`RelationOutcome` or a narrower
   diagnostic-capable wrapper, not raw boolean assignability followed by local
   semantic post-checks.
5. Boolean fast paths remain cheap; explanation mode may build structured
   failures but must be explicitly requested.
6. Accessor pairs, receiver `this`, constructor abstraction, and class
   static/instance sides are handled by class-aware relation helpers.

### Track 5: Key-Space, Indexed Access, And Property Semantics

Scope: `keyof`, indexed access, property lookup, index signatures, mapped-key
remapping, template literal pattern keys, numeric/string key compatibility,
symbol and unique-symbol keys, well-known symbols, excess-property
classification, and readonly/optional property metadata.

Core invariant: property identity is modeled as a solver-owned key space, not as
ad-hoc strings. `keyof`, `T[K]`, mapped projection, index signatures, relation
property comparison, and diagnostics ask the same key-space/query helpers.

Acceptance:

1. TS7053/TS2536/TS2353-style paths share key-space queries instead of
   duplicating string/number/symbol logic.
2. Template literal pattern keys and numeric-string compatibility are structural
   facts, not rendered-string checks.
3. Query-boundary property classification avoids owned `String` maps on hot
   semantic paths when atoms/symbols/key-space handles are available.
4. Key-space query results are interned or otherwise identity-cheap enough for
   relation/property hot paths.

### Track 6: Flow Graph And Solver-Owned Narrowing Predicates

Scope: discriminated unions, destructured discriminants, user-defined
predicates, `in` narrowing, optional/truthiness narrowing, array/object guards,
exhaustive switch behavior, and alias-aware flow facts.

Core invariant: checker supplies flow facts and locations; solver-owned
narrowing predicates compute semantic narrowed types without leaking branch
state or creating a second evaluator in checker flow code.

Acceptance:

1. Kysely/Zod guard reductions pass.
2. Destructured discriminant and mapped-union `in` narrowing cases pass.
3. Nested narrowing cannot corrupt outer flow state.
4. Predicate application is cacheable by input type, predicate payload,
   compiler flags, and resolver generation.

### Track 7: Symbol, Lib, Module, And Stable Identity

Scope: `import()` types, namespace/enum merging, module augmentations, DOM/lib
globals, symbol keys, global declarations, alias owners, `DefId` mapping,
class static/instance identity, enum value/namespace identity, display
provenance handles, and cross-file stable identity.

Core invariant: the same semantic entity has one identity across files/libs and
is referenced through stable binder/solver IDs, not recovered from syntax or
string names in hot paths.

Acceptance:

1. Lib selection and global scope construction are explicit and reproducible.
2. `import("./x").T` works inside conditional/keyof/Parameters/ReturnType-style
   contexts.
3. Unresolved identifiers do not silently become `any` unless `tsc` would do so.
4. Actual-lib alias admissions such as utility aliases and iterator/Intl rows
   are treated as transitional; stable lib identity queries replace allowlists.
5. Display provenance is a structured side channel over stable identity, not a
   semantic decision based on rendered type strings.

### Track 8: Diagnostics, Display, Parser Options, And Feature Gates

Scope: diagnostic code/position/priority, type display provenance, parser
recovery facts, compiler-option and language-version gates, decorators,
auto-accessors, top-level await, global declaration restrictions, and
syntax-only validation that should not depend on relation machinery.

Core invariant: diagnostics render from structured semantic or syntax facts.
Syntax/option gates are checker validation over AST/binder facts; semantic
diagnostics are downstream of solver/query-boundary reasons; neither path uses
source-text snippets or rendered type strings as semantic input.

Acceptance:

1. Wrong-code/wrong-position diagnostics move behind structured reason or
   syntax-gate helpers.
2. Type display fixes consume display provenance and visibility facts rather
   than changing semantic types for presentation.
3. Option-gate diagnostics are tested under both allowed and disallowed
   compiler options.
4. Parser recovery facts are explicit inputs to diagnostics/emit when needed;
   consumers do not infer malformed syntax behavior by scanning substrings.

### Track 9: Emit Robustness, DTS Boundary, LSP, And WASM Consumers

Current owner: M4-T9-10.

Scope: JS emit, declaration emit, LSP, WASM, and compiler-service facade work.
Emit/DTS has enough failures and architectural risk to be its own recovery
campaign inside this track, not a bucket of baseline whack-a-mole. LSP/WASM
expansion beyond parity or release gates remains back burner.

Core invariant: emit, LSP, and WASM consume compiler outputs and semantic views;
they do not own type algorithms or rederive checker/solver facts.

Subtracks:

1. **JS transform plan graph**: lowering produces ordered per-file/per-node
   transform actions; complex transforms converge on IR or structured output
   plans instead of hidden `Printer` state.
2. **Declaration summary boundary**: binder/checker/solver produce a
   `DeclarationSummary`/`PublicApiSummary` with exported declarations,
   nameability, import dependencies, portability diagnostics, inferred
   declaration display requests, and JSDoc-derived facts.
3. **Emit failure-family dashboard**: JS/DTS pass counts, timeout count, and top
   families are visible; every emit PR names the family it moves.
4. **Emitter state and guardrail burn-down**: remove `TypeData`/`lookup()`
   guardrail exceptions, reduce broad ambient fields, and retire text-based
   import/type usage heuristics except for explicitly structured parser-recovery
   facts.

Acceptance:

1. JS emit fixes are tied to transform families and either reduce a family count
   or unblock a named transform-plan migration.
2. DTS fixes consume documented semantic summaries instead of broad reach-through
   into solver internals or fresh type evaluation during printing.
3. LSP/WASM paths converge on one compiler service front door and consume
   semantic views rather than matching raw `TypeData`.
4. No new broad `Printer` or `DeclarationEmitter` state fields are added without
   an owner, invariant, and removal/migration condition.
5. Source-text recovery moves toward parser-provided facts; emitter code should
   not infer malformed syntax behavior by scanning substrings.

### Track 10: Guardrails, Tooling, Residency, And Performance Substrate

Current owner: M4-T9-10.

Scope: large-repo memory/runtime, stable skeleton indexes, bounded arena
residency, project graph reuse, compiler-service orchestration, incremental
invalidations, architecture guardrails, test fixtures, cache/order test
harnesses, docs cleanup, CI ergonomics, and behavior-preserving refactors that
unblock tracks 1-9.

Core invariant: performance work must preserve semantic identity and
correctness; large-repo speed comes from stable semantic facts, bounded
residency, and measurable guardrails, not from checker-local semantic shortcuts.
Refactors reduce the number of semantic paths or make invariants measurable.

Acceptance:

1. Large repo finishes without OOM/timeout, then gets faster.
2. Cross-file lookups increasingly answer from skeleton/stable indexes.
3. Cache/residency changes include before/after measurements when practical.
4. Lib/interface reuse proves semantic identity and type-parameter preservation;
   rejected missing-interface lib probes should not become name-only allowlists.
5. Guardrails catch forbidden checker/solver/emitter boundary drift.
6. Test harnesses make cache-disabled and order-randomized checks easy to run.
7. Docs stay concise and do not recreate claim-file bookkeeping.
8. Guardrails cover source-text/rendered-type semantic decisions and emitter
   direct solver-internal access once the current baselines have owners.
9. Refactor PRs that only split files are accepted when they reduce measurable
   state, remove a boundary exception, or unblock a named campaign.
10. Broad performance rewrites wait until a correctness row is green, a red row
    is blocked by runtime/residency, or the change has a clear semantic
    identity contract.

## Local Verification Rules

1. Never run full conformance, emit, or fourslash suites locally.
2. Use `cargo nextest run` instead of `cargo test`.
3. Run local commands only when they answer a specific debugging question.
4. Wrap full-suite or memory-intensive commands with `scripts/safe-run.sh`.
5. Draft PR CI runs light gates; ready-for-review CI runs heavy gates.
6. Do not wait idle for CI. Push, record the run URL if useful, and move to
   non-overlapping work.

## Definition Of Done

This roadmap is succeeding when:

1. Diagnostic conformance reaches exact `12,582 / 12,582`; rounded `100.0%` is
   not treated as exact completion.
2. JavaScript emit reaches exact `13,530 / 13,530` and declaration emit reaches
   exact `1,669 / 1,669`, or the roadmap names the remaining blocked families.
3. Fourslash reaches exact `6,562 / 6,562`, or the roadmap names the remaining
   blocked cases.
4. Kysely and Zod become green project-corpus rows.
5. At least one advanced-types corpus among ts-toolbelt, type-fest, and
   ts-essentials is green.
6. large-ts-repo exits successfully without OOM/timeout.
7. Cache-enabled/cache-disabled diagnostics agree on targeted advanced-type and
   project-corpus checks.
8. `TS2322`/`TS2345`/property/call failures route through shared relation and
   query-boundary paths.
9. Checker-local type semantics and direct solver-internal pattern matching
   trend down.
10. Fingerprint/source-text/rendered-type rewrites trend down and no new ones are
   added without an explicit temporary-shortcut ledger entry.
11. Emit/DTS work reduces named failure families while moving toward
   `EmitPlan`/`DeclarationSummary` boundaries, not merely helper splitting.
12. Emit/DTS work no longer consumes the majority of active PR bandwidth while
   checker/solver benchmark blockers remain red.
13. GitHub draft PRs and comments are sufficient to understand active ownership;
   no claim-doc system reappears.
