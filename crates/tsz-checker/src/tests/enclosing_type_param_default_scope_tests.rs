//! An enclosing generic function's DEFAULTED type parameter must stay in
//! scope inside nested-function bodies: `f<R = unknown>(box: Box<R>)` with
//! `() => box.get()` types the member read as `R`, not `unknown`.
//!
//! Structural rule: `push_enclosing_type_parameters` (the scope push used
//! when checking a nested function) must refine the enclosing parameter's
//! scope entry with the SAME `TypeParamInfo` — constraint AND default — the
//! canonical `push_type_parameters` mint carries. The decl-scoped intern
//! cache is keyed on the full info, so an entry refined without the default
//! interns to a DIFFERENT `TypeId` than the one member types reference;
//! `resolve_unbound_property_member_defaults` then treats the parameter as
//! dangling and collapses an application member read (`Box<R>.get`) to the
//! parameter's declared default (tsc keeps `R`; zustand Family B witness).
//!
//! The same re-entry path must preserve selective declaration identity when an
//! enclosing generic declaration shadows a same-named lexical owner. The
//! canonical declaration push stamps that rare binder as `DeclScoped`; a
//! nested closure must re-push the exact same identity in both its initial and
//! constraint/default refinement passes. Otherwise a captured value keeps the
//! method binder while a type annotation inside the closure resolves to a
//! newly minted same-named binder (`E` not assignable to `E`).

use crate::test_utils::check_source_diagnostics;

fn diagnostics(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|d| format!("TS{}: {}", d.code, d.message_text))
        .collect()
}

/// The Family B witness: an `unknown` default must not replace `R` in a
/// nested arrow's member read (tsc reports no error here).
#[test]
fn unknown_default_param_member_read_stays_generic_in_closure() {
    let diags = diagnostics(
        r#"
interface Box<R> { get(): R }
function f<R = unknown>(box: Box<R>) {
  const g: () => R = () => box.get()
  return g
}
"#,
    );
    assert!(
        diags.is_empty(),
        "box.get() must type as R inside the closure, got: {diags:?}"
    );
}

/// Default-value witness: a `string` default must not leak either — the
/// poisoning used the DECLARED default, not a fixed `unknown`. (Binder names
/// varied from the case above.)
#[test]
fn concrete_default_param_member_read_stays_generic_in_closure() {
    let diags = diagnostics(
        r#"
interface Carton<Q> { peek(): Q }
function open<Q = string>(carton: Carton<Q>) {
  const view: () => Q = function () { return carton.peek() }
  return view
}
"#,
    );
    assert!(
        diags.is_empty(),
        "carton.peek() must type as Q inside the function expression, got: {diags:?}"
    );
}

/// Un-contextual closure: the collapse happened regardless of a contextual
/// type, so pin the inferred-return form too.
#[test]
fn uncontextual_closure_member_read_stays_generic() {
    let diags = diagnostics(
        r#"
interface Cell<V> { read(): V }
function use<V = unknown>(cell: Cell<V>) {
  const thunk = () => cell.read()
  const value: V = thunk()
  return value
}
"#,
    );
    assert!(diags.is_empty(), "thunk() must type as V, got: {diags:?}");
}

/// Negative control: a parameter with NO default (and no constraint) was
/// never collapsed; it must keep working.
#[test]
fn bare_param_member_read_stays_generic_in_closure() {
    let diags = diagnostics(
        r#"
interface Slot<T> { take(): T }
function drain<T>(slot: Slot<T>) {
  const g: () => T = () => slot.take()
  return g
}
"#,
    );
    assert!(
        diags.is_empty(),
        "slot.take() must type as T inside the closure, got: {diags:?}"
    );
}

/// The dangling-parameter fill this scope entry gates must keep firing for
/// genuinely unbound base parameters: a class extending a generic base
/// WITHOUT type arguments still resolves the omitted argument to its default
/// (tsc's `fillMissingTypeArguments`), so the member read is concrete.
#[test]
fn omitted_base_type_args_still_fill_defaults() {
    let diags = diagnostics(
        r#"
class Base<P = string> {
  value!: P;
}
class Der extends Base {
  m() {
    const s: number = this.value;
  }
}
"#,
    );
    assert!(
        diags
            .iter()
            .any(|d| d.contains("2322") && d.contains("'string'")),
        "omitted base arg must fill to its default 'string', got: {diags:?}"
    );
}

/// A nested arrow re-enters both the class and the shadowing method scopes.
/// The method parameter annotation and captured value must keep one identity.
#[test]
fn shadowed_method_param_keeps_identity_in_nested_arrow() {
    let diags = diagnostics(
        r#"
class Container<Token> {
  static wrap<Token>(value: Token) {
    const callback = () => {
      const copy: Token = value
      return copy
    }
    return callback()
  }
}
"#,
    );
    assert!(
        diags.is_empty(),
        "the captured method binder must keep its identity in an arrow, got: {diags:?}"
    );
}

/// Function expressions use the same enclosing-scope reconstruction path.
/// Vary the binder name so the fix cannot depend on the Neverthrow spelling.
#[test]
fn shadowed_method_param_keeps_identity_in_function_expression() {
    let diags = diagnostics(
        r#"
class Envelope<Failure> {
  static wrap<Failure>(value: Failure) {
    const callback = function () {
      const copy: Failure = value
      return copy
    }
    return callback()
  }
}
"#,
    );
    assert!(
        diags.is_empty(),
        "the captured method binder must keep its identity in a function expression, got: {diags:?}"
    );
}

/// The refined pass must preserve the same identity when the shadowing binder
/// carries both a constraint and a default.
#[test]
fn constrained_defaulted_shadow_keeps_identity_in_nested_arrow() {
    let diags = diagnostics(
        r#"
class Wrapper<Item> {
  static map<Item extends { tag: string } = { tag: string }>(value: Item) {
    const callback = () => {
      const copy: Item = value
      return copy.tag
    }
    return callback()
  }
}
"#,
    );
    assert!(
        diags.is_empty(),
        "constraint/default refinement must reuse the shadowing binder identity, got: {diags:?}"
    );
}

/// Reduced Neverthrow shape: a static overload implementation shadows its
/// generic class binders, Promise callbacks reconstruct that method scope, and
/// the resulting application is passed to the enclosing generic constructor.
#[test]
fn shadowed_static_overload_promise_chain_keeps_constructor_argument_identity() {
    let diags = diagnostics(
        r#"
interface PromiseLike<Value> {
  then<Next>(onfulfilled: (value: Value) => Next): Promise<Next>
}
interface Promise<Value> extends PromiseLike<Value> {
  catch<Next>(onrejected: (reason: unknown) => Next): Promise<Value | Next>
}
class Outcome<Value, Failure> {
  value!: Value
  failure!: Failure
}
class AsyncOutcome<Value, Failure> {
  constructor(promise: Promise<Outcome<Value, Failure>>) {}

  static from<Value, Failure>(
    promise: PromiseLike<Value>,
    errorFn: (reason: unknown) => Failure,
  ): AsyncOutcome<Value, Failure>
  static from<Value, Failure>(
    promise: Promise<Value>,
    errorFn: (reason: unknown) => Failure,
  ): AsyncOutcome<Value, Failure> {
    const transformed = promise
      .then((value) => new Outcome<Value, Failure>())
      .catch((reason) => new Outcome<Value, Failure>())
    return new AsyncOutcome(transformed)
  }
}
"#,
    );
    assert!(
        diags.is_empty(),
        "static overload callbacks must keep the implementation binders, got: {diags:?}"
    );
}

/// A genuinely distinct nested binder with the same display name must remain
/// distinct; declaration identity is not a name-based compatibility escape.
#[test]
fn genuinely_nested_same_name_binder_still_reports_mismatch() {
    let diags = diagnostics(
        r#"
class Outer<T> {
  static wrap<T>(value: T) {
    return <T>(other: T) => {
      const wrong: T = value
      return [other, wrong]
    }
  }
}
"#,
    );
    assert!(
        diags.iter().any(|diag| diag.contains("2719")),
        "distinct nested binders must remain incompatible, got: {diags:?}"
    );
}
