#[test]
fn test_union_props_class_component_missing_required_emits_ts2322() {
    // Class components with union props: mode="write" matches the second member's
    // discriminant, but `value` is required and missing. Neither union member is
    // fully satisfied, so TS2322 should fire.
    let source = format!(
        r#"
{JSX_PREAMBLE}
type Props = {{ mode: "read" }} | {{ mode: "write"; value: string }};
declare class Editor {{
    constructor(props: Props);
    props: Props;
    render(): JSX.Element;
}}
let x = <Editor mode="write" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for union props with missing required property, got: {diags:?}"
    );
}

#[test]
fn test_union_of_class_component_types_missing_required_emits_ts2741() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare namespace React {{
    class Component<P, S> {{
        props: P;
    }}
}}

class RC1 extends React.Component<{{ x: number }}, {{}}> {{}}
class RC4 extends React.Component<{{}}, {{}}> {{}}

var PartRCComp = RC1 || RC4;
let a = <PartRCComp />;
let b = <PartRCComp data-extra="hello" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2741_count = diags
        .iter()
        .filter(|(code, _)| {
            *code == diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        })
        .count();
    assert_eq!(
        ts2741_count, 2,
        "Expected one TS2741 per JSX use of a component-type union with missing required props, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should not fall back to TS2322 for component-type unions missing required props, got: {diags:?}"
    );
}

// =============================================================================
// Diagnostic anchor: JSX attribute errors should point at the attribute, not
// the enclosing variable statement.
// =============================================================================

#[test]
fn test_jsx_attr_error_anchors_at_attribute_not_variable_statement() {
    // TS2322 for JSX attribute type mismatch should point at the attribute name,
    // not at the `let` statement. The attribute `name={42}` should be the anchor.
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Greet(props: {{ name: string }}) {{
    return <div>Hello</div>;
}}
let p = <Greet name={{42}} />;
"#
    );
    let diags = jsx_diagnostics_with_pos(&source);
    let ts2322 = diags
        .iter()
        .filter(|(c, _, _)| *c == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect::<Vec<_>>();
    assert!(!ts2322.is_empty(), "Expected TS2322, got: {diags:?}");
    // The error should NOT point at the `let` keyword (start of the variable statement)
    let let_pos = source.find("let p").unwrap() as u32;
    let attr_pos = source.find("name={42}").unwrap() as u32;
    for (_, start, _) in &ts2322 {
        assert!(
            *start >= attr_pos,
            "TS2322 should anchor at attribute name (pos >= {attr_pos}), not at variable statement (pos {let_pos}). Got start={start}"
        );
    }
}

// =============================================================================
// Boolean shorthand: `<Foo x/>` should report `Type 'true'` not `Type 'boolean'`
// =============================================================================

#[test]
fn test_boolean_shorthand_reports_true_not_boolean() {
    // When target is `false`, `<Foo x/>` (x=true) should produce
    // "Type 'true' is not assignable to type 'false'",
    // not "Type 'boolean' is not assignable to type 'false'".
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Foo(props: {{ x: false }}) {{
    return <div />;
}}
let p = <Foo x />;
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2322_msgs: Vec<&str> = diags
        .iter()
        .filter(|(c, _)| *c == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, m)| m.as_str())
        .collect();
    assert!(!ts2322_msgs.is_empty(), "Expected TS2322, got: {diags:?}");
    // Should say 'true', not 'boolean'
    let has_true = ts2322_msgs.iter().any(|m| m.contains("'true'"));
    let has_boolean = ts2322_msgs.iter().any(|m| m.contains("'boolean'"));
    assert!(
        has_true && !has_boolean,
        "Expected message with 'true' not 'boolean'. Got: {ts2322_msgs:?}"
    );
}

#[test]
fn test_boolean_shorthand_reports_boolean_when_target_is_not_boolean_literal() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Foo(props: {{ x: string }}) {{
    return <div />;
}}
let p = <Foo x />;
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2322_msgs: Vec<&str> = diags
        .iter()
        .filter(|(c, _)| *c == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, m)| m.as_str())
        .collect();
    assert!(!ts2322_msgs.is_empty(), "Expected TS2322, got: {diags:?}");
    let has_boolean = ts2322_msgs.iter().any(|m| m.contains("'boolean'"));
    assert!(
        has_boolean,
        "Expected message with 'boolean'. Got: {ts2322_msgs:?}"
    );
}

#[test]
fn test_explicit_attr_reports_boolean_target_for_string_value() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Foo(props: {{ x: string; n: boolean }}) {{
    return <div />;
}}
let p = <Foo x="ok" n="bad" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2322_msgs: Vec<&str> = diags
        .iter()
        .filter(|(c, _)| *c == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, m)| m.as_str())
        .collect();
    assert!(!ts2322_msgs.is_empty(), "Expected TS2322, got: {diags:?}");
    let has_explicit_target = ts2322_msgs
        .iter()
        .any(|m| m.contains("Type 'string' is not assignable to type 'boolean'"));
    assert!(
        has_explicit_target,
        "Expected explicit attribute mismatch against boolean target. Got: {ts2322_msgs:?}"
    );
}

// =============================================================================
// TS2741 source type formatting: should show types, not just property names
// =============================================================================

#[test]
fn test_ts2741_source_type_includes_property_types() {
    // TS2741 "Property 'y' is missing in type '{ x: string; }' but required in type ..."
    // should show property TYPES (not just names like `{ x }`).
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Comp(props: {{ x: string; y: number }}) {{
    return <div />;
}}
let p = <Comp x="hello" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2741_msgs: Vec<&str> = diags
        .iter()
        .filter(|(c, _)| *c == diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE)
        .map(|(_, m)| m.as_str())
        .collect();
    assert!(
        !ts2741_msgs.is_empty(),
        "Expected TS2741 for missing 'y', got: {diags:?}"
    );
    // The source type should include property types, e.g., `{ x: string; }`
    // not just `{ x }`
    let has_typed_format = ts2741_msgs
        .iter()
        .any(|m| m.contains("x: string") || m.contains("x: \"hello\""));
    assert!(
        has_typed_format,
        "TS2741 source type should include property types. Got: {ts2741_msgs:?}"
    );
}

// =============================================================================
// Generic SFC spread IntrinsicAttributes checking
// =============================================================================

/// JSX namespace preamble with optional `IntrinsicAttributes` (standard React pattern).
const JSX_PREAMBLE_WITH_IA: &str = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
        span: any;
    }
    interface IntrinsicAttributes {
        key?: string | number;
    }
    interface ElementAttributesProperty { props: {} }
    interface ElementChildrenAttribute { children: {} }
}
"#;

#[test]
fn jsx_body_children_excess_property_checks_use_intrinsic_attributes() {
    let source = format!(
        r#"
{JSX_PREAMBLE_WITH_IA}
const Tag = (x: {{}}) => <div></div>;
const k3 = <Tag children={{<div></div>}} />;
const k4 = <Tag key="1"><div></div></Tag>;
const k5 = <Tag key="1"><div></div><div></div></Tag>;
"#
    );
    let diags = jsx_diagnostics_with_pos(&source);
    let ts2322: Vec<_> = diags
        .iter()
        .filter(|(code, _, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert_eq!(
        ts2322.len(),
        3,
        "Expected explicit and body-children excess-property TS2322s, got: {diags:?}"
    );
    assert!(
        ts2322.iter().any(|(_, _, message)| message.contains(
            "Type '{ children: Element; key: string; }' is not assignable to type 'IntrinsicAttributes'."
        )),
        "Expected body children with key to synthesize '{{ children: Element; key: string; }}', got: {diags:?}"
    );
    assert!(
        ts2322.iter().any(|(_, _, message)| message.contains(
            "Type '{ children: Element[]; key: string; }' is not assignable to type 'IntrinsicAttributes'."
        )),
        "Expected multi-body children with key to synthesize '{{ children: Element[]; key: string; }}', got: {diags:?}"
    );
}

#[test]
fn test_generic_sfc_spread_unconstrained_emits_ts2322() {
    // <Component {...props} /> where Component<T>(props: T) and props: U (unconstrained)
    // should emit TS2322: "Type 'U' is not assignable to type 'IntrinsicAttributes & U'"
    // because unconstrained U's constraint (unknown) is not assignable to IntrinsicAttributes.
    let source = format!(
        r#"
{JSX_PREAMBLE_WITH_IA}
declare function Component<T>(props: T): JSX.Element;
const decorator = function <U>(props: U) {{
    return <Component {{...props}} />;
}}
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for unconstrained U not assignable to IntrinsicAttributes & U, got: {diags:?}"
    );
    // Verify the error message mentions IntrinsicAttributes
    let ts2322_msgs: Vec<_> = diags
        .iter()
        .filter(|(c, _)| *c == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, m)| m.as_str())
        .collect();
    assert!(
        ts2322_msgs
            .iter()
            .any(|m| m.contains("IntrinsicAttributes")),
        "TS2322 message should mention IntrinsicAttributes. Got: {ts2322_msgs:?}"
    );
}

#[test]
fn test_generic_sfc_spread_constrained_no_error() {
    // <Component {...props} /> where props: U extends {x: string}
    // should NOT emit TS2322 because U's constraint ({x: string}) IS assignable
    // to IntrinsicAttributes (which has all-optional properties).
    let source = format!(
        r#"
{JSX_PREAMBLE_WITH_IA}
declare function Component<T>(props: T): JSX.Element;
const decorator = function <U extends {{x: string}}>(props: U) {{
    return <Component {{...props}} />;
}}
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
        "Should NOT emit TS2322 for constrained U that satisfies IntrinsicAttributes, got: {ts2322_about_ia:?}"
    );
}

