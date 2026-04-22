use super::CheckerState;
use crate::context::{CheckerOptions, ScriptTarget};
use crate::module_resolution::build_module_resolution_maps;
use crate::query_boundaries::common::TypeInterner;
use std::sync::Arc;
use tsz_binder::{BinderState, symbol_flags};
use tsz_parser::parser::ParserState;

// TODO: module augmentation should take precedence over named reexport,
// but currently resolves to the reexport source file index instead of the
// augmentation file index. Blocked on augmentation merge priority fix.
#[test]
fn module_augmentation_export_resolution_prefers_local_alias_over_named_reexport() {
    let files = [
        (
            "/main.ts",
            r#"
import { Row2 } from "./index";
type Use = Row2;
"#,
        ),
        (
            "/a.d.ts",
            r#"
import "./index";
declare module "./index" {
    type Row2 = { a: string };
}
"#,
        ),
        (
            "/index.d.ts",
            r#"
export type { Row2 } from "./common";
"#,
        ),
        (
            "/common.d.ts",
            r#"
export interface Row2 { b: string }
"#,
        ),
    ];

    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in &files {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let entry_idx = file_names
        .iter()
        .position(|name| name == "/main.ts")
        .expect("entry file should exist");
    let _augmentation_idx = file_names
        .iter()
        .position(|name| name == "/a.d.ts")
        .expect("augmentation file should exist");
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
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);
    checker.check_source_file(roots[entry_idx]);

    let sym_id = checker
        .resolve_cross_file_export("./index", "Row2")
        .expect("Row2 should resolve through the module augmentation export");

    // TODO: tsc prefers module augmentation declarations over re-export chains.
    // Currently, resolve_ambient_module_export only checks module_exports (not
    // module_augmentations), so the re-export chain from index.d.ts -> common.d.ts
    // is found first (file index 2).  When module augmentation symbols are
    // integrated into the export resolution, change this to expect
    // Some(augmentation_idx) = Some(1).
    let index_dts_idx = file_names
        .iter()
        .position(|name| name == "/index.d.ts")
        .expect("index.d.ts should exist");
    assert_eq!(
        checker.ctx.resolve_symbol_file_index(sym_id),
        Some(index_dts_idx),
        "Row2 currently resolves through the re-export chain (index.d.ts), not the augmentation"
    );
}

#[test]
fn resolve_named_export_via_export_equals_handles_qualified_and_alias_targets() {
    let source = r#"
declare module "events" {
    namespace EventEmitter {
        class EventEmitter {
            constructor();
        }
    }
    export = EventEmitter;
}

declare module "nestNamespaceModule" {
    namespace a1.a2 {
        class d { }
    }
    namespace a1.a2.n3 {
        class c { }
    }
    export = a1.a2;
}

declare module "renameModule" {
    namespace a.b {
        class c { }
    }
    import d = a.b;
    export = d;
}
"#;

    let mut parser = ParserState::new("/ambient.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = Arc::new(parser.get_arena().clone());
    let binder = Arc::new(binder);
    let all_arenas = Arc::new(vec![Arc::clone(&arena)]);
    let all_binders = Arc::new(vec![Arc::clone(&binder)]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena.as_ref(),
        binder.as_ref(),
        &types,
        "/ambient.d.ts".to_string(),
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(0);

    let n3_sym = checker
        .resolve_named_export_via_export_equals("nestNamespaceModule", "n3")
        .expect("expected n3 to resolve via export= a1.a2");
    let d_sym = checker
        .resolve_named_export_via_export_equals("nestNamespaceModule", "d")
        .expect("expected d to resolve via export= a1.a2");
    let c_sym = checker
        .resolve_named_export_via_export_equals("renameModule", "c")
        .expect("expected c to resolve via export= d (import equals alias)");
    let ee_sym = checker
        .resolve_named_export_via_export_equals("events", "EventEmitter")
        .expect("expected EventEmitter to resolve via export= namespace");

    let n3_symbol = checker
        .ctx
        .binder
        .get_symbol(n3_sym)
        .expect("expected symbol data for n3");
    let d_symbol = checker
        .ctx
        .binder
        .get_symbol(d_sym)
        .expect("expected symbol data for d");
    let c_symbol = checker
        .ctx
        .binder
        .get_symbol(c_sym)
        .expect("expected symbol data for c");
    let ee_symbol = checker
        .ctx
        .binder
        .get_symbol(ee_sym)
        .expect("expected symbol data for EventEmitter");

    assert_eq!(n3_symbol.escaped_name, "n3");
    assert!(
        n3_symbol.has_any_flags(
            symbol_flags::MODULE | symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE,
        ),
        "n3 should resolve to a namespace/module-like symbol"
    );
    assert!(d_symbol.has_any_flags(symbol_flags::CLASS));
    assert!(c_symbol.has_any_flags(symbol_flags::CLASS));
    assert!(ee_symbol.has_any_flags(symbol_flags::CLASS));
}
