//! Tests for JSX component attribute type checking.
//!
//! Verifies that TS2322 (type mismatch) and TS2741 (missing required property)
//! are correctly emitted for JSX component attributes.

use tsz_checker::CheckerState;
use tsz_common::checker_options::{CheckerOptions, JsxMode};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Compile JSX source with inline JSX namespace and return diagnostics.
fn jsx_diagnostics(source: &str) -> Vec<(u32, String)> {
    let file_name = "test.tsx";
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let options = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        ..CheckerOptions::default()
    };

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn has_code(diags: &[(u32, String)], code: u32) -> bool {
    diags.iter().any(|(c, _)| *c == code)
}

/// Inline JSX namespace preamble for tests (with `ElementAttributesProperty` { props: {} }).
/// This mimics react16.d.ts's structure where props are accessed via instance.props.
const JSX_PREAMBLE: &str = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
        span: any;
    }
    interface ElementAttributesProperty { props: {} }
    interface ElementChildrenAttribute { children: {} }
}
"#;

// =============================================================================
// SFC attribute type checking
// =============================================================================

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
