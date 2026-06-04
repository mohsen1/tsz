#[test]
fn test_jsx_explicit_type_args_too_many_with_defaults_emits_ts2558() {
    // <MyComp2<A, B, C> /> — MyComp2 has at most 2 type params, got 3
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Prop {{ a: string }}
declare class MyComp2<P extends {{ a: string }}, P2 = {{}}> extends Object {{
    props: P;
}}
let x = <MyComp2<{{a: string}}, {{b: string}}, Prop> a="hi" />;
"#
    );
    let codes = jsx_codes(&source);
    assert!(
        codes.contains(&2558),
        "TS2558 should fire when more type args are given than the max (1-2), got: {codes:?}"
    );
}

/// JSX namespace with NO `ElementAttributesProperty` — tsc uses first constructor param as props type.
const JSX_NO_ELEMENT_ATTRS_PREAMBLE: &str = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {}
}
"#;

#[test]
fn test_jsx_class_constructor_primitive_param_no_elem_attrs_prop_emits_ts2322_at_tag() {
    // When JSX.ElementAttributesProperty is absent, tsc uses the first constructor
    // parameter as the props type even when it is a primitive (e.g. `string`).
    // The synthesized attrs object `{ x: number }` is then checked against `string`
    // → TS2322 must be anchored at the tag name (col 2), NOT per-attribute (col 7).
    let source = format!(
        r#"
{JSX_NO_ELEMENT_ATTRS_PREAMBLE}
interface Obj1type {{
    new(n: string): any;
}}
declare var Obj1: Obj1type;
<Obj1 x={{1}} />;
"#
    );
    // Obj1 returns `any` → no TS2322 expected (any swallows attribute checks).
    let codes = jsx_codes(&source);
    assert!(
        !codes.contains(&2322),
        "TS2322 should NOT fire for class component that returns `any`, got: {codes:?}"
    );
}

#[test]
fn test_jsx_class_constructor_primitive_param_no_elem_attrs_prop_obj_return_emits_ts2322() {
    // Class component with `new(n: string): { render(): any }` and no ElementAttributesProperty.
    // tsc uses first param (`string`) as props type → `{ x: number }` not assignable to `string`.
    // TS2322 must be at tag position (whole-object), not per-attribute.
    let source = format!(
        r#"
{JSX_NO_ELEMENT_ATTRS_PREAMBLE}
interface Obj2type {{
    new(n: string): {{ render(): any }};
}}
declare var Obj2: Obj2type;
<Obj2 x={{1}} render={{2}} />;
"#
    );
    let diags = jsx_diagnostics_with_pos(&source);
    let ts2322_diags: Vec<_> = diags.iter().filter(|(code, _, _)| *code == 2322).collect();
    assert!(
        !ts2322_diags.is_empty(),
        "TS2322 should fire for class component with primitive param and mismatched attrs, got: {diags:?}"
    );
    // Verify the message includes the whole attrs object (not a single attribute)
    let msg = &ts2322_diags[0].2;
    assert!(
        msg.contains("{ x: number; render: number; }")
            || msg.contains("not assignable to type 'string'"),
        "TS2322 message should show whole attrs object vs string, got: {msg:?}"
    );
    // Verify x appears before render (declaration order preserved)
    if msg.contains("{ x: number; render: number; }") {
        let x_pos = msg.find("x: number").unwrap_or(usize::MAX);
        let render_pos = msg.find("render: number").unwrap_or(usize::MAX);
        assert!(
            x_pos < render_pos,
            "Property 'x' should appear before 'render' in TS2322 message (declaration order), got: {msg:?}"
        );
    }
}

#[test]
fn test_generic_jsx_function_attr_error_anchors_at_attribute_not_body() {
    // When a function-valued JSX attribute produces a body-level type error,
    // tsc suppresses the body-level error and anchors the TS2322 at the
    // attribute name. The target type displays as the intersection of the
    // actual (inferred) function type and the expected (declared) function type.
    let lib_source = r#"
declare namespace JSX {
    interface Element {}
    interface ElementClass {
        render(): any;
    }
    interface IntrinsicElements {}
    interface ElementAttributesProperty {
        props: {};
    }
}
declare namespace React {
    class Component<P, S> {
        props: P;
        state: S;
    }
}
"#;

    let main_source = r#"
interface BaseProps<T> {
    initialValues: T;
    nextValues: (cur: T) => T;
}
declare class MyComponent<Props = {}, Values = {}> extends React.Component<Props & BaseProps<Values>, {}> {
    iv: Values;
}
// The function body returns `string` but the expected return type is `{ x: string }`.
// TS2322 should fire at the attribute anchor, and the target should show the
// intersection of the actual and expected callable types.
let d = <MyComponent initialValues={{ x: "y" }} nextValues={a => a.x} />;
"#;

    let diags = cross_file_jsx_diagnostics(lib_source, main_source);
    let ts2322_diags: Vec<_> = diags
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        !ts2322_diags.is_empty(),
        "Expected TS2322 for mismatched function-valued attr return type, got: {diags:?}"
    );

    for (_, message) in &ts2322_diags {
        assert!(
            message.contains(" & "),
            "TS2322 target should show intersection of callable types with '&', got: {message}"
        );
    }
}

/// When a class JSX component's constructor takes a primitive first parameter
/// (e.g. `new(n: string): { x: number; render(): void }`) AND the JSX
/// namespace's `IntrinsicAttributes` declares a required prop the caller did
/// not pass (typically `key`), tsc reports ONLY `TS2741` for the missing
/// required `IntrinsicAttributes` prop. It does NOT also emit `TS2322` for
/// whole-attrs assignability against the primitive props type, because
/// primitive props can never structurally accept JSX attributes — the
/// assignability failure is uninformative when TS2741 already conveys the
/// user-actionable error.
///
/// Mirrors the conformance test
/// `conformance/jsx/tsxIntrinsicAttributeErrors.tsx`.
#[test]
fn jsx_class_primitive_props_with_missing_intrinsic_required_emits_only_ts2741_not_ts2322() {
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface ElementClass { render: any; }
    interface IntrinsicAttributes { key: string | number }
    interface IntrinsicClassAttributes<T> { ref: T }
    interface IntrinsicElements { div: { text?: string }; span: any; }
}
interface I { new(n: string): { x: number; render(): void } }
declare var E: I;
<E x={10} />
"#;
    let codes = jsx_codes(source);
    assert!(
        codes.contains(&diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE),
        "Expected TS2741 (missing required `key`) for class JSX with primitive props, got: {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected NO TS2322 for class JSX whole-attrs assignability against primitive props, got: {codes:?}"
    );
}

/// Regression test for issue #3227: `JSX.LibraryManagedAttributes` was being
/// discarded whenever the formatted evaluated props type happened to contain
/// the substring `Factory<`. That was a display-text heuristic, not a
/// semantic condition, so any user type named `Factory` (or anything else
/// whose printed form started with `Factory<`) silently broke LMA.
///
/// Structural rule: when a component has `defaultProps`, the props returned
/// from `JSX.LibraryManagedAttributes<C, Props>` must reflect the mapped
/// optional-property result regardless of the names of types appearing in
/// the props.
fn jsx_lma_user_type_named_factory_does_not_disable_default_props_helper(
    user_type_name: &str,
) -> Vec<u32> {
    let source = format!(
        r#"
declare namespace JSX {{
    interface Element {{}}
    interface ElementClass {{}}
    interface IntrinsicElements {{}}
    type LibraryManagedAttributes<C, P> =
        C extends {{ defaultProps: infer D }}
          ? {{ [K in keyof P]?: P[K] }}
          : P;
}}

interface {user_type_name}<T> {{
    make(): T;
}}

interface Props {{
    value: {user_type_name}<number>;
    other: number;
}}

declare function Comp(props: Props): JSX.Element;
declare namespace Comp {{
    const defaultProps: {{
        value: {user_type_name}<number>;
    }};
}}

const _ok = <Comp />;
"#
    );
    jsx_codes(&source)
}

#[test]
fn jsx_lma_user_type_named_factory_does_not_disable_default_props() {
    // Reproduces the issue: a user type literally named `Factory` must not
    // suppress the LMA-mapped optional props.
    let codes = jsx_lma_user_type_named_factory_does_not_disable_default_props_helper("Factory");
    assert!(
        !codes.contains(&diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE),
        "User type named `Factory` should not disable JSX.LibraryManagedAttributes; \
         expected no TS2741 for `<Comp />`, got: {codes:?}"
    );
}
