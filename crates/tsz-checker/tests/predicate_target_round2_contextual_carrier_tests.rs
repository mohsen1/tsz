//! Regression tests for issue #16022: a type parameter whose only Round-1
//! evidence is a sibling predicate argument's target type (`x is I`) must
//! survive into Round 2's contextual typing of *other* sensitive callbacks,
//! including when the parameter is otherwise only return-only in one of
//! them.
//!
//! Structural rule: `compute_round2_contextual_types`
//! (`crates/tsz-checker/src/types/computation/call_inference/argument_context.rs`)
//! re-instantiates a context-sensitive callback's contextual parameter type
//! from the callee's declared shape plus a `TypeSubstitution` built by
//! `merge_arg_return_context_into_round2` /
//! `collect_return_context_substitution`
//! (`crates/tsz-checker/src/types/computation/call_inference/return_context_substitution.rs`).
//! That walk matches two function types structurally — params
//! contravariantly, then `return_type` — but a type-predicate function's
//! asserted type (`x is I`) lives in `FunctionShape::type_predicate.type_id`,
//! not in `return_type` (always `boolean`). A type parameter pinned only
//! through a predicate target therefore had no carrier into the
//! pre-seeded Round 2 substitution.
//!
//! With a single sensitive callback (`type_predicate_sibling_argument_evidence_tests`,
//! #16024) this stayed hidden: the solver's full `resolve_call_with_checker_adapter`
//! resolve independently recovers the type parameter from an ordinary
//! covariant return position elsewhere in the signature. Once a *second*
//! sensitive callback (here `annotation`) makes that recovery path itself
//! depend on another not-yet-inferred type parameter, the recovery no longer
//! fires and only the (missing) return-context carrier is left — producing
//! the real superjson `classRule` false positives that #16024 alone did not
//! close. Fixed by also recursing the predicate's target type when both
//! sides of the structural function match carry a `type_predicate`.
//!
//! Every repro below is pinned against real `tsc` 7.0.2 (`--noEmit --strict`).

use tsz_checker::test_utils::{
    check_source_with_libs, diagnostic_codes, load_lib_files, strict_checker_options,
};

fn diagnostics(source: &str) -> Vec<u32> {
    let libs = load_lib_files(&["es5.d.ts", "es2015.core.d.ts"]);
    let mut codes = diagnostic_codes(&check_source_with_libs(
        source,
        "test.ts",
        strict_checker_options(),
        &libs,
    ));
    codes.sort_unstable();
    codes
}

#[test]
fn predicate_target_return_only_in_second_sensitive_callback_stays_clean() {
    // The real superjson `transformer.ts` `classRule` shape (#15731/#16022):
    // three type params, four arguments, two of them sensitive callbacks
    // (`annotation`, `transform`). `I`'s only evidence is the predicate
    // target on the non-sensitive `isApplicable` argument, and `I` is
    // return-only in the sensitive `untransform` callback's declared type.
    // tsc: clean.
    let codes = diagnostics(
        r#"
declare function getAllowedProps(x: any): string[] | undefined;

function isInstanceOfRegisteredClass(
  potentialClass: any
): potentialClass is any {
  return !!potentialClass?.constructor;
}

function compositeTransformation<I, O, A>(
  isApplicable: (v: any) => v is I,
  annotation: (v: I) => A,
  transform: (v: I) => O,
  untransform: (v: O, a: A) => I
) {
  return { isApplicable, annotation, transform, untransform };
}

const classRule = compositeTransformation(
  isInstanceOfRegisteredClass,
  (clazz) => {
    return 'x';
  },
  (clazz) => {
    const allowedProps = getAllowedProps(clazz.constructor);
    if (!allowedProps) {
      return { ...clazz };
    }
    const result: any = {};
    allowedProps.forEach(prop => {
      result[prop] = clazz[prop];
    });
    return result;
  },
  (v, a) => v
);
"#,
    );
    assert_eq!(codes, Vec::<u32>::new(), "expected clean, got {codes:?}");
}

#[test]
fn renamed_binder_predicate_target_return_only_carrier_is_structural() {
    // Same shape with every type parameter, parameter, and argument name
    // changed, proving the fix is structural (keyed on
    // `FunctionShape::type_predicate`), not on identifiers.
    let codes = diagnostics(
        r#"
function isTag(raw: any): raw is any {
  return !!raw;
}

function makeCodec<Value, Encoded, Meta>(
  guard: (raw: any) => raw is Value,
  describe: (value: Value) => Meta,
  encode: (value: Value) => Encoded,
  decode: (encoded: Encoded, meta: Meta) => Value
) {
  return { guard, describe, encode, decode };
}

const codec = makeCodec(
  isTag,
  (value) => value,
  (value) => {
    const probe = value.field;
    return probe;
  },
  (encoded, meta) => encoded
);
"#,
    );
    assert_eq!(codes, Vec::<u32>::new(), "expected clean, got {codes:?}");
}

#[test]
fn two_type_param_variant_without_second_sensitive_callback_still_clean() {
    // Positive/regression control: the simpler two-type-param shape (no `A`,
    // `untransform: (v: O) => I`) already stayed clean before this fix
    // because the solver's full resolve recovers `I` covariantly from
    // `untransform`'s own return type. Confirms the fix does not disturb
    // that independent recovery path.
    let codes = diagnostics(
        r#"
function isInstanceOfRegisteredClass(
  potentialClass: any
): potentialClass is any {
  return !!potentialClass?.constructor;
}

function compositeTransformation<I, O>(
  isApplicable: (v: any) => v is I,
  transform: (v: I) => O,
  untransform: (v: O) => I
) {
  return { isApplicable, transform, untransform };
}

const rule = compositeTransformation(
  isInstanceOfRegisteredClass,
  (clazz) => {
    return { ...clazz };
  },
  (v) => v
);
"#,
    );
    assert_eq!(codes, Vec::<u32>::new(), "expected clean, got {codes:?}");
}

#[test]
fn non_predicate_sibling_without_evidence_still_reports_unknown() {
    // Negative control (the two-type-param shape already covered by
    // `type_predicate_sibling_argument_evidence_tests`): without a
    // predicate (or any other evidence source) for `I`, `tsc` genuinely
    // leaves the callback's parameter `unknown`. The predicate-target
    // carrier must not manufacture evidence out of nothing.
    let codes = diagnostics(
        r#"
function compositeTransformation<I, O>(
  isApplicable: (v: any) => boolean,
  transform: (v: I) => O
) {
  return { isApplicable, transform };
}

const rule = compositeTransformation(
  (potentialClass) => !!potentialClass,
  (clazz) => {
    const ctor = clazz.constructor;
    return { ...clazz };
  }
);
"#,
    );
    assert!(
        codes.contains(&18046),
        "Expected TS18046 with no evidence for `I`. Got: {codes:?}"
    );
}
