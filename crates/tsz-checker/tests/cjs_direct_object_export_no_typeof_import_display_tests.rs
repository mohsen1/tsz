//! Files whose exports surface is a single `module.exports = { … }` object
//! literal (no separate `exports.foo = …` augmentation) should display as the
//! literal shape in diagnostic messages — not as `typeof import("mod")`.
//!
//! tsc renders the require value as `{ a: number; }`; tsz used to tag the
//! synthesized type with `namespace_module_names` unconditionally, producing
//! `typeof import("js")` instead. This regression test pins the parity by
//! asserting the source-side display in a TS2339 message is the literal
//! shape rather than a typeof-import alias.

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostics_for_three_files(
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
        std::sync::Arc::make_mut(&mut binder_user.module_exports)
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
fn require_of_direct_object_export_displays_literal_shape_in_ts2339() {
    let diagnostics = diagnostics_for_three_files(
        r#"{ "a": 0 }"#,
        r#"module.exports = { a: 0 };"#,
        r#"
const js0 = require("./js.js");
js0.b;
"#,
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert_eq!(
        ts2339.len(),
        1,
        "Expected one TS2339 for `js0.b`, got: {diagnostics:#?}"
    );
    let msg = &ts2339[0].1;
    assert!(
        msg.contains("'{ a: number; }'"),
        "Expected the require value to display as the literal shape `{{ a: number; }}`, got: {msg}"
    );
    assert!(
        !msg.contains("typeof import("),
        "Expected NO `typeof import(\"…\")` alias for a single-direct-export file, got: {msg}"
    );
}
