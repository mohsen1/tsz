//! Regression coverage for #16199: an unlabeled `break`/`continue` inside a
//! class **method, accessor or constructor** that is itself nested in a loop or
//! `switch` reported nothing, where `tsc` reports TS1107.
//!
//! `tsc`'s `checkGrammarBreakOrContinueStatement` is a single upward walk from
//! the jump node that stops at the first `isFunctionLikeOrClassStaticBlockDeclaration`
//! ancestor, so an unlabeled jump can only see iteration and `switch` statements
//! declared *inside its own innermost function-like*. tsz models that walk as
//! the ambient `iteration_depth`/`switch_depth` counters on `CheckerContext`,
//! which therefore have to be zeroed wherever checking descends across such a
//! boundary. That reset used to be hand-copied at each boundary, and the class
//! member-body path had exactly one copy — on the **static block** arm — so
//! every other member kind kept seeing the enclosing loop and stayed silent.
//!
//! The reset now rides along with `CheckerContext::enter_class_member_body`,
//! which all four member kinds already went through, so the hole cannot reopen
//! for a member kind one at a time.
//!
//! Every witness below was pinned against `typescript@7.0.2`
//! (`--noEmit --strict --target es2022 --lib es2022`). Binder names are varied
//! across the matrix on purpose: this family is decided structurally, by the
//! member-body boundary, never by any name.

use crate::test_utils::check_source_codes;

/// TS1107: jump target cannot cross function boundary.
const TS1107: u32 = 1107;
/// TS1105: `break` outside any iteration or switch statement.
const TS1105: u32 = 1105;
/// TS1104: `continue` outside any iteration statement.
const TS1104: u32 = 1104;

// ---------------------------------------------------------------------------
// The four reported false negatives (#16199's witness table).
// ---------------------------------------------------------------------------

#[test]
fn break_in_method_nested_in_while_reports_ts1107() {
    let codes = check_source_codes("while (true) { class C { m() { break; } } }");
    assert!(
        codes.contains(&TS1107),
        "the method body is a function boundary the enclosing `while` cannot be jumped out of; got {codes:?}"
    );
}

#[test]
fn continue_in_getter_nested_in_while_reports_ts1107() {
    let codes = check_source_codes("while (true) { class C { get g() { continue; } } }");
    assert!(codes.contains(&TS1107), "got {codes:?}");
}

#[test]
fn break_in_method_nested_in_switch_reports_ts1107() {
    let codes = check_source_codes("switch (1) { case 1: class C { m() { break; } } }");
    assert!(
        codes.contains(&TS1107),
        "a `switch` is a boundary for unlabeled `break` exactly like an iteration statement; got {codes:?}"
    );
}

#[test]
fn break_in_constructor_nested_in_while_reports_ts1107() {
    let codes = check_source_codes("while (true) { class C { constructor() { break; } } }");
    assert!(codes.contains(&TS1107), "got {codes:?}");
}

// ---------------------------------------------------------------------------
// Adjacent member kinds: setter, static method, class expression, generator,
// async method. Renamed binders throughout.
// ---------------------------------------------------------------------------

#[test]
fn continue_in_setter_nested_in_while_reports_ts1107() {
    let codes =
        check_source_codes("while (true) { class Renamed { set s(v: number) { continue; } } }");
    assert!(codes.contains(&TS1107), "got {codes:?}");
}

#[test]
fn break_in_static_method_nested_in_for_of_reports_ts1107() {
    let codes = check_source_codes("for (const q of [1]) { class Zed { static sm() { break; } } }");
    assert!(codes.contains(&TS1107), "got {codes:?}");
}

#[test]
fn continue_in_method_nested_in_do_while_reports_ts1107() {
    let codes = check_source_codes("do { class Q { m() { continue; } } } while (false)");
    assert!(codes.contains(&TS1107), "got {codes:?}");
}

#[test]
fn break_in_class_expression_method_reports_ts1107() {
    let codes = check_source_codes("while (true) { const O = class { m() { break; } }; }");
    assert!(
        codes.contains(&TS1107),
        "an anonymous class expression's method body is the same boundary as a declaration's; got {codes:?}"
    );
}

#[test]
fn break_in_async_method_nested_in_while_reports_ts1107() {
    let codes = check_source_codes("while (true) { class C { async m() { break; } } }");
    assert!(codes.contains(&TS1107), "got {codes:?}");
}

#[test]
fn break_in_generator_method_nested_in_while_reports_ts1107() {
    let codes = check_source_codes("while (true) { class C { *m() { break; } } }");
    assert!(codes.contains(&TS1107), "got {codes:?}");
}

// ---------------------------------------------------------------------------
// Nesting: a class inside a method, a member body inside a free function.
// ---------------------------------------------------------------------------

#[test]
fn break_in_method_of_class_nested_in_method_reports_ts1107() {
    let codes =
        check_source_codes("while (true) { class C { m() { class D { n() { break; } } } } }");
    assert!(
        codes.contains(&TS1107),
        "two stacked member boundaries must not cancel out; got {codes:?}"
    );
}

#[test]
fn break_in_method_nested_in_loop_inside_function_reports_ts1107() {
    let codes = check_source_codes("function f() { while (true) { class C { m() { break; } } } }");
    assert!(codes.contains(&TS1107), "got {codes:?}");
}

#[test]
fn break_in_function_declared_in_method_reports_ts1107() {
    let codes =
        check_source_codes("while (true) { class C { m() { function inner() { break; } } } }");
    assert!(codes.contains(&TS1107), "got {codes:?}");
}

#[test]
fn break_in_arrow_declared_in_method_reports_ts1107() {
    let codes =
        check_source_codes("while (true) { class C { m() { const f = () => { break; }; } } }");
    assert!(codes.contains(&TS1107), "got {codes:?}");
}

#[test]
fn break_in_property_initializer_arrow_reports_ts1107() {
    let codes = check_source_codes("while (true) { class C { p = () => { break; }; } }");
    assert!(codes.contains(&TS1107), "got {codes:?}");
}

// ---------------------------------------------------------------------------
// Negative / fallback cases. The rule is "reset the counters at the member-body
// boundary", NOT "always error inside a class member" — a loop the member owns
// still satisfies the jump, and these must stay silent.
// ---------------------------------------------------------------------------

#[test]
fn break_in_members_own_while_stays_clean() {
    let codes = check_source_codes("while (true) { class C { m() { while (true) { break; } } } }");
    assert!(
        !codes.contains(&TS1107) && !codes.contains(&TS1105),
        "the member's own loop is inside the same function-like, so the jump is legal; got {codes:?}"
    );
}

#[test]
fn break_in_members_own_switch_stays_clean() {
    let codes =
        check_source_codes("while (true) { class C { m() { switch (1) { case 1: break; } } } }");
    assert!(
        !codes.contains(&TS1107) && !codes.contains(&TS1105),
        "got {codes:?}"
    );
}

#[test]
fn continue_in_members_own_for_stays_clean() {
    let codes = check_source_codes("while (true) { class C { m() { for (;;) { continue; } } } }");
    assert!(
        !codes.contains(&TS1107) && !codes.contains(&TS1104),
        "got {codes:?}"
    );
}

#[test]
fn continue_in_members_own_do_while_stays_clean() {
    let codes =
        check_source_codes("while (true) { class C { m() { do { continue; } while (0); } } }");
    assert!(
        !codes.contains(&TS1107) && !codes.contains(&TS1104),
        "got {codes:?}"
    );
}

#[test]
fn break_in_constructors_own_while_stays_clean() {
    let codes =
        check_source_codes("while (true) { class C { constructor() { while (1) { break; } } } }");
    assert!(
        !codes.contains(&TS1107) && !codes.contains(&TS1105),
        "got {codes:?}"
    );
}

#[test]
fn break_in_static_blocks_own_while_stays_clean() {
    let codes = check_source_codes("while (true) { class C { static { while (1) { break; } } } }");
    assert!(
        !codes.contains(&TS1107) && !codes.contains(&TS1105),
        "got {codes:?}"
    );
}

#[test]
fn labeled_break_to_members_own_label_stays_clean() {
    let codes =
        check_source_codes("while (true) { class C { m() { l: while (1) { break l; } } } }");
    assert!(
        !codes.contains(&TS1107),
        "the label is declared inside the member body, so nothing is crossed; got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Controls that already passed before the fix, pinned so the member-body reset
// cannot regress them. The labeled paths decide TS1107 by comparing the label's
// `function_depth` against the live one, so outer labels must stay resolvable
// for the whole member body — the boundary saves the label stack's length, it
// does not hide its contents.
// ---------------------------------------------------------------------------

#[test]
fn labeled_break_across_member_boundary_still_reports_ts1107() {
    let codes = check_source_codes("a: while (true) { class C { m() { break a; } } }");
    assert!(codes.contains(&TS1107), "got {codes:?}");
}

#[test]
fn labeled_continue_across_member_boundary_still_reports_ts1107() {
    let codes = check_source_codes("outer: while (true) { class C { m() { continue outer; } } }");
    assert!(codes.contains(&TS1107), "got {codes:?}");
}

#[test]
fn break_in_static_block_nested_in_while_still_reports_ts1107() {
    let codes = check_source_codes("while (true) { class C { static { break; } } }");
    assert!(
        codes.contains(&TS1107),
        "the static block arm was the one member kind that already had the reset; got {codes:?}"
    );
}

#[test]
fn break_in_method_with_no_enclosing_loop_still_reports_ts1107() {
    let codes = check_source_codes("class C { m() { break; } }");
    assert!(
        codes.contains(&TS1107),
        "TS1107 wins over TS1105 inside a member body because the body raises function_depth; got {codes:?}"
    );
}

#[test]
fn continue_in_method_with_no_enclosing_loop_still_reports_ts1107() {
    let codes = check_source_codes("class C { m() { continue; } }");
    assert!(codes.contains(&TS1107), "got {codes:?}");
}

// ---------------------------------------------------------------------------
// Top-level fallback: outside any function-like the codes are TS1105/TS1104,
// not TS1107. The member-body boundary must not leak its raised depth.
// ---------------------------------------------------------------------------

#[test]
fn top_level_break_after_a_class_member_still_reports_ts1105() {
    let codes = check_source_codes("class C { m() { while (1) {} } }\nbreak;");
    assert!(
        codes.contains(&TS1105) && !codes.contains(&TS1107),
        "the member body's raised depth and loop counters must be fully restored on exit; got {codes:?}"
    );
}

#[test]
fn top_level_continue_after_a_class_member_still_reports_ts1104() {
    let codes = check_source_codes("class C { m() { for (;;) {} } }\ncontinue;");
    assert!(
        codes.contains(&TS1104) && !codes.contains(&TS1107),
        "got {codes:?}"
    );
}

#[test]
fn break_in_loop_after_a_class_member_stays_clean() {
    let codes = check_source_codes("while (true) { class C { m() { while (1) {} } } break; }");
    assert!(
        !codes.contains(&TS1107) && !codes.contains(&TS1105),
        "the enclosing loop's counters must be restored after the member body, not left at zero; got {codes:?}"
    );
}
