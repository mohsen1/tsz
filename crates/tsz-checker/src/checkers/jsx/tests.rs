//! JSX unit tests.

use crate::test_utils::{check_multi_file, check_source, check_source_diagnostics};

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

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard module holds a contiguous slice of the original test list.
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

mod part_00;
mod part_01;
mod part_02;
mod part_03_jsx_flag;
