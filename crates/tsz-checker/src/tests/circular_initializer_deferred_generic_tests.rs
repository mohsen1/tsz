//! Regression coverage for circular-initializer (TS7022) and circular
//! return-type (TS7023/TS7024) detection when a self-referencing object flows
//! through a generic call.
//!
//! Structural rule: a self-reference confined to a deferred function/method/
//! getter body does NOT make a variable's initializer circular, because the
//! deferred function's return type is inferred lazily. `tsc` only reports a
//! circularity when
//!   * the deferred function is contextually typed by a signature whose return
//!     position is an inference target (a type parameter the variable's type
//!     depends on), or
//!   * the body recursively invokes the resolving variable (`() => x.loop()`).
//!
//! Previously tsz over-fired TS7022/TS7023 whenever a self-referencing object
//! literal was passed to a generic function (e.g. `define<T>(spec: T): T`),
//! because the deferred return site, while correctly recorded (so the variable
//! still widens to `any`), was also reported as a diagnostic. The fix marks
//! such sites as benign "lazy" references and suppresses only the diagnostic
//! emission — leaving recording and the widening behaviour untouched, so no
//! other circular-initializer case changes. See issue #10675 (kysely false
//! TS7022/TS1062).

use crate::test_utils::check_source_diagnostics;

fn circularity_codes(src: &str) -> Vec<u32> {
    let mut codes: Vec<u32> = check_source_diagnostics(src)
        .iter()
        .filter(|d| matches!(d.code, 7022..=7024))
        .map(|d| d.code)
        .collect();
    codes.sort_unstable();
    codes
}

fn assert_no_circularity(src: &str) {
    let codes = circularity_codes(src);
    assert!(
        codes.is_empty(),
        "expected no circular-initializer diagnostics, got {codes:?} for:\n{src}"
    );
}

fn assert_has_circularity(src: &str) {
    let codes = circularity_codes(src);
    assert!(
        !codes.is_empty(),
        "expected a circular-initializer diagnostic (TS7022/7023/7024), got none for:\n{src}"
    );
}

// --- Accepted (tsc resolves these lazily) -------------------------------------

#[test]
fn generic_identity_call_with_deferred_arrow_self_reference_is_not_circular() {
    // Object literal matched against a bare type parameter: the arrow's return
    // type is deferred, so the self-reference is benign.
    assert_no_circularity(
        r#"
declare function makeStore<S>(spec: S): S;
const store = makeStore({ count: 1, reload: () => store });
store.reload().count;
"#,
    );
}

#[test]
fn generic_call_returning_wrapper_with_deferred_self_reference_is_not_circular() {
    assert_no_circularity(
        r#"
declare function node<N>(spec: N): { root: N };
const graph = node({ id: 1, parent: () => graph.root });
graph.root.id;
"#,
    );
    assert_no_circularity(
        r#"
declare function many<E>(value: E): E[];
const items = many({ id: 1, again: () => items });
items[0].id;
"#,
    );
}

#[test]
fn deferred_property_access_self_reference_is_not_circular() {
    assert_no_circularity(
        r#"
declare function configure<C>(spec: C): C;
const settings = configure({ depth: 3, peek: () => settings.depth });
settings.peek;
"#,
    );
}

#[test]
fn resolving_variable_passed_as_call_argument_is_not_circular() {
    // The variable appears only as an *argument*; the callee's return type does
    // not depend on it, so this is not a circular return-type dependency.
    assert_no_circularity(
        r#"
declare function register<R>(spec: R): R;
declare function describe(value: unknown): number;
const widget = register({ render: () => describe(widget) });
widget.render();
"#,
    );
}

#[test]
fn function_expression_property_returning_self_is_not_circular() {
    assert_no_circularity(
        r#"
declare function build<B>(spec: B): B;
const service = build({ resolve: function () { return service; } });
service.resolve();
"#,
    );
}

#[test]
fn method_and_getter_returning_self_through_generic_call_are_not_circular() {
    assert_no_circularity(
        r#"
declare function build<B>(spec: B): B;
const repo = build({ size: 0, latest() { return repo; } });
repo.latest().size;
"#,
    );
    assert_no_circularity(
        r#"
declare function build<B>(spec: B): B;
const cache = build({ size: 0, get owner() { return cache; } });
cache.owner.size;
"#,
    );
}

#[test]
fn direct_object_literal_arrow_self_reference_is_not_circular() {
    // Baseline behaviour without the generic wrapper — already accepted by tsc.
    assert_no_circularity(
        r#"
const registry = { total: 1, self: () => registry };
registry.self().total;
"#,
    );
}

// --- Rejected (genuine circularities, preserved) ------------------------------

#[test]
fn callback_whose_return_infers_a_type_parameter_is_circular() {
    // `memo`'s callback return position IS the inference site for `T`, and the
    // variable's type depends on `T` — genuinely circular (tsc errors here).
    assert_has_circularity(
        r#"
declare function memo<T>(fn: () => T): () => T;
const lazy = { value: 1, getSelf: memo(() => lazy) };
lazy.getSelf().value;
"#,
    );
}

#[test]
fn structural_callback_return_type_parameter_is_circular() {
    // `spec.make: () => T` contextually types the arrow with the inference
    // target `T`, forcing evaluation of the self-referencing return.
    assert_has_circularity(
        r#"
declare function build<T>(spec: { make: () => T }): T;
const seed = build({ make: () => ({ child: seed }) });
seed;
"#,
    );
}

#[test]
fn recursive_self_invocation_in_deferred_body_is_circular() {
    // The deferred body invokes the resolving variable as the call *callee*
    // (`runtime.tick()`), so its return type depends on itself.
    assert_has_circularity(
        r#"
declare function define<D>(spec: D): D;
const runtime = define({ tick: () => runtime.tick() });
runtime;
"#,
    );
}

#[test]
fn recursive_self_invocation_through_comma_callee_is_circular() {
    // A comma expression yields its right operand, so `(0, runtime.tick)()`
    // still recursively invokes the resolving variable. tsc reports TS7023.
    assert_has_circularity(
        r#"
declare function define<D>(spec: D): D;
const runtime = define({ tick: () => (0, runtime.tick)() });
runtime;
"#,
    );
}

#[test]
fn recursive_self_invocation_through_conditional_callee_is_circular() {
    // Either branch of a conditional callee keeps the variable on the callee
    // path (`(flag ? runtime.tick : runtime.tick)()`). tsc reports TS7023.
    assert_has_circularity(
        r#"
declare const flag: boolean;
declare function define<D>(spec: D): D;
const runtime = define({ tick: () => (flag ? runtime.tick : runtime.tick)() });
runtime;
"#,
    );
}

#[test]
fn direct_non_deferred_self_reference_is_circular() {
    assert_has_circularity(
        r#"
declare function define<D>(spec: D): D;
const handle = define({ value: 1, self: handle });
handle.value;
"#,
    );
}
