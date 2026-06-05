//! Tests for trailing comma tracking in the parser
//!
//! These tests verify that the parser correctly identifies trailing commas
//! in various contexts where TypeScript allows them.

use crate::parser::test_fixture::{parse_source, parse_source_named};

fn parse_code(code: &str) -> Vec<crate::parser::ParseDiagnostic> {
    let (parser, _root) = parse_source(code);
    parser.get_diagnostics().to_vec()
}

/// Count TS1013 ("A rest parameter or binding pattern may not have a trailing comma")
/// diagnostics produced when parsing `code` under `file_name`.
fn rest_trailing_comma_errors(file_name: &str, code: &str) -> usize {
    let (parser, _root) = parse_source_named(file_name, code);
    parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1013)
        .count()
}

/// In non-ambient contexts, `tsc` reports TS1013 for a trailing comma after a
/// rest parameter. These mirror the `tsc` oracle (TypeScript 6.0.x).
#[test]
fn test_rest_trailing_comma_errors_in_non_ambient_contexts() {
    let cases = [
        ("function declaration", "function f(...a: number[],) {}"),
        ("arrow function", "const f = (...a: number[],) => {};"),
        ("class method", "class C { m(...a: number[],) {} }"),
        (
            "interface method signature",
            "interface I { m(...a: number[],): void; }",
        ),
        ("function type alias", "type F = (...a: number[],) => void;"),
        (
            "constructor type alias",
            "type C = new (...a: number[],) => object;",
        ),
        (
            "interface call signature",
            "interface I { (...a: number[],): void; }",
        ),
        (
            "interface construct signature",
            "interface I { new (...a: number[],): object; }",
        ),
        (
            "plain (non-declare) namespace function",
            "namespace N { export function f(...a: number[],) {} }",
        ),
        (
            "overload signature",
            "function f(...a: number[],): void;\nfunction f(...a: number[]): void {}",
        ),
    ];
    for (label, code) in cases {
        assert_eq!(
            rest_trailing_comma_errors("test.ts", code),
            1,
            "expected exactly one TS1013 for {label}: {code:?}",
        );
    }
}

/// In ambient contexts (inside `declare ...` or anywhere in a `.d.ts` file),
/// `tsc` tolerates a trailing comma after a rest parameter and emits no TS1013.
/// This is the real-world `@types/node` `pipeline`-overload shape from the bug.
#[test]
fn test_rest_trailing_comma_allowed_in_ambient_contexts() {
    // `declare`-modified declarations inside a regular `.ts` file.
    let ts_cases = [
        (
            "declare function",
            "declare function f(...a: number[],): void;",
        ),
        (
            "declare namespace function",
            "declare namespace N { function f(...a: number[],): void; }",
        ),
        (
            "declare class method",
            "declare class C { m(...a: number[],): void; }",
        ),
        (
            "declare const function type",
            "declare const f: (...a: number[],) => void;",
        ),
        (
            "declare const constructor type",
            "declare const C: new (...a: number[],) => object;",
        ),
        (
            "declare module function",
            "declare module \"m\" { export function f(...a: number[],): void; }",
        ),
    ];
    for (label, code) in ts_cases {
        assert_eq!(
            rest_trailing_comma_errors("test.ts", code),
            0,
            "expected no TS1013 for ambient {label}: {code:?}",
        );
    }

    // Everything in a declaration file is ambient, even without `declare`.
    let dts_cases = [
        (
            "declaration-file function (pipeline shape)",
            "declare function pipeline(\n    stream1: unknown,\n    ...streams: Array<unknown>,\n): unknown;",
        ),
        (
            "declaration-file interface method",
            "interface I { m(...a: number[],): void; }",
        ),
        (
            "declaration-file function type alias",
            "type F = (...a: number[],) => void;",
        ),
        (
            "declaration-file constructor type alias",
            "type C = new (...a: number[],) => object;",
        ),
        (
            "declaration-file call signature",
            "interface I { (...a: number[],): void; }",
        ),
        (
            "declaration-file plain function",
            "declare function f(...a: number[],): void;",
        ),
    ];
    for (label, code) in dts_cases {
        assert_eq!(
            rest_trailing_comma_errors("test.d.ts", code),
            0,
            "expected no TS1013 for {label} in a .d.ts file: {code:?}",
        );
    }
}

/// The ambient exception is specific to the trailing comma: a rest parameter
/// that is *not* last must still report TS1014 in every context, and a trailing
/// comma after a non-rest parameter is always fine.
#[test]
fn test_rest_parameter_grammar_unaffected_by_ambient_exception() {
    // Rest-not-last still errors (TS1014) even in ambient contexts.
    let (parser, _root) = parse_source_named(
        "test.d.ts",
        "declare function f(...a: number[], b: number): void;",
    );
    let ts1014 = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1014)
        .count();
    assert_eq!(ts1014, 1, "rest-not-last should still emit TS1014 in .d.ts");

    // Trailing comma after a non-rest final parameter never emits TS1013.
    assert_eq!(
        rest_trailing_comma_errors("test.ts", "function f(a: number, b: number,) {}"),
        0,
        "trailing comma after a non-rest parameter must not emit TS1013",
    );
}

#[test]
fn test_trailing_comma_in_parameter_list() {
    let code = r"
function foo(a: string, b: number,) {
return a + b;
}
";
    let diagnostics = parse_code(code);
    // Should not emit any errors - trailing comma is allowed
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
    assert_eq!(
        ts1005_count, 0,
        "Trailing comma in parameter list should not emit TS1005"
    );
}

#[test]
fn test_trailing_comma_in_enum() {
    let code = r"
enum Color {
Red,
Green,
Blue,
}
";
    let diagnostics = parse_code(code);
    // Should not emit any errors - trailing comma is allowed
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
    assert_eq!(
        ts1005_count, 0,
        "Trailing comma in enum should not emit TS1005"
    );
}

#[test]
fn test_trailing_comma_in_array_literal() {
    let code = r"
const arr = [1, 2, 3,];
";
    let diagnostics = parse_code(code);
    // Should not emit any errors - trailing comma is allowed
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
    assert_eq!(
        ts1005_count, 0,
        "Trailing comma in array literal should not emit TS1005"
    );
}

#[test]
fn test_trailing_comma_in_object_literal() {
    let code = r"
const obj = { a: 1, b: 2, };
";
    let diagnostics = parse_code(code);
    // Should not emit any errors - trailing comma is allowed
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
    assert_eq!(
        ts1005_count, 0,
        "Trailing comma in object literal should not emit TS1005"
    );
}

#[test]
fn test_trailing_comma_in_type_parameters() {
    let code = r"
function foo<T, U,>() {
// ...
}
";
    let diagnostics = parse_code(code);
    // Should not emit any errors - trailing comma is allowed
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
    assert_eq!(
        ts1005_count, 0,
        "Trailing comma in type parameters should not emit TS1005"
    );
}

#[test]
fn test_trailing_comma_in_type_arguments() {
    let code = r"
const arr: Array<string, number,> = [1, 2];
";
    let diagnostics = parse_code(code);
    // Should not emit any errors - trailing comma is allowed
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
    assert_eq!(
        ts1005_count, 0,
        "Trailing comma in type arguments should not emit TS1005"
    );
}

#[test]
fn test_no_trailing_comma() {
    let code = r"
function foo(a: string, b: number) {
return a + b;
}
";
    let diagnostics = parse_code(code);
    // Should not emit any errors - no trailing comma is fine too
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
    assert_eq!(ts1005_count, 0, "No trailing comma should not emit TS1005");
}

#[test]
fn test_asi_after_return() {
    let code = r"
function foo() {
return
42;
}
";
    let diagnostics = parse_code(code);
    // TypeScript applies ASI here, so this parses as `return; 42;`
    // The expression `42` is never returned, but this is valid syntax
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
    assert_eq!(ts1005_count, 0, "ASI after return should not emit TS1005");
}

#[test]
fn test_asi_after_break() {
    let code = r"
while (true) {
break
// some comment
}
";
    let diagnostics = parse_code(code);
    // ASI applies after break
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
    assert_eq!(ts1005_count, 0, "ASI after break should not emit TS1005");
}

#[test]
fn test_function_overloads_no_duplicate_error() {
    let code = r"
function foo(x: string): void;
function foo(x: number): void;
function foo(x: string | number): void {
console.log(x);
}
";
    let diagnostics = parse_code(code);
    // Function overloads should not emit TS2300
    let ts2300_count = diagnostics.iter().filter(|d| d.code == 2300).count();
    assert_eq!(ts2300_count, 0, "Function overloads should not emit TS2300");
}

#[test]
fn test_interface_merging_no_duplicate_error() {
    let code = r"
interface Box {
width: number;
}
interface Box {
height: number;
}
";
    let diagnostics = parse_code(code);
    // Interface merging should not emit TS2300
    let ts2300_count = diagnostics.iter().filter(|d| d.code == 2300).count();
    assert_eq!(ts2300_count, 0, "Interface merging should not emit TS2300");
}

#[test]
fn test_namespace_function_merging_no_duplicate_error() {
    let code = r"
namespace Utils {
export function helper(): void {}
}
function Utils() {
// Implementation
}
";
    let diagnostics = parse_code(code);
    // Namespace + function merging should not emit TS2300
    let ts2300_count = diagnostics.iter().filter(|d| d.code == 2300).count();
    assert_eq!(
        ts2300_count, 0,
        "Namespace + function merging should not emit TS2300"
    );
}
