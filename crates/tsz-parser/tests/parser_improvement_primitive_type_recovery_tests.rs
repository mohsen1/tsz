//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — primitive type recovery.

use crate::parser::test_fixture::{parse_source, parse_source_named};

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

// A bare keyword type never accepts type arguments (tsc parses them as
// keyword-type nodes via `tryParse(parseKeywordAndNoDot)`). A relational `<`
// after an `as`/`satisfies` expression whose type is a bare keyword must stay a
// less-than operator, not be mis-parsed as the start of a type-argument list
// (which previously emitted a spurious TS1005 "'>' expected").
#[test]
fn keyword_type_after_as_does_not_consume_relational_less_than() {
    // Exercise several keyword types and operands so the rule is proven for the
    // class, not one spelling.
    for source in [
        "const b = null as any < 1;",
        "const b = null as string < 1;",
        "const b = null as number < 1;",
        "const b = null as unknown < 1;",
        "const b = null as never < 1;",
        "declare const x: any; const b = x as any < 1;",
        "const b = (0) satisfies number < 1;",
        "const b = null as any < 1 as any;",
    ] {
        let (parser, _root) = parse_source(source);
        assert!(
            parser.get_diagnostics().iter().all(|d| d.code != 1005),
            "Relational `<` after a bare keyword `as`/`satisfies` type should not emit \
             TS1005 for `{source}`, got {:?}",
            parser.get_diagnostics()
        );
    }
}

#[test]
fn keyword_type_after_as_relational_less_than_is_clean_in_tsx() {
    // The same rule must hold in `.tsx`, where `<` is also JSX-sensitive.
    let (parser, _root) = parse_source_named("test.tsx", "const b = null as any < 1;\n");
    assert!(
        parser.get_diagnostics().iter().all(|d| d.code != 1005),
        "Relational `<` after `as any` should not emit TS1005 in .tsx, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn bare_keyword_type_with_type_arguments_is_rejected_like_tsc() {
    // tsc rejects type arguments on a bare keyword type (e.g. `any<number>`):
    // the keyword node consumes no `<...>`, so the leftover `<` triggers a
    // "',' expected." diagnostic in the declaration. Previously tsz silently
    // accepted `any<number>` as a generic type reference.
    for source in [
        "let x: any<number>;",
        "let x: string<number>;",
        "let x: never<number>;",
    ] {
        let (parser, _root) = parse_source(source);
        assert!(
            parser.get_diagnostics().iter().any(|d| d.code == 1005),
            "A bare keyword type with type arguments should be rejected (TS1005) for \
             `{source}`, got {:?}",
            parser.get_diagnostics()
        );
    }
}

#[test]
fn dotted_and_generic_type_references_still_take_type_arguments() {
    // The keyword rule must not regress real generic type references, including
    // dotted names and renamed type parameters.
    for source in [
        "type Box<T> = T; let x: Box<number>;",
        "type Pair<A, B> = [A, B]; let y: Pair<string, number>;",
        "declare namespace N { export type B<T> = T; } let z: N.B<number>;",
    ] {
        let (parser, _root) = parse_source(source);
        assert!(
            parser.get_diagnostics().is_empty(),
            "Generic type references should still parse cleanly for `{source}`, got {:?}",
            parser.get_diagnostics()
        );
    }
}
