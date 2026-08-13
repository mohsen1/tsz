//! Regression tests for issue #16203: an illegal `break`/`continue` (one that
//! draws TS1104/TS1105/TS1107) must not suppress the enclosing function's
//! return-path analysis (TS2355/TS2378).
//!
//! Structural rule: tsc's binder only treats a `break`/`continue` as a flow
//! terminator when it has a resolved jump target (`bindBreakOrContinueFlow`
//! no-ops when `breakTarget`/`continueTarget` is unset); the grammar check
//! that reports the illegal-jump diagnostic is an independent pass. tsz's
//! `statement_falls_through` (crates/tsz-checker/src/flow/reachability_checker.rs)
//! previously treated every `break`/`continue` as an unconditional terminator,
//! so the fabricated flow-graph made the function body's end point look
//! unreachable and skipped TS2355/TS2378 whenever the jump was illegal.

use crate::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    let mut codes: Vec<u32> = check_source_diagnostics(source)
        .iter()
        .map(|d| d.code)
        .collect();
    codes.sort_unstable();
    codes
}

// ── Illegal unlabeled jump still requires a return path ──

#[test]
fn free_function_illegal_continue_reports_jump_and_return_path() {
    assert_eq!(
        codes("function f(): number { continue; }"),
        vec![1107, 2355]
    );
}

#[test]
fn free_function_illegal_break_reports_jump_and_return_path() {
    assert_eq!(codes("function f(): number { break; }"), vec![1107, 2355]);
}

#[test]
fn class_getter_illegal_continue_reports_jump_and_return_path() {
    assert_eq!(
        codes("class A { get g(): number { continue; } }"),
        vec![1107, 2355, 2378]
    );
}

#[test]
fn class_getter_illegal_break_without_annotation_reports_jump_and_return_path() {
    assert_eq!(codes("class A { get g() { break; } }"), vec![1107, 2378]);
}

#[test]
fn illegal_jump_nested_in_plain_block_still_reports_jump_and_return_path() {
    // The recursive block/statement walk that computes fall-through must
    // resolve the jump's legal target from the jump node itself, not from
    // the depth it happens to be nested at.
    assert_eq!(
        codes("function f(): number { { continue; } }"),
        vec![1107, 2355]
    );
}

// ── Controls: legal jumps still terminate flow correctly ──

#[test]
fn legal_break_in_loop_still_requires_return_path() {
    // The loop's own reachable exit (condition false) still needs a return,
    // so TS2355 must fire — but from the loop falling through, not the break.
    assert_eq!(
        codes("function f(): number { while (true) { break; } }"),
        vec![2355]
    );
}

#[test]
fn legal_labeled_break_out_of_block_still_requires_return_path() {
    // Corrected expectation (was `Vec::<u32>::new()`, asserting NO
    // diagnostic): verified against typescript@7.0.2, tsc reports TS2355
    // here. `break foo;` resumes right after the labeled block — the end of
    // `f`'s body — so the declared `number` return type still needs a
    // return path. The old assertion predates
    // `contains_break_targeting`/the `LABELED_STATEMENT` fall-through fix
    // (reachabilityChecks5.ts/6.ts f11): `statement_falls_through`'s old
    // `LABELED_STATEMENT` arm only ever delegated to the wrapped statement's
    // own fall-through and had no way to notice an escaping break, so this
    // exact shape silently swallowed TS2355 too.
    assert_eq!(
        codes("function f(): number { foo: { break foo; } }"),
        vec![2355]
    );
}

#[test]
fn empty_getter_body_control_unaffected_by_jump_handling() {
    assert_eq!(codes("class A { get g() { } }"), vec![2378]);
}

#[test]
fn illegal_jump_inside_outer_loop_across_class_boundary_still_reports_jump_and_return_path() {
    // The jump is illegal because it targets a loop outside the function-like
    // boundary the getter creates, not because there is no loop in scope at
    // all — this depends on #16202's class-member-body control-flow reset.
    assert_eq!(
        codes("while (true) { class A { get g() { continue; } } }"),
        vec![1107, 2378]
    );
}
