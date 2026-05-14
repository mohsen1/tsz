use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn codes_with_file_is_esm(source: &str, file_is_esm: Option<bool>) -> Vec<u32> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let source_file = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), source_file);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.ctx.set_lib_contexts(Vec::new());
    checker.ctx.file_is_esm = file_is_esm;
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(source_file);

    checker
        .ctx
        .diagnostics
        .into_iter()
        .map(|diag| diag.code)
        .collect()
}

fn codes(source: &str) -> Vec<u32> {
    codes_with_file_is_esm(source, None)
}

#[test]
fn unresolved_await_call_in_sync_function_reports_async_suggestion() {
    let codes = codes(
        r#"
declare function value(): number;
function f() {
    const x = await(value());
}
"#,
    );

    assert!(
        codes.contains(
            &diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_TO_WRITE_THIS_IN_AN_ASYNC_FUNCTION
        ),
        "expected TS2311 for unresolved `await` call callee, got {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_NAME),
        "TS2311 should replace generic TS2304 for unresolved `await` call callee, got {codes:?}"
    );
    assert!(
        !codes.contains(
            &diagnostic_codes::AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS
        ),
        "`await(value())` in a sync function is parsed as a call to an unresolved identifier, not an await expression; got {codes:?}"
    );
}

#[test]
fn unresolved_await_spaced_call_in_sync_function_reports_async_suggestion() {
    let codes = codes(
        r#"
declare function value(): number;
function f() {
    const x = await (value());
}
"#,
    );

    assert!(
        codes.contains(
            &diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_TO_WRITE_THIS_IN_AN_ASYNC_FUNCTION
        ),
        "expected TS2311 for spaced unresolved `await` call callee, got {codes:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_NAME),
        "spaced call syntax should not fall back to generic TS2304, got {codes:?}"
    );
}

#[test]
fn bare_unresolved_await_keeps_generic_cannot_find_name() {
    let codes = codes(
        r#"
function f() {
    const x = await;
}
"#,
    );

    assert!(
        codes.contains(&diagnostic_codes::CANNOT_FIND_NAME),
        "bare unresolved `await` should still report TS2304, got {codes:?}"
    );
    assert!(
        !codes.contains(
            &diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_TO_WRITE_THIS_IN_AN_ASYNC_FUNCTION
        ),
        "bare unresolved `await` must not get the call-specific TS2311 suggestion, got {codes:?}"
    );
}

#[test]
fn await_expression_in_sync_function_keeps_await_context_error() {
    let codes = codes(
        r#"
declare function value(): number;
function f() {
    const x = await value();
}
"#,
    );

    assert!(
        codes.contains(
            &diagnostic_codes::AWAIT_EXPRESSIONS_ARE_ONLY_ALLOWED_WITHIN_ASYNC_FUNCTIONS_AND_AT_THE_TOP_LEVELS
        ),
        "`await value()` should remain an await expression diagnostic, got {codes:?}"
    );
    assert!(
        !codes.contains(
            &diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_TO_WRITE_THIS_IN_AN_ASYNC_FUNCTION
        ),
        "await expressions must not be rewritten to unresolved-name TS2311, got {codes:?}"
    );
}

#[test]
fn top_level_await_call_in_external_module_does_not_report_async_suggestion() {
    let codes = codes_with_file_is_esm(
        r#"
export {};
declare function value(): number;
const x = await(value());
"#,
        Some(true),
    );

    assert!(
        !codes.contains(
            &diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_TO_WRITE_THIS_IN_AN_ASYNC_FUNCTION
        ),
        "top-level external-module `await(value())` should not get sync-function TS2311, got {codes:?}"
    );
}
