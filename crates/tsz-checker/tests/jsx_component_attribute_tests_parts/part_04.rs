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
    // Simple case: props type is just T (type parameter). tsc still reports
    // TS2322 here because `{ x: 1, y: "blah" }` is not assignable to arbitrary
    // `T`, even though the excess-property-specific path is suppressed.
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
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should emit TS2322 for bare type-parameter props, got: {diags:?}"
    );
}

#[test]
fn test_explicit_generic_jsx_props_alias_preserves_callback_member() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Elements {{
    foo: {{ callback?: (value: number) => void }};
    bar: {{ callback?: (value: string) => void }};
}}

type Props<C extends keyof Elements> = {{ as?: C }} & Elements[C];
declare function Test<C extends keyof Elements>(props: Props<C>): null;

<Test<'bar'>
    as="bar"
    callback={{value => value.toUpperCase()}}
/>;
"#
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "explicit generic JSX props alias should not lose the callback member, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
        ),
        "explicit generic JSX props alias should not report excess-property errors for callback, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "callback parameter should remain contextually typed through the JSX props alias, got: {diags:?}"
    );
}

#[test]
fn test_inferred_generic_jsx_props_alias_preserves_callback_member() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Elements {{
    foo: {{ callback?: (value: number) => void }};
    bar: {{ callback?: (value: string) => void }};
}}

type Props<C extends keyof Elements> = {{ as?: C }} & Elements[C];
declare function Test<C extends keyof Elements>(props: Props<C>): null;

<Test
    as="bar"
    callback={{value => value.toUpperCase()}}
/>;
"#
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "inferred generic JSX props alias should not lose the callback member, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
        ),
        "inferred generic JSX props alias should not report excess-property errors for callback, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "callback parameter should remain contextually typed through inferred JSX props alias, got: {diags:?}"
    );
}

#[test]
fn test_generic_jsx_defaulted_props_contextually_type_callback_attrs() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
type Tag = "a" | "button";
type Props<T extends Tag = "a"> = {{
    as?: T;
}} & (T extends "a"
    ? {{ onClick?: (e: {{ href: string }}) => void }}
    : {{ onClick?: (e: {{ disabled: boolean }}) => void }});

declare function UnwrappedLink<T extends Tag = "a">(props: Props<T>): null;

<UnwrappedLink onClick={{(e) => e.href}} />;
<UnwrappedLink as="button" onClick={{(e) => e.disabled}} />;
"#
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "defaulted generic JSX props should contextually type callback attrs, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "defaulted generic JSX props should keep callback members assignable, got: {diags:?}"
    );
}

#[test]
fn test_generic_jsx_defaulted_conditional_props_contextually_type_callback_attrs() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
type ElementType = "a" | "button";
type Exclude<T, U> = T extends U ? never : T;
type Pick<T, K extends keyof T> = {{ [P in K]: T[P] }};
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;
type ComponentPropsWithRef<T extends ElementType> =
    T extends "a"
        ? {{ as?: never; onClick?: (e: {{ href: string }}) => void }}
        : {{ as?: never; onClick?: (e: {{ disabled: boolean }}) => void }};

declare function UnwrappedLink<T extends ElementType = ElementType>(
    props: Omit<ComponentPropsWithRef<ElementType extends T ? "a" : T>, "as">,
): null;

declare function UnwrappedLink2<T extends ElementType = ElementType>(
    props: Omit<ComponentPropsWithRef<ElementType extends T ? "a" : T>, "as"> & {{ as?: T }},
): null;

<UnwrappedLink onClick={{(e) => e.href}} />;
<UnwrappedLink2 onClick={{(e) => e.href}} />;
<UnwrappedLink2 as="button" onClick={{(e) => e.disabled}} />;
"#
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "defaulted conditional generic JSX props should contextually type callback attrs, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "defaulted conditional generic JSX props should keep callback members assignable, got: {diags:?}"
    );
}

#[test]
fn test_generic_jsx_react_style_defaulted_conditional_props_contextually_type_callback_attrs() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare namespace React {{
    interface ReactElement<T = any> {{}}
    interface Element {{}}
    interface AnchorElement extends Element {{ href: string; }}
    interface ButtonElement extends Element {{ disabled: boolean; }}
    interface MouseEvent<T = Element> {{ currentTarget: T; }}
    type EventHandler<E> = {{ bivarianceHack(event: E): void }}["bivarianceHack"];
    type MouseEventHandler<T = Element> = EventHandler<MouseEvent<T>>;

    interface JSXElementConstructor<P> {{
        (props: P): ReactElement<any> | null;
    }}
    interface ComponentType<P = {{}}> {{
        (props: P): ReactElement<any> | null;
    }}
    type ElementType<P = any> =
        {{
            [K in keyof JSX.IntrinsicElements]: P extends JSX.IntrinsicElements[K] ? K : never
        }}[keyof JSX.IntrinsicElements]
        | ComponentType<P>;

    type PropsWithRef<P> = P;
    type ComponentProps<T extends keyof JSX.IntrinsicElements | JSXElementConstructor<any>> =
        T extends JSXElementConstructor<infer P>
            ? P
            : T extends keyof JSX.IntrinsicElements
                ? JSX.IntrinsicElements[T]
                : {{}};
    type ComponentPropsWithRef<T extends ElementType> = PropsWithRef<ComponentProps<T>>;
}}

declare namespace JSX {{
    interface IntrinsicElements {{
        a: {{ onClick?: React.MouseEventHandler<React.AnchorElement> }};
        button: {{ onClick?: React.MouseEventHandler<React.ButtonElement> }};
        div: any;
        span: any;
    }}
}}

type Exclude<T, U> = T extends U ? never : T;
type Pick<T, K extends keyof T> = {{ [P in K]: T[P] }};
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;

declare function UnwrappedLink<T extends React.ElementType = React.ElementType>(
    props: Omit<React.ComponentPropsWithRef<React.ElementType extends T ? "a" : T>, "as">,
): null;

declare function UnwrappedLink2<T extends React.ElementType = React.ElementType>(
    props: Omit<React.ComponentPropsWithRef<React.ElementType extends T ? "a" : T>, "as"> & {{ as?: T }},
): null;

<UnwrappedLink onClick={{(e) => e.currentTarget.href}} />;
<UnwrappedLink2 onClick={{(e) => e.currentTarget.href}} />;
<UnwrappedLink2 as="button" onClick={{(e) => e.currentTarget.disabled}} />;
"#
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "React-style defaulted conditional generic JSX props should contextually type callback attrs, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "React-style defaulted conditional generic JSX props should keep callback members assignable, got: {diags:?}"
    );
}

#[test]
fn test_generic_jsx_props_conditional_component_props_with_ref_keeps_callback_context() {
    let lib_source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        a: { onClick?: (e: { anchorOnly: true }) => void; href?: string };
        button: { onClick?: (e: { buttonOnly: true }) => void; disabled?: boolean };
    }
}

declare namespace React {
    type ElementType = keyof JSX.IntrinsicElements;
    type ComponentPropsWithRef<T extends ElementType> = JSX.IntrinsicElements[T] & { as?: T };
}

declare module "react" {
    export default React;
    export type ElementType = React.ElementType;
    export type ComponentPropsWithRef<T extends React.ElementType> = React.ComponentPropsWithRef<T>;
}
"#;

    let main_source = r#"
type Exclude<T, U> = T extends U ? never : T;
type Pick<T, K extends keyof T> = { [P in K]: T[P] };
type Omit<T, K extends keyof any> = Pick<T, Exclude<keyof T, K>>;

import React from "react";
import { ComponentPropsWithRef, ElementType } from "react";

function UnwrappedLink<T extends ElementType = ElementType>(
  props: Omit<ComponentPropsWithRef<ElementType extends T ? "a" : T>, "as">,
) {
  return <a></a>;
}

<UnwrappedLink onClick={(e) => e.anchorOnly} />;

function UnwrappedLink2<T extends ElementType = ElementType>(
  props: Omit<ComponentPropsWithRef<ElementType extends T ? "a" : T>, "as"> & {
    as?: T;
  },
) {
  return <a></a>;
}

<UnwrappedLink2 onClick={(e) => e.anchorOnly} />;
<UnwrappedLink2 as="button" onClick={(e) => e.buttonOnly} />;
"#;

    let diags = cross_file_jsx_diagnostics(lib_source, main_source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "conditional ComponentPropsWithRef generic JSX props should contextually type callback params, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "conditional ComponentPropsWithRef generic JSX props should preserve callback member access, got: {diags:?}"
    );
}

#[test]
fn test_contextually_typed_jsx_attribute2_react16_fixture_has_no_ts2322() {
    let Some(react_types) = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts") else {
        return;
    };
    let Some(source) = load_typescript_fixture(
        "TypeScript/tests/cases/compiler/contextuallyTypedJsxAttribute2.tsx",
    ) else {
        return;
    };

    let diags = cross_file_jsx_diagnostics_with_mode(&react_types, &source, JsxMode::Preserve);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "real react16 fixture should not emit TS2322, got: {diags:?}"
    );
}

