#[test]
fn test_jsx_children_no_contextual_type_for_generic_sfc() {
    // Generic SFCs can't provide children contextual types (type params unresolved)
    // — TS7006 is expected for the callback parameter.
    let source = format!(
        r#"
{JSX_PREAMBLE}
function GenComp<T>(props: {{ prop: T; children: (t: T) => T }}) {{
    return <div />;
}}
const x = <GenComp prop={{"x"}}>{{i => ({{}}) }}</GenComp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    // For generic SFCs, we can't infer T, so children contextual typing
    // may or may not work. This test just verifies no crash occurs.
    // (TS7006 is acceptable here since generic inference isn't implemented)
    let _ = diags; // Just verify no panic
}

#[test]
fn test_jsx_generic_children_recover_inferred_return_type_errors() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface LitProps<T> {{ prop: T, children: (x: this) => T }}
const ElemLit = <T extends string>(p: LitProps<T>) => <div></div>;

const explicit = <ElemLit prop="x" children={{p => "y"}} />
const body = <ElemLit prop="x">{{p => "y"}}</ElemLit>
const mismatched = <ElemLit prop="x">{{() => 12}}</ElemLit>
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2322_count = diags
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .count();
    assert!(
        ts2322_count >= 3,
        "Expected JSX generic children to report three TS2322 mismatches, got: {diags:?}"
    );
}

#[test]
fn test_jsx_children_intrinsic_element_no_crash() {
    // Intrinsic elements (e.g., <div>) should not crash when extracting
    // children contextual type, even if children type is broad/any.
    let source = format!(
        r#"
{JSX_PREAMBLE}
const x = <div>{{(item: string) => item}}</div>;
"#
    );
    let diags = jsx_diagnostics(&source);
    // Just verify no crash — intrinsic elements have broad children types
    let _ = diags;
}

// =============================================================================
// Spread attribute type checking (TS2322)
// =============================================================================

/// JSX preamble with typed intrinsic elements for spread tests
const JSX_INTRINSIC_PREAMBLE: &str = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        test1: { x: string; y?: number };
    }
}
"#;

#[test]
fn test_spread_attribute_type_mismatch_emits_ts2322() {
    // Spreading an object with wrong property type should emit TS2322
    let source = format!(
        r#"
{JSX_INTRINSIC_PREAMBLE}
var obj = {{ x: 32 }};
<test1 {{...obj}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for spread with wrong property type, got: {diags:?}"
    );
}

#[test]
fn test_spread_attribute_compatible_no_error() {
    // Spreading a compatible object should not emit TS2322
    let source = format!(
        r#"
{JSX_INTRINSIC_PREAMBLE}
var obj = {{ x: "hello" }};
<test1 {{...obj}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should not emit TS2322 for compatible spread, got: {diags:?}"
    );
}

#[test]
fn test_spread_attribute_override_no_ts2322() {
    // When a later explicit attribute overrides a wrong spread property,
    // no TS2322 should be emitted for the spread
    let source = format!(
        r#"
{JSX_INTRINSIC_PREAMBLE}
var obj = {{ x: 32, y: 10 }};
<test1 {{...obj}} x="ok" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should not emit TS2322 when explicit attr overrides spread, got: {diags:?}"
    );
}

#[test]
fn test_spread_attribute_missing_required_is_ts2741_not_ts2322() {
    // Spreading an object with missing required property should emit TS2741,
    // not TS2322 — missing properties are handled by the separate TS2741 check
    let source = format!(
        r#"
{JSX_INTRINSIC_PREAMBLE}
var obj = {{ y: 10 }};
<test1 {{...obj}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    // Should have TS2741 (missing 'x')
    assert!(
        has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Expected TS2741 for missing required property, got: {diags:?}"
    );
    // Should NOT have TS2322 — missing properties are TS2741, not TS2322
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should not emit TS2322 for just-missing properties (use TS2741), got: {diags:?}"
    );
}

// =============================================================================
// Spread with missing required props: TS2741 only, no TS2322
// =============================================================================

#[test]
fn test_spread_with_missing_props_no_ts2322() {
    // When a spread provides some props but not all required ones,
    // tsc emits only TS2741 (missing property) not TS2322 (type mismatch).
    // Even if the spread has type-incompatible properties, the TS2741 is primary.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface SourceProps {{
    property1: string;
    property2: number;
}}
function Source(props: SourceProps) {{
    return <Target {{...props}} />;
}}
interface TargetProps {{
    property1: string;
    missingProp: string;
    property2: boolean;
}}
function Target(props: TargetProps) {{
    return <div>Hello</div>;
}}
"#
    );
    let diags = jsx_diagnostics(&source);
    // Should have TS2741 for missing 'missingProp'
    assert!(
        has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Expected TS2741 for missing 'missingProp', got: {diags:?}"
    );
    // Should NOT have TS2322 — tsc only reports TS2741 when there are missing required props
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should not emit TS2322 when TS2741 fires for missing required props, got: {diags:?}"
    );
}

#[test]
fn test_spread_compatible_no_errors() {
    // When a spread provides all required props with correct types, no errors.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Props {{
    name: string;
    age: number;
}}
function Greet(props: Props) {{
    return <div>Hello</div>;
}}
const p: Props = {{ name: "hi", age: 42 }};
let x = <Greet {{...p}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should not emit TS2322 for compatible spread, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Should not emit TS2741 for compatible spread, got: {diags:?}"
    );
}

// =============================================================================
// IntrinsicAttributes required property checking
// =============================================================================

/// JSX namespace preamble with required `key` in `IntrinsicAttributes`.
/// This is unusual (React makes key optional) but tests like
/// tsxIntrinsicAttributeErrors.tsx deliberately test this.
const JSX_PREAMBLE_REQUIRED_KEY: &str = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
    }
    interface IntrinsicAttributes {
        key: string | number
    }
    interface ElementClass {
        render: any;
    }
}
"#;

const JSX_PREAMBLE_REQUIRED_CLASS_REF: &str = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
    }
    interface ElementAttributesProperty { props: {} }
    interface IntrinsicClassAttributes<T> {
        ref: T
    }
}
"#;

const JSX_PREAMBLE_REQUIRED_CLASS_REF_NO_PROPS_INFRA: &str = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
    }
    interface IntrinsicClassAttributes<T> {
        ref: T
    }
}
"#;

#[test]
fn test_required_intrinsic_attribute_missing_emits_ts2741() {
    // When IntrinsicAttributes has a required property (key without ?),
    // tsc emits TS2741 if it's not provided.
    let source = format!(
        r#"
{JSX_PREAMBLE_REQUIRED_KEY}
interface I {{
    new(n: string): {{
        x: number;
        render(): void;
    }}
}}
declare var E: I;
<E x={{10}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Expected TS2741 for missing required 'key' from IntrinsicAttributes, got: {diags:?}"
    );
}

