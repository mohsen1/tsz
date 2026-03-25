use crate::context::CheckerOptions;
use crate::state::CheckerState;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_types_file_with_jsdoc_global(
    types_source: &str,
    js_source: &str,
    options: CheckerOptions,
) -> Vec<u32> {
    let mut parser_types = ParserState::new("types.ts".to_string(), types_source.to_string());
    let root_types = parser_types.parse_source_file();
    let mut binder_types = BinderState::new();
    binder_types.bind_source_file(parser_types.get_arena(), root_types);

    let mut parser_js = ParserState::new("other.js".to_string(), js_source.to_string());
    let root_js = parser_js.parse_source_file();
    let mut binder_js = BinderState::new();
    binder_js.bind_source_file(parser_js.get_arena(), root_js);

    let arena_types = Arc::new(parser_types.get_arena().clone());
    let arena_js = Arc::new(parser_js.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena_types), Arc::clone(&arena_js)]);

    let binder_types = Arc::new(binder_types);
    let binder_js = Arc::new(binder_js);
    let all_binders = Arc::new(vec![Arc::clone(&binder_types), Arc::clone(&binder_js)]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_types.as_ref(),
        binder_types.as_ref(),
        &types,
        "types.ts".to_string(),
        options,
    );
    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(0);
    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root_types);
    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

#[test]
fn cross_file_jsdoc_typedef_is_visible_from_ts_type_reference() {
    let codes = check_types_file_with_jsdoc_global(
        r#"
export interface F {
    (): E;
}
export interface D<T extends F = F> {}
"#,
        r#"/** @typedef {import("./types").D} E */"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..Default::default()
        },
    );

    assert!(
        !codes.contains(&2304),
        "Expected no TS2304 for cross-file JSDoc typedef visible from TS file, got codes: {codes:?}"
    );
}

#[test]
fn cross_file_generic_jsdoc_typedef_preserves_arity_and_constraints() {
    let codes = check_types_file_with_jsdoc_global(
        r#"
declare var actually: Everything<{ a: number }, undefined, { c: 1, d: 1 }, number, string>;
"#,
        r#"
/**
 * @template {{ a: number, b: string }} T,U A Comment
 * @template {{ c: boolean }} V trailing prose should not become params
 * @template W
 * @template X That last one had no comment
 * @typedef {{ t: T, u: U, v: V, w: W, x: X }} Everything
 */
"#,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            ..Default::default()
        },
    );

    assert!(
        !codes.contains(&2304),
        "Expected generic cross-file JSDoc typedef to stay visible from TS, got codes: {codes:?}"
    );
    assert!(
        codes.contains(&2344),
        "Expected TS2344 from generic JSDoc typedef constraint checking, got codes: {codes:?}"
    );
}
