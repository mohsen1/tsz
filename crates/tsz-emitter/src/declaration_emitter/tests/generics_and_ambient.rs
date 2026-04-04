use super::*;

// =============================================================================
// 6. Generic Declarations
// =============================================================================

#[test]
fn test_generic_function() {
    let output = emit_dts("export function identity<T>(x: T): T { return x; }");
    assert!(
        output.contains("<T>"),
        "Expected generic type parameter: {output}"
    );
    assert!(
        output.contains("x: T"),
        "Expected parameter with generic type: {output}"
    );
    assert!(
        output.contains("): T;"),
        "Expected return type with generic: {output}"
    );
}

#[test]
fn test_generic_interface_with_constraint() {
    let output = emit_dts(
        r#"
    export interface Container<T extends object> {
        value: T;
    }
    "#,
    );
    assert!(
        output.contains("<T extends object>"),
        "Expected generic type parameter with constraint: {output}"
    );
    assert!(
        output.contains("value: T;"),
        "Expected member with generic type: {output}"
    );
}

#[test]
fn test_generic_class_with_default() {
    let output = emit_dts(
        r#"
    export class Box<T = string> {
        content: T;
        constructor(value: T) { this.content = value; }
    }
    "#,
    );
    assert!(
        output.contains("<T = string>"),
        "Expected generic type parameter with default: {output}"
    );
}

#[test]
fn test_multiple_type_parameters() {
    let output = emit_dts(
        "export function map<T, U>(arr: T[], fn: (x: T) => U): U[] { return arr.map(fn); }",
    );
    assert!(
        output.contains("<T, U>"),
        "Expected multiple type parameters: {output}"
    );
}

// =============================================================================
// 7. Ambient / Declare Declarations
// =============================================================================

#[test]
fn test_declare_class_passthrough() {
    let output = emit_dts(
        r#"
    declare class Foo {
        bar(): void;
    }
    "#,
    );
    assert!(
        output.contains("declare class Foo"),
        "Expected declare class: {output}"
    );
    assert!(
        output.contains("bar(): void;"),
        "Expected method signature: {output}"
    );
}

#[test]
fn test_declare_function_passthrough() {
    let output = emit_dts("declare function greet(name: string): void;");
    assert!(
        output.contains("declare function greet(name: string): void;"),
        "Expected declare function: {output}"
    );
}

#[test]
fn test_declare_var_passthrough() {
    let output = emit_dts("declare var globalName: string;");
    assert!(
        output.contains("declare var globalName: string;"),
        "Expected declare var: {output}"
    );
}

// =============================================================================
// 8. Module / Namespace Declarations
// =============================================================================

#[test]
fn test_namespace_declaration() {
    let output = emit_dts(
        r#"
    export declare namespace MyLib {
        function create(): void;
        class Widget {
            name: string;
        }
    }
    "#,
    );
    assert!(
        output.contains("export declare namespace MyLib"),
        "Expected namespace declaration: {output}"
    );
    assert!(
        output.contains("function create(): void;"),
        "Expected function in namespace: {output}"
    );
    assert!(
        output.contains("class Widget"),
        "Expected class in namespace: {output}"
    );
}

#[test]
fn test_nested_namespace() {
    let output = emit_dts(
        r#"
    export declare namespace Outer {
        namespace Inner {
            const value: number;
        }
    }
    "#,
    );
    assert!(
        output.contains("namespace Outer"),
        "Expected outer namespace: {output}"
    );
    assert!(
        output.contains("namespace Inner"),
        "Expected inner namespace: {output}"
    );
}

// =============================================================================
// 9. Enum Declarations
// =============================================================================

#[test]
fn test_regular_enum() {
    let output = emit_dts(
        r#"
    export enum Color {
        Red,
        Green,
        Blue
    }
    "#,
    );
    assert!(
        output.contains("export declare enum Color"),
        "Expected exported declare enum: {output}"
    );
    assert!(output.contains("Red"), "Expected Red member: {output}");
    assert!(output.contains("Green"), "Expected Green member: {output}");
    assert!(output.contains("Blue"), "Expected Blue member: {output}");
}

#[test]
fn test_const_enum() {
    let output = emit_dts(
        r#"
    export const enum Direction {
        Up = 0,
        Down = 1,
        Left = 2,
        Right = 3
    }
    "#,
    );
    assert!(
        output.contains("export declare const enum Direction"),
        "Expected exported declare const enum: {output}"
    );
    assert!(output.contains("Up = 0"), "Expected Up = 0: {output}");
    assert!(output.contains("Right = 3"), "Expected Right = 3: {output}");
}

#[test]
fn test_invalid_const_enum_object_index_access_emits_any() {
    let output = emit_dts_with_binding(
        r#"
const enum G {
    A = 1,
    B = 2,
}
let z1 = G[G.A];
"#,
    );

    assert!(
        output.contains("declare let z1: any;"),
        "Expected invalid const enum object index access to emit any: {output}"
    );
}

#[test]
fn test_string_enum() {
    let output = emit_dts(
        r#"
    export enum Status {
        Active = "active",
        Inactive = "inactive"
    }
    "#,
    );
    assert!(
        output.contains("Active = \"active\""),
        "Expected string enum value: {output}"
    );
    assert!(
        output.contains("Inactive = \"inactive\""),
        "Expected string enum value: {output}"
    );
}

#[test]
fn test_enum_auto_increment() {
    let output = emit_dts(
        r#"
    export enum Seq {
        A = 10,
        B,
        C
    }
    "#,
    );
    assert!(output.contains("A = 10"), "Expected A = 10: {output}");
    assert!(
        output.contains("B = 11"),
        "Expected B = 11 (auto-increment): {output}"
    );
    assert!(
        output.contains("C = 12"),
        "Expected C = 12 (auto-increment): {output}"
    );
}
