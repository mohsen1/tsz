//! JSX unit tests.

use crate::test_utils::{check_source, check_source_diagnostics};

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

fn check_jsx_strict(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    use crate::context::CheckerOptions;
    use tsz_common::checker_options::JsxMode;
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.tsx", opts)
}

fn check_jsx_strict_codes(source: &str) -> Vec<u32> {
    check_jsx_strict(source).iter().map(|d| d.code).collect()
}

fn check_jsx_no_strict(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    use crate::context::CheckerOptions;
    use tsz_common::checker_options::JsxMode;
    let opts = CheckerOptions {
        jsx_mode: JsxMode::Preserve,
        strict: false,
        strict_null_checks: false,
        strict_function_types: false,
        strict_property_initialization: false,
        no_implicit_any: false,
        no_implicit_this: false,
        use_unknown_in_catch_variables: false,
        strict_builtin_iterator_return: false,
        ..CheckerOptions::default()
    };
    check_source(source, "test.tsx", opts)
}

fn check_jsx_no_strict_codes(source: &str) -> Vec<u32> {
    check_jsx_no_strict(source).iter().map(|d| d.code).collect()
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

#[test]
fn jsx_ignored_data_attribute_keeps_real_type_in_missing_prop_display() {
    let diagnostics = check_jsx(
        r#"
        declare namespace JSX { interface Element {} }
        interface Props {
            foo: string;
            [dataProp: string]: string;
        }
        declare function Comp(props: Props): JSX.Element;
        <Comp bar="hello" data-yadda={42} />;
        "#,
    );
    let ts2741 = diagnostics
        .iter()
        .find(|diag| diag.code == 2741)
        .expect("expected TS2741 for missing required prop");
    assert!(
        ts2741.message_text.contains("\"data-yadda\": number"),
        "Expected ignored data-* attr to keep its real type in TS2741 display, got: {ts2741:?}"
    );
    assert!(
        !ts2741.message_text.contains("\"data-yadda\": any"),
        "Ignored data-* attr should not fall back to any in TS2741 display, got: {ts2741:?}"
    );
}

#[test]
fn jsx_key_error_in_parenthesized_callback_body_is_not_dropped() {
    let diagnostics = check_jsx(
        r#"
        declare namespace React {
            type DetailedHTMLProps<E extends HTMLAttributes<T>, T> = E;
            interface HTMLAttributes<T> {
                children?: ReactNode;
            }
            type ReactNode = ReactChild | ReactFragment | boolean | null | undefined;
            type ReactText = string | number;
            type ReactChild = ReactText;
            type ReactFragment = {};
        }
        interface HTMLLIElement {}
        declare namespace JSX {
            interface Element {}
            interface IntrinsicElements {
                li: React.DetailedHTMLProps<React.HTMLAttributes<HTMLLIElement>, HTMLLIElement>;
            }
        }
        declare var React: any;
        declare function renderCategory(render: (category: string) => JSX.Element): void;
        renderCategory((category) => (
            <li key={category}>{category}</li>
        ));
        "#,
    );
    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 2322 && diag.message_text.contains("'key' does not exist")
        }),
        "Expected JSX key TS2322 from the parenthesized map callback body, got: {diagnostics:?}"
    );
}

#[test]
fn jsx_key_error_in_generic_callback_body_is_not_dropped() {
    let diagnostics = check_jsx(
        r#"
        declare namespace React {
            type DetailedHTMLProps<E extends HTMLAttributes<T>, T> = E;
            interface HTMLAttributes<T> {
                children?: ReactNode;
            }
            type ReactNode = ReactChild | ReactFragment | boolean | null | undefined;
            type ReactText = string | number;
            type ReactChild = ReactText;
            type ReactFragment = {};
        }
        interface Array<T> {
            map<U>(callbackfn: (value: T) => U): U[];
        }
        interface HTMLLIElement {}
        declare namespace JSX {
            interface Element {}
            interface IntrinsicElements {
                li: React.DetailedHTMLProps<React.HTMLAttributes<HTMLLIElement>, HTMLLIElement>;
            }
        }
        declare var React: any;

        const categories = ["Fruit", "Vegetables"];
        categories.map((category) => (
            <li key={category}>{category}</li>
        ));
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == 2322 && diag.message_text.contains("'key' does not exist")),
        "Expected JSX key TS2322 from the generic callback body, got: {diagnostics:?}"
    );
}

#[test]
fn jsx_key_error_in_generic_callback_body_inside_jsx_children_is_not_dropped() {
    let diagnostics = check_jsx(
        r#"
        declare namespace React {
            type DetailedHTMLProps<E extends HTMLAttributes<T>, T> = E;
            interface HTMLAttributes<T> {
                children?: ReactNode;
            }
            type ReactNode = ReactChild | ReactFragment | boolean | null | undefined;
            type ReactText = string | number;
            type ReactChild = ReactText;
            type ReactFragment = {} | ReactNodeArray;
            interface ReactNodeArray extends Array<ReactNode> {}
        }
        declare namespace JSX {
            interface IntrinsicElements {
                ul: React.DetailedHTMLProps<React.HTMLAttributes<HTMLUListElement>, HTMLUListElement>;
                li: React.DetailedHTMLProps<React.HTMLAttributes<HTMLLIElement>, HTMLLIElement>;
            }
        }
        declare var React: any;

        const Component = () => {
            const categories = ["Fruit", "Vegetables"];
            return (
                <ul>
                    <li>All</li>
                    {categories.map((category) => (
                        <li key={category}>{category}</li>
                    ))}
                </ul>
            );
        };
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.code == 2322 && diag.message_text.contains("'key' does not exist")),
        "Expected JSX key TS2322 from the generic callback nested inside JSX children, got: {diagnostics:?}"
    );
}

#[test]
fn jsx_key_error_in_excessive_stack_depth_flat_array_fixture_is_not_dropped() {
    let diagnostics = check_jsx(
        r#"
        interface Array<T> {
          map<U>(callbackfn: (value: T) => U): U[];
        }
        interface HTMLUListElement {}
        interface HTMLLIElement {}
        interface MiddlewareArray<T> extends Array<T> {}
        declare function configureStore(options: { middleware: MiddlewareArray<any> }): void;

        declare const defaultMiddleware: MiddlewareArray<any>;
        configureStore({
          middleware: [...defaultMiddleware],
        });

        declare namespace React {
          type DetailedHTMLProps<E extends HTMLAttributes<T>, T> = E;
          interface HTMLAttributes<T> {
            children?: ReactNode;
          }
          type ReactNode = ReactChild | ReactFragment | boolean | null | undefined;
          type ReactText = string | number;
          type ReactChild = ReactText;
          type ReactFragment = {} | ReactNodeArray;
          interface ReactNodeArray extends Array<ReactNode> {}
        }
        declare namespace JSX {
          interface IntrinsicElements {
            ul: React.DetailedHTMLProps<React.HTMLAttributes<HTMLUListElement>, HTMLUListElement>;
            li: React.DetailedHTMLProps<React.HTMLAttributes<HTMLLIElement>, HTMLLIElement>;
          }
        }
        declare var React: any;

        const Component = () => {
          const categories = ['Fruit', 'Vegetables'];

          return (
            <ul>
              <li>All</li>
              {categories.map((category) => (
                <li key={category}>{category}</li>
              ))}
            </ul>
          );
        };
        "#,
    );
    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 2322
                && diag
                    .message_text
                    .contains("is not assignable to type 'HTMLAttributes<HTMLLIElement>'")
                && diag
                    .message_text
                    .contains("'key' does not exist in type 'HTMLAttributes<HTMLLIElement>'")
                && diag.message_text.contains("HTMLAttributes<HTMLLIElement>")
        }),
        "Expected real excessiveStackDepthFlatArray shape to keep the nested JSX key TS2322, got: {diagnostics:?}"
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

/// TS2786 should NOT fire for SFCs whose inferred return type is `never`
/// (e.g. `function MyComp(props) { return null!; }`). `never` is the bottom
/// type and is assignable to `JSX.Element`. Mirrors tsc behavior — see
/// conformance test `spellingSuggestionJSXAttribute.tsx`.
#[test]
fn jsx_sfc_returning_never_no_ts2786() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element { }
            interface IntrinsicElements { }
        }
        function MyComp(props: { className?: string }) {
            return null!;
        }
        <MyComp className="" />;
        "#,
    );
    assert!(
        !diagnostics.contains(&2786),
        "SFC returning never (bottom type) should not emit TS2786, got: {diagnostics:?}"
    );
}

/// TS2786 SHOULD fire with strictNullChecks for an SFC returning `undefined`
/// (arrow function form). `undefined` is not assignable to `JSX.Element | null`.
/// Mirrors `tsxSfcReturnUndefinedStrictNullChecks.tsx`.
#[test]
fn jsx_sfc_returning_undefined_strict_null_checks_emits_ts2786() {
    let diagnostics = check_jsx_strict_codes(
        r#"
        declare namespace JSX {
            interface Element { }
            interface IntrinsicElements { }
        }
        const Foo = (props: any) => undefined;
        <Foo />;
        "#,
    );
    assert!(
        diagnostics.contains(&2786),
        "SFC returning undefined with strictNullChecks should emit TS2786, got: {diagnostics:?}"
    );
}

/// TS2786 SHOULD fire with strictNullChecks for an SFC whose body returns `undefined`
/// (function declaration form).
#[test]
fn jsx_sfc_function_body_returning_undefined_strict_null_checks_emits_ts2786() {
    let diagnostics = check_jsx_strict_codes(
        r#"
        declare namespace JSX {
            interface Element { }
            interface IntrinsicElements { }
        }
        function Greet(x: { name?: string }) {
            return undefined;
        }
        <Greet />;
        "#,
    );
    assert!(
        diagnostics.contains(&2786),
        "SFC returning undefined (function body) with strictNullChecks should emit TS2786, got: {diagnostics:?}"
    );
}

/// TS2786 should NOT fire without strictNullChecks for an SFC returning `undefined`
/// (undefined is a subtype of every type without strict null checks).
#[test]
fn jsx_sfc_returning_undefined_no_strict_null_checks_no_ts2786() {
    let diagnostics = check_jsx_no_strict_codes(
        r#"
        declare namespace JSX {
            interface Element { }
            interface IntrinsicElements { }
        }
        const Foo = (props: any) => undefined;
        <Foo />;
        "#,
    );
    assert!(
        !diagnostics.contains(&2786),
        "SFC returning undefined without strictNullChecks should not emit TS2786, got: {diagnostics:?}"
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

#[test]
fn jsx_element_attributes_property_multiple_members_anchors_at_interface_name() {
    let source = r#"
        declare namespace JSX {
            interface Element { }
            interface ElementAttributesProperty { pr1: any; pr2: any; }
            interface IntrinsicElements { }
        }
        interface CompType { new(n: string): {}; }
        declare var Comp: CompType;
        <Comp x={10} />;
        "#;
    let diagnostics = check_jsx(source);
    let expected_start = source
        .find("ElementAttributesProperty")
        .expect("expected ElementAttributesProperty in source") as u32;
    let ts2608 = diagnostics
        .iter()
        .find(|diag| diag.code == 2608)
        .expect("expected TS2608 for invalid ElementAttributesProperty");
    assert_eq!(
        ts2608.start, expected_start,
        "TS2608 should anchor at the ElementAttributesProperty name, got: {diagnostics:?}"
    );
}

#[test]
fn jsx_invalid_element_attributes_property_props_assignability_anchors_at_tag_name() {
    let source = r#"
        declare namespace JSX {
            interface Element { }
            interface ElementAttributesProperty { pr1: any; pr2: any; }
            interface IntrinsicElements { }
        }
        interface CompType { new(n: string): {}; }
        declare var Comp: CompType;
        <Comp x={10} />;
        "#;
    let diagnostics = check_jsx(source);
    let expected_start = source
        .find("Comp x")
        .expect("expected JSX tag name in source") as u32;
    let ts2322 = diagnostics
        .iter()
        .find(|diag| {
            diag.code == 2322
                && diag
                    .message_text
                    .contains("Type '{ x: number; }' is not assignable to type 'string'.")
        })
        .expect("expected TS2322 for invalid JSX attributes object");
    assert_eq!(
        ts2322.start, expected_start,
        "TS2322 should anchor at the JSX tag name, got: {diagnostics:?}"
    );
}

/// Empty `JSX.ElementAttributesProperty` -> the construct signature's return
/// (instance) type is the attributes type. tsc:
/// `forcedLookupLocation === "" ? getReturnTypeOfSignature(sig) : ...`.
///
/// `<Obj2 x={10} />` checks `{ x: number }` against the instance type
/// `{ q?: number }`, producing TS2322 with the instance type — not the
/// constructor's first parameter type.
#[test]
fn jsx_empty_element_attributes_property_uses_instance_type() {
    let diagnostics = check_jsx(
        r#"
        declare namespace JSX {
            interface Element { }
            interface ElementAttributesProperty { }
            interface IntrinsicElements { }
        }
        interface Obj2type { new(n: string): { q?: number }; }
        declare var Obj2: Obj2type;
        <Obj2 x={10} />;
        "#,
    );
    let ts2322: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.iter().any(|d| d
            .message_text
            .contains("is not assignable to type '{ q?: number | undefined; }'")),
        "expected TS2322 to compare against the instance type, got: {diagnostics:?}"
    );
    assert!(
        !ts2322
            .iter()
            .any(|d| d.message_text.contains("type 'string'")),
        "TS2322 should not use the constructor's first parameter ('string') as the props type, got: {diagnostics:?}"
    );
}

/// Empty EAP with a constructor whose return type already has the attribute
/// shape (`{ x: number }`) should not emit TS2322.
#[test]
fn jsx_empty_element_attributes_property_matches_instance_type_no_error() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element { }
            interface ElementAttributesProperty { }
            interface IntrinsicElements { }
        }
        interface Obj3type { new(n: string): { x: number; }; }
        declare var Obj3: Obj3type;
        <Obj3 x={10} />;
        "#,
    );
    assert!(
        !diagnostics.contains(&2322),
        "Instance type with matching attribute should not emit TS2322, got: {diagnostics:?}"
    );
}

/// `JSX.ElementAttributesProperty` with multiple members emits TS2608 and
/// then routes the attributes type back through the no-EAP branch (first
/// construct-signature parameter), matching tsc's
/// `getJsxElementPropertiesName` returning `undefined` in that case.
#[test]
fn jsx_multi_member_eap_uses_first_constructor_parameter() {
    let diagnostics = check_jsx(
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
    let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2608),
        "TS2608 expected for multi-member ElementAttributesProperty, got: {codes:?}"
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2322
            && d.message_text
                .contains("is not assignable to type 'string'")),
        "TS2322 should compare against the first constructor parameter when EAP has >1 members, got: {diagnostics:?}"
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
fn jsx_overload_mismatch_reports_ts2769_before_ts2786() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element { type: 'element'; }
            interface IntrinsicElements {}
        }

        interface LinkComponent {
            (props: { className?: string }): { invalid: true };
            (props: { htmlFor?: string }): { invalid: true };
        }

        declare const Link: LinkComponent;

        <Link class="bad" />;
        "#,
    );
    assert!(
        diagnostics.contains(&2769),
        "Overload prop mismatches should still emit TS2769, got: {diagnostics:?}"
    );
    // tsc emits TS2786 alongside TS2769 when none of the overloads return
    // a type assignable to JSX.Element. Both diagnostics are expected.
    assert!(
        diagnostics.contains(&2786),
        "When all overload return types are invalid JSX elements, TS2786 should also be emitted, got: {diagnostics:?}"
    );
}

/// Class components with multi-construct overloads (like React.Component with
/// `constructor(props: Readonly<P>)` and `constructor(props: P, context?: any)`)
/// must emit TS2769 when the overload props type evaluates to `unknown` due to
/// mapped type application failure. The evaluation fallback prevents the overload
/// from matching vacuously against `unknown`.
#[test]
fn jsx_class_component_multi_construct_overload_children_tuple_mismatch() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element { __brand: 'element'; }
            interface ElementClass { render(): any; }
            interface ElementAttributesProperty { props: {}; }
            interface ElementChildrenAttribute { children: {}; }
            interface IntrinsicElements { div: {}; }
        }
        type ReactNode = string | number | boolean | null | undefined | Element;

        type Readonly<T> = { readonly [P in keyof T]: T[P]; };

        declare class Component<P> {
            constructor(props: Readonly<P>);
            constructor(props: P, context?: any);
            props: Readonly<P> & Readonly<{ children?: ReactNode }>;
            render(): any;
        }

        interface PanelProps {
            children: [ReactNode, ReactNode];
        }

        class Panel extends Component<PanelProps> {}

        // 2 children — should match the 2-tuple, no error
        <Panel><div /><div /></Panel>;
        // 3 children — should NOT match any overload, emit TS2769
        <Panel><div /><div /><div /></Panel>;
        "#,
    );
    assert!(
        diagnostics.contains(&2769),
        "3 children should not match 2-tuple overload, expected TS2769, got: {diagnostics:?}"
    );
}

/// JSX class components with multi-construct overloads (React.Component-style)
/// must NOT report TS2769 when JSX body children are passed and the class's
/// constructor props type doesn't itself include `children`. The synthesized
/// `children` attribute (no source name token) must be exempt from the
/// overload's excess-property check, since for class JSX the children are
/// supplied by the JSX machinery, not by the constructor parameter.
#[test]
fn jsx_class_overload_synthesized_children_not_excess() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element { __brand: 'element'; }
            interface ElementClass { render(): any; }
            interface ElementAttributesProperty { props: {}; }
            interface ElementChildrenAttribute { children: {}; }
            interface IntrinsicElements { div: {}; }
        }
        type ReactNode = string | number | boolean | null | undefined | Element;

        type Readonly<T> = { readonly [P in keyof T]: T[P]; };

        declare class Component<P> {
            constructor(props: Readonly<P>);
            constructor(props: P, context?: any);
            props: Readonly<P> & Readonly<{ children?: ReactNode }>;
            render(): any;
        }

        interface BaseProps { error?: boolean; }
        // No `children` in props — the constructor's first param is just BaseProps.
        class Widget extends Component<BaseProps> {}

        // JSX body children "Hi" must not trigger TS2769 — the synthesized
        // `children` attribute is not user-written and must be exempt.
        <Widget error>Hi</Widget>;
        "#,
    );
    assert!(
        !diagnostics.contains(&2769),
        "Synthesized JSX children must not trigger TS2769 on class overloads, got: {diagnostics:?}"
    );
}

/// JSX class components with multi-construct overloads must consult
/// `static defaultProps` and treat its keys as supplied. A `<Comp />` call
/// with no attributes must not fail overload resolution just because the
/// constructor's props type marks a field required, when defaultProps
/// supplies that field. This mirrors tsc's `LibraryManagedAttributes`
/// relaxation under overload semantics.
#[test]
fn jsx_class_overload_default_props_relaxes_required() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element { __brand: 'element'; }
            interface ElementClass { render(): any; }
            interface ElementAttributesProperty { props: {}; }
            interface ElementChildrenAttribute { children: {}; }
            interface IntrinsicElements { div: {}; }
        }
        type ReactNode = string | number | boolean | null | undefined | Element;
        type Readonly<T> = { readonly [P in keyof T]: T[P]; };

        declare class Component<P> {
            constructor(props: Readonly<P>);
            constructor(props: P, context?: any);
            props: Readonly<P> & Readonly<{ children?: ReactNode }>;
            render(): any;
        }

        interface RequiredProps { when: (value: string) => boolean; }
        class Widget<P extends RequiredProps = RequiredProps> extends Component<P> {
            static defaultProps = { when: () => true };
        }

        // No attrs — `when` is supplied by defaultProps, so this is valid.
        <Widget />;
        "#,
    );
    assert!(
        !diagnostics.contains(&2769),
        "defaultProps must relax required-prop overload check, got: {diagnostics:?}"
    );
}

#[test]
fn jsx_single_child_mismatch_uses_react_element_display_and_child_anchors() {
    let source = r#"
        declare namespace React {
            interface ReactElement<P = any> {
                props: P;
            }
            class Component<P = {}, S = {}> {
                props: P;
                state: S;
                setState(state: S): void;
                forceUpdate(): void;
                render(): any;
            }
        }
        declare namespace JSX {
            interface Element extends React.ReactElement<any> {}
            interface ElementClass extends React.Component<any, any> {
                render(): any;
            }
            interface ElementAttributesProperty { props: {}; }
            interface IntrinsicElements { div: {}; }
        }

        interface Prop {
            a: number;
            b: string;
            children: Button;
        }

        class Button extends React.Component<any, any> {
            render() {
                return <div />;
            }
        }

        function Comp(_p: Prop) {
            return <div />;
        }

        let k = <Comp a={10} b="hi" />;
        let k1 =
            <Comp a={10} b="hi">
                <Button />
            </Comp>;
        let k2 =
            <Comp a={10} b="hi">
                {Button}
            </Comp>;
        "#;
    let diagnostics = check_jsx(source);
    let child_mismatch_diags: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2739 || diag.code == 2740)
        .collect();
    assert_eq!(
        child_mismatch_diags.len(),
        2,
        "Expected exactly two JSX child mismatch diagnostics, got: {diagnostics:?}"
    );

    let react_element_diag = child_mismatch_diags
        .iter()
        .copied()
        .find(|diag| diag.message_text.contains("Type 'ReactElement<any>'"))
        .expect("Expected JSX child mismatch diagnostic to report source as ReactElement<any>");
    assert!(
        !react_element_diag.message_text.contains("Type 'Element'"),
        "TS2740 should not report JSX child source as bare Element, got: {react_element_diag:?}"
    );

    let expected_button_child_start = source
        .find("<Button />")
        .expect("fixture should contain <Button />") as u32;
    assert_eq!(
        react_element_diag.start, expected_button_child_start,
        "TS2740 for JSX element child should be anchored at <Button />"
    );

    let typeof_button_diag = child_mismatch_diags
        .iter()
        .copied()
        .find(|diag| diag.message_text.contains("Type 'typeof Button'"))
        .expect("Expected JSX child mismatch diagnostic for {Button} child");
    let expected_button_expr_start = source
        .find("{Button}")
        .expect("fixture should contain {Button}") as u32
        + 1;
    assert_eq!(
        typeof_button_diag.start, expected_button_expr_start,
        "TS2740 for expression child should be anchored at the Button identifier"
    );
}

#[test]
fn jsx_generic_sfc_defaulted_props_contextually_type_function_attributes() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element {}
            interface IntrinsicElements { a: { onClick?: (e: { currentTarget: HTMLAnchorElement }) => void } }
        }

        interface HTMLAnchorElement {
            href: string;
        }

        type ElementType = "a" | "button";
        type ComponentPropsWithRef<T extends ElementType> =
            T extends "a"
                ? { onClick?: (e: { currentTarget: HTMLAnchorElement }) => void }
                : { onClick?: (e: { currentTarget: { disabled: boolean } }) => void };
        type Omit<T, K extends PropertyKey> = Pick<T, Exclude<keyof T, K>>;

        declare function Link<T extends ElementType = ElementType>(
            props: Omit<ComponentPropsWithRef<ElementType extends T ? "a" : T>, "as">
        ): JSX.Element;

        <Link onClick={(e) => e.currentTarget.href} />;
        "#,
    );
    assert!(
        !diagnostics.contains(&7006),
        "Expected generic JSX SFC defaults to contextually type function attrs, got: {diagnostics:?}"
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
fn jsx_library_managed_attributes_preserves_function_default_props_in_jsx() {
    let diagnostics = check_jsx_codes(
        r#"
        type Defaultize<TProps, TDefaults> =
            & { [K in Extract<keyof TProps, keyof TDefaults>]?: TProps[K] }
            & { [K in Exclude<keyof TProps, keyof TDefaults>]: TProps[K] }
            & Partial<TDefaults>;

        declare namespace JSX {
            interface Element {}
            interface IntrinsicElements { div: {}; }
            type LibraryManagedAttributes<TComponent, TProps> =
                TComponent extends { defaultProps: infer D }
                    ? Defaultize<TProps, D>
                    : TProps;
        }

        interface Props {
            text: string;
        }

        function BackButton(_props: Props) {
            return <div />;
        }

        BackButton.defaultProps = {
            text: "Go Back",
        };

        let element = <BackButton />;
        "#,
    );
    assert!(
        !diagnostics.contains(&2741),
        "Expected function component defaultProps to flow through JSX.LibraryManagedAttributes, got: {diagnostics:?}"
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

// NOTE: This test is disabled because it depends on fixing conditional type
// evaluation with `infer` in React's distributive Defaultize type. The root
// cause is in the solver: `keyof D` evaluates to `never` when D comes from
// conditional infer and the check type is a class constructor/callable type.
// This is a Tier 1 (big3-unification) issue, not a Tier 3 leaf fix.
#[test]
#[ignore]
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

// ── JSX.ElementType (component-validity broadening) ──────────────────────
//
// When `JSX.ElementType` is declared, tsc validates the *component type*
// against `JSX.ElementType` instead of the legacy `JSX.Element` /
// `JSX.ElementClass` return-type checks. This lets users broaden what
// counts as a valid JSX component (e.g. async/string/Promise function
// components beyond `ReactElement`).

/// When `JSX.ElementType` accepts a function returning `string`, an SFC
/// returning `string` must NOT emit TS2786 — even though its return type
/// is not assignable to the legacy `JSX.Element`.
#[test]
fn jsx_element_type_broadens_valid_components_no_ts2786() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {}
    type ElementType = ((props: any) => string | number) | (new (props: any) => { render(): any });
}
const RenderString = ({ title }: { title: string }) => title;
<RenderString title="x" />;
"#;
    let diagnostics = check_jsx_codes(source);
    assert!(
        !diagnostics.contains(&2786),
        "ElementType allowing string-returning SFCs must not flag them as invalid components, got: {diagnostics:?}"
    );
}

/// Same fix with a different iteration-variable spelling for the
/// `ElementType` arrow parameter — guards against any name-based
/// hardcoding (per CLAUDE.md §25).
#[test]
fn jsx_element_type_broadens_valid_components_no_ts2786_alt_param_name() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {}
    type ElementType = ((p: any) => string | number) | (new (p: any) => { render(): any });
}
const RenderNumber = ({ title }: { title: string }) => title.length;
<RenderNumber title="x" />;
"#;
    let diagnostics = check_jsx_codes(source);
    assert!(
        !diagnostics.contains(&2786),
        "ElementType param-name spelling must not affect the result, got: {diagnostics:?}"
    );
}

/// When the SFC's signature does NOT satisfy `JSX.ElementType` (e.g. a
/// 2-parameter function but `ElementType`'s SFC member is single-param),
/// tsc emits TS2786. tsz must too.
#[test]
fn jsx_element_type_rejects_extra_param_function() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {}
    type ElementType = ((props: any) => any) | (new (props: any) => { render(): any });
}
function ExtraParam(props: {}, ref: number) { return null; }
<ExtraParam />;
"#;
    let diagnostics = check_jsx_codes(source);
    assert!(
        diagnostics.contains(&2786),
        "ElementType requires single-param SFCs; 2-param function must emit TS2786, got: {diagnostics:?}"
    );
}

/// When `JSX.ElementType` is NOT declared, the legacy `JSX.Element`
/// return-type check still applies — an SFC returning `string` is invalid.
#[test]
fn jsx_no_element_type_falls_back_to_legacy_jsx_element_check() {
    let source = r#"
declare namespace JSX {
    interface Element { __element_brand: void; }
    interface IntrinsicElements {}
}
const RenderString = ({ title }: { title: string }) => title;
<RenderString title="x" />;
"#;
    let diagnostics = check_jsx_codes(source);
    assert!(
        diagnostics.contains(&2786),
        "Without ElementType, string-returning SFC must still emit TS2786, got: {diagnostics:?}"
    );
}
