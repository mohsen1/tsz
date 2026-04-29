use super::*;
use crate::context::{CheckerOptions, ScriptTarget};
use crate::module_resolution::build_module_resolution_maps;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

#[test]
fn check_imported_members_emits_ts18042_for_default_interface_import_in_js() {
    let files = [
        (
            "dep.d.ts",
            r#"
export default interface TruffleContract {
  foo: number;
}
                "#,
        ),
        (
            "caller.js",
            r#"
import TruffleContract from "./dep";
                "#,
        ),
    ];

    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in files {
        let mut parser = ParserState::new(name.to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let entry_idx = file_names
        .iter()
        .position(|name| name == "caller.js")
        .expect("entry file should exist");
    let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);

    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &types,
        file_names[entry_idx].clone(),
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ES2015,
            module: ModuleKind::ES2020,
            ..CheckerOptions::default()
        },
    );

    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    checker.ctx.set_lib_contexts(Vec::new());
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);
    checker.check_source_file(roots[entry_idx]);

    let source_file = checker
        .ctx
        .arena
        .get(roots[entry_idx])
        .and_then(|node| checker.ctx.arena.get_source_file(node))
        .expect("source file data should exist");
    let import_idx = *source_file
        .statements
        .nodes
        .first()
        .expect("entry file should start with an import");
    let import = checker
        .ctx
        .arena
        .get(import_idx)
        .and_then(|node| checker.ctx.arena.get_import_decl(node))
        .cloned()
        .expect("import declaration should exist");
    let clause = checker
        .ctx
        .arena
        .get(import.import_clause)
        .and_then(|node| checker.ctx.arena.get_import_clause(node))
        .expect("import clause should exist");

    assert!(clause.name.is_some(), "expected default import binding");
    assert!(
        checker.import_binding_is_type_only("./dep", "default"),
        "default import should be recognized as type-only"
    );

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&18042),
        "expected TS18042 from the checked-JS import walk, got codes: {codes:?}"
    );
}
