use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

#[test]
fn js_optional_class_elements_report_ts8009_at_question_token() {
    let source = r#"class C {
    foo?() {
    }
    bar? = 1;
}"#;

    let diagnostics = check_source(
        source,
        "a.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts8009: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 8009)
        .collect();

    assert_eq!(ts8009.len(), 2, "unexpected diagnostics: {ts8009:#?}");

    let first_q = source.find('?').expect("first optional marker") as u32;
    let second_q = source.rfind('?').expect("second optional marker") as u32;

    assert!(
        ts8009
            .iter()
            .any(|diag| diag.start == first_q && diag.length == 1),
        "Expected method optional marker to anchor at '?'. Actual diagnostics: {ts8009:#?}"
    );
    assert!(
        ts8009
            .iter()
            .any(|diag| diag.start == second_q && diag.length == 1),
        "Expected property optional marker to anchor at '?'. Actual diagnostics: {ts8009:#?}"
    );
}

#[test]
fn js_optional_parameters_report_ts8009_at_question_token() {
    let source = "function F(p?) { }";

    let diagnostics = check_source(
        source,
        "a.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts8009: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 8009)
        .collect();

    assert_eq!(ts8009.len(), 1, "unexpected diagnostics: {ts8009:#?}");

    let question = source.find('?').expect("optional marker") as u32;
    assert_eq!(
        ts8009[0].start, question,
        "Expected parameter optional marker to anchor at '?'. Actual diagnostics: {ts8009:#?}"
    );
    assert_eq!(
        ts8009[0].length, 1,
        "unexpected diagnostic length: {ts8009:#?}"
    );
}

#[test]
fn parameter_property_rest_error_anchors_at_modifier() {
    let source = r#"class Foo3 {
  constructor (public ...args: string[]) { }
}"#;

    let diagnostics = check_source(source, "test.ts", CheckerOptions::default());
    let ts1317: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 1317)
        .collect();

    assert_eq!(ts1317.len(), 1, "unexpected diagnostics: {diagnostics:#?}");

    let public_start = source.find("public").expect("public keyword") as u32;
    assert_eq!(
        ts1317[0].start, public_start,
        "Expected TS1317 to anchor at the parameter property modifier. Actual diagnostics: {ts1317:#?}"
    );
}

#[test]
fn js_function_overload_reports_ts8017_at_full_declaration() {
    let source = "function foo(): string;";

    let diagnostics = check_source(
        source,
        "a.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts8017: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 8017)
        .collect();

    assert_eq!(ts8017.len(), 1, "unexpected diagnostics: {diagnostics:#?}");

    assert_eq!(
        ts8017[0].start, 0,
        "Expected TS8017 to anchor at the declaration start. Actual diagnostics: {ts8017:#?}"
    );
    assert_eq!(
        ts8017[0].length,
        source.len() as u32,
        "unexpected diagnostic length: {ts8017:#?}"
    );
    assert!(
        diagnostics.iter().all(|diag| diag.code != 8010),
        "a bodyless signature must not also report TS8010 for its return type: {diagnostics:#?}"
    );
}

#[test]
fn js_class_index_signatures_report_ts8017_at_bracket_without_ts8010() {
    let source = r#"class Registry {
  [label: string]: unknown;
  [slot: number]: string;
}"#;
    let diagnostics = check_source(
        source,
        "a.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..CheckerOptions::default()
        },
    );
    let ts8017: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 8017)
        .collect();
    let bracket_starts: Vec<u32> = source
        .match_indices('[')
        .map(|(start, _)| start as u32)
        .collect();
    assert_eq!(ts8017.len(), bracket_starts.len(), "{diagnostics:#?}");
    assert_eq!(
        ts8017.iter().map(|diag| diag.start).collect::<Vec<_>>(),
        bracket_starts,
        "TS8017 must anchor at each opening bracket: {diagnostics:#?}"
    );
    assert_eq!(
        ts8017.iter().map(|diag| diag.length).collect::<Vec<_>>(),
        vec![25, 23],
        "TS8017 must cover each complete index signature: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().all(|diag| diag.code != 8010),
        "index signatures must not also report TS8010: {diagnostics:#?}"
    );
}

#[test]
fn js_ordinary_typed_property_remains_ts8010_not_ts8017() {
    let source = "class Plain { item: string; }";
    let diagnostics = check_source(
        source,
        "a.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..CheckerOptions::default()
        },
    );
    let ts8010: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 8010)
        .collect();
    assert_eq!(ts8010.len(), 1, "{diagnostics:#?}");
    assert_eq!(ts8010[0].start, source.find("string").unwrap() as u32);
    assert_eq!(ts8010[0].length, "string".len() as u32);
    assert!(diagnostics.iter().all(|diag| diag.code != 8017));
}

#[test]
fn js_property_modifier_policy_accepts_export_and_async_but_rejects_const() {
    let source = r#"class ModifierMatrix {
  export exportedField = 1;
  async deferredField = 2;
  const blockedField = 3;
}"#;
    let diagnostics = check_source(
        source,
        "a.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..CheckerOptions::default()
        },
    );
    let ts8009 = diagnostics
        .iter()
        .filter(|diag| diag.code == 8009)
        .collect::<Vec<_>>();
    assert_eq!(ts8009.len(), 1, "unexpected diagnostics: {diagnostics:#?}");
    assert_eq!(ts8009[0].start, source.find("const").unwrap() as u32);
    assert_eq!(ts8009[0].length, "const".len() as u32);
    assert_eq!(
        ts8009[0].message_text,
        "The 'const' modifier can only be used in TypeScript files."
    );
}

#[test]
fn js_bodyless_accessor_reports_ts8017_for_full_declaration() {
    let source = "class Ghost { get incorporeal(); }";
    let diagnostics = check_source(
        source,
        "a.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..CheckerOptions::default()
        },
    );
    let ts8017: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 8017)
        .collect();
    assert_eq!(ts8017.len(), 1, "{diagnostics:#?}");
    let declaration = "get incorporeal();";
    assert_eq!(
        ts8017[0].start,
        source.find(declaration).unwrap() as u32,
        "{diagnostics:#?}"
    );
    assert_eq!(
        ts8017[0].length,
        declaration.len() as u32,
        "{diagnostics:#?}"
    );
    assert!(diagnostics.iter().all(|diag| diag.code != 8010));
}

#[test]
fn js_namespace_declaration_uses_parser_namespace_flag_for_ts8006_text() {
    let source = "/* module */ namespace N {}";

    let diagnostics = check_source(
        source,
        "a.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts8006: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 8006)
        .collect();

    assert_eq!(ts8006.len(), 1, "unexpected diagnostics: {diagnostics:#?}");
    assert!(
        ts8006[0].message_text.contains("'namespace' declarations"),
        "expected TS8006 to identify a namespace declaration from parser flags. Actual diagnostics: {ts8006:#?}"
    );
}

#[test]
fn js_module_declaration_ignores_namespace_word_in_comment_for_ts8006_text() {
    let source = "/* namespace */ module M {}";

    let diagnostics = check_source(
        source,
        "a.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts8006: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == 8006)
        .collect();

    assert_eq!(ts8006.len(), 1, "unexpected diagnostics: {diagnostics:#?}");
    assert!(
        ts8006[0].message_text.contains("'module' declarations"),
        "expected TS8006 to identify a module declaration from parser flags. Actual diagnostics: {ts8006:#?}"
    );
}
