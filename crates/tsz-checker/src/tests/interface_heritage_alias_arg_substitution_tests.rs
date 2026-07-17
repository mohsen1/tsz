//! Regression tests: an interface extending a *passthrough* generic type alias
//! applied to a generic object base is legal (M5 tanstack Pattern 1).
//!
//! Structural rule: `interface I<V> extends Alias<Base<V>>` is valid whenever
//! `Alias`, with its arguments substituted, resolves to an object type (or
//! intersection of object types). tsc runs `isValidBaseType` on the substituted
//! base. When the argument references `I`'s own (unbound) type parameter the full
//! instantiation erases, so tsz classifies the base from the alias body; before
//! the fix it used the RAW body, so a passthrough alias (`type Alias<T> = T`, or
//! `T & { ... }`) whose bare type-parameter body is not itself a valid base was
//! wrongly rejected with TS2312 — even though the substituted body (`Base<V>`) is
//! a valid object base. The fix substitutes the alias's arguments into its body
//! before the validity check.
//!
//! The negative guards confirm a genuinely generic body (mapped / conditional /
//! indexed-access / keyof, and a bare unconstrained parameter) still stays an
//! invalid base after substitution and keeps its real TS2312.
//!
//! Binder names are arbitrary (no tanstack source text).

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn check(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .iter()
    .map(|d| d.code)
    .collect()
}

// Positive: passthrough / intersection aliases applied to a generic object base
// must NOT emit TS2312.
#[test]
fn interface_extends_passthrough_alias_of_generic_base_no_ts2312() {
    let codes = check(
        r#"
interface BaseOpts<TVal = unknown> { key?: unknown; val?: TVal; }
type IdU<T> = T;
type Inter<T> = T & { extra: {} };
type Remap<T, K extends keyof T> = T & { [_ in K]: {} };
interface S1<TVal = unknown> extends IdU<BaseOpts<TVal>> { more?: boolean }
interface S2<TVal = unknown> extends Inter<BaseOpts<TVal>> { more?: boolean }
interface S3<TVal = unknown> extends Remap<BaseOpts<TVal>, 'key'> { more?: boolean }
"#,
    );
    assert!(
        !codes.contains(&2312),
        "passthrough/intersection alias of a generic object base is a valid interface base (no TS2312). Got: {codes:?}"
    );
}

// Nested passthrough alias chains over a free type parameter, and concrete-arg
// controls, must all stay clean: the outer substitution yields an object-typed
// application (`Base<V>` / an inner alias application), which is a valid base.
#[test]
fn interface_extends_nested_and_concrete_alias_bases_no_ts2312() {
    let codes = check(
        r#"
interface BaseOpts<TVal = unknown> { key?: unknown; val?: TVal; }
type IdA<T> = T;
type IdB<T> = T;
type Inter<T> = T & { extra: {} };
// nested alias chains over the enclosing free param
interface N1<TVal = unknown> extends IdA<IdB<BaseOpts<TVal>>> {}
interface N2<TVal = unknown> extends IdA<Inter<BaseOpts<TVal>>> {}
interface N3<TVal = unknown> extends Inter<IdA<BaseOpts<TVal>>> {}
// concrete-arg controls (clean before and after the fix)
interface C1 extends IdA<BaseOpts<number>> {}
interface C2 extends IdA<IdB<BaseOpts<string>>> {}
"#,
    );
    assert!(
        !codes.contains(&2312),
        "nested passthrough alias chains and concrete-arg alias bases are valid interface bases (no TS2312). Got: {codes:?}"
    );
}

// Negative: a genuinely generic non-object alias body stays an invalid base after
// substitution and must keep TS2312 (matching tsc). Each of the five bases is
// independently invalid, so tsc emits five TS2312.
#[test]
fn interface_extends_generic_non_object_alias_still_ts2312() {
    let codes = check(
        r#"
type GMap<T> = { [K in keyof T]: T[K] };
type GCond<T> = T extends string ? { a: 1 } : { b: 2 };
type GIdx<T> = T[keyof T];
type GKeyof<T> = keyof T;
type PassU<T> = T;
interface M1<T> extends GMap<T> {}
interface M2<T> extends GCond<T> {}
interface M3<T> extends GIdx<T> {}
interface M4<T> extends GKeyof<T> {}
interface P1<T> extends PassU<T> {}
"#,
    );
    let ts2312 = codes.iter().filter(|&&c| c == 2312).count();
    assert!(
        ts2312 >= 5,
        "generic non-object alias bases (mapped/conditional/indexed/keyof/bare-param) must each still emit TS2312. Got {ts2312} of code 2312 in: {codes:?}"
    );
}
