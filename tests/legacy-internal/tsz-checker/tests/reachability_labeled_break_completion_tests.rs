//! Regression coverage for `statement_falls_through`'s `LABELED_STATEMENT`
//! arm and `loop_falls_through`'s break-target check, covering the
//! `reachabilityChecks5.ts`/`6.ts` f11 shape left open by #17309: a
//! `break <label>` where `<label>` names an enclosing non-iteration
//! statement (a `try`, a bare block, ...), reachable only through one or
//! more nested loops.
//!
//! Before this fix, `contains_break_statement` (used by both
//! `loop_falls_through` and the old `LABELED_STATEMENT` arm) never checked
//! which construct a `break` actually targets and never recursed into a
//! nested loop's own body — so a labeled break several loops deep was
//! invisible to an outer loop's fall-through check, and a `LABELED_STATEMENT`
//! wrapping something other than a loop/switch had no way to notice an
//! escaping break at all. `contains_break_targeting` replaces both uses by
//! resolving each break's real target (mirroring
//! `jump_statement_has_legal_target`'s own structural walk) and comparing it
//! — through any stack of labels — against the construct being asked about.
//!
//! `switch_falls_through`'s clause-completion check had the exact same bug
//! (still calling the untargeted `contains_break_statement` after the fixes
//! above landed): a labeled `break` reachable from a `case`/`default` clause
//! but resolving to some *other* construct (a label on a block that merely
//! wraps the switch, not the switch itself) was wrongly counted as "this
//! clause completes the switch." It now shares `contains_break_targeting`
//! with the loop and labeled-statement checks above.

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

fn reachability_codes(source: &str) -> Vec<u32> {
    let diagnostics = check_source(
        source,
        "test.ts",
        CheckerOptions {
            allow_unreachable_code: Some(false),
            no_implicit_returns: true,
            ..CheckerOptions::default()
        },
    );
    diagnostics.iter().map(|diag| diag.code).collect()
}

/// The `reachabilityChecks5.ts`/`6.ts` f11 shape: `break test;` deep inside
/// two nested `do...while(true)` loops targets a label on the enclosing
/// `try`, not either loop. tsc (verified against typescript@7.0.2): the
/// function falls off the end (TS7030) and the statement following the
/// inner loop is unreachable (TS7027) — both asserted together since they
/// share one root cause.
#[test]
fn break_to_label_on_try_falls_through_and_makes_code_after_inner_loop_unreachable() {
    let codes = reachability_codes(
        r#"
function f(x: boolean) {
    test:
    try {
        do {
            do {
                break test;
            } while (true);
            x = false;
        } while (true);
    }
    catch (e) {
        return 1;
    }
}
"#,
    );
    assert!(
        codes.contains(&7030),
        "expected TS7030: `break test` exits the whole labeled `try`, so `f` \
         can fall off the end, got {codes:?}"
    );
    assert!(
        codes.contains(&7027),
        "expected TS7027 on `x = false`: the inner `do...while` never \
         completes normally (its only reachable break targets the outer \
         `test:` label, not this loop), got {codes:?}"
    );
}

/// Renamed-binder adjacent case (Anti-Hardcoding Gate): same shape,
/// different identifiers throughout.
#[test]
fn break_to_label_on_try_renamed_binders_still_reports_both() {
    let codes = reachability_codes(
        r#"
function checkOrder(shouldRetry: boolean) {
    outerRetry:
    try {
        do {
            do {
                break outerRetry;
            } while (true);
            shouldRetry = false;
        } while (true);
    }
    catch (err) {
        return 1;
    }
}
"#,
    );
    assert!(
        codes.contains(&7030),
        "expected TS7030 regardless of binder names, got {codes:?}"
    );
    assert!(
        codes.contains(&7027),
        "expected TS7027 regardless of binder names, got {codes:?}"
    );
}

/// A label directly on the loop it breaks (no intervening `try`/block) must
/// keep working exactly as before this change: `break outer` still exits
/// `outer`'s own loop, so `f` falls off the end. Declared `number` return
/// type reports TS2355, not TS7030.
#[test]
fn break_to_label_directly_on_loop_reports_ts2355() {
    let codes = reachability_codes(
        r#"
function f(): number {
    outer: while (true) {
        break outer;
    }
}
"#,
    );
    assert!(
        codes.contains(&2355),
        "expected TS2355: a label directly on the loop it breaks behaves \
         exactly like an unlabeled break of that loop, got {codes:?}"
    );
}

/// Stacked labels (`a: b: while (true) { break a; }`): the outer label's
/// break must still resolve through the inner label down to the loop it
/// wraps, exercising `innermost_labeled_target`'s unwrap loop.
#[test]
fn break_to_outer_of_two_stacked_labels_reports_ts2355() {
    let codes = reachability_codes(
        r#"
function f(): number {
    a: b: while (true) {
        break a;
    }
}
"#,
    );
    assert!(
        codes.contains(&2355),
        "expected TS2355: `break a` through a stacked `b:` label still \
         exits the loop both labels wrap, got {codes:?}"
    );
}

/// A label on a bare block (not a loop, not a `try`) wrapping two nested
/// loops: `break outer` from the innermost loop must still be visible to
/// the block's own fall-through check, leaving the code after the block
/// reachable.
#[test]
fn break_to_label_on_block_through_nested_loops_leaves_code_after_reachable() {
    let codes = reachability_codes(
        r#"
function f() {
    outer: {
        while (true) {
            while (true) {
                break outer;
            }
        }
    }
    console.log("reachable");
}
"#,
    );
    assert!(
        !codes.contains(&7027),
        "expected no TS7027: `break outer` two loops deep still exits the \
         labeled block, so the `console.log` after it is reachable, got {codes:?}"
    );
}

/// Negative control: an *unlabeled* `break` stays local to its nearest
/// enclosing loop even when reached through `contains_break_targeting`'s
/// now-unrestricted recursion into nested loops — it must not leak out to
/// satisfy an outer loop's own break-target check.
#[test]
fn unlabeled_break_in_nested_loop_does_not_escape_the_outer_loop() {
    let codes = reachability_codes(
        r#"
function f() {
    while (true) {
        while (true) {
            break;
        }
        console.log("inner-exit");
    }
    console.log("after-outer");
}
"#,
    );
    assert!(
        codes.contains(&7027),
        "expected TS7027 on the final `console.log`: the unlabeled `break` \
         only exits the inner loop, so the outer `while (true)` still has no \
         break targeting it and never completes, got {codes:?}"
    );
}

/// `switch_falls_through` adjacent case: a `break` inside a `case` clause
/// that targets an outer label wrapping the switch (not the switch itself)
/// must not be mistaken for "this clause completes the switch normally."
/// Oracle-verified against `typescript@7.0.2`: `return 1;` is unreachable
/// (TS7027) because neither clause completes the switch — case 1's `break
/// outer` exits the whole labeled block, skipping past `return 1;`, and the
/// `default` clause always throws.
#[test]
fn break_to_label_wrapping_switch_does_not_complete_the_switch_clause() {
    let codes = reachability_codes(
        r#"
function f(x: number) {
    outer: {
        switch (x) {
            case 1:
                break outer;
            default:
                throw new Error();
        }
        return 1;
    }
    return 2;
}
"#,
    );
    assert!(
        codes.contains(&7027),
        "expected TS7027 on `return 1;`: neither switch clause completes the \
         switch itself, so it is unreachable, got {codes:?}"
    );
}

/// Positive control: an unlabeled `break` that targets the switch itself
/// must keep completing it normally — this is the existing, common case the
/// fix above must not regress.
#[test]
fn unlabeled_break_still_completes_its_own_switch_clause() {
    let codes = reachability_codes(
        r#"
function f(x: number) {
    switch (x) {
        case 1:
            break;
        default:
            throw new Error();
    }
    console.log("reachable");
}
"#,
    );
    assert!(
        !codes.contains(&7027),
        "did not expect TS7027: an unlabeled `break` in `case 1` completes \
         the switch normally, so `console.log` after it stays reachable, \
         got {codes:?}"
    );
}
