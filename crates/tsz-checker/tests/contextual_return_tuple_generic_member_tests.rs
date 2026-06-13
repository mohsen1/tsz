//! Regression tests for issue #13477.
//!
//! When a method's *inferred* (block-body) contextual return type is a generic
//! tuple whose type argument owns a generic *member* (e.g. a lib type like
//! `MessagePort` with `addEventListener<K extends keyof M>(...)`), the array
//! literal returned from the body must type as the contextual tuple, not widen
//! to a plain array. The bound member type parameter `K` must not be mistaken
//! for an unresolved/free inference hole that suppresses contextual return
//! typing.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_with_options;

fn check_strict(source: &str) -> Vec<Diagnostic> {
    check_with_options(
        source,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            strict_function_types: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    )
}

fn ts2322_count(source: &str) -> usize {
    check_strict(source)
        .iter()
        .filter(|d| d.code == 2322)
        .count()
}

/// The reported bug: a block-body inferred return whose contextual tuple
/// argument owns a generic member must not widen the returned array literal.
#[test]
fn inferred_block_return_tuple_with_generic_member_arg_does_not_widen() {
    let source = r#"
type Pair<S> = [S, number];
interface EvMap { "x": number; }
interface Port {
    id: number;
    on<K extends keyof EvMap>(t: K, l: (ev: EvMap[K]) => void): void;
}
interface Handler<T, S> { serialize(value: T): Pair<S>; }
declare const p: Port;
const h: Handler<object, Port> = {
    serialize(obj) {
        return [p, 1];
    },
};
"#;
    let count = ts2322_count(source);
    assert!(
        count == 0,
        "expected no TS2322 for the contextually-typed tuple return, got: {count} TS2322"
    );
}

/// Same shape, but with renamed binders: the fix is structural, not keyed on
/// any identifier.
#[test]
fn inferred_block_return_tuple_with_generic_member_arg_is_binder_name_independent() {
    let source = r#"
type Tup<W> = [W, number];
interface Listenable { "evt": string; }
interface Widget {
    handle: number;
    subscribe<E extends keyof Listenable>(name: E, cb: (v: Listenable[E]) => void): void;
}
interface Shape<A, B> { build(input: A): Tup<B>; }
declare const w: Widget;
const s: Shape<object, Widget> = {
    build(input) {
        return [w, 7];
    },
};
"#;
    let count = ts2322_count(source);
    assert!(
        count == 0,
        "renamed-binder variant should also be clean, got: {count} TS2322"
    );
}

/// Control: a plain (non-generic-member) argument was already handled; keep it
/// passing to isolate the trigger.
#[test]
fn inferred_block_return_tuple_with_plain_arg_stays_clean() {
    let source = r#"
type Pair<S> = [S, number];
interface Plain { id: number; }
interface Handler<T, S> { serialize(value: T): Pair<S>; }
declare const p: Plain;
const h: Handler<object, Plain> = {
    serialize(obj) {
        return [p, 1];
    },
};
"#;
    let count = ts2322_count(source);
    assert!(
        count == 0,
        "plain-arg control should be clean, got: {count} TS2322"
    );
}

/// Negative: a genuinely incompatible return element must still report TS2322 —
/// contextual typing is enabled, not silenced.
#[test]
fn inferred_block_return_tuple_real_mismatch_still_errors() {
    let source = r#"
type Pair<S> = [S, number];
interface EvMap { "x": number; }
interface Port {
    id: number;
    on<K extends keyof EvMap>(t: K, l: (ev: EvMap[K]) => void): void;
}
interface Handler<T, S> { serialize(value: T): Pair<S>; }
const h: Handler<object, Port> = {
    serialize(obj) {
        return [123, 1];
    },
};
"#;
    let count = ts2322_count(source);
    assert!(
        count > 0,
        "a real element mismatch must still report TS2322"
    );
}

/// Negative/free: a bare free type parameter return must still suppress
/// contextual typing (and not panic) — the free-variable predicate keeps that
/// behavior.
#[test]
fn expression_body_arrow_variant_with_generic_member_arg_is_clean() {
    let source = r#"
type Pair<S> = [S, number];
interface EvMap { "x": number; }
interface Port {
    id: number;
    on<K extends keyof EvMap>(t: K, l: (ev: EvMap[K]) => void): void;
}
interface Handler<T, S> { serialize: (value: T) => Pair<S>; }
declare const p: Port;
const h: Handler<object, Port> = { serialize: (obj) => [p, 1] };
"#;
    let count = ts2322_count(source);
    assert!(
        count == 0,
        "expression-body arrow variant should be clean, got: {count} TS2322"
    );
}
