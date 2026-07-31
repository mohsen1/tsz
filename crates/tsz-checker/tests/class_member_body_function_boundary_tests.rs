//! A class member body is a function boundary.
//!
//! `tsc` treats the body of a class **method**, **constructor**, **get
//! accessor**, **set accessor** and **static block** as a function boundary for
//! every grammar and name-resolution decision that asks "am I at the top level
//! of a file?":
//!
//! | construct | inside a class member body | at file top level |
//! | --- | --- | --- |
//! | `await x` | TS1308 | TS1375 / TS1378 |
//! | `await using x = …` | TS2852 | TS2853 / TS2854 |
//! | `await(x)` in a script | TS2311 | TS1375 / TS1378 |
//! | `break;` / `continue;` | TS1107 | TS1105 / TS1104 |
//!
//! tsz raised `ctx.function_depth` for free function bodies and static blocks
//! but not for method/constructor/accessor bodies, so every one of those rows
//! answered with the *top-level* diagnostic instead. The fix routes all four
//! member-body entries through `enter_class_member_body`.
//!
//! The one check that needs the narrower "directly in this member body"
//! question is the abstract-property-access family (TS2715): `tsc` reports
//! `this.abstractProp` in a constructor body but **not** inside a function
//! nested in that constructor. Those readers compare against the member-body
//! baseline rather than absolute depth 0, and the last section pins both
//! directions.
//!
//! Every expectation below was taken from `tsc@7.0.2`
//! (`--noEmit --strict --pretty false --target es2022 --module esnext`).
//! The rule keys only on syntactic nesting, so binder spellings are varied in
//! the renamed-binder controls and never drive a decision.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;
use tsz_common::common::{ModuleKind, ScriptTarget};

/// Diagnostic codes for `source` under a module/target that supports
/// top-level `await`, so the top-level branch is distinguishable by code
/// rather than by count.
fn codes(source: &str) -> Vec<u32> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            module: ModuleKind::ESNext,
            target: ScriptTarget::ES2022,
            ..CheckerOptions::default()
        },
    )
    .iter()
    .map(|d| d.code)
    .collect()
}

fn has(source: &str, code: u32) -> bool {
    codes(source).contains(&code)
}

/// Count occurrences so a cardinality claim is not satisfied by membership
/// alone.
fn count(source: &str, code: u32) -> usize {
    codes(source).iter().filter(|c| **c == code).count()
}

const AWAIT_OUTSIDE_ASYNC: u32 = 1308;
const TOP_LEVEL_AWAIT_NEEDS_MODULE: u32 = 1375;
const TOP_LEVEL_AWAIT_NEEDS_OPTIONS: u32 = 1378;
const AWAIT_USING_OUTSIDE_ASYNC: u32 = 2852;
const TOP_LEVEL_AWAIT_USING_NEEDS_MODULE: u32 = 2853;
const JUMP_CROSSES_FUNCTION_BOUNDARY: u32 = 1107;
const BREAK_NEEDS_ENCLOSING_ITERATION: u32 = 1105;
const CONTINUE_NEEDS_ENCLOSING_ITERATION: u32 = 1104;
const ABSTRACT_PROPERTY_IN_CONSTRUCTOR: u32 = 2715;

// ---------------------------------------------------------------------------
// `await` in a class member body — TS1308, not the top-level pair.
// ---------------------------------------------------------------------------

#[test]
fn await_in_method_body_reports_ts1308() {
    let source = "class K { m() { await 1; } }";
    assert!(
        has(source, AWAIT_OUTSIDE_ASYNC),
        "codes: {:?}",
        codes(source)
    );
    assert!(!has(source, TOP_LEVEL_AWAIT_NEEDS_MODULE));
    assert!(!has(source, TOP_LEVEL_AWAIT_NEEDS_OPTIONS));
}

#[test]
fn await_in_constructor_body_reports_ts1308() {
    let source = "class K { constructor() { await 1; } }";
    assert!(
        has(source, AWAIT_OUTSIDE_ASYNC),
        "codes: {:?}",
        codes(source)
    );
    assert!(!has(source, TOP_LEVEL_AWAIT_NEEDS_MODULE));
}

#[test]
fn await_in_get_accessor_body_reports_ts1308() {
    let source = "class K { get g(): number { await 1; return 1; } }";
    assert!(
        has(source, AWAIT_OUTSIDE_ASYNC),
        "codes: {:?}",
        codes(source)
    );
    assert!(!has(source, TOP_LEVEL_AWAIT_NEEDS_MODULE));
}

#[test]
fn await_in_set_accessor_body_reports_ts1308() {
    let source = "class K { set s(v: number) { await 1; } }";
    assert!(
        has(source, AWAIT_OUTSIDE_ASYNC),
        "codes: {:?}",
        codes(source)
    );
    assert!(!has(source, TOP_LEVEL_AWAIT_NEEDS_MODULE));
}

/// The member-body rule is syntactic: renaming the class, the method and the
/// awaited binding changes nothing.
#[test]
fn await_in_renamed_member_body_reports_ts1308() {
    let source = "const zzz = 1; class QqqWidget { performOperation() { await zzz; } }";
    assert!(
        has(source, AWAIT_OUTSIDE_ASYNC),
        "codes: {:?}",
        codes(source)
    );
    assert!(!has(source, TOP_LEVEL_AWAIT_NEEDS_MODULE));
}

/// A class *expression*'s member body is the same boundary as a declaration's.
#[test]
fn await_in_class_expression_method_body_reports_ts1308() {
    let source = "const C = class { m() { await 1; } };";
    assert!(
        has(source, AWAIT_OUTSIDE_ASYNC),
        "codes: {:?}",
        codes(source)
    );
    assert!(!has(source, TOP_LEVEL_AWAIT_NEEDS_MODULE));
}

// ---------------------------------------------------------------------------
// Sibling `await` forms take the same branch.
//
// `for await` is deliberately absent: tsz never emits TS1103 from any
// non-async function body — free functions included — so a member body cannot
// be made to answer it by fixing the boundary alone. Filed separately.
// ---------------------------------------------------------------------------

#[test]
fn await_using_in_method_body_reports_ts2852() {
    let source = "class K { m() { await using x = null as any; } }";
    assert!(
        has(source, AWAIT_USING_OUTSIDE_ASYNC),
        "codes: {:?}",
        codes(source)
    );
    assert!(!has(source, TOP_LEVEL_AWAIT_USING_NEEDS_MODULE));
}

// ---------------------------------------------------------------------------
// `break` / `continue` cross a function boundary out of a member body.
// ---------------------------------------------------------------------------

#[test]
fn unlabeled_break_in_method_body_reports_ts1107() {
    let source = "class K { m() { break; } }";
    assert!(
        has(source, JUMP_CROSSES_FUNCTION_BOUNDARY),
        "codes: {:?}",
        codes(source)
    );
    assert!(!has(source, BREAK_NEEDS_ENCLOSING_ITERATION));
}

#[test]
fn unlabeled_continue_in_method_body_reports_ts1107() {
    let source = "class K { m() { continue; } }";
    assert!(
        has(source, JUMP_CROSSES_FUNCTION_BOUNDARY),
        "codes: {:?}",
        codes(source)
    );
    assert!(!has(source, CONTINUE_NEEDS_ENCLOSING_ITERATION));
}

#[test]
fn unlabeled_break_in_constructor_body_reports_ts1107() {
    let source = "class K { constructor() { break; } }";
    assert!(
        has(source, JUMP_CROSSES_FUNCTION_BOUNDARY),
        "codes: {:?}",
        codes(source)
    );
}

/// A `break` that *does* have a target inside the same member body stays
/// clean — the boundary must not turn well-formed jumps into errors.
#[test]
fn labeled_break_within_member_body_stays_clean() {
    let source = "class K { m() { outer: for (;;) { break outer; } } }";
    assert!(
        !has(source, JUMP_CROSSES_FUNCTION_BOUNDARY),
        "codes: {:?}",
        codes(source)
    );
    assert!(!has(source, BREAK_NEEDS_ENCLOSING_ITERATION));
}

#[test]
fn break_inside_member_body_loop_stays_clean() {
    let source = "class K { m() { for (;;) { break; } } }";
    assert!(
        !has(source, JUMP_CROSSES_FUNCTION_BOUNDARY),
        "codes: {:?}",
        codes(source)
    );
}

// ---------------------------------------------------------------------------
// Controls that were already correct and must stay correct.
// ---------------------------------------------------------------------------

/// An object-literal method body was already a function body on the generic
/// path — it never went through the class-member entry.
#[test]
fn await_in_object_literal_method_reports_ts1308() {
    let source = "const o = { m() { await 1; } };";
    assert!(
        has(source, AWAIT_OUTSIDE_ASYNC),
        "codes: {:?}",
        codes(source)
    );
}

/// A plain function nested in a method body: already TS1308 before the fix,
/// and still exactly one diagnostic after it — the two nested boundaries must
/// not report twice.
#[test]
fn await_in_function_nested_in_method_reports_one_ts1308() {
    let source = "class K { m() { function inner() { await 1; } } }";
    assert_eq!(
        count(source, AWAIT_OUTSIDE_ASYNC),
        1,
        "codes: {:?}",
        codes(source)
    );
}

/// File top level is unchanged: still the top-level pair, not TS1308.
#[test]
fn await_at_file_top_level_still_reports_the_top_level_pair() {
    let source = "await 1;";
    assert!(
        has(source, TOP_LEVEL_AWAIT_NEEDS_MODULE),
        "codes: {:?}",
        codes(source)
    );
    assert!(!has(source, AWAIT_OUTSIDE_ASYNC));
}

#[test]
fn break_at_file_top_level_still_reports_ts1105() {
    let source = "break;";
    assert!(
        has(source, BREAK_NEEDS_ENCLOSING_ITERATION),
        "codes: {:?}",
        codes(source)
    );
    assert!(!has(source, JUMP_CROSSES_FUNCTION_BOUNDARY));
}

/// An `async` member body has no grammar error at all — the boundary changes
/// which diagnostic a *non-async* body gets, never whether a legal `await`
/// becomes illegal.
#[test]
fn await_in_async_method_body_stays_clean() {
    let source = "class K { async m() { await 1; } }";
    assert!(
        !has(source, AWAIT_OUTSIDE_ASYNC),
        "codes: {:?}",
        codes(source)
    );
    assert!(!has(source, TOP_LEVEL_AWAIT_NEEDS_MODULE));
}

#[test]
fn await_in_async_constructor_adjacent_accessor_stays_clean() {
    let source = "class K { async m() { await 1; } get g(): number { return 1; } }";
    assert!(
        !has(source, AWAIT_OUTSIDE_ASYNC),
        "codes: {:?}",
        codes(source)
    );
}

// ---------------------------------------------------------------------------
// TS2715 keeps the narrower "directly in this member body" question.
// ---------------------------------------------------------------------------

/// Positive: `this.p` directly in the constructor body still reports TS2715.
/// This is the check that a blind `function_depth` bump would have silenced.
#[test]
fn abstract_property_read_in_constructor_still_reports_ts2715() {
    let source = "abstract class A { abstract p: number; }\n\
                  class B extends A { constructor() { super(); this.p; } }";
    assert!(
        has(source, ABSTRACT_PROPERTY_IN_CONSTRUCTOR),
        "codes: {:?}",
        codes(source)
    );
}

/// Same, through a binding-pattern destructure of `this`.
#[test]
fn abstract_property_destructured_from_this_in_constructor_still_reports_ts2715() {
    let source = "abstract class A { abstract p: number; }\n\
                  class B extends A { constructor() { super(); const { p } = this; } }";
    assert!(
        has(source, ABSTRACT_PROPERTY_IN_CONSTRUCTOR),
        "codes: {:?}",
        codes(source)
    );
}

/// Renamed-binder control for the positive case.
#[test]
fn abstract_property_read_in_constructor_renamed_binders_reports_ts2715() {
    let source = "abstract class ShapeBase { abstract areaValue: number; }\n\
                  class Square extends ShapeBase { constructor() { super(); this.areaValue; } }";
    assert!(
        has(source, ABSTRACT_PROPERTY_IN_CONSTRUCTOR),
        "codes: {:?}",
        codes(source)
    );
}

/// Negative: a function *nested* inside the constructor is past the member
/// body baseline, and `tsc` reports nothing there. This is the direction that
/// `function_depth == 0` used to protect and that the baseline comparison now
/// protects.
#[test]
fn abstract_property_read_in_function_nested_in_constructor_stays_clean() {
    let source = "abstract class A { abstract p: number; }\n\
                  class B extends A { constructor() { super(); function f(this: B) { this.p; } } }";
    assert!(
        !has(source, ABSTRACT_PROPERTY_IN_CONSTRUCTOR),
        "codes: {:?}",
        codes(source)
    );
}

/// An instance property initializer is checked at the class body's own depth,
/// not inside a member body, so it must keep reporting TS2715.
#[test]
fn abstract_property_read_in_instance_property_initializer_reports_ts2715() {
    let source = "abstract class A { abstract p: number; }\n\
                  class B extends A { q = this.p; }";
    assert!(
        has(source, ABSTRACT_PROPERTY_IN_CONSTRUCTOR),
        "codes: {:?}",
        codes(source)
    );
}

/// A method body is not a constructor body, so an abstract read there is
/// legal — the baseline must not turn every member body into a constructor.
#[test]
fn abstract_property_read_in_method_body_stays_clean() {
    let source = "abstract class A { abstract p: number; m() { return this.p; } }";
    assert!(
        !has(source, ABSTRACT_PROPERTY_IN_CONSTRUCTOR),
        "codes: {:?}",
        codes(source)
    );
}
