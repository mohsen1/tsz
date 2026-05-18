//! Display and anchoring regression tests split out of the main assignment
//! checker test module.

use super::diagnostics_for;

#[test]
fn type_parameter_to_template_literal_of_self_emits_ts2322() {
    let source = r#"
function f<T extends "a" | "b">(x: T) {
    const test1: `${T}` = x;
}
"#;
    let diags = diagnostics_for(source);
    let ts2322s: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        !ts2322s.is_empty(),
        "expected TS2322 for `T -> \\`${{T}}\\`` assignment; diagnostics: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    let lhs_diag = ts2322s
        .iter()
        .find(|d| d.message_text.contains("'T'") && d.message_text.contains("`${T}`"))
        .expect("expected TS2322 message naming T and `${T}`");
    let test1_start = source.find("test1").expect("expected variable name") as u32;
    assert_eq!(
        lhs_diag.start, test1_start,
        "TS2322 should anchor at the variable declaration name (test1)"
    );
}

#[test]
fn string_intrinsic_type_parameter_variance_emits_ts2322() {
    let diags = diagnostics_for(
        r#"
function foo<T extends string, U extends T>(x: Uppercase<T>, y: Uppercase<U>) {
    x = y;
    y = x;
}
"#,
    );

    let ts2322s: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322s.len(),
        1,
        "expected only `Uppercase<T> -> Uppercase<U>` to be rejected; got: {diags:?}"
    );
    assert!(
        ts2322s[0]
            .message_text
            .contains("Type 'Uppercase<T>' is not assignable to type 'Uppercase<U>'."),
        "expected intrinsic variance diagnostic to preserve generic intrinsic display; got: {ts2322s:?}"
    );
}

// Companion check: template-literal vs template-literal assignments where
// both sides share a type parameter (e.g. ``${Uppercase<T>}``) must keep
// their existing suppression. This locks in the narrowness of the
// template-literal carve-out so it does not regress
// `templateLiteralTypes3.ts` (where tsc accepts the spread of values typed
// `Uppercase<`1.${T}.4`>` against an inferred `Uppercase<`1.${T}.3`>`).
#[test]
fn template_literal_to_template_literal_with_generic_intrinsic_does_not_emit_ts2345() {
    let source = r#"
type DotString = `${string}.${string}.${string}`;
declare function spread<P extends DotString>(...args: P[]): P;
function ft1<T extends string>(
    u1: Uppercase<`1.${T}.3`>,
    u2: Uppercase<`1.${T}.4`>,
) {
    spread(u1, u2);
}
"#;
    let diags = diagnostics_for(source);
    let ts2345s: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert!(
        ts2345s.is_empty(),
        "template-vs-template generic intrinsic spread must stay suppressed; \
         got TS2345 diagnostics: {:?}",
        ts2345s.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// `function h({ prop = "baz" }: StringUnion)` - when a binding-element default
/// is a non-elaboratable expression (e.g. a string literal that doesn't fit a
/// literal-union target), tsc anchors TS2322 on the binding name (`prop`)
/// rather than the initializer expression (`"baz"`).
///
/// Regression test for
/// `conformance/types/contextualTypes/methodDeclarations/contextuallyTypedBindingInitializerNegative.ts`.
#[test]
fn binding_default_string_lit_anchors_at_binding_name() {
    let source = r#"
interface StringUnion { prop: "foo" | "bar"; }
function h({ prop = "baz" }: StringUnion) {}
"#;
    let diagnostics = diagnostics_for(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322 for non-fitting binding default");

    // Locate the binding name `prop` and the initializer `"baz"` in the
    // source so the assertion stays robust if surrounding text changes.
    let prop_offset = source.find("prop = ").expect("expected `prop = `") as u32;
    let baz_offset = source.find("\"baz\"").expect("expected `\"baz\"`") as u32;

    assert_eq!(
        diag.start, prop_offset,
        "TS2322 should anchor at the binding name `prop` (offset {prop_offset}), \
         not the initializer `\"baz\"` (offset {baz_offset}); got: {diag:?}"
    );
    assert!(
        diag.message_text.contains("\"baz\"") && diag.message_text.contains("\"foo\" | \"bar\""),
        "TS2322 message should still describe the actual mismatch (\"baz\" vs literal union), \
         got: {:?}",
        diag.message_text
    );
}

/// Even though the binding-default anchor walks to the binding name, an arrow
/// function default with a body return-type mismatch (e.g.
/// `function f({ show: x = v => v }: Show)` where `Show.show` returns `string`)
/// should still elaborate to the body expression - the elaboration path
/// (`try_elaborate_function_arg_return_error`) overrides the binding-name
/// anchor with its own body anchor. This test pins that contract.
#[test]
fn binding_default_arrow_body_return_mismatch_still_elaborates_to_body() {
    let source = r#"
interface Show { show: (x: number) => string; }
function f({ show: showRename = v => v }: Show) {}
"#;
    let diagnostics = diagnostics_for(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322 for arrow body return type mismatch");

    // The error must anchor at the second `v` (the body), not at `show:`,
    // `showRename`, or the whole arrow `v => v`.
    let body_offset = {
        let arrow_idx = source.find("v => v").expect("expected `v => v`");
        let body_start = arrow_idx + "v => ".len();
        body_start as u32
    };
    assert_eq!(
        diag.start, body_offset,
        "TS2322 for arrow body return mismatch should anchor at the body expression \
         (offset {body_offset}); got: {diag:?}"
    );
    assert!(
        diag.message_text.contains("'number'") && diag.message_text.contains("'string'"),
        "TS2322 should describe the body return-type mismatch (number vs string), got: {:?}",
        diag.message_text
    );
}

#[test]
fn recursive_mapped_alias_application_display_stays_at_application() {
    let diagnostics = diagnostics_for(
        r#"
type Id2<T> = { [K in keyof T]: Id2<Id2<T[K]>> };
type Foo3 = Id2<{ x: { y: { z: { a: { b: { c: number } } } } } }>;
type Foo4 = Id2<{ x: { y: { z: { a: { b: { c: string } } } } } }>;
declare const foo3: Foo3;
const foo4: Foo4 = foo3;
"#,
    );

    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322 for recursive mapped alias mismatch");
    assert!(
        diag.message_text
            .contains("Id2<{ x: { y: { z: { a: { b: { c: number; }; }; }; }; }; }>")
            && diag
                .message_text
                .contains("Id2<{ x: { y: { z: { a: { b: { c: string; }; }; }; }; }; }>"),
        "TS2322 should preserve the recursive alias application display, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("'Foo3'") && !diag.message_text.contains("'Foo4'"),
        "TS2322 should not repaint the application as wrapper aliases, got: {diag:?}"
    );
}
