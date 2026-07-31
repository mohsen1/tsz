//! Regression tests: a sibling argument's type-predicate target (`x is I`)
//! must seed the shared type parameter `I` even when `I` is *also*
//! return-only in another, unrelated sensitive callback's parameter type
//! (`untransform: (v: O, a: A) => I` in the superjson `classRule` shape,
//! issue #16022).
//!
//! Structural rule: `tsc` fixes a type parameter from non-deferred evidence
//! (here, a concrete sibling argument's predicate target) before contextually
//! typing any sensitive callback body, regardless of how many other sensitive
//! callbacks also mention that parameter.
//!
//! Root cause: `collect_return_context_substitution`'s function-to-function
//! matching arm
//! (`crates/tsz-checker/src/types/computation/call_inference/return_context_substitution.rs`)
//! walked a callable pair's `params` and `return_type` to recover a tracked
//! type parameter's binding, but never visited `type_predicate.type_id` — the
//! only place a predicate-returning function's target type lives (its
//! `return_type` is just `boolean`). A type parameter whose sole concrete
//! evidence was a sibling's predicate target was therefore invisible to this
//! pre-seed pass, which independently gates the deeper stripping/defaulting
//! path (`should_strip_sensitive_placeholder_substitution`,
//! `crates/tsz-checker/src/checkers/call_context.rs`) that issue #16022
//! traced. #16024 (`argument_provides_type_param_evidence`) already fixed the
//! simpler 2-type-param variant of this family; these tests cover the
//! 3-type-param shape that survived it.
//!
//! Pinned against real `tsc` 7.0.2 (`--noEmit --strict`), reduced from
//! `superjson/src/transformer.ts`'s `classRule`/`compositeTransformation`.

fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(source)
}

fn assert_no_ts18046(diagnostics: &[(u32, String)], context: &str) {
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 18046),
        "Did not expect TS18046 in {context}. Got: {diagnostics:?}"
    );
}

fn assert_no_ts2698(diagnostics: &[(u32, String)], context: &str) {
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2698),
        "Did not expect TS2698 in {context}. Got: {diagnostics:?}"
    );
}

#[test]
fn predicate_evidence_survives_when_type_param_is_also_return_only_elsewhere() {
    // The real #16022 shape: `I` is evidenced only by `isApplicable`'s
    // predicate target, and is *also* return-only in `untransform`'s
    // parameter type alongside a third type parameter `A` that itself
    // depends on the sensitive `annotation` callback.
    let diagnostics = compile_and_get_diagnostics(
        r#"
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
    return { ...clazz };
  },
  (v, a) => v
);
export { classRule };
"#,
    );
    assert_no_ts18046(&diagnostics, "three-type-param classRule shape");
    assert_no_ts2698(&diagnostics, "three-type-param classRule shape");
}

#[test]
fn renamed_binders_three_type_param_predicate_evidence_is_structural() {
    // Same shape with every type parameter and parameter name renamed,
    // proving the rule is structural rather than keyed on a specific
    // identifier.
    let diagnostics = compile_and_get_diagnostics(
        r#"
function isRegistered(candidate: any): candidate is any {
  return !!candidate?.constructor;
}

function makeCodec<Target, Encoded, Meta>(
  guard: (raw: any) => raw is Target,
  describe: (value: Target) => Meta,
  encode: (value: Target) => Encoded,
  decode: (encoded: Encoded, meta: Meta) => Target
) {
  return { guard, describe, encode, decode };
}

const codec = makeCodec(
  isRegistered,
  (value) => {
    return 'meta';
  },
  (value) => {
    return { ...value };
  },
  (encoded, meta) => encoded
);
export { codec };
"#,
    );
    assert_no_ts18046(&diagnostics, "renamed three-type-param shape");
    assert_no_ts2698(&diagnostics, "renamed three-type-param shape");
}

// A "no predicate evidence" negative control for this exact 3-type-param
// shape is deliberately not included here: with `isApplicable` replaced by a
// plain boolean-returning function, `I` has no evidence source at all, and a
// separate, pre-existing gap in `should_strip_sensitive_placeholder_substitution`
// (return-only type param in a third sensitive callback) already defaults it
// to `any` instead of `unknown` on main, independent of this fix — verified
// against real tsc 7.0.2, which reports TS2698 there and tsz does not, both
// before and after this change. The simpler 2-type-param negative control
// (`type_predicate_sibling_argument_evidence_tests.rs`,
// `non_predicate_sibling_without_evidence_still_reports_unknown`) does not
// hit that gap and stays green.
