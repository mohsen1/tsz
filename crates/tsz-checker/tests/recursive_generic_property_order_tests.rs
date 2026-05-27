use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::{check_source, diagnostic_count};
use tsz_common::common::ScriptTarget;

fn check_es2015(source: &str) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    )
}

#[test]
fn recursive_generic_args_ignore_object_property_order() {
    let diagnostics = check_es2015(
        r#"
interface A<T> { x: T }
interface B<T> { x: T }
interface C<S> extends A<D<S>> { y: S }
interface D<S> extends B<C<S>> { y: S }

declare const c: C<{ s: string; n: number }>;
const d: D<{ n: number; s: string }> = c;
"#,
    );

    assert_eq!(diagnostic_count(&diagnostics, 2322), 0, "{diagnostics:#?}");
}

#[test]
fn recursive_generic_args_compare_structurally_without_name_coupling() {
    let diagnostics = check_es2015(
        r#"
interface LeftBox<V> { item: V }
interface RightBox<V> { item: V }
interface Outer<Q> extends LeftBox<Inner<Q>> { payload: Q }
interface Inner<Q> extends RightBox<Outer<Q>> { payload: Q }

declare const outer: Outer<{ first: string; second: number }>;
const inner: Inner<{ second: number; first: string }> = outer;
"#,
    );

    assert_eq!(diagnostic_count(&diagnostics, 2322), 0, "{diagnostics:#?}");
}

#[test]
fn recursive_generic_args_still_reject_real_mismatch() {
    let diagnostics = check_es2015(
        r#"
interface LeftBox<V> { item: V }
interface RightBox<V> { item: V }
interface Outer<Q> extends LeftBox<Inner<Q>> { payload: Q }
interface Inner<Q> extends RightBox<Outer<Q>> { payload: Q }

declare const outer: Outer<{ first: string; second: number }>;
const inner: Inner<{ second: boolean; first: string }> = outer;
"#,
    );

    assert!(diagnostic_count(&diagnostics, 2322) > 0, "{diagnostics:#?}");
}
