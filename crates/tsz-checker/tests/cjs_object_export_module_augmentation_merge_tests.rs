//! Module augmentation that adds an interface for a class re-exported via
//! a CommonJS object literal (`module.exports = { Cls }`) is *valid*
//! declaration merging — class+interface for instance-side, namespace for
//! static-side. The duplicate-identifier check on CJS object exports used
//! to only allow function-merges; this regression test pins the broader
//! class-and-friends merge skip.

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostics_for_two_files(
    js_source: &str,
    ts_source: &str,
    user_file_idx: usize,
) -> Vec<(u32, String)> {
    let mut parser_js = ParserState::new("test.js".to_string(), js_source.to_string());
    let root_js = parser_js.parse_source_file();
    let mut binder_js = BinderState::new();
    binder_js.bind_source_file(parser_js.get_arena(), root_js);

    let mut parser_ts = ParserState::new("index.ts".to_string(), ts_source.to_string());
    let root_ts = parser_ts.parse_source_file();
    let mut binder_ts = BinderState::new();
    binder_ts.bind_source_file(parser_ts.get_arena(), root_ts);

    let arena_js = Arc::new(parser_js.get_arena().clone());
    let arena_ts = Arc::new(parser_ts.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena_js), Arc::clone(&arena_ts)]);

    let file_js_exports = binder_js.module_exports.get("test.js").cloned();
    if let Some(exports) = &file_js_exports {
        std::sync::Arc::make_mut(&mut binder_ts.module_exports)
            .insert("./test".to_string(), exports.clone());
    }

    let mut cross_file_targets = FxHashMap::default();
    if let Some(exports) = &file_js_exports {
        for (_name, &sym_id) in exports.iter() {
            cross_file_targets.insert(sym_id, 0usize);
        }
    }

    let binder_js = Arc::new(binder_js);
    let binder_ts = Arc::new(binder_ts);
    let all_binders = Arc::new(vec![Arc::clone(&binder_js), Arc::clone(&binder_ts)]);

    let types = TypeInterner::new();
    let (target_arena, target_binder, target_root, target_file_name) = if user_file_idx == 0 {
        (&arena_js, &binder_js, root_js, "test.js")
    } else {
        (&arena_ts, &binder_ts, root_ts, "index.ts")
    };
    let mut checker = CheckerState::new(
        target_arena.as_ref(),
        target_binder.as_ref(),
        &types,
        target_file_name.to_string(),
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            no_lib: true,
            ..Default::default()
        },
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(user_file_idx);
    for (sym_id, file_idx) in &cross_file_targets {
        checker.ctx.register_symbol_file_target(*sym_id, *file_idx);
    }

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((1, "./test".to_string()), 0);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));

    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert("./test".to_string());
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(target_root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn cjs_object_export_class_and_interface_module_aug_does_not_emit_ts2300() {
    let js_source = r#"
class Abcde {
}

module.exports = {
  Abcde
};
"#;
    let ts_source = r#"
import { Abcde } from "./test";

declare module "./test" {
  interface Abcde { b: string }
}
"#;
    let diags = diagnostics_for_two_files(js_source, ts_source, 0);
    let ts2300: Vec<_> = diags.iter().filter(|(c, _)| *c == 2300).collect();
    assert!(
        ts2300.is_empty(),
        "Expected no TS2300 (interface augmenting a CJS-exported class is valid declaration merging), got: {diags:#?}"
    );
}
