# Flow Graph Construction and Narrowing Orchestration in the Checker

The checker is the layer that knows *where* a value is read in the program and
*what syntactic facts* control flow established on the way there. It does not
know the type algebra of narrowing. When the checker needs the narrowed type of
a reference at a program point, it walks the control-flow graph the binder
already built, extracts an AST-free *observation* at each guard it crosses, and
hands that observation to the solver, which owns every type-level operation
(union/intersection, member filtering, discriminant reduction, `typeof`
filtering). The boundary is sharp: the file `flow.rs` under
`query_boundaries` even names it — "checker extracts syntactic observations from
the CFG/AST; solver owns the semantic narrowing result."

The heart of this subsystem is `FlowAnalyzer` (in
`crates/tsz-checker/src/flow/control_flow/core.rs`), a per-query object that
performs an iterative backward worklist traversal of the binder's
`FlowNodeArena`. Its two public entry points are `get_flow_type` and the
internal `check_flow` traversal. Everything else in the `flow/control_flow`
tree is either reference matching (does this flow node touch the reference we
care about?), condition decoding (what guard is this `if`?), or the worklist
plumbing (defer, splice, cache, fixed-point loops) that keeps a fundamentally
`O(N^2)`-shaped problem closer to linear on real code.

This document covers: how the flow graph is built (and by whom), the data the
checker consumes, how `check_flow` traverses it, exactly where it stops being
checker work and becomes solver work, the caches and their invariants, and the
`tsc`-parity edge cases the implementation pins down.

---

## Owns / Must not own

The flow subsystem in the checker spans two responsibilities that are easy to
conflate, so state them precisely.

**Flow orchestration (this subsystem) owns:**

- Consuming the binder's flow graph (`binder.flow_nodes`, `binder.node_flow`)
  and walking it backward from a reference.
- Deciding *which* flow node is relevant to a reference
  (`is_matching_reference`, `assignment_targets_reference_node`,
  `switch_can_affect_reference`).
- Decoding a condition AST into an AST-free observation (`extract_type_guard` →
  `TypeGuard`, `narrow_by_typeof_result`'s `TypeofKind`, discriminant property
  paths).
- Worklist scheduling: merge-point readiness, deferral to antecedents that
  carry narrowing, loop fixed-point iteration, the linear pass-through splice,
  and all flow-walk caches.
- Re-entrancy and termination guards (`flow_query_depth`, `flow_step_budget`,
  `MAX_FLOW_QUERY_DEPTH`).

**It must not own:**

- The narrowing type algebra. Filtering a union by `typeof`, intersecting a
  guard type, reducing a discriminated union, removing nullish constituents —
  all of that is `tsz_solver::narrowing::NarrowingContext` and the
  `query_boundaries::flow_analysis` / `query_boundaries::flow` wrappers.
- Constructing raw solver types. The checker calls `query::union_types`,
  `self.interner.union_preserve_members`, `query::array_type`,
  `flow_boundary::narrow_optional_chain`, etc.; it never interns `TypeData`
  directly.
- Reading printer output to make a flow decision (forbidden by the
  anti-hardcoding gate).

---

## Who actually builds the flow graph

There is a subtle but load-bearing fact here: the flow graph that narrowing
walks is built by the **binder**, not the checker.

| Concern | Module | Role |
| --- | --- | --- |
| Flow node type + arena | `crates/tsz-binder/src/flow.rs` | `FlowNode`, `FlowNodeId`, `FlowNodeArena`, `flow_flags` |
| Flow node allocation | `crates/tsz-binder/src/state/flow_helpers.rs` | `create_flow_condition`, `create_flow_assignment`, branch/loop labels, `add_antecedent`, `finish_flow_label` |
| Statement flow wiring | `crates/tsz-binder/src/nodes/flow_statements.rs` | `bind_if_statement`, `bind_while_or_do_statement`, for/switch/try flow |
| Per-node flow map | `crates/tsz-binder/src/state/mod.rs` | `BinderState::node_flow: Arc<FxHashMap<u32, FlowNodeId>>` |

`FlowNode` is intentionally tiny: `flags: u32`, `id: FlowNodeId`,
`antecedent: Vec<FlowNodeId>`, and `node: NodeIndex`. The flow graph is a graph
of antecedents (predecessors); narrowing walks it *backward* from a usage to the
declaration. The flags are a direct port of TypeScript's `FlowFlags`:
`START (1<<1)`, `BRANCH_LABEL (1<<2)`, `LOOP_LABEL (1<<3)`,
`ASSIGNMENT (1<<4)`, `TRUE_CONDITION (1<<5)`, `FALSE_CONDITION (1<<6)`,
`SWITCH_CLAUSE (1<<7)`, `ARRAY_MUTATION (1<<8)`, `CALL (1<<9)`,
`AWAIT_POINT (1<<12)`, `YIELD_POINT (1<<13)`, with the composite
`CONDITION = TRUE_CONDITION | FALSE_CONDITION` and `LABEL = BRANCH_LABEL |
LOOP_LABEL`. This matches the binder/checker split mandated by the architecture:
the binder produces a syntax-level skeleton (symbols, scopes, hoisting, flow)
and computes **no** types.

### The checker's own `FlowGraphBuilder` is a separate side-table

Confusingly, the checker *also* has a `flow/flow_graph_builder/` module with a
`FlowGraphBuilder` and its own `FlowGraph` side-table
(`crates/tsz-checker/src/flow/flow_graph_builder/core.rs`). This is **not** the
graph that narrowing consumes. It is a parallel construction used by the
checker's reachability analysis (`flow/reachability_checker.rs`,
`flow/reachability_analyzer.rs`) and tests; it builds its `FlowGraph` from the
`NodeArena` post-binding without mutating the AST. Narrowing, by contrast, reads
`self.binder.flow_nodes` directly via `FlowAnalyzer`. When this document says
"the flow graph," it means the binder's `FlowNodeArena` unless it explicitly
names `FlowGraphBuilder`.

The checker also has a `control_flow::FlowGraph<'a>` (in `core.rs`) which is a
read-only query wrapper around `&FlowNodeArena` (`get`, `antecedents`, `node`)
— again a view, not a second graph.

---

## What the checker consumes

A `FlowAnalyzer` (`crates/tsz-checker/src/flow/control_flow/core.rs`) borrows:

- `arena: &NodeArena` — the AST, to decode conditions and assignments.
- `binder: &BinderState` — `flow_nodes` (the graph), `node_flow`
  (`NodeIndex` → `FlowNodeId`), `node_symbols`, and symbol resolution.
- `interner: &dyn QueryDatabase` — the solver-facing handle for all type
  construction/queries.
- `type_environment: Option<&RefCell<TypeEnvironment>>` — resolves
  `TypeData::Lazy(DefId)` semantic refs during narrowing.
- `node_types: Option<&NodeTypeCache>` — already-computed expression types
  (e.g. a call's return type, an assignment RHS type).
- `shared: Option<&FlowSharedCaches>` — the context-owned cache bundle.

The single production construction path is `FlowAnalyzer::from_ctx(ctx)`, which
wires the shared cache bundle, the `TypeEnvironment`, the checker context, the
destructured-binding map, and the enclosing-class `this` type all at once, so no
launch site can wire a partial subset. Isolated unit-test analyzers built with
`FlowAnalyzer::new` run uncached.

Callers reach the analyzer through context helpers — for example
`flow_analyzer()` in `types/computation/identifier_flow.rs` returns
`FlowAnalyzer::from_ctx(&self.ctx)`, and property reads in
`types/property_access_type/resolve.rs` and `types/computation/access.rs` call
`flow_analyzer_for_property_reads().get_flow_type(...)`. The `typeof` type-query
node handler (`types/type_node_advanced.rs`) also narrows its operand this way.
A reference's flow node is located via `binder.get_node_flow(idx)`, falling back
to the nearest ancestor with a recorded flow (see
`flow_node_for_identifier_usage`).

---

## Entry point: `get_flow_type`

`get_flow_type` (in `flow/control_flow/core/flow_query.rs`) is the boundary the
rest of the checker calls. It layers three concerns on top of the raw traversal:

```text
get_flow_type(reference, initial_type, flow_node)
  |
  |-- if initial_type == ERROR: return ERROR   (never narrow a suppressed error
  |                                              into a concrete false positive)
  |
  |-- get_flow_type_uncorrelated
  |     |-- depth guard: if flow_query_depth >= MAX_FLOW_QUERY_DEPTH (2000)
  |     |     return initial_type   (mirrors tsc's flowDepth ceiling)
  |     |-- flow_query_depth += 1
  |     |-- get_flow_type_uncorrelated_inner
  |     |     |-- resolve symbol_id (plain ident / this / super only;
  |     |     |     member-like refs key by structural path, not member symbol)
  |     |     |-- non-narrowable short-circuit: a member access whose receiver
  |     |     |     chain bottoms out at a call result has no storage root,
  |     |     |     so return initial_type (skips O(N^2) per-antecedent matching)
  |     |     `-- check_flow(...)
  |     `-- flow_query_depth -= 1
  |
  `-- apply_correlated_destructured_narrowing
        (refine a const destructured binding by what its sibling bindings'
         narrowing implies about the shared source union)
```

Two guards here are direct `tsc` parity points. The `ERROR` short-circuit
exists because condition handlers like `== null` can synthesize
`null | undefined` regardless of input, which would turn a suppressed error into
a fresh diagnostic. The `MAX_FLOW_QUERY_DEPTH = 2000` ceiling mirrors `tsc`'s
`flowDepth` guard in `getFlowTypeOfReference`: narrowing one reference can
require the flow type of *another* (an aliased condition, an optional-chain
guard), so `get_flow_type → check_flow → get_flow_type` re-enters; without a
ceiling, deeply nested narrowing in large modules (the `effect` canary) would
overflow the native stack. The bound returns the un-narrowed declared type, just
as `tsc` does.

The correlated-destructured step is the one piece of "narrowing" the entry point
adds that the backward walk cannot: given `const { kind, payload } = u` where
`u` is a discriminated union, narrowing `kind` tells you which union members
survive, which constrains `payload`. It re-runs the *uncorrelated* flow query on
each sibling binding, filters the source union members by overlap, then
re-derives this binding's type from the surviving members and intersects.

---

## The traversal: `check_flow`

`check_flow` (in `flow/control_flow/core/flow_traversal.rs`) is the iterative
worklist that replaced a recursive walk to avoid stack overflow on deep CFGs. It
is `~1200` lines and worth understanding as a state machine.

### Setup

- It borrows reusable buffers from `FlowSharedCaches` when available
  (`flow_worklist`, `flow_in_worklist`, `flow_visited`, `flow_results`), using
  `try_borrow_mut` so re-entrant calls fall back to local buffers safely. This
  is why bidirectional narrowing (a flow query inside a flow query) does not
  panic on the shared `RefCell`s.
- It computes `initial_has_type_params` **once** (via
  `contains_type_parameters_cached`): generic types must never be cached across
  instantiations.
- It selects a `cache_symbol` from three disjoint spaces: a real binder
  `SymbolId` (bit 31 clear); else a structural-path key from
  `flow_reference_path_symbol` (bit 31 set — shared across occurrences of `a.b`);
  else a `per_node_flow_cache_symbol` fallback for non-pathy references like
  `f().x`. These partitions (`structural_flow_cache_symbol`,
  `FLOW_CACHE_SYNTHETIC_BIT`, `FLOW_CACHE_PER_NODE_BIT`) guarantee distinct
  references can never alias in the flow cache.

### Per-node dispatch

The worklist pops `(flow_node, current_type)` pairs and dispatches on flags:

| Flag | Handling |
| --- | --- |
| `BRANCH_LABEL` | Merge point: schedule all antecedents, then union their finalized results (`union_preserve_members`); filter `UNREACHABLE_NEVER` dead branches |
| `LOOP_LABEL` | `analyze_loop_fixed_point`: union entry type with back-edge types, up to 5 iterations, with cache injection to break recursion |
| `CONDITION` (`TRUE`/`FALSE`) | `narrow_type_by_condition_with_dp_memos(pre_type, flow.node, reference, is_true_branch, ...)` |
| `SWITCH_CLAUSE` | `handle_switch_clause_iterative`: per-clause discriminant / `switch(true)` / default narrowing |
| `ASSIGNMENT` | Killing-definition narrowing if it targets the reference; pass-through otherwise |
| `ARRAY_MUTATION` | Evolving-array element accretion (`push`/`unshift`) |
| `CALL` | `handle_call_iterative`: assertion predicates and never-returning calls |
| `START` | Closure boundary: reset narrowing for captured-mutable and member-like refs |
| `AWAIT_POINT` / `YIELD_POINT` | Pure pass-through: resolve the single antecedent's narrowed type |

The crucial structural rule for *merge points* (branch labels, loop headers,
switch fallthrough, and `CALL` nodes — which need their antecedent for assertion
functions) is the readiness check: if any antecedent has not been finalized, the
node reschedules its antecedents to the front of the worklist and re-enqueues
itself at the back, then `continue`s. This guarantees a node never narrows from a
stale `current_type` when an else-if chain, a loop header, or an assertion call
upstream still has pending narrowing.

### Deferral

Conditions and assignments that are *not* themselves merge points still must not
finalize before an antecedent that carries narrowing. `antecedent_requires_defer`
(and the condition-specific `condition_antecedent_requires_defer_cached`) decide
this. An antecedent forces a defer when it is a `CONDITION`, a `LOOP_LABEL`, a
`BRANCH_LABEL`, an assignment that targets this reference, a `CALL` that can
narrow or divert (a never-returning call or an `asserts` predicate, via
`call_node_may_narrow_or_divert`), or an `await`/`yield` whose own antecedent
needs a defer. A pure pass-through call whose antecedent needs no defer must
**not** force a defer — otherwise interleaved const/call dispatch-table chains
would scale `O(N^2)`. `defer_to_antecedent` pushes the antecedent to the front
and re-pushes the current node to the back.

### The linear pass-through splice

The single biggest performance lever in `check_flow` is
`chase_linear_passthrough`. A straight-line run of `ASSIGNMENT` nodes
(`const x_1 = ...; const x_2 = ...; ...`) that neither target nor affect the
reference being narrowed carries no narrowing — each node's flow type equals its
antecedent's. The naive worklist still pops, cache-probes, and re-pushes every
one of them, which is `Σ O(i)` per reference and `O(N^2)` over a scope. The chase
collapses such a run in `O(1)` per node by chasing the single antecedent in
place and landing on the first node that is *not* a pure pass-through. It only
splices a node when **all** gates hold (pure `ASSIGNMENT` flag with no
merge/condition/call/loop/switch/array-mutation/start flag; exactly one
antecedent; neither targets nor affects the reference; antecedent does not
require a defer; antecedent not already finalized). The walk is disabled for
type-parameter-bearing, control-flow-`any`, `ANY`, `ERROR`, and `UNKNOWN`
initial types, mirroring the cache-eligibility gate, so it cannot perturb loop
fixed-point or generic-result caching. If the queried `flow_id` itself gets
spliced out, the chase records `flow_id_landed_on` so the landed node's result
is aliased to `flow_id` once it finalizes. Only `flow_id` is aliased — interior
spliced nodes are left untracked so a surviving merge can re-derive them
independently (the `jsxComplexSignature` family depends on this).

---

## Loops: `analyze_loop_fixed_point`

Loop headers (`LOOP_LABEL`) need a fixed point because a variable's type at the
top of a loop depends on its type at the bottom (the back-edge).
`analyze_loop_fixed_point` (in `core.rs`):

1. For a `const` symbol, returns the entry type immediately (cannot be
   reassigned).
2. When `symbol_id` is `None` (e.g. `fns.length`, a member access whose base
   symbol is tracked separately), returns the entry type — the property
   expression is never reassigned inside the loop, and without a symbol there is
   no cache key to break the recursion cycle.
3. With only one antecedent (no back-edge), returns the entry type.
4. Otherwise iterates at most `MAX_ITERATIONS = 5`: each round injects the
   current assumption into the flow cache under the loop header key (and a second
   key for the inner back-edge traversal) to break the
   `get_flow_type → check_flow → LOOP_LABEL → analyze_loop_fixed_point`
   recursion, then unions the entry type with `get_flow_type` of every back-edge.
   When the type stabilizes it updates the cache with the converged result; if it
   does not converge in 5 rounds it widens conservatively to
   `union(entry_type, initial_type)`.

The cache injection is the mechanism that makes recursive loop analysis
terminate; it is also why `LOOP_LABEL` nodes must *always* consult the flow cache
(the worklist's cache gate special-cases `is_loop_label_node`), even when the
initial type contains type parameters.

---

## The checker → solver narrowing boundary

This is the architecturally important part: **the checker decides what was
observed; the solver computes the narrowed type.** The handoff happens through
two layers under `query_boundaries`.

### Layer 1: observation types (AST-free)

The solver defines AST-free observation types in
`crates/tsz-solver/src/narrowing/core.rs`:

- `TypeofKind` — the 8 standard `typeof` results (`String`, `Number`, `Boolean`,
  `BigInt`, `Symbol`, `Undefined`, `Object`, `Function`), parsed from the
  `typeof` comparison string with `TypeofKind::parse`. Using an enum avoids a
  heap allocation per guard.
- `TypeGuard` — every narrowing condition shape, with **no** `NodeIndex` or
  `SyntaxKind`: `Typeof`, `Instanceof(TypeId, bool)`, `LiteralEquality(TypeId)`,
  `NullishEquality`, `Truthy`, `Discriminant { property_path: Vec<Atom>,
  value_type }`, `InProperty(Atom)`, `Predicate { type_id, asserts }`, `Array`,
  `ArrayElementPredicate`, `Constructor(TypeId)`, and more.

The checker decodes a condition AST into one of these. `extract_type_guard` (in
`flow/control_flow/type_guards.rs`) is the principal decoder: given a condition
`NodeIndex`, it returns `Option<(TypeGuard, NodeIndex, bool)>` — the guard, the
target reference node, and whether the chain was optional. It recognizes call
guards (user predicates, `Array.isArray`), `instanceof`, assignment-wrapped
conditions, and the various equality/`typeof` shapes.

### Layer 2: the narrowing wrappers

The checker never calls solver narrowing kernels directly. It calls thin
wrappers in `query_boundaries::flow_analysis` (aliased `flow_query`) and
`query_boundaries::flow` (aliased `flow_boundary`), which forward to the solver's
`NarrowingContext`:

| Checker call site | Boundary function | Solver work |
| --- | --- | --- |
| truthy/falsy condition | `flow_query::narrow_to_falsy`, `narrow_to_truthy_in_context` | remove/keep falsy constituents |
| `typeof x === "s"` | `flow_query::narrow_by_typeof_result` | filter union by `TypeofKind` |
| user type guard | `flow_query::narrow_with_guard_in_context` | apply a `TypeGuard` |
| discriminant | `flow_query::narrow_by_discriminant_in_context`, `narrow_by_discriminant_for_type_in_context` | reduce a discriminated union |
| literal `===`/`!==` | `flow_query::narrow_to_type_in_context`, `narrow_excluding_type(s)_in_context` | include/exclude literal members |
| predicate function | `flow_query::narrow_type_predicate` | intersect/narrow to predicate type |
| optional chain truthy | `flow_boundary::narrow_optional_chain`, `narrow_non_nullish` | strip nullish from base |

The decoding entry point is `narrow_type_by_condition` (in
`condition_narrowing_entrypoints.rs`), which initializes an `AliasCycleTracker`
and dispatches to `narrow_type_by_condition_inner` (in `condition_narrowing.rs`).
That inner function pattern-matches the condition AST (binary expression,
`typeof`, truthiness, `in`, logical `&&`/`||`) and, for each shape, calls the
appropriate `flow_query::narrow_*_in_context` wrapper — never type algebra
inline. For example the truthy case calls `narrow_to_falsy_via_flow_boundary`,
which is one line: `flow_query::narrow_to_falsy(self.interner, env, type_id)`.

`NarrowingContext` itself lives in `tsz_solver::narrowing::core` and is created
through `make_narrowing_context()` (which reuses the shared `NarrowingCache` to
avoid 7 `FxHashMap` allocations per narrowing op) and wired with
`.with_resolver(&*env_borrow)` so the solver can resolve `Lazy(DefId)` types
during narrowing. The `query_boundaries::flow` module additionally models a
small `FlowObservation` enum (`DestructuringElement`, `CatchVariable`,
`OptionalChainNonNullish`, `NonNullish`, `ForOfElement`, `TruthyNarrow`,
`CatchVariableTypeofReset`, ...) consumed by `apply_flow_observation`, the single
entry point for non-condition observations.

### A concrete walk-through

Consider:

```typescript
function f(x: string | number) {
  if (typeof x === "string") {
    x.length; // x is string here
  }
}
```

1. The binder binds the `if`, creating a `TRUE_CONDITION` flow node whose `node`
   field is the `typeof x === "string"` expression and whose antecedent is the
   pre-condition flow (`bind_if_statement` in
   `tsz-binder/.../nodes/flow_statements.rs`).
2. Checking `x.length` resolves `x`'s declared type to `string | number` and
   finds the flow node active at that read via `binder.get_node_flow`.
3. The checker calls `get_flow_type(x_ref, string|number, flow_node)` →
   `check_flow`.
4. The worklist pops the `TRUE_CONDITION` node. It has the `CONDITION` flag, so
   `check_flow` reads the antecedent's already-computed type (`string | number`)
   and calls `narrow_type_by_condition_with_dp_memos(string|number,
   typeof-expr-node, x_ref, is_true_branch=true, ...)`.
5. `narrow_type_by_condition_inner` decodes the binary expression: left is
   `typeof x`, operator is `===`, right is `"string"`. It confirms the target
   matches `x` via `is_matching_reference`, parses `"string"` into
   `TypeofKind::String`, and calls
   `narrow_by_typeof_result_via_flow_boundary(string|number, "string", true)`.
6. That wrapper calls `flow_query::narrow_by_typeof_result`, which is **solver**
   code: it filters the union, keeps `string`, and returns `TypeId` for
   `string`.
7. `check_flow` finalizes the node with `string`, caches it under
   `(flow_node, x_symbol, string|number)`, and `get_flow_type` returns `string`.

Note steps 1–5 are entirely checker (where + what-shape); steps 6 is entirely
solver (the actual filtering). No union member set is ever computed in the
checker.

---

## Switch narrowing

Switches are the most elaborate flow shape. The binder emits `SWITCH_CLAUSE`
flow nodes per case (with the pre-switch antecedent plus any fallthrough
antecedent). `handle_switch_clause_iterative` (in `core.rs`):

- Resolves the owning switch via `binder.get_switch_for_clause`, or treats a
  clause whose `node` is the `CASE_BLOCK` itself as the *implicit default* (the
  "no case matched" path the builder synthesizes when there is no explicit
  `default`).
- Computes the `base_type` by unioning the pre-switch type with fallthrough
  antecedent results, but preserves the pre-switch *identity* (alias/display
  metadata) when the merge expands back to the same member set
  (`same_union_member_set`).
- Fast-paths out via `switch_can_affect_reference` (cached in
  `flow_switch_reference_cache`) when the switch cannot narrow this reference at
  all — covering direct match, discriminant path, `typeof` target, optional
  chain, and aliased discriminant (`const kind = obj.kind; switch (kind)`).
- Dispatches: implicit-default and default clauses → `narrow_by_default_switch_clause`
  (exclude all case literal types); `switch (true)` → `narrow_by_switch_true_case_clause`
  (each case is an independent guard requiring prior cases false); ordinary
  cases → `narrow_by_switch_case_clause`, which synthesizes an `x === caseLabel`
  comparison and routes through `narrow_by_binary_expr`.

`switch (true)` is special-cased throughout (`is_switch_true`) because each case
expression acts as an independent boolean guard rather than a comparison against
the discriminant.

---

## Assignments, calls, evolving arrays

**Assignments** (`ASSIGNMENT`): when an assignment targets the reference it is a
"killing definition" — the type becomes the RHS type and the walk stops (the
prior narrowing no longer applies). `get_assigned_type` reads the RHS type from
`node_types`; if it is `ERROR` (the RHS not yet checked, common during loop
fixed-point), the walk is marked `cacheable_walk = false` so a stale declared
type is never published. Crucial parity carve-outs: `any` absorbs assignments
(stays `any`); `ERROR` persists; `unknown` *is* narrowed by assignments
(`let x: unknown; x = 123` narrows to `number`) except for catch variables;
const-destructuring assignments keep the full declaration-time union; logical
assignments (`??=`, `||=`, `&&=`) take the RHS branch type directly; property/
element-access reads preserve their declared read surface for generic/callable
members. Narrowing always uses the **declared** type as the base
(`getAssignmentReducedType` parity), not an already-narrowed loop type.

**Calls** (`CALL`): `handle_call_iterative` only changes the type in two cases —
a never-returning call (`never` return) diverts the branch to the internal
`UNREACHABLE_NEVER` sentinel (`TypeId(98)`, distinct from a legitimate
exhaustive `NEVER`), or an `asserts` type-predicate call narrows the target. It
resolves the predicate (preferring solver-instantiated predicates from
`call_type_predicates` over the raw callee signature), handles negated guards
(`assert(!isFoo(x))`), optional-chain transport (`assertNonNull(o?.foo)` implies
the prefix is non-nullish), discriminant assertions, and condition-style
assertions (`assert(typeof x === "string")`). `UNREACHABLE_NEVER` is filtered out
at `BRANCH_LABEL` merges and mapped back to the declared type at the final
return, matching `tsc`'s `unreachableNeverType` vs `neverType` distinction so
unreachable code does not produce false `TS2339` property errors.

**Evolving arrays** (`ARRAY_MUTATION`): for an implicitly-typed `let a = []`,
`array_mutation_evolved_type` accretes `push`/`unshift` argument types into the
element type. It only applies to a `reference_is_evolving_array_symbol` (an
unannotated `[]` initializer or a control-flow-typed `any` symbol); incomplete
argument typing marks the walk non-cacheable.

---

## Caches and invariants

All flow caches live in `FlowSharedCaches` (in `context/mod.rs`), owned by the
`CheckerContext` and wired whole into the analyzer.

| Cache | Key → value | Invariant / invalidation |
| --- | --- | --- |
| `flow_analysis_cache` | `(FlowNodeId, SymbolId, TypeId)` → `TypeId` | Only written when `cacheable_walk` and neither initial nor final type has type params; structural-path entries dropped on incremental save (`is_session_stable_flow_cache_symbol`) |
| `flow_reference_keys` | structural path `Vec<u32>` → interned id | Append-only, rebuildable; gives `a.b` an occurrence-stable cache symbol |
| `flow_worklist`/`flow_in_worklist`/`flow_visited`/`flow_results` | reusable buffers | Cleared at the top of each `check_flow`; `try_borrow_mut` falls back to locals under re-entrancy |
| `narrowing_cache` | solver `NarrowingCache` | Shared across passes to keep CFA chains off `O(N^2)`; also backs `contains_type_parameters_cache` |
| `flow_switch_reference_cache` | `(switch_expr, ref)` → bool | Pure function of immutable post-bind AST |
| `flow_switch_case_literal_cache` | clause-expr `NodeIndex` → `Option<TypeId>` | Makes an N-arm literal switch `O(N)` instead of `O(N^2)` interns |
| `flow_switch_all_distinct_literals_cache` | case-block → bool | Collapses per-clause predecessor scans to one `O(N)` pass |
| `flow_reference_match_cache` | `(min, max)` node ids → bool | Symmetric key; avoids `O(N^2)` `is_matching_reference` |
| `symbol_flow_memo` | symbol-stable memos (last-assignment pos, nested-closure assignment, first-identifier ref, alias-base/path assignment) | Keyed by `SymbolId`; stable for a file check |
| `call_type_predicates` | call `NodeIndex` → instantiated predicate | Generic predicates with inferred type args applied |

Key invariants:

- **Cache key disjointness.** The `SymbolId` slot of a flow-cache key carries one
  of three disjoint kinds (real binder symbol / structural path / per-node
  fallback) so distinct references never alias. `this`/`super` use reserved base
  components (`FLOW_CACHE_THIS_BASE_KEY`, `FLOW_CACHE_SUPER_BASE_KEY`).
- **No generic caching.** `initial_has_type_params` is computed once; a walk over
  a type-parameter-bearing type is never cached (except the loop-header recursion
  guard), preventing the "generic result" bug where narrowing leaks type
  parameters across instantiations.
- **Provisional walks are non-cacheable.** When an RHS type is not yet computed
  (`ERROR` from a not-yet-checked expression, an incomplete evolving array, a
  self-referential assignment), `cacheable_walk = false` and `pending_cache_writes`
  are dropped, so a stale declared type is never published.
- **Per-walk memos are reference-scoped.** `FlowDeferMemos` and
  `FlowConditionDpMemos` are created fresh per `check_flow` and keyed purely by
  `FlowNodeId`; this is sound *only* because `reference`/`symbol_id` are constant
  for the walk. Sharing them across walks would serve stale verdicts.
- **`UNKNOWN` skips the cache through switch/typeof chains.** When the initial
  type is `unknown` and the chain passes a switch clause
  (`flow_chain_contains_switch_clause_with_memo`) or exhaustive `typeof`
  exclusions (`flow_has_exhaustive_typeof_exclusions_with_memo`), the cache is
  bypassed so dedicated narrowing always runs.

---

## Termination guards

| Guard | Location | Purpose |
| --- | --- | --- |
| `flow_step_budget(n)` | `core.rs` | Per-walk worklist step cap: `clamp(12·n, 10_000, 40_000)`. On overflow `check_flow` bails to the best result so far (conservative) to bound pathological CFGs |
| `MAX_FLOW_QUERY_DEPTH = 2000` | `flow_query.rs` | Bounds re-entrant `get_flow_type` nesting; mirrors `tsc`'s `flowDepth` |
| `flow_query_depth: Cell<u32>` | `core.rs` | Tracks current re-entry depth |
| `MAX_ITERATIONS = 5` | `analyze_loop_fixed_point` | Loop fixed-point round cap; widens on non-convergence |
| `MAX_FLOW_ANALYSIS_ITERATIONS = 100_000` | `flow_analyzer.rs` | Definite-assignment worklist cap (separate analysis) |
| `resolve_chain_reachability` memo | `flow_dp.rs` | Exact memoized backward reachability for switch/typeof chain queries; only proven verdicts are memoized |

The `flow_step_budget` constants are empirically tuned: a linear flow graph with
branch conditions can visit `O(N^2)` total worklist steps because condition nodes
defer to antecedents and re-enqueue; the minimum floor of 10,000 keeps small-to-
medium files exact while the scale and ceiling bound large files.

---

## Edge cases and `tsc` parity

- **Closure boundaries (`START`).** At a function-start flow node, a captured
  *mutable* variable that is actually reassigned (`is_captured_variable` &&
  `!is_effectively_const_for_narrowing`) resets to its declared type — the
  closure may run after the mutation. Const and effectively-const captures
  preserve outer narrowing (`tsc`'s implicit-const-parameter feature).
  Member-like references (`a.b`) and class-property-initializer captures also
  reset, mirroring `tsc`'s `PropertyAccessExpression`/`ElementAccessExpression`
  exclusion in `getTypeAtFlowNode`'s `FlowStart` handling.
- **`await`/`yield` are not narrowing nodes.** `tsc` does not model suspension
  points as flow nodes; here they are pure pass-throughs whose flow type is
  their antecedent's, but they *do* force a defer when their own antecedent
  carries narrowing (e.g. `if (x.isErr()) return; await p; x.value`).
- **Unreachable-never vs never.** A never-returning call yields the internal
  `UNREACHABLE_NEVER` sentinel, mapped back to the declared type at the boundary,
  so unreachable code keeps its declared type and does not emit false `TS2339`.
  A genuine exhaustive `never` (from typeof/discriminant exhaustion) is
  preserved.
- **Exhaustive `typeof` over `unknown`.** When `unknown` is exhaustively
  excluded by `typeof` guards, the merge yields `empty_object_type` rather than
  `never`, matching `tsc`.
- **Switch alias identity.** Fallthrough merges that re-expand to the original
  union preserve the pre-switch type identity (named alias display like
  `MyType`) via `same_union_member_set`, keeping diagnostics stable.
- **`==`/`!=` null matches both null and undefined.** Encoded as
  `TypeGuard::NullishEquality`, applied by the solver.
- **Non-narrowable references short-circuit.** A member access rooted at a call
  result (`readIndexed('p').a.b`) has no storage root; the walk is skipped and
  the declared type returned, byte-identical to walking but `O(N)` instead of
  `O(N^2)`.
- **Flow merges use identity dedup only.** `simplify_flow_merge_types` dedups by
  `TypeId` identity, never by structural assignability, because subtype reduction
  would collapse distinct class types that share an interface and lose narrowing
  facts. The solver's `union()` applies appropriate subtype reduction when the
  union is finally constructed.

---

## Cross-references

- [checker-context-and-state](checker-context-and-state.md) — `CheckerContext`,
  `FlowSharedCaches`, and how the analyzer is wired via `from_ctx`.
- [solver-narrowing](solver-narrowing.md) — `NarrowingContext`, `TypeGuard`,
  `TypeofKind`, and the narrowing kernels this subsystem delegates to.
- [checker-assignability-gateway](checker-assignability-gateway.md) — the
  `query_boundaries/assignability` gateway flow narrowing borrows for
  relatedness checks (`flow_assignability_outcome`).
- [solver-types-intern-def](solver-types-intern-def.md) — `TypeId`,
  `TypeData::Lazy(DefId)`, and how `TypeEnvironment` resolves semantic refs.
- [solver-evaluation](solver-evaluation.md) — evaluation of discriminant/`keyof`
  shapes the narrowing wrappers depend on.
- [binder](binder.md) — how `FlowNode`, `FlowNodeArena`, and `node_flow` are
  built by the binder this subsystem consumes.
- [checker-calls-signatures-generics](checker-calls-signatures-generics.md) —
  how generic call resolution populates `call_type_predicates`.
- [front-end-scanner-parser](front-end-scanner-parser.md) — the AST arenas the
  condition decoders read.
- [end-to-end-timeline](end-to-end-timeline.md) — where flow narrowing sits in
  the overall check pass.
