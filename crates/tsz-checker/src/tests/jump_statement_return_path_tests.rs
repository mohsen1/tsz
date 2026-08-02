//! Tests for #16203: an illegal `break`/`continue` (one that draws TS1107 or
//! TS1115) must not suppress the enclosing function-like's return-path
//! analysis (TS2355/TS2378).
//!
//! Structural rule: `tsc`'s `checkGrammarBreakOrContinueStatement` is a
//! grammar pass, independent of the return-path analysis
//! (`check_function_return_paths`/`function_body_falls_through`). A jump
//! statement only terminates its enclosing control-flow region when its
//! target actually resolves inside the current function-like; an illegal
//! jump — one whose target lies outside the function-like, or whose label
//! doesn't wrap an iteration for `continue` — is not a body exit and must
//! not make the function's end point look unreachable.
//!
//! Oracle-pinned against `typescript@6.0.2` (`/opt/node22/.../typescript.js`),
//! `--noEmit --strict --pretty false --target es2022 --lib es2022`.

use crate::test_utils::check_source_diagnostics;

fn codes(src: &str) -> Vec<u32> {
    let mut codes: Vec<u32> = check_source_diagnostics(src)
        .into_iter()
        .map(|d| d.code)
        .collect();
    codes.sort_unstable();
    codes
}

#[test]
fn illegal_break_in_getter_still_reports_return_path() {
    // tsc: TS2378, TS1107.
    assert_eq!(codes("class A { get g() { break; } }"), vec![1107, 2378]);
}

#[test]
fn illegal_continue_in_getter_still_reports_return_path() {
    // tsc: TS2378, TS1107.
    assert_eq!(codes("class A { get g() { continue; } }"), vec![1107, 2378]);
}

#[test]
fn illegal_continue_in_annotated_getter_reports_ts2378_and_ts1107() {
    // tsc additionally reports TS2355 here (`class A { get g(): number {
    // continue; } }` -> 1107, 2355, 2378). tsz is missing TS2355 for EVERY
    // annotated getter whose body has no return at all, jump or not — e.g.
    // the jump-free control `class A { get g(): number { } }` is already
    // `tsc 2355,2378` / `tsz 2378` on current main. That is a separate,
    // pre-existing gap in the accessor-specific return-path check
    // (`ambient_signature_checks.rs`'s getter arm never falls into a TS2355
    // branch, only TS2378/TS2366), not something this fix touches — this
    // case is not a jump/reachability regression, so it is pinned as-is
    // rather than silently left uncovered.
    assert_eq!(
        codes("class A { get g(): number { continue; } }"),
        vec![1107, 2378]
    );
}

#[test]
fn illegal_continue_in_free_function_reports_ts2355() {
    // Not accessor-specific, not class-specific: tsc TS2355, TS1107.
    assert_eq!(
        codes("function f(): number { continue; }"),
        vec![1107, 2355]
    );
}

#[test]
fn illegal_continue_in_function_nested_in_outer_loop_ignores_outer_loop() {
    // The outer `while` is not `f`'s own loop; tsc still reports both.
    // (A class-member variant of this row also exists in tsc's matrix, but
    // depends on #16199/#16202's separate fix for TS1107 false negatives on
    // jumps nested under a class member *and* an outer loop — orthogonal to
    // this return-path fix, so this control stays on a free function, whose
    // TS1107 detection does not depend on that other fix.)
    assert_eq!(
        codes("while (true) { function f(): number { continue; } }"),
        vec![1107, 2355]
    );
}

#[test]
fn illegal_continue_to_non_iteration_label_also_reports_ts2355() {
    // TS1115 (label found, but doesn't wrap an iteration) is a distinct
    // grammar error from TS1107, but the same independence rule applies.
    assert_eq!(
        codes("function f(): number { outer: switch (1) { case 1: continue outer; } }"),
        vec![1115, 2355]
    );
}

#[test]
fn illegal_continue_in_switch_reports_ts2355() {
    // Unlabeled continue inside a switch with no enclosing loop: TS1107.
    assert_eq!(
        codes("function f(): number { switch (1) { case 1: continue; } }"),
        vec![1107, 2355]
    );
}

// --- Controls: bodies without a jump still behave as before. ---

#[test]
fn getter_with_empty_body_reports_return_path_only() {
    assert_eq!(codes("class A { get g() { } }"), vec![2378]);
}

#[test]
fn getter_with_debugger_statement_reports_return_path_only() {
    // Proves the suppression was specific to the jump, not "any statement".
    assert_eq!(codes("class A { get g() { debugger; } }"), vec![2378]);
}

#[test]
fn getter_that_always_throws_is_clean() {
    assert_eq!(codes("class A { get g() { throw 1; } }"), Vec::<u32>::new());
}

// --- Controls: a *legal* jump target still terminates the region normally. ---

#[test]
fn legal_break_in_own_loop_still_falls_through_to_ts2355() {
    // The `break` legally exits the `while`, then the function body falls
    // off the end: tsc reports only TS2355, no TS1107.
    assert_eq!(
        codes("function f(): number { while (true) { break; } }"),
        vec![2355]
    );
}

#[test]
fn legal_labeled_break_still_falls_through_to_ts2355() {
    assert_eq!(
        codes("function f(): number { outer: while (true) { break outer; } }"),
        vec![2355]
    );
}

#[test]
fn legal_break_in_for_loop_still_falls_through_to_ts2355() {
    assert_eq!(
        codes("function f(): number { for (;;) { break; } }"),
        vec![2355]
    );
}

#[test]
fn legal_continue_in_infinite_loop_stays_unreachable_and_clean() {
    // No `break`, so the `while (true)` never exits: the method body's end
    // point is genuinely unreachable and tsc reports nothing.
    assert_eq!(
        codes("class A { m() { while (true) { continue; } } }"),
        Vec::<u32>::new()
    );
}
