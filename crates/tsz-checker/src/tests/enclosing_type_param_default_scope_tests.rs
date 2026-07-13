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
