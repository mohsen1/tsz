//! Regression tests for [issue #16018]: an explicitly annotated parameter on a
//! sibling callback argument must count as inference evidence for a shared
//! type parameter.
//!
//! Structural rule: when a generic call passes two callback arguments whose
//! contextual parameter types share a type parameter `T`, and the sibling
//! callback annotates the parameter sitting at a position whose contextual
//! type mentions `T`, `tsc` infers `T` contravariantly from that annotation
//! and fixes it before contextually typing the other callback's body.
//!
//! tsz's post-generic `emit_uninferred_callback_unknown_body_diagnostics`
//! recheck (`argument_provides_type_param_evidence`,
//! `crates/tsz-checker/src/types/computation/call_inference/unknown_callback.rs`)
//! counted a sibling *callback* argument as evidence only when `T` occurred in
//! that callback's contextual **return** position, on the reasoning that
//! parameter positions are contravariant and supply context *to* the lambda.
//! That reasoning holds only for an *unannotated* parameter, whose type is
//! genuinely produced by the contextual type. An explicitly annotated
//! parameter is an inference *source*, so excluding it made the recheck
//! conclude "no evidence", default `T` to `unknown`, and re-check the sibling
//! callback body against it — a spurious `TS18046`.
//!
//! Every expectation below is pinned against real `tsc` 7.0.2
//! (`--noEmit --strict`); the negative controls
//! (`unannotated_sibling_callback_still_reports_unknown`,
//! `annotation_at_a_non_type_param_position_is_not_evidence`) are the cases
//! where `tsc` genuinely does report `TS18046`, and they are what force the
//! rule to be *position-aware* rather than "the sibling has some annotation".
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

fn count_ts18046(diagnostics: &[(u32, String)]) -> usize {
    diagnostics
        .iter()
        .filter(|(code, _)| *code == 18046)
        .count()
}

#[test]
fn annotated_sibling_callback_param_seeds_type_param() {
    // tsc 7.0.2: clean. `T` is inferred as `string` from `value: string`.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function run<T>(setup: (value: T) => void, use: (value: T) => void): void;
run((value: string) => { value.length; }, (value) => { value.length; });
"#,
    );
    assert_no_ts18046(&diagnostics, "annotated sibling callback parameter");
}

#[test]
fn annotated_sibling_callback_param_seeds_renamed_type_param() {
    // Same shape with every binder renamed (`Elem`/`first`/`second`/`item`) —
    // proves the rule is structural, not keyed on any identifier.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function apply<Elem>(first: (item: Elem) => void, second: (item: Elem) => void): void;
apply((item: string) => { item.length; }, (item) => { item.length; });
"#,
    );
    assert_no_ts18046(&diagnostics, "renamed type parameter `Elem`");
}

#[test]
fn partially_annotated_sibling_callback_is_still_evidence() {
    // tsc 7.0.2: clean. The sibling is context-sensitive as a whole (its
    // second parameter is unannotated), but the annotated first parameter
    // still supplies the contravariant candidate for `T`.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function run<T>(setup: (value: T, extra: number) => void, use: (value: T) => void): void;
run((value: string, extra) => { value.length; }, (value) => { value.length; });
"#,
    );
    assert_no_ts18046(&diagnostics, "partially annotated sibling callback");
}

#[test]
fn annotated_function_expression_sibling_is_evidence() {
    // The sibling need not be an arrow function.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function run<T>(setup: (value: T) => void, use: (value: T) => void): void;
run(function (value: string): void { value.length; }, (value) => { value.length; });
"#,
    );
    assert_no_ts18046(&diagnostics, "annotated function-expression sibling");
}

#[test]
fn type_param_nested_inside_annotated_param_is_evidence() {
    // `T` occurs nested inside the contextual parameter type rather than
    // being the whole parameter.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function run<T>(setup: (box: { item: T }) => void, use: (value: T) => void): void;
run((box: { item: string }) => { box.item.length; }, (value) => { value.length; });
"#,
    );
    assert_no_ts18046(&diagnostics, "type parameter nested in an annotated param");
}

#[test]
fn type_param_under_an_array_in_annotated_param_is_evidence() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function run<T>(setup: (values: T[]) => void, use: (value: T) => void): void;
run((values: number[]) => { values.length; }, (value) => { value.toFixed(); });
"#,
    );
    assert_no_ts18046(
        &diagnostics,
        "type parameter under an array in an annotated param",
    );
}

#[test]
fn annotated_param_through_a_generic_alias_callable_is_evidence() {
    // The contextual parameter type is an aliased callable interface stored as
    // an `Application`, not a bare function type — the same wrapper shape
    // #7653 exercises for the return position.
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface Take<U> { (n: U): void; }
declare function run<U>(setup: Take<U>, use: Take<U>): void;
run((value: string) => { value.length; }, (value) => { value.length; });
"#,
    );
    assert_no_ts18046(
        &diagnostics,
        "annotated param through a generic alias callable",
    );
}

#[test]
fn unannotated_sibling_callback_still_reports_unknown() {
    // NEGATIVE CONTROL. tsc 7.0.2 reports TS18046 twice here: neither lambda
    // annotates anything, so `T` really is uninferred and falls to `unknown`.
    // The fix must not silence this.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function run<T>(setup: (value: T) => void, use: (value: T) => void): void;
run((value) => { value.length; }, (value) => { value.length; });
"#,
    );
    assert_eq!(
        count_ts18046(&diagnostics),
        2,
        "both unannotated callback bodies must keep TS18046. Got: {diagnostics:?}"
    );
}

#[test]
fn annotation_at_a_non_type_param_position_is_not_evidence() {
    // NEGATIVE CONTROL, and the discriminating case. The sibling *does* carry
    // an explicit annotation, but at the `tag: string` position — the `value`
    // position that actually mentions `T` is unannotated. tsc 7.0.2 still
    // reports TS18046 on the second callback's body, so "sibling has some
    // annotation" is the wrong rule; the annotation must sit at a position
    // whose contextual type mentions the type parameter.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function run<T>(setup: (value: T, tag: string) => void, use: (value: T) => void): void;
run((value, tag: string) => { tag.length; }, (value) => { value.length; });
"#,
    );
    assert_eq!(
        count_ts18046(&diagnostics),
        1,
        "an annotation away from the type parameter's position is not evidence. Got: {diagnostics:?}"
    );
}

#[test]
fn return_position_evidence_still_works() {
    // The pre-existing rule (#7653) must keep holding: `T` in the sibling
    // callback's contextual return position is evidence on its own, with no
    // annotated parameter anywhere.
    let diagnostics = compile_and_get_diagnostics(
        r#"
// @target: es2015
function foo<T>(o: Take<T>, i: Make<T>) { }
interface Make<T> { (): T; }
interface Take<T> { (n: T): void; }
foo(n => n.length, () => 'hi');
"#,
    );
    assert_no_ts18046(&diagnostics, "return-position evidence (#7653)");
}
