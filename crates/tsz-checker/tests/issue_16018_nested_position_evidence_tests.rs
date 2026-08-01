//! Regression tests for [issue #16018]: `bare_type_param_position_resolved_by_round1`
//! (`crates/tsz-checker/src/types/computation/call_inference/unknown_callback.rs`,
//! introduced in #16084) only consulted Round 1's own finalized substitution
//! when a sibling parameter position was declared as a *bare* type-parameter
//! reference (`x: T`, no nesting) — its own doc comment named a type
//! parameter nested inside a compound parameter type (`Array<T>`, a generic
//! alias or wrapper) as unhandled, falling through to the narrower
//! `argument_provides_type_param_evidence` argument-shape heuristic instead.
//!
//! Structural rule: when a sibling parameter position is declared as a
//! compound type mentioning `T` and the call's own `finalized_contextual_param_types`
//! already resolved that position to a fully concrete type, `tsc`'s behavior
//! follows that resolved value regardless of whether the slot's declared
//! shape is bare or nested; tsz's recheck should trust it the same way,
//! recovering `T`'s binding via the same structural-unification primitive
//! (`infer_type_arguments_from_param_args`) `predicate_resolution.rs` already
//! uses to instantiate a type-predicate target nested inside a wrapper type.
//!
//! This is a generalization of #16084's bare-position fix, not a
//! newly-observed diagnostic flip: the non-callback and callback-return/
//! -annotated-parameter branches of `argument_provides_type_param_evidence`
//! already perform a full structural `type_contains_type_parameter_binder`
//! containment check (nested-safe) rather than a bare-identity check, so most
//! nested-position evidence was already recognized through that path. The
//! tests below pin the new code path's behavior — including the case it was
//! written for (a sibling position whose declared type is nested) — and its
//! negative controls, so the removed `bare`-only restriction cannot silently
//! regress a genuinely-uninferred case into a suppressed diagnostic.
//!
//! [issue #16018]: https://github.com/tsz-org/tsz/issues/16018

fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_source_code_messages(source)
}

fn assert_no_ts18046(diagnostics: &[(u32, String)], context: &str) {
    assert!(
        diagnostics.iter().all(|(code, _)| *code != 18046),
        "Did not expect TS18046 in {context}. Got: {diagnostics:?}"
    );
}

#[test]
fn nested_array_sibling_seeds_type_param_for_callback() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function ap<I>(seed: I[], use: (v: I) => void): void;
declare const seed: { field: number }[];
ap(seed, item => { item.field; });
"#,
    );
    assert_no_ts18046(&diagnostics, "nested Array<I> sibling seeds I");
}

#[test]
fn nested_array_sibling_seeds_type_param_with_renamed_binder() {
    // Same shape with `J`/`use`/`v` renamed — proves the rule is structural,
    // not keyed on a specific identifier (per CLAUDE.md's anti-hardcoding
    // gate).
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function apply<J>(xs: J[], run: (w: J) => void): void;
declare const xs: { member: string }[];
apply(xs, w => { w.member; });
"#,
    );
    assert_no_ts18046(&diagnostics, "renamed nested type parameter J");
}

#[test]
fn nested_wrapper_object_sibling_seeds_type_param() {
    // The sibling's declared type is a wrapper object (`{ value: T }`), not
    // an array — exercises the general structural match, not just `Array<T>`.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function withDefault<T>(box: { value: T }, use: (v: T) => void): void;
declare const box: { value: { field: number } };
withDefault(box, item => { item.field; });
"#,
    );
    assert_no_ts18046(&diagnostics, "wrapper object sibling seeds T");
}

#[test]
fn nested_sibling_position_still_emits_when_type_param_truly_uninferred() {
    // Negative control: the only other parameter mentioning `I` is itself an
    // unannotated callback whose body never calls back to any argument that
    // would supply `I` — Round 1 leaves `I` genuinely unresolved, so
    // `finalized_contextual_param_types` at that position still contains a
    // type parameter and the new structural-match path must not manufacture
    // evidence from it.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function ap<I>(seed: (mk: () => I[]) => void, use: (v: I) => void): void;
ap(mk => { mk(); }, item => { item.field; });
"#,
    );
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 18046),
        "Expected TS18046 for genuinely uninferred I. Got: {diagnostics:?}"
    );
}

#[test]
fn empty_array_literal_sibling_still_reports_real_property_error() {
    // Negative control: an empty array literal infers as `never[]`, which is
    // a fully concrete (not type-parameter-containing) resolved type — the
    // structural match must not treat this as "no evidence" and fall back to
    // TS18046; `tsc` reports the real `never`-typed property access instead.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function ap<I>(seed: I[], use: (v: I) => void): void;
ap([], item => { item.field; });
"#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(code, msg)| *code == 2339 && msg.contains("never")),
        "Expected TS2339 on 'never'. Got: {diagnostics:?}"
    );
    assert_no_ts18046(&diagnostics, "empty array literal sibling");
}
