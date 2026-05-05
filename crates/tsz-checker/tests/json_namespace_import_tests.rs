use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostics_for_json_namespace_import(
    json_source: &str,
    user_file_name: &str,
    user_source: &str,
) -> Vec<(u32, String)> {
    let mut parser_json = ParserState::new("config.json".to_string(), json_source.to_string());
    let root_json = parser_json.parse_source_file();
    let mut binder_json = BinderState::new();
    binder_json.bind_source_file(parser_json.get_arena(), root_json);

    let mut parser_user = ParserState::new(user_file_name.to_string(), user_source.to_string());
    let root_user = parser_user.parse_source_file();
    let mut binder_user = BinderState::new();
    binder_user.bind_source_file(parser_user.get_arena(), root_user);

    let arena_json = Arc::new(parser_json.get_arena().clone());
    let arena_user = Arc::new(parser_user.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena_json), Arc::clone(&arena_user)]);

    let binder_json = Arc::new(binder_json);
    let binder_user = Arc::new(binder_user);
    let all_binders = Arc::new(vec![Arc::clone(&binder_json), Arc::clone(&binder_user)]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_user.as_ref(),
        binder_user.as_ref(),
        &types,
        user_file_name.to_string(),
        CheckerOptions {
            no_lib: true,
            target: ScriptTarget::ES2022,
            module: ModuleKind::Node18,
            resolve_json_module: true,
            ..Default::default()
        },
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(1);

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((1, "./config.json".to_string()), 0);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));

    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert("./config.json".to_string());
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
fn cts_json_namespace_import_exposes_json_object_directly() {
    let diagnostics = diagnostics_for_json_namespace_import(
        r#"{ "version": 1 }"#,
        "main.cts",
        r#"
import * as config from "./config.json";
config.version;
config.default;
"#,
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        1,
        "Expected one TS2339 for config.default, got: {diagnostics:#?}"
    );
    assert!(
        ts2339[0]
            .1
            .contains("Property 'default' does not exist on type '{ version: number; }'."),
        "Expected the CJS JSON namespace to be the JSON object shape, got: {diagnostics:#?}"
    );
}

#[test]
fn mts_json_namespace_import_exposes_default_only() {
    let diagnostics = diagnostics_for_json_namespace_import(
        r#"{ "version": 1 }"#,
        "main.mts",
        r#"
import * as config from "./config.json" with { type: "json" };
config.version;
config.default;
"#,
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        1,
        "Expected one TS2339 for config.version, got: {diagnostics:#?}"
    );
    assert!(
        ts2339[0].1.contains(
            "Property 'version' does not exist on type '{ default: { version: number; }; }'."
        ),
        "Expected the ESM JSON namespace to expose only default, got: {diagnostics:#?}"
    );
}

#[test]
fn cts_json_namespace_default_property_points_at_json_object() {
    let diagnostics = diagnostics_for_json_namespace_import(
        r#"{ "name": "pkg", "default": "misedirection" }"#,
        "main.cts",
        r#"
import * as config from "./config.json";
config.name;
config.default.name;
config.default.default;
"#,
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected the CJS JSON namespace `default` property to expose the JSON object, got: {diagnostics:#?}"
    );
}
