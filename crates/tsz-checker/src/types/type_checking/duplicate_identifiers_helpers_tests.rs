use super::CheckerState;

use crate::context::{CheckerOptions, ScriptTarget};
use crate::module_resolution::build_module_resolution_maps;
use crate::query_boundaries::common::TypeInterner;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;

fn with_checker(
    files: &[(&str, &str)],
    entry_file: &str,
    f: impl FnOnce(&mut CheckerState<'_>, usize, usize),
) {
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

    let entry_idx = file_names
        .iter()
        .position(|name| name == entry_file)
        .expect("entry file should exist");
    let index_idx = file_names
        .iter()
        .position(|name| name == "/index.d.ts")
        .expect("index.d.ts should exist");
    let a_idx = file_names
        .iter()
        .position(|name| name == "/a.d.ts")
        .expect("a.d.ts should exist");
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
    f(&mut checker, a_idx, index_idx);
}

#[test]
fn module_augmentation_conflict_helper_sees_target_export_from_augmentation_file() {
    let files = [
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

    with_checker(&files, "/a.d.ts", |checker, _a_idx, index_idx| {
        let conflicts = checker.module_augmentation_conflict_declarations_for_current_file("Row2");

        assert!(
            !conflicts.is_empty(),
            "Expected the augmentation file to see the target export surface as a duplicate partner"
        );
        assert!(
            conflicts.iter().all(|(_, _, is_local, _, _)| !*is_local),
            "Expected augmentation conflicts to be recorded as remote declarations: {conflicts:#?}"
        );
        let index_arena = checker.ctx.get_arena_for_file(index_idx as u32);
        assert!(
            conflicts.iter().any(|(decl_idx, _, _, _, _)| {
                index_arena.get(*decl_idx).is_some_and(|node| {
                    node.kind == tsz_parser::parser::syntax_kind_ext::EXPORT_SPECIFIER
                })
            }),
            "Expected the duplicate partner to be the local export binding in index.d.ts: {conflicts:#?}"
        );
    });
}

#[test]
fn module_augmentation_conflict_helper_sees_augmentation_from_target_file() {
    let files = [
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

    with_checker(&files, "/index.d.ts", |checker, a_idx, _index_idx| {
        let conflicts = checker.module_augmentation_conflict_declarations_for_current_file("Row2");

        assert!(
            !conflicts.is_empty(),
            "Expected the target file to see the augmentation declaration as a duplicate partner"
        );
        let a_arena = checker.ctx.get_arena_for_file(a_idx as u32);
        assert!(
            conflicts.iter().any(|(decl_idx, _, _, _, _)| {
                a_arena.get(*decl_idx).is_some_and(|node| {
                    node.kind == tsz_parser::parser::syntax_kind_ext::TYPE_ALIAS_DECLARATION
                })
            }),
            "Expected the duplicate partner to be the augmentation type alias in a.d.ts: {conflicts:#?}"
        );
    });
}

#[test]
fn module_augmentation_conflict_helper_skips_importing_consumer_file() {
    let files = [
        (
            "/main.ts",
            r#"
import { Row2 } from "./index";
const x: Row2 = {};
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

    with_checker(&files, "/main.ts", |checker, _a_idx, _index_idx| {
        let conflicts = checker.module_augmentation_conflict_declarations_for_current_file("Row2");

        assert!(
            conflicts.is_empty(),
            "Importing consumers should not be treated as module augmentation duplicate partners: {conflicts:#?}"
        );
    });
}

#[test]
fn importing_consumer_row2_alias_stays_local_to_main() {
    let files = [
        (
            "/main.ts",
            r#"
import { Row2 } from "./index";
const x: Row2 = {};
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

    with_checker(&files, "/main.ts", |checker, _a_idx, _index_idx| {
        let sym_id = checker
            .ctx
            .binder
            .file_locals
            .get("Row2")
            .expect("main import alias should exist");
        let symbol = checker
            .ctx
            .binder
            .get_symbol(sym_id)
            .expect("symbol should exist");

        let remote_decl_count = symbol
            .declarations
            .iter()
            .filter_map(|&decl_idx| {
                checker
                    .ctx
                    .binder
                    .declaration_arenas
                    .get(&(sym_id, decl_idx))
            })
            .flat_map(|arenas| arenas.iter())
            .filter(|arena| !std::ptr::eq(arena.as_ref(), checker.ctx.arena))
            .count();

        assert_eq!(
            remote_decl_count, 0,
            "Imported consumer alias should not carry remote declarations: {symbol:#?}"
        );
    });
}

#[test]
fn export_surface_declarations_follow_export_equals_members_to_real_interface_decls() {
    let files = [
        (
            "/a.d.ts",
            r#"
import * as e from "express";
declare module "express" {
    interface Request {
        id: number;
    }
}
"#,
        ),
        (
            "/index.d.ts",
            r#"
declare namespace Express {
    export interface Request { }
}

declare module "express" {
    function e(): e.Express;
    namespace e {
        interface Request extends Express.Request {
            get(name: string): string;
        }
        interface Express {
            createApplication(): Application;
        }
        interface Application {}
        export = e;
    }
}
"#,
        ),
    ];

    with_checker(&files, "/a.d.ts", |checker, _a_idx, index_idx| {
        let decls = checker.export_surface_declarations_in_file(index_idx, "Request");

        assert!(
            !decls.is_empty(),
            "Expected Request to resolve through export= surface to real declarations"
        );
        assert!(
            decls
                .iter()
                .any(|(_, flags, _)| (flags & tsz_binder::symbol_flags::INTERFACE) != 0),
            "Expected export surface to include interface flags, got: {decls:#?}"
        );
    });
}

#[test]
fn module_block_scoped_conflict_detects_global_vs_module_let() {
    // Simulates typeReferenceDirectives7.ts:
    // Script file declares `let $` (global, block-scoped)
    // Module file declares `export let $` (module, block-scoped)
    // Expected: the helper finds the module file's `$` as a conflict
    let files = [
        (
            "/a.d.ts",
            // Script file (no import/export) — global `let $`
            "declare let $: { x: number }\n",
        ),
        (
            "/index.d.ts",
            // Module file (has export) — module-scoped `let $`
            "export let $ = 1;\nexport let x: typeof $;\n",
        ),
    ];

    with_checker(&files, "/a.d.ts", |checker, _a_idx, _index_idx| {
        let conflicts = checker.module_file_block_scoped_conflict_declarations_for_current_file(
            "$",
            tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE,
        );

        assert!(
            !conflicts.is_empty(),
            "Expected to find module file's `$` as a block-scoped conflict"
        );
        assert!(
            conflicts.iter().all(|(_, _, is_local, _, _)| !*is_local),
            "All conflict declarations should be remote: {conflicts:#?}"
        );
    });
}
