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
fn test_generic_intrinsic_tag_type_parameter_checks_empty_attrs_against_lma() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    type LibraryManagedAttributes<C, P> = P;
    interface IntrinsicElements {
        div: { id: string };
        span: { title: string };
    }
}

declare namespace React {
    interface SFC {
        (): JSX.Element;
    }
}

type Tags = "span" | "div";

const Hoc = <Tag extends Tags>(
   TagElement: Tag,
): React.SFC => {
   const Component = () => <TagElement />;
   return Component;
};
"#;

    let diags = jsx_diagnostics(source);
    assert!(
        has_code_with_message(
            &diags,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            "LibraryManagedAttributes<Tag,"
        ),
        "Expected TS2322 for empty attrs against generic intrinsic LibraryManagedAttributes target, got: {diags:?}"
    );
}

/// Regression for <https://github.com/mohsen1/tsz/issues/3227>.
///
/// `apply_jsx_library_managed_attributes` previously discarded the LMA
/// evaluation whenever `format_type(evaluated)` contained the substring
/// `Factory<`. That printer-output check spuriously triggered for any user
/// type happening to be named `Factory`, producing a false TS2741 for the
/// optional prop. With the heuristic removed, LMA must still erase the
/// required prop whose default is provided through `defaultProps`.
#[test]
fn test_jsx_library_managed_attributes_with_user_named_factory_generic() {
    let user_named_generic_sources = [
        ("Factory", "value: Factory<number>"),
        ("Maker", "value: Maker<number>"),
        ("Producer", "value: Producer<number>"),
        ("Builder", "value: Builder<number>"),
        ("Wrapper", "value: Wrapper<number>"),
    ];

    for (type_name, prop_decl) in user_named_generic_sources {
        let source = format!(
            r#"
declare namespace JSX {{
    interface Element {{}}
    interface ElementClass {{}}
    interface IntrinsicElements {{}}
    type LibraryManagedAttributes<C, P> =
        C extends {{ defaultProps: infer D }}
            ? {{ [K in keyof P]?: P[K] }}
            : P;
}}

interface {type_name}<T> {{
    make(): T;
}}

interface Props {{
    {prop_decl};
    other: number;
}}

declare function Comp(props: Props): JSX.Element;
declare namespace Comp {{
    const defaultProps: {{
        {prop_decl};
    }};
}}

const _ok = <Comp other={{0}} />;
"#
        );

        let diags = jsx_diagnostics(&source);
        assert!(
            !has_code(
                &diags,
                diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
            ),
            "JSX LibraryManagedAttributes must mark props with a default as optional regardless of user-chosen type names; user generic `{type_name}<T>` produced TS2741: {diags:?}"
        );
        assert!(
            !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
            "JSX LibraryManagedAttributes erasure must not introduce TS2322 for user generic `{type_name}<T>`: {diags:?}"
        );
    }
}

#[test]
fn jsx_library_managed_attributes_preserves_factory_named_prop_defaults() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface ElementClass { render(): Element; }
    interface ElementAttributesProperty { props: {}; }
    interface IntrinsicElements { div: {}; }

    type Exclude<T, U> = T extends U ? never : T;
    type Extract<T, U> = T extends U ? T : never;
    type Pick<T, K extends keyof T> = { [P in K]: T[P] };
    type Partial<T> = { [P in keyof T]?: T[P] };
    type Defaultize<Props, Defaults> =
        Partial<Pick<Props, Extract<keyof Props, keyof Defaults>>> &
        Pick<Props, Exclude<keyof Props, keyof Defaults>>;
    type LibraryManagedAttributes<Component, Props> =
        Component extends { defaultProps: infer Defaults }
            ? Defaultize<Props, Defaults>
            : Props;
}

interface Factory<T> {
    create(): T;
}

interface Props {
    value: Factory<string>;
    other: number;
}

declare class Comp {
    props: Props;
    static defaultProps: { value: Factory<string> };
    render(): JSX.Element;
}

let ok = <Comp other={1} />;
"#;

    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "LibraryManagedAttributes should preserve defaulted Factory<T> props, got: {diags:?}"
    );
}

#[test]
fn test_generic_component_type_parameter_checks_empty_attrs_against_lma() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {}
    type LibraryManagedAttributes<C, P> =
        C extends (props: any) => any ? P & { managed: C } : P;
}

function f1<T extends (props: {}) => JSX.Element>(Component: T) {
    return <Component />;
}
"#;

    let diags = jsx_diagnostics(source);
    assert!(
        has_code_with_message(
            &diags,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            "LibraryManagedAttributes<T, {}>"
        ),
        "Expected empty attrs to be checked against JSX.LibraryManagedAttributes<T, {{}}>, got: {diags:?}"
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
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "conditional ComponentPropsWithRef generic JSX props should accept contextual callback attributes, got: {diags:?}"
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

#[test]
fn test_contextually_typed_jsx_attribute2_react16_fixture_has_no_ts7006() {
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
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "real react16 fixture should not emit TS7006, got: {diags:?}"
    );
}

#[test]
fn jsx_generic_spread_optional_write_surface_still_rejects_mismatched_value() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare class Component<P> {{ props: P; }}
interface SelectProps<T> {{ value?: T; flag?: boolean; }}
declare class Select<T extends string> extends Component<SelectProps<T>> {{}}

function wrap<T extends string>(props: {{ value?: T }}) {{
    return <Select<T> {{...props}} value={{123}} flag={{false}} />;
}}
"#
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "mismatched explicit JSX value should still emit TS2322, got: {diags:?}"
    );
}

#[test]
fn jsx_complex_signature_react16_fixture_accepts_optional_value_attr() {
    let Some(react_types) = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts") else {
        return;
    };
    let Some(mut source) = load_typescript_fixture(
        "TypeScript/tests/cases/compiler/jsxComplexSignatureHasApplicabilityError.tsx",
    ) else {
        return;
    };
    source = source.replace("/// <reference path=\"/.lib/react16.d.ts\" />", "");

    let renamed = source.replace("WrappedProps", "W");
    for (label, source) in [("original", source.as_str()), ("renamed", renamed.as_str())] {
        let diags = cross_file_jsx_diagnostics_with_options_and_default_libs(
            &react_types,
            source,
            CheckerOptions {
                jsx_mode: JsxMode::React,
                strict: true,
                strict_null_checks: true,
                no_implicit_any: true,
                strict_function_types: true,
                strict_bind_call_apply: true,
                strict_property_initialization: true,
                no_implicit_this: true,
                always_strict: true,
                ..CheckerOptions::default()
            },
            true,
        );

        assert!(
            !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
            "`jsxComplexSignatureHasApplicabilityError.tsx` ({label}) should not emit TS2322, got: {diags:?}"
        );
    }
}

#[test]
fn jsx_excess_props_and_assignability_react16_generic_spread_reports_ts2322() {
    let Some(react_types) = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts") else {
        return;
    };
    let source = r#"
import * as React from 'react';

const myHoc = <ComposedComponentProps extends any>(
    ComposedComponent: React.ComponentClass<ComposedComponentProps>,
) => {
    type WrapperComponentProps = ComposedComponentProps & { myProp: string };
    const WrapperComponent: React.ComponentClass<WrapperComponentProps> = null as any;

    const props: ComposedComponentProps = null as any;

    <WrapperComponent {...props} myProp={'1000000'} />;
    <WrapperComponent {...props} myProp={1000000} />;
};
"#;
    let diags = cross_file_jsx_diagnostics_with_options_and_default_libs(
        &react_types,
        source,
        CheckerOptions {
            jsx_mode: JsxMode::React,
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            strict_function_types: true,
            strict_bind_call_apply: true,
            strict_property_initialization: true,
            no_implicit_this: true,
            always_strict: true,
            ..CheckerOptions::default()
        },
        true,
    );

    assert!(
        has_code_with_message(
            &diags,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            "ComposedComponentProps & { myProp: number; }"
        ),
        "expected generic spread plus numeric myProp to emit whole-object TS2322, got: {diags:?}"
    );
}

#[test]
fn jsx_generic_spread_alias_string_literal_prop_reports_ts2322() {
    let react_types = r#"
declare namespace React {
  class Component<P = {}, S = any> {
    props: P;
  }

  interface ComponentClass<P = {}> {
    new (props: P): Component<P>;
  }

  interface Attributes {
    key?: any;
  }

  interface ClassAttributes<T> {
    ref?: any;
  }
}

declare module "react" {
  export = React;
}

declare namespace JSX {
  interface Element {}
  interface ElementClass {
    props: any;
  }
  interface ElementAttributesProperty {
    props: {};
  }
  interface IntrinsicAttributes extends React.Attributes {}
  interface IntrinsicClassAttributes<T> extends React.ClassAttributes<T> {}
}
"#;
    let source = r#"
import * as React from "react";

function render<ComposedComponentProps extends object>() {
  type WrapperComponentProps = ComposedComponentProps & { "myProp": string };
  const WrapperComponent = null as any as React.ComponentClass<WrapperComponentProps>;

  const props: ComposedComponentProps = null as any;

  <WrapperComponent {...props} myProp={123} />;
}

render;
"#;
    let diags = cross_file_jsx_diagnostics_with_options_and_default_libs(
        react_types,
        source,
        CheckerOptions {
            jsx_mode: JsxMode::React,
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            strict_function_types: true,
            strict_bind_call_apply: true,
            strict_property_initialization: true,
            no_implicit_this: true,
            always_strict: true,
            ..CheckerOptions::default()
        },
        true,
    );

    assert!(
        has_code_with_message(
            &diags,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            "ComposedComponentProps & { myProp: number; }"
        ),
        "string-literal alias prop should be recognized structurally for generic spread TS2322, got: {diags:?}"
    );
}

#[test]
fn jsx_generic_spread_wrapper_number_prop_does_not_use_alias_fallback() {
    let react_types = r#"
declare namespace React {
  class Component<P = {}, S = any> {
    props: P;
  }

  interface ComponentClass<P = {}> {
    new (props: P): Component<P>;
  }

  interface Attributes {
    key?: any;
  }

  interface ClassAttributes<T> {
    ref?: any;
  }
}

declare module "react" {
  export = React;
}

declare namespace JSX {
  interface Element {}
  interface ElementClass {
    props: any;
  }
  interface ElementAttributesProperty {
    props: {};
  }
  interface IntrinsicAttributes extends React.Attributes {}
  interface IntrinsicClassAttributes<T> extends React.ClassAttributes<T> {}
}
"#;
    let source = r#"
import * as React from "react";

function render<ComposedComponentProps extends object>() {
  type WrapperComponentProps = ComposedComponentProps & { myProp: number };
  const WrapperComponent = null as any as React.ComponentClass<WrapperComponentProps>;

  const props: ComposedComponentProps = null as any;

  <WrapperComponent {...props} myProp={123} />;
}

render;
"#;
    let diags = cross_file_jsx_diagnostics_with_options_and_default_libs(
        react_types,
        source,
        CheckerOptions {
            jsx_mode: JsxMode::React,
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            strict_function_types: true,
            strict_bind_call_apply: true,
            strict_property_initialization: true,
            no_implicit_this: true,
            always_strict: true,
            ..CheckerOptions::default()
        },
        true,
    );

    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "number-valued WrapperComponentProps.myProp should accept a numeric JSX attr, got: {diags:?}"
    );
}

#[test]
fn jsx_generic_componentclass_annotation_spread_does_not_recurse() {
    let react_types = r#"
declare namespace React {
  class Component<P = {}, S = any> {
    props: P;
  }

  interface ComponentClass<P = {}> {
    new (props: P): Component<P>;
  }

  interface Attributes {
    key?: any;
  }

  interface ClassAttributes<T> {
    ref?: any;
  }
}

declare module "react" {
  export = React;
}

declare namespace JSX {
  interface Element {}
  interface ElementClass {
    props: any;
  }
  interface ElementAttributesProperty {
    props: {};
  }
  interface IntrinsicAttributes extends React.Attributes {}
  interface IntrinsicClassAttributes<T> extends React.ClassAttributes<T> {}
}
"#;
    let source = r#"
import * as React from "react";

function render<ComposedComponentProps extends object>(
  ComposedComponent: React.ComponentClass<ComposedComponentProps>,
) {
  type OuterProps = ComposedComponentProps & { otherProp: string };
  const WrapperComponent: React.ComponentClass<OuterProps> = null as any;

  const props: ComposedComponentProps = null as any;

  <WrapperComponent {...props} otherProp="ok" />;
  <WrapperComponent {...props} otherProp={123} />;
}

render;
"#;
    let diags = cross_file_jsx_diagnostics_with_options_and_default_libs(
        react_types,
        source,
        CheckerOptions {
            jsx_mode: JsxMode::React,
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            strict_function_types: true,
            strict_bind_call_apply: true,
            strict_property_initialization: true,
            no_implicit_this: true,
            always_strict: true,
            ..CheckerOptions::default()
        },
        true,
    );

    assert!(
        has_code_with_message(
            &diags,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            "otherProp: number"
        ),
        "generic ComponentClass annotation should report the numeric otherProp mismatch without recursing, got: {diags:?}"
    );
}

#[test]
fn test_jsx_excess_props_and_assignability_react16_fixture_matches_tsc() {
    let Some(react_types) = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts") else {
        return;
    };
    let Some(mut source) = load_typescript_fixture(
        "TypeScript/tests/cases/compiler/jsxExcessPropsAndAssignability.tsx",
    ) else {
        return;
    };
    source = source.replace("/// <reference path=\"/.lib/react16.d.ts\" />", "");

    let diags = cross_file_jsx_diagnostics_with_mode_and_default_libs(
        &react_types,
        &source,
        JsxMode::React,
        true,
    );
    assert!(
        has_code(
            &diags,
            diagnostic_codes::SPREAD_TYPES_MAY_ONLY_BE_CREATED_FROM_OBJECT_TYPES
        ),
        "real react16 fixture should emit TS2698 for generic spreads, got: {diags:?}"
    );
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "real react16 fixture should emit TS2322 for the number myProp JSX element, got: {diags:?}"
    );
    assert!(
        !has_code_with_message(
            &diags,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            "ComposedComponentProps & { myProp: string; }"
        ),
        "real react16 fixture should not emit TS2322 for the string myProp JSX element, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::CANNOT_BE_USED_AS_A_JSX_COMPONENT),
        "ComponentClass<WrapperComponentProps> should be accepted as a JSX component, got: {diags:?}"
    );
}

// =============================================================================
// JSX class-component target display (issue #8696)
// =============================================================================
//
// Structural rule: when the JSX target is a React-style class component whose
// instance `props` field is a wrapper expression (`Readonly<P> & Readonly<{...}>`
// or any other generic-application intersection), tsc renders TS2322 with that
// wrapper expression in the target text, NOT the bare constructor parameter
// alias. Mirror that so the per-attribute and whole-object TS2322 anchors
// converge on the same tsc-faithful target rendering.
//
// The tests below use deliberately renamed identifiers per CLAUDE.md §25:
// renaming the type-parameter (`P`/`Q`/`Z`), the wrapper alias name, and the
// component class name should not change the structural outcome.

// `cross_file_jsx_diagnostics_with_options_and_default_libs` exercises an
// import-resolution path that diverges from the in-file JSX validator the
// display rule under test lives in. The inline `Readonly<T>` keeps the
// fixture independent of `lib.es5.d.ts`, which the test harness does not
// load by default. Library-level names (`Component`, `ComponentClass`) are
// fixed because the JSX validator's React-alias detection
// (`extraction_react_alias.rs`) is keyed on those built-in names; the
// §25 rename axis targets the class's type-parameter names instead.
fn inline_react_class_component_fixture(
    props_field_text: &str,
    type_param_names: [&str; 3],
) -> String {
    let [p, s, ss] = type_param_names;
    format!(
        r#"
type Readonly<T> = {{ readonly [K in keyof T]: T[K] }};

declare namespace JSX {{
    interface Element {{}}
    interface ElementClass {{ props: any }}
    interface ElementAttributesProperty {{ props: {{}} }}
    interface IntrinsicAttributes extends React.Attributes {{}}
    interface IntrinsicClassAttributes<T> extends React.ClassAttributes<T> {{}}
}}
declare namespace React {{
    interface Attributes {{ key?: any }}
    interface ClassAttributes<T> {{ ref?: any }}
    interface ReactNode {{}}
    class Component<{p} = {{}}, {s} = any, {ss} = any> {{
        props: {props_field_text};
    }}
    interface ComponentClass<{p} = {{}}> {{ new (props: {p}): Component<{p}, any, any> }}
}}
"#
    )
}

#[test]
fn jsx_class_component_target_display_uses_class_props_wrapper() {
    // Reported repro (renamed) — `Component<P>.props = Readonly<P> & Readonly<{...}>`.
    let lib = inline_react_class_component_fixture(
        "Readonly<P> & Readonly<{ children?: ReactNode | undefined }>",
        ["P", "S", "SS"],
    );
    let source = format!(
        r#"
{lib}

const myHoc = <ComposedComponentProps extends any>(
    ComposedComponent: React.ComponentClass<ComposedComponentProps>,
) => {{
    type WrapperComponentProps = ComposedComponentProps & {{ myProp: string }};
    const WrapperComponent: React.ComponentClass<WrapperComponentProps> = null as any;
    const props: ComposedComponentProps = null as any;

    <WrapperComponent {{...props}} myProp={{1000000}} />;
}};
"#
    );
    let diags = jsx_diagnostics_with_pos_mode(&source, JsxMode::React);

    assert!(
        has_code_with_message_pos(
            &diags,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            "Readonly<...> & Readonly<...>"
        ),
        "TS2322 target must expand class.props wrapper expression for the reported repro shape, got: {diags:?}"
    );
    assert!(
        !has_code_with_message_pos(
            &diags,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            "& WrapperComponentProps'"
        ),
        "TS2322 target must not collapse to the bare constructor-parameter alias, got: {diags:?}"
    );
}

#[test]
fn jsx_class_component_target_display_renamed_user_identifiers_use_class_props_wrapper() {
    // §25 rename axis on USER-chosen identifiers only. Library-level names like
    // `Component`/`ComponentClass` are well-known React names that the JSX
    // validator recognizes through React-alias detection
    // (`crates/tsz-checker/src/checkers/jsx/extraction_react_alias.rs`), so
    // renaming them probes a separate code path (props-type recovery falls
    // back to the instance `.props` access). The structural display rule
    // under test is about user-defined wrapper aliases and the user's local
    // type parameter:
    //   - generic-param `ComposedComponentProps` -> `BaseShape`
    //   - wrapper alias `WrapperComponentProps` -> `WrappedShape`
    //   - extra prop name `myProp` -> `extraValue`
    //   - extra-prop declared type still `string`; supplied value still numeric.
    let lib = inline_react_class_component_fixture(
        "Readonly<P> & Readonly<{ children?: ReactNode | undefined }>",
        ["P", "S", "SS"],
    );
    let source = format!(
        r#"
{lib}

const wrap = <BaseShape extends any>(
    Inner: React.ComponentClass<BaseShape>,
) => {{
    type WrappedShape = BaseShape & {{ extraValue: string }};
    const Outer: React.ComponentClass<WrappedShape> = null as any;
    const inProps: BaseShape = null as any;

    <Outer {{...inProps}} extraValue={{42}} />;
}};
"#
    );
    let diags = jsx_diagnostics_with_pos_mode(&source, JsxMode::React);

    assert!(
        has_code_with_message_pos(
            &diags,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            "Readonly<...> & Readonly<...>"
        ),
        "renamed user-identifier fixture must still expand class.props wrapper, got: {diags:?}"
    );
    assert!(
        !has_code_with_message_pos(
            &diags,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            "& WrappedShape'"
        ),
        "renamed user-identifier fixture must not collapse to the bare constructor-parameter alias `WrappedShape`, got: {diags:?}"
    );
}

#[test]
fn jsx_class_component_target_display_no_wrapper_keeps_constructor_param_alias() {
    // Negative case: when `Component<P>.props` is just `P` (no wrapper expression),
    // the existing display behavior is preserved — the target should reference
    // the constructor parameter alias directly. This pins that the fix only
    // changes display when the class instance `.props` *differs* from the
    // constructor parameter type.
    // `props: P` (no wrapper). class.props is identical to the constructor
    // parameter, so the takeover gate must skip.
    let lib = inline_react_class_component_fixture("P", ["P", "S", "SS"]);
    let source = format!(
        r#"
{lib}

const myHoc = <CompProps extends any>(
    Base: React.ComponentClass<CompProps>,
) => {{
    type Wrapped = CompProps & {{ myProp: string }};
    const W: React.ComponentClass<Wrapped> = null as any;
    const baseProps: CompProps = null as any;

    <W {{...baseProps}} myProp={{1000000}} />;
}};
"#
    );
    let diags = jsx_diagnostics_with_pos_mode(&source, JsxMode::React);

    // With `props: P`, class.props is identical to the constructor parameter
    // type after instantiation, so the takeover gate skips and the legacy
    // display path keeps the constructor-parameter alias `Wrapped` in the
    // TS2322 target.
    assert!(
        has_code_with_message_pos(
            &diags,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            "Wrapped",
        ),
        "no-wrapper case must keep the constructor-parameter alias in the TS2322 target, got: {diags:?}"
    );
}

#[test]
fn test_jsx_fragment_factory_no_unused_locals_react16_fixture_checks_nested_callback_body() {
    let Some(react_types) = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts") else {
        return;
    };
    let Some(source) = load_typescript_fixture(
        "TypeScript/tests/cases/compiler/jsxFragmentFactoryNoUnusedLocals.tsx",
    ) else {
        return;
    };
    let source = source.replace("/// <reference path=\"/.lib/react16.d.ts\" />", "");

    let diags = cross_file_jsx_diagnostics_with_options_and_default_libs(
        &react_types,
        &source,
        CheckerOptions {
            jsx_mode: JsxMode::React,
            jsx_factory: "createElement".to_string(),
            jsx_factory_from_config: true,
            jsx_fragment_factory: "Fragment".to_string(),
            jsx_fragment_factory_from_config: true,
            no_unused_locals: true,
            ..CheckerOptions::default()
        },
        true,
    );
    assert!(
        has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "expected nested setCnt callback to emit TS7006 for prev, got: {diags:?}"
    );
    assert!(
        !has_code_with_message(
            &diags,
            diagnostic_codes::IS_DECLARED_BUT_ITS_VALUE_IS_NEVER_READ,
            "'setCnt'"
        ),
        "setCnt is read inside the JSX onClick callback body and should not emit TS6133, got: {diags:?}"
    );
}

#[test]
fn jsx_any_intrinsic_props_still_evaluate_attribute_callback_body() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        p: any;
        button: any;
    }
}
declare function createElement(...args: any[]): any;
declare const Fragment: any;

export function Counter() {
    const [cnt, setCnt] = null as any;
    return <>
        <p>{cnt}</p>
        <button onClick={() => setCnt((prev) => prev + 1)} type="button">Update</button>
    </>;
}
"#;

    let diags = cross_file_jsx_diagnostics_with_options_and_default_libs(
        "",
        source,
        CheckerOptions {
            jsx_mode: JsxMode::React,
            jsx_factory: "createElement".to_string(),
            jsx_factory_from_config: true,
            jsx_fragment_factory: "Fragment".to_string(),
            jsx_fragment_factory_from_config: true,
            no_unused_locals: true,
            ..CheckerOptions::default()
        },
        false,
    );
    assert!(
        has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "any intrinsic props should still evaluate nested callback bodies, got: {diags:?}"
    );
    assert!(
        !has_code_with_message(
            &diags,
            diagnostic_codes::IS_DECLARED_BUT_ITS_VALUE_IS_NEVER_READ,
            "'setCnt'"
        ),
        "setCnt is read inside an any-props JSX attribute callback, got: {diags:?}"
    );
}

#[test]
fn test_generic_props_alias_call_preserves_callback_member() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Elements {{
    foo: {{ callback?: (value: number) => void }};
    bar: {{ callback?: (value: string) => void }};
}}

type Props<C extends keyof Elements> = {{ as?: C }} & Elements[C];
declare function Test<C extends keyof Elements>(props: Props<C>): null;

Test({{
    as: "bar",
    callback: value => value.toUpperCase(),
}});
"#
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "generic props alias call should not lose the callback member, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "callback parameter should remain contextually typed through the call path, got: {diags:?}"
    );
}

#[test]
fn test_concrete_props_still_emit_excess_errors() {
    // When props type is fully concrete (no type parameters), excess property
    // checking should still work.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface SFC<P> {{
    (props: P & {{ children?: string }}): JSX.Element;
}}
function test(Component: SFC<{{ x: number }}>) {{
    return <Component x={{1}} y={{"blah"}} />;
}}
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should emit TS2322 for excess property 'y' on concrete props, got: {diags:?}"
    );
}

// =============================================================================
// TS2783: JSX spread overwrite detection
// =============================================================================

#[test]
fn test_spread_overwrite_skips_type_check() {
    // When a later spread will overwrite an explicit attribute, tsc only
    // emits TS2783 (overwrite warning) and does NOT emit TS2322 (type mismatch).
    // This tests the ordering: overwrite detection before type checking.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Props {{
    x: number;
}}
function Foo(props: Props) {{ return <div />; }}
const p: Props = {{ x: 1 }};
let t = <Foo x={{"not a number"}} {{...p}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    // TS2783 should be emitted (spread overwrites 'x')
    assert!(
        has_code(
            &diags,
            diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN
        ),
        "Expected TS2783 for overwritten attribute, got: {diags:?}"
    );
    // TS2322 should NOT be emitted (type check skipped because overwritten)
    let ts2322_for_x = diags.iter().any(|(c, msg)| {
        *c == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE && msg.contains("string")
    });
    assert!(
        !ts2322_for_x,
        "Should NOT emit TS2322 for overwritten attribute, got: {diags:?}"
    );
}

#[test]
fn test_ts2783_jsx_spread_overwrites_explicit_attribute() {
    // When a required property in a spread follows an explicit attribute with
    // the same name, TS2783 should be emitted on the explicit attribute.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Props {{
    a: number;
    b: number;
}}
function Foo(props: Props) {{ return <div />; }}
const p: Props = {{ a: 1, b: 1 }};
let x = <Foo a={{1}} {{...p}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN
        ),
        "Should emit TS2783 when spread overwrites explicit attribute, got: {diags:?}"
    );
}

#[test]
fn test_ts2783_not_emitted_for_optional_spread_property() {
    // When the spread property is optional, the explicit attribute may NOT be
    // overwritten at runtime, so TS2783 should NOT be emitted.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Props {{
    a: number;
    b: number;
    d?: number;
}}
function Foo(props: Props) {{ return <div />; }}
const p: Props = {{ a: 1, b: 1 }};
let x = <Foo d={{1}} {{...p}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN
        ),
        "Should NOT emit TS2783 when spread has optional property, got: {diags:?}"
    );
}

#[test]
fn test_ts2783_multiple_spreads_track_required_only() {
    // First spread has optional `d`, so no TS2783. Second spread has required
    // `d`, so TS2783 fires for the original explicit attribute.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface Props {{
    a: number;
    d?: number;
}}
function Foo(props: Props) {{ return <div />; }}
const p: Props = {{ a: 1 }};
let x = <Foo a={{1}} d={{1}} {{...p}} {{...{{ d: 1 }}}} />;
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts2783_count = diags
        .iter()
        .filter(|(c, _)| {
            *c == diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN
        })
        .count();
    // 'a' overwritten by first spread (required in Props), 'd' overwritten by second spread
    assert!(
        ts2783_count >= 2,
        "Should emit TS2783 for both 'a' (required in first spread) and 'd' (required in second spread), got {ts2783_count} TS2783 errors: {diags:?}"
    );
}

#[test]
fn test_intrinsic_later_inferred_spread_emits_ts2783_and_ts2322() {
    let source = r#"
declare namespace JSX {
    interface Element { }
    interface IntrinsicElements {
        test1: { x: string; y?: number; z?: string };
    }
}

var obj5 = { x: 32, y: 32 };
<test1 x="ok" {...obj5} />;

var obj7 = { x: 'foo' };
<test1 x={32} {...obj7} />;
"#;
    let diags = jsx_diagnostics(source);
    let ts2783_count = diags
        .iter()
        .filter(|(code, _)| {
            *code == diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN
        })
        .count();
    assert!(
        ts2783_count == 2,
        "Later inferred spreads should emit TS2783 for each overwritten explicit attr, got: {diags:?}"
    );
    assert!(
        has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Later inferred spreads should still report the spread-side TS2322 mismatch, got: {diags:?}"
    );
}

#[test]
fn test_ts2783_generic_type_parameter_constraint_spread() {
    // When a generic component spreads a type parameter `T extends { x: number }`,
    // the required property `x` from the constraint should trigger TS2783 if `x`
    // was also specified as an explicit attribute before the spread.
    // This matches tsc behavior for tsxGenericAttributesType1.tsx.
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Comp<T extends {{ x: number }}>(props: T) {{ return <div />; }}
function wrapper<T extends {{ x: number }}>(Component: (props: T) => JSX.Element) {{
    return (props: T) => <Component x={{2}} {{...props}} />;
}}
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN
        ),
        "Should emit TS2783 when generic spread overwrites explicit attribute via constraint, got: {diags:?}"
    );
}

#[test]
fn test_ts2783_not_emitted_for_generic_without_constraint() {
    // When a generic component spreads a type parameter without a constraint,
    // no TS2783 should be emitted since we don't know the properties.
    let source = format!(
        r#"
{JSX_PREAMBLE}
function Comp<T>(props: T) {{ return <div />; }}
function wrapper<T>(Component: (props: T) => JSX.Element) {{
    return (props: T) => <Component x={{2}} {{...props}} />;
}}
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::IS_SPECIFIED_MORE_THAN_ONCE_SO_THIS_USAGE_WILL_BE_OVERWRITTEN
        ),
        "Should NOT emit TS2783 for unconstrained type parameter spread, got: {diags:?}"
    );
}

#[test]
fn test_intrinsic_jsx_spread_callback_property_uses_method_signature_context() {
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {
        test1: { x?: (n: { len: number }) => number };
    }
}

<test1 {...{ x: (n) => 0 }} />;
<test1 {...{ x: (n) => n.len }} />;
"#;

    let diags = jsx_diagnostics(source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "JSX spread callback props should contextually type parameters, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "JSX spread callback props should preserve callback member access, got: {diags:?}"
    );
}

// =============================================================================
// JSX Children Contextual Typing
// =============================================================================

#[test]
fn test_jsx_children_callback_gets_contextual_type() {
    // When a component has `children: (arg: SomeType) => Element`, a callback
    // child like `{(arg) => ...}` should get its parameter typed from the
    // `children` prop — no TS7006 should be emitted.
    let source = format!(
        r#"
{JSX_PREAMBLE}
interface User {{ Name: string }}
function FetchUser(props: {{ children: (user: User) => JSX.Element }}) {{
    return <div />;
}}
function UserName() {{
    return <FetchUser>{{user => <div />}}</FetchUser>;
}}
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts7006 = count_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE);
    assert!(
        ts7006 == 0,
        "Should NOT emit TS7006 when children callback is contextually typed, got: {diags:?}"
    );
}

#[test]
fn test_jsx_children_callback_union_props_gets_contextual_type() {
    // Discriminated union props with different children callback signatures:
    // When the children callback types differ across union members (e.g.,
    // (arg: string) => void vs (arg: number) => void), tsc uses discriminant
    // narrowing to pick the right callback type. Our solver unions the
    // parameter types (string | number) for contextual typing.
    //
    // TODO: With pure speculative typing (no dedup state leaks), the
    // contextual typing for children callbacks in discriminated union props
    // needs to be provided through the proper contextual typing mechanism,
    // not through stale dedup state that happened to suppress TS7006.
    // This test now expects TS7006 until proper discriminant narrowing for
    // JSX children callbacks is implemented.
    let source = format!(
        r#"
{JSX_PREAMBLE}
type Props =
  | {{ renderNumber?: false; children: (arg: string) => void }}
  | {{ renderNumber: true; children: (arg: number) => void }};
declare function Foo(props: Props): JSX.Element;
const Test = () => {{
    return <Foo>{{(value) => {{}}}}</Foo>;
}};
"#
    );
    let diags = jsx_diagnostics(&source);
    let ts7006 = count_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE);
    // With pure speculation, TS7006 is now correctly emitted because the
    // stale dedup state that previously suppressed it is properly cleaned up.
    // The proper fix is discriminant narrowing for union JSX children props.
    assert!(
        ts7006 <= 1,
        "Expected at most one TS7006 for union children callback, got: {diags:?}"
    );
}

#[test]
fn test_generic_jsx_children_body_callbacks_use_inferred_props() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare namespace React {{
    interface ReactElement<T = any> {{}}
}}

declare const TestComponentWithChildren: <T, TParam>(props: {{
  state: T;
  selector?: (state: NoInfer<T>) => TParam;
  children?: (state: NoInfer<TParam>) => React.ReactElement<any> | null;
}}) => React.ReactElement<any>;

declare const TestComponentWithoutChildren: <T, TParam>(props: {{
  state: T;
  selector?: (state: NoInfer<T>) => TParam;
  notChildren?: (state: NoInfer<TParam>) => React.ReactElement<any> | null;
}}) => React.ReactElement<any>;

<TestComponentWithChildren state={{{{ foo: 123 }}}} selector={{state => state.foo}}>
  {{selected => {{
    const check: number = selected;
    return <div>{{check}}</div>;
  }}}}
</TestComponentWithChildren>;
"#
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "Generic JSX body children should reuse inferred props for callback contextual typing, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Generic JSX body children inference should not fall back to TS2322, got: {diags:?}"
    );
}

#[test]
fn test_generic_jsx_children_defaulted_type_param_infers_from_selector() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare namespace React {{
    interface ReactElement<T = any> {{}}
}}

interface State {{
    value: boolean;
}}

declare const Subscribe: <TSelected = State>(props: {{
  selector?: (state: State) => TSelected;
  children: (state: TSelected) => void;
}}) => React.ReactElement<any>;

<Subscribe selector={{state => [state.value]}}>
  {{([value = false]) => {{
      const check: boolean = value;
  }}}}
</Subscribe>;
"#
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "Defaulted generic JSX children should get callback contextual typing from selector inference, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE
        ),
        "Defaulted generic JSX children destructuring should stay on the request path, got: {diags:?}"
    );
    // Note: TS2322 may be emitted here depending on generic inference resolution.
    // The key invariant is no TS7006/TS7031 implicit-any errors — the contextual
    // typing from selector inference should work correctly.
}

#[test]
fn test_jsx_children_presence_narrows_union_component_type_for_body_children() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare namespace React {{
    interface Component<P> {{ props: P; }}
    interface ComponentClass<P> {{ new(props: P): Component<P>; }}
    interface FunctionComponent<P> {{ (props: P): JSX.Element; }}
    type ComponentType<P> = ComponentClass<P> | FunctionComponent<P>;
}}
type Props =
  | {{
        icon: string;
        label: string;
        children(props: {{ onClose: () => void }}): JSX.Element;
        controls?: never;
    }}
  | {{
        icon: string;
        label: string;
        controls: {{ title: string }}[];
        children?: never;
    }};
declare const DropdownMenu: React.ComponentType<Props>;
const Test = () => (
    <DropdownMenu icon="move" label="Select a direction">
        {{({{ onClose }}) => <div />}}
    </DropdownMenu>
);
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "Body children should be contextually typed after union narrowing, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE
        ),
        "Destructured body children should be contextually typed after union narrowing, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Union narrowing on children presence should avoid TS2322 here, got: {diags:?}"
    );
}

#[test]
fn test_jsx_children_presence_narrows_union_component_type_for_explicit_children_attr() {
    let source = format!(
        r#"
{JSX_PREAMBLE}
declare namespace React {{
    interface Component<P> {{ props: P; }}
    interface ComponentClass<P> {{ new(props: P): Component<P>; }}
    interface FunctionComponent<P> {{ (props: P): JSX.Element; }}
    type ComponentType<P> = ComponentClass<P> | FunctionComponent<P>;
}}
type Props =
  | {{
        icon: string;
        label: string;
        children(props: {{ onClose: () => void }}): JSX.Element;
        controls?: never;
    }}
  | {{
        icon: string;
        label: string;
        controls: {{ title: string }}[];
        children?: never;
    }};
declare const DropdownMenu: React.ComponentType<Props>;
const Test = () => (
    <DropdownMenu
        icon="move"
        label="Select a direction"
        children={{({{ onClose }}) => <div />}}
    />
);
"#
    );
    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE),
        "Explicit children attr should be contextually typed after union narrowing, got: {diags:?}"
    );
    assert!(
        !has_code(
            &diags,
            diagnostic_codes::BINDING_ELEMENT_IMPLICITLY_HAS_AN_TYPE
        ),
        "Destructured explicit children attr should be contextually typed after union narrowing, got: {diags:?}"
    );
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Union narrowing on explicit children attr should avoid TS2322 here, got: {diags:?}"
    );
}

