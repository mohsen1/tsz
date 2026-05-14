//! Regression tests for TS1433 emission on `this` parameters preceded by
//! invalid parameter modifiers like `async`.
//!
//! Background: `parse_error_at` deduplicates parser diagnostics that share a
//! start position. `parse_parameter_modifiers` previously emitted TS1090
//! ("'X' modifier cannot appear on a parameter") for invalid modifiers like
//! `async`, which was eaten the same-position TS1433 emitted just after by
//! `parse_parameter` ("Neither decorators nor modifiers may be applied to
//! 'this' parameters"). tsc emits ONLY TS1433 for `async this:` /
//! `static this:` etc., so we suppress the TS1090 in that path.

use crate::parser::ParserState;
use crate::parser::test_fixture::parse_source;

fn has_error_code(parser: &ParserState, code: u32) -> bool {
    parser.get_diagnostics().iter().any(|d| d.code == code)
}

fn count_error_code(parser: &ParserState, code: u32) -> usize {
    parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == code)
        .count()
}

fn diagnostic_codes(parser: &ParserState) -> Vec<u32> {
    parser.get_diagnostics().iter().map(|d| d.code).collect()
}

/// `async this:` should emit TS1433, not TS1090. tsc collapses the modifier
/// error into the `this`-parameter error.
#[test]
fn async_this_emits_ts1433_only() {
    let (parser, _root) = parse_source("function f(async this: any): number { return 0; }");
    assert!(
        has_error_code(&parser, 1433),
        "async this should emit TS1433 (modifiers on this parameter)"
    );
    assert!(
        !has_error_code(&parser, 1090),
        "async this should NOT emit TS1090 — tsc routes through TS1433 only"
    );
}

/// `static this:` — same shape as `async this`. `static` is not a valid
/// parameter modifier either, but the `this`-parameter error wins.
#[test]
fn static_this_emits_ts1433_only() {
    let (parser, _root) = parse_source("function f(static this: any): number { return 0; }");
    assert!(has_error_code(&parser, 1433));
    assert!(!has_error_code(&parser, 1090));
}

/// Multi-modifier case: `static async this:`. Both modifiers should be
/// suppressed in favor of a single TS1433.
#[test]
fn static_async_this_emits_ts1433_only() {
    let (parser, _root) = parse_source("function f(static async this: any): number { return 0; }");
    assert_eq!(
        count_error_code(&parser, 1433),
        1,
        "exactly one TS1433 should fire for the modifier run"
    );
    assert!(
        !has_error_code(&parser, 1090),
        "neither static nor async should emit TS1090 in front of `this`"
    );
}

/// `public this:` — already worked before this fix because `public` is a
/// VALID parameter modifier (no TS1090 emitted), so TS1433 was never deduped.
/// Locked here so the suppression flag does not regress the path.
#[test]
fn public_this_still_emits_ts1433() {
    let (parser, _root) = parse_source("function f(public this: any): number { return 0; }");
    assert!(has_error_code(&parser, 1433));
}

/// `@deco() this:` — decorators-on-this also already worked. Locked here.
#[test]
fn decorator_this_still_emits_ts1433() {
    let (parser, _root) =
        parse_source("declare const deco: any;\nfunction f(@deco this: any): number { return 0; }");
    assert!(has_error_code(&parser, 1433));
}

/// Negative case: `async` as the parameter NAME (not a modifier) — `(async)`
/// is a parameter named `async`. The lookahead must not misfire and suppress
/// real diagnostics. Here there's no `this` after, so TS1090 never fires
/// either — but we still verify TS1433 doesn't get falsely emitted.
#[test]
fn async_parameter_name_does_not_emit_ts1433() {
    let (parser, _root) = parse_source("function f(async: number): number { return 0; }");
    assert!(
        !has_error_code(&parser, 1433),
        "TS1433 must not fire when there is no `this` parameter"
    );
}

/// Negative case: `async x:` (invalid modifier, parameter is regular). TS1090
/// should still fire (we only suppress when the parameter name is `this`).
#[test]
fn async_modifier_on_regular_parameter_still_emits_ts1090() {
    let (parser, _root) = parse_source("function f(async x: any): number { return 0; }");
    assert!(
        has_error_code(&parser, 1090),
        "TS1090 should still fire when the parameter is not `this`"
    );
}

/// Function type parameter lists do not emit TS1433 for `this` parameters, so
/// they must keep TS1090 instead of suppressing the invalid modifier diagnostic.
#[test]
fn static_this_in_function_type_still_emits_ts1090() {
    let mut parser = ParserState::new("test.ts".to_string(), "static this: C)".to_string());
    parser.next_token();
    parser.parse_type_parameter_list();
    assert!(
        has_error_code(&parser, 1090),
        "TS1090 should fire in function type parameter lists because TS1433 is not emitted there. diagnostics={:?}",
        parser.get_diagnostics()
    );
    assert!(
        !has_error_code(&parser, 1433),
        "Function type parameter parsing should not synthesize TS1433. diagnostics={:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn rest_this_parameter_reports_reserved_word() {
    let source = "function f(...this: C): number { return 0; }";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    assert_eq!(diagnostic_codes(&parser), vec![1359], "{diagnostics:?}");

    let this_pos = source.find("this").expect("this token") as u32;
    assert_eq!(diagnostics[0].start, this_pos, "{diagnostics:?}");
}

#[test]
fn optional_this_parameter_reports_missing_comma() {
    let source = "function f(this?: C): number { return 0; }";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    assert_eq!(diagnostic_codes(&parser), vec![1005], "{diagnostics:?}");

    let question_pos = source.find('?').expect("question token") as u32;
    assert_eq!(diagnostics[0].start, question_pos, "{diagnostics:?}");
    assert_eq!(diagnostics[0].message, "',' expected.");
}

#[test]
fn this_parameter_initializer_new_call_matches_tsc_recovery() {
    let source = "function f(this: C = new C()): number { return 0; }";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let actual: Vec<(u32, u32, &str)> = diagnostics
        .iter()
        .map(|diag| (diag.code, diag.start, diag.message.as_str()))
        .collect();

    let expected = vec![
        (1005, source.find('=').unwrap() as u32, "',' expected."),
        (
            1359,
            source.find("new").unwrap() as u32,
            "Identifier expected. 'new' is a reserved word that cannot be used here.",
        ),
        (
            1005,
            source.find("C()").unwrap() as u32 + 1,
            "',' expected.",
        ),
        (
            1109,
            source.find("()").unwrap() as u32 + 1,
            "Expression expected.",
        ),
        (
            1005,
            source.find("): number").unwrap() as u32,
            "';' expected.",
        ),
        (
            1128,
            source.find(": number").unwrap() as u32,
            "Declaration or statement expected.",
        ),
        (
            1434,
            source.find("number").unwrap() as u32,
            "Unexpected keyword or identifier.",
        ),
    ];

    assert_eq!(actual, expected, "{diagnostics:?}");
}

#[test]
fn this_parameter_initializer_identifier_only_reports_missing_comma() {
    let source = "function f(this: C = value): number { return 0; }";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    assert_eq!(diagnostic_codes(&parser), vec![1005], "{diagnostics:?}");
    assert_eq!(
        diagnostics[0].start,
        source.find('=').expect("equals token") as u32
    );
}
