//! Regression tests for issue #14262.
//!
//! When a generic call's return type is a bare type parameter `T` that is ALSO
//! directly seeded by a concrete value argument, `tsc` lets argument inference
//! own `T` and treats an outer contextual-return type — including an `as`-cast
//! target like `as never` or `as { ... }` — as a low-priority hint that cannot
//! override argument inference. tsz previously constrained the bare placeholder
//! against the contextual type as an upper bound (and propagated it into the
//! callback's contextual signature), so `as never` clamped `T = never` and broke
//! the value argument and callback (TS2322 / TS2769 / TS2698).
//!
//! Anti-hardcoding: the structural rule is "the return type parameter is the
//! naked type of a value parameter pinned by a concrete argument", so the tests
//! vary binder names and argument shapes rather than matching any specific
//! identifier or rendered message.

use tsz_checker::test_utils::{check_source_diagnostics, diagnostic_code_message_refs};

const CLAMP_CODES: &[u32] = &[2322, 2345, 2769, 2698];

fn assert_no_clamp(source: &str, context: &str) {
    let diagnostics = check_source_diagnostics(source);
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| CLAMP_CODES.contains(&diagnostic.code)),
        "{context}: expected no contextual-return clamp diagnostic, got {:#?}",
        diagnostic_code_message_refs(&diagnostics),
    );
}

fn assert_has_code(source: &str, code: u32, context: &str) {
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.code == code),
        "{context}: expected TS{code}, got {:#?}",
        diagnostic_code_message_refs(&diagnostics),
    );
}

#[test]
fn as_never_does_not_clamp_argument_pinned_return_param() {
    // The original witness: `reduce`'s `T` is pinned by `init`, the callback adds
    // a computed key. The outer `as never` must not clamp `T`.
    assert_no_clamp(
        r#"
declare function reduce<T>(init: T, fn: (acc: T, key: string) => T): T
const r = reduce({ x: 1 }, (acc, key) => ({ ...acc, [key]: 1 })) as never
"#,
        "as never over a value-pinned bare return parameter",
    );
}

#[test]
fn as_never_with_function_member_argument_does_not_clamp() {
    // The pinning argument is an object literal that also carries a function
    // member, which previously defeated the contextual-return suppression.
    assert_no_clamp(
        r#"
declare function reduce<T>(init: T, fn: (acc: T, key: string) => T): T
const r = reduce({ x: 1, m: () => 2 }, (acc, key) => ({ ...acc, [key]: 1 })) as never
"#,
        "function-member-bearing pinning argument",
    );
}

#[test]
fn as_narrower_object_cast_does_not_clamp() {
    // `as SomeNarrower` is the same family: the cast target is an object type.
    assert_no_clamp(
        r#"
declare function fold<S>(seed: S, step: (acc: S, k: string) => S): S
const r = fold({ a: 1, m: () => 9 }, (acc, k) => ({ ...acc, [k]: 1 })) as { a: number }
"#,
        "as narrower-object over a value-pinned bare return parameter",
    );
}

#[test]
fn renamed_binders_do_not_clamp() {
    // Anti-hardcoding: arbitrary binder names must behave identically.
    assert_no_clamp(
        r#"
declare function QQ<ZZZ>(init: ZZZ, cb: (state: ZZZ, key: string) => ZZZ): ZZZ
const out = QQ({ p: 1, fn: () => 7 }, (state, key) => ({ ...state, [key]: 1 })) as never
"#,
        "renamed binders over a value-pinned bare return parameter",
    );
}

#[test]
fn return_param_only_from_context_still_applies_cast() {
    // Negative control: when `U` is seeded ONLY by the contextual type (no value
    // argument pins it), the cast must still apply and narrow `U`.
    assert_no_clamp(
        r#"
declare function make<U>(): U
const r = make() as never
const s: number = make<number>()
"#,
        "context-only return parameter keeps cast semantics",
    );
}

#[test]
fn literal_union_annotation_is_preserved_for_callback_only_return() {
    // Regression guard: callback-only seeded return parameters still use the
    // contextual type to preserve literal unions (the upper-bound path).
    assert_no_clamp(
        r#"
declare function invoke<T>(f: () => T): T
let x: 0 | 1 | 2 = invoke(() => 1)
"#,
        "literal-union contextual preserved for callback-only return",
    );
}

#[test]
fn assignment_to_never_is_still_a_real_error() {
    // The fix must not silence a genuine assignment error: assigning the
    // argument-inferred object type to a `never`-annotated binding still fails.
    assert_has_code(
        r#"
declare function reduce<T>(init: T, fn: (acc: T, key: string) => T): T
const r: never = reduce({ x: 1 }, (acc, key) => ({ ...acc, [key]: 1 }))
"#,
        2322,
        "object inferred from arguments is not assignable to never",
    );
}
