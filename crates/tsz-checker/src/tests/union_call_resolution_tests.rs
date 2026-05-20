//! Regression tests for `resolve_union_call` (in tsz-solver) — specifically
//! the diagnostic-code routing for unions of callable types.
//!
//! Background: `resolve_union_call` has different branches for "exactly one
//! member has multiple overloads" vs "≥2 members have multiple overloads".
//! The `≥2` branch used to fall through to a deferred this-mismatch path that
//! emitted TS2684 instead of TS2349 when no compatible signature pair existed.
//! tsc routes the `no-compat` case through TS2349 unconditionally, like the
//! `==1` branch already did. This test file locks the fixed behavior.

use crate::test_utils::check_source_diagnostics;

/// `f3: F3 | F4` — both members have multi-overload `this`-typed signatures
/// with NO compatible pair. tsc emits TS2349 ("not callable"), NOT TS2684.
#[test]
fn union_two_multi_overload_no_compat_emits_ts2349_not_ts2684() {
    let diags = check_source_diagnostics(
        r#"
type A = { a: string };
type B = { b: number };
type C = { c: string };
type D = { d: number };

interface F3 {
  (this: A): void;
  (this: B): void;
}
interface F4 {
  (this: C): void;
  (this: D): void;
}

declare var x: A & C & {
  f: F3 | F4;
};
x.f();
"#,
    );

    let ts2349: Vec<_> = diags.iter().filter(|d| d.code == 2349).collect();
    assert_eq!(
        ts2349.len(),
        1,
        "Expected exactly one TS2349 for F3|F4 (no compatible overload pair), got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );

    let ts2684: Vec<_> = diags.iter().filter(|d| d.code == 2684).collect();
    assert!(
        ts2684.is_empty(),
        "TS2684 must not fire for F3|F4 with no compatible signatures — tsc emits only TS2349. Got: {:?}",
        ts2684.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// Companion lock: `f4: F3 | F5` — both multi-overload, but `(this: B)` is
/// in both members. The union IS callable; the actual `this` (`A & C & {...}`)
/// fails the intersection so TS2684 is correct here. Verifies the fix did not
/// over-shoot into the compat-found branch.
#[test]
fn union_two_multi_overload_compat_found_emits_ts2684() {
    let diags = check_source_diagnostics(
        r#"
type A = { a: string };
type B = { b: number };
type C = { c: string };

interface F3 {
  (this: A): void;
  (this: B): void;
}
interface F5 {
  (this: C): void;
  (this: B): void;
}

declare var x: A & C & {
  f: F3 | F5;
};
x.f();
"#,
    );

    let ts2684: Vec<_> = diags.iter().filter(|d| d.code == 2684).collect();
    assert_eq!(
        ts2684.len(),
        1,
        "Expected exactly one TS2684 for F3|F5 (this:B common but x not assignable to B), got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );

    let ts2349: Vec<_> = diags.iter().filter(|d| d.code == 2349).collect();
    assert!(
        ts2349.is_empty(),
        "TS2349 must not fire when a compatible signature exists. Got: {:?}",
        ts2349.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

#[test]
fn union_rest_tuple_callback_reports_nested_array_argument_mismatch() {
    let diags = check_source_diagnostics(
        r#"
declare function test2<
  A extends readonly unknown[],
  B extends readonly unknown[],
>(
  c: (...args: A) => void,
  d: (...args: B) => void,
  e: (arg: typeof c | typeof d) => void,
): void;

test2(
  (a: number | boolean, b: string | number) => {},
  (c: string | boolean, d: number | boolean) => {},
  (cb) => {
    cb(true, [42]);
  },
);
"#,
    );

    assert!(
        diags.iter().any(|d| {
            d.code == 2345
                && d.message_text.contains(
                    "Argument of type 'number[]' is not assignable to parameter of type 'number'.",
                )
        }),
        "Expected TS2345 for nested union callback rest tuple argument, got: {diags:?}"
    );
}

#[test]
fn correlated_index_access_argument_satisfies_union_callee_param_union() {
    let diags = check_source_diagnostics(
        r#"
type TypeMap = {
    foo: string,
    bar: number
};

type Keys = keyof TypeMap;
type HandlerMap = { [P in Keys]: (x: TypeMap[P]) => void };
declare const handlers: HandlerMap;
type DataEntry<K extends Keys = Keys> = { [P in K]: {
    type: P,
    data: TypeMap[P]
}}[K];

function process<K extends Keys>(block: DataEntry<K>) {
    handlers[block.type](block.data);
}
"#,
    );

    assert!(
        !diags.iter().any(|d| d.code == 2345),
        "Expected no TS2345 for correlated indexed-access union call, got: {diags:?}"
    );
}
