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
