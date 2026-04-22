# Diagnostic Render Conformance Plan

Last updated: April 22, 2026.

This plan targets the largest remaining conformance gap after the latest audit:
diagnostic fingerprints. The current snapshot is `12097 / 12581` passing
(`96.2%`), with `484` failing tests. Of those failures, `345` are
fingerprint-only: `tsz` emits the same diagnostic code set as `tsc`, but differs
in message text, location, file, or diagnostic count.

This is not primarily a missing-feature problem. The dominant gap is diagnostic
rendering and emission policy.

## Audit Findings

The fingerprint-only failures break down into four surfaces:

| Surface | Problem | Representative impact |
| --- | --- | --- |
| Type display identity | Alias, expansion, apparent type, and structural display differ from `tsc`. | `TS2322`, `TS2345`, `TS2339` message drift |
| Pair display | Source and target are often formatted independently, so same-named types collide. | Messages like `Type 'MyClass' is not assignable to type 'MyClass'.` |
| Literal display | Widening and literal preservation are context-sensitive and currently inconsistent. | `boolean` vs `true`, `"false"` vs `string` |
| Emit count and anchors | Some failures are the right code with the wrong number of instances or wrong span. | under-count, over-count, and location-only buckets |

The top fingerprint delta codes are:

| Code | Fingerprint deltas |
| --- | ---: |
| `TS2322` | 554 |
| `TS2345` | 232 |
| `TS2339` | 166 |
| `TS1005` | 52 |
| `TS2353` | 37 |

Test-level fingerprint-only categories:

| Category | Tests |
| --- | ---: |
| mixed message/count/location | 116 |
| message-only | 84 |
| under-count | 82 |
| over-count | 44 |
| location-only | 12 |
| per-instance wrong code | 7 |

## Validated Code Findings

The initial architectural hypothesis was that `tsz` needed a new diagnostic
rendering system. Code review showed a narrower and safer path:

- `TypeFormatter` already has useful policy knobs in
  `crates/tsz-solver/src/diagnostics/format/mod.rs`: diagnostic mode, display
  properties, strict-null display, long property receiver display, alias
  skipping, and intersection display controls.
- The checker already creates fully contextual diagnostic formatters through
  `create_diagnostic_type_formatter` in
  `crates/tsz-checker/src/context/def_mapping.rs`.
- Pair-aware formatting already exists through
  `format_type_pair_diagnostic` in
  `crates/tsz-checker/src/state/type_environment/formatting.rs`, backed by
  `TypeFormatter::format_pair_disambiguated` in
  `crates/tsz-solver/src/diagnostics/format/compound.rs`.
- `DiagnosticRenderRequest` already exists in
  `crates/tsz-checker/src/error_reporter/fingerprint_policy.rs`, but it owns
  anchor, code, message, and related-info emission. It should not be stretched
  into type-display policy.
- Many high-volume paths still format source and target independently after
  contextual display recovery. Examples include the top-level `TS2322` helpers
  in `crates/tsz-checker/src/error_reporter/assignability.rs`, `TS2345`
  rendering in `crates/tsz-checker/src/error_reporter/call_errors/error_emission.rs`,
  and related-info rendering in
  `crates/tsz-checker/src/error_reporter/fingerprint_policy.rs`.

The key correction is: do not start by globally changing
`format_type_for_assignability_message`. It is a broad single-type formatter
used in many non-pair contexts, so global changes there have high regression
risk.

## Principles

1. Reuse existing formatter knobs before adding new solver behavior.
2. Keep checker-only concerns in the checker: `NodeIndex`, annotation text,
   expression text, diagnostic role, and source-span policy must not leak into
   solver formatting.
3. Preserve existing contextual display recovery. Pair-aware rendering should
   be a conservative finalization step, not a replacement for source-specific
   display logic.
4. Separate message rendering from diagnostic count and anchor policy.
5. Treat parser diagnostics as a separate pipeline from checker diagnostics.

## Phase 1: Build A Stable Render Corpus

Create or extend a script under `scripts/conformance/` that consumes the current
snapshot artifacts and verbose fingerprint logs, then emits JSON or CSV buckets.

Inputs:

- `scripts/conformance/conformance-detail.json`
- `scripts/conformance/conformance-snapshot.json`
- verbose logs produced with `--print-fingerprints`

Required buckets:

- message-only
- location-only
- under-count
- over-count
- same-code wrong-instance
- true wrong-code
- missing-code

The script should also report code-specific deltas for `TS2322`, `TS2345`,
`TS2339`, `TS1005`, and `TS2353`.

Success criteria:

- We can measure whether a render change reduces fingerprint-only failures.
- The output distinguishes message fixes from count and location changes.
- The top-code buckets are tracked independently.

## Phase 2: Add Conservative Pair Finalization

Add a checker-side helper that runs after existing source, target, argument, and
parameter display recovery.

The helper should:

- accept source type, target type, current source display, and current target
  display
- call `format_type_pair_diagnostic(source, target)` only when the current
  displays collide or have the same bare nominal name
- replace displays only when pair formatting actually distinguishes the types
- avoid replacing complex displays recovered from annotation text, expression
  text, literal-sensitive call arguments, rest parameters, or contextual
  function displays

Initial wiring targets:

- `format_top_level_assignability_message_types`
- `format_top_level_assignability_message_types_at`
- generic `TS2322` fallback before the `TS2719` decision
- `TS2345` primary rendering after
  `format_call_argument_type_for_diagnostic` and
  `format_call_parameter_type_for_diagnostic`

Important guard:

- Preserve `TS2719` only when pair formatting cannot distinguish the two names.
  If import qualification or namespace qualification can distinguish them, emit
  normal `TS2322` with the qualified displays.

Success criteria:

- Same-named cross-module and symlink class tests stop rendering identical type
  names.
- `TS2322` and `TS2345` fingerprint deltas drop.
- No broad churn in literal, rest-parameter, or annotation-derived messages.

## Phase 3: Apply Pair Policy To Related Information

After primary `TS2322` and `TS2345` rendering is stable, apply the same
conservative pair finalization to related information.

Start with:

- missing-property related info
- missing-properties related info
- property-type-mismatch related info
- return-type-mismatch related info

Primary target file:

- `crates/tsz-checker/src/error_reporter/fingerprint_policy.rs`

Success criteria:

- `TS2345` tests with correct primary messages but mismatched related messages
  move out of the fingerprint-only bucket.
- Related-info ordering, deduplication, and anchor behavior remain stable.

## Phase 4: Introduce Checker-Side Type Display Policy

Only after pair finalization is measured, introduce a small checker-owned display
policy adapter. This should not replace the solver formatter and should not
replace `DiagnosticRenderRequest`.

The policy should describe display intent and delegate to existing helpers:

| Policy role | Existing helper to preserve |
| --- | --- |
| default diagnostic | `format_type_diagnostic` |
| widened diagnostic | `format_type_diagnostic_widened` |
| flattened object/TS2741 display | `format_type_diagnostic_flattened` |
| assignability display | `format_type_for_assignability_message` |
| assignment source | `format_assignment_source_type_for_diagnostic` |
| assignment target | `format_assignment_target_type_for_diagnostic` |
| call argument | `format_call_argument_type_for_diagnostic` |
| call parameter | `format_call_parameter_type_for_diagnostic` |
| property receiver | `format_property_receiver_type_for_diagnostic` |

The initial policy should model only roles already present in the code. Add new
fields only when a measured failure requires them, such as:

- preserve source alias
- preserve target annotation
- widen literal
- preserve literal
- use apparent type
- use flattened object display
- use property receiver display
- allow import qualification

Success criteria:

- New display behavior is requested through role/policy, not scattered boolean
  branches.
- Solver `TypeFormatter` remains checker-agnostic.
- Existing bridge helpers become easier to reason about, not bypassed.

## Phase 5: Count And Anchor Workstream

Do not expect type formatting changes to fix under-count, over-count, or
location-only failures.

Semantic anchor fixes belong in:

- `crates/tsz-checker/src/error_reporter/fingerprint_policy.rs`
- `DiagnosticAnchorKind`
- `resolve_diagnostic_anchor_node`
- `normalized_anchor_span`

Semantic count fixes usually belong where diagnostics are produced, not in
`DiagnosticRenderRequest`. Important bypasses include:

- direct `Diagnostic` construction in
  `crates/tsz-checker/src/error_reporter/render_failure.rs`
- solver diagnostic builders used by call and overload diagnostics
- producer-side suppression and deduplication in checker context code

Success criteria:

- Location-only tests are categorized by anchor kind.
- Under-count and over-count tests are traced to producer paths.
- `DiagnosticRenderRequest` usage expands where it centralizes anchor and
  related-info policy, but it is not used as a catch-all count fix.

## Phase 6: Parser Diagnostic Workstream

Parser diagnostics should be tracked separately from checker render work.

Parser mismatches such as `',' expected` vs `':' expected` or `')' expected`
come from parser/scanner generation and recovery, primarily:

- `crates/tsz-parser/src/parser/state.rs`
- parser `parse_expected` and `parse_error_at` paths
- scanner diagnostics
- CLI parse-diagnostic filtering and conversion

Success criteria:

- Parser failures are bucketed independently from type-render failures.
- Parser fixes do not churn checker diagnostic formatting.

## Validation Loop

For each phase, run targeted tests first, then a snapshot.

Targeted examples:

```sh
./scripts/conformance/conformance.sh run --filter "arrayFrom.ts" --verbose
./scripts/conformance/conformance.sh run --filter "moduleResolutionWithSymlinks" --verbose
./scripts/conformance/conformance.sh run --filter "constraintWithIndexedAccess.ts" --verbose
./scripts/conformance/conformance.sh run --filter "didYouMeanElaborationsForExpressionsWhichCouldBeCalled" --verbose
```

Full snapshot:

```sh
./scripts/conformance/conformance.sh snapshot
```

Track these metrics after every change:

- total passing tests
- fingerprint-only tests
- message-only tests
- under-count tests
- over-count tests
- location-only tests
- `TS2322` fingerprint deltas
- `TS2345` fingerprint deltas
- `TS2339` fingerprint deltas

## Near-Term Implementation Order

1. Add the render-corpus classifier.
2. Add conservative pair finalization.
3. Apply pair finalization to `TS2322`.
4. Apply pair finalization to `TS2345`.
5. Apply pair finalization to related information.
6. Add checker-side display policy only where remaining measured failures need
   explicit roles.
7. Work count, anchor, and parser buckets independently.

## Definition Of Done

This workstream is complete when:

- diagnostic display decisions are driven by explicit checker-side context
- pair-aware rendering is consistently used for same-name source/target pairs
- alias and literal display policy is role-based rather than scattered
- anchor and count issues are tracked separately from message rendering
- parser diagnostics have their own corpus and validation loop
- fingerprint-only failures are no longer the dominant remaining conformance gap
