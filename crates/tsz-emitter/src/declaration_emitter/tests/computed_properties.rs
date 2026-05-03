use super::*;

// =============================================================================
// Computed property names in declaration emit
// =============================================================================

#[test]
fn test_object_literal_computed_property_names_no_crash() {
    // Regression test: the declaration emitter used to panic with
    // "insertion index should be <= len" when an object literal had multiple
    // computed property members and some matched existing printed lines while
    // others didn't. The `offset` counter incremented per loop iteration
    // rather than per actual insertion, causing out-of-bounds Vec::insert.
    let output = emit_dts(
        r#"
export const D = {
    [Symbol.iterator]: 1,
    [1]: 2,
    ["2"]: 3,
};
"#,
    );
    // Should not crash — any output is acceptable
    assert!(!output.is_empty(), "Expected non-empty declaration output");
}

#[test]
fn test_interface_computed_property_legal_names_emitted() {
    // Numeric and string literal computed property names are legal in .d.ts
    let output = emit_dts(
        r#"
export interface Foo {
    [1]: number;
    ["hello"]: string;
}
"#,
    );
    assert!(
        output.contains("[1]") || output.contains("1:"),
        "Expected numeric computed property to be emitted: {output}"
    );
    assert!(
        output.contains("hello"),
        "Expected string literal computed property to be emitted: {output}"
    );
}

#[test]
fn test_type_alias_computed_property_names_no_crash() {
    let output = emit_dts(
        r#"
export type A = {
    [Symbol.iterator]: number;
    [1]: number;
    ["2"]: number;
};
"#,
    );
    assert!(
        !output.is_empty(),
        "Expected non-empty output for type alias with computed properties"
    );
}

#[test]
fn test_class_computed_property_names_no_crash() {
    let output = emit_dts(
        r#"
export class C {
    [Symbol.iterator]: number = 1;
    [1]: number = 1;
    ["2"]: number = 1;
}
"#,
    );
    assert!(
        !output.is_empty(),
        "Expected non-empty output for class with computed properties"
    );
}

#[test]
fn test_const_enum_computed_method_keeps_method_syntax() {
    // Computed method names referencing const enum members should use method
    // syntax `[G.A](): void` not property syntax `[G.A]: () => void`, because
    // const enum values are always literals (valid property names in .d.ts).
    let output = emit_dts_with_binding(
        r#"
const enum G {
    A = 0,
    B = 1,
}
export class C {
    [G.A]() { }
    get [G.B]() {
        return true;
    }
    set [G.B](x: number) { }
}
"#,
    );
    assert!(
        !output.contains("[G.A]: () =>"),
        "Expected method syntax not property syntax for const enum computed method: {output}"
    );
}

#[test]
fn test_inline_mapped_type_emits_as_clause_and_value_type() {
    // Inline mapped types inside type literals must emit the `as` clause
    // correctly (before `]`, not as `: `) and must emit the value type.
    let output = emit_dts(
        r#"
export type Remap<T> = {
    [K in keyof T as K extends string ? `get_${K}` : never]: T[K];
};
"#,
    );
    assert!(
        output.contains(" as K extends string ? `get_${K}` : never]"),
        "Expected 'as' clause for key remapping in mapped type: {output}"
    );
    assert!(
        output.contains("]: T[K];"),
        "Expected value type T[K] in mapped type: {output}"
    );
    assert!(
        !output.contains("]: ;"),
        "Must not emit empty value type in mapped type: {output}"
    );
}

#[test]
fn test_override_modifier_stripped_in_dts() {
    // tsc strips `override` from class members in .d.ts output —
    // it is not part of the declaration surface.
    let output = emit_dts(
        r#"
declare class Base {
    method(): void;
    prop: number;
}
export declare class Derived extends Base {
    override method(): void;
    override prop: number;
}
"#,
    );
    assert!(
        !output.contains("override"),
        "Expected override modifier to be stripped in .d.ts: {output}"
    );
    assert!(
        output.contains("method(): void;"),
        "Expected method in .d.ts: {output}"
    );
    assert!(
        output.contains("prop: number;"),
        "Expected prop in .d.ts: {output}"
    );
}

#[test]
fn test_export_default_class_emits_parameter_properties() {
    // `export default class` with constructor parameter properties must emit
    // the properties as class members, same as non-default exported classes.
    let output = emit_dts(
        r#"
export default class Foo {
    constructor(public x: number, private y: string) {}
}
"#,
    );
    assert!(
        output.contains("x: number;"),
        "Expected parameter property 'x' as class member in export default class: {output}"
    );
    assert!(
        output.contains("private y;"),
        "Expected private parameter property 'y' in export default class: {output}"
    );
}

#[test]
fn test_non_ambient_namespace_strips_export_keyword_from_members() {
    // Non-ambient namespaces gain `declare` in .d.ts output, making them
    // ambient. Members inside should not have `export` keyword unless
    // there is a scope marker.
    let output = emit_dts(
        r#"
export namespace Utils {
    export function helper(): void;
    export interface Options {
        verbose: boolean;
    }
}
"#,
    );
    assert!(
        output.contains("function helper(): void;"),
        "Expected 'function helper' without export keyword: {output}"
    );
    assert!(
        output.contains("interface Options"),
        "Expected 'interface Options' without export keyword: {output}"
    );
    assert!(
        !output.contains("export function helper"),
        "Should not have 'export function' inside declare namespace: {output}"
    );
    assert!(
        !output.contains("export interface Options"),
        "Should not have 'export interface' inside declare namespace: {output}"
    );
}

#[test]
fn test_declare_global_augmentation_emitted_in_module_file() {
    // `declare global { ... }` should be emitted even when the file
    // has exports (public API filter enabled).
    let output = emit_dts(
        r#"
export function foo(): void;
declare global {
    interface String {
        customMethod(): void;
    }
}
"#,
    );
    assert!(
        output.contains("declare global"),
        "Expected 'declare global' block in output: {output}"
    );
    assert!(
        output.contains("customMethod(): void;"),
        "Expected customMethod in declare global block: {output}"
    );
    // Should not have 'namespace global' instead of 'global'
    assert!(
        !output.contains("namespace global"),
        "Should emit 'declare global' not 'declare namespace global': {output}"
    );
}

#[test]
fn test_declare_module_augmentation_emitted_in_module_file() {
    // `declare module "foo" { ... }` should be emitted even when the
    // file has exports (public API filter enabled).
    let output = emit_dts(
        r#"
export {};
declare module "some-module" {
    interface SomeType {
        x: number;
    }
}
"#,
    );
    assert!(
        output.contains("declare module \"some-module\""),
        "Expected 'declare module \"some-module\"' in output: {output}"
    );
    assert!(
        output.contains("interface SomeType"),
        "Expected SomeType interface in module augmentation: {output}"
    );
}

#[test]
fn test_module_augmentation_does_not_trigger_extra_export_marker() {
    // Module augmentations should not cause an extra `export {};` to be
    // emitted when the file already has a scope marker.
    let output = emit_dts(
        r#"
export function foo(): void;
declare global {
    interface Window {
        myProp: string;
    }
}
"#,
    );
    // The file has `export function foo` which is a module indicator,
    // so no extra `export {};` should appear.
    let export_marker_count = output.matches("export {};").count();
    assert_eq!(
        export_marker_count, 0,
        "Should not have extra 'export {{}}' marker when declare global is present: {output}"
    );
}

#[test]
fn test_export_default_interface_emits_correctly() {
    // `export default interface` should be emitted as
    // `export default interface Name { ... }` not as
    // `declare const _default: any; export default _default;`.
    let output = emit_dts(
        r#"
export default interface MyInterface {
    x: number;
    y: string;
}
"#,
    );
    assert!(
        output.contains("export default interface MyInterface"),
        "Expected 'export default interface MyInterface': {output}"
    );
    assert!(
        output.contains("x: number;"),
        "Expected 'x: number' member in interface: {output}"
    );
    assert!(
        output.contains("y: string;"),
        "Expected 'y: string' member in interface: {output}"
    );
    // Must not produce the fallback `any` pattern
    assert!(
        !output.contains("_default"),
        "Should not fall back to _default pattern: {output}"
    );
}

#[test]
fn test_export_default_interface_with_generics_and_heritage() {
    let output = emit_dts(
        r#"
interface Base { base: boolean; }
export default interface Extended<T> extends Base {
    value: T;
}
"#,
    );
    assert!(
        output.contains("export default interface Extended<T> extends Base"),
        "Expected interface with generics and extends: {output}"
    );
}

#[test]
fn test_union_in_intersection_gets_parenthesized() {
    // `(string | number) & { tag: "complex" }` must preserve parentheses
    // around the union to maintain correct operator precedence. Without
    // them, `string | number & { tag: "complex" }` means
    // `string | (number & { tag: "complex" })`.
    let output = emit_dts(
        r#"
export type Complex = (string | number) & { tag: "complex" };
"#,
    );
    assert!(
        output.contains("(string | number) & {"),
        "Expected parenthesized union in intersection type: {output}"
    );
}

#[test]
fn test_export_default_class_skips_overload_implementation() {
    // `export default class` with method overloads should skip the
    // implementation signature, same as non-default exported classes.
    let output = emit_dts(
        r#"
export default class Bar {
    method(x: number): number;
    method(x: string): string;
    method(x: number | string): number | string {
        return x;
    }
}
"#,
    );
    let method_count = output.matches("method(").count();
    assert_eq!(
        method_count, 2,
        "Expected exactly 2 overload signatures (not implementation) in export default class, got {method_count}: {output}"
    );
}

#[test]
fn test_namespace_non_exported_type_used_by_export_emits_scope_marker() {
    // When a non-ambient namespace has a non-exported type alias referenced
    // by an exported member, tsc emits the type alias and adds `export {};`.
    let output = emit_dts_with_usage_analysis(
        r#"
namespace M {
    type W = string | number;
    export namespace N {
        export class Window {}
        export var p: W;
    }
}
"#,
    );
    assert!(
        output.contains("type W = string | number;"),
        "Expected non-exported type alias 'W' to be emitted (referenced by exported member): {output}"
    );
    assert!(
        output.contains("export namespace N"),
        "Expected 'export namespace N' to preserve export keyword: {output}"
    );
    assert!(
        output.contains("export {};"),
        "Expected 'export {{}};' scope marker in namespace with mixed exports: {output}"
    );
}

#[test]
fn test_non_ambient_namespace_unused_aliases_are_elided() {
    let output = emit_dts_with_usage_analysis(
        r#"
declare namespace External {
    interface Thing {}
}

namespace M {
    import Alias = External;
    class Hidden {
        value: string;
    }

    export interface Visible {
        value: string;
    }
}
"#,
    );

    assert!(
        !output.contains("import Alias = External;"),
        "Expected unused non-exported import alias to be elided inside namespace: {output}"
    );
    assert!(
        !output.contains("class Hidden"),
        "Expected unused non-exported class to be elided inside namespace: {output}"
    );
    assert!(
        output.contains("declare namespace M"),
        "Expected namespace to be preserved: {output}"
    );
    assert!(
        output.contains("interface Visible"),
        "Expected exported interface body member to be preserved: {output}"
    );
}

#[test]
fn test_non_ambient_empty_inner_namespace_conflict_is_elided() {
    let output = emit_dts_with_usage_analysis(
        r#"
namespace X.A.C {
    export interface Z {
    }
}
namespace X.A.B.C {
    namespace A {
    }
    export class W implements X.A.C.Z {
    }
}
"#,
    );

    assert!(
        output.contains("class W implements X.A.C.Z"),
        "Expected heritage reference to preserve the outer namespace path: {output}"
    );
    assert!(
        !output.contains("namespace A { }"),
        "Expected empty non-exported inner namespace to be elided: {output}"
    );
    assert!(
        !output.contains("export {};"),
        "Expected elided empty namespace not to trigger a scope marker: {output}"
    );
}

#[test]
fn test_non_ambient_later_empty_inner_namespace_conflict_is_elided() {
    let output = emit_dts_with_usage_analysis(
        r#"
namespace X.A.C {
    export interface Z {
    }
}
namespace X.A.B.C {
    export class W implements A.C.Z {
    }
}

namespace X.A.B.C {
    namespace A {
    }
}
"#,
    );

    assert!(
        output.contains("class W implements A.C.Z"),
        "Expected heritage reference to remain context-relative: {output}"
    );
    assert!(
        !output.contains("namespace A { }"),
        "Expected later empty non-exported inner namespace to be elided: {output}"
    );
    assert!(
        !output.contains("export {};"),
        "Expected elided empty namespace not to trigger a scope marker: {output}"
    );
}

#[test]
fn test_non_ambient_exported_empty_inner_namespace_is_preserved() {
    let output = emit_dts_with_usage_analysis(
        r#"
namespace X.A.C {
    export interface Z {
    }
}
namespace X.A.B.C {
    export class W implements X.A.C.Z {
    }
}

namespace X.A.B.C {
    export namespace A {
    }
}
"#,
    );

    assert!(
        output.contains("namespace A { }"),
        "Expected exported empty inner namespace to be preserved inside the declare namespace: {output}"
    );
}
