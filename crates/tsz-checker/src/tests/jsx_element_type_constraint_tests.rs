//! Regression tests for `JSX.ElementType` as the JSX-component validity
//! constraint.
//!
//! When the user defines `JSX.ElementType`, that type — not `JSX.Element`
//! — is the authoritative constraint for what can appear as a JSX
//! component. Source: `compiler/jsxElementType.tsx`.

use crate::test_utils::check_source_codes_named;

fn diag_codes(source: &str) -> Vec<u32> {
    check_source_codes_named(source, "test.tsx")
}

const JSX_ELEMENT_TYPE_PRELUDE: &str = r#"
declare global {
    namespace JSX {
        interface Element {}
        interface ElementClass {}
        interface IntrinsicElements {}
        type ElementType = string | ((props: any) => string | number | boolean);
    }
}
"#;

/// `ElementType` only permits strings and class constructors; function
/// components (call-only signatures) must be rejected with TS2786.
const JSX_CONSTRUCTOR_ONLY_ELEMENT_TYPE_PRELUDE: &str = r#"
declare global {
    namespace JSX {
        interface Element {}
        interface ElementClass {}
        interface IntrinsicElements {}
        type ElementType = string | (new (...args: any[]) => any);
    }
}
"#;

// ─── ElementType admits function components ───────────────────────────────────

/// When `JSX.ElementType` admits `string`-returning function components,
/// using one as JSX should NOT emit TS2786.
#[test]
fn jsx_element_type_admits_string_returning_function_component() {
    let source = format!(
        r#"
{JSX_ELEMENT_TYPE_PRELUDE}
const RenderString = ({{ title }}: {{ title: string }}) => title;
const _ = <RenderString title="hi" />;
"#
    );
    let codes = diag_codes(&source);
    assert!(
        !codes.contains(&2786),
        "JSX.ElementType admits string-returning function — TS2786 should not fire. Got: {codes:?}"
    );
}

/// Anti-hardcoding cover: same shape with renamed identifiers.
#[test]
fn jsx_element_type_admits_number_returning_function_component_renamed() {
    let source = format!(
        r#"
{JSX_ELEMENT_TYPE_PRELUDE}
const Counter = ({{ value }}: {{ value: number }}) => value + 1;
const _ = <Counter value={{42}} />;
"#
    );
    let codes = diag_codes(&source);
    assert!(
        !codes.contains(&2786),
        "Renamed: number-returning fn allowed by JSX.ElementType. Got: {codes:?}"
    );
}

// ─── Constructor-only ElementType rejects function components ─────────────────

/// When `JSX.ElementType` only allows strings and constructors, a plain
/// function component (call-only) must emit TS2786.
#[test]
fn jsx_constructor_only_element_type_rejects_function_component() {
    let source = format!(
        r#"
{JSX_CONSTRUCTOR_ONLY_ELEMENT_TYPE_PRELUDE}
function ListView(p: {{ items: string[] }}) {{ return null as any; }}
const _ = <ListView items={{[]}} />;
"#
    );
    let codes = diag_codes(&source);
    assert!(
        codes.contains(&2786),
        "Constructor-only ElementType must reject function component with TS2786. Got: {codes:?}"
    );
}

/// Anti-hardcoding: renamed component, same constructor-only constraint.
#[test]
fn jsx_constructor_only_element_type_rejects_function_component_renamed() {
    let source = format!(
        r#"
{JSX_CONSTRUCTOR_ONLY_ELEMENT_TYPE_PRELUDE}
function DataGrid(p: {{ rows: number[] }}) {{ return null as any; }}
const _ = <DataGrid rows={{[1, 2]}} />;
"#
    );
    let codes = diag_codes(&source);
    assert!(
        codes.contains(&2786),
        "Renamed: constructor-only ElementType must reject SFC. Got: {codes:?}"
    );
}

/// Arrow-function component (call-only) is also rejected when ElementType
/// requires a constructor.
#[test]
fn jsx_constructor_only_element_type_rejects_arrow_function_component() {
    let source = format!(
        r#"
{JSX_CONSTRUCTOR_ONLY_ELEMENT_TYPE_PRELUDE}
const Alert = (p: {{ msg: string }}) => null as any;
const _ = <Alert msg="hi" />;
"#
    );
    let codes = diag_codes(&source);
    assert!(
        codes.contains(&2786),
        "Constructor-only ElementType must reject arrow function with TS2786. Got: {codes:?}"
    );
}

/// A class component (construct signature) is accepted even when ElementType
/// only allows constructors.
#[test]
fn jsx_constructor_only_element_type_accepts_class_component() {
    let source = format!(
        r#"
{JSX_CONSTRUCTOR_ONLY_ELEMENT_TYPE_PRELUDE}
class TableView {{
    constructor(p: {{ items: string[] }}) {{}}
    render() {{ return null; }}
}}
const _ = <TableView items={{[]}} />;
"#
    );
    let codes = diag_codes(&source);
    assert!(
        !codes.contains(&2786),
        "Constructor-only ElementType must accept class component without TS2786. Got: {codes:?}"
    );
}

/// Anti-hardcoding: renamed class, same constraint.
#[test]
fn jsx_constructor_only_element_type_accepts_class_component_renamed() {
    let source = format!(
        r#"
{JSX_CONSTRUCTOR_ONLY_ELEMENT_TYPE_PRELUDE}
class GridView {{
    constructor(p: {{ cols: number }}) {{}}
    render() {{ return null; }}
}}
const _ = <GridView cols={{3}} />;
"#
    );
    let codes = diag_codes(&source);
    assert!(
        !codes.contains(&2786),
        "Renamed class: constructor-only ElementType must not emit TS2786. Got: {codes:?}"
    );
}
