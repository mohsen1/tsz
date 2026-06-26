//! TS1259: a default import of a CommonJS `export =` module
//! (`import X from "m"`) binds a *synthetic* default that is only legal under
//! `esModuleInterop` / `allowSyntheticDefaultImports`. Without either flag tsc
//! reports:
//!
//! ```text
//! error TS1259: Module '"m"' can only be default-imported using the '<flag>' flag
//! ```
//!
//! where `<flag>` is `esModuleInterop` for CommonJS/AMD/UMD output
//! (module < ES2015) and `allowSyntheticDefaultImports` for ES2015+ output —
//! the same module-kind selection tsc uses for the namespace/named TS2497
//! elaboration.
//!
//! Previously tsz silently accepted the default import: `has_default_binding`
//! (and the default-binding fast path it relies on) treats an `export =` entry
//! as an unconditional default provider, so the interop-gated TS1259 path was
//! never reached. These tests pin the structural rule across module kinds,
//! `export =` target shapes (variable / class / function), interop toggles, and
//! varied binder names so the fix cannot regress into a name-keyed fast path.

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_common::common::ModuleKind;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

/// Check `main.ts` (which imports `./dep`) under the supplied module/interop
/// configuration and return the emitted `(code, message)` pairs.
fn diagnostics_for_default_import(
    dep_source: &str,
    main_source: &str,
    module: ModuleKind,
    es_module_interop: bool,
    allow_synthetic_default_imports: bool,
) -> Vec<(u32, String)> {
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
            module,
            es_module_interop,
            allow_synthetic_default_imports,
            no_lib: true,
            ..CheckerOptions::default()
        },
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(1);
    checker.ctx.file_is_esm = Some(false);
    let mut file_esm_map: FxHashMap<String, bool> = FxHashMap::default();
    file_esm_map.insert("main.ts".to_string(), false);
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

fn ts1259(diagnostics: &[(u32, String)]) -> Option<&str> {
    diagnostics
        .iter()
        .find(|(code, _)| *code == 1259)
        .map(|(_, msg)| msg.as_str())
}

const DEP_VAR: &str = r#"
declare const obj: { a: number };
export = obj;
"#;
const MAIN_VAR: &str = r#"
import obj from "./dep";
export const a: number = obj.a;
"#;

#[test]
fn commonjs_no_interop_reports_ts1259_with_esmoduleinterop_flag() {
    let diagnostics =
        diagnostics_for_default_import(DEP_VAR, MAIN_VAR, ModuleKind::CommonJS, false, false);
    let msg = ts1259(&diagnostics).unwrap_or_else(|| {
        panic!("expected TS1259 for default import of export= under CommonJS without interop, got: {diagnostics:#?}")
    });
    // CommonJS output (module < ES2015) → tsc suggests `esModuleInterop`.
    assert!(
        msg.contains("esModuleInterop"),
        "TS1259 under CommonJS must suggest 'esModuleInterop', got: {msg:?}"
    );
    // tsc renders the module name as the bare basename, double-quoted.
    assert!(
        msg.contains("\"dep\""),
        "TS1259 must render the module as '\"dep\"' (basename, quoted), got: {msg:?}"
    );
}

#[test]
fn esmodule_target_no_interop_reports_ts1259_with_allowsynthetic_flag() {
    // ES2015+ output → tsc suggests `allowSyntheticDefaultImports` instead.
    let diagnostics =
        diagnostics_for_default_import(DEP_VAR, MAIN_VAR, ModuleKind::ES2022, false, false);
    let msg = ts1259(&diagnostics).unwrap_or_else(|| {
        panic!("expected TS1259 for default import of export= under ES2022 without interop, got: {diagnostics:#?}")
    });
    assert!(
        msg.contains("allowSyntheticDefaultImports"),
        "TS1259 under ES2015+ must suggest 'allowSyntheticDefaultImports', got: {msg:?}"
    );
}

#[test]
fn esmoduleinterop_suppresses_ts1259() {
    let diagnostics =
        diagnostics_for_default_import(DEP_VAR, MAIN_VAR, ModuleKind::CommonJS, true, false);
    assert!(
        ts1259(&diagnostics).is_none(),
        "esModuleInterop must suppress TS1259, got: {diagnostics:#?}"
    );
}

#[test]
fn allow_synthetic_default_imports_suppresses_ts1259() {
    let diagnostics =
        diagnostics_for_default_import(DEP_VAR, MAIN_VAR, ModuleKind::CommonJS, false, true);
    assert!(
        ts1259(&diagnostics).is_none(),
        "allowSyntheticDefaultImports must suppress TS1259, got: {diagnostics:#?}"
    );
}

#[test]
fn export_equals_class_target_reports_ts1259() {
    // The `export =` target shape (here a class) does not change the rule: a
    // default import still requires interop. Vary the binder names so the fix
    // cannot key on the user-chosen identifier.
    let dep = r#"
declare class Widget {
    constructor(value: number);
    value: number;
}
export = Widget;
"#;
    let main = r#"
import Widget from "./dep";
export const w = new Widget(1);
"#;
    let diagnostics = diagnostics_for_default_import(dep, main, ModuleKind::CommonJS, false, false);
    assert!(
        ts1259(&diagnostics).is_some(),
        "default import of export= class without interop must report TS1259, got: {diagnostics:#?}"
    );
}

#[test]
fn export_equals_function_target_reports_ts1259_distinct_names() {
    // Function target + a different identifier and parameter name: still TS1259.
    let dep = r#"
declare function compute(input: string): number;
export = compute;
"#;
    let main = r#"
import compute from "./dep";
export const n: number = compute("x");
"#;
    let diagnostics = diagnostics_for_default_import(dep, main, ModuleKind::CommonJS, false, false);
    assert!(
        ts1259(&diagnostics).is_some(),
        "default import of export= function without interop must report TS1259, got: {diagnostics:#?}"
    );
}
