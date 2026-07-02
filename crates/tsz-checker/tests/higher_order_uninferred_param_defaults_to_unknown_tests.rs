//! Regression tests: an uninferable type parameter reached only through a
//! nested/curried callback position must default to its constraint (or
//! `unknown`), never leak an internal `__infer_*` inference placeholder into the
//! call-result type.
//!
//! Repro family (`inferentialTypingWithFunctionTypeZip`): a generic `zipWith`
//! whose callback parameter is itself a curried generic function
//! (`f: (x: T) => (y: S) => U`) is called with a generic `pair`. `T` infers from
//! the array argument, `U` infers to the object the inner callback returns, but
//! `S` occupies only a contravariant inner-parameter slot and receives no
//! inference candidate. tsc resolves such a parameter with `getInferredType`
//! (constraint, else `unknown`), so `U` renders as `{ x: number; y: unknown }`.
//! tsz previously let `S` settle on a bare self-referential placeholder that
//! rode into `U`, rendering `{ x: number; y: __infer_3 }`.
//!
//! Owner layer: solver generic-call finalization
//! (`default_leaked_return_type_placeholders`).

use tsz_checker::test_utils::check_source_code_messages as compile_and_get_diagnostics;

/// The TS2322 message text for the single assignment error in `source`.
fn ts2322_message(source: &str) -> String {
    let diags = compile_and_get_diagnostics(source);
    let msgs: Vec<&String> = diags
        .iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, msg)| msg)
        .collect();
    assert_eq!(msgs.len(), 1, "expected exactly one TS2322, got {diags:?}");
    msgs[0].clone()
}

#[test]
fn uninferred_curried_callback_param_defaults_to_unknown() {
    let source = r#"
declare var zipWith: <T, S, U>(a: T[], f: (x: T) => (y: S) => U) => U[];
declare var pair: <T, S>(x: T) => (y: S) => { x: T; y: S; };
const r = zipWith([1], pair);
const bad: null = r;
"#;
    let msg = ts2322_message(source);
    assert!(
        msg.contains("{ x: number; y: unknown; }[]"),
        "uninferred S must default to `unknown`, got: {msg}"
    );
    assert!(
        !msg.contains("__infer"),
        "internal inference placeholder must not leak into the result, got: {msg}"
    );
}

#[test]
fn uninferred_curried_callback_param_leak_survives_renamed_binders() {
    // Same structural shape, different type-parameter and value names. The fix
    // must be structural, not keyed on the `T`/`S`/`U`/`zipWith`/`pair` spelling.
    let source = r#"
declare var combine: <Elem, Ignored, Out>(xs: Elem[], make: (e: Elem) => (i: Ignored) => Out) => Out[];
declare var build: <A, B>(a: A) => (b: B) => { first: A; second: B; };
const out = combine(["s"], build);
const bad: null = out;
"#;
    let msg = ts2322_message(source);
    assert!(
        msg.contains("{ first: string; second: unknown; }[]"),
        "renamed binders must still default the uninferred parameter to `unknown`, got: {msg}"
    );
    assert!(
        !msg.contains("__infer"),
        "internal inference placeholder must not leak under renamed binders, got: {msg}"
    );
}

#[test]
fn constrained_uninferred_param_defaults_to_its_constraint() {
    // When the uninferable parameter carries a constraint, tsc uses the
    // constraint (not `unknown`); this already worked and must keep working.
    let source = r#"
declare var zipWith: <T, S extends string, U>(a: T[], f: (x: T) => (y: S) => U) => U[];
declare var pair: <T, S extends string>(x: T) => (y: S) => { x: T; y: S; };
const r = zipWith([1], pair);
const bad: null = r;
"#;
    let msg = ts2322_message(source);
    assert!(
        msg.contains("{ x: number; y: string; }[]"),
        "constrained uninferred S must default to its constraint `string`, got: {msg}"
    );
    assert!(
        !msg.contains("__infer"),
        "internal inference placeholder must not leak for a constrained parameter, got: {msg}"
    );
}
