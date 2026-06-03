use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::test_utils::check_source_diagnostics;

#[test]
fn ts1209_invalid_optional_chain_from_new_anchors_question_dot() {
    let source = r#"
class A {
    b() {}
}
new A?.b();
"#;
    let diagnostics = check_source_diagnostics(source);
    let diag = diagnostics
        .iter()
        .find(|d| {
            d.code
                == diagnostic_codes::INVALID_OPTIONAL_CHAIN_FROM_NEW_EXPRESSION_DID_YOU_MEAN_TO_CALL
        })
        .expect("expected TS1209");

    let question_dot_start = source.find("?.").expect("expected optional chain token") as u32;
    assert_eq!(
        diag.start, question_dot_start,
        "TS1209 should anchor at `?.`, got: {diag:?}"
    );
    assert_eq!(diag.length, 2, "TS1209 should cover only `?.`");
}

#[test]
fn new_with_bad_arg_still_emits_ts2339_on_subsequent_member_access() {
    let source = r#"
class C1 {
    constructor(n: number) {}
}
var a = new C1("bad");
a.foo;
"#;
    let codes: Vec<u32> = check_source_diagnostics(source)
        .iter()
        .map(|d| d.code)
        .collect();
    assert!(
        codes.contains(&diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE),
        "Expected TS2345 for bad constructor arg: {codes:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "Expected TS2339 on `a.foo` even when `new C1` had bad args: {codes:?}"
    );
}

/// TS17012 for `new.targ` (misspelled `new.target`) is emitted by the parser,
/// not the checker. This test documents that the checker does NOT add a second
/// TS17012 — it returns `any` for the unknown meta-property to avoid cascading
/// false positives. The full-pipeline TS17012 is exercised by the parser test
/// `test_new_dot_targ_meta_property_ts17012` in `tsz-parser`.
#[test]
fn new_dot_targ_misspelling_checker_emits_no_ts17012() {
    // check_source_diagnostics returns only checker-layer diagnostics, not parse diagnostics.
    // TS17012 originates in the parser; the checker should not double-emit it.
    let source = "function f() { return new.targ; }";
    let diagnostics = check_source_diagnostics(source);
    let ts17012_count = diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::IS_NOT_A_VALID_META_PROPERTY_FOR_KEYWORD_DID_YOU_MEAN
        })
        .count();
    assert_eq!(
        ts17012_count, 0,
        "Checker must not double-emit TS17012 for new.targ; parser owns this diagnostic. \
         All checker diagnostics: {diagnostics:?}",
    );
}

/// `new.target` (correct spelling) must not produce any TS17012 at any layer.
#[test]
fn new_dot_target_correct_spelling_no_ts17012() {
    let source = "function f() { return new.target; }";
    let diagnostics = check_source_diagnostics(source);
    let ts17012_count = diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::IS_NOT_A_VALID_META_PROPERTY_FOR_KEYWORD_DID_YOU_MEAN
        })
        .count();
    assert_eq!(
        ts17012_count, 0,
        "No TS17012 for correctly-spelled new.target: {diagnostics:?}",
    );
}
