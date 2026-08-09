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
        diag.message_text.contains("\"baz\"") && diag.message_text.contains("\"bar\" | \"foo\""),
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

#[test]
fn adjacent_recursive_mapped_alias_applications_both_report() {
    let diagnostics = diagnostics_for(
        r#"
type Id<T> = { [K in keyof T]: Id<T[K]> };
type Foo1 = Id<{ x: { y: { z: { a: { b: { c: number } } } } } }>;
type Foo2 = Id<{ x: { y: { z: { a: { b: { c: string } } } } } }>;
declare const foo1: Foo1;
const foo2: Foo2 = foo1;

type Id2<T> = { [K in keyof T]: Id2<Id2<T[K]>> };
type Foo3 = Id2<{ x: { y: { z: { a: { b: { c: number } } } } } }>;
type Foo4 = Id2<{ x: { y: { z: { a: { b: { c: string } } } } } }>;
declare const foo3: Foo3;
const foo4: Foo4 = foo3;
"#,
    );

    let ts2322_count = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 2322)
        .count();
    assert_eq!(
        ts2322_count, 2,
        "both recursive mapped alias assignments should report TS2322, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message_text
            .contains("Id2<{ x: { y: { z: { a: { b: { c: number; }; }; }; }; }; }>")),
        "second TS2322 should preserve the recursive alias application display, got: {diagnostics:?}"
    );
}

#[test]
fn recursive_mapped_alias_application_rejects_with_renamed_binders() {
    let diagnostics = diagnostics_for(
        r#"
type Recur<Value> = { [Prop in keyof Value]: Recur<Recur<Value[Prop]>> };
type SourceWrap = Recur<{ outer: { inner: number } }>;
type TargetWrap = Recur<{ outer: { inner: string } }>;
declare const source: SourceWrap;
const target: TargetWrap = source;
"#,
    );

    let diag = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == 2322)
        .expect("expected TS2322 for recursive mapped alias mismatch");
    assert!(
        diag.message_text
            .contains("Recur<{ outer: { inner: number; }; }>")
            && diag
                .message_text
                .contains("Recur<{ outer: { inner: string; }; }>"),
        "TS2322 should preserve recursive alias application args independent of binder names, got: {diag:?}"
    );
}

#[test]
fn array_return_to_bare_type_parameter_keeps_target_surface() {
    let diagnostics = diagnostics_for(
        r#"
type Input = { level1: { level2: { foo: string } } };
type Output = { level1: { level2: { foo: string; bar: string } } };
function convert<Result extends Output[]>(ors: Input[]): Result {
    return ors;
}
"#,
    );

    let diag = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == 2322)
        .expect("expected TS2322 for array return to bare type parameter");
    assert!(
        diag.message_text
            .contains("is not assignable to type 'Result'")
            && !diag.message_text.contains("Output[]"),
        "TS2322 should keep the bare type-parameter target instead of repainting its constraint, got: {diag:?}"
    );
    assert!(
        diag.related_information.iter().any(|related| {
            related.code == crate::diagnostics::diagnostic_codes::COULD_BE_INSTANTIATED_WITH_AN_ARBITRARY_TYPE_WHICH_COULD_BE_UNRELATED_TO
                && related.message_text.contains("'Result' could be instantiated with an arbitrary type")
                && related.message_text.contains("Input[]")
        }),
        "TS2322 should include the arbitrary-type elaboration for the bare target parameter, got: {diag:?}"
    );
}

#[test]
fn static_schema_alias_array_return_to_bare_type_parameter_uses_structural_source_display() {
    let diagnostics = diagnostics_for(
        r#"
interface TSchema { static: unknown }
interface TString extends TSchema { static: string }
interface TObject<T extends Record<string, TSchema>> extends TSchema {
    static: { [K in keyof T]: Static<T[K]> }
}
type Static<T extends TSchema> = T["static"];
declare const Type: {
    String(): TString;
    Object<T extends Record<string, TSchema>>(properties: T): TObject<T>;
};

type Input = Static<typeof Input>;
const Input = Type.Object({
    level1: Type.Object({
        level2: Type.Object({
            foo: Type.String(),
        }),
    }),
});

type Output = Static<typeof Output>;
const Output = Type.Object({
    level1: Type.Object({
        level2: Type.Object({
            foo: Type.String(),
            bar: Type.String(),
        }),
    }),
});

function convert<Result extends Output[]>(ors: Input[]): Result {
    return ors;
}
"#,
    );

    let diag = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == 2322)
        .expect("expected TS2322 for static-schema alias array return");
    assert!(
        diag.message_text
            .contains("Type '{ level1: { level2: { foo: string; }; }; }[]' is not assignable to type 'Result'.")
            && !diag.message_text.contains("Input[]")
            && !diag.message_text.contains("Output[]"),
        "TS2322 should structurally display the static-schema alias source while keeping the bare target, got: {diag:?}"
    );
    assert!(
        diag.related_information.iter().any(|related| {
            related.code == crate::diagnostics::diagnostic_codes::COULD_BE_INSTANTIATED_WITH_AN_ARBITRARY_TYPE_WHICH_COULD_BE_UNRELATED_TO
                && related.message_text.contains("'Result' could be instantiated with an arbitrary type")
                && related.message_text.contains("{ level1: { level2: { foo: string; }; }; }[]")
        }),
        "TS2322 should use the structural source display in the arbitrary-type elaboration, got: {diag:?}"
    );
}

// An untyped function literal assigned to a MULTI-call-signature (overload)
// target: `tsc`'s `elaborateArrowFunction` relates the literal's return against
// the UNION of every target signature's return type. When that union relates
// (the literal's body unions the overload parameters, e.g. `(p) => p` gives
// `string | number`), elaboration declines and the outer whole-function
// relation reports a single TS2322 at the binding — NOT an inner TS2322 drilled
// into the arrow body against one overload's return. Regression for #16986.
#[test]
fn arrow_assigned_to_overloaded_target_reports_outer_ts2322_at_binding() {
    let source = r#"
var handler: {
    (p: string): string;
    (p: number): number;
} = (p) => p;
"#;
    let diagnostics = diagnostics_for(source);
    let ts2322s: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322s.len(),
        1,
        "expected exactly one TS2322 for the overloaded-target arrow; got: {diagnostics:?}"
    );
    let diag = ts2322s[0];
    // The whole-function message must appear (outer relation), not the
    // inner-only `string | number -> string` body drill.
    assert!(
        diag.message_text
            .contains("(p: string | number) => string | number")
            && diag.message_text.contains("(p: string): string"),
        "TS2322 should carry the whole-function-type outer message, got: {:?}",
        diag.message_text
    );
    // Anchor at the binding name `handler`, not inside the arrow body.
    let binding_start = source.find("handler").expect("expected binding name") as u32;
    assert_eq!(
        diag.start, binding_start,
        "TS2322 should anchor at the binding name, not the arrow body; got: {diag:?}"
    );
}

// A single-call-signature target with a matching contextual return must remain
// clean — the union collapses to the sole return type, so this stays identical
// to the pre-change first-signature behavior (no spurious diagnostic).
#[test]
fn arrow_assigned_to_single_signature_target_is_accepted() {
    let source = r#"
var identity: {
    (p: string): string;
} = (p) => p;
"#;
    let diagnostics = diagnostics_for(source);
    assert!(
        diagnostics.iter().all(|d| d.code != 2322),
        "single-signature contextual return must not emit TS2322; got: {diagnostics:?}"
    );
}

// An untyped function literal PASSED AS AN ARGUMENT to a multi-signature
// overload parameter: when the contextually-typed literal's whole type still
// fails to satisfy the overload set, `tsc` reports TS2345 at the argument (the
// return union relates, so no inner body drill fires). Regression for #16986:
// the argument-level report must not be suppressed just because the callback
// has unannotated parameters — its parameters DID resolve to concrete types.
#[test]
fn arrow_argument_to_overloaded_parameter_reports_ts2345() {
    let source = r#"
declare function accept(cb: { (p: string): string; (p: number): number; }): void;
accept((p) => p);
"#;
    let diagnostics = diagnostics_for(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2345),
        "expected TS2345 at the overloaded-callback argument; got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code != 2322),
        "the argument mismatch must surface as TS2345, not an inner-body TS2322; got: {diagnostics:?}"
    );
    let diag = diagnostics.iter().find(|d| d.code == 2345).unwrap();
    assert!(
        diag.message_text
            .contains("(p: string | number) => string | number"),
        "TS2345 should display the contextually-typed whole-function source, got: {:?}",
        diag.message_text
    );
}

// A return value genuinely OUTSIDE the union of overload returns still drills
// into the arrow body (matching `tsc`'s `elaborateArrowFunction` when the
// return does not relate to the target-return union). Guards that the union
// change did not disable the legitimate body drill.
#[test]
fn arrow_argument_return_outside_overload_union_drills_to_body() {
    let source = r#"
declare function accept(cb: { (p: string): string; (p: number): number; }): void;
accept((p) => true);
"#;
    let diagnostics = diagnostics_for(source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322 drilled into the arrow body for an out-of-union return");
    assert!(
        diag.message_text.contains("'boolean'") && diag.message_text.contains("'string | number'"),
        "TS2322 should describe the body return-type mismatch (boolean vs string | number), got: {:?}",
        diag.message_text
    );
    // Anchor at the body `true`, not the argument.
    let body_start = source.find("true").expect("expected body expression") as u32;
    assert_eq!(
        diag.start, body_start,
        "TS2322 should anchor at the arrow body expression; got: {diag:?}"
    );
}

// An untyped arrow written as an object-literal PROPERTY value whose declared
// member type is an overload set: the same `elaborateArrowFunction` rule
// applies. The member drill must relate the arrow return against the union of
// the member's signature returns, so `(p) => p` (body `string | number`) does
// not spuriously inner-drill against the first signature's `string`. Regression
// for #16986 — the object-literal-property arrow drill shares the union
// resolver with the argument/assignment sites.
#[test]
fn arrow_object_literal_property_to_overloaded_member_does_not_inner_drill() {
    let source = r#"
const container: { run: { (p: string): string; (p: number): number } } = {
    run: (p) => p,
};
"#;
    let diagnostics = diagnostics_for(source);
    let ts2322s: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322s.len(),
        1,
        "expected exactly one TS2322 for the overloaded-member property arrow; got: {diagnostics:?}"
    );
    // The single diagnostic must NOT be the inner-body drill against `string`.
    assert!(
        !ts2322s[0]
            .message_text
            .contains("Type 'string | number' is not assignable to type 'string'.")
            || ts2322s[0].message_text.contains("(p: string): string"),
        "TS2322 must carry the property/function-type frame, not the inner-only \
         `string | number -> string` body drill; got: {:?}",
        ts2322s[0].message_text
    );
}

// A single-signature object-literal member with a matching contextual return
// stays clean — the union collapses to the sole return, so the alias/inline
// drill behavior is unchanged.
#[test]
fn arrow_object_literal_property_to_single_signature_member_is_accepted() {
    let source = r#"
const container: { run: { (p: string): string } } = {
    run: (p) => p,
};
"#;
    let diagnostics = diagnostics_for(source);
    assert!(
        diagnostics.iter().all(|d| d.code != 2322),
        "single-signature member contextual return must not emit TS2322; got: {diagnostics:?}"
    );
}
