//! Tests for #14157 — `as const` object-literal property literals preserved
//! through generic-call inference.
//!
//! ## Structural rule
//!
//! When an object-literal property value comes from a *non-widening* source —
//! an `as const` assertion, a plain `as T` / `<T>expr` assertion, an identifier
//! whose declared type is non-widening, or a literal index access — the property
//! holds a *regular* (non-widening) literal. tsc's `getWidenedType` never widens
//! a regular literal, so the property survives generic-call inference unchanged:
//! `id({ single: true as const })` infers `{ single: true }`, not
//! `{ single: boolean }`.
//!
//! tsz records the decision on the interned property (`PropertyInfo.non_widening`)
//! at object-literal construction, so an `as const`-preserved `{ a: 1 }` interns
//! apart from a plain `{ a: 1 }` that is merely *deferred* under an object-typed
//! contextual parameter and must still widen. The solver's widening passes honour
//! the flag.
//!
//! The rule is keyed on structure, not identifier spelling: renaming the type
//! parameter, property names, or aliases must not change the decision
//! (anti-hardcoding directive).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_lib_files};
use tsz_common::common::ScriptTarget;

fn strict_opts() -> CheckerOptions {
    CheckerOptions {
        target: ScriptTarget::ES2015,
        strict: true,
        ..CheckerOptions::default()
    }
}

fn diags(source: &str) -> Vec<(u32, String)> {
    let libs = load_lib_files(&["es5.d.ts"]);
    check_source_with_libs(source, "test.ts", strict_opts(), &libs)
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn ts2322_codes(source: &str) -> Vec<u32> {
    diags(source)
        .into_iter()
        .map(|d| d.0)
        .filter(|&code| code == 2322)
        .collect()
}

#[test]
fn as_const_property_preserved_through_generic_identity() {
    // `id({ single: true as const })` must infer `{ single: true }`, so the
    // result is assignable to the literal target.
    let ok = r#"
declare function id<U>(u: U): U;
const x = id({ single: true as const });
const probe: { single: true } = x;
"#;
    assert!(
        ts2322_codes(ok).is_empty(),
        "as const property literal must survive generic inference: {:?}",
        diags(ok)
    );

    // The preserved literal is still `true`, not the opposite literal.
    let bad = r#"
declare function id<U>(u: U): U;
const x = id({ single: true as const });
const probe: { single: false } = x;
"#;
    assert_eq!(
        ts2322_codes(bad),
        vec![2322],
        "preserved literal must remain `true`, rejecting `false`: {:?}",
        diags(bad)
    );
}

#[test]
fn plain_property_still_widens_through_generic_identity() {
    // Regression guard: a *plain* literal property (no `as const`) must still
    // widen, matching tsc — `id({ a: 1 })` is `{ a: number }`.
    let source = r#"
declare function id<U>(u: U): U;
const x = id({ a: 1 });
const probe: { a: 1 } = x;
"#;
    assert_eq!(
        ts2322_codes(source),
        vec![2322],
        "plain property literal must widen to its primitive: {:?}",
        diags(source)
    );
}

#[test]
fn constrained_generic_preserves_as_const_but_widens_plain() {
    // Under an object-constrained type parameter the plain literal is deferred at
    // construction yet must still widen, while the `as const` one is preserved.
    let preserved = r#"
declare function id<U extends object>(u: U): U;
const x = id({ a: 1 as const });
const probe: { a: 1 } = x;
"#;
    assert!(
        ts2322_codes(preserved).is_empty(),
        "as const survives even under an object-constrained parameter: {:?}",
        diags(preserved)
    );

    let widened = r#"
declare function id<U extends object>(u: U): U;
const x = id({ a: 1 });
const probe: { a: 1 } = x;
"#;
    assert_eq!(
        ts2322_codes(widened),
        vec![2322],
        "plain literal still widens under an object-constrained parameter: {:?}",
        diags(widened)
    );
}

#[test]
fn mixed_const_and_plain_properties_widen_independently() {
    // `{ a: 1, b: 2 as const }` → `{ a: number, b: 2 }`: the plain property
    // widens while the `as const` one is preserved, per-property.
    let ok = r#"
declare function id<U>(u: U): U;
const x = id({ a: 1, b: 2 as const });
const probe: { a: number; b: 2 } = x;
"#;
    assert!(
        ts2322_codes(ok).is_empty(),
        "mixed object must widen `a` but preserve `b`: {:?}",
        diags(ok)
    );

    // `a` is widened, so it is not the literal `1`.
    let bad = r#"
declare function id<U>(u: U): U;
const x = id({ a: 1, b: 2 as const });
const probe: { a: 1; b: 2 } = x;
"#;
    assert_eq!(
        ts2322_codes(bad),
        vec![2322],
        "plain `a` must have widened away from literal `1`: {:?}",
        diags(bad)
    );
}

#[test]
fn string_literal_and_plain_assertion_preserved() {
    // `as const` string literal and a plain `as 0` assertion both yield regular
    // literals that survive inference.
    let source = r#"
declare function id<U>(u: U): U;
const s = id({ k: "x" as const });
const sp: { k: "x" } = s;
const n = id({ k: 0 as 0 });
const np: { k: 0 } = n;
"#;
    assert!(
        ts2322_codes(source).is_empty(),
        "string `as const` and `as 0` literals must survive inference: {:?}",
        diags(source)
    );
}

#[test]
fn object_assign_const_property_preserved() {
    // The original #14157 construct: `Object.assign(fn, { single: true as const })`
    // against `Func & { readonly single: true }` must type-check.
    let source = r#"
type StrictFunction = (...args: never) => unknown;
type Single<Func extends StrictFunction> = Func & { readonly single: true };
export const toSingle = <Func extends StrictFunction>(fn: Func): Single<Func> =>
  Object.assign(fn, { single: true as const });
"#;
    assert!(
        ts2322_codes(source).is_empty(),
        "Object.assign with an `as const` property must produce `Func & {{ single: true }}`: {:?}",
        diags(source)
    );
}

#[test]
fn rule_is_structural_not_identifier_keyed() {
    // Anti-hardcoding: rename the type parameter, function, property, and alias —
    // the preservation decision must be unchanged.
    let source = r#"
declare function wrap<Element>(value: Element): Element;
const renamed = wrap({ flag: true as const });
const probe: { flag: true } = renamed;
"#;
    assert!(
        ts2322_codes(source).is_empty(),
        "renamed binders must not change the as-const preservation: {:?}",
        diags(source)
    );
}
