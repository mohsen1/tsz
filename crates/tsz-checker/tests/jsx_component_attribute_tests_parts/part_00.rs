#[test]
fn test_sfc_excess_property_emits_ts2322() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Greet(props: {{ name: string }}) {{
    return <div>Hello</div>;
}}
let x = <Greet name="world" unknownProp="oops" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for excess property 'unknownProp', got: {diags:?}"
    );
}

#[test]
fn jsx_generic_class_component_uses_constraint_for_props_checking() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare class Component<P> {{
    constructor(props: P);
    props: P;
    render(): JSX.Element;
}}

interface Prop {{
    a: number;
    b: string;
}}

declare class MyComp<P extends Prop> extends Component<P> {{
    internalProp: P;
}}

let x1 = <MyComp />;
let x2 = <MyComp a="hi" />;
"#
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for generic class prop mismatch, got: {diags:?}"
    );
    assert!(
        has_code(
            &diags,
            diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE
        ),
        "Expected TS2739 for missing constrained props on generic class JSX element, got: {diags:?}"
    );
}

#[test]
fn test_sfc_type_mismatch_emits_ts2322() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Greet(props: {{ name: string }}) {{
    return <div>Hello</div>;
}}
let x = <Greet name={{42}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for number not assignable to string, got: {diags:?}"
    );
}

#[test]
fn test_sfc_missing_required_prop_emits_ts2741() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Greet(props: {{ name: string }}) {{
    return <div>Hello</div>;
}}
let x = <Greet />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Expected TS2741 for missing required 'name', got: {diags:?}"
    );
}

#[test]
fn test_sfc_correct_props_no_errors() {
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
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should not emit TS2322 for correct props, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Should not emit TS2741 for correct props, got: {diags:?}"
    );
}

#[test]
fn test_sfc_optional_props_no_errors() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Greet(props: {{ name?: string }}) {{
    return <div>Hello</div>;
}}
let x = <Greet />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Should not emit TS2741 for optional props, got: {diags:?}"
    );
}

// =============================================================================
// Guards: generic, overloaded, union, parse errors
// =============================================================================

#[test]
fn test_generic_sfc_skips_checking() {
    // G3 equivalent for SFCs: generic functions are skipped
    let source = format!(
        r#"
{JSX_PREAMBLE}
function GenericComp<T>(props: T) {{
    return <div>Hello</div>;
}}
let x = <GenericComp unknownProp="anything" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    // Should NOT produce TS2322 because we skip generic SFCs
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should skip checking for generic SFCs, got: {diags:?}"
    );
}

#[test]
fn test_union_props_skips_checking() {
    // G5: union-typed props are skipped
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface PA {{ kind: "a"; x: number }}
interface PB {{ kind: "b"; y: string }}
function UnionComp(props: PA | PB) {{
    return <div>Hello</div>;
}}
let x = <UnionComp kind="a" x={{42}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    // Should NOT produce TS2322 because we skip union props
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should skip checking for union props, got: {diags:?}"
    );
}

#[test]
fn test_spread_does_not_produce_false_positives() {
    // Spread attributes should not produce false TS2741
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Props {{ a: string; b: number }}
function Comp(props: Props) {{ return <div />; }}
declare var partial: {{ a: string }};
let x = <Comp {{...partial}} b={{42}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    // Should NOT produce TS2741 — spread + explicit attrs may cover all required props
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Should not emit TS2741 when spread is present, got: {diags:?}"
    );
}

#[test]
fn test_string_index_signature_no_excess_errors() {
    // Props with string index signature should not report excess properties
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Props {{ name: string; [key: string]: any }}
function Comp(props: Props) {{ return <div />; }}
let x = <Comp name="hi" anyOtherProp="fine" />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should not emit TS2322 with string index signature, got: {diags:?}"
    );
}

// =============================================================================
// Intrinsic element attribute checking with interface-referenced props
// =============================================================================
//
// When JSX.IntrinsicElements maps a tag to an *interface reference* (e.g.,
// `test1: Attribs1`), the props type arrives as Lazy(DefId). The checker must
// resolve it before attribute checking; otherwise, the solver's
// PropertyAccessEvaluator returns TypeId::ANY (QueryCache.resolve_lazy → None),
// silently suppressing all type errors.

