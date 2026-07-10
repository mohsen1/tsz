//! Contiguous test shard split out of the parent module to satisfy the
//! source-file line cap.

use super::*;

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
    let react_types = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts");
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
    let react_types = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts");
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
    let react_types = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts");
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
    let react_types = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts");
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
    let react_types = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts");
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
    let react_types = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts");
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
    let react_types = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts");
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

/// Build a React-like JSX lib whose framework attribute aliases use the given
/// names, so the same structural scenario can be exercised with `Key`/`LegacyRef`
/// and with renamed aliases (proving the fix keys on `IntrinsicAttributes` /
/// `IntrinsicClassAttributes` membership, not on the alias spelling).
fn jsx_key_ref_lib(key_alias: &str, ref_alias: &str, element_iface: &str) -> String {
    format!(
        r#"
declare namespace React {{
  type {key_alias} = string | number | bigint;
  interface RefObject<T> {{ current: T | null; }}
  type RefCallback<T> = (instance: T | null) => void;
  type Ref<T> = RefCallback<T> | RefObject<T> | null;
  type {ref_alias}<T> = string | Ref<T>;
  interface Attributes {{
    key?: {key_alias} | null | undefined;
  }}
  interface ClassAttributes<T> extends Attributes {{
    ref?: {ref_alias}<T> | undefined;
  }}
}}
declare module "react" {{
  export = React;
}}
interface {element_iface} {{ tagName: string; }}
declare namespace JSX {{
  interface Element {{}}
  interface IntrinsicAttributes extends React.Attributes {{}}
  interface IntrinsicClassAttributes<T> extends React.ClassAttributes<T> {{}}
  interface IntrinsicElements {{
    div: {{ onClick?: string | undefined }} & React.ClassAttributes<{element_iface}>;
  }}
}}
"#
    )
}

fn jsx_key_ref_diags(lib: &str) -> Vec<(u32, String)> {
    let source = r#"
import * as React from "react";
const k = <div key={true} />;
const r = <div ref={5} />;
const o = <div onClick={5} />;
"#;
    cross_file_jsx_diagnostics_with_options_and_default_libs(
        lib,
        source,
        CheckerOptions {
            jsx_mode: JsxMode::React,
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
        true,
    )
}

/// #14818: tsc checks JSX `key`/`ref` as members of the merged
/// `IntrinsicAttributes` / `IntrinsicClassAttributes` object, so its TS2322
/// target keeps the property's full declared apparent type — alias name intact
/// and `| null | undefined` retained. tsz used to strip nullish and expand the
/// alias (the ordinary write-position-prop display). The `onClick` control must
/// still strip `| undefined` (matching tsc) to prove ordinary props are
/// unaffected.
#[test]
fn jsx_intrinsic_key_ref_ts2322_target_preserves_declared_optional_type() {
    let lib = jsx_key_ref_lib("Key", "LegacyRef", "HTMLDivElement");
    let diags = jsx_key_ref_diags(&lib);

    assert!(
        has_code_with_message(
            &diags,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            "Type 'boolean' is not assignable to type 'Key | null | undefined'.",
        ),
        "key target should preserve `Key | null | undefined`, got: {diags:?}"
    );
    // The `ref` target keeps the `LegacyRef<…>` alias wrapper and the trailing
    // `| undefined`, instead of the previously-expanded `string | RefObject<…> |
    // ((instance: …) => void)` structural form with nullish dropped. (The alias
    // TYPE ARGUMENT renders structurally here only because this minimal lib's
    // element interface is tiny; that is a pre-existing, orthogonal display
    // behavior — the raw declared property type already carried a structural arg
    // before this fix — so the assertion intentionally does not pin it.)
    assert!(
        messages_for_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
            .iter()
            .any(
                |m| m.starts_with("Type 'number' is not assignable to type 'LegacyRef<")
                    && m.ends_with("> | undefined'.")
            ),
        "ref target should preserve the `LegacyRef<…> | undefined` alias + nullish, got: {diags:?}"
    );
    // Control: an ordinary optional prop still strips `| undefined`, matching tsc.
    assert!(
        has_code_with_message(
            &diags,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            "Type 'number' is not assignable to type 'string'.",
        ),
        "ordinary `onClick?: string | undefined` should still display `string`, got: {diags:?}"
    );
}

/// Sister test: rename the framework attribute aliases (`Key` → `ReactKey`,
/// `LegacyRef` → `DomRef`) and the element interface. The TS2322 targets must
/// follow the renamed aliases, proving the display is driven by
/// `IntrinsicAttributes` / `IntrinsicClassAttributes` membership and the declared
/// property type — not by the literal alias spelling `Key`/`LegacyRef`.
#[test]
fn jsx_intrinsic_key_ref_ts2322_target_follows_renamed_alias_structurally() {
    let lib = jsx_key_ref_lib("ReactKey", "DomRef", "DivEl");
    let diags = jsx_key_ref_diags(&lib);

    assert!(
        has_code_with_message(
            &diags,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            "Type 'boolean' is not assignable to type 'ReactKey | null | undefined'.",
        ),
        "key target should follow renamed alias `ReactKey | null | undefined`, got: {diags:?}"
    );
    assert!(
        messages_for_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
            .iter()
            .any(
                |m| m.starts_with("Type 'number' is not assignable to type 'DomRef<")
                    && m.ends_with("> | undefined'.")
            ),
        "ref target should follow renamed alias `DomRef<…> | undefined`, got: {diags:?}"
    );
}

// ── Issue #14817: JSX component-prop diagnostic target must render the resolved
// props object, not the unevaluated LibraryManagedAttributes conditional ──────
//
// When a function-component's props flow through `JSX.LibraryManagedAttributes`
// whose body is a conditional (the React shape, but reproducible with a minimal
// local LMA), the solver resolves the props to a concrete object but kept the
// raw `C extends … ? … : …` conditional as the object's *display alias*. tsc
// resolves the conditional and prints the props object, e.g.
// `{ name: string; count: number; }`. The fix renders the structural object,
// skipping the inline-conditional alias, across TS2741 / TS2322 / excess.

const ISSUE_14817_LMA_NAMESPACE: &str = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {}
    type LibraryManagedAttributes<C, P> =
        C extends (props: infer Q) => any ? Q : P;
}
"#;

#[test]
fn issue_14817_missing_prop_ts2741_shows_resolved_props_object() {
    let source = format!(
        "{ISSUE_14817_LMA_NAMESPACE}\n\
         function Comp(props: {{ name: string; count: number }}): JSX.Element {{ return null as any; }}\n\
         const b = <Comp name=\"x\" />;\n"
    );

    let diags = jsx_diagnostics(&source);
    let messages = messages_for_code(
        &diags,
        diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
    );
    assert!(
        messages.iter().any(|m| m.contains(
            "Property 'count' is missing in type '{ name: string; }' but required in type '{ name: string; count: number; }'."
        )),
        "TS2741 target must be the resolved props object, not the LMA conditional, got: {diags:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|m| m.contains("extends") && m.contains("infer Q")),
        "TS2741 target must not leak the unevaluated LMA conditional, got: {diags:?}"
    );
}

#[test]
fn issue_14817_prop_type_mismatch_ts2322_shows_resolved_props_object() {
    let source = format!(
        "{ISSUE_14817_LMA_NAMESPACE}\n\
         function Comp(props: {{ name: string; count: number }}): JSX.Element {{ return null as any; }}\n\
         const b = <Comp name=\"x\" count=\"nope\" />;\n"
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        !diags
            .iter()
            .any(|(_, m)| m.contains("extends") && m.contains("infer Q")),
        "JSX prop-mismatch diagnostics must not leak the unevaluated LMA conditional, got: {diags:?}"
    );
}

#[test]
fn issue_14817_named_props_interface_alias_is_not_stripped() {
    // A named conditional-bodied alias is a `Lazy(DefId)` reference, not an
    // inline conditional; the structural gate must leave such named aliases on
    // their normal display path. Here `Props` is a plain interface, the common
    // case — its name/structure must survive and TS2741 must still fire.
    let source = format!(
        "{ISSUE_14817_LMA_NAMESPACE}\n\
         interface Props {{ name: string; count: number; }}\n\
         function Comp(props: Props): JSX.Element {{ return null as any; }}\n\
         const b = <Comp name=\"x\" />;\n"
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        has_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
        ),
        "named-interface props must still report the missing required prop, got: {diags:?}"
    );
    assert!(
        !diags
            .iter()
            .any(|(_, m)| m.contains("extends") && m.contains("infer Q")),
        "named-interface props target must not leak the LMA conditional, got: {diags:?}"
    );
}

#[test]
fn issue_14817_renamed_binders_still_resolve_props_object() {
    // Vary the component binder name and the LMA type-parameter names to prove
    // the fix is structural and not keyed on any identifier spelling.
    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {}
    type LibraryManagedAttributes<Component, Properties> =
        Component extends (p: infer Inferred) => any ? Inferred : Properties;
}

function Widget(props: { title: string; size: number }): JSX.Element { return null as any; }

const w = <Widget title="x" />;
"#;

    let diags = jsx_diagnostics(source);
    let messages = messages_for_code(
        &diags,
        diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("but required in type '{ title: string; size: number; }'.")),
        "renamed binders must still resolve to the structural props object, got: {diags:?}"
    );
}

#[test]
fn issue_14817_spread_attrs_target_shows_resolved_props_object() {
    // The whole-object / spread assignability path renders its target through a
    // different code path than explicit attrs; it must also resolve the LMA
    // conditional to the props object rather than leaking the raw conditional.
    let source = format!(
        "{ISSUE_14817_LMA_NAMESPACE}\n\
         function Comp(props: {{ name: string; count: number }}): JSX.Element {{ return null as any; }}\n\
         const p = {{ name: \"x\", count: \"nope\" }};\n\
         const b = <Comp {{...p}} />;\n"
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        !diags
            .iter()
            .any(|(_, m)| m.contains("extends") && m.contains("infer Q")),
        "spread-attrs target must not leak the unevaluated LMA conditional, got: {diags:?}"
    );
    assert!(
        diags.iter().any(
            |(code, m)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && m.contains("type '{ name: string; count: number; }'")
        ),
        "spread-attrs target must be the resolved props object, got: {diags:?}"
    );
}
