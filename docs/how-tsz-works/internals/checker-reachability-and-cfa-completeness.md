# Reachability and Control-Flow Completeness

## Orientation

[checker-flow-and-narrowing](checker-flow-and-narrowing.md) is about *type*: it
reads the binder's `FlowNodeArena` to answer "what is the narrowed type of this
reference at this program point." It deliberately defers a different question —
"can control actually *reach* this point, and does every path that must produce
a value actually do so." That deferred question is **reachability and
control-flow completeness**, and it is what this document covers.

The two analyses share vocabulary ("flow graph", "antecedent", "unreachable")
but they are structurally distinct subsystems with different owners, different
inputs, and different diagnostics. Narrowing consumes the binder's
`tsz_binder::FlowNodeArena` through `FlowAnalyzer`. Reachability is computed two
ways: a **structural, syntax-driven walk** over the `NodeArena`
(`flow/reachability_checker.rs`, the dominant path that drives TS7027 / TS7029 /
TS2355 / TS2366 / TS7030) and a **side-table flow graph** built by
`FlowGraphBuilder` (`flow/flow_graph_builder/`) that marks unreachable AST nodes
during a post-binding traversal. Definite-assignment (TS2454) is a *third* leg —
a forward dataflow over the binder flow graph — covered here because it answers
"has every path reaching this use already assigned the variable," which is a
reachability-completeness property, not a narrowing one.

The sibling [checker-flow-and-narrowing](checker-flow-and-narrowing.md) already
draws the line precisely (its "The checker's own `FlowGraphBuilder` is a
separate side-table" section): narrowing reads `self.binder.flow_nodes`;
reachability uses `FlowGraphBuilder` plus the structural AST walk. This document
goes *into* that deferred half. It extends
[checker-context-and-state](checker-context-and-state.md) (the
`is_unreachable` / `has_reported_unreachable` state bits it relies on),
[checker-declarations-modules](checker-declarations-modules.md) and
[checker-calls-signatures-generics](checker-calls-signatures-generics.md) (which
own the function-return-completeness call site), and the
[binder](binder.md) (which lays down the `flow_flags::UNREACHABLE` sentinel and
the `FlowNode` skeleton this analysis reads).

---

## Owns / Must not own

| Owns | Must not own |
| --- | --- |
| Syntactic fall-through analysis of statements/blocks/switch/try/loops (`statement_falls_through`, `block_falls_through`, `switch_falls_through`, `try_falls_through`, `loop_falls_through`) | Type relations, inference, instantiation, evaluation kernels — those belong to the solver (see [solver-relations](solver-relations.md), [solver-evaluation](solver-evaluation.md)) |
| TS7027 unreachable-code reporting and the "one diagnostic per unreachable group" reset policy (`report_unreachable_statement`) | Narrowing of reference *types* across the flow graph — that is [checker-flow-and-narrowing](checker-flow-and-narrowing.md) / [solver-narrowing](solver-narrowing.md) |
| TS7029 fallthrough-case detection under `noFallthroughCasesInSwitch` | Computing the discriminant/case *types* — those come from `get_type_of_node*` and the solver |
| TS2355 / TS2366 / TS7030 function-return completeness orchestration (`check_function_return_completeness`) | Deciding *whether a type requires a return value* via raw type inspection — that is delegated to solver-backed queries (`requires_return_value`, `query::function_return_type`) |
| TS2454 definite-assignment dataflow over the binder flow graph (`DefiniteAssignmentAnalyzer`, `emit_definite_assignment_error`) | Constructing `TypeKey`s or pattern-matching solver internals to decide exhaustiveness — routed through `query_boundaries::flow_analysis` |
| Switch exhaustiveness *as a reachability input* (`switch_has_exhaustive_coverage`) | The exhaustiveness narrowing *math* (`narrow_excluding_types`, `cases_exhaust_type`) — that lives in the solver behind the query boundary |
| Recognizing never-returning calls syntactically (`call_expression_terminates_control_flow`, `callee_explicitly_returns_never`) | Fully type-checking the call expression to decide it returns `never` (would cache stale types in early phases) |

---

## Module map

| Path | Role |
| --- | --- |
| `crates/tsz-checker/src/flow/reachability_checker.rs` | The heart: syntactic fall-through / never-call / switch-coverage analysis as methods on `CheckerState`. No type kernels; asks the solver only through `query_boundaries::flow_analysis`. |
| `crates/tsz-checker/src/flow/reachability_analyzer.rs` | Thin read-only `ReachabilityAnalyzer` wrapper over a `FlowGraph` (`unreachable_count`, `has_unreachable_code`). Test-facing; not on the diagnostic path. |
| `crates/tsz-checker/src/flow/flow_graph_builder/core.rs` | `FlowGraphBuilder` + the side-table `FlowGraph` (`unreachable_nodes`, `unreachable_flow`, `mark_unreachable`, `create_flow_node`). Post-binding AST traversal that marks unreachable nodes. |
| `crates/tsz-checker/src/flow/flow_graph_builder/expressions.rs` | Expression-level flow construction for the side-table builder. |
| `crates/tsz-checker/src/statements.rs` | `StatementChecker` dispatcher + `StatementCheckCallbacks` trait. Drives reachability state transitions per statement and emits TS7027/TS7029 at the right anchors. |
| `crates/tsz-checker/src/state/state_checking_members/statement_callback_bridge.rs` | `CheckerState`'s implementation of the callbacks: `report_unreachable_statement`, `report_fallthrough_case`, `report_unreachable_code_at_node`, exhaustiveness bridge. |
| `crates/tsz-checker/src/types/function_type_helpers.rs` | `check_function_return_completeness` — TS2355/TS2366/TS7030 orchestration. |
| `crates/tsz-checker/src/types/utilities/return_type.rs` | `function_body_falls_through`, `body_has_return_with_value` — the fall-through/return-presence probes the return check consumes. |
| `crates/tsz-checker/src/checkers/promise_checker.rs` | `requires_return_value`, `type_requires_return_ts2355`, `should_skip_no_implicit_return_check`, `return_type_for_implicit_return_check` — the type-policy predicates for the return family. |
| `crates/tsz-checker/src/flow/flow_analysis/usage.rs` | TS2454 emission (`emit_definite_assignment_error`), `should_check_definite_assignment`, `skip_definite_assignment_for_type`. |
| `crates/tsz-checker/src/flow/flow_analyzer.rs` | `DefiniteAssignmentAnalyzer` — forward dataflow over the binder `FlowNodeArena`; `AssignmentState`, `MAX_FLOW_ANALYSIS_ITERATIONS`. |
| `crates/tsz-checker/src/query_boundaries/flow_analysis.rs` | Solver-facing gateway: `cases_exhaust_type`, `typeof_switch_domain`, `nullish_coalescing_switch_domain`, `property_access_function_returns_never`, `function_return_type`. |
| `crates/tsz-binder/src/flow.rs` | `FlowNode`, `FlowNodeArena`, `flow_flags::UNREACHABLE` (= `1 << 0`) and the rest of the flag set. The binder lays the skeleton; the checker reads it. |

---

## Two graphs, one sentinel: where reachability data comes from

The single most important orientation fact (already flagged by
[checker-flow-and-narrowing](checker-flow-and-narrowing.md)) is that there are
*three* flow-graph-shaped things, and reachability uses different ones than
narrowing.

```
 binder                          checker
 ──────                          ───────
 FlowNodeArena  ───────────────► control_flow::FlowGraph<'a>   (read-only view)
 (flow.rs)        wraps          FlowGraph::new(&binder.flow_nodes)
 flow_flags::                       │
   UNREACHABLE = 1<<0                ├─► FlowAnalyzer  ──► narrowing  (OTHER doc)
                                     └─► DefiniteAssignmentAnalyzer ─► TS2454

 NodeArena (AST) ──────────────► flow_graph_builder::FlowGraph (side-table)
                  post-binding     unreachable_nodes / unreachable_flow
                  traversal        mark_unreachable() / is_unreachable()
                                       │
                                       └─► ReachabilityAnalyzer (test-facing)

 NodeArena (AST) ──────────────► reachability_checker.rs   (THE diagnostic path)
                  syntactic        statement_falls_through / switch_falls_through
                  walk             call_expression_terminates_control_flow
                                       │
                                       └─► TS7027 / TS7029 / TS2355 / TS2366 / TS7030
```

- The **binder** builds `FlowNode`s in `crates/tsz-binder/src/flow.rs` and owns
  the `flow_flags::UNREACHABLE` sentinel. It computes *no* types — per the
  [binder](binder.md) contract.
- `control_flow::FlowGraph<'a>` (in `flow/control_flow/core.rs`, constructed
  `FlowGraph::new(&binder.flow_nodes)`) is a read-only query wrapper. Narrowing
  and definite-assignment read through it. It is a *view*, not a second graph.
- `flow_graph_builder::FlowGraph` (in `flow/flow_graph_builder/core.rs`) is a
  genuinely separate side-table: it owns its own `FlowNodeArena`
  (`FlowGraph::new()` allocs `unreachable_flow = nodes.alloc(flow_flags::UNREACHABLE)`),
  walks the `NodeArena` post-binding, and records `unreachable_nodes`. Its
  `create_flow_node` propagates unreachability: *if the antecedent is
  `unreachable_flow`, it returns `unreachable_flow`* (see `core.rs`,
  `create_flow_node`), and `record_node_flow` calls `mark_unreachable(node)`
  whenever `current_flow == unreachable_flow`. `ReachabilityAnalyzer`
  (`reachability_analyzer.rs`) only *queries* this — `unreachable_count`,
  `has_unreachable_code` — and is test-facing.
- The **diagnostic-bearing** reachability path is none of those graphs. It is
  the *structural* walk in `reachability_checker.rs`, driven statement-by-
  statement from `statements.rs`. That is the path that actually emits TS7027,
  TS7029, and feeds the function-return family. The rest of this document is
  mostly about that path.

> Why a structural walk and not the side-table graph? Because tsc itself
> computes most of these (`isStatementKindThatDoesNotAffectControlFlow`,
> `checkUnreachableNodes`, `getControlFlowContainer` return analysis) by
> inspecting statement *kinds* and never-returning calls, not by reifying a CFG
> just for the diagnostic. The structural walk matches that shape and avoids the
> cost of building a per-function CFG when all that's needed is "does this block
> fall off the end."

---

## The reachability state machine

Reachability reporting is driven by two boolean bits on `CheckerState.ctx`
(allocated in [checker-context-and-state](checker-context-and-state.md)):

| Bit | Meaning |
| --- | --- |
| `is_unreachable` | Control flow is statically known not to reach the current statement. |
| `has_reported_unreachable` | A TS7027 has already been emitted for the current contiguous unreachable group, so suppress further ones until the group resets. |

The dispatcher `StatementChecker::check_with_request`
(`statements.rs`) calls `state.report_unreachable_statement(stmt_idx)` **before**
type-checking each statement. The implementation
(`statement_callback_bridge.rs`, `report_unreachable_statement`) is the policy
center:

```text
report_unreachable_statement(stmt):
  if not ctx.is_unreachable: return            # reachable -> nothing to do
  should_skip = statement_kind_does_not_affect_control_flow(stmt)
     # EMPTY_STATEMENT, FUNCTION_DECLARATION, INTERFACE_DECLARATION,
     # TYPE_ALIAS_DECLARATION, BLOCK,
     # erased const enum (unless preserveConstEnums),
     # ambient/declare module (or module with no body),
     # `var x;` without initializer (is_var_without_initializer)
  if not should_skip and not ctx.has_reported_unreachable:
      if allow_unreachable_code != Some(false): return    # only when explicitly disabled
      error_at_node(stmt, TS7027 "Unreachable code detected.")
      ctx.has_reported_unreachable = true
  else if should_skip:
      ctx.has_reported_unreachable = false    # hoisted decl ends a group
```

Two parity subtleties live here:

1. **One TS7027 per unreachable group, not per statement.** tsc reports the
   *first* unreachable statement in a contiguous run and stays quiet for the
   rest. `has_reported_unreachable` enforces that. The crucial twist is the
   `else if should_skip` branch: a statement that does not affect control flow
   (a hoisted `function` declaration, an `interface`, a bare `var t;`) *resets*
   `has_reported_unreachable` to `false`, so a *subsequent* unreachable
   statement starts a fresh group and emits its own TS7027. This mirrors tsc's
   `unreachableJavascriptChecked` behavior where hoisted declarations split
   groups.

2. **`allowUnreachableCode` gating.** The default option value is `None`
   (`crates/tsz-common/src/options/checker.rs`, `allow_unreachable_code: None`).
   TS7027 fires **only** when `allow_unreachable_code == Some(false)` — i.e. the
   user explicitly passed `"allowUnreachableCode": false`. With the default
   `None`, tsc emits the *suggestion* but not the error in the standard program
   diagnostic stream, so the checker returns early.

### Where `is_unreachable` is set and restored

The dispatcher manages `is_unreachable` as a save/restore stack across nested
statement contexts (`statements.rs`). The pattern is uniform: snapshot
`prev_unreachable` / `prev_reported`, mutate, recurse, restore.

- **Block** (`BLOCK` arm): iterate statements; after each, if
  `!statement_falls_through(inner_stmt)` set `is_unreachable = true` so the
  *next* statement in the block is flagged. Restore on exit.
- **If** (`IF_STATEMENT` arm): when the condition `is_false_condition`, the then
  branch is checked via `check_unreachable_condition_branch_with_request`; when
  `is_true_condition`, the else branch is. That helper
  (`statement_callback_bridge.rs`) sets `is_unreachable = true`,
  `has_reported_unreachable = true`, and temporarily forces
  `allow_unreachable_code = Some(true)` before recursing, then restores all
  three. The forcing is deliberate: tsc does *not* report TS7027 merely because
  a branch is statically dead from a constant condition — only because of
  control-flow terminators (`return`/`throw`/never-call). The branch still needs
  semantic checking (the body can have its own errors), so it is checked but
  silenced for TS7027.
- **While / do** (`WHILE_STATEMENT | DO_STATEMENT`): a `while (false) { ... }`
  body is marked unreachable (`is_false_condition(condition)`); the loop body is
  checked twice (once for declarations, once after `clear_loop_body_recheck_caches`
  for stabilized loop-entry flow types — a narrowing concern, see
  [checker-flow-and-narrowing](checker-flow-and-narrowing.md)).
- **For** (`FOR_STATEMENT`): the most intricate. If the initializer terminates
  control flow (`call_expression_terminates_control_flow(initializer)`), the
  condition and incrementor are unreachable and reported at the right anchors
  (below). If the condition is `is_false_condition` or itself terminates, the
  body and incrementor become unreachable.

---

## Walk-through 1: TS7027 after a `return`

Source:

```typescript
function f(): number {
  return 1;
  console.log("dead");   // TS7027
}
```

1. The function body block is checked. The dispatcher's `BLOCK` arm iterates the
   two statements.
2. `return 1;` is checked. Then `statement_falls_through(return_stmt)` is asked
   (`reachability_checker.rs`): the `RETURN_STATEMENT` arm returns `false`.
3. Because it does not fall through, the `BLOCK` arm sets
   `state.set_unreachable(true)`.
4. The next iteration calls `report_unreachable_statement(console_log_stmt)`
   first. `ctx.is_unreachable` is now `true`. The statement is an
   `EXPRESSION_STATEMENT` (not a skip kind), `has_reported_unreachable` is
   `false`, and — *if* `allowUnreachableCode: false` was set — `error_at_node`
   emits TS7027 and sets `has_reported_unreachable = true`.

If a second dead statement followed, `has_reported_unreachable` would suppress
its TS7027 — unless a hoisted `function`/`interface`/bare-`var` reset the bit in
between.

---

## Never-returning calls: `call_expression_terminates_control_flow`

`return`/`throw`/`break`/`continue` are the easy terminators. The interesting
parity surface is **calls that never return** — `Debug.fail()`, an assertion
with a literally-`false` condition, or a throwing IIFE. tsc's
`isNeverReturningCall` examines the *callee's signature*, not the fully-checked
call. The checker mirrors this exactly in
`call_expression_terminates_control_flow` (`reachability_checker.rs`):

```text
call_expression_terminates_control_flow(expr):
  CALL_EXPRESSION:
      callee = skip_parenthesized_and_assertions(call.expression)
      callee_explicitly_returns_never(callee)
        OR assertion_call_with_false_condition_terminates(expr, callee)
  NEW_EXPRESSION:
      get_type_of_node(expr).is_never()
  _ : false
```

### `callee_explicitly_returns_never` — signature inspection, not call evaluation

The doc comment is explicit about *why*: fully type-checking the call would
"cache a potentially stale result in `node_types` during early phases (e.g.,
type environment building) when `this` hasn't been resolved yet." So the
function resolves the callee's *declaration* and inspects its return-type
annotation:

| Callee shape | How it resolves `never` |
| --- | --- |
| `Identifier` (`fail()`) | `resolve_identifier_symbol` then `symbol_explicitly_returns_never` (reads the symbol's `primary_declaration` and `declaration_explicitly_returns_never`). |
| `PropertyAccessExpression` (`this.fail()`, `Debug.fail()`) | `property_access_callee_explicitly_returns_never`: first try the binder's `node_symbols`; for `this.m()`, scan `enclosing_class.member_nodes` for a matching method and check its annotation; for `Ns.m()`, resolve the namespace symbol's `exports`; finally a *guarded* type fallback. |
| `FunctionExpression` / `ArrowFunction` (a direct IIFE callee) | `declaration_explicitly_returns_never(callee, check_body_for_throws = true)` — the only path that may inspect the body. |

`declaration_explicitly_returns_never` checks an explicit `: never` annotation
(`get_type_from_type_node(...) == TypeId::NEVER`), and — for JS files under
`checkJs` — a `@returns {never}` JSDoc tag
(`resolve_jsdoc_return_type(...) == Some(TypeId::NEVER)`). The body-throws branch
is gated behind `check_body_for_throws` so it only fires for a *direct* IIFE
callee; resolving through a *symbol* (e.g. a named function expression `self`
that calls itself) must not analyze the body or it would recurse infinitely.

### Assertion calls with a literally-false condition

`assertion_call_with_false_condition_terminates` handles
`assert(false, ...)`-style asserts: it pulls the assertion predicate
(`assertion_predicate_for_call`), requires `predicate.type_id` to be `None` (a
plain `asserts cond` not `asserts cond is T`), validates the call target, finds
the asserted expression, and checks `is_false_condition(asserted_expr)`. When
the asserted condition is statically `false`, the call cannot return.

### Throwing IIFEs and the anchor problem

`(function () { throw "x"; })()` never returns, but tsc anchors the resulting
TS7027 *inside* the IIFE body, not at the statement after the call.
`terminating_iife_unreachable_anchor` (`reachability_checker.rs`) finds the
first statement in the IIFE body for which `statement_always_throws` holds and
returns that statement index, so
`report_unreachable_code_at_terminating_iife_body`
(`statement_callback_bridge.rs`) can point the diagnostic there.

`statement_always_throws` is distinct from `!statement_falls_through`: it returns
`true` for `throw` and never-call expression statements, but `false` for
`return` — because *from the caller's perspective* a function that returns
completes normally. `block_always_throws` enforces "terminates via throw, not
via return": it returns `false` the moment a statement terminates the block
without throwing (a `return`).

---

## Statement fall-through: `statement_falls_through`

This is the structural core (`reachability_checker.rs`). It answers "can
execution continue past this statement," returning `true` for "yes, falls
through":

| Statement kind | Falls through? |
| --- | --- |
| `RETURN` / `THROW` / `BREAK` / `CONTINUE` | `false` (terminators) |
| `BLOCK` | `block_falls_through(stmts)` — `false` iff *any* statement does not fall through (sequential semantics) |
| `EXPRESSION_STATEMENT` | `!call_expression_terminates_control_flow(expr)` — a never-call statement does not fall through |
| `IF` | then-falls `||` else-falls; **if no else clause, always `true`** (the missing branch always falls through) |
| `SWITCH` | `switch_falls_through` |
| `TRY` | `try_falls_through` |
| `CATCH_CLAUSE` | falls through iff its block does |
| `WHILE` / `DO` / `FOR` | `loop_falls_through` |
| `LABELED` | falls through iff the wrapped statement does |
| everything else (incl. `VARIABLE_STATEMENT`) | `true` |

Note the deliberate `VARIABLE_STATEMENT` comment: `const x = fail();` is **not**
treated as a control-flow terminator even though `fail()` returns `never`. tsc
only treats *expression-statement-level* never-calls as terminators; a never-
call in a `const` initializer still leaves the function "falling off the end,"
which is exactly what makes TS2355 / TS2366 fire correctly for code after it.

### `switch_falls_through` — exhaustiveness + bottom-up clause analysis

`switch_falls_through` (`reachability_checker.rs`) is the densest piece:

1. Find whether there is a `DEFAULT_CLAUSE`.
2. **No default + not exhaustive ⇒ falls through.** Without a default clause,
   an unmatched discriminant skips the body — *unless*
   `switch_has_exhaustive_coverage` proves the cases cover the discriminant.
3. Walk clauses **bottom to top** (`.rev()`) so empty/grouped clauses inherit
   the next clause's fall-through:
   - empty clause body ⇒ inherits `falls_from_next`;
   - any clause containing a `break` ⇒ falls through (`true`) regardless of
     later clauses (a `break` completes the switch normally);
   - non-terminating clause body ⇒ inherits `falls_from_next`;
   - terminating clause body (return/throw) ⇒ `false`.
4. The switch falls through iff *any* entry falls through
   (`any_entry_falls_through`).

### `try_falls_through` and `loop_falls_through`

`try_falls_through`: a `finally` that does not fall through forces the whole try
to not fall through (the finally dominates the exit). Otherwise the try falls
through iff the try block or the catch block does.

`loop_falls_through`: a loop with an always-true condition
(`condition.is_none()` for `for(;;)`, or `is_true_condition(condition)`) and
**no reachable `break`** (`!contains_break_statement`) does not fall through —
it is an infinite loop, so code after it is unreachable. `contains_break_statement`
walks into blocks/ifs/try/labeled statements but, importantly, stops at nested
loops/switches implicitly (a `break` inside a nested loop binds to *that* loop,
so only directly-reachable breaks are counted at this level).

### `is_true_condition` / `is_false_condition`

These are *syntactic* constant folders over literals and `&&`/`||`/`!`
(`reachability_checker.rs`). They skip parentheses and assertions
(`skip_parenthesized_and_assertions`), recognize `TrueKeyword`/`FalseKeyword`,
and propagate through boolean operators with the right short-circuit algebra
(`a && b` true iff both true; `a || b` false iff both false; `!a` flips). They do
**not** consult the solver — they are deliberately conservative syntax matches,
mirroring tsc's `getConstantValue`-flavored constant condition detection used for
loop reachability.

---

## Function-return completeness: TS2355 / TS2366 / TS7030

This family lives in `check_function_return_completeness`
(`types/function_type_helpers.rs`), invoked from `types/function_type.rs`
(the function-shape construction path; see
[checker-class-shape-construction](checker-class-shape-construction.md) and
[checker-calls-signatures-generics](checker-calls-signatures-generics.md)). It
combines a *type policy* (does the declared return type require a value?) with a
*reachability fact* (does the body fall off the end?).

```text
check_function_return_completeness(ctx):
  skip if is_function_declaration or body is None
  skip METHOD_DECLARATION / CONSTRUCTOR        # checked later, with enclosing_class set
  check_return_type = return_type_for_implicit_return_check(effective_return, is_async, is_generator)
  if async and annotation is exactly global Promise<...>: check_return_type = VOID  # suppress
  requires_return = requires_return_value(check_return_type)
  has_return      = body_has_return_with_value(body)
  falls_through   = function_body_falls_through(body)

  if has_type_annotation and requires_return and falls_through and check_return_type != VOID:
      if not has_return: TS2355  (A function whose declared type ... must return a value)
      else:              TS2366  (Function lacks ending return statement ...)
  else if noImplicitReturns and has_return and falls_through:
      ts7030_type = return_type_for_implicit_return_check(...)
      if not should_skip_no_implicit_return_check(ts7030_type, ...): TS7030 (Not all code paths return a value)
```

The reachability input is `function_body_falls_through`
(`types/utilities/return_type.rs`): for a block body it is
`block_falls_through(stmts)`; for an expression body (concise arrow) it is
`false` (an expression body always produces a value). `body_has_return_with_value`
scans for any `return <expr>;`.

The type policy is delegated, never hand-rolled:

- `requires_return_value` (`promise_checker.rs`) returns `false` for `void`,
  `undefined`, `any`, `never`, `unknown`, `error`, and for any union containing
  `void` or `undefined`. This gates TS2366.
- `type_requires_return_ts2355` is *stricter*: a union containing `undefined`
  (but not `void`) still requires a return for TS2355, because the declared type
  as a whole is not purely void/undefined/any. Hence `string | undefined`
  triggers TS2355 but is exempt from TS2366.
- `should_skip_no_implicit_return_check` and
  `return_type_for_implicit_return_check` handle the async/generator unwrap
  (`Promise<T>` → `T`, generator completion type) and the void/any TS7030 skip.

This split is exactly tsc's: TS2355 vs TS2366 differ on whether the body has
*any* value-return, and TS7030 (`noImplicitReturns`) is the opt-in superset that
fires even for void-ish types only when *some* path returns a value and another
falls through.

### TS2355/TS2366 diagnostic anchoring

tsc points TS2355/TS2366 at the **return-type annotation**
(`type_annotation`). TS7030 points at the annotation if present, else the
function name node, else the function node itself (`error_node` selection in
`check_function_return_completeness`). These anchors are parity-load-bearing —
conformance baselines compare the diagnostic *span*, not just the code.

---

## Switch fallthrough: TS7029

TS7029 (`noFallthroughCasesInSwitch`) is computed in the `SWITCH_STATEMENT` arm
of the dispatcher (`statements.rs`), not in `reachability_checker.rs`:

```text
for each clause i:
  check clause statements; track clause_falls_through (false once a statement doesn't fall through)
  if no_fallthrough_cases_in_switch
     and clause_falls_through
     and clause has statements
     and i < last
     and next clause has statements:
        report_fallthrough_case(clause_idx)   # TS7029
```

The two "has statements" guards encode the parity rule that *empty* case
grouping is legal: `case 1: case 2: break;` does not warn because clause `1` has
no statements (it groups into `2`), and a clause whose *next* clause is empty is
not a fallthrough into executable code. `report_fallthrough_case`
(`statement_callback_bridge.rs`) emits TS7029 anchored at the *clause* node.

Note this is the **`noFallthroughCasesInSwitch`** diagnostic and is independent
of the *reachability* `switch_falls_through` computation used for fall-off-end
analysis — the two share the "does this clause fall through" sub-question but
serve different diagnostics.

---

## Switch exhaustiveness as a reachability input

`switch_has_exhaustive_coverage` (`reachability_checker.rs`) decides whether a
defaultless switch is nonetheless exhaustive, which feeds `switch_falls_through`
(step 2 above) and downstream return analysis. The checker **recognizes** the
switch shapes; the **type math** is delegated to
`query_boundaries::flow_analysis`:

| Switch operand shape | Discriminant domain helper |
| --- | --- |
| `switch (typeof x)` | `typeof_switch_operand` extracts `x`; `typeof_switch_domain(types, env, operand_type)` computes the union of surviving `typeof` strings via `NarrowingContext::narrow_by_typeof` over the 8 JS `typeof` results |
| `switch (a ?? b)` | `nullish_coalescing_switch_type` → `nullish_coalescing_switch_domain(types, left, right)` removes nullish from the left and unions with the right |
| plain discriminant | literal type from initializer, else `get_type_of_node(expr)` |

Coverage itself is `switch_exhaustive_with_types` → `cases_exhaust_type`
(`query_boundaries/flow_analysis.rs`), which:

1. normalizes enum members to their domains (`enum_member_domain`);
2. bails for `error`/`any`/`unknown` discriminants or any such case type;
3. tries exact set coverage (`case_types_exactly_cover_switch_domain`, an
   `FxHashSet` removal over union members) — cheap and identity-based;
4. falls back to `NarrowingContext::narrow_excluding_types(switch, cases) == NEVER`
   — the solver's exclusion semantics.

When the direct path fails, `switch_has_exhaustive_coverage` also tries an
enum-normalized assignability check: it unions the normalized cases and asks the
relation gateway whether the normalized switch type is assignable to that union
(`query::flow_assignability_outcome(...).related`). This is the *only* place
this analysis touches the relation kernel, and it does so through the
[checker-assignability-gateway](checker-assignability-gateway.md) boundary, never
by hand-walking type shapes — honoring the hard rule that the checker must not
run relation kernels itself ([solver-relations](solver-relations.md)).

There is also a `*_cached` variant, `switch_has_exhaustive_coverage_cached`,
that is `&self` (immutable) and reads case/discriminant types from
`node_types` / `literal_type_from_initializer` instead of computing them. It
exists for immutable analysis paths (e.g. flow narrowing) that cannot call the
`&mut self` `get_type_of_node`. The cached path skips the `??` and assignability
fallbacks — it is a fast, side-effect-free probe.

---

## Definite assignment: TS2454

TS2454 ("Variable '{0}' is used before being assigned.") is the third
reachability-completeness diagnostic. Unlike fall-through (syntactic) it is a
**forward dataflow** over the binder flow graph, because "every path reaching
this use has already assigned the variable" is a join/merge property.

### The dataflow engine

`DefiniteAssignmentAnalyzer` (`flow/flow_analyzer.rs`) walks the binder
`FlowNodeArena` with a worklist fixed-point:

- `AssignmentState` is a three-valued lattice: `Unassigned`,
  `MaybeAssigned`, `DefinitelyAssigned`. `AssignmentStateMap::is_definite()`
  is `true` only when no variable is `MaybeAssigned`.
- `analyze(entry)` seeds an empty `AssignmentStateMap`, pushes `entry`, and
  iterates. For a node with multiple antecedents it **merges** predecessor
  states (a join: `DefinitelyAssigned` ∧ `Unassigned` = `MaybeAssigned`).
- The loop is **fuel-bounded**: `iterations > MAX_FLOW_ANALYSIS_ITERATIONS`
  (= `100_000`) breaks out, preventing non-termination on malformed graphs.
  This is the analysis's recursion guard; it is a hard ceiling, not a
  correctness lever.

`is_definitely_assigned(var_id, flow_id)` queries the computed state at a flow
node.

### Emission path

In practice TS2454 is emitted from the narrowing path in
`flow/flow_analysis/usage.rs` (it shares the same flow-graph walk as narrowing
for efficiency):

1. `should_check_definite_assignment(sym_id, idx)` and
   `skip_definite_assignment_for_type(declared_type)` gate the check.
   `skip_definite_assignment_for_type` returns `true` when
   `strictNullChecks` is off (every type then implicitly includes `undefined`),
   or the type is `any`/`unknown`/`error`, or it contains `undefined`/`void`
   (delegated to `query::type_contains_undefined`).
2. `should_report_variable_use_before_assignment` (the
   `query_boundaries::definite_assignment` boundary) plus
   `is_definitely_assigned_at_with_symbol` decide.
3. `emit_definite_assignment_error(idx, sym_id)` emits TS2454. It deduplicates
   on `(node.pos, sym_id)` via `ctx.emitted_ts2454_errors`, preserves the
   declared name's `original_text` (so `xx` is shown as written, matching
   tsc), and **also** pushes onto `ctx.deferred_ts2454_errors` — because
   definite-assignment runs inside speculative call-checker contexts (overload
   probing, generic inference) whose diagnostics get truncated on rollback. The
   deferred buffer survives rollback and is re-emitted at end of
   `check_source_file`. See
   [checker-calls-signatures-generics](checker-calls-signatures-generics.md) for
   the speculative-rollback machinery this defends against.

For a control-flow-typed implicit-`any` symbol (`var a;`), when TS2454 fires the
expression type becomes `undefined` (matching tsc's use of `undefined` as the
initial CFA type for hoisted `var`), so downstream errors cascade correctly
(e.g. TS2345 with an `undefined` argument).

---

## Walk-through 2: defaultless exhaustive switch and fall-off-end

Source:

```typescript
type Dir = "n" | "s";
function opp(d: Dir): Dir {
  switch (d) {
    case "n": return "s";
    case "s": return "n";
  }
}                          // no TS2366 — switch is exhaustive
```

1. The function-shape path calls `check_function_return_completeness`. The
   declared return is `Dir` (`"n" | "s"`), so `requires_return_value` is `true`.
2. `function_body_falls_through(body)` → `block_falls_through([switch])` →
   `statement_falls_through(switch)` → `switch_falls_through`.
3. `switch_falls_through`: no `DEFAULT_CLAUSE`. It asks
   `switch_has_exhaustive_coverage(switch_data)`.
4. The discriminant is plain, so the switch type is `"n" | "s"`. Case types are
   `"n"` and `"s"`. `cases_exhaust_type` → `case_types_exactly_cover_switch_domain`
   removes `"n"` and `"s"` from the union member set, leaving it empty ⇒ `true`.
5. Exhaustive ⇒ the no-default-not-exhaustive early `return true` is skipped.
   Both clauses `return`, so each clause is non-falling; `any_entry_falls_through`
   is `false`.
6. `switch_falls_through` is `false` ⇒ `function_body_falls_through` is `false`
   ⇒ the `falls_through` guard in `check_function_return_completeness` is not
   met ⇒ **no TS2366**.

Remove `case "s"` and the union member set retains `"s"`,
`case_types_exactly_cover_switch_domain` is `false`, the narrowing fallback
`narrow_excluding_types("n"|"s", ["n"])` is `"s"` (≠ `NEVER`), so the switch is
non-exhaustive, falls through, and TS2366 fires anchored at the `: Dir`
annotation.

---

## Caches and invariants

| Cache / state | Owner | Invalidation / lifetime |
| --- | --- | --- |
| `ctx.is_unreachable`, `ctx.has_reported_unreachable` | `CheckerState.ctx` ([checker-context-and-state](checker-context-and-state.md)) | Save/restore stack across nested statement contexts in `statements.rs`; reset per group by the `should_skip` branch of `report_unreachable_statement`. |
| `ctx.emitted_ts2454_errors` (set of `(pos, sym_id)`) | `CheckerState.ctx` | Dedup TS2454; snapshotted/restored around speculative overload/inference rounds (`restore_ts2454_state` in call-checker). |
| `ctx.deferred_ts2454_errors` (vec of `(idx, sym_id)`) | `CheckerState.ctx` | Survives speculative rollback; drained and re-emitted at end of `check_source_file`. |
| `ctx.daa_error_nodes`, `ctx.flow_narrowed_nodes` | `CheckerState.ctx` | Mark nodes where TS2454/invalid-narrowing fired so the second flow-narrowing pass in `get_type_of_node` does not re-narrow and hide TS2322. |
| `node_states` in `DefiniteAssignmentAnalyzer` | analyzer instance | Per-`analyze` run; fixed-point map keyed by `FlowNodeId`. |
| `unreachable_nodes` in `flow_graph_builder::FlowGraph` | the side-table graph | Populated during one post-binding traversal; `mark_unreachable` is monotonic (insert-only). |
| `flow_flags::UNREACHABLE` sentinel | binder `FlowNodeArena` | Set once at bind time; the checker's `create_flow_node` propagates it (unreachable antecedent ⇒ unreachable node). |

Invariants worth stating:

1. **No type kernels in the structural walk.** `reachability_checker.rs` never
   calls a relation, inference, instantiation, or evaluation kernel directly.
   The *only* solver contact is via `query_boundaries::flow_analysis`
   (`cases_exhaust_type`, `typeof_switch_domain`, `function_return_type`,
   `property_access_function_returns_never`, `flow_assignability_outcome`). This
   is the hard architecture rule for the checker.
2. **Signature inspection, not call evaluation, for never-calls.** Deciding a
   call returns `never` reads the callee declaration's annotation/JSDoc, not the
   checked call type — to avoid caching stale `node_types` during early phases.
   The only body-analysis exception is a *direct* IIFE callee
   (`check_body_for_throws = true`).
3. **One TS7027 per contiguous unreachable group.** Enforced by
   `has_reported_unreachable`, reset by control-flow-neutral statements.
4. **`VARIABLE_STATEMENT` never terminates control flow.** A never-call in a
   `const`/`let`/`var` initializer leaves the function falling off the end, so
   TS2355/TS2366 still apply.
5. **TS2454 dataflow is fuel-bounded** at `MAX_FLOW_ANALYSIS_ITERATIONS`
   (`100_000`).

---

## Edge cases and tsc parity

- **`allowUnreachableCode` default is `None`, not `false`.** TS7027 only fires
  under `Some(false)`. Under the default, the structural walk still runs (it
  feeds return analysis) but suppresses the diagnostic, matching tsc's
  suggestion-only behavior.
- **Statically-dead branches do not produce TS7027.** `if (false) { ... }` and
  `if (true) {} else { ... }` check the dead branch via
  `check_unreachable_condition_branch_with_request`, which forces
  `allow_unreachable_code = Some(true)` for that subtree — tsc reports
  unreachable code only from control-flow terminators, not constant conditions.
- **Hoisted declarations split unreachable groups.** A `function`/`interface`/
  bare-`var` after a `return` resets `has_reported_unreachable`, so a subsequent
  dead statement gets its own TS7027 (tsc's `unreachableJavascriptChecked`).
- **`var x;` without initializer after `return` is silent** but `var x = 10;` is
  not — `is_var_without_initializer` walks *both* the declaration list and each
  inner declaration to detect initializers. (A bug where iterating only the
  outer list missed `var x = 10;` is fixed there.)
- **Empty case grouping is legal under `noFallthroughCasesInSwitch`.** The two
  "has statements" guards in the TS7029 site mean `case 1: case 2: break;` does
  not warn.
- **`break` rescues a clause and an infinite loop.** A `break` anywhere a clause
  body makes `switch_falls_through` treat the clause as falling through; a
  reachable `break` in a `while (true)` makes `loop_falls_through` return `true`.
- **TS2355 vs TS2366 split on `string | undefined`.** `requires_return_value`
  exempts unions with `undefined` (no TS2366), but `type_requires_return_ts2355`
  does not (TS2355 still fires) unless the union also contains `void`.
- **Async `Promise<...>` annotation suppresses return completeness** only when
  the annotation resolves to the *global* `Promise`
  (`return_type_annotation_is_exactly_promise`); a locally-named `Promise`
  follows normal rules.
- **Methods/constructors are checked later.** `check_function_return_completeness`
  skips `METHOD_DECLARATION`/`CONSTRUCTOR` so they are analyzed during class
  checking when `enclosing_class` is set (needed for `this.fail()` never-call
  detection) — see [checker-classes](checker-classes.md).
- **TS2454 unicode preservation.** The diagnostic shows the declared name's
  `original_text` (`xx`), matching tsc's verbatim identifier rendering.
- **TS2454 under speculation.** Emitted into both the immediate diagnostics and
  a deferred buffer so it survives overload/inference rollback.

---

## Where this sits in the pipeline

```
parser ──► binder ──► checker ──────────────────────► solver
            │           │                               ▲
   flow_flags::         │  reachability_checker.rs       │
   UNREACHABLE          │  (structural AST walk)         │
   FlowNodeArena        │      │                         │
   (skeleton, no types) │      ├─ statement_falls_through│
                        │      ├─ switch_falls_through   │
                        │      ├─ call_..terminates..    │
                        │      └─ switch_has_exhaustive..│
                        │            │                   │
                        │            └─ query_boundaries/flow_analysis ──► solver
                        │                (cases_exhaust_type, typeof_switch_domain,
                        │                 function_return_type, flow_assignability_outcome)
                        │
                        ├─ DefiniteAssignmentAnalyzer (binder FlowNodeArena) ─► TS2454
                        └─ check_function_return_completeness ─► TS2355/TS2366/TS7030
```

The binder ([binder](binder.md)) lays the `FlowNode` skeleton and the
`UNREACHABLE` sentinel but computes no types. The checker's structural walk and
dataflow turn that skeleton plus the AST into the TS7027 / TS7029 / TS2355 /
TS2366 / TS7030 / TS2454 diagnostics, asking the solver only through the
`query_boundaries::flow_analysis` gateway and the
[checker-assignability-gateway](checker-assignability-gateway.md). For the *type*
side of flow — narrowing references across the same graph — see
[checker-flow-and-narrowing](checker-flow-and-narrowing.md) and
[solver-narrowing](solver-narrowing.md). For how these diagnostics are formatted
and ordered, see
[checker-error-reporter-diagnostics](checker-error-reporter-diagnostics.md). For
the timeline that schedules statement checking and the end-of-file deferred
re-emission, see [end-to-end-timeline](end-to-end-timeline.md).
