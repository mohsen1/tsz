//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — object array literal recovery.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

#[test]
fn test_object_literal_statement_recovery_after_shorthand_property() {
    let source = "var v = { a\nreturn;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let return_pos = source.find("return").expect("return position") as u32;
    let semicolon_pos = source.rfind(';').expect("semicolon position") as u32;
    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == return_pos
            && diag.message == "',' expected."),
        "Expected missing comma at the statement keyword, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == semicolon_pos
            && diag.message == "':' expected."),
        "Expected missing ':' at the trailing semicolon, got {diagnostics:?}"
    );
    // tsc suppresses '}}' expected at EOF when a recent error (within 1 char)
    // already reported the issue. Matching that behavior here.
}

#[test]
fn test_object_literal_statement_recovery_after_missing_initializer() {
    let source = "var v = { a:\nreturn;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let return_pos = source.find("return").expect("return position") as u32;
    let semicolon_pos = source.rfind(';').expect("semicolon position") as u32;

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == 1109 && diag.start == return_pos),
        "Expected TS1109 at the statement keyword after a missing initializer, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|diag| !(diag.code == 1005
            && diag.start == return_pos
            && diag.message == "',' expected.")),
        "Missing initializer recovery should not inject a comma error at the next statement keyword: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == semicolon_pos
            && diag.message == "':' expected."),
        "Expected missing ':' at the trailing semicolon, got {diagnostics:?}"
    );
    // tsc suppresses '}}' expected at EOF when a recent error (within 1 char)
    // already reported the issue. Matching that behavior here.
}

#[test]
fn test_object_literal_statement_recovery_after_trailing_comma() {
    let source = "var v = { a: 1,\nreturn;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let return_pos = source.find("return").expect("return position") as u32;
    let semicolon_pos = source.rfind(';').expect("semicolon position") as u32;

    assert!(
        diagnostics.iter().all(|diag| !(diag.code == 1005
            && diag.start == return_pos
            && diag.message == "',' expected.")),
        "Trailing-comma recovery should not add an extra comma error at the next statement keyword: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == semicolon_pos
            && diag.message == "':' expected."),
        "Expected missing ':' at the trailing semicolon, got {diagnostics:?}"
    );
    // tsc suppresses '}}' expected at EOF when a recent error (within 1 char)
    // already reported the issue. Matching that behavior here.
}

#[test]
fn test_array_literal_semicolon_recovers_as_missing_comma() {
    let source = "var texCoords = [2, 2, 0.5000001192092895, 0.8749999 ; 403953552, 0.5000001192092895, 0.8749999403953552];";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let semicolon_pos = source.find(';').expect("semicolon position") as u32;
    let close_bracket_pos = source.rfind(']').expect("close bracket position") as u32;

    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == semicolon_pos
            && diag.message == "',' expected."),
        "Expected missing comma at the array literal semicolon, got {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|diag| diag.code == 1005
            && diag.start == close_bracket_pos
            && diag.message == "';' expected."),
        "Expected trailing ';' recovery at the array close bracket, got {diagnostics:?}"
    );
}

#[test]
fn test_trailing_comma_in_object_literal() {
    // Trailing commas should be allowed in object literals
    let source = r"
const obj = {
    a: 1,
    b: 2,
};
";
    let (parser, _root) = parse_source(source);

    // Should not emit any errors for trailing comma
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no errors for trailing comma in object literal, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_trailing_comma_in_array_literal() {
    // Trailing commas should be allowed in array literals
    let source = r"
const arr = [
    1,
    2,
    3,
];
";
    let (parser, _root) = parse_source(source);

    // Should not emit any errors for trailing comma
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no errors for trailing comma in array literal, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_array_terminated_by_close_paren_emits_comma_expected() {
    // Regression for conformance test
    // `destructuringParameterDeclaration2.ts` line 8:
    //   `a0([1, "string", [["world"]]);`
    // The outer `[` is never closed before the `)`. tsc reports a single TS1005
    // `',' expected.` at the `)`. Before this fix, we reported `']' expected.`
    // because the array-literal loop broke without first emitting the missing-
    // separator diagnostic that tsc's parseDelimitedList unconditionally emits.
    let source = "a0([1, \"string\", [[\"world\"]]);\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();

    let close_paren_pos = source.find(')').expect("`)` is in the source") as u32;
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED
                && d.start == close_paren_pos
                && d.message == "',' expected."),
        "expected TS1005 `',' expected.` at the `)`, got {diagnostics:?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED
                && d.start == close_paren_pos
                && d.message == "']' expected."),
        "TS1005 `']' expected.` at the `)` should be dedup'd by the comma error, got {diagnostics:?}"
    );
}

#[test]
fn test_array_terminated_by_close_brace_emits_comma_expected() {
    // Sibling case: array literal terminated by an enclosing `}` (e.g. block
    // boundary). Same expectation — tsc reports `,' expected` rather than
    // `]' expected`.
    let source = "{ const x = [1, 2 }\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();

    let close_brace_pos = source.find('}').expect("`}` is in the source") as u32;
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED
                && d.start == close_brace_pos
                && d.message == "',' expected."),
        "expected TS1005 `',' expected.` at the `}}`, got {diagnostics:?}"
    );
}

#[test]
fn test_array_terminated_by_close_bracket_keeps_clean_close() {
    // Sanity guard: a normal `[1, 2]` must not gain a spurious comma diagnostic.
    let source = "var a = [1, 2];\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics.is_empty(),
        "well-formed array literal must not emit diagnostics, got {diagnostics:?}"
    );
}

#[test]
fn test_object_literal_comma_recovery_after_short_distance_colon_error() {
    // Regression for conformance test
    // `conformance/classes/nestedClassDeclaration.ts`:
    //   `var x = {\n    class C4 {\n    }\n}`
    // tsc emits TWO TS1005 errors here:
    //   - `':' expected.` at column 11 (the `C` of `C4`)
    //   - `',' expected.` at column 14 (the `{`)
    // We previously emitted only the first because our `error_comma_expected`
    // applies a 3-byte distance suppression that swallows the legitimate comma
    // diagnostic when the gap is exactly 3 columns. tsc's `parseErrorAtPosition`
    // dedups only on exact same position; the unexpected-token recovery path in
    // `parse_object_literal` now bypasses the distance gate so it emits.
    let source = "var x = {\n    class C4 {\n    }\n}\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let line2_offset = source.find("    class C4").expect("C4 line is in source") as u32;
    let c4_pos = line2_offset + "    class ".len() as u32; // position of `C` in `C4`
    let open_brace_pos = source.find("C4 {").expect("C4 { is in source") as u32 + 3; // position of `{`

    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED
                && d.start == c4_pos
                && d.message == "':' expected."),
        "expected TS1005 `':' expected.` at `C4`, got {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED
                && d.start == open_brace_pos
                && d.message == "',' expected."),
        "expected TS1005 `',' expected.` at `{{` after `C4`, got {diagnostics:?}"
    );
}
