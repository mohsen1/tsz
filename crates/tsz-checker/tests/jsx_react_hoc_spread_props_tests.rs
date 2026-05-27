//! JSX spread checks for `React` higher-order component props surfaces.
//!
//! Structural rule: when a JSX spread source is a `React` class `.props` surface
//! that carries the target component props parameter through a readonly wrapper
//! (`Readonly<P & Extra>`), the spread already satisfies bare `P`. The
//! synthesized JSX attrs object must not drop that `P` identity and report
//! `TS2322` against only wrapper-owned props such as `children`.

use tsz_common::checker_options::{CheckerOptions, JsxMode};
use tsz_common::diagnostics::diagnostic_codes;

fn jsx_opts() -> CheckerOptions {
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
    }
}

fn jsx_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source(source, "test.tsx", jsx_opts())
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn has_code(diags: &[(u32, String)], code: u32) -> bool {
    diags.iter().any(|(diag_code, _)| *diag_code == code)
}

fn load_typescript_fixture(rel_path: &str) -> Option<String> {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../../").join(rel_path),
        manifest_dir.join("../../../").join(rel_path),
    ];

    candidates
        .into_iter()
        .find(|candidate| candidate.exists())
        .and_then(|candidate| std::fs::read_to_string(candidate).ok())
}

const REACT_PREAMBLE: &str = r#"
declare namespace JSX {
    interface Element {}
    interface ElementClass { render(): React.ReactNode; }
    interface ElementAttributesProperty { props: {}; }
    interface IntrinsicElements { div: {}; }
}

declare namespace React {
    interface ReactElement<P = any> {}
    type ReactNode = ReactElement<any> | string | number | boolean | null | undefined;

    class Component<P = {}, S = {}> {
        constructor(props: Readonly<P>);
        constructor(props: P, context?: any);
        readonly props: Readonly<{ children?: ReactNode }> & Readonly<P>;
        readonly state: Readonly<S>;
        render(): ReactNode;
    }

    interface ComponentClass<P = {}, S = {}> {
        new(props: P, context?: any): Component<P, S>;
    }

    interface StatelessComponent<P = {}> {
        (props: P & { children?: ReactNode }, context?: any): ReactElement<any> | null;
    }
}
"#;

#[test]
fn react_component_alias_union_accepts_readonly_wrapped_props_spread() {
    let source = format!(
        r#"
{REACT_PREAMBLE}

function wrap<P>(App: React.ComponentClass<P> | React.StatelessComponent<P>): void {{
    class Wrapper extends React.Component<P & {{ x: number }}> {{
        render() {{
            return <App {{...this.props}} />;
        }}
    }}
}}
"#
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "readonly wrapped `P & Extra` props spread should satisfy bare `P`, got: {diags:?}"
    );
}

#[test]
fn react_hoc_spreadprops_react16_fixture_has_no_ts2322() {
    let Some(react_types) = load_typescript_fixture("TypeScript/tests/lib/react16.d.ts") else {
        return;
    };
    let Some(source) =
        load_typescript_fixture("TypeScript/tests/cases/compiler/reactHOCSpreadprops.tsx")
    else {
        return;
    };
    let source = source.replace("/// <reference path=\"/.lib/react16.d.ts\" />", "");
    let libs = tsz_checker::test_utils::load_compiled_lib_files(&["lib.es5.d.ts"]);
    let diags = tsz_checker::test_utils::check_multi_file_with_libs(
        &[("react.d.ts", &react_types), ("test.tsx", &source)],
        "test.tsx",
        jsx_opts(),
        &libs,
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect::<Vec<_>>();

    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "real react16 `reactHOCSpreadprops.tsx` fixture should not emit TS2322, got: {diags:?}"
    );
}

#[test]
fn react_component_class_accepts_renamed_readonly_wrapped_props_spread() {
    let source = format!(
        r#"
{REACT_PREAMBLE}

function decorate<Q>(Widget: React.ComponentClass<Q>): void {{
    class Decorated extends React.Component<Q & {{ label: string }}> {{
        render() {{
            return <Widget {{...this.props}} />;
        }}
    }}
}}
"#
    );

    let diags = jsx_diagnostics(&source);
    assert!(
        !has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "renamed props parameter should be recognized structurally, got: {diags:?}"
    );
}
