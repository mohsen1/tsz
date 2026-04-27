//! Tests for isolatedModules import/export-default conflict diagnostics:
//!
//! - TS1292: `'X' resolves to a type and must be marked type-only in this file
//!   before re-exporting when 'isolatedModules' is enabled.` Triggered by
//!   `export default X` where X is an alias to a type-only symbol.
//! - TS2440: import declaration conflicts with local declaration. Triggered
//!   when both the import target and the local symbol carry pure-type
//!   meanings (no Value).
//! - TS2865: import conflicts with local value, must use `import type` under
//!   isolatedModules. Triggered when the import is type-only at the target
//!   but the local file declares a runtime value with the same name.

use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::module_resolution::build_module_resolution_maps;
use tsz_checker::state::CheckerState;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn compile_with_isolated_modules(
    files: &[(&str, &str)],
    entry_idx: usize,
) -> Vec<(u32, String, u32)> {
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
        isolated_modules: true,
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
        .map(|d| (d.code, d.message_text.clone(), d.start))
        .collect()
}

/// `import { T }` from a type-only module + `export default T`:
/// emits TS1292 because the alias target is type-only.
/// (Mirrors `isolatedModulesExportDeclarationType.ts` test3.ts.)
#[test]
fn export_default_of_type_only_alias_emits_ts1292() {
    let type_ts = "export type T = number;\n";
    let test3_ts = "import { T } from \"./type\";\nexport default T;\n";

    let diags = compile_with_isolated_modules(&[("/type.ts", type_ts), ("/test3.ts", test3_ts)], 1);

    assert!(
        diags.iter().any(|(code, _, _)| *code == 1292),
        "Expected TS1292 for `export default T` when T resolves to a type-only \
         alias under isolatedModules. Got: {diags:?}"
    );
}

/// `import { T }` (type-only target) + `type T = number` (local) + `export default T`:
/// emits both TS2440 (local type alias clashes with the imported type) and
/// TS1292 (export default of a type-only merged alias under isolatedModules).
/// (Mirrors test2.ts in `isolatedModulesExportDeclarationType.ts`.)
#[test]
fn import_type_clashing_with_local_type_alias_emits_ts2440_and_ts1292() {
    let type_ts = "export type T = number;\n";
    let test2_ts = "import { T } from \"./type\";\ntype T = number;\nexport default T;\n";

    let diags = compile_with_isolated_modules(&[("/type.ts", type_ts), ("/test2.ts", test2_ts)], 1);

    assert!(
        diags.iter().any(|(code, _, _)| *code == 2440),
        "Expected TS2440 when imported type-only T clashes with local `type T = number`. \
         Got: {diags:?}"
    );
    assert!(
        diags.iter().any(|(code, _, _)| *code == 1292),
        "Expected TS1292 for `export default T` when T resolves to a type-only \
         merged alias. Got: {diags:?}"
    );
}

/// `import { T }` (type-only target) + `const T = 0` (local value) + `export default T`:
/// emits TS2865 because under isolatedModules the import would be erased,
/// leaving the const visible to the transpiler. The export default is OK
/// (the const provides a runtime value), so no TS1292 fires.
/// (Mirrors test1.ts in `isolatedModulesExportDeclarationType.ts`.)
#[test]
fn import_type_clashing_with_local_const_emits_ts2865() {
    let type_ts = "export type T = number;\n";
    let test1_ts = "import { T } from \"./type\";\nconst T = 0;\nexport default T;\n";

    let diags = compile_with_isolated_modules(&[("/type.ts", type_ts), ("/test1.ts", test1_ts)], 1);

    assert!(
        diags.iter().any(|(code, _, _)| *code == 2865),
        "Expected TS2865 when imported type-only T clashes with `const T = 0` \
         under isolatedModules. Got: {diags:?}"
    );
    assert!(
        diags.iter().all(|(code, _, _)| *code != 1292),
        "Expected no TS1292 when `export default T` resolves to the local const value. \
         Got: {diags:?}"
    );
}

/// Sanity check: type-only imports with no local conflicts must not emit
/// TS2440/TS2865 even under isolatedModules.
#[test]
fn type_only_import_with_no_local_conflict_is_clean() {
    let type_ts = "export type T = number;\n";
    let consumer_ts = "import { T } from \"./type\";\nexport type Alias = T;\n";

    let diags =
        compile_with_isolated_modules(&[("/type.ts", type_ts), ("/consumer.ts", consumer_ts)], 1);

    assert!(
        diags
            .iter()
            .all(|(code, _, _)| *code != 2440 && *code != 2865),
        "Expected no TS2440/TS2865 for a clean type-only import. Got: {diags:?}"
    );
}

/// `import type { T }` is a type-only import — it should never trigger TS2865
/// even when a local value of the same name exists, because the import is
/// already type-only.
#[test]
fn type_only_import_modifier_suppresses_ts2865() {
    let type_ts = "export type T = number;\n";
    let consumer_ts = "import type { T } from \"./type\";\nconst T = 0;\nexport { T };\n";

    let diags =
        compile_with_isolated_modules(&[("/type.ts", type_ts), ("/consumer.ts", consumer_ts)], 1);

    assert!(
        diags.iter().all(|(code, _, _)| *code != 2865),
        "Expected no TS2865 when the import already uses `import type`. Got: {diags:?}"
    );
}
