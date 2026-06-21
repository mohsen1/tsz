//! TS2556 for non-tuple array spreads into the rest parameter of an
//! **overloaded** callable.
//!
//! Structural rule: `tsc` resolves overloads existentially. A non-tuple array
//! (or iterable) spread is legal as long as *some* call/construct signature has
//! a rest parameter that can absorb it at the spread position — exactly
//! `hasCorrectArity`'s spread branch, where TS2556 fires only when *no*
//! signature has correct arity for the spread. A sibling overload without a
//! rest parameter (e.g. `Array.prototype.splice`'s `(start, deleteCount?)`
//! overload) must not poison a valid spread into another overload's rest.
//!
//! Owner layer: the spread-position decision routes through the solver
//! `allows_non_tuple_spread_position` boundary; the overload-resolution first
//! pass marks the synthetic `sigA | sigB | ...` contextual union as an
//! existential alternative set so the union check is disjunctive rather than
//! conjunctive. A genuine union-typed *value* keeps the conjunctive semantics.
//!
//! Anti-hardcoding: every fixture varies the receiver/method/binder names so the
//! rule is exercised structurally, never keyed on an identifier or on `splice`.

use tsz_checker::test_utils::{check_source_diagnostics, diagnostic_count};

// ---------------------------------------------------------------------------
// Positive: spread into a rest-bearing overload -> no TS2556.
// ---------------------------------------------------------------------------

#[test]
fn mutable_and_readonly_array_spread_into_splice_shaped_overload_is_clean() {
    // The remeda witness, modelled on `Array.prototype.splice`'s overload set:
    // a no-rest `(head, mid?)` signature plus a rest-bearing
    // `(head, mid, ...tail)` signature, with the rest sitting after two fixed
    // args. Both a mutable `U[]` and a `readonly U[]` may spread into the rest,
    // and the rest-bearing signature is declared second (the lib `splice` order).
    let src = r#"
        interface Sink {
            push(head: number, mid?: number): void;
            push(head: number, mid: number, ...tail: string[]): void;
        }
        declare const sink: Sink;
        declare const mut: string[];
        declare const ro: readonly string[];
        sink.push(1, 2, ...mut);
        sink.push(1, 2, ...ro);
    "#;
    let diags = check_source_diagnostics(src);
    assert_eq!(
        diagnostic_count(&diags, 2556),
        0,
        "spread into a rest-bearing overload (declared after a no-rest sibling) must be clean: {diags:?}"
    );
}

#[test]
fn rest_bearing_overload_declared_first_is_clean() {
    // Declaration order must not matter: rest-bearing overload first.
    let src = r#"
        interface Box {
            put(head: number, mid: number, ...tail: string[]): void;
            put(head: number, mid?: number): void;
        }
        declare const box: Box;
        declare const xs: string[];
        box.put(1, 2, ...xs);
    "#;
    let diags = check_source_diagnostics(src);
    assert_eq!(
        diagnostic_count(&diags, 2556),
        0,
        "rest-bearing overload declared first must be clean: {diags:?}"
    );
}

#[test]
fn spread_with_zero_one_and_many_leading_fixed_args_is_clean() {
    // The rest slot may begin after 0, 1, or N fixed positional args, in each
    // case alongside a no-rest sibling overload.
    let src = r#"
        interface Multi {
            f(...rest: string[]): void;
            f(only: number): void;
        }
        interface One {
            g(head: number, ...rest: string[]): void;
            g(head: number, extra?: boolean): void;
        }
        interface Many {
            h(a: number, b: number, c: number, ...rest: string[]): void;
            h(a: number, b: number): void;
        }
        declare const m: Multi;
        declare const o: One;
        declare const k: Many;
        declare const ss: string[];
        m.f(...ss);
        o.g(1, ...ss);
        k.h(1, 2, 3, ...ss);
    "#;
    let diags = check_source_diagnostics(src);
    assert_eq!(
        diagnostic_count(&diags, 2556),
        0,
        "leading-fixed-arg variations spreading into a rest overload must be clean: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Negative: no overload has a rest at the spread position -> TS2556.
// ---------------------------------------------------------------------------

#[test]
fn overload_set_without_any_rest_still_emits_ts2556() {
    // Neither overload has a rest parameter, so a non-tuple spread cannot land
    // anywhere legal: TS2556 must still fire.
    let src = r#"
        interface NoRest {
            q(a: number): void;
            q(a: number, b: number): void;
        }
        declare const nr: NoRest;
        declare const ss: string[];
        nr.q(...ss);
    "#;
    let diags = check_source_diagnostics(src);
    assert_eq!(
        diagnostic_count(&diags, 2556),
        1,
        "overload set with no rest parameter must still emit TS2556: {diags:?}"
    );
}

#[test]
fn plain_function_without_rest_still_emits_ts2556() {
    // Single-signature negative control is unchanged by the existential-union
    // overload handling.
    let src = r#"
        function pair(a: number, b: number): void {}
        declare const ss: string[];
        pair(...ss);
    "#;
    let diags = check_source_diagnostics(src);
    assert_eq!(
        diagnostic_count(&diags, 2556),
        1,
        "plain no-rest function must still emit TS2556: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// `Object.assign`-shaped overload set: generic fixed-arity overloads followed
// by a trailing `(target, ...rest)` overload. The spread collapses to a single
// argument, so the fixed-arity overload would win on arg count and emit a
// spurious TS2556 unless non-rest overloads are skipped for an open-ended
// spread (tsc's `hasEffectiveRestParameter` precondition during overload
// resolution). The reachable rest overload then absorbs the spread.
// ---------------------------------------------------------------------------

#[test]
fn object_assign_shaped_overload_set_with_any_array_spread_is_clean() {
    // Mirrors `ObjectConstructor.assign`'s overloads: three generic fixed-arity
    // signatures plus a trailing rest signature. The `any[]` spread must bind to
    // the rest overload, not the two-param generic one. Binder names varied so
    // the rule is exercised structurally.
    let src = r#"
        interface Merger {
            blend<A extends {}, B>(target: A, source: B): A & B;
            blend<A extends {}, B, C>(target: A, s1: B, s2: C): A & B & C;
            blend<A extends {}, B, C, D>(target: A, s1: B, s2: C, s3: D): A & B & C & D;
            blend(target: object, ...sources: any[]): any;
        }
        declare const merge: Merger;
        declare const parts: any[];
        const out = merge.blend({}, ...parts);
    "#;
    let diags = check_source_diagnostics(src);
    assert_eq!(
        diagnostic_count(&diags, 2556),
        0,
        "any[] spread into the trailing rest overload of an Object.assign-shaped set must be clean: {diags:?}"
    );
}

#[test]
fn overload_set_typed_array_spread_into_trailing_rest_is_clean() {
    // A concretely typed `T[]` spread (not `any[]`) into the trailing rest
    // overload of a mixed fixed/rest overload set is likewise clean.
    let src = r#"
        function combine(first: string, second: number): void;
        function combine(first: string, ...rest: number[]): void;
        function combine(...args: any[]): void {}
        declare const nums: number[];
        combine("lead", ...nums);
    "#;
    let diags = check_source_diagnostics(src);
    assert_eq!(
        diagnostic_count(&diags, 2556),
        0,
        "typed T[] spread into a trailing rest overload must be clean: {diags:?}"
    );
}

#[test]
fn object_assign_shaped_set_with_unreachable_rest_preserves_diagnostic() {
    // Negative guard for the reachability gate: the only rest overload places
    // its rest parameter *after* more fixed parameters than the call provides
    // before the spread, so the spread lands on a fixed parameter of every
    // overload. The non-rest siblings must NOT be skipped (no reachable rest),
    // so a spread diagnostic is still produced rather than silently accepted.
    let src = r#"
        interface Picker {
            grab<A, B>(a: A, b: B): void;
            grab(a: object, b: object, c: object, ...rest: object[]): void;
        }
        declare const p: Picker;
        declare const xs: number[];
        p.grab("lead", ...xs);
    "#;
    let diags = check_source_diagnostics(src);
    // The spread reaches only position 1 (`b`), a non-rest slot in both
    // overloads, so the call must not resolve cleanly: a diagnostic remains.
    assert!(
        !diags.is_empty(),
        "spread that cannot reach any rest parameter must still produce a diagnostic: {diags:?}"
    );
}
