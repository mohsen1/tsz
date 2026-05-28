//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — primitive type recovery.

use crate::parser::test_fixture::parse_source;

#[test]
fn test_void_return_type() {
    // void return type should be parsed correctly without TS1110/TS1109 errors
    let source = r"
declare function fn(arg0: boolean): void;
";
    let (parser, _root) = parse_source(source);

    // Should not emit any parser errors for void return type
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for void return type, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_primitive_type_keywords() {
    // All primitive type keywords should be parsed correctly
    let source = r"
declare function fn1(): void;
declare function fn2(): string;
declare function fn3(): number;
declare function fn4(): boolean;
declare function fn5(): symbol;
declare function fn6(): bigint;
declare function fn7(): any;
declare function fn8(): unknown;
declare function fn9(): never;
declare function fn10(): null;
declare function fn11(): undefined;
declare function fn12(): object;
";
    let (parser, _root) = parse_source(source);

    // Should not emit any parser errors for primitive type keywords
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for primitive type keywords, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_primitive_types_in_type_aliases() {
    // Primitive type keywords should work in type aliases
    let source = r"
type T1 = void;
type T2 = string;
type T3 = number;
type T4 = boolean;
type T5 = any;
type T6 = unknown;
type T7 = never;
";
    let (parser, _root) = parse_source(source);

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for primitive types in type aliases, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_primitive_types_in_parameters() {
    // Primitive type keywords should work in parameter types
    let source = r"
declare function fn(a: void, b: string, c: number): boolean;
";
    let (parser, _root) = parse_source(source);

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for primitive types in parameters, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_primitive_types_in_arrow_functions() {
    // Primitive type keywords should work in arrow function types
    let source = r#"
const arrow1: () => void = () => {};
const arrow2: (x: number) => string = (x) => "";
"#;
    let (parser, _root) = parse_source(source);

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for primitive types in arrow functions, got {:?}",
        parser.get_diagnostics()
    );
}
