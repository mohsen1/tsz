use std::sync::Arc;

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_ts_file_with_prior_js_global(js_source: &str, ts_source: &str) -> Vec<u32> {
    let mut parser_js = ParserState::new("a.js".to_string(), js_source.to_string());
    let root_js = parser_js.parse_source_file();
    let mut binder_js = BinderState::new();
    binder_js.bind_source_file(parser_js.get_arena(), root_js);

    let mut parser_ts = ParserState::new("a.ts".to_string(), ts_source.to_string());
    let root_ts = parser_ts.parse_source_file();
    let mut binder_ts = BinderState::new();
    binder_ts.bind_source_file(parser_ts.get_arena(), root_ts);

    let arena_js = Arc::new(parser_js.get_arena().clone());
    let arena_ts = Arc::new(parser_ts.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena_js), Arc::clone(&arena_ts)]);

    let binder_js = Arc::new(binder_js);
    let binder_ts = Arc::new(binder_ts);
    let all_binders = Arc::new(vec![Arc::clone(&binder_js), Arc::clone(&binder_ts)]);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        allow_js: true,
        check_js: false,
        no_lib: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        arena_ts.as_ref(),
        binder_ts.as_ref(),
        &types,
        "a.ts".to_string(),
        options,
    );
    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(1);
    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root_ts);
    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

#[test]
fn unchecked_js_global_does_not_trigger_cross_file_ts2403() {
    let codes = check_ts_file_with_prior_js_global(r#"var t = [1, "x"];"#, r#"var t: [any, any];"#);

    assert!(
        !codes.contains(&2403),
        "Unchecked JS globals should not participate in cross-file TS2403 comparisons. Actual codes: {codes:?}"
    );
}

#[test]
fn checked_js_global_does_not_trigger_cross_file_ts2403() {
    let codes = check_ts_file_with_prior_js_global(
        "// @ts-check\nvar t = [1, \"x\"];",
        r#"var t: [any, any];"#,
    );

    assert!(
        !codes.contains(&2403),
        "Checked JS globals should not act as the source side of cross-file TS2403 comparisons. Actual codes: {codes:?}"
    );
}
