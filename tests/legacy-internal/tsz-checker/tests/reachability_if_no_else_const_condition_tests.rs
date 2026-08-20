//! Regression coverage for `statement_falls_through`'s `IF_STATEMENT` arm
//! when there is no `else` clause and the condition is a compile-time
//! constant. tsc treats `if (true) { <terminator> }` with no `else` as
//! having no implicit fall-through path (the `then` branch always runs), the
//! same way `while (true)` with no reachable `break` is a non-completing
//! loop. Before this fix, a missing `else` unconditionally made the whole
//! `if` "fall through" regardless of the condition, so code after
//! `if (true) { throw ...; }` was never flagged unreachable (TS7027).

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn unreachable_codes(source: &str) -> Vec<u32> {
    let diagnostics = check_source(
        source,
        "test.ts",
        CheckerOptions {
            allow_unreachable_code: Some(false),
            ..CheckerOptions::default()
        },
    );
    diagnostics.iter().map(|diag| diag.code).collect()
}

#[test]
fn if_true_throw_no_else_reports_unreachable_after() {
    let codes = unreachable_codes(
        r#"
function f() {
    if (true) {
        throw new Error("x");
    }
    console.log("dead");
}
"#,
    );
    assert!(
        codes.contains(&7027),
        "expected TS7027 after `if (true) {{ throw }}` with no else, got {codes:?}"
    );
}

#[test]
fn if_true_return_no_else_reports_unreachable_after() {
    let codes = unreachable_codes(
        r#"
function f(): number {
    if (true) {
        return 1;
    }
    console.log("dead");
    return 2;
}
"#,
    );
    assert!(
        codes.contains(&7027),
        "expected TS7027 after `if (true) {{ return }}` with no else, got {codes:?}"
    );
}

#[test]
fn if_true_and_true_throw_no_else_reports_unreachable_after() {
    // Constant-folds through `&&`, matching `is_true_condition`'s existing
    // boolean-operator handling used elsewhere (e.g. loop conditions).
    let codes = unreachable_codes(
        r#"
function f() {
    if (true && true) {
        throw new Error("x");
    }
    console.log("dead");
}
"#,
    );
    assert!(
        codes.contains(&7027),
        "expected TS7027 after `if (true && true) {{ throw }}` with no else, got {codes:?}"
    );
}

#[test]
fn if_true_non_terminating_body_no_else_does_not_report_unreachable() {
    // The `then` branch itself falls through (no throw/return/never-call), so
    // even though the condition is always true, code after the `if` is
    // reachable. This guards against over-correcting to "always unreachable"
    // whenever the condition folds to `true`.
    let codes = unreachable_codes(
        r#"
function f() {
    if (true) {
        console.log("taken");
    }
    console.log("also reachable");
}
"#,
    );
    assert!(
        !codes.contains(&7027),
        "did not expect TS7027 when the `then` branch falls through, got {codes:?}"
    );
}

#[test]
fn if_false_throw_no_else_reports_unreachable_inside_dead_branch() {
    // A constant-FALSE condition with no else makes the `then` branch itself
    // dead code (tsc reports TS7027 on the `throw` verified against
    // typescript@7.0.2), while code after the `if` stays reachable (the
    // branch never executes, so the `if` as a whole always falls through).
    let codes = unreachable_codes(
        r#"
function f() {
    if (false) {
        throw new Error("x");
    }
    console.log("reachable");
}
"#,
    );
    assert!(
        codes.contains(&7027),
        "expected TS7027 on the dead `throw` inside `if (false) {{ ... }}`, got {codes:?}"
    );
}

#[test]
fn if_non_constant_condition_throw_no_else_does_not_report_unreachable() {
    // Negative control: a non-constant condition with no else keeps its
    // existing (pre-fix) behavior — the missing else is always a possible
    // skip path, so code after the `if` is reachable.
    let codes = unreachable_codes(
        r#"
function f(cond: boolean) {
    if (cond) {
        throw new Error("x");
    }
    console.log("reachable");
}
"#,
    );
    assert!(
        !codes.contains(&7027),
        "did not expect TS7027 after `if (cond) {{ throw }}` with a non-constant condition, got {codes:?}"
    );
}

#[test]
fn if_true_throw_inside_renamed_arrow_reports_unreachable_after() {
    // Adjacent case: same shape inside a differently-named arrow function
    // assigned to a `const`, to guard against any accidental name-based
    // suppression.
    let codes = unreachable_codes(
        r#"
const guardAlways = () => {
    if (true) {
        throw new Error("x");
    }
    const neverRuns = 1;
    console.log(neverRuns);
};
"#,
    );
    assert!(
        codes.contains(&7027),
        "expected TS7027 after `if (true) {{ throw }}` inside a renamed arrow function, got {codes:?}"
    );
}

// tsc's constant-condition rule for reachability is narrower than constant
// folding. `binder.ts`'s `createFlowCondition` tests
// `expression.kind === SyntaxKind.TrueKeyword` on the condition **as written**:
// it neither skips parentheses nor folds a prefix `!`. `&&`/`||` still compose,
// but through `bindCondition` recursing into each operand rather than through
// folding — so the literal `true` inside `true && true` is what reaches the
// kind check, while `(true)` and `!false` never do.
//
// Verified against typescript@7.0.2 with `--allowUnreachableCode false`:
// `if (true)` reports TS7027; `if ((true))`, `if (!false)` and `if (!!true)`
// report nothing.

#[test]
fn if_parenthesized_true_return_no_else_does_not_report_unreachable() {
    let codes = unreachable_codes(
        r#"
function f() {
    if ((true)) {
        return;
    }
    console.log("reachable");
}
"#,
    );
    assert!(
        !codes.contains(&7027),
        "a parenthesized `(true)` is not the TrueKeyword node tsc tests, so the \
         implicit else stays reachable; got {codes:?}"
    );
}

#[test]
fn if_negated_false_return_no_else_does_not_report_unreachable() {
    let codes = unreachable_codes(
        r#"
function f() {
    if (!false) {
        return;
    }
    console.log("reachable");
}
"#,
    );
    assert!(
        !codes.contains(&7027),
        "tsc does not fold a prefix `!` for this reachability rule; got {codes:?}"
    );
}

#[test]
fn if_double_negated_true_no_else_does_not_report_unreachable() {
    let codes = unreachable_codes(
        r#"
function f() {
    let x = 0;
    if (!!true) {
        x = 1;
        return;
    }
    if (!!true) {
        x = 2;
        throw 0;
    }
}
"#,
    );
    assert!(
        !codes.contains(&7027),
        "`!!true` is a prefix-unary node, not TrueKeyword — this is the shape \
         that regressed `tryCatchFinallyControlFlow` in the corpus; got {codes:?}"
    );
}

#[test]
fn while_parenthesized_true_does_not_swallow_following_code() {
    let codes = unreachable_codes(
        r#"
function f() {
    while ((true)) {
    }
    console.log("reachable");
}
"#,
    );
    assert!(
        !codes.contains(&7027),
        "the loop path shares `is_true_condition`, so `(true)` must not make \
         `while` non-completing either; got {codes:?}"
    );
}

#[test]
fn while_parenthesized_true_leaves_function_falling_off_the_end() {
    let codes = unreachable_codes(
        r#"
function f(): number {
    while ((true)) {
    }
}
"#,
    );
    assert!(
        codes.contains(&2355),
        "`while ((true))` is not an infinite loop for tsc, so the function can \
         fall off the end and must report TS2355; got {codes:?}"
    );
}

// `check_unreachable_condition_branch_with_request` used to force
// `has_reported_unreachable = true` and `allow_unreachable_code = Some(true)`
// before checking a constant-condition's dead branch, which unconditionally
// suppressed TS7027 for anything inside it. tsc still reports TS7027 for the
// dead branch itself (verified against typescript@7.0.2) — only code that
// stays reachable after the `if` completes must be spared. These cover the
// `else`-present shape (the no-else shape is above,
// `if_false_throw_no_else_reports_unreachable_inside_dead_branch`).

#[test]
fn if_true_else_dead_return_reports_unreachable_in_else_branch() {
    let codes = unreachable_codes(
        r#"
function f(x: boolean): number {
    if (true) {
        x = false;
    }
    else {
        return 1;
    }
}
"#,
    );
    assert!(
        codes.contains(&7027),
        "expected TS7027 on the dead `return` in the `else` of `if (true)`, got {codes:?}"
    );
}

#[test]
fn if_false_else_live_dead_then_reports_unreachable_in_then_branch() {
    let codes = unreachable_codes(
        r#"
function f(x: boolean): number {
    if (false) {
        return 1;
    }
    else {
        x = false;
    }
}
"#,
    );
    assert!(
        codes.contains(&7027),
        "expected TS7027 on the dead `return` in the `then` of `if (false)`, got {codes:?}"
    );
}

#[test]
fn if_true_else_dead_branch_inside_try_reports_unreachable() {
    // Adjacent case: the dead branch sits inside a `try`, matching the
    // `reachabilityChecks5.ts`/`6.ts` corpus shape (f8) where the `catch`'s
    // own `return` keeps the function from getting TS2355 instead of TS7030.
    let codes = unreachable_codes(
        r#"
function f(x: boolean): number {
    try {
        if (true) {
            x = false;
        }
        else {
            return 1;
        }
    }
    catch (e) {
        return 1;
    }
}
"#,
    );
    assert!(
        codes.contains(&7027),
        "expected TS7027 on the dead `return` inside a `try`'s `if (true) {{ }} else {{ }}`, got {codes:?}"
    );
}

#[test]
fn if_renamed_condition_true_else_dead_reports_unreachable() {
    // Adjacent case: renamed binders guard against any accidental name-based
    // suppression (Anti-Hardcoding Gate).
    let codes = unreachable_codes(
        r#"
function computeWidgetTotal(quantityAvailable: boolean): number {
    if (true) {
        quantityAvailable = false;
    }
    else {
        return 99;
    }
}
"#,
    );
    assert!(
        codes.contains(&7027),
        "expected TS7027 on the dead `else` branch regardless of binder names, got {codes:?}"
    );
}

#[test]
fn if_non_constant_condition_else_does_not_report_unreachable() {
    // Negative control: a non-constant condition never makes either branch
    // provably dead, so neither should be flagged.
    let codes = unreachable_codes(
        r#"
function f(cond: boolean, x: boolean): number {
    if (cond) {
        x = false;
    }
    else {
        return 1;
    }
}
"#,
    );
    assert!(
        !codes.contains(&7027),
        "did not expect TS7027 in either branch of a non-constant `if`/`else`, got {codes:?}"
    );
}

#[test]
fn while_true_still_suppresses_the_missing_return() {
    let codes = unreachable_codes(
        r#"
function f(): number {
    while (true) {
    }
}
"#,
    );
    assert!(
        !codes.contains(&2355),
        "the bare `while (true)` must stay non-completing — this is the \
         positive control for the narrowing; got {codes:?}"
    );
}
