#[test]
fn test_optional_intrinsic_attribute_no_error() {
    // Standard React pattern: IntrinsicAttributes has optional key.
    // No error when key is not provided.
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Greet(props: {{ name: string }}) {{
    return <div>Hello</div>;
}}
let x = <Greet name="world" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    // JSX_PREAMBLE doesn't define IntrinsicAttributes with required key,
    // so no TS2741 for missing key
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Should not emit TS2741 when IntrinsicAttributes has no required props, got: {diags:?}"
    );
}

#[test]
fn test_required_intrinsic_class_attribute_missing_emits_ts2741() {
    let source = format!(
        r#"
{JSX_PREAMBLE_REQUIRED_CLASS_REF}
class App {{
    props = {{}};
    render() {{
        return <div />;
    }}
}}
let x = <App />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Expected TS2741 for missing required 'ref' from IntrinsicClassAttributes<T>, got: {diags:?}"
    );
}

#[test]
fn test_required_intrinsic_class_attribute_missing_without_props_infra_emits_ts2741() {
    let source = format!(
        r#"
{JSX_PREAMBLE_REQUIRED_CLASS_REF_NO_PROPS_INFRA}
class App {{}}
let x = <App />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        diags.iter().any(|(code, msg)| {
            *code == diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
                && msg.contains("ref")
        }),
        "Expected TS2741 for missing required 'ref' even without ElementAttributesProperty, got: {diags:?}"
    );
}

#[test]
fn test_required_intrinsic_class_attribute_satisfied_for_class_component() {
    let source = format!(
        r#"
{JSX_PREAMBLE_REQUIRED_CLASS_REF}
class App {{
    props = {{}};
    render() {{
        return <div />;
    }}
}}
const app = new App();
let x = <App ref={{app}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Should not emit TS2741 when required IntrinsicClassAttributes<T> are provided, got: {diags:?}"
    );
}

#[test]
fn test_required_intrinsic_class_attribute_alias_missing_emits_ts2741() {
    let source = r#"
class App {}
export const a = <App></App>;
"#;
    let react_types = r#"
interface IntrinsicClassAttributesAlias<T> {
    ref: T
}
declare namespace JSX {
    interface Element {}
    type IntrinsicClassAttributes<T> = IntrinsicClassAttributesAlias<T>
}
"#;

    let diags = cross_file_jsx_diagnostics_with_mode(react_types, source, JsxMode::ReactJsx);
    let relevant_diags: Vec<_> = diags
        .into_iter()
        .filter(|(code, _)| *code != diagnostic_codes::CANNOT_FIND_GLOBAL_TYPE)
        .collect();

    assert!(
        relevant_diags.iter().any(|(code, msg)| {
            *code == diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
                && msg.contains("Property 'ref' is missing")
                && (msg.contains("IntrinsicClassAttributesAlias")
                    || msg.contains("IntrinsicClassAttributes"))
        }),
        "Expected TS2741 for missing required 'ref' from alias-based IntrinsicClassAttributes<T>, got: {relevant_diags:?}"
    );
}

#[test]
fn test_jsx_sfc_with_too_many_required_parameters_emits_ts6229() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
function MyComp4(props: {{ x: number }}, context: any, bad: any, verybad: any) {{
    return <div></div>;
}}
function MyComp3(props: {{ x: number }}, context: any, bad: any) {{
    return <div></div>;
}}
function MyComp2(props: {{ x: number }}, context: any) {{
    return <div></div>;
}}
declare function MyTagWithOptionalNonJSXBits(
    props: {{ x: number }},
    context: any,
    nonReactArg?: string
): JSX.Element;
const a = <MyComp4 x={{2}} />;
const b = <MyComp3 x={{2}} />;
const c = <MyComp2 x={{2}} />;
const d = <MyTagWithOptionalNonJSXBits x={{2}} />;
"#
    );

    let diags = jsx_diagnostics_with_mode(&source, JsxMode::React);
    let ts6229: Vec<&String> = diags
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TAG_EXPECTS_AT_LEAST_ARGUMENTS_BUT_THE_JSX_FACTORY_PROVIDES_AT_MOST)
        .map(|(_, msg)| msg)
        .collect();

    assert_eq!(
        ts6229.len(),
        2,
        "Expected TS6229 only for JSX tags requiring more than props+context, got: {diags:?}"
    );
    assert!(
        ts6229
            .iter()
            .any(|msg| msg.contains("MyComp4") && msg.contains("'4'")),
        "Expected TS6229 for MyComp4, got: {ts6229:?}"
    );
    assert!(
        ts6229
            .iter()
            .any(|msg| msg.contains("MyComp3") && msg.contains("'3'")),
        "Expected TS6229 for MyComp3, got: {ts6229:?}"
    );
}

#[test]
fn test_required_intrinsic_class_attribute_not_required_for_sfc() {
    let source = format!(
        r#"
{JSX_PREAMBLE_REQUIRED_CLASS_REF}
function App(props: {{ label: string }}) {{
    return <div />;
}}
let x = <App label="ok" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !diags.iter().any(|(code, msg)| {
            *code == diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
                && msg.contains("ref")
        }),
        "Should not emit missing required 'ref' for function components, got: {diags:?}"
    );
}

// =============================================================================
// Union-typed props checking (discriminated unions)
// =============================================================================

#[test]
fn test_union_props_conflicting_discriminant_emits_ts2322() {
    // When JSX attributes conflict with ALL union members, emit TS2322.
    // <TextComponent editable={true} /> without onEdit conflicts with both members:
    // - { editable: false } requires editable=false
    // - { editable: true, onEdit: ... } requires onEdit
    // But per-attribute type checking only checks VALUE compatibility, not missing props.
    // Since editable=true is NOT assignable to editable: false (first member),
    // and no member has a compatible editable value, TS2322 fires.
    let source = format!(
        r#"
{JSX_PREAMBLE}
type TextProps = {{ editable: false }}
               | {{ editable: true; onEdit: (text: string) => void }};
declare function TextComponent(props: TextProps): JSX.Element;
let x = <TextComponent editable={{true}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for discriminated union props mismatch, got: {diags:?}"
    );
}

#[test]
fn test_union_props_matching_discriminant_no_error() {
    // When attributes match at least one union member, no TS2322.
    // <UnionComp kind="a" x={42} /> matches PA { kind: "a"; x: number }
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface PA {{ kind: "a"; x: number }}
interface PB {{ kind: "b"; y: string }}
declare function UnionComp(props: PA | PB): JSX.Element;
let x = <UnionComp kind="a" x={{42}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should NOT emit TS2322 when attributes match union member, got: {diags:?}"
    );
}

#[test]
fn test_union_props_callback_attribute_skips_check() {
    // When attributes include callback expressions, skip the union check
    // to avoid false positives from missing contextual typing.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface PS {{ multi: false; onChange: (s: string) => void }}
interface PM {{ multi: true; onChange: (s: string[]) => void }}
declare function Comp(props: PS | PM): JSX.Element;
let x = <Comp multi={{false}} onChange={{val => {{}}}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should skip union check when callback attributes present, got: {diags:?}"
    );
}

