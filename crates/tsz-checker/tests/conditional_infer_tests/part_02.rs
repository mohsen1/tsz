//! Contiguous test shard split out of the parent module to satisfy the
//! source-file line cap.
//!
//! Regression coverage for #14489: a conditional whose `extends`-type is a
//! generic *wrapper alias* carrying an `infer` (`type AB<T> = Promise<T[]>`,
//! pattern `AB<infer U>`) must bind `U` when the check-type is the alias's
//! *expanded* structural form (`Promise<number[]>`). The alias base (`AB`) does
//! not positionally correspond to the structural base (`Promise`) — the alias's
//! type parameter threads through a nested position of the body — so matching
//! through the positional source-peeling / base-subtype shortcut bound the
//! `infer` one structural level too shallow (`U = Promise<number[]>` instead of
//! `number`), or collapsed the conditional to its false branch (`never`),
//! producing a false TS2322 at the use site. The fix reduces a generic
//! wrapper-alias pattern to its structural application form (infer-preserving
//! head-only substitution) before positional matching.

use super::*;

/// Assert that the source produces at least one TS2322 (used by the
/// negative-control cases where the false branch must stay selected and its
/// literal type must be enforced).
fn assert_has_ts2322(source: &str, label: &str) {
    let diags = tsz_checker::test_utils::check_source_strict(source);
    let has = diags.iter().any(|d| d.code == 2322);
    assert!(
        has,
        "[{label}] expected a TS2322 (false branch must stay selected), got:\n{:#?}",
        diags
            .iter()
            .map(|d| (d.code, d.start, d.message_text.as_str()))
            .collect::<Vec<_>>()
    );
}

fn assert_no_ts2322_with_libs(source: &str, label: &str) {
    let diags = check_source_strict_with_default_libs(source);
    let errors: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        errors.is_empty(),
        "[{label}] expected no TS2322, got:\n{:#?}",
        diags
            .iter()
            .map(|d| (d.code, d.start, d.message_text.as_str()))
            .collect::<Vec<_>>()
    );
}

// =============================================================================
// #14489 — generic wrapper-alias `infer` over an expanded structural source.
// =============================================================================

#[test]
fn promise_wrapper_alias_infer_binds_through_expanded_source() {
    // The exact issue repro: `AB<T> = Promise<T[]>`, pattern `AB<infer U>`,
    // check-type the *expanded* `Promise<number[]>`. tsc infers `U = number`.
    assert_no_ts2322_with_libs(
        r#"
type AB<T> = Promise<T[]>;
type Ex<X> = X extends AB<infer U> ? U : never;
type R = Ex<Promise<number[]>>;
const a: R = 7;
export {};
"#,
        "AB<T> = Promise<T[]> ; Ex<Promise<number[]>> = number",
    );
}

#[test]
fn promise_wrapper_alias_infer_object_payload_shape() {
    // Wrapper body carries the param inside a nested object position.
    assert_no_ts2322_with_libs(
        r#"
type Envelope<T> = Promise<{ payload: T[] }>;
type Unwrap<X> = X extends Envelope<infer U> ? U : never;
type R = Unwrap<Promise<{ payload: number[] }>>;
const a: R = 7;
export {};
"#,
        "Envelope<T> = Promise<{payload:T[]}> ; Unwrap = number",
    );
}

#[test]
fn promise_wrapper_alias_infer_returntype_over_async_fn() {
    // `ReturnType<typeof asyncFn>` yields the expanded `Promise<number[]>`.
    assert_no_ts2322_with_libs(
        r#"
type AB<T> = Promise<T[]>;
async function asyncFn(): Promise<number[]> { return [1]; }
type Ex<X> = X extends AB<infer U> ? U : never;
type R = Ex<ReturnType<typeof asyncFn>>;
const a: R = 7;
export {};
"#,
        "Ex<ReturnType<typeof asyncFn>> = number",
    );
}

#[test]
fn user_interface_wrapper_alias_infer_binds_at_correct_depth() {
    // Lib-free, with binders renamed away from the issue text: a generic
    // *user interface* `Box<T>` as the wrapper base. Single nesting level.
    assert_no_ts2322(
        r#"
interface Box<T> { value: T; }
type Wrap<P> = Box<P[]>;
type Pull<X> = X extends Wrap<infer Out> ? Out : never;
type R = Pull<Box<number[]>>;
const ok: R = 7;
export {};
"#,
        "Wrap<P> = Box<P[]> ; Pull<Box<number[]>> = number",
    );
}

#[test]
fn nested_user_interface_wrapper_alias_infer_binds_at_correct_depth() {
    // Two-level nesting through a user interface: the previous "stops one level
    // early" divergence (`Z` bound to `Cell<string[]>` instead of `string`).
    // Binders renamed again so the fix cannot key off any identifier.
    assert_no_ts2322(
        r#"
interface Cell<V> { cell: V; }
type DeepWrap<Q> = Cell<Cell<Q[]>>;
type DeepPull<Y> = Y extends DeepWrap<infer Z> ? Z : never;
type R = DeepPull<Cell<Cell<string[]>>>;
const ok: R = "s";
export {};
"#,
        "DeepWrap<Q> = Cell<Cell<Q[]>> ; DeepPull = string",
    );
}

#[test]
fn wrapper_alias_infer_false_branch_preserved_when_source_shape_differs() {
    // Negative control: the source genuinely does not match the wrapper shape
    // (`Box<number>` is not `Box<number[]>`), so the conditional must take its
    // false branch and the literal type `"MISS"` must be enforced — assigning a
    // number is a real TS2322, not suppressed by the reduction path.
    assert_has_ts2322(
        r#"
interface Box<T> { value: T; }
type Wrap<P> = Box<P[]>;
type PullOr<X> = X extends Wrap<infer Out> ? Out : "MISS";
type R = PullOr<Box<number>>;
const ok: R = "MISS";
const bad: R = 7;
export {};
"#,
        "PullOr<Box<number>> = \"MISS\" (false branch); number is not assignable",
    );
}

// =============================================================================
// Type-predicate `infer`: `T extends (v: any) => v is infer R ? R : never`.
//
// A type guard's `infer` variable lives in the predicate target (`x is infer R`),
// not the boolean return type. Two solver gaps suppressed the binding: the
// contains-infer walk did not descend into `FunctionShape.type_predicate` (so a
// conditional never ran infer matching at all), and `match_infer_function_pattern`
// had no branch for predicate-only infer. tsc infers `R` from the source guard's
// asserted type (`inferFromSignature`); tsz collapsed it to the `never` false
// branch, producing spurious TS2322 (witnessed in the ts-pattern `P.string` /
// `when(isString)` chainable family). Binder names are varied so the fix cannot
// key off any identifier.
// =============================================================================

#[test]
fn type_predicate_infer_binds_narrowed_type() {
    // `R` must bind to the guard's asserted type (`string`), not `never`.
    assert_no_ts2322(
        r#"
type Narrowed<P> = P extends (value: any) => value is infer R ? R : never;
declare const isText: (probe: unknown) => probe is string;
type Out = Narrowed<typeof isText>;
const ok: Out = "hello";
export {};
"#,
        "Narrowed<(probe) => probe is string> = string",
    );
}

#[test]
fn type_predicate_infer_is_not_never_or_any() {
    // The dual control: a non-`string` assignment must still error, proving the
    // bound type is exactly `string` (not `never`, which would also reject the
    // positive case, nor `any`/`unknown`, which would accept this one).
    assert_has_ts2322(
        r#"
type Narrowed<P> = P extends (value: any) => value is infer R ? R : never;
declare const isText: (probe: unknown) => probe is string;
type Out = Narrowed<typeof isText>;
const bad: Out = 123;
export {};
"#,
        "Narrowed<...> = string; number is not assignable",
    );
}

#[test]
fn generic_type_guard_when_pattern_infers_narrowed() {
    // The ts-pattern `when(isString)` shape: a generic helper whose return type
    // is a conditional extracting the predicate's narrowed type from a generic
    // type guard `<U>(x: U | string) => x is string`.
    assert_no_ts2322(
        r#"
function pick<input, predicate extends (value: input) => unknown>(
  predicate: predicate
): predicate extends (value: any) => value is infer narrowed ? narrowed : never {
  return null as any;
}
function looksTextual<U>(candidate: U | string): candidate is string {
  return typeof candidate === "string";
}
const picked = pick(looksTextual);
const ok: string = picked;
export {};
"#,
        "pick(looksTextual) narrowed = string",
    );
}

#[test]
fn generic_type_guard_when_pattern_is_not_any() {
    // Control for the generic-guard case: the bound narrowed type is `string`,
    // so a `number` annotation on the result is a real TS2322.
    assert_has_ts2322(
        r#"
function pick<input, predicate extends (value: input) => unknown>(
  predicate: predicate
): predicate extends (value: any) => value is infer narrowed ? narrowed : never {
  return null as any;
}
function looksTextual<U>(candidate: U | string): candidate is string {
  return typeof candidate === "string";
}
const picked = pick(looksTextual);
const bad: number = picked;
export {};
"#,
        "pick(looksTextual) narrowed = string; number is not assignable",
    );
}

#[test]
fn asserts_predicate_infer_binds_narrowed_type() {
    // The `asserts value is infer R` variant binds `R` the same way.
    assert_has_ts2322(
        r#"
type AssertNarrowed<P> = P extends (value: any) => asserts value is infer R ? R : never;
declare const ensureText: (subject: unknown) => asserts subject is string;
type Out = AssertNarrowed<typeof ensureText>;
const ok: Out = "x";
const bad: Out = 7;
export {};
"#,
        "AssertNarrowed<asserts subject is string> = string; number is not assignable",
    );
}

#[test]
fn non_guard_source_takes_false_branch() {
    // Negative control: a source that returns `boolean` (no type predicate) is
    // not a guard, so the conditional must keep its false branch — the suppress
    // must not over-fire on every function type.
    assert_has_ts2322(
        r#"
type GuardOut<P> = P extends (value: any) => value is infer R ? R : "NOT_A_GUARD";
declare const plainCheck: (subject: unknown) => boolean;
type Out = GuardOut<typeof plainCheck>;
const ok: Out = "NOT_A_GUARD";
const bad: Out = "other";
export {};
"#,
        "GuardOut<(subject) => boolean> = \"NOT_A_GUARD\" (false branch)",
    );
}

#[test]
fn asserts_source_does_not_bind_non_asserts_infer_pattern() {
    // Predicate kinds must match: an `asserts x is string` source does NOT bind a
    // non-asserts `value is infer R` pattern, so the conditional takes its false
    // branch (mirrors tsc's `typePredicateKindsMatch`).
    assert_has_ts2322(
        r#"
type GuardOut<P> = P extends (value: any) => value is infer R ? R : "NO_MATCH";
declare const ensureText: (subject: unknown) => asserts subject is string;
type Out = GuardOut<typeof ensureText>;
const ok: Out = "NO_MATCH";
const bad: Out = "other";
export {};
"#,
        "asserts source vs non-asserts pattern = \"NO_MATCH\" (false branch)",
    );
}
