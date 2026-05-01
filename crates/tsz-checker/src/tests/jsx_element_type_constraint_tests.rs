//! Regression tests for `JSX.ElementType` as the JSX-component validity
//! constraint.
//!
//! When the user defines `JSX.ElementType`, that type — not `JSX.Element`
//! — is the authoritative constraint for what can appear as a JSX
//! component. Source: `compiler/jsxElementType.tsx`. Without this rule,
//! tsz emits TS2786 for any function component that returns string /
//! number / array / Promise even when `JSX.ElementType` admits them.

use crate::test_utils::check_source_diagnostics;

fn diag_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| d.code)
        .collect()
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
