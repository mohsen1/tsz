#[test]
fn jsx_lma_user_type_named_widget_does_not_emit_ts2741() {
    // Sister test with a different name: proves the rule is structural and not
    // tied to the literal spelling `Factory`.
    let codes = jsx_lma_user_type_named_factory_does_not_disable_default_props_helper("Widget");
    assert!(
        !codes.contains(&diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE),
        "User type named `Widget` should also not disable JSX.LibraryManagedAttributes; \
         expected no TS2741 for `<Comp />`, got: {codes:?}"
    );
}

// =============================================================================
// TS2786 false positive: union component types that include string
// =============================================================================

const fn react16_like_jsx_lib() -> &'static str {
    // Mirrors React 16 types that include ReactElement<any, any> 2-arg return,
    // PropsWithChildren wrapping, and the full JSX namespace shape.
    r#"
declare namespace React {
    interface ReactElement<P = any, T = any> {
        type: T;
        props: P;
    }
    type ReactNode = ReactElement | string | number | null | undefined;
    type PropsWithChildren<P> = P & { children?: ReactNode };
    interface Component<P, S = {}> { props: P; state: S; render(): ReactElement | null; }
    interface ComponentClass<P = {}, S = {}> {
        new(props: P, context?: any): Component<P, S>;
        defaultProps?: Partial<P>;
        displayName?: string;
    }
    interface FunctionComponent<P = {}> {
        (props: PropsWithChildren<P>, context?: any): ReactElement<any, any> | null;
        defaultProps?: Partial<P>;
        displayName?: string;
    }
    type ComponentType<P = {}> = ComponentClass<P> | FunctionComponent<P>;
    type ReactType = string | ComponentType<any>;
}
declare namespace JSX {
    interface Element extends React.ReactElement<any> {}
    interface ElementClass extends React.Component<any> {}
    interface ElementAttributesProperty { props: {}; }
    interface IntrinsicElements {
        div: {};
        a: {};
        button: {};
    }
}
"#
}

#[test]
fn jsx_react_type_union_no_ts2786_with_actual_react16() {
    // Rule: when a JSX component type is `React.ReactType` from the actual react16.d.ts,
    // no TS2786 should be emitted.
    let Some(react_types) = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts") else {
        return; // Skip when TypeScript submodule is not available
    };
    let source = r#"
import React from "react";
function App(props: { component: React.ReactType }) {
    const Comp: React.ReactType = props.component;
    return (<Comp />);
}
"#;
    let diags = cross_file_jsx_diagnostics_with_mode(&react_types, source, JsxMode::React);
    assert!(
        !has_code(&diags, diagnostic_codes::CANNOT_BE_USED_AS_A_JSX_COMPONENT),
        "React.ReactType from react16.d.ts should not emit TS2786, got: {diags:?}"
    );
}

#[test]
fn jsx_component_type_union_no_ts2786_with_actual_react16() {
    // Rule: when a JSX component type is `ComponentType<P1> | ComponentType<P2>` from react16.d.ts,
    // no TS2786 should be emitted.
    let Some(react_types) = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts") else {
        return; // Skip when TypeScript submodule is not available
    };
    let source = r#"
import React from "react";
function render2() {
    interface P1 { p?: boolean; c?: string; }
    interface P2 { p?: boolean; c?: any; d?: any; }
    var C: React.ComponentType<P1> | React.ComponentType<P2> = null as any;
    const a = <C p={true} />;
}
"#;
    let diags = cross_file_jsx_diagnostics_with_mode(&react_types, source, JsxMode::React);
    assert!(
        !has_code(&diags, diagnostic_codes::CANNOT_BE_USED_AS_A_JSX_COMPONENT),
        "ComponentType<P1> | ComponentType<P2> from react16.d.ts should not emit TS2786, got: {diags:?}"
    );
}

#[test]
fn jsx_react_type_union_global_jsx_augmentation_no_ts2786() {
    // Tests `declare module "react"` with `global { namespace JSX {...} }` augmentation,
    // which is the pattern used by react16.d.ts. TS2786 must not fire for ReactType.
    let lib = r#"
declare module "react" {
    interface ReactElement<P = any> {
        type: any;
        props: P;
        key: string | null;
    }
    interface Component<P, S = {}> {
        props: Readonly<P>;
        state: Readonly<S>;
        render(): ReactElement<any> | null;
    }
    interface StatelessComponent<P = {}> {
        (props: P & { children?: ReactNode }, context?: any): ReactElement<any> | null;
        displayName?: string;
    }
    type ReactNode = ReactElement<any> | string | number | null | undefined;
    interface ComponentClass<P = {}, S = {}> {
        new(props: P, context?: any): Component<P, S>;
        displayName?: string;
    }
    type ComponentType<P = {}> = ComponentClass<P> | StatelessComponent<P>;
    type ReactType<P = any> = string | ComponentType<P>;
    global {
        namespace JSX {
            interface Element extends React.ReactElement<any> {}
            interface ElementClass extends React.Component<any> {
                render(): React.ReactElement<any> | null;
            }
            interface ElementAttributesProperty { props: {}; }
            interface IntrinsicElements { div: {}; a: {}; button: {}; }
        }
    }
}
"#;
    let main = r#"
import React from "react";
declare const Comp: React.ReactType;
const _ = <Comp />;
"#;
    let diags = cross_file_jsx_diagnostics_with_mode(lib, main, JsxMode::React);
    assert!(
        !has_code(&diags, diagnostic_codes::CANNOT_BE_USED_AS_A_JSX_COMPONENT),
        "React.ReactType with global JSX augmentation should not emit TS2786, got: {diags:?}"
    );
}

#[test]
fn jsx_react_type_with_validation_map_no_ts2786() {
    // Tests that ValidationMap<P> in propTypes does not cause TS2786.
    let lib = r#"
declare module "prop-types" {
    export interface Validator<T> {
        (props: object, propName: string, componentName: string): Error | null;
    }
    export type ValidationMap<T> = { [K in keyof T]-?: Validator<T[K]> };
}
declare module "react" {
    import * as PropTypes from 'prop-types';
    interface ReactElement<P = any> { type: any; props: P; key: string | null; }
    type ReactNode = ReactElement<any> | string | number | null | undefined;
    interface Component<P, S = {}> { props: Readonly<P>; render(): ReactElement<any> | null; }
    interface StatelessComponent<P = {}> {
        (props: P & { children?: ReactNode }, context?: any): ReactElement<any> | null;
        propTypes?: PropTypes.ValidationMap<P>;
        displayName?: string;
    }
    interface ComponentClass<P = {}, S = {}> {
        new(props: P, context?: any): Component<P, S>;
        propTypes?: PropTypes.ValidationMap<P>;
        displayName?: string;
    }
    type ComponentType<P = {}> = ComponentClass<P> | StatelessComponent<P>;
    type ReactType<P = any> = string | ComponentType<P>;
    global {
        namespace JSX {
            interface Element extends React.ReactElement<any> {}
            interface ElementClass extends React.Component<any> {}
            interface ElementAttributesProperty { props: {}; }
            interface IntrinsicElements { div: {}; a: {}; button: {}; }
        }
    }
}
"#;
    let main = r#"
import React from "react";
declare const Comp: React.ReactType;
const _ = <Comp />;
"#;
    let diags = cross_file_jsx_diagnostics_with_mode(lib, main, JsxMode::React);
    assert!(
        !has_code(&diags, diagnostic_codes::CANNOT_BE_USED_AS_A_JSX_COMPONENT),
        "React.ReactType with ValidationMap propTypes should not emit TS2786, got: {diags:?}"
    );
}

#[test]
fn jsx_react_type_union_no_ts2786() {
    // Rule: when a JSX component type is `ReactType = string | ComponentType<any>`,
    // all union members are valid JSX components so TS2786 must not fire.
    let lib = react16_like_jsx_lib();
    let main = r#"
declare const Comp: React.ReactType;
const _ = <Comp />;
"#;
    let diags = cross_file_jsx_diagnostics_with_mode(lib, main, JsxMode::React);
    assert!(
        !has_code(&diags, diagnostic_codes::CANNOT_BE_USED_AS_A_JSX_COMPONENT),
        "React.ReactType JSX component should not emit TS2786, got: {diags:?}"
    );
}

#[test]
fn jsx_component_type_union_no_ts2786() {
    // Rule: when a JSX component type is `ComponentType<P1> | ComponentType<P2>`,
    // all members are valid JSX components so TS2786 must not fire.
    let lib = react16_like_jsx_lib();
    let main = r#"
type Props1 = { a: string };
type Props2 = { b: number };
declare const C: React.ComponentType<Props1> | React.ComponentType<Props2>;
const _ = <C />;
"#;
    let diags = cross_file_jsx_diagnostics_with_mode(lib, main, JsxMode::React);
    assert!(
        !has_code(&diags, diagnostic_codes::CANNOT_BE_USED_AS_A_JSX_COMPONENT),
        "ComponentType<P1> | ComponentType<P2> JSX component should not emit TS2786, got: {diags:?}"
    );
}

#[test]
fn jsx_react_type_renamed_component_alias_no_ts2786() {
    // Rule: renaming `ComponentType` or `ReactType` to a user-chosen name
    // must not affect TS2786 suppression — the fix is structural.
    let lib = r#"
declare namespace MyLib {
    interface Elem {}
    interface Inst<P> { props: P; }
    interface Ctor<P = {}> { new(props: P): Inst<P>; }
    interface SFC<P = {}> { (props: P): Elem | null; }
    type AnyComp<P = {}> = Ctor<P> | SFC<P>;
    type Tag = string | AnyComp<any>;
}
declare namespace JSX {
    interface Element extends MyLib.Elem {}
    interface ElementClass extends MyLib.Inst<any> {}
    interface ElementAttributesProperty { props: {}; }
    interface IntrinsicElements { div: {}; }
}
"#;
    let main = r#"
declare const Comp: MyLib.Tag;
const _ = <Comp />;
"#;
    let diags = cross_file_jsx_diagnostics_with_mode(lib, main, JsxMode::React);
    assert!(
        !has_code(&diags, diagnostic_codes::CANNOT_BE_USED_AS_A_JSX_COMPONENT),
        "User-named union JSX component alias should not emit TS2786, got: {diags:?}"
    );
}

#[test]
fn jsx_react_type_union_no_ts2786_with_actual_react16_top_level() {
    // Same as jsx_react_type_union_no_ts2786_with_actual_react16 but top-level declaration.
    let Some(react_types) = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts") else {
        return;
    };
    let source = r#"
import React from "react";
declare const Comp: React.ReactType;
const _ = <Comp />;
"#;
    let diags = cross_file_jsx_diagnostics_with_mode(&react_types, source, JsxMode::React);
    assert!(
        !has_code(&diags, diagnostic_codes::CANNOT_BE_USED_AS_A_JSX_COMPONENT),
        "React.ReactType top-level from react16.d.ts should not emit TS2786, got: {diags:?}"
    );
}

#[test]
fn jsx_react_type_union_no_ts2786_with_actual_react16_in_function() {
    // Same as top-level but inside function body without function params.
    let Some(react_types) = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts") else {
        return;
    };
    let source = r#"
import React from "react";
function App() {
    const Comp: React.ReactType = "div" as any;
    return (<Comp />);
}
"#;
    let diags = cross_file_jsx_diagnostics_with_mode(&react_types, source, JsxMode::React);
    assert!(
        !has_code(&diags, diagnostic_codes::CANNOT_BE_USED_AS_A_JSX_COMPONENT),
        "React.ReactType in function body from react16.d.ts should not emit TS2786, got: {diags:?}"
    );
}

#[test]
fn jsx_react_type_union_no_ts2786_with_actual_react16_top_level_const() {
    // Top-level (non-ambient) const with explicit ReactType annotation.
    let Some(react_types) = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts") else {
        return;
    };
    let source = r#"
import React from "react";
const Comp: React.ReactType = "div" as any;
const _ = <Comp />;
"#;
    let diags = cross_file_jsx_diagnostics_with_mode(&react_types, source, JsxMode::React);
    assert!(
        !has_code(&diags, diagnostic_codes::CANNOT_BE_USED_AS_A_JSX_COMPONENT),
        "React.ReactType top-level const from react16.d.ts should not emit TS2786, got: {diags:?}"
    );
}

#[test]
fn jsx_react_type_union_no_ts2786_with_actual_react16_in_function_assign() {
    // Function body but assigns JSX element instead of returning.
    let Some(react_types) = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts") else {
        return;
    };
    let source = r#"
import React from "react";
function App(): void {
    const Comp: React.ReactType = "div" as any;
    const _: any = <Comp />;
}
"#;
    let diags = cross_file_jsx_diagnostics_with_mode(&react_types, source, JsxMode::React);
    assert!(
        !has_code(&diags, diagnostic_codes::CANNOT_BE_USED_AS_A_JSX_COMPONENT),
        "React.ReactType in function body assign from react16.d.ts should not emit TS2786, got: {diags:?}"
    );
}

#[test]
fn jsx_react_type_union_no_ts2786_with_actual_react16_explicit_return_type() {
    // Function body with explicit return type annotation to test if inferred return type causes issue.
    let Some(react_types) = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts") else {
        return;
    };
    let source = r#"
import React from "react";
function App(): React.ReactElement<any> | null {
    const Comp: React.ReactType = "div" as any;
    return (<Comp />);
}
"#;
    let diags = cross_file_jsx_diagnostics_with_mode(&react_types, source, JsxMode::React);
    assert!(
        !has_code(&diags, diagnostic_codes::CANNOT_BE_USED_AS_A_JSX_COMPONENT),
        "React.ReactType with explicit return type in react16.d.ts should not emit TS2786, got: {diags:?}"
    );
}

#[test]
fn jsx_react_type_inline_in_function_return() {
    // Tests if inline ReactType union in a function with inferred return type emits TS2786.
    let lib = react16_like_jsx_lib();
    let main = r#"
function App() {
    const Comp: React.ReactType = "div" as any;
    return (<Comp />);
}
"#;
    let diags = cross_file_jsx_diagnostics_with_mode(lib, main, JsxMode::React);
    assert!(
        !has_code(&diags, diagnostic_codes::CANNOT_BE_USED_AS_A_JSX_COMPONENT),
        "React.ReactType in function with inferred return type should not emit TS2786, got: {diags:?}"
    );
}
