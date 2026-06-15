//! Regression tests for issue #13619: a homomorphic mapped type applied to an
//! object that carries a well-known-symbol member (`[Symbol.iterator]`,
//! `[Symbol.asyncIterator]`) must preserve that member so the result stays
//! iterable.
//!
//! Structural rule: when a homomorphic mapped type `{ [K in keyof T]: F<T[K]> }`
//! iterates over `keyof T`, the well-known-symbol keys in `keyof T` are modeled
//! as `UniqueSymbol(SymbolRef)`. The materialized property — and the `T[K]`
//! indexed access used to compute its value — must round-trip that `SymbolRef`
//! back to its canonical `[Symbol.xxx]` shape key (not the synthetic
//! `__unique_N` placeholder reserved for user-authored unique symbols).
//! Otherwise the symbol method is dropped/typed as `undefined`, and a `for-of`
//! (or spread / destructuring) over the mapped result wrongly reports TS2488.
//!
//! Owner: solver `keyof` -> mapped-type materialization and indexed-access
//! evaluation over well-known-symbol keys. The tests vary alias / type-parameter
//! names to confirm the behaviour is binder-name-independent.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};

fn diagnostic_codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    assert!(!libs.is_empty(), "default lib files must be available");
    let diags = check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs);
    for d in &diags {
        println!(
            "DIAG code={} start={} msg={}",
            d.code, d.start, d.message_text
        );
    }
    diags.into_iter().map(|d| d.code).collect()
}

/// The reduced `DeepReadonly`-style recursive homomorphic mapped type that
/// reproduces the ts-essentials micro-bench failure: the template is a
/// *conditional wrapper* over `T[K]` (non-identity), which forces the per-key
/// `DeepReadonly<T[K]>` instantiation path through indexed access by the
/// well-known symbol key.
const DEEP_READONLY: &str = r#"
type Builtin = Function | Date | Error | RegExp;
type DeepReadonly<T> =
  T extends Builtin ? T
  : T extends ReadonlyArray<infer U> ? ReadonlyArray<DeepReadonly<U>>
  : T extends object ? { readonly [K in keyof T]: DeepReadonly<T[K]> }
  : T;
"#;

#[test]
fn for_of_over_deep_readonly_iterable_with_extra_prop_no_ts2488() {
    let codes = diagnostic_codes(&format!(
        r#"{DEEP_READONLY}
interface IterableWithExtraProp<T> {{
  [Symbol.iterator](): Iterator<T>;
  size: number;
}}
declare const x: DeepReadonly<IterableWithExtraProp<number>>;
for (const item of x) {{ const n: number = item; }}
"#
    ));
    assert!(
        !codes.contains(&2488),
        "DeepReadonly over an iterable-with-extra-prop must stay iterable; got: {codes:?}"
    );
}

#[test]
fn for_of_over_deep_readonly_plain_iterable_no_ts2488() {
    let codes = diagnostic_codes(&format!(
        r#"{DEEP_READONLY}
declare const y: DeepReadonly<Iterable<string>>;
for (const item of y) {{ const s: string = item; }}
"#
    ));
    assert!(
        !codes.contains(&2488),
        "DeepReadonly over a plain Iterable must stay iterable; got: {codes:?}"
    );
}

#[test]
fn spread_of_deep_readonly_iterable_no_ts2488() {
    let codes = diagnostic_codes(&format!(
        r#"{DEEP_READONLY}
interface Bag<T> {{ [Symbol.iterator](): Iterator<T>; size: number; }}
declare const a: DeepReadonly<Bag<number>>;
const arr = [...a];
"#
    ));
    assert!(
        !codes.contains(&2488),
        "spread over a DeepReadonly iterable must not report TS2488; got: {codes:?}"
    );
}

#[test]
fn array_destructuring_of_deep_readonly_iterable_no_ts2488() {
    let codes = diagnostic_codes(&format!(
        r#"{DEEP_READONLY}
declare const b: DeepReadonly<Iterable<string>>;
const [first] = b;
"#
    ));
    assert!(
        !codes.contains(&2488),
        "array destructuring of a DeepReadonly iterable must not report TS2488; got: {codes:?}"
    );
}

/// Identity homomorphic template (`T[K]`) with a *different* alias and
/// type-parameter name — guards against any binder-name dependence and against
/// the readonly/optional modifiers being applied to the symbol member.
#[test]
fn for_of_over_identity_mapped_iterable_no_ts2488() {
    let codes = diagnostic_codes(
        r#"
type Frozen<Src> = { readonly [Prop in keyof Src]: Src[Prop] };
interface Stream<E> { [Symbol.iterator](): Iterator<E>; count: number; }
declare const s: Frozen<Stream<boolean>>;
for (const e of s) { const b: boolean = e; }
"#,
    );
    assert!(
        !codes.contains(&2488),
        "identity homomorphic mapped iterable must stay iterable; got: {codes:?}"
    );
}

/// `for await ... of` over a `DeepReadonly` async-iterable must likewise keep the
/// `[Symbol.asyncIterator]` member.
#[test]
fn for_await_of_deep_readonly_async_iterable_no_ts2504() {
    let codes = diagnostic_codes(&format!(
        r#"{DEEP_READONLY}
interface MyAsync<T> {{ [Symbol.asyncIterator](): AsyncIterator<T>; size: number; }}
async function run(c: DeepReadonly<MyAsync<number>>) {{
  for await (const x of c) {{ const n: number = x; }}
}}
"#
    ));
    assert!(
        !codes.contains(&2504),
        "DeepReadonly over an async iterable must stay async-iterable; got: {codes:?}"
    );
}

/// User-authored (non-well-known) unique-symbol members must keep round-tripping
/// through a homomorphic mapped type — the fix must not regress the synthetic
/// `__unique_N` path.
#[test]
fn user_unique_symbol_member_preserved_through_mapped() {
    let codes = diagnostic_codes(
        r#"
declare const tag: unique symbol;
interface Tagged { [tag]: number; label: string; }
type Frozen<T> = { readonly [K in keyof T]: T[K] };
declare const t: Frozen<Tagged>;
const v: number = t[tag];
"#,
    );
    assert!(
        !codes.contains(&2339) && !codes.contains(&2538),
        "user unique-symbol member must survive the mapped type; got: {codes:?}"
    );
}

/// Negative guard: a homomorphic mapped type over a *non-iterable* object must
/// still report TS2488 — the fix must not make every mapped object iterable.
#[test]
fn for_of_over_mapped_non_iterable_still_reports_ts2488() {
    let codes = diagnostic_codes(
        r#"
type Frozen<T> = { readonly [K in keyof T]: T[K] };
interface NotIterable { size: number; name: string; }
declare const a: Frozen<NotIterable>;
for (const x of a) {}
"#,
    );
    assert!(
        codes.contains(&2488),
        "a mapped non-iterable object must still report TS2488; got: {codes:?}"
    );
}
