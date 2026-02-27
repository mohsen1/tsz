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

// =============================================================================
// Cross-file: import React = require('react') with ambient module
// =============================================================================

/// Helper to compile a multi-file JSX project and return diagnostics for the main file.
fn cross_file_jsx_diagnostics(lib_source: &str, main_source: &str) -> Vec<(u32, String)> {
    // Parse and bind lib file (react.d.ts equivalent)
    let mut parser_lib = ParserState::new("react.d.ts".to_string(), lib_source.to_string());
    let root_lib = parser_lib.parse_source_file();
    let mut binder_lib = tsz_binder::BinderState::new();
    binder_lib.bind_source_file(parser_lib.get_arena(), root_lib);

    // Parse and bind main file
    let mut parser_main = ParserState::new("file.tsx".to_string(), main_source.to_string());
    let root_main = parser_main.parse_source_file();
    let mut binder_main = tsz_binder::BinderState::new();
    binder_main.bind_source_file(parser_main.get_arena(), root_main);

    let arena_lib = Arc::new(parser_lib.get_arena().clone());
    let arena_main = Arc::new(parser_main.get_arena().clone());
    let binder_lib = Arc::new(binder_lib);
    let binder_main = Arc::new(binder_main);

    let all_arenas = Arc::new(vec![Arc::clone(&arena_main), Arc::clone(&arena_lib)]);
    let all_binders = Arc::new(vec![Arc::clone(&binder_main), Arc::clone(&binder_lib)]);

    let options = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
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

    checker.check_source_file(root_main);
    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
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
