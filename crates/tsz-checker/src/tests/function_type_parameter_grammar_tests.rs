//! Parameter-list grammar for signatures written in *type* position.
//!
//! `FunctionType` and `ConstructorType` nodes are parsed by
//! `parse_type_parameter_list` and typed by `get_type_from_function_type`,
//! neither of which routes through `CheckerState::check_parameter_ordering`, so
//! TS1014/TS1015/TS1016 never ran for them. tsc runs the same
//! `checkGrammarParameterList` for these nodes as for a function declaration.
//!
//! Every expectation here is pinned against `typescript@7.0.2`, including the
//! anchor column: TS1014 sits on the `...` token, TS1015/TS1016 on the
//! offending parameter's name.

use crate::test_utils::check_source_diagnostics;

/// `(code, byte offset)` for every diagnostic, in source order.
fn codes_at(source: &str) -> Vec<(u32, u32)> {
    let mut found: Vec<(u32, u32)> = check_source_diagnostics(source)
        .iter()
        .map(|diag| (diag.code, diag.start))
        .collect();
    found.sort_unstable();
    found
}

/// Byte offset of the `nth` (0-based) occurrence of `needle`.
fn offset_of(source: &str, needle: &str, nth: usize) -> u32 {
    let mut from = 0usize;
    for _ in 0..nth {
        let at = source[from..]
            .find(needle)
            .expect("fewer occurrences than requested");
        from += at + needle.len();
    }
    let at = source[from..].find(needle).expect("needle not found");
    u32::try_from(from + at).expect("offset fits in u32")
}

fn expect_only(source: &str, code: u32, needle: &str) {
    let expected = vec![(code, offset_of(source, needle, 0))];
    assert_eq!(
        codes_at(source),
        expected,
        "unexpected diagnostics for: {source}"
    );
}

fn expect_clean(source: &str) {
    assert_eq!(
        codes_at(source),
        Vec::new(),
        "expected no diagnostics for: {source}"
    );
}

// ---------------------------------------------------------------------
// TS1014 — a rest parameter must be last
// ---------------------------------------------------------------------

#[test]
fn function_type_alias_reports_rest_not_last() {
    expect_only("type F = (...a: number[], b: string) => void;", 1014, "...");
}

#[test]
fn inline_function_type_annotation_reports_rest_not_last() {
    expect_only(
        "declare let f: (...a: number[], b: string) => void;",
        1014,
        "...",
    );
}

#[test]
fn generic_function_type_reports_rest_not_last() {
    expect_only("type F = <T>(...a: T[], b: T) => void;", 1014, "...");
}

#[test]
fn constructor_type_reports_rest_not_last() {
    expect_only(
        "type C = new (...a: number[], b: string) => object;",
        1014,
        "...",
    );
}

#[test]
fn abstract_constructor_type_reports_rest_not_last() {
    expect_only(
        "type C = abstract new (...a: number[], b: string) => object;",
        1014,
        "...",
    );
}

#[test]
fn function_type_as_a_parameter_annotation_reports_rest_not_last() {
    expect_only(
        "declare function use(cb: (...a: number[], b: string) => void): void;",
        1014,
        "...",
    );
}

#[test]
fn function_type_inside_a_union_reports_rest_not_last() {
    expect_only(
        "type F = ((...a: number[], b: string) => void) | string;",
        1014,
        "...",
    );
}

#[test]
fn function_type_as_an_interface_property_reports_rest_not_last() {
    expect_only(
        "interface I { f: (...a: number[], b: string) => void }",
        1014,
        "...",
    );
}

// ---------------------------------------------------------------------
// TS1016 — a required parameter cannot follow an optional one
// ---------------------------------------------------------------------

#[test]
fn function_type_alias_reports_required_after_optional() {
    expect_only("type F = (a?: number, b: string) => void;", 1016, "b:");
}

#[test]
fn inline_function_type_annotation_reports_required_after_optional() {
    expect_only(
        "declare let f: (a?: number, b: string) => void;",
        1016,
        "b:",
    );
}

#[test]
fn generic_function_type_reports_required_after_optional() {
    expect_only("type F = <T>(a?: T, b: T) => void;", 1016, "b:");
}

#[test]
fn constructor_type_reports_required_after_optional() {
    expect_only(
        "type C = new (a?: number, b: string) => object;",
        1016,
        "b:",
    );
}

#[test]
fn abstract_constructor_type_reports_required_after_optional() {
    expect_only(
        "type C = abstract new (a?: number, b: string) => object;",
        1016,
        "b:",
    );
}

#[test]
fn function_type_as_an_object_type_member_reports_required_after_optional() {
    expect_only(
        "type T = { f: (a?: number, b: string) => void };",
        1016,
        "b:",
    );
}

#[test]
fn function_type_in_an_array_type_reports_required_after_optional() {
    expect_only("type F = ((a?: number, b: string) => void)[];", 1016, "b:");
}

// ---------------------------------------------------------------------
// TS1015 — `?` together with an initializer
// ---------------------------------------------------------------------

#[test]
fn function_type_reports_question_mark_with_initializer_alongside_ts2371() {
    // tsc reports TS1015 and TS2371 at the same anchor: the grammar arm and
    // the "initializer only in an implementation" rule are independent.
    let source = "type F = (a?: number = 1) => void;";
    let anchor = offset_of(source, "a?", 0);
    assert_eq!(codes_at(source), vec![(1015, anchor), (2371, anchor)]);
}

// ---------------------------------------------------------------------
// tsc reports at most ONE diagnostic per parameter list
// ---------------------------------------------------------------------

#[test]
fn a_misplaced_rest_wins_over_a_later_required_after_optional() {
    // `checkGrammarParameterList` returns at the first failing parameter,
    // so the trailing `b?`/`c` pair never reaches the TS1016 arm.
    expect_only(
        "type F = (...a: number[], b?: string, c: string) => void;",
        1014,
        "...",
    );
}

#[test]
fn only_the_first_required_after_optional_is_reported() {
    expect_only(
        "type F = (a?: number, b: string, c: string) => void;",
        1016,
        "b:",
    );
}

// ---------------------------------------------------------------------
// Negative controls
// ---------------------------------------------------------------------

#[test]
fn optional_after_required_is_clean() {
    expect_clean("type F = (a: number, b?: string) => void;");
}

#[test]
fn a_trailing_rest_after_an_optional_is_clean() {
    expect_clean("type F = (a?: number, ...rest: string[]) => void;");
}

#[test]
fn an_initialized_parameter_does_not_make_the_next_one_required_after_optional() {
    // tsc's `isOptionalParameter` compares the index against the minimum
    // argument count, so a leading `a = 1` is not optional here.
    expect_clean("function f(a = 1, b: number) {}");
}

#[test]
fn a_parameter_with_an_initializer_is_not_required_after_an_optional_one() {
    // Only TS2371 survives — the initializer keeps `b` out of the TS1016
    // arm, exactly as in tsc.
    let source = "type F = (a?: number, b = 1) => void;";
    assert_eq!(codes_at(source), vec![(2371, offset_of(source, "b =", 0))]);
}

#[test]
fn binding_pattern_parameters_are_clean() {
    expect_clean("type F = ({ a }: { a: number }, c?: string) => void;");
}

#[test]
fn a_this_parameter_before_the_optional_run_is_clean() {
    expect_clean("type F = (this: object, a: number, b?: string) => void;");
}

#[test]
fn an_empty_parameter_list_is_clean() {
    expect_clean("type F = () => void;");
}

// ---------------------------------------------------------------------
// One diagnostic per written signature, not per use
// ---------------------------------------------------------------------

#[test]
fn a_reused_alias_reports_its_signature_once() {
    let source = concat!(
        "type Bad = (...a: number[], b: string) => void;\n",
        "declare const x1: Bad;\n",
        "declare const x2: Bad;\n"
    );
    assert_eq!(codes_at(source), vec![(1014, offset_of(source, "...", 0))]);
}

#[test]
fn a_generic_alias_instantiated_twice_reports_its_signature_once() {
    let source = concat!(
        "type G<T> = (a?: T, b: T) => void;\n",
        "declare const g1: G<number>;\n",
        "declare const g2: G<string>;\n"
    );
    assert_eq!(codes_at(source), vec![(1016, offset_of(source, "b:", 0))]);
}
