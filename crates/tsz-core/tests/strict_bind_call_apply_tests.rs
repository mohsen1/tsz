use crate::binder::BinderState;
use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::test_fixtures::{merge_shared_lib_symbols, setup_lib_contexts};
use tsz_solver::construction::TypeInterner;

fn check_source_with_libs(source: &str) -> Vec<crate::checker::diagnostics::Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

#[test]
fn strict_bind_call_apply_bind_this_arg_mismatch_uses_ts2769() {
    let source = r#"
class C {
    foo(this: this, a: number, b: string): string { return ""; }
}
declare let c: C;
c.foo.bind(undefined);
"#;
    let diagnostics = check_source_with_libs(source);
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2769)
        .expect("expected TS2769");

    let arg_start = source
        .find("undefined")
        .expect("expected undefined argument") as u32;
    assert_eq!(
        diag.start, arg_start,
        "TS2769 should anchor at the thisArg, got: {diag:?}"
    );
    assert_eq!(diag.length, "undefined".len() as u32);
}

// TS2769 anchor position: tsc anchors at the method name "bind" for this
// generic thisArg overload mismatch.
#[test]
fn strict_bind_call_apply_bind_generic_this_arg_mismatch_uses_ts2769() {
    let source = r#"
function bar<T extends unknown[]>(callback: (this: 1, ...args: T) => void) {
    callback.bind(2);
}
"#;
    let diagnostics = check_source_with_libs(source);

    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2769)
        .expect("expected TS2769 for bind thisArg overload mismatch");
    // Verify the overload failure is correctly detected and anchored at `bind`.
    let bind_start = source.find("bind").expect("expected bind method name") as u32;
    assert_eq!(
        diag.start, bind_start,
        "generic bind overload mismatch should anchor at `bind`, got: {diag:?}"
    );
    assert_eq!(diag.length, "bind".len() as u32);
}

// `.call` on a generic function whose type parameter is constrained by its
// `this`-type parameter (`K extends keyof T`, `this: T`) must reproduce tsc's
// `never` collapse: `keyof T` is fixed with `T` still unknown, so `K` becomes
// `keyof unknown` = `never` and the supplied argument fails (TS2345).
#[test]
fn strict_call_collapses_this_dependent_type_param_to_never() {
    let source = r#"
function f<T, K extends keyof T>(this: T, key: K): void {}
declare const o: { a: number };
f.call(o, "a");
"#;
    let diagnostics = check_source_with_libs(source);
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2345)
        .unwrap_or_else(|| panic!("expected TS2345 never collapse, got: {diagnostics:?}"));
    assert!(
        diag.message_text.contains("never"),
        "expected the collapsed parameter to render as 'never', got: {diag:?}"
    );
}

// Renaming the binders must not change the outcome — the collapse is driven by
// the structural `this`-type dependency, not identifier text.
#[test]
fn strict_call_collapse_is_binder_name_agnostic() {
    let source = r#"
function pluck<Obj, Prop extends keyof Obj>(this: Obj, key: Prop): Obj[Prop] {
    return this[key];
}
declare const o: { a: number };
pluck.call(o, "a");
"#;
    let diagnostics = check_source_with_libs(source);
    assert!(
        diagnostics.iter().any(|diag| diag.code == 2345),
        "expected TS2345 regardless of binder names, got: {diagnostics:?}"
    );
}

// `.apply` passes the rest args as a tuple; the collapsed `never` element makes
// the tuple argument fail too (surfaced as the element-level mismatch against
// `never`). The behavioral fix is the collapse; assert an error fires and names
// `never` rather than pinning the exact code.
#[test]
fn strict_apply_collapses_this_dependent_type_param_to_never() {
    let source = r#"
function f<T, K extends keyof T>(this: T, key: K): void {}
declare const o: { a: number };
f.apply(o, ["a"]);
"#;
    let diagnostics = check_source_with_libs(source);
    assert!(
        diagnostics
            .iter()
            .any(|diag| matches!(diag.code, 2345 | 2322) && diag.message_text.contains("never")),
        "expected apply collapse to reject against 'never', got: {diagnostics:?}"
    );
}

// Control: a direct call infers in natural order (`T` from the object arg, `K`
// from the key arg), so there is no collapse and no diagnostic — matching tsc.
#[test]
fn strict_direct_call_does_not_collapse() {
    let source = r#"
function g<T, K extends keyof T>(obj: T, key: K): void {}
declare const o: { a: number };
g(o, "a");
"#;
    let diagnostics = check_source_with_libs(source);
    assert!(
        !diagnostics.iter().any(|diag| diag.code == 2345),
        "direct call must not collapse, got: {diagnostics:?}"
    );
}

// Control: when the constrained type parameter does NOT reference the
// `this`-type parameter (concrete `this`), there is no collapse — tsc clean.
#[test]
fn strict_call_this_independent_constraint_does_not_collapse() {
    let source = r#"
function h<K extends keyof { a: number }>(this: { a: number }, key: K): void {}
declare const o: { a: number };
h.call(o, "a");
"#;
    let diagnostics = check_source_with_libs(source);
    assert!(
        !diagnostics.iter().any(|diag| diag.code == 2345),
        "this-independent constraint must not collapse, got: {diagnostics:?}"
    );
}

// Control: `.bind` defers the rest-arg check to the bound function's later
// invocation, so binding alone produces no collapse diagnostic — tsc clean.
#[test]
fn strict_bind_does_not_collapse_at_bind_site() {
    let source = r#"
function f<T, K extends keyof T>(this: T, key: K): void {}
declare const o: { a: number };
f.bind(o);
"#;
    let diagnostics = check_source_with_libs(source);
    assert!(
        !diagnostics.iter().any(|diag| diag.code == 2345),
        "bind must not collapse at the bind site, got: {diagnostics:?}"
    );
}

#[test]
fn strict_bind_call_apply_apply_tuple_argument_display_stays_unnamed() {
    let diagnostics = check_source_with_libs(
        r#"
declare function foo(a: number, b: string): string;
foo.apply(undefined, [10]);
"#,
    );

    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2345)
        .expect("expected TS2345");
    assert!(
        diag.message_text.contains("Argument of type '[number]'"),
        "expected unnamed tuple source display, got: {diag:?}"
    );
    assert!(
        !diag.message_text.contains("[a: number]"),
        "actual tuple display should not inherit contextual names, got: {diag:?}"
    );
}
