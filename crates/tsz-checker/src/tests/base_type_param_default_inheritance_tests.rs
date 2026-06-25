//! Regression tests for false TS2339/TS7053/TS2322 when a derived class extends
//! a generic base class WITHOUT type arguments and reads, through `this`, a
//! member typed by one of the base's type parameters.
//!
//! When `class Sub extends Base` and `Base<P = …>` has a default, `tsc` binds
//! the omitted argument to that default (`fillMissingTypeArguments`), so an
//! inherited member typed `P` is seen as the default type. tsz previously left
//! the bare base parameter unbound, so `this.member` resolved to the raw
//! parameter `P` (rendered `unknown` via its implicit constraint), producing a
//! false TS7053 on an index/`TS2339` on a property access / TS2322 on a return.
//! This is the raw-parameter sibling of the `error`/`never`-in-a-type-argument
//! leak family (issue <https://github.com/tsz-org/tsz/issues/13484>).
//!
//! The fix resolves such "dangling" base parameters (free in the member but not
//! bound by the enclosing generic scope) to their `default → constraint →
//! unknown` at the property-access boundary. Type parameters of the enclosing
//! generic class/function (`T` of a generic `Box<T>`) stay in scope and must be
//! preserved — the negative/identity guards below pin that.
//!
//! Test integrity: binder names are varied (`Holder`/`Wrapper`/`Vault`, the
//! type parameters `V`/`Elem`/`Payload`, the members `slot`/`cell`/`store`) so
//! the assertions track the structural shape, not any identifier; the cases use
//! only in-source declarations so they do not depend on a lib surface.

use std::sync::{Arc, OnceLock};
use tsz_binder::lib_loader::LibFile;

use crate::CheckerOptions;
use crate::test_utils::{check_source_with_libs_code_messages, load_default_lib_files};

fn default_libs() -> &'static [Arc<LibFile>] {
    static DEFAULT_LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
    DEFAULT_LIBS.get_or_init(load_default_lib_files)
}

fn check(src: &str) -> Vec<(u32, String)> {
    check_source_with_libs_code_messages(src, "test.ts", CheckerOptions::default(), default_libs())
}

fn assert_clean(src: &str) {
    let diags = check(src);
    assert!(diags.is_empty(), "expected no diagnostics, got: {diags:?}");
}

fn assert_has_code(src: &str, code: u32) {
    let diags = check(src);
    assert!(
        diags.iter().any(|(c, _)| *c == code),
        "expected diagnostic TS{code}, got: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Positive cases: the omitted base argument binds to the parameter default.
// ---------------------------------------------------------------------------

#[test]
fn this_member_typed_by_base_param_with_any_default_is_indexable() {
    // Default `any` -> `this.slot` is `any`, so indexing with a string is fine.
    assert_clean(
        "
class Holder<V = any> { slot!: V; }
class Sub extends Holder { read(k: string) { return this.slot[k]; } }
",
    );
}

#[test]
fn this_member_typed_by_base_param_with_concrete_default() {
    // Default `string` -> `this.cell` is `string`.
    assert_clean(
        "
class Wrapper<Elem = string> { cell!: Elem; }
class Child extends Wrapper { take(): string { return this.cell; } }
",
    );
}

#[test]
fn this_member_typed_by_base_param_with_object_default() {
    assert_clean(
        "
class Vault<Payload = { count: number }> { store!: Payload; }
class Branch extends Vault { total(): number { return this.store.count; } }
",
    );
}

#[test]
fn nested_base_param_position_binds_default() {
    // The base parameter appears inside `Elem[]`; the whole element type must
    // bind so `this.items[0]` is `any`, not a bare parameter.
    assert_clean(
        "
class Bag<Elem = any> { items!: Elem[]; }
class Pouch extends Bag { peek() { return this.items[0].anything; } }
",
    );
}

#[test]
fn chained_base_param_defaults_resolve_transitively() {
    // `Second = First[]` references an earlier base parameter; both must
    // resolve (`First` -> `string`, `Second` -> `string[]`).
    assert_clean(
        "
class Pair<First = string, Second = First[]> { head!: First; tail!: Second; }
class Combo extends Pair {
  first(): string { return this.head; }
  rest(): string[] { return this.tail; }
}
",
    );
}

#[test]
fn generic_derived_still_binds_omitted_base_default() {
    // The derived class is generic, but it still omits the base argument, so
    // the base parameter binds to its default while the derived `T` is unused
    // by the inherited member.
    assert_clean(
        "
class Source<Out = any> { value!: Out; }
class Relay<T> extends Source { use(k: string) { return this.value[k]; } }
function consume(r: Relay<number>) { return r.use('x'); }
",
    );
}

#[test]
fn abstract_base_param_member_binds_default() {
    // The zod-shaped abstract base / concrete derived form.
    assert_clean(
        "
abstract class Definition<Shape> { readonly meta!: Shape; }
interface NumberShape { entries: number[]; }
class NumberDef extends Definition<NumberShape> {
  push(n: number) { const all = [...this.meta.entries, n]; const out: number[] = all; return out; }
}
",
    );
}

// ---------------------------------------------------------------------------
// Negative controls: the resolved (defaulted) member type is still enforced.
// ---------------------------------------------------------------------------

#[test]
fn defaulted_member_type_is_still_checked_against_return() {
    // `this.cell` resolves to `string`, so returning it as `number` is TS2322.
    assert_has_code(
        "
class Wrapper<Elem = string> { cell!: Elem; }
class Child extends Wrapper { take(): number { return this.cell; } }
",
        2322,
    );
}

#[test]
fn defaulted_object_member_rejects_missing_property() {
    // `this.store` resolves to `{ count: number }`, so `.missing` is TS2339 on
    // the resolved object type (not on a bare parameter).
    assert_has_code(
        "
class Vault<Payload = { count: number }> { store!: Payload; }
class Branch extends Vault { peek() { return this.store.missing; } }
",
        2339,
    );
}

#[test]
fn explicit_base_argument_is_unaffected() {
    // Explicitly supplied base argument keeps its binding (`Elem = string`),
    // so returning it as `number` is still TS2322.
    assert_has_code(
        "
class Wrapper<Elem> { cell!: Elem; }
class Child extends Wrapper<string> { take(): number { return this.cell; } }
",
        2322,
    );
}

// ---------------------------------------------------------------------------
// Identity guards: an in-scope generic parameter must NOT be substituted away.
// ---------------------------------------------------------------------------

#[test]
fn generic_class_own_parameter_is_preserved_internally() {
    // `Box<T>` reading `this.payload` must keep `T` (it is bound by the class),
    // so the round-trip through a method stays well-typed.
    assert_clean(
        "
class Box<T> { payload!: T; get(): T { return this.payload; } set(x: T) { this.payload = x; } }
function roundtrip(b: Box<number>) { const n: number = b.get(); b.set(3); }
",
    );
}

#[test]
fn explicit_base_param_threading_is_preserved() {
    // `Sub<T> extends Carrier<T>` threads the derived parameter into the base,
    // so the inherited member is `T`, not a default.
    assert_clean(
        "
class Carrier<P> { freight!: P; }
class Sub<T> extends Carrier<T> { unwrap(): T { return this.freight; } }
function open(s: Sub<number>) { const n: number = s.unwrap(); }
",
    );
}

// ---------------------------------------------------------------------------
// Closure identity guards: a class's own *defaulted* type parameter must stay
// abstract when a `this.member` read happens inside a nested `this`-capturing
// closure. The class param is absent from the `TypeId`-keyed scope there, so it
// is pinned by its declared NAME instead of being filled with its default.
// ---------------------------------------------------------------------------

#[test]
fn defaulted_class_param_preserved_in_this_capturing_arrow() {
    // `T extends string = string`: `this.val` read inside the arrow and assigned
    // back to `T` must keep `T`, not be filled to its `string` default.
    assert_clean(
        "
class Box<T extends string = string> {
  val!: T;
  m() {
    const f = () => { const x: T = this.val; return x; };
    return f();
  }
}
",
    );
}

#[test]
fn defaulted_class_param_preserved_in_arrow_getter_read() {
    // Same as above but the member is a getter; `this.g` inside the arrow stays
    // the class parameter `K`.
    assert_clean(
        "
class Crate<K extends string = string> {
  private _g!: K;
  get g(): K { return this._g; }
  m() {
    const f = () => { const x: K = this.g; return x; };
    return f();
  }
}
",
    );
}

#[test]
fn defaulted_class_param_preserved_via_self_alias_in_arrow() {
    // `const self = this; self.cell` inside the arrow also resolves through the
    // class instance and must keep the class parameter `Acc`.
    assert_clean(
        "
class Store<Acc extends string = string> {
  cell!: Acc;
  m() {
    const f = () => { const self = this; const x: Acc = self.cell; return x; };
    return f();
  }
}
",
    );
}

#[test]
fn defaulted_numeric_class_param_preserved_in_arrow() {
    // A `number` constraint+default variant — the class parameter must still be
    // preserved by name, not filled to its `number` default.
    assert_clean(
        "
class Counter<N extends number = number> {
  tally!: N;
  m() {
    const f = () => { const x: N = this.tally; return x; };
    return f();
  }
}
",
    );
}

#[test]
fn class_param_without_default_unaffected_in_arrow() {
    // No default on the class parameter: already clean before and after the fix
    // (nothing to fill). The closure read still keeps `T`.
    assert_clean(
        "
class Holder<T extends string> {
  slot!: T;
  m() {
    const f = () => { const x: T = this.slot; return x; };
    return f();
  }
}
",
    );
}

#[test]
fn defaulted_class_param_preserved_without_closure() {
    // Plain method body (no closure): already clean, and remains clean — the
    // method body keeps `T` in the `TypeId`-keyed scope.
    assert_clean(
        "
class Vault<T extends string = string> {
  item!: T;
  m(): T { const x: T = this.item; return x; }
}
",
    );
}

#[test]
fn arrow_method_param_typed_by_class_param_unaffected() {
    // The RHS is a method parameter `arg: T`, not a `this`-member read, so the
    // omitted-base binding never runs; `T` is preserved as before.
    assert_clean(
        "
class Pack<T extends string = string> {
  m(arg: T) {
    const f = () => { const x: T = arg; return x; };
    return f();
  }
}
",
    );
}

#[test]
fn omitted_base_param_still_fills_default_inside_closure() {
    // Load-bearing #14523 negative control: `Sub extends Holder` omits the base
    // argument, so the genuinely-omitted base parameter `V` (NOT a parameter of
    // the receiver class `Sub`) must STILL bind to its `any` default even when
    // the read happens inside a nested `this`-capturing closure. The name-pin
    // only preserves the receiver class's OWN declared parameters.
    assert_clean(
        "
class Holder<V = any> { slot!: V; }
class Sub extends Holder {
  read(k: string) {
    const f = () => { return this.slot[k]; };
    return f();
  }
}
",
    );
}
