//! Signature-return aliases that legitimately evaluate to `unknown` must
//! relate through their evaluated form (issue #13212, valibot F1 family).
//!
//! Structural rule: when a function return type is an alias `Application` /
//! `Lazy` reference whose evaluation genuinely produces `unknown`, tsc
//! relates the evaluated form (`unknown` relates to `unknown`); tsz does the
//! same through `check_return_compat`, taking the raw-form fallback only
//! when the referenced body is missing or still an `unknown` placeholder.
//! Additionally, results computed under `bypass_evaluation` are never
//! persisted in the shared relation cache, so a raw-mode comparison cannot
//! poison later full-evaluation checks of the same `(source, target)` pair
//! (the `'x' is not assignable to 'x'` sibling-member false positives).

use tsz_common::options::checker::CheckerOptions;

fn diags_strict(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    crate::test_utils::check_source(source, "test.ts", opts)
}

fn assert_clean(source: &str) {
    let diags = diags_strict(source);
    assert!(
        diags.is_empty(),
        "Expected no diagnostics (tsc-clean); got: {diags:?}"
    );
}

/// The valibot gate repro: a conditional alias whose branches are both
/// `unknown`, referenced in signature-return position of a generic
/// interface member. Both the `run` member and the sibling literal member
/// `kind` must relate (the latter regressed via relation-cache poisoning).
#[test]
fn conditional_alias_unknown_return_in_generic_interface_member() {
    assert_clean(
        r#"
type C<T> = T extends 1 ? unknown : unknown;
interface A<T> {
  readonly kind: 'transformation';
  readonly run: () => C<T>;
  readonly x: T;
}
function make(x: number): A<number> {
  return { kind: 'transformation', x, run: () => 1 as unknown };
}
"#,
    );
}

/// Same shape with renamed binders and a different discriminant literal.
#[test]
fn conditional_alias_unknown_return_renamed_binders() {
    assert_clean(
        r#"
type Indirection<Z> = Z extends 9 ? unknown : unknown;
interface Holder<Q> {
  readonly tag: 'holder';
  readonly cb: () => Indirection<Q>;
  readonly val: Q;
}
const h: Holder<string> = { tag: 'holder', cb: () => 'x' as unknown, val: 'ok' };
"#,
    );
}

/// Bare function-type positions (no interface): `() => unknown` must be
/// assignable to `() => C<number>` when `C<number>` evaluates to `unknown`.
#[test]
fn conditional_alias_unknown_return_in_function_return_position() {
    assert_clean(
        r#"
type C<T> = T extends 1 ? unknown : unknown;
function make(): () => C<number> {
  return () => 1 as unknown;
}
"#,
    );
}

/// Unit-type and nullish sources against an unknown-evaluating alias target
/// (the issue's adjacent matrix: `undefined`/`null`/`void` sources failed,
/// `1` passed — all must pass).
#[test]
fn unit_and_nullish_sources_against_unknown_evaluating_alias_target() {
    assert_clean(
        r#"
type Cond<T> = T extends 1 ? unknown : unknown;
const f1: () => Cond<2> = () => undefined;
const f2: () => Cond<2> = () => null;
const f3: () => Cond<2> = () => {};
const f4: () => Cond<2> = () => 1;
"#,
    );
}

/// Pure indexed-access alias (not conditional-specific): an alias whose body
/// is an indexed access producing `unknown` in signature-return position.
#[test]
fn indexed_access_alias_unknown_return_in_interface_member() {
    assert_clean(
        r#"
interface BaseSchema { types: { output: unknown } }
type Get<B extends BaseSchema> = B['types']['output'];
interface Wrapper<S extends BaseSchema> {
  readonly tag: 'wrap';
  readonly run: () => Get<S>;
}
function mk(): Wrapper<BaseSchema> {
  return { tag: 'wrap', run: () => 0 as unknown };
}
"#,
    );
}

/// Negative case: an alias that evaluates to `string` must still reject a
/// number-returning source — skipping the raw fallback must not mask real
/// mismatches in the evaluated relation.
#[test]
fn alias_evaluating_to_string_still_rejects_number_source() {
    let diags = diags_strict(
        r#"
type Pick1<T> = T extends 1 ? string : string;
function bad(): () => Pick1<number> {
  return () => 42;
}
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "Expected TS2322 for number source vs string-evaluating alias; got: {diags:?}"
    );
}

/// Negative case: the unknown-evaluating alias as the SOURCE return type is
/// still not assignable to a concrete target return type (`unknown` is only
/// assignable to `unknown`/`any`).
#[test]
fn unknown_evaluating_alias_source_still_rejected_against_string_target() {
    let diags = diags_strict(
        r#"
type U<T> = T extends 1 ? unknown : unknown;
function bad(): () => string {
  return (() => 0 as unknown) as () => U<number>;
}
"#,
    );
    assert!(
        diags.iter().any(|d| {
            d.code == 2322
                && (d
                    .message_text
                    .contains("'unknown' is not assignable to type 'string'")
                    || d.related_information.iter().any(|r| {
                        r.message_text
                            .contains("'unknown' is not assignable to type 'string'")
                    }))
        }),
        "Expected TS2322 elaborating unknown vs string; got: {diags:?}"
    );
}

/// The free-type-parameter gap: a generic alias *application* `C<T>` (`T`
/// still free) used directly as the declared return type, returning `unknown`.
/// `C<T>` evaluates to a *deferred* conditional whose branches are both
/// `unknown`; the compat fast path rejected an `unknown` source against any
/// non-top target before the structural `subtype_of_conditional_target` rule
/// (source must relate to *both* branches) could accept it. tsc accepts this
/// because each branch is itself `unknown`. The concrete `C<number>` and
/// `() => C<number>` positions already evaluated to a literal `unknown`, so
/// only the still-deferred generic-application form regressed.
#[test]
fn generic_alias_application_unknown_return_with_free_type_parameter() {
    assert_clean(
        r#"
type C<T> = T extends 1 ? unknown : unknown;
function g<T>(v: unknown): C<T> {
  return v;
}
"#,
    );
}

/// `any` branches must behave identically to `unknown` branches: a deferred
/// conditional whose every branch is a top type accepts an `unknown` source.
#[test]
fn generic_alias_application_any_branches_accept_unknown_source() {
    assert_clean(
        r#"
type C<U> = U extends 1 ? any : any;
function g<U>(v: unknown): C<U> {
  return v;
}
"#,
    );
}

/// A non-distributive identical-top-branch conditional (`[T] extends [1]`)
/// also accepts an `unknown` source — the fix is not distribution-specific.
#[test]
fn non_distributive_top_branch_conditional_accepts_unknown_source() {
    assert_clean(
        r#"
type Wrap<Elem> = [Elem] extends [1] ? unknown : unknown;
function g<Elem>(v: unknown): Wrap<Elem> {
  return v;
}
"#,
    );
}

/// Negative control: a deferred conditional with a non-top branch must still
/// reject an `unknown` source (it is not assignable to the `string` branch).
#[test]
fn deferred_conditional_with_non_top_branch_rejects_unknown_source() {
    let diags = diags_strict(
        r#"
type Mixed<T> = T extends 1 ? unknown : string;
function bad<T>(v: unknown): Mixed<T> {
  return v;
}
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "Expected TS2322 for unknown source vs conditional with a string branch; got: {diags:?}"
    );
}

/// Property position (already-passing control from the issue's adjacent
/// matrix) stays clean alongside the signature-return fix.
#[test]
fn unknown_evaluating_alias_in_property_position_stays_clean() {
    assert_clean(
        r#"
type C<T> = T extends 1 ? unknown : unknown;
interface P<T> { p: C<T> }
const v: P<number> = { p: 1 as unknown };
"#,
    );
}
