//! Regression tests for the layer-3 mechanism of issue #14261.
//!
//! When a generic call infers a callee type parameter that a context-sensitive
//! callback argument reaches *only* through a callback **parameter**
//! (contravariant) position — never a callback **return** (covariant) position
//! — the value cannot be recovered from the callback body during Round 2.
//! `tsc` fixes such a parameter from the contextual **return** type. tsz instead
//! treated the parameter as owned by direct-argument inference (because it
//! occupies a direct-parameter slot), kept it at `unknown`, and emitted a
//! spurious `TS2345` while silently widening the callback parameters.
//!
//! Structural rule: when a callee type parameter `P` appears only in a callback
//! parameter position of an expected argument signature and the call has a
//! contextual return type that binds `P`, the contextual return binding must
//! win over the dropped direct-parameter inference. Callback **return** type
//! parameters keep their Round-2 body inference and are unaffected.
//!
//! Anti-hardcoding: tests vary binder names and callback shapes (indexed-access
//! member type, method member) and include a negative control (explicit type
//! arguments) plus a callback-return-position guard, rather than matching any
//! identifier or rendered message.

use tsz_checker::test_utils::{check_source_diagnostics, diagnostic_code_message_refs};

fn assert_no_code(source: &str, code: u32, context: &str) {
    let diagnostics = check_source_diagnostics(source);
    assert!(
        !diagnostics.iter().any(|diagnostic| diagnostic.code == code),
        "{context}: expected no TS{code}, got {:#?}",
        diagnostic_code_message_refs(&diagnostics),
    );
}

fn assert_has_code(source: &str, code: u32, context: &str) {
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.code == code),
        "{context}: expected TS{code}, got {:#?}",
        diagnostic_code_message_refs(&diagnostics),
    );
}

/// Concrete contextual return type pins a callback-parameter-only type
/// parameter expressed through an indexed-access member type. No false TS2345.
#[test]
fn concrete_contextual_pins_indexed_access_callback_param_no_ts2345() {
    assert_no_code(
        r#"
type Cmp = -1 | 0 | 1
interface Comparator<Elem> { readonly run: (left: Elem, right: Elem) => Cmp }
declare const make: <Elem>(run: Comparator<Elem>['run']) => Comparator<Elem>
export const build = (o: Comparator<string>): Comparator<string> =>
  make((p, q) => (p < q ? -1 : 1))
"#,
        2345,
        "callback-parameter-only type param pinned by a concrete contextual return",
    );
}

/// The same call must type the callback parameters from the contextual return
/// type (here `string`), so assigning a parameter to `number` is a real TS2322.
/// This proves the parameters are not silently widened to `any`/`unknown`.
#[test]
fn pinned_callback_parameters_are_typed_from_contextual_return() {
    assert_has_code(
        r#"
type Cmp = -1 | 0 | 1
interface Comparator<Elem> { readonly run: (left: Elem, right: Elem) => Cmp }
declare const make: <Elem>(run: Comparator<Elem>['run']) => Comparator<Elem>
export const build = (o: Comparator<string>): Comparator<string> =>
  make((p, q) => { const n: number = p; return p < q ? -1 : 1 })
"#,
        2322,
        "callback parameter `p` should be `string`, not `any`",
    );
}

/// A genuine callback **return** mismatch is still reported once the parameters
/// are correctly pinned: the body returns a value outside the expected union.
#[test]
fn pinned_callback_still_reports_real_return_mismatch() {
    assert_has_code(
        r#"
type Cmp = -1 | 0 | 1
interface Comparator<Elem> { readonly run: (left: Elem, right: Elem) => Cmp }
declare const make: <Elem>(run: Comparator<Elem>['run']) => Comparator<Elem>
export const build = (o: Comparator<string>): Comparator<string> =>
  make((p, q) => { return 99 })
"#,
        2345,
        "callback returning 99 violates the `Cmp` union",
    );
}

/// A method-member callback shape exercises the same path through a different
/// structural form and binder names.
#[test]
fn method_member_callback_param_only_typed_from_contextual_return() {
    assert_has_code(
        r#"
interface Box<Value> { read(): Value; write(next: Value): void }
declare function makeBox<Value>(write: Box<Value>['write']): Box<Value>;
export const b: Box<string> = makeBox((next) => { const n: number = next; });
"#,
        2322,
        "callback parameter `next` should be `string` from the contextual `Box<string>`",
    );
}

/// Negative control: with explicit type arguments the assignability rule is
/// unchanged — the parameter is concretely typed and an incompatible body use
/// is still a real error (no special-casing of the inference path).
#[test]
fn explicit_type_argument_keeps_parameter_concrete() {
    assert_has_code(
        r#"
type Cmp = -1 | 0 | 1
interface Comparator<Elem> { readonly run: (left: Elem, right: Elem) => Cmp }
declare const make: <Elem>(run: Comparator<Elem>['run']) => Comparator<Elem>
export const r = make<number>((p, q) => { const s: string = p; return p < q ? -1 : 1 })
"#,
        2322,
        "explicit `make<number>` types `p` as `number`; assigning it to `string` is TS2322",
    );
}

/// Guard: a callback **return**-position type parameter must keep its Round-2
/// body inference and must NOT be overridden by the contextual return type.
/// `R` is inferred from the body (`string`); the contextual `number` does not
/// win, so the result is a real TS2322 — exactly `tsc`'s behaviour.
#[test]
fn callback_return_type_param_keeps_body_inference() {
    assert_has_code(
        r#"
declare function run<R>(f: () => R): R;
export const x: number = run(() => "hi");
"#,
        2322,
        "callback-return type param `R` is inferred from the body, not the contextual return",
    );
}
