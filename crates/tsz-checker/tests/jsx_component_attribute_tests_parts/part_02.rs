#[test]
fn test_string_literal_component_tag_uses_intrinsic_lookup() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
    }
}
var CustomTag: "h1" = "h1";
<CustomTag />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(&diags, diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Expected TS2339 for missing JSX.IntrinsicElements['h1'], got: {diags:?}"
    );
    assert!(
        has_code(
            &diags,
            diagnostic_codes::JSX_ELEMENT_TYPE_DOES_NOT_HAVE_ANY_CONSTRUCT_OR_CALL_SIGNATURES
        ),
        "Expected TS2604 after intrinsic lookup fails for literal string tag, got: {diags:?}"
    );
}

#[test]
fn test_string_literal_component_tag_succeeds_when_intrinsic_exists() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
        h1: any;
    }
}
var CustomTag: "h1" = "h1";
<CustomTag />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(&diags, diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Should not emit TS2339 when the literal string tag exists in IntrinsicElements, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::JSX_ELEMENT_TYPE_DOES_NOT_HAVE_ANY_CONSTRUCT_OR_CALL_SIGNATURES
        ),
        "Should not emit TS2604 when the literal string tag resolves as an intrinsic element, got: {diags:?}"
    );
}

#[test]
fn test_property_access_string_literal_tag_keeps_dynamic_component_behavior() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
    }
}
const tags: { header: "h1" } = { header: "h1" };
<tags.header />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(&diags, diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Property access tags should not be forced through intrinsic lookup, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::JSX_ELEMENT_TYPE_DOES_NOT_HAVE_ANY_CONSTRUCT_OR_CALL_SIGNATURES
        ),
        "Property access literal tags should keep dynamic-tag behavior, got: {diags:?}"
    );
}

#[test]
fn test_missing_intrinsic_name_reports_opening_and_closing_tag_errors() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
    }
}
<customTag> Hello World </customTag>;
"#;
    let diags = jsx_diagnostics_with_pos(source);
    let ts2339_count = diags
        .iter()
        .filter(|(code, _, message)| {
            *code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE
                && message.contains(
                    "Property 'customTag' does not exist on type 'JSX.IntrinsicElements'.",
                )
        })
        .count();
    assert_eq!(
        ts2339_count, 2,
        "Expected TS2339 on both opening and closing tags for missing intrinsic name, got: {diags:?}"
    );
}

#[test]
fn test_intrinsic_template_literal_index_signature_checks_attributes() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        [k: `foo${string}`]: { prop: string };
    }
}
<foobaz prop={10} />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 when intrinsic template-literal index signature requires string props, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Template-literal intrinsic match should not fall through to TS2339, got: {diags:?}"
    );
}

#[test]
fn test_intrinsic_template_literal_index_signature_prefers_more_specific_match() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        [k: `foo${string}`]: { prop: string };
        [k: `foobar${string}`]: { prop: 'literal' };
    }
}
<foobarbaz prop="smth" />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 from the more specific template-literal intrinsic match, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "More specific template-literal intrinsic match should not fall through to TS2339, got: {diags:?}"
    );
}

#[test]
fn test_intrinsic_template_literal_index_signature_accepts_valid_values() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        [k: `foo${string}`]: { prop: string };
        [k: `foobar${string}`]: { prop: 'literal' };
    }
}
<foobaz prop="smth" />;
<foobarbaz prop="literal" />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Valid intrinsic template-literal props should not emit TS2322, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Valid intrinsic template-literal props should not emit TS2339, got: {diags:?}"
    );
}

// =============================================================================
// Class component attribute checking (DEBUG)
// =============================================================================

#[test]
fn test_class_component_direct_constructor_emits_ts2322() {
    // Class component with direct constructor taking P — type params should be instantiated
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare class Component<P> {{
    props: P;
    constructor(props: P);
    render(): JSX.Element;
}}
interface Prop {{
    x: false;
}}
class Poisoned extends Component<Prop> {{
    render() {{
        return <div>Hello</div>;
    }}
}}
let p = <Poisoned x />;
"#
    );
    let diags = jsx_diagnostics(&source);
    // Debug: eprintln!("ALL DIAGNOSTICS: {:?}", diags);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for boolean not assignable to false, got: {diags:?}"
    );
}

#[test]
fn test_class_component_optional_constructor_emits_ts2322() {
    // React-style: constructor(props?: P, context?: any) — should still check props
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare class Component<P, S> {{
    props: P & {{ children?: any }};
    state: S;
    constructor(props?: P, context?: any);
    render(): JSX.Element | null;
}}
interface Prop {{
    x: false;
}}
class Poisoned extends Component<Prop, {{}}> {{
    render() {{
        return <div>Hello</div>;
    }}
}}
let p = <Poisoned x />;
"#
    );
    let diags = jsx_diagnostics(&source);
    // Debug: eprintln!("ALL DIAGNOSTICS: {:?}", diags);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for boolean not assignable to false (React-style class), got: {diags:?}"
    );
}

#[test]
fn test_class_component_missing_required_prop_emits_ts2322_not_ts2741() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare class Component<P, S> {{
    props: P;
    state: S;
    constructor(props?: P, context?: any);
    render(): JSX.Element;
}}
class NeedsProp extends Component<{{ reqd: string }}, {{}}> {{
    render() {{
        return <div>Hello</div>;
    }}
}}
let p = <NeedsProp />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Missing required class-component props should emit TS2322, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Missing required class-component props should not fall back to TS2741, got: {diags:?}"
    );
}

