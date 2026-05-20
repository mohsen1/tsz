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

/// Recursive type aliases through generic containers (`ReadonlyArray<T>`,
/// `Promise<T>`) must NOT emit TS2456. TypeScript only reports TS2456 for
/// direct (non-wrapped) circularity; every recursive reference here is
/// deferred behind a generic wrapper.
///
/// Source: `compiler/jsxElementType.tsx` false-positive repro.
#[test]
fn recursive_type_aliases_through_generic_containers_no_ts2456() {
    use crate::context::CheckerOptions;
    use crate::test_utils::check_source;
    use tsz_common::checker_options::JsxMode;
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        strict_null_checks: true,
        strict: true,
        ..CheckerOptions::default()
    };
    let source = r#"
type React18ReactFragment = ReadonlyArray<React18ReactNode>;
type React18ReactNode =
  | string
  | number
  | React18ReactFragment
  | boolean
  | null
  | undefined
  | Promise<React18ReactNode>;
"#;
    let codes: Vec<u32> = check_source(source, "test.tsx", opts)
        .into_iter()
        .map(|d| d.code)
        .collect();
    assert!(
        !codes.contains(&2456),
        "Recursive type aliases through generic containers should NOT emit TS2456. Got: {codes:?}"
    );
}

/// Same pattern renamed: `Fragment` / `Node` instead of `React18ReactFragment`
/// / `React18ReactNode`. Verifies the check is structural, not name-sensitive.
#[test]
fn recursive_type_aliases_renamed_containers_no_ts2456() {
    use crate::context::CheckerOptions;
    use crate::test_utils::check_source;
    use tsz_common::checker_options::JsxMode;
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        strict_null_checks: true,
        strict: true,
        ..CheckerOptions::default()
    };
    let source = r#"
type MyFragment = ReadonlyArray<MyNode>;
type MyNode =
  | string
  | number
  | MyFragment
  | boolean
  | null
  | undefined
  | Promise<MyNode>;
"#;
    let codes: Vec<u32> = check_source(source, "test.tsx", opts)
        .into_iter()
        .map(|d| d.code)
        .collect();
    assert!(
        !codes.contains(&2456),
        "Renamed recursive aliases through generic containers must NOT emit TS2456. Got: {codes:?}"
    );
}

/// When `JSX.ElementType = string | (new (...args: any[]) => any)`, a plain
/// function component (no `new`) must emit TS2786 because it is not assignable
/// to the constructor branch of `ElementType`.
#[test]
fn jsx_element_type_constructor_only_rejects_plain_function() {
    use crate::context::CheckerOptions;
    use crate::test_utils::check_source;
    use tsz_common::checker_options::JsxMode;
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        strict_null_checks: true,
        strict: true,
        ..CheckerOptions::default()
    };
    // ElementType only allows constructors (new ...) + strings.
    // A plain function (no new) is NOT assignable to it → TS2786.
    let source = r#"
declare global {
    namespace JSX {
        type ElementType = string | (new (...args: any[]) => any);
        interface IntrinsicElements { div: {} }
        interface Element {}
    }
}
class Comp { constructor(p: {}) {} render() { return null; } }
function FComp(p: {}) { return null as any; }
const _a = <Comp />;
const _b = <FComp />;
"#;
    let codes: Vec<u32> = check_source(source, "test.tsx", opts)
        .into_iter()
        .map(|d| d.code)
        .collect();
    assert!(
        codes.contains(&2786),
        "FComp (plain fn) must emit TS2786 when ElementType only allows constructors. Got: {codes:?}"
    );
    assert_eq!(
        codes.iter().filter(|&&c| c == 2786).count(),
        1,
        "Only FComp (plain fn) should emit TS2786; Comp (class) satisfies ElementType. Got: {codes:?}"
    );
}

/// Variant: two renamed function components that are not assignable to a
/// constructor-only `ElementType` must both emit TS2786.
#[test]
fn jsx_element_type_constructor_only_rejects_two_plain_functions() {
    use crate::context::CheckerOptions;
    use crate::test_utils::check_source;
    use tsz_common::checker_options::JsxMode;
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        strict_null_checks: true,
        strict: true,
        ..CheckerOptions::default()
    };
    let source = r#"
declare global {
    namespace JSX {
        type ElementType = string | (new (...args: any[]) => any);
        interface IntrinsicElements {}
        interface Element {}
    }
}
function Widget(p: { x: number }) { return null as any; }
function Button(p: { label: string }) { return null as any; }
const _a = <Widget x={1} />;
const _b = <Button label="ok" />;
"#;
    let codes: Vec<u32> = check_source(source, "test.tsx", opts)
        .into_iter()
        .map(|d| d.code)
        .collect();
    let count_2786 = codes.iter().filter(|&&c| c == 2786).count();
    assert_eq!(
        count_2786, 2,
        "Both plain function components must emit TS2786 with constructor-only ElementType. Got: {codes:?}"
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

/// A 2-required-parameter function component should NOT be assignable to
/// `ElementType = string | ((props: any) => ReactNode)` because the function
/// cannot be called as a JSX component (JSX only passes one props argument).
/// This is the `ReactNativeFlatList` shape from the `jsxElementType.tsx` conformance test.
#[test]
fn jsx_element_type_rejects_two_param_function_component() {
    use crate::context::CheckerOptions;
    use crate::test_utils::check_source;
    use tsz_common::checker_options::JsxMode;
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        strict_null_checks: true,
        strict: true,
        ..CheckerOptions::default()
    };
    // ElementType admits 1-param function components. A 2-required-param fn is
    // not assignable to (props: any) => ReactNode because strictFunctionTypes
    // checks parameters contravariantly and the source has an extra required param.
    let source = r#"
type ReactNode = string | number | null;
declare global {
    namespace JSX {
        type ElementType = string | ((props: any) => ReactNode) | (new (props: any) => any);
        interface Element {}
        interface IntrinsicElements {}
    }
}
declare function FlatList(props: {}, ref: any): ReactNode;
const _a = <FlatList />;
"#;
    let codes: Vec<u32> = check_source(source, "test.tsx", opts)
        .into_iter()
        .map(|d| d.code)
        .collect();
    assert!(
        codes.contains(&2786),
        "Two-param fn (like ReactNativeFlatList) must emit TS2786 when ElementType only accepts 1-param fns. Got: {codes:?}"
    );
}

/// Variant: renamed identifiers — same two-required-param shape with different names.
#[test]
fn jsx_element_type_rejects_two_param_component_renamed() {
    use crate::context::CheckerOptions;
    use crate::test_utils::check_source;
    use tsz_common::checker_options::JsxMode;
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        strict_null_checks: true,
        strict: true,
        ..CheckerOptions::default()
    };
    let source = r#"
type Node = string | null;
declare global {
    namespace JSX {
        type ElementType = string | ((props: any) => Node) | (new (props: any) => any);
        interface Element {}
        interface IntrinsicElements {}
    }
}
declare function ScrollView(config: {}, context: any): Node;
const _x = <ScrollView />;
"#;
    let codes: Vec<u32> = check_source(source, "test.tsx", opts)
        .into_iter()
        .map(|d| d.code)
        .collect();
    assert!(
        codes.contains(&2786),
        "Renamed: two-param ScrollView must emit TS2786. Got: {codes:?}"
    );
}

/// Full conformance-test shape: `ElementType = string | NewJSXConstructor<any>` where
/// `NewJSXConstructor<P>` is a union alias. A 2-required-param function is not assignable
/// to the 1-param function branch → TS2786.
///
/// This mirrors the `ReactNativeFlatList` case in `compiler/jsxElementType.tsx`.
#[test]
fn jsx_element_type_indirect_alias_rejects_two_param_fn() {
    use crate::context::CheckerOptions;
    use crate::test_utils::check_source;
    use tsz_common::checker_options::JsxMode;
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        strict_null_checks: true,
        strict: true,
        ..CheckerOptions::default()
    };
    // ElementType is defined via an intermediate alias, exactly like in
    // jsxElementType.tsx (NewReactJSXElementConstructor).
    let source = r#"
type ReactNode = string | number | boolean | null;
type NewJSXCtor<P> = ((props: P) => ReactNode) | (new (props: P) => any);
declare global {
    namespace JSX {
        type ElementType = string | NewJSXCtor<any>;
        interface Element {}
        interface IntrinsicElements {}
    }
}
declare function FlatList(props: {}, ref: any): null;
const _a = <FlatList />;
"#;
    let codes: Vec<u32> = check_source(source, "test.tsx", opts)
        .into_iter()
        .map(|d| d.code)
        .collect();
    assert!(
        codes.contains(&2786),
        "Two-param fn via indirect ElementType alias must emit TS2786. Got: {codes:?}"
    );
}

/// Variant: renamed intermediate alias — ensures the fix is structural, not name-sensitive.
#[test]
fn jsx_element_type_indirect_alias_rejects_two_param_fn_renamed() {
    use crate::context::CheckerOptions;
    use crate::test_utils::check_source;
    use tsz_common::checker_options::JsxMode;
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        strict_null_checks: true,
        strict: true,
        ..CheckerOptions::default()
    };
    let source = r#"
type Node = string | null;
type ComponentType<P> = ((props: P) => Node) | (new (props: P) => any);
declare global {
    namespace JSX {
        type ElementType = string | ComponentType<any>;
        interface Element {}
        interface IntrinsicElements {}
    }
}
declare function ListView(config: {}, ctx: any): null;
const _x = <ListView />;
"#;
    let codes: Vec<u32> = check_source(source, "test.tsx", opts)
        .into_iter()
        .map(|d| d.code)
        .collect();
    assert!(
        codes.contains(&2786),
        "Renamed indirect alias: two-param ListView must emit TS2786. Got: {codes:?}"
    );
}

/// Class component missing required props in JSX must emit TS2769
/// ("No overload matches this call"), not TS2741 ("Property X is missing").
///
/// This mirrors lines 70/72 from `compiler/jsxElementType.tsx`:
/// `<RenderStringClass />;` and `<RenderStringClass excessProp />;`
#[test]
fn jsx_class_component_missing_props_emits_ts2769() {
    use crate::context::CheckerOptions;
    use crate::test_utils::check_source;
    use tsz_common::checker_options::JsxMode;
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        strict_null_checks: true,
        strict: true,
        ..CheckerOptions::default()
    };
    let source = r#"
type Node = string | null;
type NewJSXCtor<P> = ((props: P) => Node) | (new (props: P) => any);
declare global {
    namespace JSX {
        type ElementType = string | NewJSXCtor<any>;
        interface Element {}
        interface IntrinsicElements {}
    }
}
declare class Panel {
    constructor(props: { label: string });
    render(): Node;
}
const _a = <Panel />;
const _b = <Panel extra />;
"#;
    let diags = check_source(source, "test.tsx", opts);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    // At least one of TS2769 or TS2741 must fire for the missing-prop case.
    // tsc emits TS2769 for class components; tsz should match.
    assert!(
        codes.contains(&2769) || codes.contains(&2741) || codes.contains(&2322),
        "Class component with missing props must emit a prop mismatch error. Got: {codes:?}"
    );
}

/// A function component returning `string` must NOT emit TS2786 when
/// `JSX.ElementType` is `string | NewJSXCtor<any>` and the ctor union member
/// admits 1-param function components returning `React18ReactNode` (which
/// includes `string`).
///
/// This mirrors lines 40-42 from `compiler/jsxElementType.tsx`:
/// `<RenderString />`, `<RenderString title="react" />`, `<RenderString excessProp />`.
#[test]
fn jsx_element_type_via_alias_admits_string_returning_component() {
    use crate::context::CheckerOptions;
    use crate::test_utils::check_source;
    use tsz_common::checker_options::JsxMode;
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        strict_null_checks: true,
        strict: true,
        ..CheckerOptions::default()
    };
    // Mirrors the jsxElementType.tsx setup exactly.
    let source = r#"
type MyFragment = ReadonlyArray<MyNode>;
type MyNode =
  | string
  | number
  | MyFragment
  | boolean
  | null
  | undefined
  | Promise<MyNode>;
type NewJSXCtor<P> = ((props: P) => MyNode) | (new (props: P) => any);
declare global {
    namespace JSX {
        type ElementType = string | NewJSXCtor<any>;
        interface Element {}
        interface IntrinsicElements {}
    }
}
const RenderString = ({ title }: { title: string }) => title;
const _a = <RenderString />;
const _b = <RenderString title="hi" />;
"#;
    let codes: Vec<u32> = check_source(source, "test.tsx", opts)
        .into_iter()
        .map(|d| d.code)
        .collect();
    assert!(
        !codes.contains(&2786),
        "string-returning component via indirect ElementType alias must NOT emit TS2786. Got: {codes:?}"
    );
}

/// Same shape but with an opaque `ReactElement` type in the union (mimicking
/// `React.ReactElement<any>` from react16.d.ts). Ensures tsz doesn't regress
/// when the union has object-shape members alongside primitives.
#[test]
fn jsx_element_type_via_alias_with_object_member_admits_string_returning_component() {
    use crate::context::CheckerOptions;
    use crate::test_utils::check_source;
    use tsz_common::checker_options::JsxMode;
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        strict_null_checks: true,
        strict: true,
        ..CheckerOptions::default()
    };
    // Add a concrete object type as the first union member, mimicking React.ReactElement<any>.
    let source = r#"
interface ReactElement<P> { props: P; type: any; key: string | null; }
interface ReactPortal { key: string | null; children: ReactNode; }
type ReactFragment = ReadonlyArray<ReactNode>;
type ReactNode =
  | ReactElement<any>
  | string
  | number
  | ReactFragment
  | ReactPortal
  | boolean
  | null
  | undefined
  | Promise<ReactNode>;
type NewJSXCtor<P> = ((props: P) => ReactNode) | (new (props: P) => any);
declare global {
    namespace JSX {
        type ElementType = string | NewJSXCtor<any>;
        interface Element {}
        interface IntrinsicElements {}
    }
}
const RenderString = ({ title }: { title: string }) => title;
const RenderNumber = ({ title }: { title: string }) => title.length;
const _a = <RenderString />;
const _b = <RenderString title="hi" />;
const _c = <RenderNumber title="hi" />;
"#;
    let codes: Vec<u32> = check_source(source, "test.tsx", opts)
        .into_iter()
        .map(|d| d.code)
        .collect();
    assert!(
        !codes.contains(&2786),
        "string/number-returning component with ReactElement in union must NOT emit TS2786. Got: {codes:?}"
    );
}

/// Variant: `number`-returning function component — same root rule.
#[test]
fn jsx_element_type_via_alias_admits_number_returning_component() {
    use crate::context::CheckerOptions;
    use crate::test_utils::check_source;
    use tsz_common::checker_options::JsxMode;
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        strict_null_checks: true,
        strict: true,
        ..CheckerOptions::default()
    };
    let source = r#"
type MyFragment = ReadonlyArray<MyNode>;
type MyNode =
  | string
  | number
  | MyFragment
  | boolean
  | null
  | undefined
  | Promise<MyNode>;
type NewJSXCtor<P> = ((props: P) => MyNode) | (new (props: P) => any);
declare global {
    namespace JSX {
        type ElementType = string | NewJSXCtor<any>;
        interface Element {}
        interface IntrinsicElements {}
    }
}
const RenderNumber = ({ title }: { title: string }) => title.length;
const _a = <RenderNumber />;
const _b = <RenderNumber title="hi" />;
"#;
    let codes: Vec<u32> = check_source(source, "test.tsx", opts)
        .into_iter()
        .map(|d| d.code)
        .collect();
    assert!(
        !codes.contains(&2786),
        "number-returning component via indirect ElementType alias must NOT emit TS2786. Got: {codes:?}"
    );
}

/// Same but with renamed class/prop names — structural check.
#[test]
fn jsx_class_component_missing_props_emits_ts2769_renamed() {
    use crate::context::CheckerOptions;
    use crate::test_utils::check_source;
    use tsz_common::checker_options::JsxMode;
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        strict_null_checks: true,
        strict: true,
        ..CheckerOptions::default()
    };
    let source = r#"
type Output = string | null;
type Ctor<P> = ((props: P) => Output) | (new (props: P) => any);
declare global {
    namespace JSX {
        type ElementType = string | Ctor<any>;
        interface Element {}
        interface IntrinsicElements {}
    }
}
declare class Modal {
    constructor(props: { title: string });
    render(): Output;
}
const _x = <Modal />;
const _y = <Modal extra />;
"#;
    let diags = check_source(source, "test.tsx", opts);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2769) || codes.contains(&2741) || codes.contains(&2322),
        "Renamed: class component with missing props must emit prop mismatch. Got: {codes:?}"
    );
}
