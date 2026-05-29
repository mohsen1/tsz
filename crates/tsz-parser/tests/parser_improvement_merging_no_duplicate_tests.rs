//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — merging no duplicate.

use crate::parser::test_fixture::parse_source;

#[test]
fn test_interface_merging_no_duplicate() {
    // Interface merging should not emit TS2300
    let source = r"
interface Foo {
    a: number;
}
interface Foo {
    b: string;
}
";
    let (parser, _root) = parse_source(source);

    // Should not emit TS2300 for interface merging
    let ts2300_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 2300)
        .count();
    assert_eq!(
        ts2300_count, 0,
        "Expected no TS2300 errors for interface merging, got {ts2300_count}",
    );
}

#[test]
fn test_function_overloads_no_duplicate() {
    // Function overloads should not emit TS2300
    let source = r"
function foo(x: number): void;
function foo(x: string): void;
function foo(x: number | string): void {
    console.log(x);
}
";
    let (parser, _root) = parse_source(source);

    // Should not emit TS2300 for function overloads
    let ts2300_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 2300)
        .count();
    assert_eq!(
        ts2300_count, 0,
        "Expected no TS2300 errors for function overloads, got {ts2300_count}",
    );
}

#[test]
fn test_namespace_function_merging_no_duplicate() {
    // Namespace + function merging should not emit TS2300
    let source = r#"
namespace Utils {
    export function helper(): void {
        console.log("helper");
    }
}
function Utils() {
    console.log("constructor");
}
"#;
    let (parser, _root) = parse_source(source);

    // Should not emit TS2300 for namespace + function merging
    let ts2300_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 2300)
        .count();
    assert_eq!(
        ts2300_count, 0,
        "Expected no TS2300 errors for namespace+function merging, got {ts2300_count}",
    );
}
