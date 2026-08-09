//! Tests for TS2462 ("A rest element must be last in a destructuring pattern"),
//! emitted as a grammar check while parsing array/object binding patterns
//! (`report_rest_element_not_last`).
//!
//! `tsc` reports TS2462 for any binding-pattern rest element (`...x`) followed
//! by another element, anchored at the element's name, uniformly across every
//! position: variable declarations, parameters (function, arrow, method,
//! type-signature, ambient), `catch` bindings, and nested patterns. Assignment
//! destructuring targets are array/object *literals* reinterpreted elsewhere and
//! carry their own TS2462 check, so they are out of scope for this file.

use crate::parser::test_fixture::parse_source;

const TS2462: u32 = 2462;

/// All `(code, start_offset)` pairs the parser reports for `source`.
fn diags(source: &str) -> Vec<(u32, u32)> {
    let (parser, _root) = parse_source(source);
    parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.start))
        .collect()
}

/// The byte offsets at which TS2462 was reported.
fn ts2462_starts(source: &str) -> Vec<u32> {
    diags(source)
        .into_iter()
        .filter(|(code, _)| *code == TS2462)
        .map(|(_, start)| start)
        .collect()
}

/// Byte offset of the identifier `name`, anchored just past its leading `...`
/// occurrence so the helper points at the same character `tsc` anchors on.
fn rest_name_offset(source: &str, name: &str) -> u32 {
    let needle = format!("...{name}");
    let dots = source
        .find(&needle)
        .unwrap_or_else(|| panic!("`{needle}` not found in {source:?}"));
    (dots + 3) as u32
}

// --- array binding patterns, every declaration/parameter position ---

#[test]
fn variable_declaration_array_rest_not_last() {
    let src = "const [...a, b] = [1, 2];";
    assert_eq!(ts2462_starts(src), vec![rest_name_offset(src, "a")]);
}

#[test]
fn variable_declaration_array_rest_middle() {
    let src = "const [a, ...b, c] = [1, 2, 3];";
    assert_eq!(ts2462_starts(src), vec![rest_name_offset(src, "b")]);
}

#[test]
fn function_parameter_array_rest_not_last_untyped() {
    // The gap that motivated this fix: an untyped function parameter binding
    // pattern never reached the type-checker's binding-pattern walk.
    let src = "function f([...a, b]) {}";
    assert_eq!(ts2462_starts(src), vec![rest_name_offset(src, "a")]);
}

#[test]
fn function_parameter_array_rest_not_last_typed() {
    let src = "function f([...a, b]: number[]) {}";
    assert_eq!(ts2462_starts(src), vec![rest_name_offset(src, "a")]);
}

#[test]
fn arrow_parameter_array_rest_not_last() {
    let src = "const g = ([...a, b]) => a;";
    assert_eq!(ts2462_starts(src), vec![rest_name_offset(src, "a")]);
}

#[test]
fn arrow_parameter_array_rest_not_last_typed() {
    let src = "const g = ([...a, b]: number[]) => a;";
    assert_eq!(ts2462_starts(src), vec![rest_name_offset(src, "a")]);
}

#[test]
fn method_parameter_array_rest_not_last() {
    let src = "class C { m([...a, b]) {} }";
    assert_eq!(ts2462_starts(src), vec![rest_name_offset(src, "a")]);
}

#[test]
fn type_signature_parameter_array_rest_not_last() {
    let src = "type F = ([...a, b]) => void;";
    assert_eq!(ts2462_starts(src), vec![rest_name_offset(src, "a")]);
}

#[test]
fn ambient_function_parameter_array_rest_not_last() {
    let src = "declare function f([...a, b]): void;";
    assert_eq!(ts2462_starts(src), vec![rest_name_offset(src, "a")]);
}

#[test]
fn catch_clause_array_rest_not_last() {
    let src = "try {} catch ([...a, b]) {}";
    assert_eq!(ts2462_starts(src), vec![rest_name_offset(src, "a")]);
}

// --- object binding patterns ---

#[test]
fn variable_declaration_object_rest_not_last() {
    let src = "const { ...a, b } = { };";
    assert_eq!(ts2462_starts(src), vec![rest_name_offset(src, "a")]);
}

#[test]
fn function_parameter_object_rest_not_last() {
    let src = "function f({ ...a, b }) {}";
    assert_eq!(ts2462_starts(src), vec![rest_name_offset(src, "a")]);
}

#[test]
fn arrow_parameter_object_rest_not_last() {
    let src = "const h = ({ ...a, b }) => a;";
    assert_eq!(ts2462_starts(src), vec![rest_name_offset(src, "a")]);
}

// --- nested patterns: each pattern is checked independently, anchored at its
// own rest element (previously the checker anchored nested rests at the wrong
// column) ---

#[test]
fn nested_array_pattern_rest_not_last() {
    let src = "const [a, [...b, c]] = [1, [2, 3]];";
    assert_eq!(ts2462_starts(src), vec![rest_name_offset(src, "b")]);
}

#[test]
fn nested_object_to_array_pattern_rest_not_last() {
    let src = "const { a: [...p, q] } = obj;";
    assert_eq!(ts2462_starts(src), vec![rest_name_offset(src, "p")]);
}

// --- binder-name invariance: the diagnostic must not depend on identifiers ---

#[test]
fn rest_name_invariance() {
    let renamed = "function fn([...zz, yy]) {}";
    assert_eq!(
        ts2462_starts(renamed),
        vec![rest_name_offset(renamed, "zz")]
    );
}

// --- multiple offending rests each report ---

#[test]
fn two_rests_report_twice() {
    let src = "const [...a, ...b, c] = [];";
    assert_eq!(
        ts2462_starts(src),
        vec![rest_name_offset(src, "a"), rest_name_offset(src, "b")]
    );
}

#[test]
fn elision_after_rest_reports() {
    // A hole following the rest still makes it not-last.
    let src = "const [...a, ,] = [1, 2];";
    assert_eq!(ts2462_starts(src), vec![rest_name_offset(src, "a")]);
}

// --- negative cases: a trailing rest is legal, so no TS2462 ---

#[test]
fn array_trailing_rest_is_clean() {
    assert!(
        !diags("const [a, ...b] = [1, 2];")
            .iter()
            .any(|(c, _)| *c == TS2462)
    );
}

#[test]
fn object_trailing_rest_is_clean() {
    assert!(
        !diags("const { a, ...b } = { a: 1 };")
            .iter()
            .any(|(c, _)| *c == TS2462)
    );
}

#[test]
fn param_trailing_rest_is_clean() {
    assert!(
        !diags("function f([a, ...b]) {}")
            .iter()
            .any(|(c, _)| *c == TS2462)
    );
}

#[test]
fn trailing_comma_after_rest_is_not_ts2462() {
    // `[...a,]` is a trailing comma (TS1013), not a rest-not-last (TS2462).
    let src = "const [...a,] = [1];";
    assert!(!diags(src).iter().any(|(c, _)| *c == TS2462));
}

#[test]
fn no_rest_pattern_is_clean() {
    assert!(
        !diags("const [a, b, c] = [1, 2, 3];")
            .iter()
            .any(|(c, _)| *c == TS2462)
    );
}
