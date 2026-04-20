#[test]
fn test_intrinsic_interface_ref_type_mismatch() {
    // Interface-referenced props (Attribs1) should be resolved from Lazy(DefId)
    // so that type mismatches are detected.
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        test1: Attribs1;
    }
}
interface Attribs1 {
    x?: number;
    s?: string;
}
let a = <test1 x={'not a number'} />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for string not assignable to number on interface-ref props, got: {diags:?}"
    );
}

#[test]
fn test_intrinsic_interface_ref_excess_property() {
    // Excess properties on interface-referenced props should be detected.
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        test1: Attribs1;
    }
}
interface Attribs1 {
    x?: number;
}
let a = <test1 y={0} />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for excess property 'y' on interface-ref props, got: {diags:?}"
    );
}

#[test]
fn test_intrinsic_interface_ref_correct_props() {
    // Correct props on interface-referenced types should not produce errors.
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        test1: Attribs1;
    }
}
interface Attribs1 {
    x?: number;
    s?: string;
}
let a = <test1 x={42} />;
let b = <test1 />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should not emit TS2322 for correct interface-ref props, got: {diags:?}"
    );
}

#[test]
fn test_intrinsic_inline_type_still_works() {
    // Inline object types (not interface references) should continue to work.
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        test2: { reqd: string };
    }
}
let a = <test2 reqd={42} />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for number not assignable to string on inline props, got: {diags:?}"
    );
}

#[test]
fn test_intrinsic_interface_ref_missing_required() {
    // Missing required props on interface-referenced types should be detected.
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        test2: { n: boolean };
    }
}
let a = <test2 />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Expected TS2741 for missing required 'n' on inline props, got: {diags:?}"
    );
}

// =============================================================================
// Hyphenated attribute handling
// =============================================================================

#[test]
fn test_hyphenated_attrs_bypass_type_checking() {
    // TSC treats hyphenated attributes (data-*, aria-*) as untyped
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Comp(props: {{ name: string }}) {{ return <div />; }}
let x = <Comp name="hi" data-testid="foo" aria-label="bar" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should not type-check hyphenated attributes, got: {diags:?}"
    );
}

#[test]
fn test_declared_hyphenated_attr_uses_synthesized_assignability_error() {
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        test1: { "data-foo"?: string };
    }
}

<test1 data-foo={32} />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        diags.iter().any(|(code, message)| {
            *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && message.contains("data-foo")
                && message.contains("number")
                && message.contains("not assignable")
        }),
        "Declared hyphenated attrs should use synthesized JSX-attrs assignability, got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|(code, message)| {
            *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && message.contains("Type 'number' is not assignable to type 'string'")
        }),
        "Declared hyphenated attrs should not use the per-attribute TS2322 path, got: {diags:?}"
    );
}

// =============================================================================
// TS2604: JSX element type without call/construct signatures
// =============================================================================

#[test]
fn test_ts2604_emitted_for_non_callable_element() {
    // A non-callable value used as JSX tag should emit TS2604
    let source = format!(
        r#"
{JSX_PREAMBLE}
var Div = 3;
<Div />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::JSX_ELEMENT_TYPE_DOES_NOT_HAVE_ANY_CONSTRUCT_OR_CALL_SIGNATURES
        ),
        "Should emit TS2604 for non-callable JSX element, got: {diags:?}"
    );
}

#[test]
fn test_ts2604_not_emitted_for_callable_element() {
    // A callable value used as JSX tag should NOT get TS2604
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Comp() {{ return <div />; }}
<Comp />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::JSX_ELEMENT_TYPE_DOES_NOT_HAVE_ANY_CONSTRUCT_OR_CALL_SIGNATURES
        ),
        "Should NOT emit TS2604 for callable JSX element, got: {diags:?}"
    );
}

#[test]
fn test_ts2604_not_emitted_for_empty_interface_with_no_intrinsics() {
    // When no JSX.IntrinsicElements exists, string-typed tags shouldn't get TS2604
    let source = r#"
declare namespace JSX {
    interface Element {}
}
var CustomTag = "h1";
<CustomTag />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::JSX_ELEMENT_TYPE_DOES_NOT_HAVE_ANY_CONSTRUCT_OR_CALL_SIGNATURES
        ),
        "Should NOT emit TS2604 for string-typed JSX tag, got: {diags:?}"
    );
}

