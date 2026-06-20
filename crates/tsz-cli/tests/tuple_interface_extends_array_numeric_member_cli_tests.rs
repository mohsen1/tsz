//! A tuple literal satisfies an interface that `extends Array` and adds an own
//! numeric member (issue #14170).
//!
//! Structural rule: when `interface I<A> extends Array<A> { 0: A }`, a tuple
//! source `[a]` is assignable to `I<…>` in `tsc`. The interface's inherited
//! `this`-returning Array members (`fill`/`sort`/`reverse`/`copyWithin`) resolve
//! `this` to the *target* receiver. `tsz` compared a synthetic `Array<A>`
//! surface whose `this`-returns stayed polymorphic (then resolved to
//! `Array<A>`), forcing `Array<A> <: I<…>` (false) and a spurious `TS2322`.
//! Resolving `this` per side — source surface bound to the tuple source, target
//! members bound to the target — reduces a `this`-return to `source <: target`
//! (the relation already in progress), satisfied coinductively, matching `tsc`.
//!
//! Run against the bundled `es2022` lib so `Array` carries its real
//! `this`-returning members; this is exactly where the divergence lived.
//!
//! Binder names are varied (anti-hardcoding) and the negative controls are
//! pinned so the acceptance is not a blanket "tuple ⇒ interface-extends-Array".

use crate::args::CliArgs;
use clap::Parser;

/// Compile a single-file program with the bundled `es2022` lib and return the
/// non-hint diagnostic codes.
fn check(src: &str) -> Vec<u32> {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("main.ts"), src).expect("write source");
    let args = CliArgs::try_parse_from([
        "tsz",
        "--ignoreConfig",
        "--noEmit",
        "--strict",
        "--target",
        "es2022",
        "--lib",
        "es2022",
        "main.ts",
    ])
    .expect("parse args");
    let result = crate::driver::compile(&args, dir.path()).expect("compile");
    result
        .diagnostics
        .iter()
        .map(|d| d.code)
        .filter(|code| code / 100 != 61) // drop unused-symbol hints
        .collect()
}

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|c| **c == code).count()
}

// ---- Positive: tsc-clean cases that tsz wrongly rejected with TS2322. ----

#[test]
fn generic_tuple_literal_returned_from_generic_arrow() {
    // Exact issue repro shape.
    let codes = check(
        r#"
interface NonEmptyArray<A> extends Array<A> {
  0: A
}
const singleton = <A>(a: A): NonEmptyArray<A> => [a];
"#,
    );
    assert_eq!(count(&codes, 2322), 0, "got {codes:?}");
}

#[test]
fn generic_tuple_annotation() {
    let codes = check(
        r#"
interface NE<A> extends Array<A> {
  0: A
}
const x: NE<number> = [1];
"#,
    );
    assert_eq!(count(&codes, 2322), 0, "got {codes:?}");
}

#[test]
fn nongeneric_interface_extends_array() {
    let codes = check(
        r#"
interface NE extends Array<number> {
  0: number
}
const x: NE = [1];
"#,
    );
    assert_eq!(count(&codes, 2322), 0, "got {codes:?}");
}

#[test]
fn multiple_numeric_members_satisfied_by_matching_tuple() {
    let codes = check(
        r#"
interface Pair<A> extends Array<A> {
  0: A
  1: A
}
const ok: Pair<number> = [1, 2];
"#,
    );
    assert_eq!(count(&codes, 2322), 0, "got {codes:?}");
}

#[test]
fn renamed_binders_are_irrelevant() {
    // Anti-hardcoding: the rule is structural, never keyed on a spelling.
    let codes = check(
        r#"
interface Crate<Widget> extends Array<Widget> {
  0: Widget
}
const y: Crate<string> = ["a"];
"#,
    );
    assert_eq!(count(&codes, 2322), 0, "got {codes:?}");
}

#[test]
fn returned_from_generic_function_body() {
    let codes = check(
        r#"
interface NE<A> extends Array<A> {
  0: A
}
function make<T>(t: T): NE<T> {
  return [t];
}
"#,
    );
    assert_eq!(count(&codes, 2322), 0, "got {codes:?}");
}

// ---- Negative controls: these must still report TS2322 (parity with tsc). ----

#[test]
fn plain_array_source_cannot_prove_numeric_member() {
    // A plain array has no fixed `0` slot to satisfy `0: A`; tsc reports TS2741.
    let codes = check(
        r#"
interface NE<A> extends Array<A> {
  0: A
}
declare const a: number[];
const x: NE<number> = a;
"#,
    );
    // tsc surfaces the missing-property form (TS2741); pin that it is rejected.
    assert!(
        count(&codes, 2741) + count(&codes, 2322) >= 1,
        "expected a rejection for number[] vs NE<number>, got {codes:?}"
    );
}

#[test]
fn wrong_element_type_rejected() {
    let codes = check(
        r#"
interface NE<A> extends Array<A> {
  0: A
}
const x: NE<string> = [1];
"#,
    );
    assert_eq!(count(&codes, 2322), 1, "got {codes:?}");
}

#[test]
fn extra_required_own_member_not_on_tuple_rejected() {
    let codes = check(
        r#"
interface NE<A> extends Array<A> {
  0: A
  extra: string
}
const x: NE<number> = [1];
"#,
    );
    assert_eq!(count(&codes, 2322), 1, "got {codes:?}");
}

#[test]
fn tuple_too_short_for_required_numeric_member_rejected() {
    let codes = check(
        r#"
interface Pair<A> extends Array<A> {
  0: A
  1: A
}
const x: Pair<number> = [1];
"#,
    );
    assert_eq!(count(&codes, 2322), 1, "got {codes:?}");
}
