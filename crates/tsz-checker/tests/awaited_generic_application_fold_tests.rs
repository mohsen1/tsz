//! `Awaited<X>` must fold the same way whether it appears directly or behind
//! an intermediate alias. Regression coverage for issue #5824.

use tsz_checker::test_utils::check_source_codes;

#[test]
fn awaited_union_with_promise_folds_to_value() {
    let source = r#"
function checkB<T>(x: Awaited<T | Promise<T>>): T {
    return x;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "Awaited<T | Promise<T>> must fold to T; got: {codes:?}"
    );
}

#[test]
fn awaited_union_with_promise_through_alias_folds_to_value() {
    let source = r#"
type _A<T> = Awaited<T | Promise<T>>;
function checkA<T>(x: _A<T>): T {
    return x;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "Awaited<T | Promise<T>> wrapped in an alias must still fold to T; got: {codes:?}"
    );
}

#[test]
fn awaited_through_inner_union_alias_folds_to_value() {
    let source = r#"
type _U<T> = T | Promise<T>;
function checkU<T>(x: Awaited<_U<T>>): T {
    return x;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "Awaited<X> must evaluate X (an alias to a value-or-promise union) before folding; \
         got: {codes:?}"
    );
}

#[test]
fn awaited_promise_through_alias_folds_to_value() {
    let source = r#"
type _AP<T> = Awaited<Promise<T>>;
function checkAP<T>(x: _AP<T>): T {
    return x;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "Awaited<Promise<T>> wrapped in an alias must still fold to T; got: {codes:?}"
    );
}

#[test]
fn awaited_triple_union_folds_to_value() {
    let source = r#"
type _Triple<T> = Awaited<T | Promise<T> | PromiseLike<T>>;
function checkTriple<T>(x: _Triple<T>): T {
    return x;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "Awaited over a value | Promise | PromiseLike union must fold to T; got: {codes:?}"
    );
}

#[test]
fn await_distributes_over_renamed_type_parameter_via_alias() {
    // Same shape as the issue repro: the type-parameter name (`Value` vs `T`)
    // must not matter — the fold is structural.
    let source = r#"
async function process<Value>(input: () => Value | Promise<Value>): Promise<Value> {
    const out: Value = await input();
    return out;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "await over `Value | Promise<Value>` must produce Value regardless of the parameter \
         name; got: {codes:?}"
    );
}

#[test]
fn awaited_nested_promise_through_alias_folds_to_value() {
    let source = r#"
type _Nested<T> = Awaited<Promise<T | Promise<T>>>;
function checkNested<T>(x: _Nested<T>): T {
    return x;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "Awaited<Promise<T | Promise<T>>> must recursively unwrap to T; got: {codes:?}"
    );
}

#[test]
fn awaited_keeps_non_thenable_unchanged() {
    // Non-thenables must pass through Awaited untouched — the fold is not
    // allowed to discard the value.
    let source = r#"
function checkString(x: Awaited<string>): string {
    return x;
}
function checkNumber(x: Awaited<number | undefined>): number | undefined {
    return x;
}
function checkNull(x: Awaited<null>): null {
    return x;
}
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "Awaited<NonThenable> must equal NonThenable; got: {codes:?}"
    );
}
