//! Regression tests for #14385.
//!
//! Structural rule: when two object types are related and a `this`-relative
//! member is an `infer`-introducing conditional over a deferred `this[...]`
//! indexed access (e.g. `this["rawArgs"] extends infer a extends unknown[] ? a
//! : never`), binding each member's `this` to its own receiver leaves the
//! member as a deferred `Conditional` keyed on that receiver. The two members
//! then differ only by receiver and the relation reports them as unrelated even
//! though both reduce to the same concrete branch (here `never`). `tsc` reduces
//! a conditional once its check type is concrete, so the members relate and the
//! enclosing generic constraint is satisfied — no spurious TS2344.
//!
//! Mined from the higher-kinded-type `Fn` combinator pattern in `hotscript`,
//! where `PartialApply extends Fn` is checked against an `Fn | unset`
//! constraint.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
        &libs,
    )
    .iter()
    .map(|diagnostic| (diagnostic.code, diagnostic.message_text.clone()))
    .collect()
}

fn assert_clean(source: &str, label: &str) {
    let diagnostics = diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "[{label}] expected clean, got {diagnostics:?}"
    );
}

/// The witnessed repro: an `infer`-bearing `this`-relative conditional member on
/// a self-referential interface used as both heritage base and generic
/// constraint. `tsc` accepts; tsz previously emitted TS2344.
#[test]
fn partial_apply_satisfies_fn_union_unset_constraint() {
    let source = r#"
declare const unsetSym: unique symbol;
type unset = typeof unsetSym;

interface Fn {
  rawArgs: unknown;
  args: this["rawArgs"] extends infer a extends unknown[] ? a : never;
  return: unknown;
}

interface PartialApply<fn extends Fn, partialArgs extends unknown[]> extends Fn {
  rawArgs: unknown;
  return: never;
}

type Apply<fn extends Fn | unset, args> = fn extends Fn ? fn : never;
type Get<K> = PartialApply<Fn, [K]>;
type X = Apply<Get<"length">, []>;
export {};
"#;
    assert_clean(source, "PartialApply<Fn> satisfies Fn | unset");
}

/// Same structural shape with every binder renamed — guards against any
/// name-driven fast path (anti-hardcoding).
#[test]
fn renamed_binders_keep_the_constraint_clean() {
    let source = r#"
declare const NONE: unique symbol;

interface Hkt {
  raw: unknown;
  out: this["raw"] extends infer r extends unknown[] ? r : never;
  ret: unknown;
}

interface Bind<f extends Hkt, ps extends unknown[]> extends Hkt {
  raw: unknown;
  ret: never;
}

type Run<f extends Hkt | typeof NONE, a> = f extends Hkt ? f : never;
type Pick1<K> = Bind<Hkt, [K]>;
type Y = Run<Pick1<"size">, []>;
export {};
"#;
    assert_clean(source, "renamed HKT binders stay clean");
}

/// The `this`-relative conditional member declared directly on the derived
/// interface (not inherited) is also accepted.
#[test]
fn member_declared_on_derived_interface_is_clean() {
    let source = r#"
declare const unsetSym: unique symbol;
type unset = typeof unsetSym;

interface Fn {
  rawArgs: unknown;
  return: unknown;
}

interface PartialApply<fn extends Fn, partialArgs extends unknown[]> extends Fn {
  rawArgs: unknown;
  args: this["rawArgs"] extends infer a extends unknown[] ? a : never;
  return: never;
}

type Apply<fn extends Fn | unset, args> = fn extends Fn ? fn : never;
type Get<K> = PartialApply<Fn, [K]>;
type X = Apply<Get<"length">, []>;
export {};
"#;
    assert_clean(source, "member declared on derived interface");
}

/// Two independent instantiations of the same generic combinator both relate
/// against the constraint — the reduction is per-receiver, not a one-shot
/// cache artifact.
#[test]
fn two_instantiations_both_clean() {
    let source = r#"
declare const Sym: unique symbol;

interface Fn {
  rawArgs: unknown;
  args: this["rawArgs"] extends infer a extends unknown[] ? a : never;
  return: unknown;
}

interface PartialApply<fn extends Fn, partialArgs extends unknown[]> extends Fn {
  rawArgs: unknown;
  return: never;
}

type Apply<fn extends Fn | typeof Sym, args> = fn extends Fn ? fn : never;
type Get<K> = PartialApply<Fn, [K]>;
type X1 = Apply<Get<"a">, []>;
type X2 = Apply<Get<"b">, []>;
export {};
"#;
    assert_clean(source, "two instantiations both clean");
}
