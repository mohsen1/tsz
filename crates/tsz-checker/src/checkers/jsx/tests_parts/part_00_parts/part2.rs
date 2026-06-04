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
