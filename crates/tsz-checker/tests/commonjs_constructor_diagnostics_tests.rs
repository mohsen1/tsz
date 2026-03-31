use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

#[test]
fn test_commonjs_constructor_index_error_uses_exported_function_name() {
    let a_source = r#"
const s = Symbol();
function F() {}
F.prototype[s] = "ok";
module.exports.F = F;
module.exports.S = s;
"#;
    let b_source = r#"
const x = require("./a.js");
const inst = new x.F();
inst[x.S];
"#;

    let mut parser_a = ParserState::new("a.js".to_string(), a_source.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = ParserState::new("b.js".to_string(), b_source.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    let arena_a = Arc::new(parser_a.get_arena().clone());
    let arena_b = Arc::new(parser_b.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena_a), Arc::clone(&arena_b)]);

    let file_a_exports = binder_a.module_exports.get("a.js").cloned();
    if let Some(exports) = &file_a_exports {
        binder_b
            .module_exports
            .insert("./a.js".to_string(), exports.clone());
    }

    let mut cross_file_targets = FxHashMap::default();
    if let Some(exports) = &file_a_exports {
        for (_name, &sym_id) in exports.iter() {
            cross_file_targets.insert(sym_id, 0usize);
        }
    }

    let binder_a = Arc::new(binder_a);
    let binder_b = Arc::new(binder_b);
    let all_binders = Arc::new(vec![Arc::clone(&binder_a), Arc::clone(&binder_b)]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_b.as_ref(),
        binder_b.as_ref(),
        &types,
        "b.js".to_string(),
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            no_lib: true,
            module: tsz_common::common::ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(1);
    for (sym_id, file_idx) in &cross_file_targets {
        checker.ctx.register_symbol_file_target(*sym_id, *file_idx);
    }

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((1, "./a.js".to_string()), 0);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));

    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert("./a.js".to_string());
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(root_b);

    let ts7053 = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 7053)
        .map(|d| d.message_text.as_str())
        .collect::<Vec<_>>();

    assert!(
        ts7053.is_empty(),
        "Expected no false TS7053 for cross-file CommonJS constructor symbol-keyed access, got: {ts7053:#?}"
    );
}

#[test]
fn test_commonjs_exported_unique_symbol_stays_concrete_for_namespace_index_errors() {
    let a_source = r#"
const s = Symbol();
const str = "my-fake-sym";
module.exports[s] = "ok";
module.exports[str] = "ok";
module.exports.S = s;
"#;
    let b_source = r#"
const x = require("./a.js");
x[x.S];
"#;

    let mut parser_a = ParserState::new("a.js".to_string(), a_source.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = ParserState::new("b.js".to_string(), b_source.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    let arena_a = Arc::new(parser_a.get_arena().clone());
    let arena_b = Arc::new(parser_b.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena_a), Arc::clone(&arena_b)]);

    let file_a_exports = binder_a.module_exports.get("a.js").cloned();
    if let Some(exports) = &file_a_exports {
        binder_b
            .module_exports
            .insert("./a.js".to_string(), exports.clone());
    }

    let mut cross_file_targets = FxHashMap::default();
    if let Some(exports) = &file_a_exports {
        for (_name, &sym_id) in exports.iter() {
            cross_file_targets.insert(sym_id, 0usize);
        }
    }

    let binder_a = Arc::new(binder_a);
    let binder_b = Arc::new(binder_b);
    let all_binders = Arc::new(vec![Arc::clone(&binder_a), Arc::clone(&binder_b)]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_b.as_ref(),
        binder_b.as_ref(),
        &types,
        "b.js".to_string(),
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            target: tsz_common::common::ScriptTarget::ES2015,
            module: tsz_common::common::ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(1);
    for (sym_id, file_idx) in &cross_file_targets {
        checker.ctx.register_symbol_file_target(*sym_id, *file_idx);
    }

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((1, "./a.js".to_string()), 0);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));

    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert("./a.js".to_string());
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(root_b);

    let ts7053 = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 7053)
        .map(|d| d.message_text.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        ts7053.len(),
        1,
        "Expected one TS7053 for indexing the imported CommonJS namespace with its exported unique symbol, got: {ts7053:#?}"
    );
    assert!(
        ts7053[0].contains("expression of type 'unique symbol'"),
        "Expected the imported CommonJS export to preserve unique symbol identity, got: {ts7053:#?}"
    );
}
