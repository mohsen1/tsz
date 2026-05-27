#[test]
fn jsx_library_managed_attributes_function_variable_display_uses_param_props() {
    let diagnostics = check_jsx_strict(
        r#"
        type MergePropTypes<P, T> = P;
        type Defaultize<P, D> = P;
        declare namespace PropTypes {
            type InferProps<T> = any;
        }
        declare namespace JSX {
            interface Element {}
            interface IntrinsicElements { div: {}; }
            interface IntrinsicAttributes {}
            type LibraryManagedAttributes<C, P> =
                C extends { propTypes: infer T; defaultProps: infer D; }
                    ? Defaultize<MergePropTypes<P, PropTypes.InferProps<T>>, D>
                    : C extends { propTypes: infer T; }
                        ? MergePropTypes<P, PropTypes.InferProps<T>>
                        : C extends { defaultProps: infer D; }
                            ? Defaultize<P, D>
                            : P;
        }

        const RenderTitle = ({ title }: { title: string }) => <div />;
        <RenderTitle />;
        <RenderTitle excessProp />;
        "#,
    );
    let missing = diagnostics
        .iter()
        .find(|diag| diag.code == 2741)
        .expect("expected missing title diagnostic");
    assert!(
        missing
            .message_text
            .contains("required in type '{ title: string; }'"),
        "expected TS2741 to display the function parameter props type, got: {missing:?}"
    );
    let excess = diagnostics
        .iter()
        .find(|diag| diag.code == 2322)
        .expect("expected excess prop diagnostic");
    assert!(
        excess
            .message_text
            .contains("IntrinsicAttributes & { title: string; }"),
        "expected TS2322 to display the function parameter props type, got: {excess:?}"
    );
    assert!(
        !missing.message_text.contains("propTypes: infer T")
            && !excess.message_text.contains("propTypes: infer T"),
        "JSX diagnostics should not display the full LibraryManagedAttributes conditional: {diagnostics:?}"
    );
}

#[test]
fn jsx_element_type_lma_without_metadata_uses_raw_function_props_for_excess() {
    let diagnostics = check_jsx_strict(
        r#"
        type MergePropTypes<P, T> = P;
        type Defaultize<P, D> = P;
        declare namespace PropTypes {
            type InferProps<T> = any;
        }
        type CustomElementConstructor<P> =
            | ((props: P) => JSX.Element | string)
            | (new (props: P) => { render(): JSX.Element | string });
        declare namespace JSX {
            interface Element {}
            interface IntrinsicElements { div: {}; }
            interface IntrinsicAttributes {}
            type ElementType = string | CustomElementConstructor<any>;
            type LibraryManagedAttributes<C, P> =
                C extends { propTypes: infer T; defaultProps: infer D; }
                    ? Defaultize<MergePropTypes<P, PropTypes.InferProps<T>>, D>
                    : C extends { propTypes: infer T; }
                        ? MergePropTypes<P, PropTypes.InferProps<T>>
                        : C extends { defaultProps: infer D; }
                            ? Defaultize<P, D>
                            : P;
        }

        const Caption = ({ label }: { label: string }) => label;
        <Caption spare />;
        "#,
    );
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected one excess-prop TS2322, got: {diagnostics:?}"
    );
    let message = &ts2322[0].message_text;
    assert!(
        message.contains("IntrinsicAttributes & { label: string; }"),
        "Expected TS2322 to target raw function props, got: {ts2322:?}"
    );
    assert!(
        !message.contains("propTypes: infer") && !message.contains("CustomElementConstructor"),
        "TS2322 should not expose unresolved LMA or ElementType constructor text, got: {ts2322:?}"
    );
}

/// Reproduces conformance test `compiler/ignoredJsxAttributes.tsx`. When a
/// function component has no `propTypes`/`defaultProps`, TS2741's target-type
/// display must use the props alias (`Props`), not the unevaluated
/// `LibraryManagedAttributes<C, P>` conditional that contains `propTypes: infer T`.
#[test]
fn jsx_library_managed_attributes_function_component_with_index_signature_props_displays_alias() {
    let diagnostics = check_jsx_strict(
        r#"
        type Pick<T, K extends keyof T> = { [P in K]: T[P]; };
        type Exclude<T, U> = T extends U ? never : T;
        type Extract<T, U> = T extends U ? T : never;
        type Partial<T> = { [P in keyof T]?: T[P]; };

        type MergePropTypes<P, T> = P & Pick<T, Exclude<keyof T, keyof P>>;
        type Defaultize<P, D> = P extends any
            ? string extends keyof P
                ? P
                : Pick<P, Exclude<keyof P, keyof D>>
                    & Partial<Pick<P, Extract<keyof P, keyof D>>>
                    & Partial<Pick<D, Exclude<keyof D, keyof P>>>
            : never;
        declare namespace PropTypes {
            type InferProps<T> = any;
        }

        declare namespace JSX {
            interface Element {}
            interface IntrinsicElements {}
            interface IntrinsicAttributes {}
            type LibraryManagedAttributes<C, P> =
                C extends { propTypes: infer T; defaultProps: infer D; }
                    ? Defaultize<MergePropTypes<P, PropTypes.InferProps<T>>, D>
                    : C extends { propTypes: infer T; }
                        ? MergePropTypes<P, PropTypes.InferProps<T>>
                        : C extends { defaultProps: infer D; }
                            ? Defaultize<P, D>
                            : P;
        }

        interface Props {
            foo: string;
            [dataProp: string]: string;
        }
        declare function Yadda(props: Props): JSX.Element;
        let x = <Yadda bar="hello" data-yadda={42}/>;
        "#,
    );
    let ts2741 = diagnostics
        .iter()
        .find(|diag| diag.code == 2741)
        .expect("expected TS2741 for missing required prop");
    assert!(
        !ts2741.message_text.contains("propTypes: infer"),
        "TS2741 must not display the unevaluated LibraryManagedAttributes conditional, got: {ts2741:?}"
    );
    assert!(
        ts2741.message_text.contains("required in type 'Props'"),
        "TS2741 should display the named props alias 'Props', got: {ts2741:?}"
    );
}

/// Generic component parameters keep the raw
/// `LibraryManagedAttributes<T, P>` display. When the raw component is a type
/// parameter constrained to a function component, `P` is the first parameter
/// type (`{}` here), not the constraint function type itself.
#[test]
fn jsx_generic_component_parameter_lma_display_uses_props_parameter() {
    let diagnostics = check_jsx_strict(
        r#"
        declare namespace JSX {
            interface Element {}
            interface IntrinsicElements {}
            interface IntrinsicAttributes {}
            type LibraryManagedAttributes<C, P> =
                C extends { propTypes: infer T; defaultProps: infer D; }
                    ? P
                    : C extends { propTypes: infer T; }
                        ? P
                        : C extends { defaultProps: infer D; }
                            ? P
                            : P;
        }

        function f1<T extends (props: {}) => JSX.Element>(Component: T) {
            return <Component />;
        }
        "#,
    );
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2322)
        .expect("expected TS2322 for generic LibraryManagedAttributes target");
    assert!(
        diag.message_text
            .contains("LibraryManagedAttributes<T, {}>")
            && !diag.message_text.contains("(props: {})"),
        "Expected raw LMA display to preserve props parameter, got: {diag:?}"
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

#[test]
fn jsx_ref_attributes_use_intrinsic_class_attribute_context() {
    let source = r#"
        declare namespace React {
            type Key = string | number;
            type Ref<T> = string | ((instance: T) => any);

            interface Attributes {
                key?: Key;
            }

            interface ClassAttributes<T> extends Attributes {
                ref?: Ref<T>;
            }

            class Component<P, S> {
                props: P;
                state: S;
                render(): JSX.Element | null;
            }
        }

        declare namespace JSX {
            interface Element {}
            interface IntrinsicAttributes extends React.Attributes {}
            interface IntrinsicClassAttributes<T> extends React.ClassAttributes<T> {}
            interface IntrinsicElements {
                div: React.ClassAttributes<HTMLDivElement> & {};
            }
        }

        interface HTMLDivElement {
            innerText: string;
        }

        function Greet(_props: { name?: string }) {
            return <div />;
        }

        class BigGreeter extends React.Component<{ name?: string }, {}> {
            greeting: string;
            render(): JSX.Element { return <div />; }
        }

        <Greet ref="myRef" />;
        <BigGreeter ref={x => x.greeting.subtr(10)} />;
        <BigGreeter ref={x => x.notARealProperty} />;
        <div ref={x => x.propertyNotOnHtmlDivElement} />;
        "#;
    let diagnostics = check_jsx(source);
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    let sfc_ref_start = source
        .find("ref=\"myRef\"")
        .expect("expected SFC ref attribute in source") as u32;
    let sfc_ref_diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected JSX SFC ref TS2322 diagnostic");
    assert_eq!(
        sfc_ref_diag.start, sfc_ref_start,
        "Expected SFC ref diagnostic to anchor at the ref attribute, got: {diagnostics:?}"
    );
    assert!(
        codes.contains(&2322),
        "Expected ref on SFC to be rejected, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|d| {
            d.message_text.contains("notARealProperty")
                && d.message_text.contains("type 'BigGreeter'")
        }),
        "Expected class ref callback to be contextually typed as BigGreeter, got: {diagnostics:?}"
    );
    assert!(
        codes.contains(&2339),
        "Expected contextually typed property errors for ref callbacks, got: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&2812),
        "Expected real DOM/property diagnostics, not missing-lib TS2812, got: {diagnostics:?}"
    );
}

#[test]
fn non_dom_named_local_interface_missing_property_is_not_ts2812() {
    let diagnostics = check_source_diagnostics(
        r#"
        interface HTMLElementFake {}
        declare const el: HTMLElementFake;
        el.propertyNotOnHtmlDivElement;
        "#,
    );
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2339),
        "Expected ordinary missing-property TS2339, got: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&2812),
        "Expected user-defined DOM-like names not to trigger TS2812, got: {diagnostics:?}"
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

/// Class component union-props TS2322 should render the synthesized JSX
/// attributes object as the source, not the component constructor type.
///
/// Conformance test: `tsxSpreadAttributesResolution6.tsx`.
#[test]
fn jsx_class_union_props_ts2322_uses_attrs_source_display() {
    let diagnostics = check_jsx(
        r#"
        declare namespace JSX {
            interface Element {}
            interface IntrinsicAttributes {}
            interface IntrinsicClassAttributes<T> {}
            interface ElementChildrenAttribute { children: {}; }
            interface IntrinsicElements {}
        }
        type ReactNode = string | number | null | undefined;
        declare class Component<P, S> {
            constructor(props: P);
            render(): any;
            props: P & { children?: ReactNode };
            state: S;
        }

        type TextProps = { editable: false }
                       | { editable: true, onEdit: (newText: string) => void };

        class TextComponent extends Component<TextProps, {}> {
            render() { return null; }
        }

        let x = <TextComponent editable={true} />;
        "#,
    );
    let ts2322 = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .unwrap_or_else(|| panic!("expected TS2322, got: {diagnostics:?}"));
    assert!(
        ts2322
            .message_text
            .contains("Type '{ editable: true; }' is not assignable to type"),
        "TS2322 should use the JSX attributes object as source, got: {ts2322:?}"
    );
    assert!(
        !ts2322.message_text.contains("Type 'typeof TextComponent'"),
        "TS2322 must not use the component constructor as source, got: {ts2322:?}"
    );
    assert!(
        ts2322.message_text.contains("IntrinsicAttributes")
            && ts2322
                .message_text
                .contains("IntrinsicClassAttributes<TextComponent>")
            && ts2322.message_text.contains("TextProps"),
        "TS2322 should render the full JSX component target, got: {ts2322:?}"
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

#[test]
fn jsx_multiple_children_readonly_mapped_wrapper_uses_shape_not_alias_name() {
    let diagnostics = check_jsx_codes(
        r#"
        type Frozen<T> = { readonly [Slot in keyof T]: T[Slot] };
        type Renderable = string | number | Element;
        declare namespace JSX {
            interface Element {}
            interface ElementChildrenAttribute { children: {}; }
            interface IntrinsicElements { span: {}; }
        }
        declare class ComponentBase<P = {}> {
            props: P & Frozen<{ children: Renderable[] }>;
            render(): Renderable;
        }
        class Panel extends ComponentBase {}
        <Panel><span /><span /></Panel>;
        "#,
    );
    assert!(
        !diagnostics.contains(&2746),
        "Multiple children should use the structurally readonly mapped wrapper member, got: {diagnostics:?}"
    );
}

/// Intra-expression JSX generic inference: when all attributes are function-valued
/// (no concrete attrs), bootstrap inference from attrs whose contextual parameter
/// types are concrete (don't depend on type params being inferred).
#[test]
fn jsx_intra_expression_inference_all_function_valued_attrs() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element {}
            interface IntrinsicElements { div: {}; }
        }

        interface Props<T> {
            a: (x: string) => T;
            b: (arg: T) => void;
        }

        function Foo<T>(props: Props<T>) {
            return <div />;
        }

        <Foo a={() => 10} b={(arg) => { arg.toString(); }} />;
        <Foo a={(x) => 10} b={(arg) => { arg.toString(); }} />;
        "#,
    );
    let ts18046_count = diagnostics.iter().filter(|&&d| d == 18046).count();
    assert_eq!(
        ts18046_count, 0,
        "Expected no TS18046 for intra-expression JSX inference, got: {diagnostics:?}"
    );
}

// Regression test for #3734. The LMA conditional `C extends { defaultProps:
// infer D } ? Defaultize<P, D> : P` substitutes `D = { text: string }` from
// the class's `defaultProps` correctly, but the user-defined `Defaultize`
// helper expands to an intersection of `Pick<…, Exclude<keyof P, keyof D>>` /
// `Partial<…>` arms whose `Pick`/`Exclude`/`Extract` applications cannot
// reduce further on their own. The intersection therefore lacks an object
// shape, and prior to the fix the JSX assignability check compared the empty
// JSX attribute object against the still-required `text` property. The
// `apply_jsx_library_managed_attributes` fallback now triggers on
// "evaluated has no object shape AND defaultProps metadata is present",
// reusing the same default-props-become-optional transform that already
// handles `evaluated == any`.
#[test]
fn jsx_type_predicate_default_props_no_false_ts2322() {
    let diagnostics = check_jsx_codes(
        r#"
        type Defaultize<P, D> = P extends any
            ? string extends keyof P ? P :
            & Pick<P, Exclude<keyof P, keyof D>>
            & Partial<Pick<P, Extract<keyof P, keyof D>>>
            & Partial<Pick<D, Exclude<keyof D, keyof P>>>
            : never;

        declare class ReactComponent<P = {}, S = {}> {
            props: P;
        }

        declare namespace JSX {
            interface Element extends ReactComponent {}
            interface IntrinsicElements {}
            interface ElementAttributesProperty { props: {}; }
            type LibraryManagedAttributes<C, P> = C extends { defaultProps: infer D; }
                ? Defaultize<P, D>
                : P;
        }

        class SimpleComp extends ReactComponent<{ text: string }> {
            static defaultProps = { text: "hello" }
        }

        const Render = () => <SimpleComp />;
        "#,
    );
    assert!(
        !diagnostics.contains(&2322),
        "Expected no TS2322 for component with defaultProps (React Defaultize), got: {diagnostics:?}"
    );
}

/// Regression for #3734: same Defaultize/LMA pattern but with the bound type
/// parameter renamed (`Q`/`E` instead of `P`/`D`) and the iteration variable
/// in `Pick<...>` renamed. The fix must be structural — driven by the absence
/// of a derivable object shape on the LMA evaluation result, not by the
/// alias's name or the user-chosen type-parameter spellings.
#[test]
fn jsx_defaultize_with_alternate_param_names_no_false_ts2322() {
    let diagnostics = check_jsx_codes(
        r#"
        type MyDefaults<Q, E> = Q extends any
            ? string extends keyof Q ? Q :
            & Pick<Q, Exclude<keyof Q, keyof E>>
            & Partial<Pick<Q, Extract<keyof Q, keyof E>>>
            & Partial<Pick<E, Exclude<keyof E, keyof Q>>>
            : never;

        declare class ReactComponent<P = {}, S = {}> {
            props: P;
        }

        declare namespace JSX {
            interface Element extends ReactComponent {}
            interface IntrinsicElements {}
            interface ElementAttributesProperty { props: {}; }
            type LibraryManagedAttributes<C, P> = C extends { defaultProps: infer D; }
                ? MyDefaults<P, D>
                : P;
        }

        class WidgetComp extends ReactComponent<{ label: string }> {
            static defaultProps = { label: "default" }
        }

        const Render = () => <WidgetComp />;
        "#,
    );
    assert!(
        !diagnostics.contains(&2322),
        "Expected no TS2322 for renamed-helper Defaultize (#3734 structural rule), got: {diagnostics:?}"
    );
}

/// Regression for #3734: when the component has NO defaultProps, the LMA
/// fallback must NOT fire — passing `<NoDefaults />` without the required
/// `count` prop must still emit TS2741 (missing required prop). This locks
/// the trigger condition to "`default_props_type` is Some".
#[test]
fn jsx_no_default_props_still_emits_required_prop_diagnostic() {
    let diagnostics = check_jsx_codes(
        r#"
        type Defaultize<P, D> = P extends any
            ? string extends keyof P ? P :
            & Pick<P, Exclude<keyof P, keyof D>>
            & Partial<Pick<P, Extract<keyof P, keyof D>>>
            & Partial<Pick<D, Exclude<keyof D, keyof P>>>
            : never;

        declare class ReactComponent<P = {}, S = {}> {
            props: P;
        }

        declare namespace JSX {
            interface Element extends ReactComponent {}
            interface IntrinsicElements {}
            interface ElementAttributesProperty { props: {}; }
            type LibraryManagedAttributes<C, P> = C extends { defaultProps: infer D; }
                ? Defaultize<P, D>
                : P;
        }

        class NoDefaults extends ReactComponent<{ count: number }> {}

        const Render = () => <NoDefaults />;
        "#,
    );
    // Either TS2741 (missing required prop) or TS2322 (assignability) is fine
    // — the point is that the LMA fallback must NOT swallow the missing-prop
    // error when the component has no defaultProps.
    assert!(
        diagnostics.contains(&2741) || diagnostics.contains(&2322),
        "Expected TS2741 or TS2322 when component has no defaultProps, got: {diagnostics:?}"
    );
}

/// JSX spread that overrides an EARLIER explicit attribute with a mismatched
/// type emits TS2322 anchored at the explicit attribute's name (matching tsc
/// at the same anchor as TS2783), with the per-property message
/// ("Type 'X' is not assignable to type 'Y'") rather than the whole-type
/// message at the JSX tag name.
///
/// Repro from `TypeScript/tests/cases/conformance/jsx/tsxAttributeResolution3.tsx`:
/// ```tsx
/// var obj5 = { x: 32, y: 32 };
/// <test1 x="ok" {...obj5} />
/// ```
/// tsc emits:
///   TS2783 at `x` of `x="ok"` ('x' is specified more than once...)
///   TS2322 at `x` of `x="ok"` (Type 'number' is not assignable to type 'string'.)
#[test]
fn jsx_spread_overrides_earlier_attr_anchors_per_property_ts2322_at_attr() {
    let source = concat!(
        "declare namespace JSX {\n",
        "  interface Element {}\n",
        "  interface IntrinsicElements { test1: { x: string }; }\n",
        "}\n",
        "var obj5 = { x: 32 };\n",
        "<test1 x=\"ok\" {...obj5} />;\n",
    );
    let diagnostics = check_jsx(source);
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2322),
        "Expected TS2322 for spread overriding earlier attr with mismatched type, got: {codes:?}",
    );
    assert!(
        codes.contains(&2783),
        "Expected TS2783 for spread overriding explicit attr, got: {codes:?}",
    );

    let ts2322 = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("TS2322 must be present");
    // Per-property message, not whole-type ("Type 'X' is not assignable to type 'Y'").
    assert!(
        ts2322.message_text.contains("'number'") && ts2322.message_text.contains("'string'"),
        "Expected per-property TS2322 message about number→string, got: {}",
        ts2322.message_text
    );
    // Should NOT include the whole-type message (the synthesized object type).
    assert!(
        !ts2322.message_text.contains("{ x: number"),
        "Expected per-property message, not whole-type message, got: {}",
        ts2322.message_text
    );

    // Anchor parity: TS2322 and TS2783 should share the same anchor (the `x` of `x="ok"`).
    let ts2783 = diagnostics
        .iter()
        .find(|d| d.code == 2783)
        .expect("TS2783 must be present");
    assert_eq!(
        ts2322.start, ts2783.start,
        "TS2322 must share TS2783's anchor at the earlier explicit attribute name",
    );
}

/// When a spread overrides an EARLIER explicit attribute but the spread's
/// property TYPE matches the expected, only TS2783 is emitted — no TS2322.
///
/// Repro: `<test1 x={32} {...{ x: 'foo' }} />` against `{ x: string }`.
#[test]
fn jsx_spread_overrides_earlier_attr_with_matching_type_no_ts2322() {
    let source = concat!(
        "declare namespace JSX {\n",
        "  interface Element {}\n",
        "  interface IntrinsicElements { test1: { x: string }; }\n",
        "}\n",
        "var obj7 = { x: \"foo\" };\n",
        "<test1 x={32} {...obj7} />;\n",
    );
    let diagnostics = check_jsx(source);
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2322),
        "Expected NO TS2322 when spread's prop type matches expected (TS2783 only), got: {codes:?}",
    );
    assert!(
        codes.contains(&2783),
        "Expected TS2783 for spread overriding explicit attr, got: {codes:?}",
    );
}

/// JSX class component with optional constructor parameter must still report
/// missing required props when the type param only has a constraint (no default).
#[test]
fn jsx_generic_class_optional_ctor_constraint_reports_errors() {
    let source = concat!(
        "declare namespace JSX {\n",
        "  interface Element {}\n",
        "  interface ElementAttributesProperty { props: {}; }\n",
        "  interface IntrinsicElements {}\n",
        "}\n",
        "declare class Component<P, S> {\n",
        "  constructor(props?: P, context?: any);\n",
        "  props: P;\n",
        "  state: S;\n",
        "  render(): JSX.Element;\n",
        "}\n",
        "interface Prop { a: number; b: string; }\n",
        "declare class MyComp<P extends Prop> extends Component<P, {}> {\n",
        "  internalProp: P;\n",
        "}\n",
        "let x1 = <MyComp />;\n",
    );
    let diagnostics = check_jsx(source);
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2739) || codes.contains(&2322),
        "Expected TS2739 or TS2322 for constraint-only generic with optional ctor, got: {codes:?}",
    );
}

#[test]
fn jsx_library_managed_attributes_reports_excess_props_with_metadata() {
    let diagnostics = check_jsx_strict(
        r#"
        type Exclude<T, U> = T extends U ? never : T;
        type Extract<T, U> = T extends U ? T : never;
        type Partial<T> = { [K in keyof T]?: T[K] };
        type Readonly<T> = { readonly [K in keyof T]: T[K] };
        type Defaultize<TProps, TDefaults> =
            & { [K in Extract<keyof TProps, keyof TDefaults>]?: TProps[K] }
            & { [K in Exclude<keyof TProps, keyof TDefaults>]: TProps[K] }
            & Partial<TDefaults>;
        type Inferred<S> = {
            [K in keyof S]: S[K] extends Checker<infer V, infer R>
                ? Checker<V, R>[typeof marker]
                : {}
        };

        declare const marker: unique symbol;
        interface Checker<V, Required = false> {
            isRequired: Checker<V, true>;
            [marker]: Required extends true ? V : V | null | undefined;
        }
        declare namespace Kinds {
            const number: Checker<number>;
            const renderable: Checker<Renderable>;
        }
        type Renderable = string | number | ComponentBase<{}, {}>;

        declare class ComponentBase<P = {}, S = {}> {
            constructor(props: P);
            props: P & Readonly<{ children: Renderable[] }>;
            setState(s: Partial<S>): S;
            render(): Renderable;
        }
        declare namespace JSX {
            interface Element extends ComponentBase {}
            interface IntrinsicElements {}
            type LibraryManagedAttributes<C, P> =
                C extends { defaultProps: infer D; propTypes: infer T; }
                    ? Defaultize<P & Inferred<T>, D>
                    : C extends { defaultProps: infer D }
                        ? Defaultize<P, D>
                        : C extends { propTypes: infer T }
                            ? P & Inferred<T>
                            : P;
        }

        class WithBoth extends ComponentBase {
            static propTypes = {
                label: Kinds.renderable.isRequired,
                count: Kinds.number,
            };
            static defaultProps = { count: 1 };
        }
        <WithBoth label="ok" extra="bad" />;

        class WithDefaults extends ComponentBase {
            static defaultProps = { count: 1 };
        }
        <WithDefaults count={1} extra="bad" />;
        "#,
    );
    let excess_messages: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2322 && diag.message_text.contains("'extra' does not exist"))
        .map(|diag| diag.message_text.as_str())
        .collect();
    assert_eq!(
        excess_messages.len(),
        2,
        "Expected excess-property TS2322 for both managed-props components, got: {diagnostics:?}"
    );
    assert!(
        excess_messages
            .iter()
            .any(|message| message.contains("Defaultize<Inferred<")),
        "Expected propTypes+defaultProps excess target to preserve Defaultize<Inferred<...>>, got: {excess_messages:?}"
    );
    assert!(
        excess_messages
            .iter()
            .any(|message| message.contains("Defaultize<{}, { count: number; }>")),
        "Expected defaultProps-only excess target to preserve Defaultize<{{}}, defaults>, got: {excess_messages:?}"
    );
}

#[test]
fn jsx_library_managed_attributes_preserves_inferred_union_alias_for_required_prop() {
    let diagnostics = check_jsx_strict(
        r#"
        type Partial<T> = { [K in keyof T]?: T[K] };
        type Readonly<T> = { readonly [K in keyof T]: T[K] };
        type Inferred<S> = {
            [K in keyof S]: S[K] extends Checker<infer V, infer R>
                ? Checker<V, R>[typeof marker]
                : never
        };
        declare const marker: unique symbol;
        interface Checker<V, Required = false> {
            isRequired: Checker<V, true>;
            [marker]: Required extends true ? V : V | null | undefined;
        }
        declare namespace Kinds {
            const renderable: Checker<Renderable>;
        }
        type Renderable = string | number | ComponentBase<{}, {}>;
        declare class ComponentBase<P = {}, S = {}> {
            constructor(props: P);
            props: P & Readonly<{ children: Renderable[] }>;
            setState(s: Partial<S>): S;
            render(): Renderable;
        }
        declare namespace JSX {
            interface Element extends ComponentBase {}
            interface IntrinsicElements {}
            type LibraryManagedAttributes<C, P> =
                C extends { propTypes: infer T } ? P & Inferred<T> : P;
        }
        class RequiredRenderable extends ComponentBase {
            static propTypes = {
                item: Kinds.renderable.isRequired,
            };
        }
        <RequiredRenderable item={null} />;
        "#,
    );
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2322)
        .expect("expected TS2322 for null assigned to required Renderable prop");
    assert!(
        diag.message_text.contains("type 'Renderable'"),
        "Expected target display to preserve the inferred union alias, got: {diag:?}"
    );
    assert!(
        !diag
            .message_text
            .contains("string | number | ComponentBase"),
        "Required prop display should not expand the Renderable alias, got: {diag:?}"
    );
}

#[test]
fn jsx_library_managed_attributes_preserves_named_props_inside_defaultize() {
    let diagnostics = check_jsx_strict(
        r#"
        type Exclude<T, U> = T extends U ? never : T;
        type Extract<T, U> = T extends U ? T : never;
        type Partial<T> = { [K in keyof T]?: T[K] };
        type Readonly<T> = { readonly [K in keyof T]: T[K] };
        type Defaultize<TProps, TDefaults> =
            & { [K in Extract<keyof TProps, keyof TDefaults>]?: TProps[K] }
            & { [K in Exclude<keyof TProps, keyof TDefaults>]: TProps[K] }
            & Partial<TDefaults>;
        type Inferred<S> = {
            [K in keyof S]: S[K] extends Checker<infer V, infer R>
                ? Checker<V, R>[typeof marker]
                : {}
        };
        declare const marker: unique symbol;
        interface Checker<V, Required = false> {
            isRequired: Checker<V, true>;
            [marker]: Required extends true ? V : V | null | undefined;
        }
        declare namespace Kinds {
            const renderable: Checker<Renderable>;
        }
        type Renderable = string | number | ComponentBase<{}, {}>;
        interface OwnProps {
            title: string;
        }
        declare class ComponentBase<P = {}, S = {}> {
            constructor(props: P);
            props: P & Readonly<{ children: Renderable[] }>;
            setState(s: Partial<S>): S;
            render(): Renderable;
        }
        declare namespace JSX {
            interface Element extends ComponentBase {}
            interface IntrinsicElements {}
            type LibraryManagedAttributes<C, P> =
                C extends { defaultProps: infer D; propTypes: infer T; }
                    ? Defaultize<P & Inferred<T>, D>
                    : P;
        }
        class WithOwnProps extends ComponentBase<OwnProps> {
            static propTypes = {
                child: Kinds.renderable.isRequired,
            };
            static defaultProps = { title: "fallback" };
        }
        <WithOwnProps title="ok" />;
        "#,
    );
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2322)
        .expect("expected TS2322 for missing required propTypes-derived child");
    assert!(
        diag.message_text
            .contains("Defaultize<OwnProps & Inferred<"),
        "Expected Defaultize target to preserve OwnProps inside the intersection, got: {diag:?}"
    );
    assert!(
        !diag
            .message_text
            .contains("Defaultize<{ title: string; } & Inferred<"),
        "Defaultize target should not expand OwnProps structurally, got: {diag:?}"
    );
}

// -- tsxNotUsingApparentTypeOfSFC fingerprint fixes ---------------------------

/// Minimal JSX namespace with `IntrinsicAttributes` for the apparent-type-of-SFC tests.
const JSX_WITH_INTRINSIC_ATTRS: &str = concat!(
    "declare namespace JSX {\n",
    "  interface Element {}\n",
    "  interface ElementClass { render(): any; }\n",
    "  interface ElementAttributesProperty { props: {}; }\n",
    "  interface IntrinsicElements {}\n",
    "  interface IntrinsicAttributes { key?: string; }\n",
    "}\n",
);

/// `<MySFC />` where `MySFC` uses a free type variable `P` should emit TS2322 with target
/// displayed as just `'P'`, not `'IntrinsicAttributes & P'`.
/// Regression for tsxNotUsingApparentTypeOfSFC.tsx fingerprint bug.
#[test]
fn jsx_sfc_free_type_param_no_props_reports_plain_type_param_target() {
    let source = format!(
        "{JSX_WITH_INTRINSIC_ATTRS}
function test<P>(wrappedProps: P) {{
    let MySFC = function(props: P): JSX.Element {{ return null as any; }};
    let x = <MySFC />;
}}
"
    );
    let diagnostics = check_jsx_strict(&source);
    let has2322 = diagnostics.iter().any(|d| d.code == 2322);
    assert!(
        has2322,
        "Expected TS2322 for <MySFC /> with free type param, got: {diagnostics:?}"
    );
    // The target in the error message must be 'P', not 'IntrinsicAttributes & P'.
    let wrong_target = diagnostics
        .iter()
        .any(|d| d.code == 2322 && d.message_text.contains("IntrinsicAttributes & P"));
    assert!(
        !wrong_target,
        "TS2322 must say 'not assignable to type P', not 'IntrinsicAttributes & P'. Got: {diagnostics:?}"
    );
    let correct_target = diagnostics
        .iter()
        .any(|d| d.code == 2322 && d.message_text.contains("not assignable to type 'P'"));
    assert!(
        correct_target,
        "TS2322 message should contain \"not assignable to type 'P'\", got: {diagnostics:?}"
    );
}

/// `<MySFC {{...wrappedProps}} />` where `wrappedProps: P` (unconstrained) should emit TS2322
/// with target `'IntrinsicAttributes & P'` because P doesn't satisfy `IntrinsicAttributes`.
/// Regression for tsxNotUsingApparentTypeOfSFC.tsx fingerprint bug.
#[test]
fn jsx_sfc_free_type_param_spread_reports_intrinsic_attrs_target() {
    let source = format!(
        "{JSX_WITH_INTRINSIC_ATTRS}
function test<P>(wrappedProps: P) {{
    let MySFC = function(props: P): JSX.Element {{ return null as any; }};
    let z = <MySFC {{...wrappedProps}} />;
}}
"
    );
    let diagnostics = check_jsx_strict(&source);
    let has2322 = diagnostics.iter().any(|d| d.code == 2322);
    assert!(
        has2322,
        "Expected TS2322 for <MySFC {{...wrappedProps}} /> with unconstrained P, got: {diagnostics:?}"
    );
    // The target in the error message should be 'IntrinsicAttributes & P'.
    let correct_target = diagnostics
        .iter()
        .any(|d| d.code == 2322 && d.message_text.contains("IntrinsicAttributes & P"));
    assert!(
        correct_target,
        "TS2322 must say 'not assignable to type IntrinsicAttributes & P', got: {diagnostics:?}"
    );
}

/// Sanity: when P extends `IntrinsicAttributes`, the spread should not error.
#[test]
fn jsx_sfc_type_param_constrained_to_intrinsic_attrs_no_error_on_spread() {
    let source = format!(
        "{JSX_WITH_INTRINSIC_ATTRS}
function test<P extends JSX.IntrinsicAttributes>(wrappedProps: P) {{
    let MySFC = function(props: P): JSX.Element {{ return null as any; }};
    let z = <MySFC {{...wrappedProps}} />;
}}
"
    );
    let diagnostics = check_jsx_strict(&source);
    let has2322 = diagnostics.iter().any(|d| d.code == 2322);
    assert!(
        !has2322,
        "No TS2322 expected when P extends IntrinsicAttributes, got: {diagnostics:?}"
    );
}

/// SFC excess-property TS2322: target display should include `IntrinsicAttributes &`
/// when the JSX namespace declares `IntrinsicAttributes`. This mirrors
/// `jsxElementType.tsx`'s expected fingerprint shape.
#[test]
fn jsx_sfc_excess_property_target_includes_intrinsic_attributes_prefix() {
    let source = format!(
        "{JSX_WITH_INTRINSIC_ATTRS}
const RenderElement = ({{ title }}: {{ title: string }}) => null as any as JSX.Element;
<RenderElement excessProp />;
"
    );
    let diagnostics = check_jsx_strict(&source);
    let diag = diagnostics
        .iter()
        .find(|d| d.code == 2322)
        .expect("expected TS2322 for excess JSX attribute");
    assert!(
        diag.message_text
            .contains("IntrinsicAttributes & { title: string"),
        "Expected `IntrinsicAttributes &` prefix in TS2322 target display, got: {diag:?}"
    );
}

/// Regression for jsxChildrenGenericContextualTypes.tsx:
/// a zero-parameter callback nested in JSX body children whose body returns a
/// string literal must not be flagged when the children prop's expected return
/// type is the same string literal. The literal narrowing on the contextual
/// return type matches `tsc`; rechecking the raw widened type would otherwise
/// surface a spurious `Type '() => string' is not assignable to type
/// '(x: ...) => "x"'.` mismatch.
#[test]
fn jsx_zero_param_child_callback_with_literal_return_no_false_positive() {
    let source = r#"
namespace JSX {
    export interface Element {}
    export interface ElementAttributesProperty { props: {}; }
    export interface ElementChildrenAttribute { children: {}; }
    export interface IntrinsicAttributes {}
    export interface IntrinsicElements { [key: string]: Element }
}
interface LitProps<T> { prop: T, children: (x: this) => T }
const ElemLit = <T extends string>(p: LitProps<T>) => <div></div>;
const jj = <ElemLit prop="x">{() => "x"}</ElemLit>;
"#;
    let diagnostics = check_jsx_strict(source);
    assert!(
        !diagnostics.iter().any(|d| d.code == 2322),
        "No TS2322 expected for `() => \"x\"` matching contextual return `\"x\"`, got: {diagnostics:?}"
    );
}

/// Companion: when the body literal does NOT match the expected literal return,
/// tsz must still emit TS2322. The tag below corresponds to `mismatched` in
/// jsxChildrenGenericContextualTypes.tsx.
#[test]
fn jsx_zero_param_child_callback_with_mismatched_literal_return_emits_ts2322() {
    let source = r#"
namespace JSX {
    export interface Element {}
    export interface ElementAttributesProperty { props: {}; }
    export interface ElementChildrenAttribute { children: {}; }
    export interface IntrinsicAttributes {}
    export interface IntrinsicElements { [key: string]: Element }
}
interface LitProps<T> { prop: T, children: (x: this) => T }
const ElemLit = <T extends string>(p: LitProps<T>) => <div></div>;
const mismatched = <ElemLit prop="x">{() => 12}</ElemLit>;
"#;
    let diagnostics = check_jsx_strict(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2322),
        "Expected TS2322 for `() => 12` against contextual return `\"x\"`, got: {diagnostics:?}"
    );
}

/// Structural rule: when a JSX element contains an `any`-typed spread attribute
/// before an explicit attribute, the explicit attribute's value must NOT be
/// type-checked against the props type. The merged JSX attributes type after
/// an `any` spread is `any`-compatible, so per-attribute assignability is moot.
///
/// Mirrors the `let x2 = <OverWriteAttr {...anyobj} x={3} />` line in
/// `tsxSpreadAttributesResolution12.tsx`: tsc emits no diagnostic, but tsz
/// previously emitted TS2322 `Type '3' is not assignable to type '2'.` at the
/// `3` value position.
#[test]
fn jsx_explicit_attr_after_any_spread_no_ts2322() {
    let source = r#"
namespace JSX {
    export interface Element {}
    export interface ElementAttributesProperty { props: {}; }
    export interface IntrinsicAttributes {}
    export interface IntrinsicElements {}
}
interface Prop { x: 2; y: false; overwrite: string; }
declare class Comp { props: Prop; }
declare let anyobj: any;
let v = <Comp {...anyobj} x={3} />;
"#;
    let diagnostics = check_jsx(source);
    assert!(
        !diagnostics.iter().any(|d| d.code == 2322),
        "Explicit attr after any-spread must not produce TS2322; got: {diagnostics:?}"
    );
}

/// Same structural rule as above with a shorthand boolean attribute after the
/// any-spread. tsc emits no diagnostic; tsz must match.
#[test]
fn jsx_shorthand_attr_after_any_spread_no_ts2322() {
    let source = r#"
namespace JSX {
    export interface Element {}
    export interface ElementAttributesProperty { props: {}; }
    export interface IntrinsicAttributes {}
    export interface IntrinsicElements {}
}
interface Prop { y: false; }
declare class Comp { props: Prop; }
declare let anyobj: any;
let v = <Comp {...anyobj} y />;
"#;
    let diagnostics = check_jsx(source);
    assert!(
        !diagnostics.iter().any(|d| d.code == 2322),
        "Shorthand attr after any-spread must not produce TS2322; got: {diagnostics:?}"
    );
}

/// Sanity: when there is NO any-spread, the explicit attribute is still checked
/// against props and the mismatch produces TS2322. Guards the fix from
/// over-suppressing the normal path.
#[test]
fn jsx_explicit_attr_without_any_spread_still_emits_ts2322() {
    let source = r#"
namespace JSX {
    export interface Element {}
    export interface ElementAttributesProperty { props: {}; }
    export interface IntrinsicAttributes {}
    export interface IntrinsicElements {}
}
interface Prop { x: 2; }
declare class Comp { props: Prop; }
let v = <Comp x={3} />;
"#;
    let diagnostics = check_jsx(source);
    assert!(
        diagnostics.iter().any(|d| d.code == 2322),
        "Without any-spread, mismatched explicit attr must produce TS2322; got: {diagnostics:?}"
    );
}

// --- children union (Element | Element[]) no spurious TS2322 ---

const JSX_CHILDREN_UNION_PRELUDE: &str = r#"
namespace JSX {
    export interface Element {}
    export interface ElementAttributesProperty { props: {}; }
    export interface ElementChildrenAttribute { children: {}; }
    export interface IntrinsicAttributes {}
    export interface IntrinsicElements { div: {}; h1: {}; }
}
"#;

fn make_children_union_source(children_type: &str, jsx_body: &str) -> String {
    format!(
        r#"{JSX_CHILDREN_UNION_PRELUDE}
interface Props {{ children: {children_type}; }}
declare function Comp(p: Props): JSX.Element;
declare function A(): JSX.Element;
declare function B(): JSX.Element;
{jsx_body}
"#,
    )
}

#[test]
fn jsx_children_union_element_or_array_two_direct_children_no_ts2322() {
    let src = make_children_union_source(
        "JSX.Element | JSX.Element[]",
        "let k = <Comp><A /><B /></Comp>;",
    );
    let codes: Vec<u32> = check_jsx(&src).iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2322),
        "Two direct element children for Element|Element[] must not produce TS2322; got: {codes:?}"
    );
}

#[test]
fn jsx_children_union_single_child_no_ts2322() {
    let src =
        make_children_union_source("JSX.Element | JSX.Element[]", "let k = <Comp><A /></Comp>;");
    let codes: Vec<u32> = check_jsx(&src).iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2322),
        "Single element child for Element|Element[] must not produce TS2322; got: {codes:?}"
    );
}

#[test]
fn jsx_children_union_three_direct_children_no_ts2322() {
    let src = make_children_union_source(
        "JSX.Element | JSX.Element[]",
        "declare function C(): JSX.Element; let k = <Comp><A /><B /><C /></Comp>;",
    );
    let codes: Vec<u32> = check_jsx(&src).iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2322),
        "Three direct element children for Element|Element[] must not produce TS2322; got: {codes:?}"
    );
}

#[test]
fn jsx_children_union_node_name_variant_no_ts2322() {
    // Verify fix is not tied to the name "Element": use a user-defined Node type.
    let src = format!(
        r#"{JSX_CHILDREN_UNION_PRELUDE}
interface MyNode {{}}
interface NodeProps {{ children: MyNode | MyNode[]; }}
declare function Widget(p: NodeProps): JSX.Element;
declare function Child(): JSX.Element;
let k = <Widget><Child /><Child /></Widget>;
"#,
    );
    let codes: Vec<u32> = check_jsx(&src).iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2322),
        "Two children for MyNode|MyNode[] must not produce TS2322; got: {codes:?}"
    );
}

// --- TS2746 / TS2747 display: namespace-qualified children type must show
//     resolved symbol name (no `JSX.` qualifier, no trailing AST punctuation) ---

/// Brand on `Element` blocks `Array<Element>` ⊑ `Element`, so multi-child
/// against a single-child prop actually triggers TS2746 (open `Element {}`
/// silently accepts arrays via structural fallback).
const JSX_CHILDREN_BRANDED_PRELUDE: &str = r#"
namespace JSX {
    export interface Element { __brand_element: void; }
    export interface ElementAttributesProperty { props: {}; }
    export interface ElementChildrenAttribute { children: {}; }
    export interface IntrinsicAttributes {}
    export interface IntrinsicElements { div: {}; h1: {}; }
}
"#;

/// TS2746 display: when the children type is a namespace-qualified reference like
/// `JSX.Element`, the diagnostic must show the resolved symbol name (`Element`),
/// not the raw AST annotation text (`JSX.Element`). tsc routes `typeToString`
/// over the resolved type rather than printing the user's surface annotation.
#[test]
fn jsx_children_ts2746_strips_namespace_prefix_for_qualified_alias() {
    let src = format!(
        r#"{JSX_CHILDREN_BRANDED_PRELUDE}
interface SingleChildProp {{ children: JSX.Element; }}
declare function SingleChildComp(p: SingleChildProp): JSX.Element;
declare function A(): JSX.Element;
declare function B(): JSX.Element;
declare function C(): JSX.Element;
let k = <SingleChildComp><A /><B /><C /></SingleChildComp>;
"#,
    );
    let diagnostics = check_jsx(&src);
    let ts2746 = diagnostics
        .iter()
        .find(|d| d.code == 2746)
        .unwrap_or_else(|| {
            panic!(
                "Expected TS2746 for multi-child against single JSX.Element; got: {diagnostics:?}"
            )
        });
    assert!(
        ts2746.message_text.contains("'Element'"),
        "TS2746 message must use 'Element' (resolved symbol name), not 'JSX.Element' or other forms; got: {}",
        ts2746.message_text
    );
    assert!(
        !ts2746.message_text.contains("'JSX.Element'"),
        "TS2746 message must NOT include the 'JSX.' namespace prefix; got: {}",
        ts2746.message_text
    );
    assert!(
        !ts2746.message_text.contains("Element;"),
        "TS2746 message must NOT include a trailing semicolon from AST source text; got: {}",
        ts2746.message_text
    );
}

/// TS2746 display: a top-level user-defined alias must be preserved verbatim
/// — `Cb` stays `Cb`, never the structural body it resolves to.
#[test]
fn jsx_children_ts2746_preserves_user_defined_alias_name() {
    let src = format!(
        r#"{JSX_CHILDREN_BRANDED_PRELUDE}
interface Brand {{ __brand_cb: void; }}
type Cb = Brand;
interface Prop {{ children: Cb; }}
declare function Comp(p: Prop): JSX.Element;
declare function A(): JSX.Element;
declare function B(): JSX.Element;
declare function C(): JSX.Element;
let k = <Comp><A /><B /><C /></Comp>;
"#,
    );
    let diagnostics = check_jsx(&src);
    let ts2746 = diagnostics
        .iter()
        .find(|d| d.code == 2746)
        .unwrap_or_else(|| {
            panic!(
                "Expected TS2746 for multi-child against single alias-typed children; got: {diagnostics:?}"
            )
        });
    assert!(
        ts2746.message_text.contains("'Cb'"),
        "TS2746 message must use the alias name 'Cb' (matching the AST annotation), not its structural body; got: {}",
        ts2746.message_text
    );
    assert!(
        !ts2746.message_text.contains("__brand_cb"),
        "TS2746 message must NOT expand the alias to its structural body; got: {}",
        ts2746.message_text
    );
}

/// TS2746 display: when the children type is renamed to a non-`Element` name
/// inside the JSX namespace, the resolved symbol name (not the qualifier) is
/// what shows up. Proves the fix is not name-keyed to `Element`.
#[test]
fn jsx_children_ts2746_strips_namespace_prefix_for_arbitrary_member_name() {
    let src = r#"
namespace JSX {
    export interface Element { __brand_element: void; }
    export interface MyNode { __brand_my_node: void; }
    export interface ElementAttributesProperty { props: {}; }
    export interface ElementChildrenAttribute { children: {}; }
    export interface IntrinsicAttributes {}
    export interface IntrinsicElements { div: {}; }
}
interface SingleChildProp { children: JSX.MyNode; }
declare function SingleChildComp(p: SingleChildProp): JSX.Element;
declare function A(): JSX.MyNode;
declare function B(): JSX.MyNode;
declare function C(): JSX.MyNode;
let k = <SingleChildComp><A /><B /><C /></SingleChildComp>;
"#;
    let diagnostics = check_jsx(src);
    let ts2746 = diagnostics
        .iter()
        .find(|d| d.code == 2746)
        .unwrap_or_else(|| {
            panic!("Expected TS2746 for multi-child against JSX.MyNode; got: {diagnostics:?}")
        });
    assert!(
        ts2746.message_text.contains("'MyNode'"),
        "TS2746 must use 'MyNode' (resolved name), not 'JSX.MyNode'; got: {}",
        ts2746.message_text
    );
    assert!(
        !ts2746.message_text.contains("'JSX.MyNode'"),
        "TS2746 must NOT include 'JSX.' namespace prefix; got: {}",
        ts2746.message_text
    );
}

#[test]
fn jsx_children_ts2746_preserves_dotted_string_literal_type_text() {
    let src = format!(
        r#"{JSX_CHILDREN_BRANDED_PRELUDE}
interface Prop {{ children: "foo.bar"; }}
declare function Comp(p: Prop): JSX.Element;
declare function A(): JSX.Element;
declare function B(): JSX.Element;
let k = <Comp><A /><B /></Comp>;
"#,
    );
    let diagnostics = check_jsx(&src);
    let ts2746 = diagnostics
        .iter()
        .find(|d| d.code == 2746)
        .unwrap_or_else(|| {
            panic!(
                "Expected TS2746 for multi-child against string-literal children; got: {diagnostics:?}"
            )
        });
    assert!(
        ts2746.message_text.contains("'\"foo.bar\"'"),
        "TS2746 must preserve dotted string literal text; got: {}",
        ts2746.message_text
    );
    assert!(
        !ts2746.message_text.contains("'\"bar\"'"),
        "TS2746 must not strip dotted prefixes inside string literals; got: {}",
        ts2746.message_text
    );
}

#[test]
fn jsx_children_ts2746_preserves_dotted_template_literal_type_text() {
    let src = format!(
        r#"{JSX_CHILDREN_BRANDED_PRELUDE}
interface Prop {{ children: `foo.bar`; }}
declare function Comp(p: Prop): JSX.Element;
declare function A(): JSX.Element;
declare function B(): JSX.Element;
let k = <Comp><A /><B /></Comp>;
"#,
    );
    let diagnostics = check_jsx(&src);
    let ts2746 = diagnostics
        .iter()
        .find(|d| d.code == 2746)
        .unwrap_or_else(|| {
            panic!(
                "Expected TS2746 for multi-child against template-literal children; got: {diagnostics:?}"
            )
        });
    assert!(
        ts2746.message_text.contains("'`foo.bar`'"),
        "TS2746 must preserve dotted template literal text; got: {}",
        ts2746.message_text
    );
    assert!(
        !ts2746.message_text.contains("'`bar`'"),
        "TS2746 must not strip dotted prefixes inside template literals; got: {}",
        ts2746.message_text
    );
}

/// TS2747 display: same structural rule applies to text-children rejection
/// messages. Both TS2746 and TS2747 flow through the same children-type-display
/// helper and must produce the same namespace-stripped output.
#[test]
fn jsx_children_ts2747_strips_namespace_prefix() {
    let src = format!(
        r#"{JSX_CHILDREN_BRANDED_PRELUDE}
interface Prop {{ children: JSX.Element | JSX.Element[]; }}
declare function Comp(p: Prop): JSX.Element;
declare function A(): JSX.Element;
let k = <Comp>hello<A /></Comp>;
"#,
    );
    let diagnostics = check_jsx(&src);
    let ts2747 = diagnostics
        .iter()
        .find(|d| d.code == 2747)
        .unwrap_or_else(|| {
            panic!(
                "Expected TS2747 for text children against JSX.Element-union prop; got: {diagnostics:?}"
            )
        });
    assert!(
        !ts2747.message_text.contains("'JSX.Element"),
        "TS2747 message must NOT include the 'JSX.' namespace prefix; got: {}",
        ts2747.message_text
    );
    assert!(
        !ts2747.message_text.contains("Element;"),
        "TS2747 message must NOT include a trailing semicolon; got: {}",
        ts2747.message_text
    );
}

fn cross_file_jsx_opts() -> crate::context::CheckerOptions {
    use tsz_common::checker_options::JsxMode;
    crate::context::CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        strict_null_checks: true,
        ..Default::default()
    }
}

// Plain project file (not a lib), so its binder is in `all_binders` but not `lib_binders`.
const REACT_DECL: &str = r#"
declare namespace React {
    type ReactNode = ReactElement<any> | string | number | null;
    interface ReactElement<P> { props: P; }
    type ComponentState = any;
    interface Component<P = {}, S = ComponentState> {
        readonly props: P;
        state: S;
        render(): ReactNode;
    }
    interface ComponentClass<P = {}, S = ComponentState> {
        new(props: P, context?: any): Component<P, S>;
        defaultProps?: Partial<P>;
    }
    interface StatelessComponent<P = {}> {
        (props: P & { children?: ReactNode }, context?: any): ReactElement<any> | null;
        defaultProps?: Partial<P>;
    }
    type ComponentType<P = {}> = ComponentClass<P> | StatelessComponent<P>;
    type ReactType<P = any> = string | ComponentType<P>;
}
declare namespace JSX {
    interface Element extends React.ReactElement<any> {}
    interface ElementClass extends React.Component<any> {
        render(): React.ReactNode;
    }
    interface ElementAttributesProperty { props: {}; }
    interface IntrinsicElements {
        a: {};
        button: {};
    }
}
"#;

#[test]
fn cross_file_component_type_union_no_ts2786() {
    // `React.ComponentType<P1> | React.ComponentType<P2>` where `ComponentType`
    // lives in a separate project file (not a lib binder).
    let entry = r#"
interface P1 { p?: boolean; c?: string; }
interface P2 { p?: boolean; c?: any; d?: any; }
var C: React.ComponentType<P1> | React.ComponentType<P2>;
const a = <C p={true} />;
"#;
    let diags = check_multi_file(
        &[("react.d.ts", REACT_DECL), ("test.tsx", entry)],
        "test.tsx",
        cross_file_jsx_opts(),
    );
    assert!(
        !diags.iter().any(|d| d.code == 2786),
        "React.ComponentType union from cross-file decl must not emit TS2786; got: {diags:?}"
    );
}

#[test]
fn cross_file_react_type_union_with_string_no_ts2786() {
    // `React.ReactType` (= `string | ComponentType<P>`) from a cross-file binder.
    let entry = r#"
declare const props: { component: React.ReactType };
const Comp: React.ReactType = props.component;
const elem = <Comp />;
"#;
    let diags = check_multi_file(
        &[("react.d.ts", REACT_DECL), ("test.tsx", entry)],
        "test.tsx",
        cross_file_jsx_opts(),
    );
    assert!(
        !diags.iter().any(|d| d.code == 2786),
        "React.ReactType from cross-file decl must not emit TS2786; got: {diags:?}"
    );
}

#[test]
fn cross_file_component_class_generic_no_ts2786() {
    // `React.ComponentClass<P>` used directly as a JSX component type from a
    // cross-file binder — also exercises the `react_component_alias_application_props_arg`
    // path that calls `react_component_alias_def_has_react_origin`.
    let entry = r#"
interface Props { x?: number; }
declare const Widget: React.ComponentClass<Props>;
const elem = <Widget x={1} />;
"#;
    let diags = check_multi_file(
        &[("react.d.ts", REACT_DECL), ("test.tsx", entry)],
        "test.tsx",
        cross_file_jsx_opts(),
    );
    assert!(
        !diags.iter().any(|d| d.code == 2786),
        "React.ComponentClass from cross-file decl must not emit TS2786; got: {diags:?}"
    );
}

#[test]
fn cross_file_component_type_alias_renamed_param_no_ts2786() {
    // Same as `cross_file_component_type_union_no_ts2786` but with a renamed
    // type-parameter to prove the fix is not keyed on the name "P".
    let entry = r#"
interface Foo { a?: string; }
interface Bar { a?: number; }
var C: React.ComponentType<Foo> | React.ComponentType<Bar>;
const elem = <C />;
"#;
    let diags = check_multi_file(
        &[("react.d.ts", REACT_DECL), ("test.tsx", entry)],
        "test.tsx",
        cross_file_jsx_opts(),
    );
    assert!(
        !diags.iter().any(|d| d.code == 2786),
        "React.ComponentType union with renamed type arg must not emit TS2786; got: {diags:?}"
    );
}

