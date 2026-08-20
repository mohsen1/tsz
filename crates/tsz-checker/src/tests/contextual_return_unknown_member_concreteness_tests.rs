//! A generic call's contextual-return adoption gate
//! (`contextual_return_is_concrete` in `computation/call/inner.rs`) must treat
//! `unknown` as disqualifying only at *instantiation positions* of the
//! contextual type (the type itself, union/intersection members, application
//! type arguments, tuple/array elements) — never inside a declared *member*.
//!
//! `interface V { value: unknown }` is committed, user-written structure; a
//! deep `contains unknown` walk rejected `Readonly<V>` (and `V` itself) as
//! "not concrete", so a returned generic call kept its argument-widened
//! instantiation instead of adopting the contextual one, and fresh literal
//! properties widened (`kind: 'V'` → `kind: string`), producing false TS2322s.
//! The deep walk was also representation-dependent: a `Lazy` member boundary
//! hid the very `unknown` a materialized shape exposed, so the same written
//! contextual type flipped the gate depending on which interned form arrived.
//!
//! tsc reports no error on any of these shapes (verified against tsc 6.0.2 and
//! pinned 7.0.2, `--strict`). The witnesses reduce the kysely row's
//! `value-node.ts` / `primitive-value-list-node.ts` false TS2322 family
//! (#16074): a `freeze<T>(obj: T): Readonly<T>` factory whose members return
//! inner `freeze({ kind: '...', ... })` calls.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn check(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
}

fn assert_no_diagnostics(source: &str) {
    let diags = check(source);
    assert!(
        diags.is_empty(),
        "expected no diagnostics, got: {:?}",
        diags
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

/// The reduced kysely `value-node.ts` shape: an object literal with method
/// members passed to an explicitly-instantiated `freeze<Factory>` call, each
/// method returning an inner inferred `freeze({...})` call.
#[test]
fn factory_method_members_keep_literal_kind_through_inner_generic_call() {
    assert_no_diagnostics(
        r#"
export function freeze<T>(obj: T): Readonly<T> {
  return Object.freeze(obj);
}
interface OperationNode {
  readonly kind: 'ValueNode' | 'OtherNode';
}
export interface ValueNode extends OperationNode {
  readonly kind: 'ValueNode';
  readonly value: unknown;
  readonly immediate?: boolean;
}
type ValueNodeFactory = Readonly<{
  is(node: OperationNode): node is ValueNode;
  create(value: unknown): Readonly<ValueNode>;
  createImmediate(value: unknown): Readonly<ValueNode>;
}>;
export const ValueNode: ValueNodeFactory = freeze<ValueNodeFactory>({
  is(node): node is ValueNode {
    return node.kind === 'ValueNode';
  },
  create(value) {
    return freeze({
      kind: 'ValueNode',
      value,
    });
  },
  createImmediate(value) {
    return freeze({
      kind: 'ValueNode',
      value,
      immediate: true,
    });
  },
});
"#,
    );
}

/// Object-literal method container, contextual return `Readonly<V>` from a
/// direct variable annotation (no outer call). Renamed binders vs the kysely
/// witness.
#[test]
fn method_member_return_call_adopts_wrapped_contextual_with_unknown_member() {
    assert_no_diagnostics(
        r#"
declare function seal<W>(input: W): Readonly<W>;
interface Payload {
  readonly tag: 'payload';
  readonly data: unknown;
}
type Builder = { make(data: unknown): Readonly<Payload> };
const builder: Builder = {
  make(data) {
    return seal({ tag: 'payload', data });
  },
};
"#,
    );
}

/// Block-body arrow and function-expression property containers behave like
/// the method container.
#[test]
fn block_body_arrow_and_function_expression_returns_adopt_contextual() {
    assert_no_diagnostics(
        r#"
declare function seal<W>(input: W): Readonly<W>;
interface Payload {
  readonly tag: 'payload';
  readonly data: unknown;
}
type Builder = { make(data: unknown): Readonly<Payload> };
const viaArrow: Builder = {
  make: (data) => {
    return seal({ tag: 'payload', data });
  },
};
const viaFunction: Builder = {
  make: function (data) {
    return seal({ tag: 'payload', data });
  },
};
"#,
    );
}

/// The contextual return may also be the bare interface (no `Readonly`
/// wrapper on the target): `Readonly<{tag: 'payload'}>` still relates once
/// the contextual instantiation is adopted.
#[test]
fn bare_interface_contextual_return_adopts_through_readonly_result() {
    assert_no_diagnostics(
        r#"
declare function seal<W>(input: W): Readonly<W>;
interface Payload {
  readonly tag: 'payload';
  readonly data: unknown;
}
type Builder = { make(data: unknown): Payload };
const builder: Builder = {
  make(data) {
    return seal({ tag: 'payload', data });
  },
};
"#,
    );
}

/// A conditional expression of generic calls in return position is part of
/// the same family.
#[test]
fn conditional_return_of_generic_calls_adopts_contextual() {
    assert_no_diagnostics(
        r#"
declare function seal<W>(input: W): Readonly<W>;
interface Payload {
  readonly tag: 'payload';
  readonly data: unknown;
}
type Builder = { make(data: unknown): Readonly<Payload> };
const builder: Builder = {
  make(data) {
    return data ? seal({ tag: 'payload', data }) : seal({ tag: 'payload', data });
  },
};
"#,
    );
}

/// Negative control: `unknown` at an instantiation position of the contextual
/// type must still disqualify adoption. A `Box<unknown>` contextual return
/// must not clamp the argument-derived instantiation `Box<string>` — tsc
/// reports TS2322 on the outer assignment because `Box<string>` is not
/// assignable to `Box<'a'>` after `sink`'s literal widening (verified against
/// tsc 6.0.2/7.0.2: this stays an error).
#[test]
fn unknown_type_argument_contextual_return_still_rejected_as_adoption_source() {
    let diags = check(
        r#"
interface Box<T> {
  inner: T;
}
declare function sink<T>(x: T): Box<T>;
type F = { run(): Box<'a'> };
const f: F = {
  run() {
    return sink('a' as string);
  },
};
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "expected the widened Box<string> return to stay TS2322 against Box<'a'>, got: {:?}",
        diags
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}

/// Negative control from the gate's original protective case: an `unknown`
/// argument pinning the type parameter keeps its argument-owned result; the
/// contextual return type must not repair it (tsc reports TS2322 here).
#[test]
fn unknown_argument_result_is_not_repaired_by_contextual_return() {
    let diags = check(
        r#"
declare function generic<T>(x: T): T;
declare const w: unknown;
const s: string = generic(w);
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "expected TS2322 for unknown flowing into a string target, got: {:?}",
        diags
            .iter()
            .map(|d| format!("TS{}: {}", d.code, d.message_text))
            .collect::<Vec<_>>()
    );
}
