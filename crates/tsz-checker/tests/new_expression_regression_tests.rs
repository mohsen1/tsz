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

/// TS17013's `{0}` placeholder must be substituted with the canonical
/// meta-property text (`new.target`), not leaked verbatim (#14840).
#[test]
fn ts17013_top_level_new_target_substitutes_meta_property_name() {
    let source = "console.log(new.target);";
    let diagnostics = check_source_diagnostics(source);
    let diag = diagnostics
        .iter()
        .find(|d| {
            d.code
                == diagnostic_codes::META_PROPERTY_IS_ONLY_ALLOWED_IN_THE_BODY_OF_A_FUNCTION_DECLARATION_FUNCTION_EXP
        })
        .expect("expected TS17013 at top level");
    assert!(
        diag.message_text.contains("'new.target'"),
        "TS17013 must name the meta-property: {diag:?}",
    );
    assert!(
        !diag.message_text.contains("{0}"),
        "TS17013 must not leak the raw placeholder: {diag:?}",
    );
}

/// tsc reports the *canonical* `new.target` even for a misspelled name like
/// `new.foo` (its `checkNewTargetMetaProperty` hardcodes `"new.target"`).
#[test]
fn ts17013_misspelled_new_meta_property_still_names_new_target() {
    let source = "const x = new.foo;";
    let diagnostics = check_source_diagnostics(source);
    let diag = diagnostics
        .iter()
        .find(|d| {
            d.code
                == diagnostic_codes::META_PROPERTY_IS_ONLY_ALLOWED_IN_THE_BODY_OF_A_FUNCTION_DECLARATION_FUNCTION_EXP
        })
        .expect("expected TS17013 for new.foo at top level");
    assert!(
        diag.message_text.contains("'new.target'") && !diag.message_text.contains("{0}"),
        "TS17013 for a misspelled meta-property must still name 'new.target': {diag:?}",
    );
}

/// A `new.target` in a non-function owner (here a class field initializer) hits
/// the same invalid-context branch and must substitute the placeholder too.
#[test]
fn ts17013_in_class_field_initializer_substitutes_meta_property_name() {
    let source = "class C { f = new.target; }";
    let diagnostics = check_source_diagnostics(source);
    let diag = diagnostics
        .iter()
        .find(|d| {
            d.code
                == diagnostic_codes::META_PROPERTY_IS_ONLY_ALLOWED_IN_THE_BODY_OF_A_FUNCTION_DECLARATION_FUNCTION_EXP
        })
        .expect("expected TS17013 in class field initializer");
    assert!(
        diag.message_text.contains("'new.target'") && !diag.message_text.contains("{0}"),
        "TS17013 in a field initializer must name 'new.target': {diag:?}",
    );
}

/// `new.target` inside a function body is valid: no TS17013.
#[test]
fn ts17013_not_emitted_for_new_target_in_function_body() {
    let source = "function f() { return new.target; }";
    let diagnostics = check_source_diagnostics(source);
    let count = diagnostics
        .iter()
        .filter(|d| {
            d.code
                == diagnostic_codes::META_PROPERTY_IS_ONLY_ALLOWED_IN_THE_BODY_OF_A_FUNCTION_DECLARATION_FUNCTION_EXP
        })
        .count();
    assert_eq!(
        count, 0,
        "No TS17013 for new.target in a function body: {diagnostics:?}",
    );
}

/// When a new-expression argument is a context-sensitive callback with a
/// BLOCK body whose return type fails against the constructor parameter, tsc
/// reports TS2345 at the argument level (return reason as elaboration) instead
/// of drilling an inner TS2322 — `checkTypeRelatedToAndOptionallyElaborate`
/// only elaborates concise bodies. The call-expression path already followed
/// this rule via `callback_prefers_argument_level_return_mismatch`; the
/// new-expression result arm must apply the same gate (oracle: tsc 7.0.2,
/// strict and non-strict).
fn assert_single_argument_level_ts2345(source: &str) {
    let diagnostics = check_source_diagnostics(source);
    let ts2345_count = diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .count();
    let ts2322_count = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .count();
    assert_eq!(
        ts2345_count, 1,
        "block-body callback mismatch must surface exactly one argument-level TS2345: {diagnostics:?}",
    );
    assert_eq!(
        ts2322_count, 0,
        "no TS2322 may leak for a block-body callback argument mismatch: {diagnostics:?}",
    );
}

#[test]
fn new_block_body_function_callback_mismatch_is_ts2345_nongeneric() {
    assert_single_argument_level_ts2345(
        r#"
class Gadget {
    constructor(cb: (x: number) => number) {}
}
new Gadget(function (a) { return ''; });
"#,
    );
}

#[test]
fn new_block_body_arrow_callback_mismatch_is_ts2345_explicit_type_args() {
    assert_single_argument_level_ts2345(
        r#"
class Crate2<TA, TB> {
    constructor(a: TA, cb: (x: TA) => TB, b: TB) {}
}
new Crate2<number, number>(1, (a) => { return ''; }, 1);
"#,
    );
}

#[test]
fn new_block_body_function_callback_mismatch_is_ts2345_inferred_type_args() {
    assert_single_argument_level_ts2345(
        r#"
class Sack<E1, E2> {
    constructor(a: E1, cb: (x: E1) => E2, b: E2) {}
}
new Sack(1, function (item) { return ''; }, 1);
"#,
    );
}

/// Renamed-binder twin of the explicit-type-args witness: the rule must not
/// depend on class/parameter spellings.
#[test]
fn new_block_body_callback_mismatch_is_ts2345_renamed_binders() {
    assert_single_argument_level_ts2345(
        r#"
class Zug<Q, R> {
    constructor(erste: Q, wandler: (wert: Q) => R, letzte: R) {}
}
new Zug<string, string>('a', function (wert) { return 0; }, 'b');
"#,
    );
}

/// Negative control: a CONCISE arrow body elaborates to a TS2322 on the
/// returned expression itself (tsc drills concise bodies), so the
/// argument-level promotion must not fire.
#[test]
fn new_concise_arrow_body_mismatch_keeps_expression_level_ts2322() {
    let source = r#"
class Kite<M, N> {
    constructor(a: M, cb: (x: M) => N, b: N) {}
}
new Kite<number, number>(1, (a) => '', 1);
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2322_count = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .count();
    let ts2345_count = diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .count();
    assert_eq!(
        ts2322_count, 1,
        "concise arrow body keeps the expression-level TS2322: {diagnostics:?}",
    );
    assert_eq!(
        ts2345_count, 0,
        "concise arrow body must not also promote an argument-level TS2345: {diagnostics:?}",
    );
}

/// Negative control: object-literal arguments keep their per-property TS2322
/// elaboration; the block-body gate must not swallow it.
#[test]
fn new_object_literal_argument_keeps_elaborated_ts2322() {
    let source = r#"
class Opts2 {
    constructor(o: { a: number }) {}
}
new Opts2({ a: '' });
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2322_count = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .count();
    assert_eq!(
        ts2322_count, 1,
        "object-literal argument keeps the elaborated TS2322: {diagnostics:?}",
    );
}

/// Guard: the call-expression twin of the fixed witness stays on the
/// argument-level TS2345 path.
#[test]
fn call_block_body_function_callback_mismatch_stays_ts2345() {
    assert_single_argument_level_ts2345(
        r#"
declare function fabrik<K1, K2>(a: K1, cb: (x: K1) => K2, b: K2): void;
fabrik<number, number>(1, function (a) { return ''; }, 1);
"#,
    );
}
