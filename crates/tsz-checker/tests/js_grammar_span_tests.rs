use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_source(source: &str, file_name: &str, options: CheckerOptions) -> Vec<tsz_common::diagnostics::Diagnostic> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let source_file = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), source_file);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(source_file);
    checker.ctx.diagnostics.clone()
}

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
    assert_eq!(ts8009[0].length, 1, "unexpected diagnostic length: {ts8009:#?}");
}
