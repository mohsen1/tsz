//! Union call/construct resolution when a member declares the called property
//! with an overload set (multiple signatures).
//!
//! Structural rule: when every member of a union has a callable property,
//! `tsc` combines their signatures via `getUnionSignatures` — intersecting
//! parameter types and unioning return types — and the union is callable even
//! when one member's property is *overloaded*. tsz previously could only
//! combine members with exactly one signature each, so a single overloaded
//! member made the whole union look "not callable" (a false TS2349 / TS2351).
//!
//! Mined from jotai (`atomWithObservable.ts`'s `subscribable.subscribe(...)`).
//! Binder names are varied across cases so the checks exercise the structural
//! rule rather than any identifier or fixture-name fast path.

use tsz_checker::test_utils::{
    check_source_with_libs_code_messages, load_default_lib_files, strict_checker_options,
};

fn check(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs_code_messages(source, "test.ts", strict_checker_options(), &libs)
        .into_iter()
        .map(|(code, _)| code)
        .filter(|&code| code != 2318)
        .collect()
}

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

// ---------------------------------------------------------------------------
// Positive cases: the union is callable, no diagnostic.
// ---------------------------------------------------------------------------

/// The headline jotai repro: third member declares `subscribe` overloaded.
#[test]
fn union_with_overloaded_last_member_is_callable() {
    let codes = check(
        r#"
type Teardown = { unsubscribe: () => void }
type Sink<T> = { next: (value: T) => void }
type Source<T> =
  | { subscribe(observer: Sink<T>): Teardown }
  | { subscribe(observer: Partial<Sink<T>>): Teardown }
  | {
      subscribe(observer: Partial<Sink<T>>): Teardown
      subscribe(next: (value: T) => void): Teardown
    }
declare const source: Source<number>
source.subscribe({ next: (received) => { received.toFixed(); } })
"#,
    );
    assert!(
        codes.is_empty(),
        "a union whose last member has an overloaded method must be callable, got: {codes:?}"
    );
}

/// The overload set on the FIRST member instead of the last.
#[test]
fn union_with_overloaded_first_member_is_callable() {
    let codes = check(
        r#"
type Teardown = { close: () => void }
type Sink<T> = { push: (value: T) => void }
type Channel<T> =
  | {
      attach(handler: Partial<Sink<T>>): Teardown
      attach(push: (value: T) => void): Teardown
    }
  | { attach(handler: Sink<T>): Teardown }
  | { attach(handler: Partial<Sink<T>>): Teardown }
declare const channel: Channel<string>
channel.attach({ push: (value) => { value.length; } })
"#,
    );
    assert!(
        codes.is_empty(),
        "a union whose first member has an overloaded method must be callable, got: {codes:?}"
    );
}

/// Three or more overloads in one member.
#[test]
fn union_member_with_three_overloads_is_callable() {
    let codes = check(
        r#"
type Outcome = { tag: "ok" }
type Dispatcher =
  | { run(a: number): Outcome }
  | {
      run(a: number): Outcome
      run(a: number, b: string): Outcome
      run(a: number, b: string, c: boolean): Outcome
    }
declare const dispatcher: Dispatcher
dispatcher.run(1)
"#,
    );
    assert!(
        codes.is_empty(),
        "a union member with 3+ overloads must still be callable, got: {codes:?}"
    );
}

/// Positive control: a two-member union, neither overloaded, stays callable.
#[test]
fn two_member_union_no_overload_is_callable() {
    let codes = check(
        r#"
type Outcome = { tag: "ok" }
type Pair = { handle(x: number): Outcome } | { handle(x: number): Outcome }
declare const pair: Pair
pair.handle(1)
"#,
    );
    assert!(
        codes.is_empty(),
        "a plain two-member union without overloads must be callable, got: {codes:?}"
    );
}

/// Positive control: a single (non-union) overloaded type is callable on each
/// overload — the fix must not perturb this path.
#[test]
fn single_overloaded_type_is_callable() {
    let codes = check(
        r#"
type Outcome = { tag: "ok" }
type Overloaded = { pick(x: number): Outcome; pick(x: string): Outcome }
declare const overloaded: Overloaded
overloaded.pick(1)
overloaded.pick("a")
"#,
    );
    assert!(
        codes.is_empty(),
        "a single overloaded (non-union) type must be callable, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative cases: the union must stay genuinely uncallable / mismatched.
// ---------------------------------------------------------------------------

/// A union member whose property is NOT callable keeps the genuine TS2349.
#[test]
fn union_member_without_call_signature_reports_ts2349() {
    let codes = check(
        r#"
type Outcome = { tag: "ok" }
type Mixed =
  | { emit(a: number): Outcome; emit(a: string): Outcome }
  | { emit: number }
declare const mixed: Mixed
mixed.emit(1)
"#,
    );
    assert_eq!(
        count(&codes, 2349),
        1,
        "a union member whose property is not callable must report TS2349, got: {codes:?}"
    );
}

/// An argument that satisfies no overload still reports TS2345.
#[test]
fn union_overloaded_member_rejects_bad_argument() {
    let codes = check(
        r#"
type Outcome = { tag: "ok" }
type Dispatcher =
  | { run(a: number): Outcome }
  | { run(a: number): Outcome; run(a: number, b: string): Outcome }
declare const dispatcher: Dispatcher
dispatcher.run("nope")
"#,
    );
    assert_eq!(
        count(&codes, 2345),
        1,
        "an argument matching no overload must report TS2345, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Construct signatures (the analogous TS2351 path).
// ---------------------------------------------------------------------------

/// A union with an overloaded construct signature in one member is constructable.
#[test]
fn union_with_overloaded_construct_member_is_constructable() {
    let codes = check(
        r#"
type Widget = { id: number }
type Factory =
  | { new (a: number): Widget }
  | {
      new (a: number): Widget
      new (a: number, b: string): Widget
    }
declare const factory: Factory
new factory(1)
"#,
    );
    assert!(
        codes.is_empty(),
        "a union whose member has an overloaded construct signature must be constructable, got: {codes:?}"
    );
}
