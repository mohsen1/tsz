//! Regression tests for `JSX.ElementType` as the JSX-component validity
//! constraint.
//!
//! When the user defines `JSX.ElementType`, that type — not `JSX.Element`
//! — is the authoritative constraint for what can appear as a JSX
//! component. Source: `compiler/jsxElementType.tsx`.

use crate::CheckerOptions;
use crate::test_utils::check_source;
use crate::test_utils::check_source_codes_named;

fn diag_codes(source: &str) -> Vec<u32> {
    check_source_codes_named(source, "test.tsx")
}

fn diagnostics(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    check_source(source, "test.tsx", CheckerOptions::default())
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

const JSX_ALIAS_APPLICATION_ELEMENT_TYPE_PRELUDE: &str = r#"
declare global {
    namespace JSX {
        interface Element {}
        interface ElementClass {}
        interface IntrinsicElements {}
        type NodeLike = string | number;
        type ComponentLike<P> = ((props: P) => NodeLike) | (new (props: P) => any);
        type ElementType = string | ComponentLike<any>;
    }
}
"#;

const JSX_CONSTRUCTOR_ALIAS_ELEMENT_TYPE_PRELUDE: &str = r#"
declare global {
    namespace JSX {
        interface Element {}
        interface ElementClass {}
        interface IntrinsicElements {}
        type ConstructLike<P> = new (props: P) => any;
        type ElementType = string | ConstructLike<any>;
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

/// Alias/application cover: callable arms inside a generic `ElementType` alias
/// still authorize components by return type.
#[test]
fn jsx_element_type_alias_application_admits_string_returning_function_component() {
    let source = format!(
        r#"
{JSX_ALIAS_APPLICATION_ELEMENT_TYPE_PRELUDE}
const Label = ({{ title }}: {{ title: string }}) => title;
const _ = <Label title="ok" />;
"#
    );
    let codes = diag_codes(&source);
    assert!(
        !codes.contains(&2786),
        "Generic alias ElementType admits string-returning function. Got: {codes:?}"
    );
}

#[test]
fn jsx_element_type_alias_assignment_admits_primitive_returning_components() {
    let source = format!(
        r#"
{JSX_ALIAS_APPLICATION_ELEMENT_TYPE_PRELUDE}
type MergePropTypes<P, T> = P;
type Defaultize<P, D> = P;
declare namespace PropTypes {{
    type InferProps<T> = any;
}}
declare global {{
    namespace JSX {{
        interface IntrinsicAttributes {{}}
        type LibraryManagedAttributes<C, P> =
            C extends {{ propTypes: infer T; defaultProps: infer D; }}
                ? Defaultize<MergePropTypes<P, PropTypes.InferProps<T>>, D>
                : C extends {{ propTypes: infer T; }}
                    ? MergePropTypes<P, PropTypes.InferProps<T>>
                    : C extends {{ defaultProps: infer D; }}
                        ? Defaultize<P, D>
                        : P;
    }}
}}
let Component: JSX.ComponentLike<{{ title: string }}>;
const Caption = ({{ title }}: {{ title: string }}) => title;
const Count = ({{ title }}: {{ title: string }}) => title.length;
Component = Caption;
Component = Count;
const a = <Caption extra />;
const b = <Count extra />;
"#
    );
    let diagnostics = diagnostics(&source);
    assert!(
        diagnostics
            .iter()
            .filter(|diag| {
                diag.code == 2322
                    && (diag
                        .message_text
                        .contains("not assignable to type 'JSX.ComponentLike")
                        || diag
                            .message_text
                            .contains("not assignable to type 'ComponentLike"))
            })
            .count()
            == 0,
        "ElementType constructor alias assignment should not emit TS2322 for primitive returns. Got: {diagnostics:?}"
    );
}

#[test]
fn jsx_element_type_react_node_constructor_alias_does_not_leak_component_assignment() {
    let source = r#"
declare namespace React {
    interface ReactElement<P = any> {}
    interface ReactPortal {}
    class Component<P, S = any> {
        constructor(props: P);
        props: P;
    }
}
type React18ReactFragment = ReadonlyArray<React18ReactNode>;
type React18ReactNode =
    | React.ReactElement<any>
    | string
    | number
    | React18ReactFragment
    | React.ReactPortal
    | boolean
    | null
    | undefined
    | Promise<React18ReactNode>;
type NewElementConstructor<P> =
    | ((props: P) => React18ReactNode)
    | (new (props: P) => React.Component<P, any>);
type MergePropTypes<P, T> = P;
type Defaultize<P, D> = P;
declare namespace PropTypes {
    type InferProps<T> = any;
}
declare global {
    namespace JSX {
        interface Element extends React.ReactElement<any> {}
        interface IntrinsicAttributes {}
        interface IntrinsicElements { div: {}; }
        type ElementType = string | NewElementConstructor<any>;
        type LibraryManagedAttributes<C, P> =
            C extends { propTypes: infer T; defaultProps: infer D; }
                ? Defaultize<MergePropTypes<P, PropTypes.InferProps<T>>, D>
                : C extends { propTypes: infer T; }
                    ? MergePropTypes<P, PropTypes.InferProps<T>>
                    : C extends { defaultProps: infer D; }
                        ? Defaultize<P, D>
                        : P;
    }
}

let Component: NewElementConstructor<{ title: string }>;
const RenderText = ({ title }: { title: string }) => title;
const RenderCount = ({ title }: { title: string }) => title.length;
Component = RenderText;
Component = RenderCount;
<RenderText excessProp />;
<RenderCount excessProp />;
"#;
    let diagnostics = diagnostics(source);
    assert!(
        diagnostics.iter().all(|diag| {
            diag.code != 2322
                || !diag
                    .message_text
                    .contains("not assignable to type 'NewElementConstructor")
        }),
        "ElementType validation should not leak constructor-alias TS2322, got: {diagnostics:?}"
    );
}

#[test]
fn jsx_generic_lma_display_uses_constraint_props_after_invalid_two_param_component() {
    let source = r#"
declare namespace React {
    interface ReactElement<P = any> {}
    type ForwardedRef<T> = unknown;
}
declare global {
    namespace JSX {
        interface Element extends React.ReactElement<any> {}
        interface IntrinsicAttributes {}
        interface IntrinsicElements {}
        type LibraryManagedAttributes<C, P> =
            C extends { propTypes: infer T; defaultProps: infer D; }
                ? P
                : C extends { propTypes: infer T; }
                    ? P
                    : C extends { defaultProps: infer D; }
                        ? P
                        : P;
        type ElementType = string | ((props: any) => React.ReactElement<any>);
    }
}

function IgnoredPrior(props: {}, ref: React.ForwardedRef<typeof IgnoredPrior>) {
    return null;
}
<IgnoredPrior />;

function wrap<T extends (props: {}) => React.ReactElement<any>>(Component: T) {
    return <Component />;
}
"#;
    let diagnostics = diagnostics(source);
    let generic_diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2322 && diag.message_text.contains("LibraryManagedAttributes"))
        .expect("expected generic LibraryManagedAttributes TS2322");
    assert!(
        generic_diag
            .message_text
            .contains("LibraryManagedAttributes<T, {}>")
            && !generic_diag.message_text.contains("IgnoredPrior"),
        "Generic LMA display should use the type-parameter constraint props, got: {generic_diag:?}"
    );
}

/// Return compatibility alone is not enough: a source callable that requires
/// more parameters than the `ElementType` callable arm is not a valid JSX
/// component.
#[test]
fn jsx_element_type_alias_application_rejects_two_parameter_function_component() {
    let source = format!(
        r#"
{JSX_ALIAS_APPLICATION_ELEMENT_TYPE_PRELUDE}
function Forwarded(props: {{}}, ref: unknown) {{ return "ok"; }}
const _ = <Forwarded />;
"#
    );
    let codes = diag_codes(&source);
    assert!(
        codes.contains(&2786),
        "Generic alias ElementType must reject a two-parameter function component. Got: {codes:?}"
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

/// Arrow-function component (call-only) is also rejected when `ElementType`
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

/// Constructor-only aliases must not be accepted by the callable-return
/// fallback used for generic `ElementType` aliases.
#[test]
fn jsx_constructor_alias_element_type_rejects_function_component() {
    let source = format!(
        r#"
{JSX_CONSTRUCTOR_ALIAS_ELEMENT_TYPE_PRELUDE}
const Label = ({{ title }}: {{ title: string }}) => title;
const _ = <Label title="ok" />;
"#
    );
    let codes = diag_codes(&source);
    assert!(
        codes.contains(&2786),
        "Constructor-only alias ElementType must reject function component. Got: {codes:?}"
    );
}

/// A class component (construct signature) is accepted even when `ElementType`
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
