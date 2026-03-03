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

/// Return diagnostics with position info (code, start, message).
fn jsx_diagnostics_with_pos(source: &str) -> Vec<(u32, u32, String)> {
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
    // Discriminated union props: children callback should get contextual type
    // from the union of `children` prop types across union members.
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
    assert!(
        ts7006 == 0,
        "Should NOT emit TS7006 for children callback in discriminated union props, got: {diags:?}"
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
