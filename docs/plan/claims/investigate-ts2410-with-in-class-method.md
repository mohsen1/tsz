# investigate(checker): TS2410 missed on `with` inside class member body

- **Date**: 2026-05-02
- **Branch**: `investigate/ts2410-with-in-class-method`
- **PR**: TBD (hand-off doc, no code change)
- **Status**: claim
- **Workstream**: 1 (Conformance — `superCallsInConstructor.ts` and any
  test that exercises a `with` inside a class method/constructor body)

## Summary

For `with` statements **outside** a class (top-level or in a plain
function body), tsz emits both TS1101 ("not allowed in strict mode")
and TS2410 ("`with` statement is not supported"). Inside a class
method or constructor body, **only TS1101 fires** — TS2410 is dropped.
tsc emits both in all contexts.

## Minimal repro

```ts
declare const obj: any;

// PLAIN FUNCTION — both TS1101 + TS2410 emitted (correct)
function f() {
    with (obj) { console.log("hi"); }
}

// CLASS METHOD — only TS1101 emitted (BUG: TS2410 missing)
class C {
    method() {
        with (obj) { console.log("hi"); }
    }
}
```

Run with `.target/dist-fast/tsz --noEmit --target es2015 file.ts`.

## Why I think this is the bug

`crates/tsz-checker/src/state/state_checking_members/statement_checks.rs:199`
unconditionally emits TS2410 for `.ts` files. It is reached for the
plain-function path:

```
function_declaration_checks.rs:748 check_statement_with_request(func.body)
  → StatementChecker::check_with_request
  → BLOCK arm → recurses into statements → WITH_STATEMENT arm
  → state.check_with_statement(stmt_idx)
  → emits TS1101 + TS2410
```

The class-method path *should* reach the same dispatch:

```
member_declaration_checks.rs:1350 check_statement_with_request(stmt_idx)
  → StatementChecker::check_with_request
  → WITH_STATEMENT arm → state.check_with_statement(stmt_idx)
  → emits TS1101 + TS2410
```

…but in practice only TS1101 fires for the class-method case. TS1101
comes from `is_with_statement_in_strict_mode_context` plus the
subtree walk in `report_strict_mode_with_in_subtree`; that subtree
walk handles the `with` statement *inside* the constructor body
implicitly through a different walk that doesn't go through
`check_with_statement`.

## Verification update (2026-05-02 19:50)

Re-investigated this iteration with `eprintln!` debug prints:

- **Hypothesis 1 ruled out.** `check_with_statement` IS called for
  the class-method case (`stmt_idx=NodeIndex(17), is_js_file=false`).
- **Hypothesis 2 ruled out** in the simple form: `error_at_node`
  pushes TS2410 into `ctx.diagnostics`. After the `if !is_js_file …`
  branch, `ctx.diagnostics.len()` goes from 0 to 1; the last
  diagnostic is `code=2410 start=57 length=39 msg="The 'with'
  statement is not supported."`.
- **New finding:** at the *end* of `check_with_statement`,
  `ctx.diagnostics` contains exactly `[2410]` (TS1101 is NOT yet in
  the list — `has_syntax_parse_errors` is true because the **parser**
  already emitted TS1101 in
  `crates/tsz-parser/src/parser/state_declarations_exports.rs:2317`).
- **Output divergence:** the final CLI output shows only TS1101,
  *not* TS2410 — meaning some later stage drops the checker's TS2410
  in this case. It's not the `error()` dedup (different code), and
  not the `source_file::collect_diagnostics` retains
  (those filter TS7006 and TS2322-nested-wrapper, neither matches).

So the trail now points to the **diagnostic merge in the CLI driver**
(`crates/tsz-cli/src/driver/check.rs:312` `collect_diagnostics`) or
something else after `check_with_statement` returns but before the
diagnostic list reaches the reporter. The plain-function case takes
the same checker path (TS2410 is added) but somehow keeps it in the
final list; the class-method case loses it.

A likely culprit (untested): some pass that strips checker-emitted
diagnostics whose span overlaps a parser-emitted diagnostic at a
shorter span. The plain-function test ALSO has TS1101 emitted by the
parser at the same start, so this would have to differ on something
else — possibly `program_has_real_syntax_errors(program)` or a
parse-error-suppression flag that's set per-file in class context but
not in plain-function context.

The minimal next step is to instrument `collect_diagnostics` (and
intermediate merges) to print every diagnostic added for the file,
then compare the trace between plain-function and class-method
cases. The diff in trace will pinpoint the dropping stage.

## (Original two hypotheses — kept for context)

1. **The class-method `check_statement_with_request` path is short-
   circuiting before reaching `check_with_statement`.** Maybe
   `report_unreachable_statement` or the in-class-member context flag
   skips the WITH_STATEMENT arm. Add a temporary `eprintln!` in
   `crates/tsz-checker/src/state/state_checking_members/statement_checks.rs:199`
   and run the class-method repro to confirm the function isn't
   called.

2. **`check_with_statement` *is* called but `error_at_node` rejects
   the emission.** `error_at_node` deduplicates by `(start, code)`; if
   TS2410 was somehow suppressed earlier, the second emission would
   silently drop. Less likely — TS2410 would have to have been
   emitted *somewhere* in the path, which we can rule out by checking
   all diagnostic emissions during the class-method repro.

## Likely fix shape

If hypothesis 1: identify the short-circuit and remove it for
WITH_STATEMENT. A cheap fix is to dispatch `check_with_statement`
from the class-member statement walker directly when the statement
kind is WITH_STATEMENT, before the general `check_statement_with_request`
recursion takes over.

If hypothesis 2: trace the duplicate-emit path; possibly remove a
spurious early call.

## Conformance impact

`superCallsInConstructor.ts` becomes a 1-fingerprint-closer test (was
2 missing fingerprints, would become 1 missing once TS2410 lands).
Other tests that have `with` inside a class member body would benefit
similarly. `superCallsInConstructor.ts` is unlikely to flip from
FAIL→PASS just from this fix because it also misses TS2564 (property
not initialized) — that's a separate strict-property-init issue.

## Why this is a hand-off, not a fix

Verifying which hypothesis applies requires either a debug build with
tracing enabled or a one-off `eprintln!` — both of those need a
clean rebuild and a careful run. The 25-min loop iteration window was
spent isolating the call paths and the failure conditions; the
verify-and-fix step needs another iteration. This claim doc preserves
all the context (paths, line numbers, two hypotheses, the proposed
fix shape, the conformance impact estimate) so the next agent can
pick up where this trail goes cold.
