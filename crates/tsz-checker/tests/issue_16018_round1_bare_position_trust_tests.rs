//! Regression tests for [issue #16018]: the post-generic uninferred-callback
//! recheck must trust the call's own already-computed generic solve for a
//! bare type-parameter position, instead of exclusively re-deriving "was
//! there evidence" from a narrower argument-shape heuristic.
//!
//! Structural rule: `emit_uninferred_callback_unknown_body_diagnostics`
//! (`crates/tsz-checker/src/types/computation/call_inference/unknown_callback.rs`)
//! decides whether a shared type parameter `T`, consumed by a
//! context-sensitive callback argument, was genuinely left uninferred by
//! Round 1. For a sibling parameter position declared as a bare `T`
//! reference, the call's own `finalized_contextual_param_types` (the
//! callee's parameter types after substituting the real solver-inferred type
//! arguments — the same answer `tsc` already computed) is authoritative and
//! must be consulted directly (`bare_type_param_position_resolved_by_round1`)
//! rather than re-derived solely from the sibling argument's own checked
//! type, which can lag the checker's own later refinements
//! (`refine_instantiated_params_with_checker_substitution`,
//! `refine_bare_instantiated_params_with_direct_literal_conflicts`) that
//! land in the substituted parameter type but not in the raw argument type
//! array.
//!
//! This complements `issue_16018_annotated_sibling_callback_evidence_tests`
//! (the annotated-sibling-parameter evidence channel), which is a different
//! sub-fix under the same tracking issue.
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
fn bare_sibling_seeds_type_param_for_callback() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function ap<I>(seed: I, use: (v: I) => void): void;
declare const seed: { field: number };
ap(seed, item => { item.field; });
"#,
    );
    assert_no_ts18046(&diagnostics, "bare sibling seeds I");
}

#[test]
fn bare_sibling_seeds_type_param_with_renamed_binder() {
    // Same shape with `J`/`use`/`v` renamed — proves the rule is structural,
    // not keyed on a specific identifier (per CLAUDE.md's anti-hardcoding
    // gate).
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function apply<J>(x: J, run: (w: J) => void): void;
declare const x: { member: string };
apply(x, w => { w.member; });
"#,
    );
    assert_no_ts18046(&diagnostics, "renamed type parameter J");
}

#[test]
fn bare_sibling_seeds_type_param_when_third_argument_present() {
    // Three arguments, current callback last, bare-T seed first — exercises
    // the "other" position scan beyond the minimal two-argument shape.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function ap3<I>(seed: I, label: string, use: (v: I) => void): void;
declare const seed: { field: number };
ap3(seed, "l", item => { item.field; });
"#,
    );
    assert_no_ts18046(&diagnostics, "bare sibling seeds I with extra argument");
}

#[test]
fn callback_with_unannotated_param_still_emits_when_t_truly_uninferred() {
    // Negative control (mirrors issue_7653's own fallback case): neither
    // argument is a bare-T position, so the trusted-Round1 path must not
    // fire, and the narrower heuristic still correctly reports TS18046.
    let diagnostics = compile_and_get_diagnostics(
        r#"
function foo<T>(o: (n: T) => void, i: (t: T) => void) { }
foo(n => n.length, t => { });
"#,
    );
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 18046),
        "Expected TS18046 for genuinely uninferred T. Got: {diagnostics:?}"
    );
}
