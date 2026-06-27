//! Regression guards for #14745: calling a method accessed on a UNION type must
//! stay callable when one union member declares that method with an *overload
//! set* (multiple call signatures).
//!
//! tsc's `getUnionSignatures` treats a member contributing an overload set the
//! same as a member contributing a single signature: a matching signature is
//! sought across every member's signature list using a *partial-subtype* match
//! (`compareSignaturesIdentical(partialMatch=true)` → `compareTypesSubtypeOf`),
//! not type identity. The union is callable when every member contributes at
//! least one call signature and a compatible signature is found (or, for the
//! single-multi-overload case, the master overload list is combined with the
//! other members). tsz previously gated this on *type identity* of the
//! parameter types, so an overloaded member whose parameter was a
//! subtype-but-not-identical sibling of another member's parameter was dropped,
//! collapsing the union to "not callable" (false TS2349).
//!
//! `Partial` is declared inline (as the mapped type it is in `lib.es5`) so the
//! test exercises the *expanded* `{ next?: ... }` shape rather than the `any`
//! the lib-less harness would otherwise produce — the expanded shape is what
//! makes the overloaded member's parameter a non-identical (but subtype-related)
//! sibling of the stricter member's parameter, which is the precise trigger.
//!
//! Binder names are varied across cases so the behaviour follows the type shape,
//! not a spelling.

use super::super::core::*;

const PARTIAL: &str = "type Partial<T> = { [P in keyof T]?: T[P] };\n";

fn diagnostics_for(body: &str) -> Vec<(u32, String)> {
    compile_and_get_diagnostics(&format!("{PARTIAL}{body}"))
}

/// The minimal jotai witness (non-generic): a union of three object types where
/// the third member declares `subscribe` with two overloaded call signatures and
/// the first member's parameter `Observer` is a strict subtype of the overloaded
/// member's `Partial<Observer>` parameter. Every member has a callable
/// `subscribe`, so the call must resolve (no TS2349).
#[test]
fn union_member_overloaded_method_is_callable() {
    let diags = diagnostics_for(
        r#"
type Subscription = { unsubscribe: () => void };
type Observer = { next: (value: number) => void };
type Sub =
  | { subscribe(observer: Observer): Subscription }
  | { subscribe(observer: Partial<Observer>): Subscription }
  | {
      subscribe(observer: Partial<Observer>): Subscription;
      subscribe(next: (value: number) => void): Subscription;
    };
declare const s: Sub;
s.subscribe({ next: (d) => {} });
export {};
"#,
    );
    assert!(
        !has_error(&diags, 2349),
        "no TS2349 expected — every member of the union has a callable `subscribe`, \
         even though one member's `subscribe` is overloaded. Actual: {diags:#?}"
    );
}

/// The generic-instantiated jotai witness: `Sub<number>` resolves the member
/// parameters to concrete expanded shapes before the union call. Same trigger,
/// must remain callable.
#[test]
fn union_member_overloaded_method_generic_is_callable() {
    let diags = diagnostics_for(
        r#"
type Subscription = { unsubscribe: () => void };
type Observer<T> = { next: (value: T) => void };
type Sub<T> =
  | { subscribe(observer: Observer<T>): Subscription }
  | { subscribe(observer: Partial<Observer<T>>): Subscription }
  | {
      subscribe(observer: Partial<Observer<T>>): Subscription;
      subscribe(next: (value: T) => void): Subscription;
    };
declare const s: Sub<number>;
s.subscribe({ next: (d) => {} });
export {};
"#,
    );
    assert!(
        !has_error(&diags, 2349),
        "no TS2349 expected — `Sub<number>` instantiates the member parameters to \
         concrete shapes; the overloaded member must still be callable. \
         Actual: {diags:#?}"
    );
}

/// The full jotai overload shape: the overloaded member's second signature is a
/// multi-parameter `(next, error?, complete?)` form. The match is still found
/// against the single-parameter `Partial<Observer>` overload, so the union is
/// callable.
#[test]
fn union_member_overloaded_multiparam_is_callable() {
    let diags = diagnostics_for(
        r#"
type Subscription = { unsubscribe: () => void };
type Observer<T> = {
  next: (value: T) => void;
  error: (error: unknown) => void;
  complete: () => void;
};
type Sub<T> =
  | { subscribe(observer: Observer<T>): Subscription }
  | { subscribe(observer: Partial<Observer<T>>): Subscription }
  | {
      subscribe(observer: Partial<Observer<T>>): Subscription;
      subscribe(
        next: (value: T) => void,
        error?: (error: unknown) => void,
        complete?: () => void,
      ): Subscription;
    };
declare const s: Sub<number>;
s.subscribe({ next: (d) => {} });
export {};
"#,
    );
    assert!(
        !has_error(&diags, 2349),
        "no TS2349 expected — the overload set mixing single- and multi-parameter \
         `subscribe` signatures must still be callable on the union. \
         Actual: {diags:#?}"
    );
}

/// Adjacent case: the overload set lives on the FIRST union member rather than
/// the last. The match is position-independent, so the union is still callable.
#[test]
fn union_member_overloaded_method_first_member_is_callable() {
    let diags = diagnostics_for(
        r#"
type Sink = { close: () => void };
type Watcher = { tick: (value: string) => void };
type Stream =
  | {
      observe(watcher: Partial<Watcher>): Sink;
      observe(tick: (value: string) => void): Sink;
    }
  | { observe(watcher: Watcher): Sink }
  | { observe(watcher: Partial<Watcher>): Sink };
declare const stream: Stream;
stream.observe({ tick: (v) => {} });
export {};
"#,
    );
    assert!(
        !has_error(&diags, 2349),
        "no TS2349 expected — overload set on the first union member must not drop \
         the member's contribution. Actual: {diags:#?}"
    );
}

/// Control: a two-member union with single (non-overloaded) signatures whose
/// parameters are subtype-related (`Listener` <: `Partial<Listener>`) stays
/// callable — the existing happy path must be unaffected by the fix.
#[test]
fn union_member_single_signature_methods_callable_control() {
    let diags = diagnostics_for(
        r#"
type Handle = { drop: () => void };
type Listener = { on: (event: number) => void };
type Source =
  | { attach(listener: Listener): Handle }
  | { attach(listener: Partial<Listener>): Handle };
declare const src: Source;
src.attach({ on: (e) => {} });
export {};
"#,
    );
    assert!(
        !has_error(&diags, 2349),
        "no TS2349 expected — two-member union with single signatures (control). \
         Actual: {diags:#?}"
    );
}

/// Control: a single (non-union) type with an overloaded method stays callable.
#[test]
fn single_overloaded_type_method_callable_control() {
    let diags = diagnostics_for(
        r#"
type Receipt = { ack: () => void };
type Feed = {
  push(consumer: Partial<{ take: (value: boolean) => void }>): Receipt;
  push(take: (value: boolean) => void): Receipt;
};
declare const feed: Feed;
feed.push({ take: (m) => {} });
export {};
"#,
    );
    assert!(
        !has_error(&diags, 2349),
        "no TS2349 expected — single overloaded type method (control). \
         Actual: {diags:#?}"
    );
}

/// Negative control: when one union member's accessed member is NOT callable (a
/// plain property), the union is genuinely not callable and tsc DOES report
/// TS2349. The fix must not over-relax this.
#[test]
fn union_member_without_call_signature_is_not_callable() {
    let diags = diagnostics_for(
        r#"
type Subscription = { unsubscribe: () => void };
type Observer = { next: (value: number) => void };
type Mixed =
  | { subscribe(observer: Observer): Subscription }
  | { subscribe: number }
  | {
      subscribe(observer: Partial<Observer>): Subscription;
      subscribe(next: (value: number) => void): Subscription;
    };
declare const m: Mixed;
m.subscribe({ next: (d) => {} });
export {};
"#,
    );
    assert!(
        has_error(&diags, 2349),
        "TS2349 expected — one union member's `subscribe` is a non-callable number, \
         so the union is genuinely not callable. Actual: {diags:#?}"
    );
}

/// Negative control: a genuine argument-type mismatch on the union call must
/// still be reported (TS2345), not silently accepted. Passing a `string` where
/// every overload/member expects an object/function argument is wrong.
#[test]
fn union_member_overloaded_method_rejects_bad_argument() {
    let diags = diagnostics_for(
        r#"
type Subscription = { unsubscribe: () => void };
type Observer = { next: (value: number) => void };
type Sub =
  | { subscribe(observer: Observer): Subscription }
  | { subscribe(observer: Partial<Observer>): Subscription }
  | {
      subscribe(observer: Partial<Observer>): Subscription;
      subscribe(next: (value: number) => void): Subscription;
    };
declare const s: Sub;
s.subscribe("nope");
export {};
"#,
    );
    assert!(
        has_error(&diags, 2345) || has_error(&diags, 2769),
        "an argument-type mismatch must still be reported even though the union is \
         now callable. Actual: {diags:#?}"
    );
}
