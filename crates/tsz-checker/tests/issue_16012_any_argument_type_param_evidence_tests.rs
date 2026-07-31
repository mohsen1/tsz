//! Regression tests for [issue #16012]: an `any`-typed sibling argument must
//! count as inference evidence for a shared, unconstrained type parameter.
//!
//! Structural rule: when a generic call's only inference candidate for an
//! unconstrained type parameter is `any` (from an `any`-typed sibling
//! argument), `tsc` fixes the parameter to `any` before contextually typing a
//! context-sensitive callback argument that mentions it. tsz's post-generic
//! `emit_uninferred_callback_unknown_body_diagnostics` recheck
//! (`argument_provides_type_param_evidence`,
//! `crates/tsz-checker/src/types/computation/call_inference/unknown_callback.rs`)
//! excluded `TypeId::ANY` from counting as evidence, alongside the genuinely
//! uninformative `unknown`/`error`/still-resolving cases. That made the
//! recheck conclude the type parameter had no evidence, default it to
//! `unknown` (no constraint), and re-check the callback body against that
//! defaulted type — surfacing a spurious `TS18046` even though Round 1
//! inference had already correctly fixed the parameter to `any`.
//!
//! [issue #16012]: https://github.com/tsz-org/tsz/issues/16012

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
fn any_seed_argument_seeds_type_param_for_sibling_callback() {
    // Minimal repro from #16012.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function ap<I>(seed: I, use: (v: I) => void): void;
declare var anyVal: any;
ap(anyVal, item => { item.field; });
"#,
    );
    assert_no_ts18046(&diagnostics, "any-seed callback param evidence");
}

#[test]
fn any_seed_seeds_type_param_with_renamed_binder() {
    // Same shape with `J`/`use`/`v` renamed — proves the rule is structural,
    // not keyed on a specific identifier.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function apply<J>(x: J, run: (w: J) => void): void;
declare var dyn: any;
apply(dyn, w => { w.member; });
"#,
    );
    assert_no_ts18046(&diagnostics, "renamed type parameter `J`");
}

#[test]
fn any_seed_via_property_access_seeds_type_param() {
    // The `any`-typed sibling need not be a bare identifier.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function ap<I>(seed: I, use: (v: I) => void): void;
declare var holder: { inner: any };
ap(holder.inner, item => { item.field; });
"#,
    );
    assert_no_ts18046(&diagnostics, "any via property access");
}

#[test]
fn any_seed_with_type_param_in_return_position_seeds_type_param() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function ap2<I>(seed: I, use: (v: I) => void): I;
declare var anyVal: any;
ap2(anyVal, item => { item.field; });
"#,
    );
    assert_no_ts18046(&diagnostics, "type param also in return position");
}

#[test]
fn any_seed_with_callback_listed_first_seeds_type_param() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function ap3<I>(use: (v: I) => void, seed: I): void;
declare var anyVal: any;
ap3(item => { item.field; }, anyVal);
"#,
    );
    assert_no_ts18046(&diagnostics, "callback argument listed before seed");
}

#[test]
fn any_seed_with_second_type_param_seeds_type_param() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function ap6<I, O>(seed: I, use: (v: I) => O): O;
declare var anyVal: any;
ap6(anyVal, item => { item.field; });
"#,
    );
    assert_no_ts18046(&diagnostics, "second, return-only type param");
}

#[test]
fn unknown_and_error_siblings_still_do_not_count_as_evidence() {
    // Negative control: an `unknown`-typed sibling must NOT seed the type
    // parameter, so the genuinely-uninferred recheck still fires. This locks
    // in that only `TypeId::ANY` moved, not the `unknown`/`error` cases.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function ap<I>(seed: I, use: (v: I) => void): void;
declare var unknownVal: unknown;
ap(unknownVal, item => { item.field; });
"#,
    );
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 18046),
        "Expected TS18046 for an unknown-typed seed (no real evidence for `I`). \
         Got: {diagnostics:?}"
    );
}
