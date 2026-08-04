//! Regression tests for the "callable `then` with no fulfillment payload"
//! half of the invalid-thenable family (TS1320 / TS1321 / TS1058).
//!
//! Structural rule: when a type is *thenable* — its `then` property is
//! callable once `null`/`undefined` are stripped — but
//! `getPromisedTypeOfPromiseEx` still recovers no promised type, `tsc` reports
//! "must either be a valid promise or must not contain a callable `then`
//! member" at the operand position. tsz does this through
//! `CheckerState::await_operand_is_invalid_thenable` in
//! `checkers/promise_checker.rs`.
//!
//! Before this file, tsz implemented only the `this`-type-mismatch sub-case
//! (`crates/tsz-checker/tests/await_thenable_this_context_tests.rs`,
//! `generator_yield_invalid_thenable_tests.rs`), so every witness below was a
//! silent false negative. The three failure shapes `tsc` actually reaches, all
//! pinned against `typescript@7.0.2`:
//!
//! 1. an optional `then?:` — thenable (`isThenableType` strips `undefined`)
//!    but with no call signature on the *raw* property, which is what
//!    `getPromisedTypeOfPromiseEx` reads;
//! 2. a `then` whose `onfulfilled` parameter is not callable — including
//!    `any`, `unknown`, `never`, and a `then` that declares no parameter at
//!    all (`tsc` falls back to `never` for the parameter type, and `never` has
//!    no call signatures);
//! 3. every `then` signature rejected by its own `this` annotation.
//!
//! The negatives that stop the rule from over-firing are as load-bearing as
//! the positives: a callable `onfulfilled` that declares *no* parameters is a
//! valid promise (`tsc` resolves the payload to `never` rather than rejecting
//! the shape), a non-callable `then` is not a thenable at all, and a primitive
//! carrying a `then` member through an intersection is never a thenable
//! however the property lookup resolves.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn strict_codes(source: &str) -> Vec<u32> {
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
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

fn await_codes(declaration: &str) -> Vec<u32> {
    strict_codes(&format!(
        "export {{}};\n{declaration}\nasync function zzConsume() {{ await zzOperand; }}\n"
    ))
}

// ---------------------------------------------------------------------------
// Positives: thenable, but no promised type is recoverable.
// ---------------------------------------------------------------------------

#[test]
fn await_optional_then_property_reports_ts1320() {
    let codes =
        await_codes("declare const zzOperand: { then?: (cb: (v: number) => void) => void };");
    assert!(
        codes.contains(&1320),
        "an optional `then` is thenable but has no raw call signature: {codes:?}"
    );
}

#[test]
fn await_then_with_non_callable_parameter_reports_ts1320() {
    let codes = await_codes("declare const zzOperand: { then(cb: string): void };");
    assert!(
        codes.contains(&1320),
        "a non-callable `onfulfilled` parameter yields no promised type: {codes:?}"
    );
}

#[test]
fn await_then_with_no_parameters_reports_ts1320() {
    let codes = await_codes("declare const zzOperand: { then(): void };");
    assert!(
        codes.contains(&1320),
        "a parameterless `then` resolves `onfulfilled` to `never`: {codes:?}"
    );
}

#[test]
fn await_then_with_any_parameter_reports_ts1320() {
    let codes = await_codes("declare const zzOperand: { then(cb: any): void };");
    assert!(
        codes.contains(&1320),
        "an `any` `onfulfilled` parameter yields no promised type: {codes:?}"
    );
}

#[test]
fn await_then_with_unknown_parameter_reports_ts1320() {
    let codes = await_codes("declare const zzOperand: { then(cb: unknown): void };");
    assert!(codes.contains(&1320), "{codes:?}");
}

#[test]
fn await_then_with_never_parameter_reports_ts1320() {
    let codes = await_codes("declare const zzOperand: { then(cb: never): void };");
    assert!(codes.contains(&1320), "{codes:?}");
}

/// Union distribution: a union whose `then` members differ has no call
/// signature of its own, so the constituents must be examined individually.
#[test]
fn await_union_with_one_invalid_thenable_branch_reports_ts1320() {
    let codes = await_codes(
        "declare const zzOperand: { then(cb: (v: number) => void): void } | { then(cb: string): void };",
    );
    assert!(
        codes.contains(&1320),
        "a single invalid branch makes the whole union invalid: {codes:?}"
    );
}

/// Renamed-binder control: the same shape behind named interfaces and a
/// differently-named consumer must still report, proving the rule is
/// structural and not keyed off any identifier.
#[test]
fn await_invalid_thenable_through_renamed_interfaces_reports_ts1320() {
    let codes = strict_codes(
        r#"
export {};
interface WidgetSource { then(callbackParam: string): void }
type WidgetAlias = WidgetSource;
declare const widgetValue: WidgetAlias;
async function consumeWidget() { await widgetValue; }
"#,
    );
    assert!(codes.contains(&1320), "{codes:?}");
}

/// Generic form: the same shape as a type argument reaches the rule through
/// the instantiated member.
#[test]
fn await_invalid_thenable_through_generic_instantiation_reports_ts1320() {
    let codes = strict_codes(
        r#"
export {};
interface BoxedThenable<TPayload> { then(onDone: TPayload): void }
declare const boxedValue: BoxedThenable<string>;
async function consumeBoxed() { await boxedValue; }
"#,
    );
    assert!(codes.contains(&1320), "{codes:?}");
}

// ---------------------------------------------------------------------------
// The same rule at the adjacent operand positions.
// ---------------------------------------------------------------------------

#[test]
fn plain_yield_of_invalid_thenable_in_async_generator_reports_ts1321() {
    let codes = strict_codes(
        r#"
export {};
interface NoPayloadThenable { then(onDone: string): void }
declare const yieldSource: NoPayloadThenable;
async function* produceValues(): AsyncGenerator<any> { yield yieldSource; }
"#,
    );
    assert!(
        codes.contains(&1321),
        "an async generator's plain `yield` validates its operand like `await`: {codes:?}"
    );
}

#[test]
fn annotated_async_return_of_invalid_thenable_reports_ts1058() {
    let codes = strict_codes(
        r#"
export {};
interface NoPayloadThenable { then(onDone: string): void }
declare const returnSource: NoPayloadThenable;
async function produceReturn(): NoPayloadThenable { return returnSource; }
"#,
    );
    assert!(
        codes.contains(&1058),
        "an annotated async return validates its expression like `await`: {codes:?}"
    );
}

#[test]
fn await_invalid_thenable_in_class_method_reports_ts1320() {
    let codes = strict_codes(
        r#"
export {};
interface NoPayloadThenable { then(onDone: string): void }
declare const methodSource: NoPayloadThenable;
class ConsumerHolder { async consume() { await methodSource; } }
"#,
    );
    assert!(codes.contains(&1320), "{codes:?}");
}

#[test]
fn await_invalid_thenable_in_async_arrow_reports_ts1320() {
    let codes = strict_codes(
        r#"
export {};
interface NoPayloadThenable { then(onDone: string): void }
declare const arrowSource: NoPayloadThenable;
const consumeArrow = async () => { await arrowSource; };
"#,
    );
    assert!(codes.contains(&1320), "{codes:?}");
}

// ---------------------------------------------------------------------------
// Negatives: shapes `tsc` accepts, which the widened rule must not claim.
// ---------------------------------------------------------------------------

#[test]
fn await_valid_thenable_reports_nothing() {
    let codes = await_codes("declare const zzOperand: { then(cb: (v: number) => void): void };");
    assert!(!codes.contains(&1320), "{codes:?}");
}

/// A callable `onfulfilled` with no parameters is still a valid promise —
/// `tsc` falls back to `never` for the payload instead of rejecting.
#[test]
fn await_then_with_parameterless_callback_reports_nothing() {
    let codes = await_codes("declare const zzOperand: { then(cb: () => void): void };");
    assert!(
        !codes.contains(&1320),
        "a zero-parameter `onfulfilled` resolves the payload to `never`: {codes:?}"
    );
}

/// The `PromiseLike` shape: `onfulfilled` is optional and nullable, so the
/// callable surface only appears after `null`/`undefined` are stripped.
#[test]
fn await_nullable_optional_callback_reports_nothing() {
    let codes =
        await_codes("declare const zzOperand: { then(cb?: ((v: number) => void) | null): void };");
    assert!(!codes.contains(&1320), "{codes:?}");
}

#[test]
fn await_non_callable_then_member_reports_nothing() {
    let codes = await_codes("declare const zzOperand: { then: number };");
    assert!(
        !codes.contains(&1320),
        "a non-callable `then` makes the type a plain object, not a thenable: {codes:?}"
    );
}

#[test]
fn await_any_then_member_reports_nothing() {
    let codes = await_codes("declare const zzOperand: { then: any };");
    assert!(!codes.contains(&1320), "{codes:?}");
}

/// A primitive never adopts a `then` member, however the intersection's
/// property lookup resolves it.
#[test]
fn await_primitive_intersection_carrying_then_reports_nothing() {
    let codes = await_codes("declare const zzOperand: string & { then(cb: string): void };");
    assert!(
        !codes.contains(&1320),
        "a primitive is not a thenable: {codes:?}"
    );
}

/// `getBaseConstraintOrType` first: a type parameter constrained to a
/// primitive is in the primitive domain regardless of any `then` its
/// constraint also carries. Corpus witness `awaitedType.ts` (`f16`), which the
/// fixture itself annotates "T belongs to the domain of primitive types
/// (regardless of `then`)" — the widened rule reported a spurious TS1320 here
/// until the primitive guard resolved the constraint.
#[test]
fn await_type_parameter_constrained_to_primitive_with_then_reports_nothing() {
    let codes = strict_codes(
        r#"
export {};
async function consumeConstrained<TValue extends number & { then(): void }>(operand: TValue) {
  await operand;
}
"#,
    );
    assert!(
        !codes.contains(&1320),
        "a type parameter whose base constraint is primitive is not a thenable: {codes:?}"
    );
}

/// The negative half of the same guard: a type parameter whose base constraint
/// is a plain object with an invalid `then` *is* a thenable, so the rule must
/// still fire through the constraint.
#[test]
fn await_type_parameter_constrained_to_invalid_thenable_reports_ts1320() {
    let codes = strict_codes(
        r#"
export {};
async function consumeObjectConstrained<TValue extends { then(onDone: string): void }>(
  operand: TValue,
) {
  await operand;
}
"#,
    );
    assert!(
        codes.contains(&1320),
        "the primitive guard must not swallow an object-constrained type parameter: {codes:?}"
    );
}

/// A type parameter constrained to a non-callable `then` is not thenable at
/// all — corpus witness `awaitedType.ts` (`f15`).
#[test]
fn await_type_parameter_constrained_to_non_callable_then_reports_nothing() {
    let codes = strict_codes(
        r#"
export {};
async function consumeNonCallable<TValue extends { then: number }>(operand: TValue) {
  await operand;
}
"#,
    );
    assert!(!codes.contains(&1320), "{codes:?}");
}

/// An intersection whose object half carries a valid `then` stays valid.
#[test]
fn await_object_intersection_with_valid_then_reports_nothing() {
    let codes = await_codes(
        "declare const zzOperand: { then(cb: (v: number) => void): void } & { extra: number };",
    );
    assert!(!codes.contains(&1320), "{codes:?}");
}

#[test]
fn await_real_promise_reports_nothing() {
    let codes = strict_codes(
        r#"
export {};
declare const promiseValue: Promise<number>;
async function consumePromise() { await promiseValue; }
"#,
    );
    assert!(!codes.contains(&1320), "{codes:?}");
}

#[test]
fn await_promise_like_reports_nothing() {
    let codes = strict_codes(
        r#"
export {};
declare const promiseLikeValue: PromiseLike<string>;
async function consumePromiseLike() { await promiseLikeValue; }
"#,
    );
    assert!(!codes.contains(&1320), "{codes:?}");
}

#[test]
fn await_non_thenable_object_reports_nothing() {
    let codes = await_codes("declare const zzOperand: { value: number };");
    assert!(!codes.contains(&1320), "{codes:?}");
}

#[test]
fn valid_thenable_yield_in_async_generator_reports_nothing() {
    let codes = strict_codes(
        r#"
export {};
interface ValidThenable { then(onDone: (v: number) => void): void }
declare const validYieldSource: ValidThenable;
async function* produceValid(): AsyncGenerator<any> { yield validYieldSource; }
"#,
    );
    assert!(!codes.contains(&1321), "{codes:?}");
}

#[test]
fn annotated_async_return_of_valid_thenable_reports_no_ts1058() {
    let codes = strict_codes(
        r#"
export {};
interface ValidThenable { then(onDone: (v: number) => void): void }
declare const validReturnSource: ValidThenable;
async function produceValidReturn(): ValidThenable { return validReturnSource; }
"#,
    );
    assert!(!codes.contains(&1058), "{codes:?}");
}
