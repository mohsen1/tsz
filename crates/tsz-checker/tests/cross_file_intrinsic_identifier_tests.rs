use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::module_resolution::build_module_resolution_maps;
use tsz_checker::state::CheckerState;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn compile_entry_file(files: &[(&str, &str)], entry_idx: usize) -> Vec<(u32, String)> {
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in files {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);
    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
    let options = CheckerOptions {
        module: ModuleKind::CommonJS,
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };

    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &types,
        file_names[entry_idx].clone(),
        options,
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

    checker
        .ctx
        .diagnostics
        .iter()
        .filter(|diag| diag.code != 2318)
        .map(|diag| (diag.code, diag.message_text.clone()))
        .collect()
}

#[test]
fn exported_undefined_alias_does_not_shadow_intrinsic_undefined_in_other_module() {
    let zod_like_exports = r#"
const undefinedType = (params?: {}) => params;
export { undefinedType as undefined };
"#;

    let zod_like_util = r#"
export function find<T>(value: T): T | undefined {
    if (false) return value;
    return undefined;
}
"#;

    let diagnostics = compile_entry_file(
        &[
            ("types.ts", zod_like_exports),
            ("helpers/util.ts", zod_like_util),
        ],
        1,
    );
    let codes: Vec<u32> = diagnostics.iter().map(|(code, _)| *code).collect();

    assert!(
        !codes.contains(&2322),
        "exported alias named undefined from another module must not shadow intrinsic undefined; got {diagnostics:#?}"
    );
}
