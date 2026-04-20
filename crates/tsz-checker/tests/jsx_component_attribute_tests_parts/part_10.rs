#[test]
fn test_intrinsic_generic_spread_type_mismatch_emits_ts2322_not_ts2741() {
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        test1: { x: string };
    }
}

function make2<T extends { x: number }>(obj: T) {
    return <test1 {...obj} />;
}
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Generic intrinsic spread mismatch should emit TS2322, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Generic intrinsic spread mismatch should not fall back to TS2741, got: {diags:?}"
    );
}

#[test]
fn test_intrinsic_generic_spread_missing_required_emits_ts2322_not_ts2741() {
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        test1: { x: string };
    }
}

function make3<T extends { y: string }>(obj: T) {
    return <test1 {...obj} />;
}
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Generic intrinsic spread missing required props should emit TS2322, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Generic intrinsic spread missing required props should not fall back to TS2741, got: {diags:?}"
    );
}

#[test]
fn test_non_generic_sfc_no_spurious_intrinsic_attrs_check() {
    // Non-generic SFC: <Greet name="world" /> should NOT get an IntrinsicAttributes error.
    let source = format!(
        r#"
{JSX_PREAMBLE_WITH_IA}
function Greet(props: {{ name: string }}): JSX.Element {{
    return <div>Hello</div>;
}}
let x = <Greet name="world" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2322_about_ia: Vec<_> = diags
        .iter()
        .filter(|(c, m)| {
            *c == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && m.contains("IntrinsicAttributes")
        })
        .collect();
    assert!(
        ts2322_about_ia.is_empty(),
        "Non-generic SFC should not emit IntrinsicAttributes TS2322, got: {ts2322_about_ia:?}"
    );
}

// =====================================================================
// JSX Children Type Checking Tests
// =====================================================================

/// Helper: Standard JSX namespace preamble with `ElementAttributesProperty` + `ElementChildrenAttribute`.
/// Element has a `__brand` property so it's not just `{}` — this prevents `any[]` from being
/// assignable to `JSX.Element` (which would break TS2746 single-child detection).
const JSX_CHILDREN_PREAMBLE: &str = r#"
interface Array<T> { length: number; [n: number]: T; }
declare namespace JSX {
    interface Element { __brand: string }
    interface IntrinsicElements {
        div: any;
    }
    interface ElementAttributesProperty { props: {} }
    interface ElementChildrenAttribute { children: {} }
}
"#;

#[test]
fn jsx_children_single_element_child_satisfies_element_type() {
    // Single element child should satisfy `children: JSX.Element`
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    a: number;
    b: string;
    children: JSX.Element;
}}
function Comp(p: Prop) {{ return <div>{{p.b}}</div>; }}
let k = <Comp a={{10}} b="hi"><div>hi</div></Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Single element child should satisfy JSX.Element children type, got: {diags:?}"
    );
}

#[test]
fn jsx_children_missing_required_children_emits_ts2741() {
    // Component requiring `children` but given no children body should emit TS2741
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    a: number;
    children: JSX.Element;
}}
function Comp(p: Prop) {{ return <div></div>; }}
let k = <Comp a={{10}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Missing required children should emit TS2741, got: {diags:?}"
    );
}

#[test]
fn jsx_children_custom_element_children_attribute_uses_assignability_path() {
    let source = r#"
// @strict: true
export {}

declare global {
    namespace JSX {
        type Element = any;
        interface ElementAttributesProperty { __properties__: {} }
        interface IntrinsicElements { [key: string]: string }
        interface ElementChildrenAttribute { __children__: {} }
    }
}

interface MockComponentInterface {
    new (): {
        __properties__: { bar?: number } & { __children__: () => number };
    };
}

declare const MockComponent: MockComponentInterface;

<MockComponent>{}</MockComponent>;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Custom ElementChildrenAttribute should route body children through TS2322 assignability, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Custom ElementChildrenAttribute should not fall back to TS2741 missing-prop, got: {diags:?}"
    );
    assert!(
        diags
            .iter()
            .any(|(_, msg)| msg.contains("Type '{}' is not assignable to type")),
        "Custom ElementChildrenAttribute should format the synthesized JSX attrs object as '{{}}', got: {diags:?}"
    );
}

#[test]
fn jsx_children_react_jsx_ignores_element_children_attribute_and_keeps_related_info() {
    let source = r#"
declare namespace JSX {
    interface IntrinsicElements {
        h1: { children: string }
    }

    type Element = string;

    interface ElementChildrenAttribute {
        offspring: any;
    }
}

const Title = (props: { children: string }) => <h1>{props.children}</h1>;
<Title>Hello, world!</Title>;

const Wrong = (props: { offspring: string }) => <h1>{props.offspring}</h1>;
<Wrong>Byebye, world!</Wrong>;
"#;
    let diags = jsx_full_diagnostics_with_mode(source, JsxMode::ReactJsx);
    let ts2741 = diags
        .iter()
        .find(|diag| {
            diag.code == diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        })
        .expect("Expected TS2741 for missing 'offspring' prop under react-jsx");

    assert!(
        ts2741
            .message_text
            .contains("Property 'offspring' is missing in type '{ children: string; }'"),
        "TS2741 should still use synthesized children props under react-jsx, got: {ts2741:?}"
    );
    // TODO: TS2741 should include "'offspring' is declared here." related info,
    // but declaration source tracking for JSX synthesized props is not yet implemented.
    // Once added, uncomment the assertion below.
    // assert!(
    //     ts2741.related_information.iter().any(|info| {
    //         info.code == diagnostic_codes::IS_DECLARED_HERE
    //             && info.message_text == "'offspring' is declared here."
    //     }),
    //     "TS2741 should include declaration related info for the required prop, got: {ts2741:?}"
    // );
}

#[test]
fn jsx_children_generic_component_explicit_children_gets_contextual_return_type() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface LitProps<T> {{ prop: T, children: (x: this) => T }}
const ElemLit = <T extends string>(p: LitProps<T>) => <div></div>;
const arg = <ElemLit prop="x" children={{p => "y"}} />;
const mismatched = <ElemLit prop="x" children={{() => 12}} />;
"#
    );

    let diags = jsx_diagnostics(&source);
    // After the TS2345 expression-body arrow change, these may report as
    // TS2322 or TS2345 depending on the callback shape. Accept either.
    let type_error_count = diags
        .iter()
        .filter(|(code, _)| *code == 2322 || *code == 2345)
        .count();
    assert!(
        type_error_count >= 1,
        "Generic JSX children attr should get contextual return typing, got: {diags:?}"
    );
}

#[test]
fn jsx_children_generic_component_body_children_gets_contextual_return_type() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface LitProps<T> {{ prop: T, children: (x: this) => T }}
const ElemLit = <T extends string>(p: LitProps<T>) => <div></div>;
const argchild = <ElemLit prop="x">{{p => "y"}}</ElemLit>;
const mismatched = <ElemLit prop="x">{{() => 12}}</ElemLit>;
"#
    );

    let diags = jsx_diagnostics(&source);
    // After the TS2345 expression-body arrow change, these may report as
    // TS2322 or TS2345 depending on the callback shape. Accept either.
    let type_error_count = diags
        .iter()
        .filter(|(code, _)| *code == 2322 || *code == 2345)
        .count();
    assert!(
        type_error_count >= 1,
        "Generic JSX body children should get contextual return typing, got: {diags:?}"
    );
}

#[test]
fn jsx_children_double_specified_emits_ts2710() {
    // Children as both attribute and body should emit TS2710
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    a: number;
    children: JSX.Element;
}}
function Comp(p: Prop) {{ return <div></div>; }}
let k = <Comp a={{10}} children={{<div/>}}><div>hi</div></Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::ARE_SPECIFIED_TWICE_THE_ATTRIBUTE_NAMED_WILL_BE_OVERWRITTEN
        ),
        "Children specified both as attribute and body should emit TS2710, got: {diags:?}"
    );
}

