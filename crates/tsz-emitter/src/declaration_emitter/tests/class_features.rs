use super::*;

// =============================================================================
// 10. Class Advanced Features
// =============================================================================

#[test]
fn test_abstract_class() {
    let output = emit_dts(
        r#"
    export abstract class Shape {
        abstract area(): number;
        name: string;
        constructor(name: string) { this.name = name; }
    }
    "#,
    );
    assert!(
        output.contains("export declare abstract class Shape"),
        "Expected abstract class: {output}"
    );
    assert!(
        output.contains("abstract area(): number;"),
        "Expected abstract method: {output}"
    );
}

#[test]
fn test_class_with_heritage() {
    let output = emit_dts(
        r#"
    export class Dog extends Animal implements Pet {
        bark(): void {}
    }
    "#,
    );
    assert!(
        output.contains("extends Animal"),
        "Expected extends clause: {output}"
    );
    assert!(
        output.contains("implements Pet"),
        "Expected implements clause: {output}"
    );
}

#[test]
fn test_constructor_declaration() {
    let output = emit_dts(
        r#"
    export class Point {
        x: number;
        y: number;
        constructor(x: number, y: number) {
            this.x = x;
            this.y = y;
        }
    }
    "#,
    );
    assert!(
        output.contains("constructor(x: number, y: number);"),
        "Expected constructor in .d.ts: {output}"
    );
}

#[test]
fn test_parameter_properties() {
    let output = emit_dts(
        r#"
    export class Point {
        constructor(public x: number, protected y: number, private z: number) {}
    }
    "#,
    );
    // Parameter properties should be emitted as class properties
    assert!(
        output.contains("x: number;"),
        "Expected public parameter property as class property: {output}"
    );
    assert!(
        output.contains("protected y: number;"),
        "Expected protected parameter property: {output}"
    );
    assert!(
        output.contains("private z;"),
        "Expected private parameter property (without type): {output}"
    );
}

#[test]
fn test_optional_parameter_property_emits_undefined_in_constructor_and_property() {
    let output = emit_dts(
        r#"
    export class Point {
        constructor(public x?: string) {}
    }
    "#,
    );

    assert!(
        output.contains("x?: string | undefined;"),
        "Expected optional parameter property to include undefined in property type: {output}"
    );
    assert!(
        output.contains("constructor(x?: string | undefined);"),
        "Expected optional parameter property to include undefined in constructor type: {output}"
    );
}

#[test]
fn test_optional_function_type_preserves_explicit_undefined() {
    let output = emit_dts(
        r#"
    export type Fn = (x?: string | undefined, y?: number | undefined) => void;
    "#,
    );

    assert!(
        output.contains("x?: string | undefined"),
        "Expected optional parameter to keep explicit undefined in alias: {output}"
    );
    assert!(
        output.contains("y?: number | undefined"),
        "Expected optional parameter to keep explicit undefined in alias: {output}"
    );
}

#[test]
fn test_parameter_property_initializer_infers_property_type() {
    let output = emit_dts(
        r#"
    export class Point {
        constructor(public x = "hello") {}
    }
    "#,
    );

    assert!(
        output.contains("x: string;"),
        "Expected initializer-backed parameter property to infer a property type: {output}"
    );
    assert!(
        output.contains("constructor(x?: string);"),
        "Expected initializer-backed parameter property constructor to stay optional: {output}"
    );
}

#[test]
fn test_getter_and_setter() {
    let output = emit_dts(
        r#"
    export class Foo {
        get value(): number { return 42; }
        set value(v: number) {}
    }
    "#,
    );
    assert!(
        output.contains("get value(): number;"),
        "Expected getter declaration: {output}"
    );
    assert!(
        output.contains("set value(v: number);"),
        "Expected setter declaration: {output}"
    );
}

#[test]
fn test_static_member() {
    let output = emit_dts(
        r#"
    export class Singleton {
        static instance: Singleton;
        static create(): Singleton { return new Singleton(); }
    }
    "#,
    );
    assert!(
        output.contains("static instance"),
        "Expected static property: {output}"
    );
    assert!(
        output.contains("static create"),
        "Expected static method: {output}"
    );
}

#[test]
fn test_readonly_property() {
    let output = emit_dts(
        r#"
    export class Config {
        readonly name: string;
        constructor(name: string) { this.name = name; }
    }
    "#,
    );
    assert!(
        output.contains("readonly name: string;"),
        "Expected readonly property: {output}"
    );
}

#[test]
fn test_index_signature_in_class() {
    let output = emit_dts(
        r#"
    export class Dict {
        [key: string]: any;
    }
    "#,
    );
    assert!(
        output.contains("[key: string]: any;"),
        "Expected index signature in class: {output}"
    );
}

#[test]
fn test_index_signature_in_interface() {
    let output = emit_dts(
        r#"
    export interface StringMap {
        [key: string]: string;
    }
    "#,
    );
    assert!(
        output.contains("[key: string]: string;"),
        "Expected index signature in interface: {output}"
    );
}

#[test]
fn test_optional_property_in_interface() {
    let output = emit_dts(
        r#"
    export interface Config {
        name: string;
        debug?: boolean;
    }
    "#,
    );
    assert!(
        output.contains("debug?: boolean;"),
        "Expected optional property: {output}"
    );
}

#[test]
fn test_optional_method_in_interface() {
    let output = emit_dts(
        r#"
    export interface Plugin {
        init?(): void;
    }
    "#,
    );
    assert!(
        output.contains("init?(): void;"),
        "Expected optional method: {output}"
    );
}

#[test]
fn test_optional_computed_method_in_class_emits_optional_property_function_type() {
    let output = emit_dts(
        r#"
    export const dataSomething: `data-${string}` = "data-x" as `data-${string}`;
    export class WithData {
        [dataSomething]?(): string {
            return "something";
        }
    }
    "#,
    );
    // tsc emits optional COMPUTED methods as property signatures with function
    // types (unlike non-computed optional methods which keep method syntax).
    assert!(
        output.contains("[dataSomething]?: (() => string) | undefined;"),
        "Expected optional computed method to emit as property signature: {output}"
    );
}

#[test]
fn test_static_computed_methods_emit_body_inferred_return_types() {
    let output = emit_dts(
        r#"
    export declare const f1: string;
    export declare const f2: string;

    export class Holder {
        static [f1]() {
            return { static: true };
        }
        static [f2]() {
            return { static: "sometimes" };
        }
    }

    export const staticLookup = Holder["x"];
    "#,
    );
    // tsc emits computed methods as method signatures, not property signatures.
    assert!(
        output.contains("static [f1](): {")
            && output.contains("static: boolean;")
            && output.contains("static [f2](): {")
            && output.contains("static: string;"),
        "Expected static computed methods to use method syntax with body-inferred return types: {output}"
    );
}

// =============================================================================
// 11. Function Overloads
// =============================================================================

#[test]
fn test_function_overloads_emit_only_signatures() {
    let output = emit_dts(
        r#"
    export function parse(input: string): number;
    export function parse(input: number): string;
    export function parse(input: any): any { return input; }
    "#,
    );
    // Both overload signatures should be emitted
    assert!(
        output.contains("export declare function parse(input: string): number;"),
        "Expected first overload: {output}"
    );
    assert!(
        output.contains("export declare function parse(input: number): string;"),
        "Expected second overload: {output}"
    );
    // Implementation should NOT be emitted
    assert!(
        !output.contains("input: any): any;"),
        "Implementation signature should not appear: {output}"
    );
}

// =============================================================================
// 12. Interface Heritage
// =============================================================================

#[test]
fn test_interface_extends() {
    let output = emit_dts(
        r#"
    export interface Animal {
        name: string;
    }
    export interface Dog extends Animal {
        breed: string;
    }
    "#,
    );
    assert!(
        output.contains("interface Dog extends Animal"),
        "Expected interface extends: {output}"
    );
}

// =============================================================================
// 13. Private Identifier (#private)
// =============================================================================

#[test]
fn test_private_identifier_emits_private_marker() {
    let output = emit_dts(
        r#"
    export class Foo {
        #secret: number;
        getValue(): number { return this.#secret; }
    }
    "#,
    );
    // Private identifiers should produce `#private;`
    assert!(
        output.contains("#private;"),
        "Expected #private marker for private identifiers: {output}"
    );
    // The actual #secret name should NOT appear
    assert!(
        !output.contains("#secret"),
        "#secret should not appear in .d.ts: {output}"
    );
}
