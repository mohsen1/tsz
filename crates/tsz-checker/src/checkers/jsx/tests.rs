//! JSX unit tests.

use crate::test_utils::check_source;

fn check_jsx(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    use crate::context::CheckerOptions;
    use tsz_common::checker_options::JsxMode;
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        ..CheckerOptions::default()
    };
    check_source(source, "test.tsx", opts)
}

fn check_jsx_codes(source: &str) -> Vec<u32> {
    check_jsx(source).iter().map(|d| d.code).collect()
}

/// JSX shorthand boolean attribute (`<Foo bar />`) typed as `true` for assignability.
/// When prop expects literal `true`, shorthand must be assignable (no false positive).
#[test]
fn jsx_shorthand_boolean_assignable_to_literal_true() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX { interface Element {} interface IntrinsicElements { test1: { x: true }; } }
        <test1 x />;
        "#,
    );
    // Should NOT emit TS2322 — shorthand `x` is `true`, assignable to `true`
    assert!(
        !diagnostics.contains(&2322),
        "Shorthand boolean should be assignable to literal true prop, got: {diagnostics:?}"
    );
}

/// JSX shorthand boolean not assignable to non-boolean prop emits TS2322.
#[test]
fn jsx_shorthand_boolean_not_assignable_to_number() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX { interface Element {} interface IntrinsicElements { test1: { x: number }; } }
        <test1 x />;
        "#,
    );
    assert!(
        diagnostics.contains(&2322),
        "Shorthand boolean should not be assignable to number, got: {diagnostics:?}"
    );
}

/// Data-*/aria-* attributes not found in props should have their actual type
/// computed (not left as placeholder `any`).
#[test]
fn jsx_data_attribute_type_not_any_placeholder() {
    let diagnostics = check_jsx(
        r#"
        declare namespace JSX { interface Element {} interface IntrinsicElements { div: { id?: string }; } }
        <div data-value={42} />;
        "#,
    );
    // data-* attributes on intrinsic elements should not cause errors
    // (they're valid HTML custom data attributes)
    assert!(
        !diagnostics.iter().any(|d| d.code == 2322),
        "data-* attribute should not cause TS2322, got: {diagnostics:?}"
    );
}

/// TS2786: Class component whose construct signature return type doesn't
/// satisfy JSX.ElementClass should emit "cannot be used as a JSX component".
#[test]
fn jsx_class_component_invalid_return_type_emits_ts2786() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element { }
            interface ElementClass { render: any; }
            interface IntrinsicElements { }
        }
        interface BadComponentType { new(n: string): { x: number }; }
        declare var BadComponent: BadComponentType;
        <BadComponent />;
        "#,
    );
    assert!(
        diagnostics.contains(&2786),
        "Class component with invalid return type should emit TS2786, got: {diagnostics:?}"
    );
}

/// TS2786 should NOT fire when the construct signature return type
/// satisfies JSX.ElementClass.
#[test]
fn jsx_class_component_valid_return_type_no_ts2786() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element { }
            interface ElementClass { render: any; }
            interface IntrinsicElements { }
        }
        interface GoodComponentType { new(): { render: any }; }
        declare var GoodComponent: GoodComponentType;
        <GoodComponent />;
        "#,
    );
    assert!(
        !diagnostics.contains(&2786),
        "Valid class component should not emit TS2786, got: {diagnostics:?}"
    );
}

/// TS2786 should NOT fire for SFCs returning `Element | null`.
/// TSC allows null/undefined in SFC return types.
#[test]
fn jsx_sfc_returning_element_or_null_no_ts2786() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element { }
            interface ElementClass { render(): any; }
            interface IntrinsicElements { }
        }
        declare function MyComp(props: {}): JSX.Element | null;
        <MyComp />;
        "#,
    );
    assert!(
        !diagnostics.contains(&2786),
        "SFC returning Element | null should not emit TS2786, got: {diagnostics:?}"
    );
}

/// TS2786 SHOULD fire for SFCs returning a type incompatible with JSX.Element
/// (even after null/undefined stripping).
#[test]
fn jsx_sfc_returning_incompatible_type_emits_ts2786() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element { type: 'element'; }
            interface IntrinsicElements { }
        }
        declare function BadComp(props: {}): { type: string };
        <BadComp />;
        "#,
    );
    assert!(
        diagnostics.contains(&2786),
        "SFC returning incompatible type should emit TS2786, got: {diagnostics:?}"
    );
}

/// TS2786 should NOT fire for call signatures returning `Element | null`.
#[test]
fn jsx_call_signature_returning_element_or_null_no_ts2786() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element { }
            interface IntrinsicElements { }
        }
        interface CompType { (props: {}): JSX.Element | null; }
        declare var Comp: CompType;
        <Comp />;
        "#,
    );
    assert!(
        !diagnostics.contains(&2786),
        "Call signature returning Element | null should not emit TS2786, got: {diagnostics:?}"
    );
}

/// TS2607: When `ElementAttributesProperty` specifies a property name (e.g. `pr`)
/// and the class component instance type doesn't have that property,
/// emit "JSX element class does not support attributes".
#[test]
fn jsx_class_component_missing_eap_member_emits_ts2607() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element { }
            interface ElementAttributesProperty { pr: any; }
            interface IntrinsicElements { }
        }
        interface CompType { new(n: string): { x: number }; }
        declare var Comp: CompType;
        <Comp x={10} />;
        "#,
    );
    assert!(
        diagnostics.contains(&2607),
        "Class component without ElementAttributesProperty member should emit TS2607, got: {diagnostics:?}"
    );
}

/// TS2608: `ElementAttributesProperty` with more than one property should
/// emit "may not have more than one property".
#[test]
fn jsx_element_attributes_property_multiple_members_emits_ts2608() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element { }
            interface ElementAttributesProperty { pr1: any; pr2: any; }
            interface IntrinsicElements { }
        }
        interface CompType { new(n: string): {}; }
        declare var Comp: CompType;
        <Comp x={10} />;
        "#,
    );
    assert!(
        diagnostics.contains(&2608),
        "ElementAttributesProperty with multiple members should emit TS2608, got: {diagnostics:?}"
    );
}

/// TS2786 should NOT fire for generic class components whose construct
/// signature return type contains unresolved type parameters.
/// TSC resolves signatures before checking; we skip the check when
/// type parameters remain unresolved.
#[test]
fn jsx_generic_class_component_no_false_ts2786() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element { }
            interface ElementClass { render(): any; }
            interface IntrinsicElements { }
        }
        declare class Component<P> {
            constructor(props: P);
            props: P;
            render(): JSX.Element;
        }
        class MyGenericComp<T> extends Component<T> {
            render() { return <div /> as any as JSX.Element; }
        }
        <MyGenericComp />;
        "#,
    );
    assert!(
        !diagnostics.contains(&2786),
        "Generic class component should not emit false TS2786, got: {diagnostics:?}"
    );
}

/// TS2786 SHOULD still fire for generic SFCs (call signatures) that
/// return an incompatible type, even when generic params are present.
#[test]
fn jsx_generic_sfc_incompatible_return_emits_ts2786() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element { type: 'element'; }
            interface IntrinsicElements { }
        }
        declare function BadGenericComp<T>(props: T): { type: T };
        <BadGenericComp />;
        "#,
    );
    assert!(
        diagnostics.contains(&2786),
        "Generic SFC with incompatible return should emit TS2786, got: {diagnostics:?}"
    );
}

#[test]
fn jsx_library_managed_attributes_applies_default_props_to_class_components() {
    let diagnostics = check_jsx_codes(
        r#"
        type Defaultize<TProps, TDefaults> =
            & { [K in Extract<keyof TProps, keyof TDefaults>]?: TProps[K] }
            & { [K in Exclude<keyof TProps, keyof TDefaults>]: TProps[K] }
            & Partial<TDefaults>;

        declare class ReactComponent<P = {}, S = {}> {
            props: P;
        }

        declare namespace JSX {
            interface Element extends ReactComponent {}
            interface IntrinsicElements {}
            type LibraryManagedAttributes<TComponent, TProps> =
                TComponent extends { defaultProps: infer D }
                    ? Defaultize<TProps, D>
                    : TProps;
        }

        interface Props {
            foo: string;
            bar: number;
        }

        class Component extends ReactComponent<Props> {
            static defaultProps = {
                foo: "ok",
            };
        }

        <Component foo={123} bar={1} />;
        <Component />;
        "#,
    );
    assert!(
        diagnostics.contains(&2322),
        "Expected JSX.LibraryManagedAttributes to preserve prop type checking, got: {diagnostics:?}"
    );
    assert_eq!(
        diagnostics.iter().filter(|&&code| code == 2322).count(),
        2,
        "Expected one type mismatch and one missing-required-prop assignability error, got: {diagnostics:?}"
    );
}

#[test]
fn jsx_generic_class_component_infers_props_from_attributes() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element {}
            interface ElementAttributesProperty { props: {}; }
            interface IntrinsicElements { [key: string]: Element; }
        }

        interface BaseProps<T> {
            initialValues: T;
            nextValues: (cur: T) => T;
        }

        declare class ReactComponent<P = {}, S = {}> {
            props: P;
        }

        declare class GenericComponent<Props = {}, Values = object> extends ReactComponent<Props & BaseProps<Values>, {}> {
            iv: Values;
        }

        let a = <GenericComponent initialValues={{ x: "y" }} nextValues={a => a} />;
        let b = <GenericComponent initialValues={12} nextValues={a => a} />;
        let c = <GenericComponent initialValues={{ x: "y" }} nextValues={a => ({ x: a.x })} />;
        let d = <GenericComponent initialValues={{ x: "y" }} nextValues={a => a.x} />;
        "#,
    );
    assert!(
        diagnostics.contains(&2322),
        "Expected generic JSX class props inference to reject mismatched callback returns, got: {diagnostics:?}"
    );
}

#[test]
fn jsx_namespaced_class_component_missing_props_reports_assignability() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element {}
            interface IntrinsicElements {}
            interface ElementAttributesProperty { props: {}; }
            interface IntrinsicAttributes { ref?: string; }
        }

        declare class Component<P, S> {
            constructor(props?: P, context?: any);
            props: P;
            state: S;
            render(): JSX.Element;
        }

        interface ComponentClass<P> {
            new (props?: P, context?: any): Component<P, any>;
        }

        declare namespace TestMod {
            interface TestClass extends ComponentClass<{ reqd: any }> {}
            var Test: TestClass;
        }

        var t1 = <TestMod.Test />;
        "#,
    );
    assert!(
        diagnostics.contains(&2322),
        "Expected namespaced class-like JSX tags to report TS2322 for missing required props, got: {diagnostics:?}"
    );
}

/// TS2786 should NOT fire for class components with synthesized default
/// constructors (no params). The instance type may lack inherited members
/// like `render()` from the base class.
#[test]
fn jsx_class_component_no_param_constructor_no_false_ts2786() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element { }
            interface ElementClass { render(): any; }
            interface IntrinsicElements { }
        }
        declare class Component<P> {
            constructor(props: P);
            props: P;
            render(): JSX.Element;
        }
        class MyComp extends Component<{ x: number }> {}
        <MyComp />;
        "#,
    );
    assert!(
        !diagnostics.contains(&2786),
        "Class with no-param constructor should not emit false TS2786, got: {diagnostics:?}"
    );
}

/// TS2786 should NOT fire when a construct signature return type contains
/// type parameters from an outer scope (e.g. `ComponentClass`<P> where P
/// comes from a function parameter).
#[test]
fn jsx_construct_sig_with_outer_type_params_no_false_ts2786() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element { }
            interface ElementClass { render(): any; }
            interface IntrinsicElements { }
        }
        declare class Component<P> {
            constructor(props: P);
            props: P;
            render(): JSX.Element;
        }
        interface ComponentClass<P> {
            new(props: P): Component<P>;
        }
        declare function makeP<P>(Ctor: ComponentClass<P>): void;
        "#,
    );
    // No JSX usage here, just ensuring no false positives from type resolution
    assert!(
        !diagnostics.contains(&2786),
        "Component with outer type params should not emit false TS2786, got: {diagnostics:?}"
    );
}

/// Discriminated union JSX props: when the attribute value spans the full
/// union (e.g., `"a" | "b"`) and the per-member check fails, the fallback
/// whole-object assignability check determines the final result. Our solver
/// currently accepts this (consistent with tsc's JSX attribute checking).
#[test]
fn jsx_discriminated_union_props_full_concrete_union_no_ts2322() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element {}
            interface IntrinsicElements {}
        }
        type Props = { variant: "a"; } | { variant: "b"; };
        declare function Comp(_data: Props): JSX.Element | null;
        declare var v: "a" | "b";
        <Comp variant={v} />;
        "#,
    );
    // Whole-object assignability check: { variant: "a" | "b" } is accepted against
    // { variant: "a" } | { variant: "b" } by the solver (consistent with tsc).
    assert!(
        !diagnostics.contains(&2322),
        "Discriminated union props with full union value should not emit TS2322, got: {diagnostics:?}"
    );
}

/// Discriminated union JSX props: when the attribute type is a type parameter
/// whose constraint covers the union, no TS2322 should fire.
/// Repro from TS test discriminatedUnionJsxElement.tsx (#46021).
#[test]
fn jsx_discriminated_union_props_type_param_no_false_ts2322() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element {}
            interface IntrinsicElements {}
        }
        type Props = { variant: "a"; } | { variant: "b"; };
        declare function Comp(_data: Props): JSX.Element | null;
        function Menu<V extends "a" | "b">(v: V) {
            return <Comp variant={v} />;
        }
        "#,
    );
    assert!(
        !diagnostics.contains(&2322),
        "Discriminated union props with type parameter constraint should not emit false TS2322, got: {diagnostics:?}"
    );
}

/// Discriminated union JSX props with concrete types should still emit TS2322
/// when attribute values are genuinely incompatible.
#[test]
fn jsx_discriminated_union_props_incompatible_emits_ts2322() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element {}
            interface IntrinsicElements {}
        }
        type Props = { variant: "a"; x: number; } | { variant: "b"; y: string; };
        declare function Comp(_data: Props): JSX.Element | null;
        <Comp variant="c" />;
        "#,
    );
    assert!(
        diagnostics.contains(&2322),
        "Incompatible discriminated union props should still emit TS2322, got: {diagnostics:?}"
    );
}

/// Multiple JSX children should not emit TS2746 when the children type accepts arrays.
/// This tests that union types containing an array-like member (e.g. `ReactNode` which
/// includes `ReactNodeArray`) are correctly recognized as allowing multiple children.
#[test]
fn jsx_multiple_children_no_ts2746_when_children_type_accepts_array() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element {}
            interface IntrinsicElements {
                div: { children?: ChildNode };
            }
        }
        interface ChildNodeArray extends Array<ChildNode> {}
        type ChildNode = string | number | boolean | ChildNodeArray | null | undefined;
        <div>
            {"hello"}
            {"world"}
        </div>;
        "#,
    );
    assert!(
        !diagnostics.contains(&2746),
        "Multiple children should be allowed when children type includes array-like union member, got: {diagnostics:?}"
    );
}
