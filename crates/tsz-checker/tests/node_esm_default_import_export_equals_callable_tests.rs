//! When an ESM file in nodenext mode default-imports a CJS module that uses
//! `export = X`, the imported binding's type must be the type of `X` directly
//! (matching tsc semantics: ESM-imports-CJS treats `module.exports` as the
//! default when no `__esModule` marker is present, and `export = X` sets
//! `module.exports = X`).
//!
//! Previously tsz returned a synthesized namespace object for every default
//! import of a node-CJS module, which made `import nullthrows from 'pkg'`
//! non-callable even when the underlying export was a function. This
//! regression test pins the structural rule: when the resolved exports table
//! contains an `export=` entry, the default import's type is the export-equals
//! target's type — not the wrapping namespace shape.
//!
//! Conformance: `nodeNextEsmImportsOfPackagesWithExtensionlessMains.ts`.

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostics_for_esm_default_import(dep_source: &str, main_source: &str) -> Vec<(u32, String)> {
    let mut parser_dep = ParserState::new("dep.d.ts".to_string(), dep_source.to_string());
    let root_dep = parser_dep.parse_source_file();
    let mut binder_dep = BinderState::new();
    binder_dep.bind_source_file(parser_dep.get_arena(), root_dep);

    let mut parser_main = ParserState::new("main.ts".to_string(), main_source.to_string());
    let root_main = parser_main.parse_source_file();
    let mut binder_main = BinderState::new();
    binder_main.bind_source_file(parser_main.get_arena(), root_main);

    let arena_dep = Arc::new(parser_dep.get_arena().clone());
    let arena_main = Arc::new(parser_main.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena_dep), Arc::clone(&arena_main)]);

    let dep_exports = binder_dep.module_exports.get("dep.d.ts").cloned();
    if let Some(exports) = &dep_exports {
        std::sync::Arc::make_mut(&mut binder_main.module_exports)
            .insert("./dep".to_string(), exports.clone());
    }

    let mut cross_file_targets = FxHashMap::default();
    if let Some(exports) = &dep_exports {
        for (_name, &sym_id) in exports.iter() {
            cross_file_targets.insert(sym_id, 0usize);
        }
    }

    let binder_dep = Arc::new(binder_dep);
    let binder_main = Arc::new(binder_main);
    let all_binders = Arc::new(vec![Arc::clone(&binder_dep), Arc::clone(&binder_main)]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_main.as_ref(),
        binder_main.as_ref(),
        &types,
        "main.ts".to_string(),
        CheckerOptions {
            module: ModuleKind::NodeNext,
            no_lib: true,
            ..CheckerOptions::default()
        },
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(1);
    checker.ctx.file_is_esm = Some(true);
    let mut file_esm_map: FxHashMap<String, bool> = FxHashMap::default();
    file_esm_map.insert("main.ts".to_string(), true);
    file_esm_map.insert("dep.d.ts".to_string(), false);
    checker.ctx.file_is_esm_map = Some(Arc::new(file_esm_map));
    for (sym_id, file_idx) in &cross_file_targets {
        checker.ctx.register_symbol_file_target(*sym_id, *file_idx);
    }

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((1, "./dep".to_string()), 0);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));

    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert("./dep".to_string());
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(root_main);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn esm_default_import_of_export_equals_function_is_callable() {
    let dep = r#"
declare function nullthrows(x: any): any;
declare namespace nullthrows {
    export {nullthrows as default};
}
export = nullthrows;
"#;
    let main = r#"
import nullthrows from './dep';
export function call(): any {
    return nullthrows(1);
}
"#;
    let diagnostics = diagnostics_for_esm_default_import(dep, main);
    assert!(
        diagnostics.iter().all(|(c, _)| *c != 2349),
        "Default import of export=function from CJS in nodenext-ESM should \
         be callable (no TS2349). Got: {diagnostics:#?}"
    );
}

#[test]
fn esm_default_import_of_export_equals_class_is_constructable() {
    // Same structural rule applies for `export = class`: the default import in
    // nodenext-ESM resolves to the class type itself, which is constructable.
    let dep = r#"
declare class Box {
    constructor(value: number);
    value: number;
}
export = Box;
"#;
    let main = r#"
import Box from './dep';
export const b = new Box(1);
"#;
    let diagnostics = diagnostics_for_esm_default_import(dep, main);
    assert!(
        diagnostics.iter().all(|(c, _)| *c != 2349 && *c != 2351),
        "Default import of export=class from CJS in nodenext-ESM should be \
         constructable (no TS2349/TS2351). Got: {diagnostics:#?}"
    );
}

#[test]
fn esm_default_import_of_export_equals_function_with_namespace_members_is_callable_with_alt_param_name()
 {
    // The fix must be structural — it must not depend on the user-chosen
    // alias name. Re-exercise the rule with a different identifier than
    // `nullthrows` (and a parameter renamed `arg`) to lock that in.
    let dep = r#"
declare function fn1(arg: any): any;
declare namespace fn1 {
    export {fn1 as default};
}
export = fn1;
"#;
    let main = r#"
import fn1 from './dep';
export function call(): any {
    return fn1(1);
}
"#;
    let diagnostics = diagnostics_for_esm_default_import(dep, main);
    assert!(
        diagnostics.iter().all(|(c, _)| *c != 2349),
        "Default import of export=function from CJS in nodenext-ESM should \
         be callable for any user-chosen identifier (no TS2349). \
         Got: {diagnostics:#?}"
    );
}
