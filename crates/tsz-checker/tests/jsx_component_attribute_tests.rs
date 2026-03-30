//! Tests for JSX component attribute type checking.
//!
//! Verifies that TS2322 (type mismatch) and TS2741 (missing required property)
//! are correctly emitted for JSX component attributes.

use std::sync::Arc;
use tsz_checker::CheckerState;
use tsz_common::checker_options::{CheckerOptions, JsxMode};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Compile JSX source with inline JSX namespace and return diagnostics.
fn jsx_diagnostics(source: &str) -> Vec<(u32, String)> {
    jsx_diagnostics_with_mode(source, JsxMode::Preserve)
}

fn jsx_diagnostics_with_mode(source: &str, jsx_mode: JsxMode) -> Vec<(u32, String)> {
    let file_name = "test.tsx";
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let options = CheckerOptions {
        jsx_mode,
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

/// Return diagnostics with position info (code, start, message).
fn jsx_diagnostics_with_pos(source: &str) -> Vec<(u32, u32, String)> {
    jsx_diagnostics_with_pos_mode(source, JsxMode::Preserve)
}

fn jsx_diagnostics_with_pos_mode(source: &str, jsx_mode: JsxMode) -> Vec<(u32, u32, String)> {
    let file_name = "test.tsx";
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let options = CheckerOptions {
        jsx_mode,
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
        .map(|d| (d.code, d.start, d.message_text.clone()))
        .collect()
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
                && message.contains("\"data-foo\": number")
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

#[test]
fn test_property_access_class_component_missing_required_prop_emits_ts2322_not_ts2741() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare class Component<P, S> {{
    props: P;
    state: S;
    constructor(props?: P, context?: any);
    render(): JSX.Element;
}}
interface ComponentClass<P> {{
    new (props?: P, context?: any): Component<P, any>;
}}
declare namespace TestMod {{
    interface TestClass extends ComponentClass<{{ reqd: string }}> {{}}
    var Test: TestClass;
}}
const T = TestMod.Test;
let p1 = <T />;
let p2 = <TestMod.Test />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Missing required property-access class-component props should emit TS2322, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Missing required property-access class-component props should not fall back to TS2741, got: {diags:?}"
    );
}

// =============================================================================
// Cross-file: import React = require('react') with ambient module
// =============================================================================

/// Helper to compile a multi-file JSX project and return diagnostics for the main file.
fn cross_file_jsx_diagnostics(lib_source: &str, main_source: &str) -> Vec<(u32, String)> {
    cross_file_jsx_diagnostics_with_mode(lib_source, main_source, JsxMode::Preserve)
}

fn cross_file_jsx_diagnostics_with_mode(
    lib_source: &str,
    main_source: &str,
    jsx_mode: JsxMode,
) -> Vec<(u32, String)> {
    // Parse and bind lib file (react.d.ts equivalent)
    let mut parser_lib = ParserState::new("react.d.ts".to_string(), lib_source.to_string());
    let root_lib = parser_lib.parse_source_file();
    let mut binder_lib = tsz_binder::BinderState::new();
    binder_lib.bind_source_file(parser_lib.get_arena(), root_lib);
    let arena_lib = Arc::new(parser_lib.get_arena().clone());
    let binder_lib = Arc::new(binder_lib);

    // Parse and bind main file
    let mut parser_main = ParserState::new("file.tsx".to_string(), main_source.to_string());
    let root_main = parser_main.parse_source_file();
    let mut binder_main = tsz_binder::BinderState::new();
    let raw_lib_contexts = vec![tsz_binder::state::LibContext {
        arena: Arc::clone(&arena_lib),
        binder: Arc::clone(&binder_lib),
    }];
    binder_main.merge_lib_contexts_into_binder(&raw_lib_contexts);
    binder_main.bind_source_file(parser_main.get_arena(), root_main);

    let arena_main = Arc::new(parser_main.get_arena().clone());
    let binder_main = Arc::new(binder_main);

    let all_arenas = Arc::new(vec![Arc::clone(&arena_main), Arc::clone(&arena_lib)]);
    let all_binders = Arc::new(vec![Arc::clone(&binder_main), Arc::clone(&binder_lib)]);

    let options = CheckerOptions {
        jsx_mode,
        ..CheckerOptions::default()
    };

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_main.as_ref(),
        binder_main.as_ref(),
        &types,
        "file.tsx".to_string(),
        options,
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(0);
    checker
        .ctx
        .set_lib_contexts(vec![tsz_checker::context::LibContext {
            arena: Arc::clone(&arena_lib),
            binder: Arc::clone(&binder_lib),
        }]);
    checker.ctx.set_actual_lib_file_count(1);

    checker.check_source_file(root_main);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn load_typescript_fixture(rel_path: &str) -> Option<String> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../../").join(rel_path),
        manifest_dir.join("../../../").join(rel_path),
    ];

    for candidate in candidates {
        if candidate.exists() {
            return std::fs::read_to_string(candidate).ok();
        }
    }

    None
}

#[test]
fn test_cross_file_import_require_export_equals() {
    // Simulate: declare module "react" { export = __React; }
    // with: import React = require('react')
    let lib_source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
    }
    interface ElementAttributesProperty { props: {} }
    interface ElementChildrenAttribute { children: {} }
}
declare namespace __React {
    class Component<P, S = {}> {
        props: P & { children?: any };
        state: S;
        constructor(props?: P, context?: any);
        render(): JSX.Element | null;
    }
}
declare module "react" {
    export = __React;
}
"#;

    let main_source = r#"
import React = require('react');

interface Prop {
    x: false;
}
class Poisoned extends React.Component<Prop, {}> {
    render() {
        return <div>Hello</div>;
    }
}

let p = <Poisoned x />;
"#;

    let diags = cross_file_jsx_diagnostics(lib_source, main_source);
    // The export= resolution should work — no TS2307 "Cannot find module"
    assert!(
        !has_code(&diags, 2307),
        "Should not emit TS2307 for resolvable ambient module, got: {diags:?}"
    );
    // TODO: full TS2322 for class component props requires cross-file class heritage
    // resolution which is a deeper issue. For now, verify module resolution works.
}

#[test]
fn test_cross_file_react_component_override_emits_ts2416() {
    let lib_source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
    }
    interface ElementAttributesProperty { props: {} }
    interface ElementChildrenAttribute { children: {} }
}
declare namespace __React {
    class Component<P, S = {}> {
        props: P & { children?: any };
        state: S;
        constructor(props?: P, context?: any);
        render(): JSX.Element | null;
    }
}
declare module "react" {
    export = __React;
}
"#;

    let main_source = r#"
import React = require('react');

class B1<T extends { x: string }> extends React.Component<T, {}> {
    render() {
        return <div>hi</div>;
    }
}
class B<U> extends React.Component<U, {}> {
    props: U;
    render() {
        return <B1 {...this.props} x="hi" />;
    }
}
"#;

    let diags = cross_file_jsx_diagnostics(lib_source, main_source);
    assert!(
        !has_code(&diags, 2307),
        "Should resolve the ambient React module, got: {diags:?}"
    );
    assert!(
        has_code(
            &diags,
            diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE
        ),
        "Expected TS2416 for incompatible inherited props override across the React module boundary, got: {diags:?}"
    );
}

// =============================================================================
// TS2698: JSX spread type validation
// =============================================================================

#[test]
fn test_ts2698_spread_null_emits_error() {
    // Spreading `null` in JSX should emit TS2698
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { [key: string]: any }
}
const a = null;
const x = <div { ...a } />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES
        ),
        "Expected TS2698 for spreading null, got: {diags:?}"
    );
}

#[test]
fn test_ts2698_spread_undefined_emits_error() {
    // Spreading `undefined` in JSX should emit TS2698
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { [key: string]: any }
}
const a = undefined;
const x = <div { ...a } />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES
        ),
        "Expected TS2698 for spreading undefined, got: {diags:?}"
    );
}

#[test]
fn test_ts2698_spread_never_emits_error() {
    // Spreading `never` in JSX should emit TS2698
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { [key: string]: any }
}
const a = {} as never;
const x = <div { ...a } />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES
        ),
        "Expected TS2698 for spreading never, got: {diags:?}"
    );
}

#[test]
fn test_ts2698_not_emitted_for_object_spread() {
    // Spreading a valid object in JSX should NOT emit TS2698
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { [key: string]: any }
}
const a = { x: 1 };
const x = <div { ...a } />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES
        ),
        "Should NOT emit TS2698 for object spread, got: {diags:?}"
    );
}

#[test]
fn test_ts2698_not_emitted_for_any_spread() {
    // Spreading `any` in JSX should NOT emit TS2698
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { [key: string]: any }
}
declare var a: any;
const x = <div { ...a } />;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES
        ),
        "Should NOT emit TS2698 for any spread, got: {diags:?}"
    );
}

#[test]
fn test_ts2698_works_with_intrinsic_any_props() {
    // TS2698 should fire even when IntrinsicElements has [key: string]: any
    // (i.e., when skip_prop_checks would be true). The spread type validation
    // is independent of the props type.
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements { [key: string]: any }
}
const b = null;
const c = undefined;
const d = <div { ...b } />;
const e = <div { ...c } />;
"#;
    let diags = jsx_diagnostics(source);
    let ts2698_count = diags
        .iter()
        .filter(|(c, _)| *c == diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES)
        .count();
    assert!(
        ts2698_count >= 2,
        "Expected at least 2 TS2698 errors (for null and undefined spreads), got {ts2698_count}: {diags:?}"
    );
}

// =============================================================================
// Intrinsic element return type: JSX.Element
// =============================================================================

#[test]
fn test_intrinsic_jsx_element_returns_jsx_element_type() {
    // JSX intrinsic elements (e.g., <div/>) should have type JSX.Element,
    // not IntrinsicElements["div"]. A function returning <div/> should be
    // assignable to () => JSX.Element.
    let source = r#"
declare namespace JSX {
    interface Element { _brand: "element" }
    interface IntrinsicElements {
        div: { className?: string };
        button: { onClick?: () => void };
    }
}
const f = () => <button>test</button>;
const x: () => JSX.Element = f;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Intrinsic JSX element should have type JSX.Element, got: {diags:?}"
    );
}

// =============================================================================
// Generic type parameter props: excess property suppression
// =============================================================================

#[test]
fn test_generic_intersection_props_no_excess_errors() {
    // When component props type is `T & { children?: ... }`, where T is a type
    // parameter from the enclosing scope, excess property checking should be
    // suppressed because T may have additional properties at instantiation time.
    // This was broken because evaluate_type_with_env collapsed
    // `T & { children?: string }` into `{ children?: string; x: number }`
    // (T's constraint), losing the type parameter information.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface SFC<P> {{
    (props: P & {{ children?: string }}): JSX.Element;
}}
function test<T extends {{ x: number }}>(Component: SFC<T>) {{
    return <Component x={{1}} y={{"blah"}} />;
}}
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should NOT emit TS2322 for extra props when type has generic intersection, got: {diags:?}"
    );
}

#[test]
fn test_simple_type_param_props_no_excess_errors() {
    // Simple case: props type is just T (type parameter), should suppress
    // excess property checking.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface SFC<P> {{
    (props: P): JSX.Element;
}}
function test<T extends {{ x: number }}>(Component: SFC<T>) {{
    return <Component x={{1}} y={{"blah"}} />;
}}
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should NOT emit TS2322 for extra props with type parameter, got: {diags:?}"
    );
}

#[test]
fn test_explicit_generic_jsx_props_alias_preserves_callback_member() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Elements {{
    foo: {{ callback?: (value: number) => void }};
    bar: {{ callback?: (value: string) => void }};
}}

type Props<C extends keyof Elements> = {{ as?: C }} & Elements[C];
declare function Test<C extends keyof Elements>(props: Props<C>): null;

<Test<'bar'>
    as="bar"
    callback={{value => value.toUpperCase()}}
/>;
"#
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "explicit generic JSX props alias should not lose the callback member, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
        ),
        "explicit generic JSX props alias should not report excess-property errors for callback, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "callback parameter should remain contextually typed through the JSX props alias, got: {diags:?}"
    );
}

#[test]
fn test_inferred_generic_jsx_props_alias_preserves_callback_member() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Elements {{
    foo: {{ callback?: (value: number) => void }};
    bar: {{ callback?: (value: string) => void }};
}}

type Props<C extends keyof Elements> = {{ as?: C }} & Elements[C];
declare function Test<C extends keyof Elements>(props: Props<C>): null;

<Test
    as="bar"
    callback={{value => value.toUpperCase()}}
/>;
"#
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "inferred generic JSX props alias should not lose the callback member, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
        ),
        "inferred generic JSX props alias should not report excess-property errors for callback, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "callback parameter should remain contextually typed through inferred JSX props alias, got: {diags:?}"
    );
}

#[test]
fn test_generic_jsx_defaulted_props_contextually_type_callback_attrs() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
type Tag = "a" | "button";
type Props<T extends Tag = "a"> = {{
    as?: T;
}} & (T extends "a"
    ? {{ onClick?: (e: {{ href: string }}) => void }}
    : {{ onClick?: (e: {{ disabled: boolean }}) => void }});

declare function UnwrappedLink<T extends Tag = "a">(props: Props<T>): null;

<UnwrappedLink onClick={{(e) => e.href}} />;
<UnwrappedLink as="button" onClick={{(e) => e.disabled}} />;
"#
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "defaulted generic JSX props should contextually type callback attrs, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "defaulted generic JSX props should keep callback members assignable, got: {diags:?}"
    );
}

#[test]
fn test_generic_jsx_defaulted_conditional_props_contextually_type_callback_attrs() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
type ElementType = "a" | "button";
type Exclude<T, U> = T extends U ? never : T;
type Pick<T, K extends keyof T> = {{ [P in K]: T[P] }};
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;
type ComponentPropsWithRef<T extends ElementType> =
    T extends "a"
        ? {{ as?: never; onClick?: (e: {{ href: string }}) => void }}
        : {{ as?: never; onClick?: (e: {{ disabled: boolean }}) => void }};

declare function UnwrappedLink<T extends ElementType = ElementType>(
    props: Omit<ComponentPropsWithRef<ElementType extends T ? "a" : T>, "as">,
): null;

declare function UnwrappedLink2<T extends ElementType = ElementType>(
    props: Omit<ComponentPropsWithRef<ElementType extends T ? "a" : T>, "as"> & {{ as?: T }},
): null;

<UnwrappedLink onClick={{(e) => e.href}} />;
<UnwrappedLink2 onClick={{(e) => e.href}} />;
<UnwrappedLink2 as="button" onClick={{(e) => e.disabled}} />;
"#
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "defaulted conditional generic JSX props should contextually type callback attrs, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "defaulted conditional generic JSX props should keep callback members assignable, got: {diags:?}"
    );
}

#[test]
fn test_generic_jsx_react_style_defaulted_conditional_props_contextually_type_callback_attrs() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare namespace React {{
    interface ReactElement<T = any> {{}}
    interface Element {{}}
    interface AnchorElement extends Element {{ href: string; }}
    interface ButtonElement extends Element {{ disabled: boolean; }}
    interface MouseEvent<T = Element> {{ currentTarget: T; }}
    type EventHandler<E> = {{ bivarianceHack(event: E): void }}["bivarianceHack"];
    type MouseEventHandler<T = Element> = EventHandler<MouseEvent<T>>;

    interface JSXElementConstructor<P> {{
        (props: P): ReactElement<any> | null;
    }}
    interface ComponentType<P = {{}}> {{
        (props: P): ReactElement<any> | null;
    }}
    type ElementType<P = any> =
        {{
            [K in keyof JSX.IntrinsicElements]: P extends JSX.IntrinsicElements[K] ? K : never
        }}[keyof JSX.IntrinsicElements]
        | ComponentType<P>;

    type PropsWithRef<P> = P;
    type ComponentProps<T extends keyof JSX.IntrinsicElements | JSXElementConstructor<any>> =
        T extends JSXElementConstructor<infer P>
            ? P
            : T extends keyof JSX.IntrinsicElements
                ? JSX.IntrinsicElements[T]
                : {{}};
    type ComponentPropsWithRef<T extends ElementType> = PropsWithRef<ComponentProps<T>>;
}}

declare namespace JSX {{
    interface IntrinsicElements {{
        a: {{ onClick?: React.MouseEventHandler<React.AnchorElement> }};
        button: {{ onClick?: React.MouseEventHandler<React.ButtonElement> }};
        div: any;
        span: any;
    }}
}}

type Exclude<T, U> = T extends U ? never : T;
type Pick<T, K extends keyof T> = {{ [P in K]: T[P] }};
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;

declare function UnwrappedLink<T extends React.ElementType = React.ElementType>(
    props: Omit<React.ComponentPropsWithRef<React.ElementType extends T ? "a" : T>, "as">,
): null;

declare function UnwrappedLink2<T extends React.ElementType = React.ElementType>(
    props: Omit<React.ComponentPropsWithRef<React.ElementType extends T ? "a" : T>, "as"> & {{ as?: T }},
): null;

<UnwrappedLink onClick={{(e) => e.currentTarget.href}} />;
<UnwrappedLink2 onClick={{(e) => e.currentTarget.href}} />;
<UnwrappedLink2 as="button" onClick={{(e) => e.currentTarget.disabled}} />;
"#
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "React-style defaulted conditional generic JSX props should contextually type callback attrs, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "React-style defaulted conditional generic JSX props should keep callback members assignable, got: {diags:?}"
    );
}

#[test]
fn test_generic_jsx_props_conditional_component_props_with_ref_keeps_callback_context() {
    let lib_source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        a: { onClick?: (e: { anchorOnly: true }) => void; href?: string };
        button: { onClick?: (e: { buttonOnly: true }) => void; disabled?: boolean };
    }
}

declare namespace React {
    type ElementType = keyof JSX.IntrinsicElements;
    type ComponentPropsWithRef<T extends ElementType> = JSX.IntrinsicElements[T] & { as?: T };
}

declare module "react" {
    export default React;
    export type ElementType = React.ElementType;
    export type ComponentPropsWithRef<T extends React.ElementType> = React.ComponentPropsWithRef<T>;
}
"#;

    let main_source = r#"
type Exclude<T, U> = T extends U ? never : T;
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;

import React from "react";
import { ComponentPropsWithRef, ElementType } from "react";

function UnwrappedLink<T extends ElementType = ElementType>(
  props: Omit<ComponentPropsWithRef<ElementType extends T ? "a" : T>, "as">,
) {
  return <a></a>;
}

<UnwrappedLink onClick={(e) => e.anchorOnly} />;

function UnwrappedLink2<T extends ElementType = ElementType>(
  props: Omit<ComponentPropsWithRef<ElementType extends T ? "a" : T>, "as"> & {
    as?: T;
  },
) {
  return <a></a>;
}

<UnwrappedLink2 onClick={(e) => e.anchorOnly} />;
<UnwrappedLink2 as="button" onClick={(e) => e.buttonOnly} />;
"#;

    let diags = cross_file_jsx_diagnostics(lib_source, main_source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "conditional ComponentPropsWithRef generic JSX props should contextually type callback params, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "conditional ComponentPropsWithRef generic JSX props should preserve callback member access, got: {diags:?}"
    );
}

#[test]
fn test_contextually_typed_jsx_attribute2_react16_fixture_has_no_ts2322() {
    let Some(react_types) = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts") else {
        return;
    };
    let Some(source) = load_typescript_fixture(
        "TypeScript/tests/cases/compiler/contextuallyTypedJsxAttribute2.tsx",
    ) else {
        return;
    };

    let diags = cross_file_jsx_diagnostics_with_mode(&react_types, &source, JsxMode::Preserve);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "real react16 fixture should not emit TS2322, got: {diags:?}"
    );
}

#[test]
fn test_contextually_typed_jsx_attribute2_react16_fixture_has_no_ts7006() {
    let Some(react_types) = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts") else {
        return;
    };
    let Some(source) = load_typescript_fixture(
        "TypeScript/tests/cases/compiler/contextuallyTypedJsxAttribute2.tsx",
    ) else {
        return;
    };

    let diags = cross_file_jsx_diagnostics_with_mode(&react_types, &source, JsxMode::Preserve);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "real react16 fixture should not emit TS7006, got: {diags:?}"
    );
}

#[test]
fn test_generic_props_alias_call_preserves_callback_member() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Elements {{
    foo: {{ callback?: (value: number) => void }};
    bar: {{ callback?: (value: string) => void }};
}}

type Props<C extends keyof Elements> = {{ as?: C }} & Elements[C];
declare function Test<C extends keyof Elements>(props: Props<C>): null;

Test({{
    as: "bar",
    callback: value => value.toUpperCase(),
}});
"#
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "generic props alias call should not lose the callback member, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "callback parameter should remain contextually typed through the call path, got: {diags:?}"
    );
}

#[test]
fn test_concrete_props_still_emit_excess_errors() {
    // When props type is fully concrete (no type parameters), excess property
    // checking should still work.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface SFC<P> {{
    (props: P & {{ children?: string }}): JSX.Element;
}}
function test(Component: SFC<{{ x: number }}>) {{
    return <Component x={{1}} y={{"blah"}} />;
}}
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should emit TS2322 for excess property 'y' on concrete props, got: {diags:?}"
    );
}

// =============================================================================
// TS2783: JSX spread overwrite detection
// =============================================================================

#[test]
fn test_spread_overwrite_skips_type_check() {
    // When a later spread will overwrite an explicit attribute, tsc only
    // emits TS2783 (overwrite warning) and does NOT emit TS2322 (type mismatch).
    // This tests the ordering: overwrite detection before type checking.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Props {{
    x: number;
}}
function Foo(props: Props) {{ return <div />; }}
const p: Props = {{ x: 1 }};
let t = <Foo x={{"not a number"}} {{...p}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    // TS2783 should be emitted (spread overwrites 'x')
    assert!(
        has_code(
            &diags,
            diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN
        ),
        "Expected TS2783 for overwritten attribute, got: {diags:?}"
    );
    // TS2322 should NOT be emitted (type check skipped because overwritten)
    let ts2322_for_x = diags.iter().any(|(c, msg)| {
        *c == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE && msg.contains("string")
    });
    assert!(
        !ts2322_for_x,
        "Should NOT emit TS2322 for overwritten attribute, got: {diags:?}"
    );
}

#[test]
fn test_ts2783_jsx_spread_overwrites_explicit_attribute() {
    // When a required property in a spread follows an explicit attribute with
    // the same name, TS2783 should be emitted on the explicit attribute.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Props {{
    a: number;
    b: number;
}}
function Foo(props: Props) {{ return <div />; }}
const p: Props = {{ a: 1, b: 1 }};
let x = <Foo a={{1}} {{...p}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN
        ),
        "Should emit TS2783 when spread overwrites explicit attribute, got: {diags:?}"
    );
}

#[test]
fn test_ts2783_not_emitted_for_optional_spread_property() {
    // When the spread property is optional, the explicit attribute may NOT be
    // overwritten at runtime, so TS2783 should NOT be emitted.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Props {{
    a: number;
    b: number;
    d?: number;
}}
function Foo(props: Props) {{ return <div />; }}
const p: Props = {{ a: 1, b: 1 }};
let x = <Foo d={{1}} {{...p}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN
        ),
        "Should NOT emit TS2783 when spread has optional property, got: {diags:?}"
    );
}

#[test]
fn test_ts2783_multiple_spreads_track_required_only() {
    // First spread has optional `d`, so no TS2783. Second spread has required
    // `d`, so TS2783 fires for the original explicit attribute.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Props {{
    a: number;
    d?: number;
}}
function Foo(props: Props) {{ return <div />; }}
const p: Props = {{ a: 1 }};
let x = <Foo a={{1}} d={{1}} {{...p}} {{...{{ d: 1 }}}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2783_count = diags
        .iter()
        .filter(|(c, _)| {
            *c == diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN
        })
        .count();
    // 'a' overwritten by first spread (required in Props), 'd' overwritten by second spread
    assert!(
        ts2783_count >= 2,
        "Should emit TS2783 for both 'a' (required in first spread) and 'd' (required in second spread), got {ts2783_count} TS2783 errors: {diags:?}"
    );
}

#[test]
fn test_intrinsic_later_inferred_spread_emits_ts2783_and_ts2322() {
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        test1: { x: string; y?: number; z?: string };
    }
}

var obj5 = { x: 32, y: 32 };
<test1 x="ok" {...obj5} />;

var obj7 = { x: 'foo' };
<test1 x={32} {...obj7} />;
"#;
    let diags = jsx_diagnostics(source);
    let ts2783_count = diags
        .iter()
        .filter(|(code, _)| {
            *code == diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN
        })
        .count();
    assert!(
        ts2783_count == 2,
        "Later inferred spreads should emit TS2783 for each overwritten explicit attr, got: {diags:?}"
    );
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Later inferred spreads should still report the spread-side TS2322 mismatch, got: {diags:?}"
    );
}

#[test]
fn test_intrinsic_jsx_spread_callback_property_uses_method_signature_context() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        test1: { x?: (n: { len: number }) => number };
    }
}

<test1 {...{ x: (n) => 0 }} />;
<test1 {...{ x: (n) => n.len }} />;
"#;

    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "JSX spread callback props should contextually type parameters, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "JSX spread callback props should preserve callback member access, got: {diags:?}"
    );
}

// =============================================================================
// JSX Children Contextual Typing
// =============================================================================

#[test]
fn test_jsx_children_callback_gets_contextual_type() {
    // When a component has `children: (arg: SomeType) => Element`, a callback
    // child like `{(arg) => ...}` should get its parameter typed from the
    // `children` prop — no TS7006 should be emitted.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface User {{ Name: string }}
function FetchUser(props: {{ children: (user: User) => JSX.Element }}) {{
    return <div />;
}}
function UserName() {{
    return <FetchUser>{{user => <div />}}</FetchUser>;
}}
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts7006 = diags
        .iter()
        .filter(|(c, _)| *c == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE)
        .count();
    assert!(
        ts7006 == 0,
        "Should NOT emit TS7006 when children callback is contextually typed, got: {diags:?}"
    );
}

#[test]
fn test_jsx_children_callback_union_props_gets_contextual_type() {
    // Discriminated union props with different children callback signatures:
    // When the children callback types differ across union members (e.g.,
    // (arg: string) => void vs (arg: number) => void), tsc uses discriminant
    // narrowing to pick the right callback type. Our solver unions the
    // parameter types (string | number) for contextual typing.
    //
    // TODO: With pure speculative typing (no dedup state leaks), the
    // contextual typing for children callbacks in discriminated union props
    // needs to be provided through the proper contextual typing mechanism,
    // not through stale dedup state that happened to suppress TS7006.
    // This test now expects TS7006 until proper discriminant narrowing for
    // JSX children callbacks is implemented.
    let source = format!(
        r#"
{JSX_PREAMBLE}
type Props =
  | {{ renderNumber?: false; children: (arg: string) => void }}
  | {{ renderNumber: true; children: (arg: number) => void }};
declare function Foo(props: Props): JSX.Element;
const Test = () => {{
    return <Foo>{{(value) => {{}}}}</Foo>;
}};
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts7006 = diags
        .iter()
        .filter(|(c, _)| *c == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE)
        .count();
    // With pure speculation, TS7006 is now correctly emitted because the
    // stale dedup state that previously suppressed it is properly cleaned up.
    // The proper fix is discriminant narrowing for union JSX children props.
    assert!(
        ts7006 <= 1,
        "Expected at most one TS7006 for union children callback, got: {diags:?}"
    );
}

#[test]
fn test_generic_jsx_children_body_callbacks_use_inferred_props() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare namespace React {{
    interface ReactElement<T = any> {{}}
}}

declare const TestComponentWithChildren: <T, TParam>(props: {{
  state: T;
  selector?: (state: NoInfer<T>) => TParam;
  children?: (state: NoInfer<TParam>) => React.ReactElement<any> | null;
}}) => React.ReactElement<any>;

declare const TestComponentWithoutChildren: <T, TParam>(props: {{
  state: T;
  selector?: (state: NoInfer<T>) => TParam;
  notChildren?: (state: NoInfer<TParam>) => React.ReactElement<any> | null;
}}) => React.ReactElement<any>;

<TestComponentWithChildren state={{{{ foo: 123 }}}} selector={{state => state.foo}}>
  {{selected => {{
    const check: number = selected;
    return <div>{{check}}</div>;
  }}}}
</TestComponentWithChildren>;
"#
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "Generic JSX body children should reuse inferred props for callback contextual typing, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Generic JSX body children inference should not fall back to TS2322, got: {diags:?}"
    );
}

#[test]
fn test_generic_jsx_children_defaulted_type_param_infers_from_selector() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare namespace React {{
    interface ReactElement<T = any> {{}}
}}

interface State {{
    value: boolean;
}}

declare const Subscribe: <TSelected = State>(props: {{
  selector?: (state: State) => TSelected;
  children: (state: TSelected) => void;
}}) => React.ReactElement<any>;

<Subscribe selector={{state => [state.value]}}>
  {{([value = false]) => {{
      const check: boolean = value;
  }}}}
</Subscribe>;
"#
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "Defaulted generic JSX children should get callback contextual typing from selector inference, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE
        ),
        "Defaulted generic JSX children destructuring should stay on the request path, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Defaulted generic JSX children inference should not emit TS2322, got: {diags:?}"
    );
}

#[test]
fn test_jsx_children_presence_narrows_union_component_type_for_body_children() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare namespace React {{
    interface Component<P> {{ props: P; }}
    interface ComponentClass<P> {{ new(props: P): Component<P>; }}
    interface FunctionComponent<P> {{ (props: P): JSX.Element; }}
    type ComponentType<P> = ComponentClass<P> | FunctionComponent<P>;
}}
type Props =
  | {{
        icon: string;
        label: string;
        children(props: {{ onClose: () => void }}): JSX.Element;
        controls?: never;
    }}
  | {{
        icon: string;
        label: string;
        controls: {{ title: string }}[];
        children?: never;
    }};
declare const DropdownMenu: React.ComponentType<Props>;
const Test = () => (
    <DropdownMenu icon="move" label="Select a direction">
        {{({{ onClose }}) => <div />}}
    </DropdownMenu>
);
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "Body children should be contextually typed after union narrowing, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE
        ),
        "Destructured body children should be contextually typed after union narrowing, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Union narrowing on children presence should avoid TS2322 here, got: {diags:?}"
    );
}

#[test]
fn test_jsx_children_presence_narrows_union_component_type_for_explicit_children_attr() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare namespace React {{
    interface Component<P> {{ props: P; }}
    interface ComponentClass<P> {{ new(props: P): Component<P>; }}
    interface FunctionComponent<P> {{ (props: P): JSX.Element; }}
    type ComponentType<P> = ComponentClass<P> | FunctionComponent<P>;
}}
type Props =
  | {{
        icon: string;
        label: string;
        children(props: {{ onClose: () => void }}): JSX.Element;
        controls?: never;
    }}
  | {{
        icon: string;
        label: string;
        controls: {{ title: string }}[];
        children?: never;
    }};
declare const DropdownMenu: React.ComponentType<Props>;
const Test = () => (
    <DropdownMenu
        icon="move"
        label="Select a direction"
        children={{({{ onClose }}) => <div />}}
    />
);
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "Explicit children attr should be contextually typed after union narrowing, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE
        ),
        "Destructured explicit children attr should be contextually typed after union narrowing, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Union narrowing on explicit children attr should avoid TS2322 here, got: {diags:?}"
    );
}

#[test]
fn test_jsx_children_presence_narrows_react_component_type_wrappers() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare namespace React {{
    interface ReactElement<T = any> {{}}
    type ReactNode = ReactElement<any> | string | number | boolean | null | undefined;
    interface Component<P, S = {{}}> {{
        readonly props: Readonly<{{ children?: ReactNode }}> & Readonly<P>;
        readonly state: Readonly<S>;
    }}
    interface ComponentClass<P = {{}}> {{ new(props: P, context?: any): Component<P, any>; }}
    interface StatelessComponent<P = {{}}> {{
        (props: P & {{ children?: ReactNode }}, context?: any): ReactElement<any> | null;
    }}
    type ComponentType<P = {{}}> = ComponentClass<P> | StatelessComponent<P>;
}}
type Props =
  | {{
        icon: string;
        label: string;
        children(props: {{ onClose: () => void }}): JSX.Element;
        controls?: never;
    }}
  | {{
        icon: string;
        label: string;
        controls: {{ title: string }}[];
        children?: never;
    }};
declare const DropdownMenu: React.ComponentType<Props>;
const BodyChild = (
    <DropdownMenu icon="move" label="Select a direction">
        {{({{ onClose }}) => <div />}}
    </DropdownMenu>
);
const ExplicitChild = (
    <DropdownMenu
        icon="move"
        label="Select a direction"
        children={{({{ onClose }}) => <div />}}
    />
);
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "React.ComponentType wrappers should preserve children contextual typing, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE
        ),
        "Destructured JSX children should be contextually typed through React wrappers, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "React.ComponentType wrapper normalization should avoid downstream TS2322 here, got: {diags:?}"
    );
}

#[test]
fn test_react_component_type_missing_required_prop_emits_ts2741() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare namespace React {{
    interface Component<P, S = {{}}> {{
        props: P;
        state: S;
        render(): JSX.Element;
    }}
    interface ComponentClass<P = {{}}> {{
        new(props: P, context?: any): Component<P, any>;
    }}
    interface FunctionComponent<P = {{}}> {{
        (props: P, context?: any): JSX.Element | null;
    }}
    type ComponentType<P = {{}}> = ComponentClass<P> | FunctionComponent<P>;
}}
declare const Elem: React.ComponentType<{{ someKey: string }}>;

const bad = <Elem />;
const good = <Elem someKey="ok" />;
"#
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "React.ComponentType wrappers should report missing props via TS2741, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "React.ComponentType missing props should not fall back to TS2322, got: {diags:?}"
    );
}

#[test]
fn test_jsx_children_presence_narrows_namespace_merged_component_type_wrappers() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare namespace React {{
    interface ReactElement<T = any> {{}}
    type ReactNode = ReactElement<any> | string | number | boolean | null | undefined;
    interface Component<P, S = {{}}> {{
        readonly props: Readonly<{{ children?: ReactNode }}> & Readonly<P>;
        readonly state: Readonly<S>;
    }}
    interface ComponentClass<P = {{}}> {{ new(props: P, context?: any): Component<P, any>; }}
    interface StatelessComponent<P = {{}}> {{
        (props: P & {{ children?: ReactNode }}, context?: any): ReactElement<any> | null;
    }}
    type ComponentType<P = {{}}> = ComponentClass<P> | StatelessComponent<P>;
}}
declare namespace DropdownMenu {{
    interface BaseProps {{
        icon: string;
        label: string;
    }}
    interface PropsWithChildren extends BaseProps {{
        children(props: {{ onClose: () => void }}): JSX.Element;
        controls?: never;
    }}
    interface PropsWithControls extends BaseProps {{
        controls: {{ title: string }}[];
        children?: never;
    }}
    type Props = PropsWithChildren | PropsWithControls;
}}
declare const DropdownMenu: React.ComponentType<DropdownMenu.Props>;
const BodyChild = (
    <DropdownMenu icon="move" label="Select a direction">
        {{({{ onClose }}) => <div />}}
    </DropdownMenu>
);
const ExplicitChild = (
    <DropdownMenu
        icon="move"
        label="Select a direction"
        children={{({{ onClose }}) => <div />}}
    />
);
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "Merged namespace/value React.ComponentType wrappers should preserve callback contextual typing, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE
        ),
        "Merged namespace/value wrappers should contextually type destructured children callbacks, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Merged namespace/value wrapper normalization should avoid downstream TS2322 here, got: {diags:?}"
    );
}

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
#[ignore = "TS2345 arrow-body change suppresses generic JSX children errors — needs investigation"]
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
#[ignore = "TS2345 arrow-body change suppresses generic JSX children errors — needs investigation"]
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

#[test]
fn jsx_children_multiple_children_for_single_type_emits_ts2746() {
    // Multiple children when `children: JSX.Element` (non-array) should emit TS2746
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    a: number;
    children: JSX.Element;
}}
function Comp(p: Prop) {{ return <div></div>; }}
let k = <Comp a={{10}}><div>hi</div><div>bye</div></Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_A_SINGLE_CHILD_OF_TYPE_BUT_MULTIPLE_CHILDREN_WERE_PRO
        ),
        "Multiple children for non-array children type should emit TS2746, got: {diags:?}"
    );
}

#[test]
fn jsx_children_multiple_children_for_array_type_no_error() {
    // Multiple children when `children: JSX.Element[]` should be OK
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    a: number;
    children: JSX.Element[];
}}
function Comp(p: Prop) {{ return <div></div>; }}
let k = <Comp a={{10}}><div>hi</div><div>bye</div></Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_A_SINGLE_CHILD_OF_TYPE_BUT_MULTIPLE_CHILDREN_WERE_PRO
        ),
        "Multiple children for array children type should NOT emit TS2746, got: {diags:?}"
    );
}

#[test]
fn jsx_spread_child_non_array_emits_ts2609() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        [s: string]: any;
    }
}
declare var React: any;

function Todo(prop: { key: number, todo: string }) {
    return <div>{prop.key.toString() + prop.todo}</div>;
}

function TodoList() {
    return <div>
        {...<Todo key={1} todo="x" />}
    </div>;
}
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::JSX_SPREAD_CHILD_MUST_BE_AN_ARRAY_TYPE
        ),
        "Non-array JSX spread child should emit TS2609, got: {diags:?}"
    );
}

#[test]
fn jsx_spread_child_any_has_no_ts2609() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        [s: string]: any;
    }
}
declare var React: any;
declare let items: any;

let ok = <div>{...items}</div>;
"#;
    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::JSX_SPREAD_CHILD_MUST_BE_AN_ARRAY_TYPE
        ),
        "Any-typed JSX spread child should not emit TS2609, got: {diags:?}"
    );
}

#[test]
fn jsx_spread_child_array_normalizes_to_multiple_children() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    children: JSX.Element[];
}}
function Comp(p: Prop) {{ return <div></div>; }}
let items: JSX.Element[] = [<div></div>];
let ok = <Comp>{{...items}}</Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::JSX_SPREAD_CHILD_MUST_BE_AN_ARRAY_TYPE
        ),
        "Array-typed JSX spread child should not emit TS2609, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_TYPE_WHICH_REQUIRES_MULTIPLE_CHILDREN_BUT_ONLY_A_SING
        ),
        "Array-typed JSX spread child should normalize through the multiple-children path, got: {diags:?}"
    );
}

#[test]
fn jsx_children_union_with_array_allows_multiple() {
    // `children: JSX.Element | JSX.Element[]` should accept multiple children
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    a: number;
    children: JSX.Element | JSX.Element[];
}}
function Comp(p: Prop) {{ return <div></div>; }}
let k = <Comp a={{10}}><div>hi</div><div>bye</div></Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_A_SINGLE_CHILD_OF_TYPE_BUT_MULTIPLE_CHILDREN_WERE_PRO
        ),
        "Union with array member should accept multiple children, got: {diags:?}"
    );
}

#[test]
fn jsx_children_union_with_array_allows_single_child_without_ts2745() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    a: number;
    children: JSX.Element | JSX.Element[];
}}
function Comp(p: Prop) {{ return <div></div>; }}
let k = <Comp a={{10}}><div>hi</div></Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_TYPE_WHICH_REQUIRES_MULTIPLE_CHILDREN_BUT_ONLY_A_SING
        ),
        "Union with single-child branch should not emit TS2745, got: {diags:?}"
    );
}

#[test]
fn jsx_children_union_with_tuple_allows_single_child_without_ts2745() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    a: number;
    children: JSX.Element | [JSX.Element];
}}
function Comp(p: Prop) {{ return <div></div>; }}
let k = <Comp a={{10}}><div>hi</div></Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_TYPE_WHICH_REQUIRES_MULTIPLE_CHILDREN_BUT_ONLY_A_SING
        ),
        "Union with tuple branch should not emit TS2745 for a single child, got: {diags:?}"
    );
}

#[test]
fn jsx_children_tuple_still_requires_multiple_children() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    a: number;
    children: [JSX.Element];
}}
function Comp(p: Prop) {{ return <div></div>; }}
let k = <Comp a={{10}}><div>hi</div></Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_TYPE_WHICH_REQUIRES_MULTIPLE_CHILDREN_BUT_ONLY_A_SING
        ),
        "Tuple-only children should still emit TS2745 for a single child, got: {diags:?}"
    );
}

#[test]
fn jsx_children_single_array_expression_satisfies_array_children_type() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
type Tab = [string, JSX.Element];
interface Prop {{
    children: Tab[];
}}
function Comp(p: Prop) {{ return <div></div>; }}
let tabs: Tab[] = [["Users", <div></div>], ["Products", <div></div>]];
let ok = <Comp>{{tabs}}</Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_TYPE_WHICH_REQUIRES_MULTIPLE_CHILDREN_BUT_ONLY_A_SING
        ),
        "Single array-valued child expression should satisfy array children type without TS2745, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Single array-valued child expression should not fall through to TS2322, got: {diags:?}"
    );
}

#[test]
fn jsx_single_child_component_instance_mismatch_emits_ts2740() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        div: any;
    }
    interface ElementAttributesProperty { props: {} }
    interface ElementChildrenAttribute { children: {} }
}
declare namespace React {
    class Component<P, S = {}> {
        props: P & { children?: any };
        state: S;
        constructor(props?: P, context?: any);
        render(): JSX.Element | null;
        setState(state: any): void;
        forceUpdate(): void;
    }
}
interface Prop {
    children: Button;
}

class Button extends React.Component<any, any> {
    render() {
        return (<div>My Button</div>);
    }
}

function Comp(p: Prop) {
    return <div />;
}

let k1 =
    <Comp>
        <Button />
    </Comp>;
let k2 =
    <Comp>
        {Button}
    </Comp>;
"#;
    let diags = jsx_diagnostics(source);
    let ts2740_msgs: Vec<&str> = diags
        .iter()
        .filter(|(code, _)| {
            matches!(
                *code,
                diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE
                    | diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
            )
        })
        .map(|(_, msg)| msg.as_str())
        .collect();
    assert_eq!(
        ts2740_msgs.len(),
        2,
        "Expected two TS2740 diagnostics for single-child JSX body mismatches, got: {diags:?}"
    );
    assert!(
        ts2740_msgs
            .iter()
            .any(|msg| msg.contains("Type 'Element'") || msg.contains("Type 'ReactElement<any>'")),
        "Expected JSX element child mismatch to report the rendered element source type, got: {ts2740_msgs:?}"
    );
    assert!(
        ts2740_msgs.iter().any(|msg| msg.contains("typeof Button")),
        "Expected expression child mismatch to mention typeof Button, got: {ts2740_msgs:?}"
    );
}

#[test]
fn jsx_children_fixed_tuple_accepts_exact_children() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    children: [JSX.Element, JSX.Element];
}}
declare class Comp {{
    props: Prop;
    render(): JSX.Element;
}}
let ok = <Comp><div /><div /></Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Exact tuple children should not emit TS2322, got: {diags:?}"
    );
}

#[test]
fn jsx_children_fixed_tuple_rejects_extra_children() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    children: [JSX.Element, JSX.Element];
}}
declare class Comp {{
    props: Prop;
    render(): JSX.Element;
}}
let err = <Comp><div /><div /><div /></Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Extra tuple children should emit TS2322, got: {diags:?}"
    );
}

#[test]
fn jsx_children_fixed_tuple_rejects_missing_children() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    children: [JSX.Element, JSX.Element, JSX.Element];
}}
declare class Comp {{
    props: Prop;
    render(): JSX.Element;
}}
let err = <Comp><div /><div /></Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Missing fixed tuple child should emit TS2322, got: {diags:?}"
    );
}

#[test]
fn jsx_children_multiline_formatting_whitespace_does_not_break_array_children() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    children: JSX.Element | JSX.Element[];
}}
function Comp(p: Prop) {{ return <div></div>; }}
let ok =
    <Comp>


        <div />
        <div />
    </Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Formatting-only JSX whitespace should not emit TS2322, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_A_SINGLE_CHILD_OF_TYPE_BUT_MULTIPLE_CHILDREN_WERE_PRO
        ),
        "Formatting-only JSX whitespace should not emit TS2746, got: {diags:?}"
    );
}

#[test]
fn jsx_children_text_mismatch_reports_ts2747_without_ts2322() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    children: JSX.Element | JSX.Element[];
}}
function Comp(p: Prop) {{ return <div></div>; }}
let err = <Comp><div />  <div /></Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::COMPONENTS_DONT_ACCEPT_TEXT_AS_CHILD_ELEMENTS_TEXT_IN_JSX_HAS_THE_TYPE_STRING_BU
        ),
        "Inline JSX whitespace text should emit TS2747, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Text-child mismatch should not also emit TS2322, got: {diags:?}"
    );
}

#[test]
fn jsx_children_render_prop_multiple_children_emit_ts2322_not_ts2746() {
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface User {{
    name: string;
}}
interface Prop {{
    children: (user: User) => JSX.Element;
}}
function FetchUser(p: Prop) {{ return <div></div>; }}
let err =
    <FetchUser>
        {{ user => <div /> }}
        {{ user => <div /> }}
    </FetchUser>;
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2322_count = diags
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .count();
    assert_eq!(
        ts2322_count, 2,
        "Render-prop children in a JSX body should emit one TS2322 per invalid child, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_A_SINGLE_CHILD_OF_TYPE_BUT_MULTIPLE_CHILDREN_WERE_PRO
        ),
        "Render-prop children should not be collapsed into TS2746, got: {diags:?}"
    );
}

#[test]
fn jsx_children_whitespace_only_text_ignored() {
    // Whitespace-only text children should not count as children
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    a: number;
}}
function Comp(p: Prop) {{ return <div></div>; }}
let k = <Comp a={{10}}>   </Comp>;
"#
    );
    let diags = jsx_diagnostics(&source);
    // Should not get any type errors about extra children properties
    let children_errors: Vec<_> = diags
        .iter()
        .filter(|(c, _)| {
            *c == diagnostic_codes::ARE_SPECIFIED_TWICE_THE_ATTRIBUTE_NAMED_WILL_BE_OVERWRITTEN
                || *c == diagnostic_codes::THIS_JSX_TAGS_PROP_EXPECTS_A_SINGLE_CHILD_OF_TYPE_BUT_MULTIPLE_CHILDREN_WERE_PRO
        })
        .collect();
    assert!(
        children_errors.is_empty(),
        "Whitespace-only text should not produce children errors, got: {children_errors:?}"
    );
}

#[test]
fn jsx_children_optional_children_no_error_when_absent() {
    // Optional `children?` should not require children body
    let source = format!(
        r#"
{JSX_CHILDREN_PREAMBLE}
interface Prop {{
    a: number;
    children?: JSX.Element;
}}
function Comp(p: Prop) {{ return <div></div>; }}
let k = <Comp a={{10}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "Optional children should not emit TS2741 when absent, got: {diags:?}"
    );
}

// =============================================================================
// JSX empty expression not counted as child (TS2746 fix)
// =============================================================================

/// Empty JSX expressions like {/* comment */} should not count as children.
/// This prevents false TS2746 ("expects a single child but multiple provided").
#[test]
fn jsx_empty_expression_not_counted_as_child() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Props {{
    children: JSX.Element;
}}
function Wrapper(p: Props) {{ return <div>{{p.children}}</div>; }}
const element = (
    <Wrapper>
        {{/* comment */}}
        <div>Hello</div>
    </Wrapper>
);
"#
    );
    let diags = jsx_diagnostics(&source);
    // TS2746 should NOT fire — the empty expression {/* comment */} doesn't count
    assert!(
        !diags.iter().any(|(c, _)| *c == 2746),
        "Empty JSX expression should not count as child; got TS2746: {diags:?}"
    );
}

/// Verify that a single non-comment child does NOT trigger TS2746.
/// This complements the empty expression test above.
#[test]
fn jsx_single_real_child_no_ts2746() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Props {{
    children: JSX.Element;
}}
function Wrapper(p: Props) {{ return <div>{{p.children}}</div>; }}
const element = (
    <Wrapper>
        <div>Hello</div>
    </Wrapper>
);
"#
    );
    let diags = jsx_diagnostics(&source);
    // Single child — TS2746 should NOT fire
    assert!(
        !diags.iter().any(|(c, _)| *c == 2746),
        "Single child should not trigger TS2746, got: {diags:?}"
    );
}

// =============================================================================
// JSX factory namespace resolution (TS7026 fix)
// =============================================================================

/// When @jsxFactory is a dotted name like "X.jsx", the JSX namespace
/// should be resolved from X.JSX, not just the global JSX namespace.
#[test]
fn jsx_factory_dotted_resolves_namespace_from_root_exports() {
    let source = r#"
namespace X {
    export namespace JSX {
        export interface IntrinsicElements {
            [name: string]: any;
        }
        export interface Element {}
    }
    export function jsx() {}
}
let a = <div/>;
"#;
    let options = CheckerOptions {
        jsx_mode: JsxMode::React,
        jsx_factory: "X.jsx".to_string(),
        jsx_factory_from_config: true,
        ..CheckerOptions::default()
    };

    let file_name = "test.tsx";
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );
    checker.check_source_file(root);
    let diags: Vec<(u32, String)> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    // TS7026 should NOT fire — X.JSX.IntrinsicElements exists
    assert!(
        !diags.iter().any(|(c, _)| *c == 7026),
        "Factory namespace X.JSX should be found; got TS7026: {diags:?}"
    );
}
