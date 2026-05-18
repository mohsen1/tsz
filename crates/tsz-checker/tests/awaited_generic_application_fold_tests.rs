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

// ── User-defined PromiseLike unwrappers (issue #6374) ─────────────────────────

#[test]
fn user_defined_my_awaited_folds_promise_to_value() {
    let source = r#"
type MyAwaited<T> = T extends PromiseLike<infer U> ? MyAwaited<U> : T;
const x: MyAwaited<Promise<string>> = "hello";
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "MyAwaited<Promise<string>> must fold to string; got: {codes:?}"
    );
}

#[test]
fn user_defined_resolved_alias_folds_promise_to_value() {
    let source = r#"
type Resolved<T> = T extends PromiseLike<infer U> ? Resolved<U> : T;
const x: Resolved<Promise<number>> = 42;
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "Resolved<Promise<number>> must fold to number; got: {codes:?}"
    );
}

#[test]
fn user_defined_unwrap_alias_folds_nested_promise() {
    let source = r#"
type Unwrap<T> = T extends PromiseLike<infer U> ? Unwrap<U> : T;
const x: Unwrap<Promise<Promise<string>>> = "hello";
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "Unwrap<Promise<Promise<string>>> must fold to string; got: {codes:?}"
    );
}

#[test]
fn user_defined_awaited_alias_non_promise_conditional_not_treated_as_awaited() {
    // If `IsString` were mistakenly treated as Awaited-like, `IsString<PromiseLike<string>>`
    // would evaluate to `IsString<string>` = `true`, and assigning `true` would produce
    // NO error.  Any error here confirms the unwrapper check is not a false positive.
    let source = r#"
type IsString<T> = T extends string ? true : false;
const bad: IsString<PromiseLike<string>> = true;
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.is_empty(),
        "IsString<PromiseLike<string>> = true must produce a diagnostic; if codes is empty \
         IsString was incorrectly treated as an Awaited unwrapper; got: {codes:?}"
    );
}

#[test]
fn user_defined_awaited_alias_non_thenable_passes_through() {
    let source = r#"
type MyAwaited<T> = T extends PromiseLike<infer U> ? MyAwaited<U> : T;
const x: MyAwaited<string> = "hello";
const y: MyAwaited<number> = 42;
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "MyAwaited<NonThenable> must equal the non-thenable itself; got: {codes:?}"
    );
}

#[test]
fn user_defined_flatten_alias_with_promise_like_extends_folds() {
    let source = r#"
type Flatten<X> = X extends PromiseLike<infer V> ? Flatten<V> : X;
const x: Flatten<Promise<boolean>> = true;
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "Flatten<Promise<boolean>> must fold to boolean; got: {codes:?}"
    );
}
