# Diagnostic Rendering, Elaboration, Priority, Suppression, and Recovery

The error reporter is where the checker turns semantic answers from the solver
into the exact `tsc` diagnostic stream: a deduplicated, deterministically
ordered list of `Diagnostic` values with anchored spans, nested
`related_information` elaboration chains, "did you mean?" spelling suggestions,
and JSDoc-surface errors. It is the largest single concern in `tsz-checker`
(`crates/tsz-checker/src/error_reporter/` is split into roughly seventy focused
modules to stay under the 2000-line file ceiling), and it sits squarely on the
checker side of the architecture boundary: it *reads* types and asks the solver
for structured `SubtypeFailureReason` values, but it never re-runs the relation,
inference, or evaluation kernels itself, and it never lets the rendered text
become an input to a semantic decision.

This document traces how a diagnostic is constructed end-to-end: the
`DiagnosticRenderRequest` policy object, anchor resolution, related-information
generation from solver failure reasons, the dedup/priority/suppression rules in
`push_diagnostic`, the display budgets that bound rendering of pathological
types, the JSDoc semantic surface, the parser-recovery interplay, and the typed
`RecoverySites` registry that distinguishes a *recovered* `any` from a declared
`any`. For the relation-side machinery that produces the failure reasons, see
[solver-relations](solver-relations.md) and the structured-reason gateway in
[checker-assignability-gateway](checker-assignability-gateway.md).

## Owns / Must not own

| The error reporter owns | The error reporter must NOT own |
|---|---|
| Mapping a `SubtypeFailureReason` to its `related_information` chain (`related_from_failure_reason`) | Computing whether `source` is assignable to `target` — that is the solver relation (see [solver-relations](solver-relations.md)) |
| Diagnostic anchor resolution and span normalization (`DiagnosticAnchorKind`, `normalized_anchor_span`) | Producing the structured failure reason — the solver builds `SubtypeFailureReason` |
| Deduplication, priority, and suppression keys (`push_diagnostic`, `error`) | Type display semantics; the printer reads types, types never read printer output |
| Type-display *roles* and pre-evaluation for display (`DiagnosticTypeDisplayRole`) | Re-deciding a relation outcome from a rendered string |
| Spelling suggestions and lib-target hints (`find_similar_identifiers`, `get_lib_for_type_property`) | Recovering an unresolved type — recovery records a reason and returns `TypeId::ANY` |
| The JSDoc diagnostic surface (`jsdoc/diagnostics.rs`) | Parsing JSDoc type expressions into `TypeId` outside the `jsdoc::resolution` kernel |

The single hard invariant worth restating: **the printer may read types; types
must never read printer output.** No reporter path uses a formatted diagnostic
string as a semantic predicate. Where a reporter needs to *decide* something
(e.g. "is this missing property an `Object.prototype` member?"), it asks a
solver/query-boundary structural helper such as `is_object_prototype_method`,
never a substring match on the rendered message.

## Module map

| Path | Role |
|---|---|
| `error_reporter/mod.rs` | Module index; re-exports the render-policy types (`DiagnosticRenderRequest`, `DiagnosticAnchorKind`, `RelatedInformationPolicy`, `ResolvedDiagnosticAnchor`) |
| `error_reporter/fingerprint_policy.rs` | Anchor resolution, span normalization, `related_from_failure_reason`, `normalize_related_information` — the heart of elaboration and anchoring |
| `error_reporter/emitters.rs` | Low-level emit helpers (`error_at_node`, `emit_render_request`, position emitters for TS6133/TS1109) |
| `context/diagnostic_push.rs` | `error` / `push_diagnostic`: the dedup keys, priority eviction, and collision reconciliation |
| `error_reporter/render_failure.rs` (+ submodules) | The TS2322 *single source of truth* renderer for a `SubtypeFailureReason` into a `Diagnostic` |
| `error_reporter/call_errors/elaboration.rs` | Per-argument elaboration for TS2345 (object/array literal, callback return, callback body) |
| `error_reporter/core/type_display/` | Display normalization passes + `recursion_guard.rs` depth cap |
| `error_reporter/display_budget.rs` | Per-rendered-type work budget (visit + eval fuel) bounding display normalization |
| `error_reporter/type_display_policy.rs` | `DiagnosticTypeDisplayRole`: checker-owned display intent kept out of the solver formatter |
| `error_reporter/suggestions.rs` | Levenshtein spelling suggestions (TS2551/TS2552), TS2550 lib-target hints |
| `error_reporter/name_resolution.rs` | TS2304/TS2552/TS2583/TS2584 name-resolution reporting |
| `error_reporter/type_value.rs` | Type-vs-value mismatch (TS2693/TS2749/TS2708/TS2709) |
| `error_reporter/operator_errors.rs`, `properties.rs`, `generics.rs` | Per-family reporters (binary operators; TS2339/TS2741; TS2314/TS2344/TS2367/TS2352) |
| `jsdoc/diagnostics.rs` (+ siblings) | JSDoc semantic surface: TS8033/TS8021/TS2304/TS2300/TS1109 for typedef/import/satisfies |
| `recovery/mod.rs` | `RecoveryReason` / `RecoverySites`: typed sentinels for `TypeId::ANY` fallbacks |
| `symbols/` | Symbol resolution helpers feeding name-text and "declared here" related info |

## Data shapes

A diagnostic is a flat record in `tsz-common`
(`crates/tsz-common/src/diagnostics/mod.rs`):

```
Diagnostic {
    category: DiagnosticCategory,   // Warning | Error | Suggestion | Message
    code: u32,                      // tsc diagnostic number, e.g. 2322
    file: String,
    start: u32, length: u32,        // half-open span [start, start+length)
    message_text: String,
    related_information: Vec<DiagnosticRelatedInformation>,
}

DiagnosticRelatedInformation {
    category, code, file, start, length, message_text,
    depth: u8,    // elaboration nesting level; 0 = first level (2 spaces)
}
```

The `depth` field is what makes a flat `Vec<DiagnosticRelatedInformation>`
render as `tsc`'s progressively indented elaboration chain. `depth: 0` renders
at two spaces; each deeper level adds two more. Genuine cross-location "see also"
entries (e.g. "'B' is declared here.") stay at depth `0`. The
`with_depth_shift` helper owns the single shift-and-clamp routine so re-seating a
sub-diagnostic's chain at a different nesting level cannot drift.

`Diagnostic` carries the canonical comparators that mirror `tsc`'s
`compareDiagnostics`: `compare_skip_related_information` orders by
`file -> start -> length -> code -> message_text`, and `compare` adds a final
`compare_related_information` tiebreaker (shorter related list first, then
element-by-element). This total order is what keeps the final reported order
deterministic regardless of the (potentially parallel, hash-map-driven) order in
which diagnostics were produced.

## End-to-end: a small TS2322 walk-through

Trace `const x: { a: string } = { a: 1 };`.

```
checker (declaration check)
  └─ check assignability of source {a: 1} to target {a: string}
       └─ solver relation answers "not related" + SubtypeFailureReason
            └─ PropertyTypeMismatch { property_name: "a",
                                       source_property_type: 1,   (number literal)
                                       target_property_type: string,
                                       nested_reason: None }
  └─ reporter builds DiagnosticRenderRequest::with_failure_reason(
           anchor_kind = RewriteAssignment, code = 2322,
           message = "Type '{ a: number; }' is not assignable to type '{ a: string; }'.",
           reason, source, target)
  └─ emit_render_request(node, request)            (emitters.rs)
       ├─ resolve_diagnostic_anchor(node, RewriteAssignment)
       │     ├─ assignment_anchor_node: walk up to the VariableDeclaration,
       │     │   pick the `x` name span (variable_declaration_anchor)
       │     └─ normalized_anchor_span: trim to the leading identifier `x`
       ├─ related_from_failure_reason(&reason, source, target, anchor.node_idx)
       │     └─ PropertyTypeMismatch arm emits two related lines:
       │          depth 0: "Types of property 'a' are incompatible." (TS2326-style header)
       │          depth 1: "Type 'number' is not assignable to type 'string'."
       ├─ normalize_related_information(items, ELABORATION)   (dedupe + depth-aware sort)
       └─ ctx.push_diagnostic(diag)                 (context/diagnostic_push.rs)
              └─ dedup key (start, 2322) [+ message hash for 2322], priority, push
```

Three layers cooperate: the **solver** decided not-assignable and named *why*
(`PropertyTypeMismatch`); the **reporter** turned that reason into an anchored
message plus a depth-tagged elaboration chain; the **context** deduplicated and
committed it. The checker never re-walked the object shapes to decide
assignability — it only walked them to *render* the answer.

## Render requests and the central emit path

`DiagnosticRenderRequest` (`fingerprint_policy.rs`) is the explicit policy object
a reporter constructs to describe *what* to report. It captures four decisions:

- `anchor_kind: DiagnosticAnchorKind` — how to resolve the span from the AST node.
- `code: u32` and `message: String` — the headline.
- `related: RelatedInfoStrategy` — `None`, `FromFailureReason { reason, source, target }`,
  or `Prebuilt(Vec<DiagnosticRelatedInformation>)`.
- `related_policy: RelatedInformationPolicy` — `ELABORATION`, `WRAPPED_DIAGNOSTIC`,
  or `OVERLOAD_FAILURES`, controlling whether the primary is folded in, whether to
  dedupe, and any length limit.

Constructors keep the common shapes ergonomic: `simple`, `simple_msg(code, args)`
(looks up the message template and formats it), `with_failure_reason`, and
`with_related`. The central method `emit_render_request` (`emitters.rs`) then
handles *how*: it resolves the anchor via `resolve_diagnostic_anchor`, generates
related info (calling `related_from_failure_reason` for the `FromFailureReason`
strategy), normalizes it under the request's policy, and pushes the assembled
`Diagnostic` through `push_diagnostic`. The variant
`emit_render_request_at_anchor` accepts a pre-resolved `ResolvedDiagnosticAnchor`
to avoid double-resolution when the related info already needed the anchor span.

This funnel is the reason "open-coded anchor/related-info decisions" do not
spread across the ~70 reporter modules: a reporter declares intent, the policy
surface executes it.

## Anchor resolution and span normalization

`DiagnosticAnchorKind` enumerates how a span is derived from the diagnostic's AST
node (`fingerprint_policy.rs`):

| Kind | Resolves to |
|---|---|
| `Exact` | the node itself |
| `RewriteAssignment` | walk up to the assignment/variable-declaration site (`assignment_anchor_node`) |
| `CallPrimary` | the callee, or the property name for `a.b(...)` (`call_primary_anchor_node`) |
| `OverloadPrimary` | concat-call first array element, else `CallPrimary` |
| `PropertyToken` | the `name_or_argument` of a property/element access |
| `ElementAccessExpr` / `ElementIndexArg` | the element-access expression or its index argument |
| `TypeAssertionOverlap { target_type }` | the first object/array member absent from `target_type` |

`resolve_diagnostic_anchor` first maps the node through
`resolve_diagnostic_anchor_node`, then reads the source location and applies
`normalized_anchor_span`. That normalization is `tsc`-parity-critical: `tsc`
anchors many declaration diagnostics on the *name* token, not the full
declaration span. So for `Identifier` nodes the span is trimmed to
`escaped_text.len()`; for `VariableDeclaration`, `PropertyAssignment`,
`PropertySignature`, and `BindingElement` it is trimmed to the leading
identifier (`leading_identifier_len` scans the source text); and for
`PropertyDeclaration` / `Parameter` (which may carry leading modifiers) it
re-resolves through the explicit `name` child so `private`/`readonly`/`...` are
excluded.

The `assignment_anchor_node` walk encodes several `tsc` quirks discovered against
baselines: a starting `Parameter` *is* the assignment site (so the error lands on
the parameter name, not the enclosing `(` of the function expression); a function
expression that is the RHS of an assignment keeps walking up to the assignment
statement; and `var x: T = obj.prop` anchors on the initializer when its type is
callable but on the variable name otherwise
(`variable_declaration_anchor` / `is_callable_type`).

## Elaboration: turning a failure reason into a related chain

`related_from_failure_reason` (`fingerprint_policy.rs`) is the dispatcher that
maps a `SubtypeFailureReason` (a re-export of the solver's enum through
`query_boundaries::common`) to a `Vec<DiagnosticRelatedInformation>`. It is the
TS2345 (call-argument) elaboration surface; the direct-assignment (TS2322)
surface lives in `render_failure.rs::render_failure_reason`. The two are kept in
sync by an explicit rule: whenever a TS2345 reason has a *structural drill* the
hand-rolled two-line shape cannot represent, the dispatcher delegates to the
TS2322 renderer via `reanchored_container_related` so both surfaces carry the
same dotted-path collapse, tuple positions, and array/index drill.

Key arms (each cites the message constant it formats):

- `MissingProperty` / `MissingProperties` — TS2741-style "Property 'p' is missing
  in type 'S' but required in type 'T'." (or the "following properties" plural,
  with a "...and N more" overflow past four). These arms carry several
  parity-driven *suppressions*: a callable source whose only missing members are
  `Object.prototype` methods is dropped
  (`should_suppress_missing_property_for_callable_source`); a primitive source
  is dropped; the synthetic private-brand name is dropped
  (`is_synthetic_private_brand_name`); and `Boolean`/`Number`/`String`/`Object`
  wrapper targets and intersection targets are dropped because `tsc` reports the
  argument-level message there instead.
- `PropertyTypeMismatch` — the `Types of property 'p' are incompatible.` header
  at depth 0 plus the inner `Type 'sp' is not assignable to type 'tp'.` at depth
  1, with an optional union-member line at depth 2. When the nested reason needs
  a full structural drill (`property_nested_reason_needs_full_drill`), the whole
  reason is delegated to the TS2322 renderer instead.
- `ReturnTypeMismatch` — emits *only* the inner `Type 'X' is not assignable to
  type 'Y'.` line (verified: `tsc` never emits an intermediate "Return type..."
  frame), then recurses into the nested reason.
- `IndexSignatureMismatch` — the `string/number index signature is
  incompatible:` header plus the inner not-assignable line.
- `OptionalPropertyRequired` — TS2327 "Property 'p' is optional in type 'S' but
  required in type 'T'." (distinct from the absent-property TS2741).
- The container family (`ArrayElementMismatch`, `TupleVariadicPositionMismatch`,
  `TypeArgumentMismatch`, `TupleElementTypeMismatch`, `TupleElementMismatch`,
  `TupleArityMismatch`), `UnionTargetMismatch`, `IntersectionTargetMismatch`, and
  `ParameterTypeMismatch` — all delegate through `reanchored_container_related`,
  which calls `render_failure_reason` and re-stamps each child line's category to
  `Message` and its span to the call-site anchor.

`reanchored_container_related` is the structural bridge that keeps TS2322 the
single source of truth: a tuple-argument mismatch would otherwise fall through to
`_ => return None` and lose its entire elaboration. Reasons that produce a
self-heading scalar/literal leaf, or union/conditional members handled by
`union_member_related_line`, deliberately *do not* delegate so their established
byte-identical shape is preserved.

## Normalization: dedupe + the depth-aware sort

`normalize_related_information` dedupes (on the tuple
`(category, code, file, start, length, message_text)`) and then sorts by
`file -> start -> depth -> message_text`. The `depth` key sitting *before* the
textual tiebreaker is load-bearing: without it, the alphabetic compare reverses
chains because `"Type "` (trailing space) sorts before `"Types"`, which would
swap a `Types of property 'p' are incompatible.` header below its
`Type 'X' is not assignable...` leaf. The policy's optional `limit` truncates the
list (used by `OVERLOAD_FAILURES`, which also sets `include_primary = false`).

## Dedup, priority, and suppression at commit time

Every diagnostic ultimately passes through `error` or `push_diagnostic` in
`context/diagnostic_push.rs`. These own three policies:

**1. Dedup keys.** The default key is `(start, code)` — a diagnostic at the same
span and code is emitted once. A set of codes instead use
`(start ^ message_hash, code)` so several genuinely distinct messages can coexist
at one span: TS18047/18048/18049, TS2322, TS2339, TS2374, TS2411, TS2413,
TS2416, TS2430, TS2536/2537/2538, TS4094. The motivating cases are documented
inline — e.g. TS2411 lets one property fail against both string and number index
signatures at the same span; TS4094 lets each private/protected member of an
exported anonymous class expression emit its own diagnostic at the owning
variable name. TS2318 at position 0 keys on the message hash alone so multiple
missing-global-type errors survive. The key derivation lives in one place,
`diagnostic_dedup_key_from_parts`.

**2. Name-resolution precedence.** `reconcile_name_resolution_precedence` (shared
by both `error` and `push_diagnostic`) encodes a small precedence lattice at a
single span: TS2301 ("Initializer of instance member cannot reference identifier
declared in constructor") outranks and evicts TS2304/TS2552/TS2663; TS2552/TS2663
("Did you mean...") outrank TS2304 ("Cannot find name"). An incoming
lower-precedence diagnostic is dropped; an incoming higher-precedence one evicts
the already-emitted losers (`retain` + remove from the `emitted` set).

**3. Related-information collision reconciliation.** When a candidate ties on
`(start, code)` *and* `compare_skip_related_information` is `Equal` but carries
*different* related info, "keep whichever arrived first" would make the surviving
diagnostic depend on solver traversal order. `prefers_candidate_diagnostic`
resolves it deterministically: prefer the richer elaboration (more related
entries), breaking ties with `Diagnostic::compare`. A related-free candidate can
never win, so the overwhelmingly common plain re-derivation short-circuits before
any scan.

**4. TS2322 overlap suppression.** `push_diagnostic` additionally drops a TS2322
that overlaps an already-recorded excess-property position
(`has_excess_property_position_in`) or an overlapping TS2322 message
(`has_overlapping_ts2322`), preventing a doubled assignability error where an
excess-property report already fired.

`finalize_recent_diagnostics` is the one sanctioned boundary for *rewriting* a
just-emitted diagnostic's span/code/message after the fact (e.g. repositioning a
missing-property error onto an initializer anchor, or downgrading TS2739 to
TS2322 once the relation outcome is known), keeping the buffer's `Vec` an
implementation detail.

## Display roles and the printer boundary

When a reporter needs a type's *string*, it does not hand a raw `TypeId` to the
solver formatter — it picks a `DiagnosticTypeDisplayRole`
(`type_display_policy.rs`) capturing the display *intent*: `DefaultDiagnostic`,
`WidenedDiagnostic`, `FlattenedDiagnostic`, `AssignmentSource`/`AssignmentTarget`
(carrying the opposite type and anchor for contextual widening),
`CallArgument`/`CallParameter`/`WeakCallParameter`, and `PropertyReceiver`. Each
role delegates to the specialized helper for that surface.

Crucially, the checker pre-*evaluates* some types before formatting because the
solver formatter cannot reach the checker's full evaluator (with
`TypeEnvironment`). For example `resolve_indexed_access_alias_for_display`
collapses `type WeakKey = WeakKeyTypes[keyof WeakKeyTypes]` to `object` to match
`tsc`'s loss of the outer alias, and `resolve_concrete_indexed_access_for_display`
reduces a *concrete* `Obj["m"]` to its member type — but only when concrete: an
indexed access carrying a free type parameter is legitimately deferred (`tsc`
keeps `T["m"]`), and pre-resolving it risks TS2589 on recursive generics, so it
is left opaque. This is the precise place where the boundary is honored:
checker-side evaluation feeds the formatter, and the formatted result is never
read back as a predicate.

## Caches, budgets, and recursion guards

Rendering one diagnostic must be bounded by the *size of the displayed type*, not
by full re-evaluation per node of an unbounded generic expansion. Three
mechanisms enforce that, all thread-local and inert outside a render scope.

| Mechanism | File | Bound | On exhaustion |
|---|---|---|---|
| `DisplayBudgetScope` | `display_budget.rs` | `DISPLAY_VISIT_BUDGET = 50_000` node visits + `DISPLAY_EVAL_FUEL = 8_000` eval steps per rendered type, plus an `eval_memo` | return the type unchanged (hard truncation, like `tsc`'s `...` elision) |
| `DisplayRecursionGuard` | `core/type_display/recursion_guard.rs` | `MAX_DIAGNOSTIC_DISPLAY_RECURSION_DEPTH = 100` nesting | leave the type unchanged |
| Callback-body elaboration depth | `call_errors/elaboration.rs` | a thread-local `CALLBACK_BODY_ELABORATION_DEPTH` cell capped at 1 | return `false` (skip re-entry) |

The `DisplayBudgetScope` exists because self-expanding generic applications such
as `Awaited<...>` chains intern fresh `TypeId`s on every evaluation, so
per-`TypeId` cycle sets never converge: the normalization pass caps recursion
*depth* but its *breadth* is unbounded, which made diagnostic emission
effectively non-terminating on large recursive types (issue #13040). The outermost
scope installs a fresh budget; nested scopes share it. `cached_eval` / `record_eval`
provide the scoped memo — with the invariant that **cycle-truncated returns must
not be memoized** (`record_eval` no-ops once `exhausted`), or a later non-cyclic
call would observe the truncation. The budgets are deliberately far above what any
realistic diagnostic consumes (the downstream formatter truncates nested printing
at depth 8 and elides property-receiver objects by depth 26 long before either
budget is visible), so legitimate messages stay byte-identical; only pathological
normalization is cut short.

The `DisplayRecursionGuard` is a single shared thread-local counter spanning all
mutually-recursive display normalization functions (resolving `Lazy` references,
widening fresh literals, re-applying display aliases, materializing finite mapped
types, stripping excess-property wrappers), decremented on `Drop` so every return
path is accounted for without threading a depth parameter. It guards against
worker-stack overflow on deeply self-expanding generic types (issue #12455).

The checker-level dedup `emitted` set (a `FxHashSet<(u32, u32)>` keyed by the
dedup key) plus the auxiliary `diagnostic_indices` (excess-property and
overlapping-TS2322 position indices, the TS2454 dedup set) are rebuilt by
`rebuild_emitted_diagnostics_from_current` / `rebuild_diagnostic_aux_indices`
after any speculative `retain` so removed diagnostics can be re-emitted on a later
pass. Speculative elaboration paths (see below) capture and roll back through
`ctx.snapshot_full` / `ctx.rollback_full` and inspect their own freshly emitted
diagnostics through `speculative_diagnostics_since` and `recent_diagnostics`.

## Call-argument elaboration (TS2345)

`call_errors/elaboration.rs` decides whether a failing argument should drill into
a *more specific* error rather than report TS2345 on the whole argument — exactly
`tsc`'s `elaborateError` behavior. The entry points are
`try_elaborate_object_literal_arg_error` (and `_with_source`),
`try_elaborate_assignment_source_error` (with an `_in_call_arg` variant that
disallows unresolved inference holes), and `try_elaborate_callback_body_diagnostics`.
Each returns `true` when it produced a more specific diagnostic, meaning the
caller must *not* also emit the argument-level TS2345.

Three parity-critical decisions live here:

- **Plain type assertions defer.** `expression_is_plain_type_assertion` short-
  circuits: an `expr as T` / `<T>expr` operand yields the asserted *non-fresh*
  type, so descending into it would manufacture per-property TS2353/TS2322
  diagnostics `tsc` never reports. `satisfies` and `as const` preserve freshness
  and still elaborate. This mirrors `tsc`'s `getRegularTypeOfObjectLiteral` /
  `elaborateError` boundary.
- **Unresolved inference holes gate return elaboration.** During generic call
  inference the expected callback return type can still reference uninstantiated
  type parameters (e.g. `B` from `compose<A, B, C>`); checking a concrete body
  type against such a placeholder would fire false TS2322s. The
  `allow_unresolved_holes` flag (`false` for call-argument paths, `true` for
  direct assignment, where the target is final) plus a check on whether the
  callable has its *own* generic type params decides whether to proceed
  (`type_has_unresolved_inference_holes`).
- **Speculative callback-body diagnostics.** `try_elaborate_callback_body_diagnostics`
  re-checks a contextually-typed callback body under a `TypingRequest`, collects
  only the relevant codes (TS2322, TS2345, TS2339, TS2769) whose spans fall inside
  the body, then `rollback_full`s the speculative state and re-pushes the unique
  diagnostics. It is depth-guarded against re-entry.

The `_with_source` family additionally short-circuits to `try_elaborate_array_literal_*`
or `try_elaborate_function_arg_return_error`. The function-return path encodes
several `tsc` quirks: it skips elaboration when the callable target carries extra
properties (the real failure is missing-property TS2739, not return-type TS2322);
skips generator callbacks (the body returns `TReturn`, not the full `Generator`
type); skips `void` expected returns; and skips callbacks with explicit parameter
annotations (`tsc` only drills into the return for fully contextually-typed
callbacks). For a `new Animal()` body it deliberately uses an `Exact` anchor so
`RewriteAssignment` does not walk up to the arrow and mis-display the source as
the function type.

## Spelling suggestions and lib-target hints

`suggestions.rs` reproduces `tsc`'s `getSpellingSuggestion`. The scorer is
`levenshtein_with_max` — edit distance with case-only substitutions weighted
cheaply (0.1) versus other substitutions (2.0) and threshold pruning. The
thresholds come from `spelling_thresholds`: `maximumLengthDifference = max(2,
floor(len * 0.34))` and initial `bestDistance = floor(len * 0.4) + 1`.
`consider_identifier_suggestion` applies the length-difference filter and a
case-only rule for short names, and `best_spelling_suggestion` owns the
single per-candidate loop.

`find_similar_identifiers` searches local visible binder names, including the
loaded lib symbols merged into the file binder, then uses global lib contexts as
a fallback. TypeScript 7 has no per-file suggestion cap or core-vs-DOM filter, so
loaded DOM candidates such as `ParseNode -> ParentNode` participate normally.
Direct checker contexts whose binders have not merged their attached libs retain
a TYPE-only lib-context fallback scoped to core lib files (`lib.es*`,
`lib.scripthost`, `lib.decorators` — see `lib_context_is_core_typings`).
Production binders skip that duplicate scan because the merged visible-name
table already covers every loaded lib. Candidate scans are deferred until a diagnostic is emitted.
The visible candidate universe is memoized per `(lexical scope, meaning)`, and
the final suggestion per `(lexical scope, misspelled name, meaning)`.
`find_similar_property` collects accessible
property names through the query boundary (resolving primitives to their boxed
interface types, and enum/namespace members from binder exports), and never
suggests a public property for a `#private` access.

`get_lib_for_type_property` drives the TS2550 "change your target library"
suggestion. It is a flat `(type_name, prop_name) -> lib` table mirroring `tsc`'s
`getScriptTargetFeatures`. Its parity rules are explicit: instance and
constructor types stay *separate* (`Error` vs `ErrorConstructor`), there are no
catch-all arms (an unlisted property must fall through to TS2339/TS2551), and
types absent from `tsc`'s table are absent here.

## The JSDoc semantic surface

JSDoc is a parser-adjacent surface that nonetheless carries semantic diagnostics,
owned entirely by `jsdoc/` (new JSDoc work belongs here, not in `types/utilities/`).
`jsdoc/resolution::resolve_jsdoc_reference` is the one authoritative entry point
for resolving a JSDoc type name/expression to a `TypeId` (typedef lookup,
`import("module").Member` lookup, `@template` scope lookup, callback/typedef
reference resolution); all callers route through it rather than re-deriving the
chain.

`jsdoc/diagnostics.rs` owns the JSDoc diagnostic emission: TS8033 duplicate
`@type`, TS8021 missing type annotation, TS2304 base-type validation, TS2300
duplicate `@import`, TS1109 malformed `@import`, and `@satisfies` malformed/
duplicate detection. Because these come from comment text rather than AST nodes,
the spans are computed from comment offsets (`find_jsdoc_typedef_name_offset`),
and the corresponding spelling suggestion path is the node-free
`find_jsdoc_type_spelling_suggestion`. `check_jsdoc_typedef_name_conflicts` walks
every arena's comments cross-file to detect typedef-versus-value collisions,
emitting through the same `error`/`push_diagnostic` dedup machinery as the rest
of the checker.

## Recovery: distinguishing recovered `any` from declared `any`

A bare `TypeId::ANY` returned from a recovery path is indistinguishable from a
user-written `: any`. `recovery/mod.rs` makes recovery *typed and auditable*.
`RecoveryReason` is a closed enum naming each *family* of fallback (not a single
test case): `ThisUnresolvedClassOrObjectLiteralMember`,
`ClassConstructorTargetUnresolved`, `YieldOutsideGenerator`,
`YieldExpressionNoGeneratorContext`. The single named entry point
`CheckerContext::recover_any(node, reason)` records the `(NodeIndex, RecoveryReason)`
pair in a per-checker `RecoverySites` registry, emits a structured
`tracing::debug!` with a stable `trace_site` label (so filters like
`TSZ_LOG=tsz_checker::recovery=trace` survive migrations), and returns
`TypeId::ANY`.

This is precisely how the reporter and relation paths can later treat a recovered
`any` differently from a declared `any` *without inspecting printed type strings*:
`RecoverySites::get(node)` answers "did this node produce `any` through recovery?"
A node legitimately producing `any` through type evaluation is simply absent from
the registry. Recovery is the *opposite* of suppression: rather than emitting and
then dropping a diagnostic, it short-circuits the cascade at the source — e.g.
`ClassConstructorTargetUnresolved` returns `any` to suppress the chain of TS2571s
that would otherwise fire on every downstream member access, and
`YieldOutsideGenerator` returns `any` because the *parser* already emitted TS1163
so the expression checker must not double-report.

## Parser-recovery interplay

The diagnostic stream is partitioned by code range, classified in
`tsz-common/src/diagnostics/mod.rs`: `is_parser_grammar_diagnostic` covers
1000-1999 (syntactic/grammar errors), and `is_js_grammar_diagnostic` covers
8000-8999 (JS-grammar errors for `.js`/`.jsx`). The checker generally does not
re-emit a diagnostic the parser already produced; the recovery sentinels above
exist exactly so the checker can stay quiet where the parser has spoken (TS1163
from the parser → `YieldOutsideGenerator` recovery in the checker). When the
checker *does* emit a grammar-style diagnostic at a token the parser would have
flagged (for example anchoring a generator error on the `*` token), it scans the
source text backward for the token (`emit_generator_error_at_asterisk`) because
the AST stores `asterisk_token` as a `bool`, not a node.

## Caches and invariants

- **`emitted` dedup set** (`diagnostic_indices.emitted: FxHashSet<(u32, u32)>`):
  keyed by `diagnostic_dedup_key`. Invalidation: rebuilt by
  `rebuild_emitted_diagnostics_from_current` after a speculative `retain`, which
  also re-syncs the TS2454 position set and the aux indices.
- **Aux position indices**: excess-property positions and overlapping-TS2322
  message index, updated by `update_aux_for` on every push and consulted by
  `push_diagnostic`'s TS2322 overlap suppression.
- **`RecoverySites`** (per-checker, `recovery/mod.rs`): records the *last* reason
  per node; re-evaluation must agree on the reason (divergence is itself a checker
  bug). Cleared per file via `clear`.
- **`DisplayBudgetScope` eval memo** (`display_budget.rs`): scoped per outermost
  rendered type; reset on scope exit; never records truncated results.
- **Determinism invariant**: the final reported order is fixed by
  `Diagnostic::compare`. Collision reconciliation (`prefers_candidate_diagnostic`)
  and the related-info `depth`-first sort exist solely to remove the last sources
  of production-order dependence, so the same program yields the same diagnostic
  stream regardless of solver memoization or parallelism order.

## Edge cases and tsc parity

- **Chain ordering is depth-keyed, not text-keyed.** `"Type "` sorts before
  `"Types"` alphabetically, so the depth key in `normalize_related_information`
  is what keeps a `Types of property...` header above its `Type 'X'...` leaf.
- **`ReturnTypeMismatch` skips the intermediate frame.** `tsc` never emits a
  "Return type 'X' is not assignable to 'Y'." line; the reporter emits only the
  inner mismatch (verified against zero baseline matches), then recurses.
- **Container reasons must delegate, not drop.** Tuple/array/type-argument and
  union/intersection-target argument mismatches reuse the TS2322 renderer via
  `reanchored_container_related`; without it they would hit `_ => return None`
  and lose their whole elaboration on the TS2345 surface.
- **Wrapper and intersection targets drop the missing-property note.**
  `Boolean`/`Number`/`String`/`Object` and intersection targets fall back to the
  argument-level message, matching `tsc`.
- **Plain assertions don't elaborate; `as const`/`satisfies` do.** The freshness
  boundary mirrors `getRegularTypeOfObjectLiteral`.
- **Anchor normalization trims to the name token.** Declarations anchor on the
  identifier (excluding modifiers / `...`), and a parameter default anchors on
  the parameter, not the function's `(`.
- **Recovered `any` is auditable, declared `any` is not in the registry** — the
  distinction is structural (a registry membership test), never a string check.
- **Pathological generic displays truncate gracefully** rather than hang, via the
  display visit/eval budgets and the depth-100 recursion guard, reproducing
  `tsc`'s `...` elision without changing any realistic output.

## See also

- [checker-assignability-gateway](checker-assignability-gateway.md) — the
  relation → structured reason → diagnostic gateway that feeds
  `related_from_failure_reason`.
- [solver-relations](solver-relations.md) — where `SubtypeFailureReason` is
  produced.
- [checker-context-and-state](checker-context-and-state.md) — `CheckerContext` /
  `CheckerState`, the diagnostic buffer, and snapshot/rollback.
- [checker-calls-signatures-generics](checker-calls-signatures-generics.md) —
  call checking that drives TS2345/TS2769 elaboration.
- [front-end-scanner-parser](front-end-scanner-parser.md) — parser grammar
  diagnostics (1000-1999) that the checker defers to.
- [end-to-end-timeline](end-to-end-timeline.md) — where diagnostic emission sits
  in the overall pipeline.
