//! Regression tests for issue #9654.
//!
//! When a generic type parameter `S` is inferred from both a fresh-literal
//! argument (e.g. `initial: 0`) and a context-sensitive callback that
//! references `S` in both parameter and return positions, the callback's
//! parameters must be typed against the *final* inferred `S` (the widened
//! `number`), not the un-widened Round-1 literal (`0`). Previously the cached
//! callback type kept `(state: 0) => number` while the final result widened to
//! `number`, producing a spurious TS2345. Only block-bodied callbacks were
//! affected; expression-bodied arrows already widened correctly.

use tsz_checker::test_utils::check_source_code_messages as compile_and_get_diagnostics;

fn ts2345_codes(source: &str) -> Vec<u32> {
    compile_and_get_diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code == 2345)
        .map(|(code, _)| code)
        .collect()
}

#[test]
fn block_body_callback_before_literal_arg_widens_type_param() {
    let source = r#"
declare function f<S>(g: (state: S) => S, initial: S): S;
const r = f((state) => { return state + 1; }, 0);
"#;
    assert!(
        ts2345_codes(source).is_empty(),
        "block-bodied callback before a fresh-literal arg must widen S to number (no TS2345)"
    );
}

#[test]
fn redux_create_store_shape_no_false_ts2345() {
    let source = r#"
declare function createStore<S>(
    reducer: (state: S, action: { type: string }) => S,
    initial: S,
): { getState(): S };
const store = createStore((state, action) => { return state + 1; }, 0);
"#;
    assert!(
        ts2345_codes(source).is_empty(),
        "Redux createStore shape must type the reducer against the widened S (no TS2345)"
    );
}

#[test]
fn function_expression_block_body_widens_type_param() {
    let source = r#"
declare function f<S>(g: (state: S) => S, initial: S): S;
const r = f(function (state) { return state + 1; }, 0);
"#;
    assert!(
        ts2345_codes(source).is_empty(),
        "function-expression block body must behave like an arrow block body (no TS2345)"
    );
}

#[test]
fn rule_is_not_keyed_on_identifier_names() {
    let source = r#"
declare function reduce<Acc>(reducer: (acc: Acc) => Acc, seed: Acc): Acc;
const out = reduce((acc) => { return acc + 1; }, 0);
"#;
    assert!(
        ts2345_codes(source).is_empty(),
        "the rule is structural; renaming the type parameter/params must not change the result"
    );
}

#[test]
fn string_literal_seed_widens_type_param() {
    let source = r#"
declare function f<S>(g: (s: S) => S, initial: S): S;
const r = f((s) => { return s + "!"; }, "x");
"#;
    assert!(
        ts2345_codes(source).is_empty(),
        "a fresh string-literal seed must widen S to string for the block-bodied callback"
    );
}

#[test]
fn expression_body_callback_still_widens_control() {
    // Control: expression-bodied arrows already widened correctly; must stay green.
    let source = r#"
declare function f<S>(g: (state: S) => S, initial: S): S;
const r = f((state) => state + 1, 0);
"#;
    assert!(
        ts2345_codes(source).is_empty(),
        "expression-bodied callback control must remain free of TS2345"
    );
}

#[test]
fn noinfer_simple_return_still_errors_negative() {
    // Negative: with `NoInfer<T>` and a simple `T` return there is no widening
    // contributor, so tsc preserves the literal and reports TS2345. The fix must
    // not blanket-widen and silence this.
    let source = r#"
declare function fn1<T>(a: T, b: NoInfer<T>): T;
const r = fn1("a", "b");
"#;
    assert!(
        !ts2345_codes(source).is_empty(),
        "NoInfer with a simple T return must still preserve the literal and report TS2345"
    );
}

#[test]
fn genuinely_incompatible_block_body_return_still_errors_negative() {
    // Negative: a block-bodied callback whose return is genuinely incompatible
    // with the seed type must still error.
    let source = r#"
declare function f<S extends number>(g: (state: S) => S, initial: S): S;
const r = f((state) => { return "wrong"; }, 0);
"#;
    assert!(
        !ts2345_codes(source).is_empty(),
        "a genuinely incompatible block-bodied callback return must still report TS2345"
    );
}
