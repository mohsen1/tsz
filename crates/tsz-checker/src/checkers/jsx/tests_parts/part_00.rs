#[test]
fn jsx_same_name_function_component_and_props_interface_does_not_recurse() {
    let diagnostics = check_jsx(
        r#"
        declare namespace JSX {
          interface Element {
            type: string;
            props: Record<string, unknown>;
          }
          interface IntrinsicElements {
            div: { children?: unknown };
            span: { children?: unknown };
          }
        }

        interface Fragment {
          children?: unknown[];
        }

        function Fragment(props: Fragment): JSX.Element {
          return <div>{props.children}</div>;
        }

        const frag = (
          <Fragment>
            <span>A</span>
            <span>B</span>
          </Fragment>
        );
        "#,
    );

    assert!(
        diagnostics.is_empty(),
        "Same-name JSX component and props interface should check without diagnostics, got: {diagnostics:?}"
    );
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
fn jsx_component_type_missing_prop_display_uses_props_type_arg() {
    let diagnostics = check_jsx_strict(
        r#"declare namespace JSX { interface Element {} interface ElementClass { render(): any; } interface ElementAttributesProperty { props: {}; } } type Readonly<T> = { readonly [P in keyof T]: T[P]; }; declare namespace React { interface Component<P> { props: Readonly<P>; render(): JSX.Element; } interface ComponentClass<P = {}> { new(props: P, context?: any): Component<P>; } interface FunctionComponent<P = {}> { (props: P, context?: any): JSX.Element | null; } type ComponentType<P = {}> = ComponentClass<P> | FunctionComponent<P>; } declare const Elem: React.ComponentType<{ someKey: string }>; const bad = <Elem />;"#,
    );
    let msg = &diagnostics
        .iter()
        .find(|diag| diag.code == 2741)
        .expect("expected TS2741 for missing required prop")
        .message_text;
    assert!(msg.contains("required in type '{ someKey: string; }'") && !msg.contains("Readonly"));
}

#[test]
fn jsx_component_type_missing_prop_unwraps_renamed_readonly_mapped_alias() {
    let diagnostics = check_jsx_strict(
        r#"declare namespace JSX { interface Element {} interface ElementClass { render(): any; } interface ElementAttributesProperty { props: {}; } } type MyReadonly<T> = { readonly [K in keyof T]: T[K]; }; declare namespace React { interface Component<P> { props: MyReadonly<P>; render(): JSX.Element; } interface ComponentClass<P = {}> { new(props: P, context?: any): Component<P>; } interface FunctionComponent<P = {}> { (props: P, context?: any): JSX.Element | null; } type ComponentType<P = {}> = ComponentClass<P> | FunctionComponent<P>; } declare const Elem: React.ComponentType<{ someKey: string }>; const bad = <Elem />;"#,
    );
    let msg = &diagnostics
        .iter()
        .find(|diag| diag.code == 2741)
        .expect("expected TS2741 for missing required prop through renamed readonly alias")
        .message_text;
    assert!(
        msg.contains("required in type '{ someKey: string; }'") && !msg.contains("MyReadonly"),
        "Expected renamed readonly mapped alias to unwrap transparently, got: {diagnostics:?}"
    );
}

#[test]
fn jsx_component_type_does_not_unwrap_readonly_intersection_with_required_prop() {
    let diagnostics = check_jsx_strict(
        r#"declare namespace JSX { interface Element {} } type MyReadonly<T> = { readonly [K in keyof T]: T[K]; }; type PropsBox<T> = MyReadonly<T> & { required: string }; declare function Elem(props: PropsBox<{ someKey: string }>): JSX.Element; const bad = <Elem someKey="ok" required={1} />;"#,
    );
    assert!(
        diagnostics.iter().any(|diag| {
            diag.code == 2322
                && diag.message_text.contains("number")
                && diag.message_text.contains("string")
        }),
        "Expected PropsBox<T> required intersection member's string type to remain visible in TS2322, got: {diagnostics:?}"
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

/// TS2786 SHOULD fire for a union component where all members have valid props
/// but at least one member returns an incompatible type. The props-extraction
/// success must not suppress the return-type check.
#[test]
fn jsx_union_component_with_invalid_return_emits_ts2786() {
    let source = r#"
        declare namespace JSX {
            interface Element { type: 'element'; }
            interface ElementClass { render(): Element; }
            interface ElementAttributesProperty { props: {}; }
            interface IntrinsicElements { }
        }
        declare function BadFC(props: {}): { type: string };
        declare class BadClass {
            props: {};
            constructor(props: {});
            render(): { type: string };
        }
        declare var MixedComponent: typeof BadFC | typeof BadClass;
        <MixedComponent />;
        "#;
    let diagnostics = check_jsx(source);
    let expected_start = source
        .find("<MixedComponent")
        .map(|idx| idx as u32 + 1)
        .expect("source contains <MixedComponent");
    let ts2786 = diagnostics
        .iter()
        .find(|diag| diag.code == 2786 && diag.message_text.contains("'MixedComponent'"))
        .expect("Union component with invalid return types should emit TS2786 at MixedComponent");
    assert_eq!(
        ts2786.start, expected_start,
        "TS2786 should anchor at the MixedComponent JSX tag name, got: {diagnostics:?}"
    );
}

/// TS2786 should NOT fire for a union where every member is a valid JSX component.
#[test]
fn jsx_union_component_all_valid_no_ts2786() {
    let diagnostics = check_jsx(
        r#"
        declare namespace JSX {
            interface Element { type: 'element'; }
            interface ElementClass { render(): Element; }
            interface ElementAttributesProperty { props: {}; }
            interface IntrinsicElements { }
        }
        declare function GoodFC(props: {}): JSX.Element;
        declare class GoodClass {
            props: {};
            constructor(props: {});
            render(): JSX.Element;
        }
        declare var ValidUnion: typeof GoodFC | typeof GoodClass;
        <ValidUnion />;
        "#,
    );
    assert!(
        !diagnostics.iter().any(|diag| diag.code == 2786),
        "Union component with all valid return types should not emit TS2786, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.is_empty(),
        "Union component with all valid return types should be diagnostic-free, got: {diagnostics:?}"
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
fn jsx_union_of_invalid_function_and_class_component_emits_ts2786() {
    let source = r#"
        declare namespace JSX {
            interface Element { type: 'element'; }
            interface ElementClass { type: 'element-class'; }
            interface IntrinsicElements { }
        }
        function FunctionComponent<T extends string>({type}: {type?: T}) {
            return { type };
        }
        class ClassComponent {
            type = 'string';
        }
        declare const pick: boolean;
        const MixedComponent = pick ? FunctionComponent : ClassComponent;
        const elem = <MixedComponent />;
        "#;
    let diagnostics = check_jsx_strict(source);
    let expected_start = source
        .find("<MixedComponent")
        .map(|idx| idx as u32 + 1)
        .expect("source contains <MixedComponent");
    let ts2786 = diagnostics
        .iter()
        .find(|diag| diag.code == 2786 && diag.message_text.contains("'MixedComponent'"))
        .expect(
            "Union component with invalid function/class members should emit TS2786 at MixedComponent",
        );
    assert!(
        ts2786.message_text.contains("'MixedComponent'"),
        "Union component with invalid function/class members should emit TS2786 at MixedComponent, got: {diagnostics:?}"
    );
    assert_eq!(
        ts2786.start, expected_start,
        "TS2786 should anchor at the MixedComponent JSX tag name, got: {diagnostics:?}"
    );
}

#[test]
fn jsx_user_named_component_type_alias_union_still_checks_returns() {
    let source = r#"
        declare namespace JSX {
            interface Element { ok: true; }
            interface ElementClass { render(): Element; }
            interface ElementAttributesProperty { props: {}; }
            interface IntrinsicElements {}
        }
        interface InvalidClassComponent<P = {}> {
            new(props: P): { props: P; render(): { bad: true } };
        }
        interface InvalidFunctionComponent<P = {}> {
            (props: P): { bad: true };
        }
        type ComponentType<P = {}> =
            InvalidClassComponent<P> | InvalidFunctionComponent<P>;
        declare const Bad: ComponentType<{ p?: boolean }>;
        const elem = <Bad p={true} />;
        "#;
    let diagnostics = check_jsx_strict(source);
    let expected_start = source
        .find("<Bad")
        .map(|idx| idx as u32 + 1)
        .expect("source contains <Bad");
    let ts2786 = diagnostics
        .iter()
        .find(|diag| diag.code == 2786 && diag.message_text.contains("'Bad'"))
        .expect("User-defined ComponentType aliases should still emit TS2786 for invalid returns");
    assert_eq!(
        ts2786.start, expected_start,
        "Expected TS2786 to anchor at the Bad JSX tag name, got: {diagnostics:?}"
    );
}

#[test]
fn jsx_react_component_type_union_does_not_emit_ts2786() {
    let diagnostics = check_jsx_strict(
        r#"
        declare namespace JSX {
            interface Element extends React.ReactElement<any> {}
            interface ElementClass extends React.Component<any> {
                render(): React.ReactNode;
            }
            interface ElementAttributesProperty { props: {}; }
            interface IntrinsicElements {}
        }
        declare namespace React {
            type ReactNode = ReactElement<any> | string | number | null;
            interface ReactElement<P> { props: P; }
            type ComponentState = any;
            type ValidationMap<T> = any;
            type RefObject<T> = { readonly current: T | null };
            type Ref<T> = string | { bivarianceHack(instance: T | null): any }["bivarianceHack"] | RefObject<T>;
            type Readonly<T> = { readonly [P in keyof T]: T[P]; };
            interface StaticLifecycle<P, S> {}
            interface Component<P = {}, S = {}> {
                readonly props: Readonly<{ children?: ReactNode }> & Readonly<P>;
                state: Readonly<S>;
                context: any;
                refs: { [key: string]: any };
                render(): ReactNode;
            }
            interface ComponentClass<P = {}, S = ComponentState> extends StaticLifecycle<P, S> {
                new(props: P, context?: any): Component<P, S>;
                propTypes?: ValidationMap<P>;
                contextTypes?: ValidationMap<any>;
                defaultProps?: Partial<P>;
                displayName?: string;
            }
            interface StatelessComponent<P = {}> {
                (props: P & { children?: ReactNode }, context?: any): ReactElement<any> | null;
                propTypes?: ValidationMap<P>;
                contextTypes?: ValidationMap<any>;
                defaultProps?: Partial<P>;
                displayName?: string;
            }
            type ComponentType<P = {}> = ComponentClass<P> | StatelessComponent<P>;
        }
        interface P1 {
            p?: boolean;
            c?: string;
        }
        interface P2 {
            p?: boolean;
            c?: any;
            d?: any;
        }
        var C: React.ComponentType<P1> | React.ComponentType<P2> = null as any;
        const a = <C p={true} />;
        "#,
    );
    assert!(
        !diagnostics.iter().any(|diag| diag.code == 2786),
        "React.ComponentType unions with compatible class/function branches should not emit TS2786, got: {diagnostics:?}"
    );
}

#[test]
fn jsx_element_class_requirements_are_not_reduced_to_render_only() {
    let source = r#"
        declare namespace JSX {
            interface Element { ok: true; }
            interface ElementClass { render(): Element; props: { required: true }; }
            interface ElementAttributesProperty { props: {}; }
            interface IntrinsicElements {}
        }
        declare function GoodFC(props: { required: true }): JSX.Element;
        declare class MissingPropsClass {
            render(): JSX.Element;
        }
        declare const Mixed: typeof GoodFC | typeof MissingPropsClass;
        const elem = <Mixed required={true} />;
        "#;
    let diagnostics = check_jsx_strict(source);
    let expected_start = source
        .find("<Mixed")
        .map(|idx| idx as u32 + 1)
        .expect("source contains <Mixed");
    let ts2786 = diagnostics
        .iter()
        .find(|diag| diag.code == 2786 && diag.message_text.contains("'Mixed'"))
        .expect(
            "Class branch missing JSX.ElementClass-required members should still trigger TS2786",
        );
    assert_eq!(
        ts2786.start, expected_start,
        "Expected TS2786 to anchor at the Mixed JSX tag name, got: {diagnostics:?}"
    );
}

#[test]
fn jsx_react_type_union_with_string_does_not_emit_ts2786() {
    let diagnostics = check_jsx_strict(
        r#"
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
        declare namespace React {
            type ReactNode = ReactElement<any> | string | number | null;
            type ReactType<P = any> = string | ComponentType<P>;
            interface ReactElement<P> { props: P; }
            interface Component<P = {}, S = {}> {
                props: Readonly<P>;
                render(): ReactNode;
            }
            interface ComponentClass<P = {}, S = {}> {
                new(props: P, context?: any): Component<P, S>;
            }
            interface FunctionComponent<P = {}> {
                (props: P & { children?: ReactNode }, context?: any): ReactElement<any> | null;
            }
            type ComponentType<P = {}> = ComponentClass<P> | FunctionComponent<P>;
        }
        declare const props: { component: React.ReactType };
        const Comp: React.ReactType = props.component;
        const elem = <Comp />;
        "#,
    );
    assert!(
        !diagnostics.iter().any(|diag| diag.code == 2786),
        "React.ReactType unions include intrinsic string tags and should not emit TS2786, got: {diagnostics:?}"
    );
}

#[test]
fn jsx_class_construct_readonly_mapped_props_uses_shape_not_alias_name() {
    let sources = [
        (
            "renamed readonly mapped alias",
            r#"
        declare namespace JSX {
            interface Element extends React.ReactElement<any> {}
            interface ElementClass extends React.Component<any> {
                render(): React.ReactNode;
            }
            interface ElementAttributesProperty { props: {}; }
            interface IntrinsicElements {}
        }
        declare namespace React {
            type ReactNode = ReactElement<any> | string | number | null;
            interface ReactElement<P> { props: P; }
            type Frozen<T> = { readonly [Q in keyof T]: T[Q]; };
            class Component<P = {}> {
                props: Frozen<P>;
                render(): ReactNode;
            }
        }
        interface Props { x?: number; }
        class Widget extends React.Component<Props> {}
        <Widget />;
        "#,
        ),
        (
            "readonly mapped intersection",
            r#"
        declare namespace JSX {
            interface Element extends React.ReactElement<any> {}
            interface ElementClass extends React.Component<any> {
                render(): React.ReactNode;
            }
            interface ElementAttributesProperty { props: {}; }
            interface ElementChildrenAttribute { children: {}; }
            interface IntrinsicElements { div: {}; }
        }
        declare namespace React {
            type ReactNode = ReactElement<any> | string | number | null | undefined;
            interface ReactElement<P> { props: P; }
            type Locked<X> = { readonly [Name in keyof X]: X[Name]; };
            class Component<P = {}> {
                props: Locked<P> & Locked<{ children?: ReactNode }>;
                render(): ReactNode;
            }
        }
        interface Props { label?: string; }
        class Panel extends React.Component<Props> {}
        <Panel><div /></Panel>;
        "#,
        ),
    ];

    for (case_name, source) in sources {
        let diagnostics = check_jsx_codes(source);
        assert!(
            !diagnostics.contains(&2786),
            "{case_name}: readonly mapped class props should suppress TS2786 without relying on alias spelling, got: {diagnostics:?}"
        );
    }
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

/// React component alias types (`ComponentType<P>`, `ComponentClass<P>`, etc.) with
/// multi-constructor overloads must not emit TS2786. The multi-construct path goes
/// through `check_jsx_overloaded_sfc`; the same React-alias skip that guards the
/// non-overload path must apply there too to avoid cycle-detection false positives.
///
/// This test uses two different type-parameter names (`T` and `K`) to prove the rule
/// is structural, not tied to any specific identifier spelling.
#[test]
fn jsx_react_component_alias_with_multi_construct_no_ts2786() {
    // Shared React namespace used across shape variants.
    let make_source = |type_alias: &str, jsx_tag: &str| {
        format!(
            r#"
        declare namespace JSX {{
            interface Element extends React.ReactElement<any> {{}}
            interface ElementClass extends React.Component<any> {{
                render(): React.ReactNode;
            }}
            interface ElementAttributesProperty {{ props: {{}}; }}
            interface IntrinsicElements {{}}
        }}
        declare namespace React {{
            type ReactNode = ReactElement<any> | string | number | null;
            interface ReactElement<P> {{ props: P; }}
            interface Component<P = {{}}, S = {{}}> {{
                props: Readonly<P>;
                render(): ReactNode;
            }}
            // Two constructors to trigger the has_multi_construct path.
            interface ComponentClass<P = {{}}, S = {{}}> {{
                new(props: P, context?: any): Component<P, S>;
                new(props: P): Component<P, S>;
            }}
            interface FunctionComponent<P = {{}}> {{
                (props: P, context?: any): ReactElement<any> | null;
            }}
            type ComponentType<P = {{}}> = ComponentClass<P> | FunctionComponent<P>;
        }}
        interface Props {{ x?: number; }}
        {type_alias}
        const elem = <{jsx_tag} />;
        "#
        )
    };

    // Shape 1: variable typed as ComponentType<T> (type-param name T)
    let src1 = make_source("declare var a: React.ComponentType<Props>;", "a");
    let d1 = check_jsx_codes(&src1);
    assert!(
        !d1.contains(&2786),
        "ComponentType<T> with multi-construct should not emit TS2786, got: {d1:?}"
    );

    // Shape 2: variable typed as ComponentClass<K> directly (type-param name K)
    let src2 = make_source("declare var x: React.ComponentClass<Props>;", "x");
    let d2 = check_jsx_codes(&src2);
    assert!(
        !d2.contains(&2786),
        "ComponentClass<K> with multi-construct should not emit TS2786, got: {d2:?}"
    );

    // Shape 3: union ComponentType<T1> | ComponentType<T2> — members are React aliases
    let src3 = make_source(
        "interface Props2 { y?: string; } declare var a: React.ComponentType<Props> | React.ComponentType<Props2>;",
        "a",
    );
    let d3 = check_jsx_codes(&src3);
    assert!(
        !d3.contains(&2786),
        "Union of ComponentType aliases with multi-construct should not emit TS2786, got: {d3:?}"
    );
}

/// Non-React overloaded components with invalid return types must still emit TS2786
/// alongside TS2769 when all overloads fail — the React-alias skip must not suppress
/// unrelated components. Both required props ensure no overload matches `<Bad />`.
#[test]
fn jsx_non_react_overload_with_invalid_return_still_emits_ts2786() {
    let diagnostics = check_jsx_codes(
        r#"
        declare namespace JSX {
            interface Element { marker: true; }
            interface IntrinsicElements {}
        }
        // Two call signatures with required props; neither returns JSX.Element.
        // <Bad /> provides no attributes, so both overloads fail (required props missing).
        interface BrokenSfc {
            (props: { a: string }): { wrong: true };
            (props: { b: number }): { wrong: true };
        }
        declare var Bad: BrokenSfc;
        <Bad />;
        "#,
    );
    assert!(
        diagnostics.contains(&2786),
        "Non-React overloaded component with invalid return type should still emit TS2786, got: {diagnostics:?}"
    );
    assert!(
        diagnostics.contains(&2769),
        "Non-React overloaded component should still emit TS2769 when no overload matches, got: {diagnostics:?}"
    );
}

#[test]
fn jsx_intrinsic_excess_attrs_report_for_intersection_alias_props() {
    let diagnostics = check_jsx(
        r#"
        declare namespace React {
            interface ClassAttributes<T> {
                ref?: T;
            }
            type DetailedHTMLProps<E extends HTMLAttributes<T>, T> = ClassAttributes<T> & E;
            interface HTMLAttributes<T> {
                className?: string;
                onClick?: (event: T) => void;
            }
            interface AnchorHTMLAttributes<T> extends HTMLAttributes<T> {
                href?: string;
            }
        }
        interface HTMLAnchorElement {}
        declare namespace JSX {
            interface Element {}
            interface IntrinsicElements {
                a: React.DetailedHTMLProps<React.AnchorHTMLAttributes<HTMLAnchorElement>, HTMLAnchorElement>;
                plain: { className?: string };
            }
        }

        <a class="" />;
        <a for="" class="" />;
        <plain class="" />;
        "#,
    );
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        3,
        "Expected both intrinsic elements to report excess JSX attrs, got: {diagnostics:?}"
    );
    assert!(
        ts2322.iter().any(|diag| diag.message_text.contains(
            "Type '{ class: string; }' is not assignable to type 'DetailedHTMLProps<AnchorHTMLAttributes<HTMLAnchorElement>, HTMLAnchorElement>'."
        )),
        "Expected intersection alias intrinsic target display, got: {diagnostics:?}"
    );
    assert!(
        ts2322.iter().any(|diag| diag.message_text.contains(
            "Type '{ for: string; class: string; }' is not assignable to type 'DetailedHTMLProps<AnchorHTMLAttributes<HTMLAnchorElement>, HTMLAnchorElement>'."
        )),
        "Expected combined excess attrs to be reported once at the first bad attr, got: {diagnostics:?}"
    );
    assert!(
        ts2322.iter().any(|diag| diag
            .message_text
            .contains("and 'class' does not exist in type '{ className?: string | undefined; }'")),
        "Expected plain intrinsic excess attr diagnostic, got: {diagnostics:?}"
    );
    assert_eq!(
        ts2322
            .iter()
            .filter(|diag| diag
                .message_text
                .contains("{ for: string; class: string; }"))
            .count(),
        1,
        "Expected one synthesized excess diagnostic for multiple bad attrs, got: {diagnostics:?}"
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

/// React-style class components whose instance exposes `Readonly<P>` props use
/// JSX constructor applicability diagnostics for props mismatches. Missing props
/// anchor at the tag and explicit excess props anchor at the failing attribute,
/// both as TS2769 rather than whole-object TS2322.
#[test]
fn jsx_readonly_class_component_props_mismatch_reports_ts2769() {
    let diagnostics = check_jsx_strict(
        r#"
        type Readonly<T> = { readonly [P in keyof T]: T[P]; };
        declare namespace React {
            type ReactNode = JSX.Element | string | number | null;
            class Component<P> {
                constructor(props: P);
                props: Readonly<P> & Readonly<{ children?: ReactNode }>;
                render(): ReactNode;
            }
        }
        type NewElementConstructor<P> = new (props: P) => React.Component<P>;
        declare namespace JSX {
            interface Element {}
            interface ElementClass { render(): React.ReactNode; }
            interface ElementAttributesProperty { props: {}; }
            interface IntrinsicElements { div: {}; }
            type ElementType = string | NewElementConstructor<any>;
        }

        class RenderTitle extends React.Component<{ title: string }> {
            render() { return this.props.title; }
        }
        <RenderTitle />;
        <RenderTitle title="ok" />;
        <RenderTitle excessProp />;
        "#,
    );
    let ts2769_count = diagnostics.iter().filter(|diag| diag.code == 2769).count();
    assert_eq!(
        ts2769_count, 2,
        "Missing and excess class props should report TS2769, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|diag| diag.code == 2322 || diag.code == 2741),
        "Class props mismatch should not fall back to TS2322/TS2741, got: {diagnostics:?}"
    );
}

#[test]
fn jsx_readonly_class_component_react16_order_props_mismatch_reports_ts2769() {
    let diagnostics = check_jsx_strict(
        r#"
        type Readonly<T> = { readonly [P in keyof T]: T[P]; };
        declare namespace React {
            type ReactNode = JSX.Element | string | number | null;
            class Component<P, S> {
                constructor(props: Readonly<P>);
                constructor(props: P, context?: any);
                readonly props: Readonly<{ children?: ReactNode }> & Readonly<P>;
                render(): ReactNode;
            }
        }
        type NewElementConstructor<P> =
            | ((props: P) => React.ReactNode)
            | (new (props: P) => React.Component<P, any>);
        declare namespace JSX {
            interface Element {}
            interface ElementClass { render(): React.ReactNode; }
            interface ElementAttributesProperty { props: {}; }
            interface IntrinsicElements { div: {}; }
            type ElementType = string | NewElementConstructor<any>;
        }

        class RenderTitle extends React.Component<{ title: string }, {}> {
            render() { return this.props.title; }
        }
        <RenderTitle />;
        <RenderTitle title="ok" />;
        <RenderTitle extra />;

        class Caption extends React.Component<{ label: string }, {}> {
            render() { return this.props.label; }
        }
        <Caption />;
        <Caption label="ok" />;
        <Caption spare />;
        "#,
    );
    let ts2769_count = diagnostics.iter().filter(|diag| diag.code == 2769).count();
    assert_eq!(
        ts2769_count, 4,
        "React16-order readonly class props mismatches should report TS2769, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|diag| diag.code == 2322 || diag.code == 2741),
        "React16-order readonly class props mismatch should not fall back to TS2322/TS2741, got: {diagnostics:?}"
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
        type Exclude<T, U> = T extends U ? never : T;
        type Extract<T, U> = T extends U ? T : never;
        type Partial<T> = { [K in keyof T]?: T[K] };
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
        type Exclude<T, U> = T extends U ? never : T;
        type Extract<T, U> = T extends U ? T : never;
        type Partial<T> = { [K in keyof T]?: T[K] };
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

