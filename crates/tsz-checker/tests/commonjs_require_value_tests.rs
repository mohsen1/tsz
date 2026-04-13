use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_js_require_value_diagnostics(
    json_source: &str,
    js_source: &str,
    user_source: &str,
) -> Vec<(u32, String)> {
    let mut parser_json = ParserState::new("json.json".to_string(), json_source.to_string());
    let root_json = parser_json.parse_source_file();
    let mut binder_json = BinderState::new();
    binder_json.bind_source_file(parser_json.get_arena(), root_json);

    let mut parser_js = ParserState::new("js.js".to_string(), js_source.to_string());
    let root_js = parser_js.parse_source_file();
    let mut binder_js = BinderState::new();
    binder_js.bind_source_file(parser_js.get_arena(), root_js);

    let mut parser_user = ParserState::new("user.js".to_string(), user_source.to_string());
    let root_user = parser_user.parse_source_file();
    let mut binder_user = BinderState::new();
    binder_user.bind_source_file(parser_user.get_arena(), root_user);

    let arena_json = Arc::new(parser_json.get_arena().clone());
    let arena_js = Arc::new(parser_js.get_arena().clone());
    let arena_user = Arc::new(parser_user.get_arena().clone());
    let all_arenas = Arc::new(vec![
        Arc::clone(&arena_json),
        Arc::clone(&arena_js),
        Arc::clone(&arena_user),
    ]);

    let file_js_exports = binder_js.module_exports.get("js.js").cloned();
    if let Some(exports) = &file_js_exports {
        binder_user
            .module_exports
            .insert("./js.js".to_string(), exports.clone());
    }

    let mut cross_file_targets = FxHashMap::default();
    if let Some(exports) = &file_js_exports {
        for (_name, &sym_id) in exports.iter() {
            cross_file_targets.insert(sym_id, 1usize);
        }
    }

    let binder_json = Arc::new(binder_json);
    let binder_js = Arc::new(binder_js);
    let binder_user = Arc::new(binder_user);
    let all_binders = Arc::new(vec![
        Arc::clone(&binder_json),
        Arc::clone(&binder_js),
        Arc::clone(&binder_user),
    ]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_user.as_ref(),
        binder_user.as_ref(),
        &types,
        "user.js".to_string(),
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            no_lib: true,
            resolve_json_module: true,
            ..Default::default()
        },
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(2);
    for (sym_id, file_idx) in &cross_file_targets {
        checker.ctx.register_symbol_file_target(*sym_id, *file_idx);
    }

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((2, "./json.json".to_string()), 0);
    resolved_module_paths.insert((2, "./js.js".to_string()), 1);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));

    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert("./json.json".to_string());
    resolved_modules.insert("./js.js".to_string());
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(root_user);

    checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn check_js_require_of_json_preserves_property_presence_and_assignment() {
    let diagnostics = check_js_require_value_diagnostics(
        r#"{ "a": 0 }"#,
        r#"module.exports = { a: 0 };"#,
        r#"
const json0 = require("./json.json");
json0.b;

/** @type {{ b: number }} */
const json1 = require("./json.json");
json1.b;
"#,
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    let ts2741: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2741)
        .collect();

    assert_eq!(
        ts2339.len(),
        1,
        "Expected the JSON require property read to keep module value shape, got: {diagnostics:#?}"
    );
    assert!(
        ts2339
            .iter()
            .all(|(_, message)| message
                .contains("Property 'b' does not exist on type '{ a: number; }'")),
        "Expected the JSON require read to report against '{{ a: number; }}', got: {diagnostics:#?}"
    );
    assert_eq!(
        ts2741.len(),
        1,
        "Expected the JSDoc-annotated JSON require to check assignment compatibility, got: {diagnostics:#?}"
    );
    assert!(
        ts2741[0]
            .1
            .contains("Property 'b' is missing in type '{ a: number; }' but required in type '{ b: number; }'."),
        "Expected JSON require assignment mismatch to report missing property, got: {diagnostics:#?}"
    );
}

#[test]
#[ignore = "pre-existing regression"]
fn check_js_require_of_commonjs_preserves_property_presence_and_assignment() {
    let diagnostics = check_js_require_value_diagnostics(
        r#"{ "a": 0 }"#,
        r#"module.exports = { a: 0 };"#,
        r#"
const js0 = require("./js.js");
js0.b;

/** @type {{ b: number }} */
const js1 = require("./js.js");
js1.b;
"#,
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    let ts2741: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2741)
        .collect();

    assert_eq!(
        ts2339.len(),
        1,
        "Expected the CommonJS require property read to keep module value shape, got: {diagnostics:#?}"
    );
    assert!(
        ts2339
            .iter()
            .all(|(_, message)| message
                .contains("Property 'b' does not exist on type '{ a: number; }'")),
        "Expected the CommonJS require read to report against '{{ a: number; }}', got: {diagnostics:#?}"
    );
    assert_eq!(
        ts2741.len(),
        1,
        "Expected the JSDoc-annotated CommonJS require to check assignment compatibility, got: {diagnostics:#?}"
    );
    assert!(
        ts2741[0]
            .1
            .contains("Property 'b' is missing in type '{ a: number; }' but required in type '{ b: number; }'."),
        "Expected CommonJS require assignment mismatch to report missing property, got: {diagnostics:#?}"
    );
}
