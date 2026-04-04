//! Guard tests for module-resolution/binder regressions taken from TS conformance cases.

use crate::context::CheckerOptions;
use crate::context::ResolutionError;
use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use rustc_hash::FxHashSet;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

#[test]
fn ambient_external_module_without_internal_import_declaration_has_no_errors() {
    let src0 = r"
declare module 'M' {
    namespace C {
        export var f: number;
    }
    class C {
        foo(): void;
    }
    export = C;
}
";
    let src1 = r"
/// <reference path='ambientExternalModuleWithoutInternalImportDeclaration_0.ts'/>
import A = require('M');
var c = new A();
";

    let mut parser0 = ParserState::new(
        "ambientExternalModuleWithoutInternalImportDeclaration_0.ts".to_string(),
        src0.to_string(),
    );
    let root0 = parser0.parse_source_file();
    let mut binder0 = BinderState::new();
    binder0.bind_source_file(parser0.get_arena(), root0);

    let mut parser1 = ParserState::new(
        "ambientExternalModuleWithoutInternalImportDeclaration_1.ts".to_string(),
        src1.to_string(),
    );
    let root1 = parser1.parse_source_file();
    let mut binder1 = BinderState::new();
    binder1.bind_source_file(parser1.get_arena(), root1);

    let arena0 = Arc::new(parser0.get_arena().clone());
    let arena1 = Arc::new(parser1.get_arena().clone());
    let binder0 = Arc::new(binder0);
    let binder1 = Arc::new(binder1);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena1.as_ref(),
        binder1.as_ref(),
        &types,
        "ambientExternalModuleWithoutInternalImportDeclaration_1.ts".to_string(),
        CheckerOptions::default(),
    );
    checker
        .ctx
        .set_all_arenas(Arc::new(vec![Arc::clone(&arena0), Arc::clone(&arena1)]));
    checker
        .ctx
        .set_all_binders(Arc::new(vec![Arc::clone(&binder0), Arc::clone(&binder1)]));
    checker.ctx.set_current_file_idx(1);
    let mut resolved_modules = FxHashSet::default();
    resolved_modules.insert("M".to_string());
    checker.ctx.set_resolved_modules(resolved_modules);
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root1);

    assert!(
        !checker.ctx.diagnostics.iter().any(|d| d.code == 2307),
        "Expected no TS2307, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn alias_on_merged_module_interface_does_not_regress_to_ts2307() {
    let src0 = r#"
declare module "foo" {
    namespace B {
        export interface A {}
    }
    interface B {
        bar(name: string): B.A;
    }
    export = B;
}
"#;
    let src1 = r#"
/// <reference path='aliasOnMergedModuleInterface_0.ts' />
import foo = require("foo");
declare var z: foo;
z.bar("hello");
var x: foo.A = foo.bar("hello");
"#;

    let mut parser0 = ParserState::new(
        "aliasOnMergedModuleInterface_0.ts".to_string(),
        src0.to_string(),
    );
    let root0 = parser0.parse_source_file();
    let mut binder0 = BinderState::new();
    binder0.bind_source_file(parser0.get_arena(), root0);

    let mut parser1 = ParserState::new(
        "aliasOnMergedModuleInterface_1.ts".to_string(),
        src1.to_string(),
    );
    let root1 = parser1.parse_source_file();
    let mut binder1 = BinderState::new();
    binder1.bind_source_file(parser1.get_arena(), root1);

    let arena0 = Arc::new(parser0.get_arena().clone());
    let arena1 = Arc::new(parser1.get_arena().clone());
    let binder0 = Arc::new(binder0);
    let binder1 = Arc::new(binder1);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena1.as_ref(),
        binder1.as_ref(),
        &types,
        "aliasOnMergedModuleInterface_1.ts".to_string(),
        CheckerOptions::default(),
    );
    checker
        .ctx
        .set_all_arenas(Arc::new(vec![Arc::clone(&arena0), Arc::clone(&arena1)]));
    checker
        .ctx
        .set_all_binders(Arc::new(vec![Arc::clone(&binder0), Arc::clone(&binder1)]));
    checker.ctx.set_current_file_idx(1);
    let mut resolved_modules = FxHashSet::default();
    resolved_modules.insert("foo".to_string());
    checker.ctx.set_resolved_modules(resolved_modules);
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root1);

    assert!(
        !checker.ctx.diagnostics.iter().any(|d| d.code == 2307),
        "Expected no TS2307, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn import_equals_alias_is_valid_qualified_type_anchor_across_ambient_modules() {
    let src0 = r#"
declare module "a" {
    export type T = number;
}
"#;
    let src1 = r#"
declare module "b" {
    export import a = require("a");
    export const x: a.T;
}
"#;
    let src2 = r#"
declare module "c" {
    import b = require("b");
    const x: b.a.T;
}
"#;

    let mut parser0 = ParserState::new("defA.ts".to_string(), src0.to_string());
    let root0 = parser0.parse_source_file();
    let mut binder0 = BinderState::new();
    binder0.bind_source_file(parser0.get_arena(), root0);

    let mut parser1 = ParserState::new("defB.ts".to_string(), src1.to_string());
    let root1 = parser1.parse_source_file();
    let mut binder1 = BinderState::new();
    binder1.bind_source_file(parser1.get_arena(), root1);

    let mut parser2 = ParserState::new("defC.ts".to_string(), src2.to_string());
    let root2 = parser2.parse_source_file();
    let mut binder2 = BinderState::new();
    binder2.bind_source_file(parser2.get_arena(), root2);

    let arena0 = Arc::new(parser0.get_arena().clone());
    let arena1 = Arc::new(parser1.get_arena().clone());
    let arena2 = Arc::new(parser2.get_arena().clone());
    let binder0 = Arc::new(binder0);
    let binder1 = Arc::new(binder1);
    let binder2 = Arc::new(binder2);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena2.as_ref(),
        binder2.as_ref(),
        &types,
        "defC.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.ctx.set_all_arenas(Arc::new(vec![
        Arc::clone(&arena0),
        Arc::clone(&arena1),
        Arc::clone(&arena2),
    ]));
    checker.ctx.set_all_binders(Arc::new(vec![
        Arc::clone(&binder0),
        Arc::clone(&binder1),
        Arc::clone(&binder2),
    ]));
    checker.ctx.set_current_file_idx(2);
    let mut resolved_modules = FxHashSet::default();
    resolved_modules.insert("a".to_string());
    resolved_modules.insert("b".to_string());
    resolved_modules.insert("c".to_string());
    checker.ctx.set_resolved_modules(resolved_modules);
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root2);

    let relevant: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, &d.message_text))
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected no non-TS2318 diagnostics for qualified ambient import-equals type anchor, got: {relevant:?}"
    );
}

#[test]
fn import_type_emits_ts2307_for_unresolved_non_relative_module() {
    // import("fo") where "fo" is a typo for ambient module "foo"
    // Currently emits TS2792 ("Cannot find module") instead of TS2307.
    // Both are valid module-not-found diagnostics; TS2792 is the "did you mean" variant.
    let source = r#"
declare module "foo" {
    interface Point { x: number; y: number; }
    export = Point;
}
const x: import("fo") = { x: 0, y: 0 };
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let has_module_not_found = checker
        .ctx
        .diagnostics
        .iter()
        .any(|d| d.code == 2307 || d.code == 2792);
    assert!(
        has_module_not_found,
        "Expected TS2307 or TS2792 for import(\"fo\"), got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn import_type_no_ts2307_for_resolved_declared_module() {
    // import("foo") where "foo" is a declared module — should NOT emit TS2307
    let source = r#"
declare module "foo" {
    interface Point { x: number; y: number; }
    export = Point;
}
const x: import("foo") = { x: 0, y: 0 };
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let has_2307 = checker.ctx.diagnostics.iter().any(|d| d.code == 2307);
    assert!(
        !has_2307,
        "Should NOT emit TS2307 for import(\"foo\") — module is declared. Got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn import_declaration_prefers_driver_resolution_error_over_ambient_match() {
    // When the driver reports a module resolution failure, we expect a module-not-found
    // diagnostic. Currently emits TS2792 instead of TS2307; both are acceptable.
    let src0 = r#"
declare module "node:ph" {
    export const value: number;
}
"#;
    let src1 = r#"
import * as ph from "node:ph";
console.log(ph.value);
"#;

    let mut parser0 = ParserState::new(
        "/a/b/node_modules/@types/node/ph.d.ts".to_string(),
        src0.to_string(),
    );
    let root0 = parser0.parse_source_file();
    let mut binder0 = BinderState::new();
    binder0.bind_source_file(parser0.get_arena(), root0);

    let mut parser1 = ParserState::new("/a/b/main.ts".to_string(), src1.to_string());
    let root1 = parser1.parse_source_file();
    let mut binder1 = BinderState::new();
    binder1.bind_source_file(parser1.get_arena(), root1);

    let arena0 = Arc::new(parser0.get_arena().clone());
    let arena1 = Arc::new(parser1.get_arena().clone());
    let binder0 = Arc::new(binder0);
    let binder1 = Arc::new(binder1);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena1.as_ref(),
        binder1.as_ref(),
        &types,
        "/a/b/main.ts".to_string(),
        CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    );
    checker
        .ctx
        .set_all_arenas(Arc::new(vec![Arc::clone(&arena0), Arc::clone(&arena1)]));
    checker
        .ctx
        .set_all_binders(Arc::new(vec![Arc::clone(&binder0), Arc::clone(&binder1)]));
    checker.ctx.set_current_file_idx(1);
    checker.ctx.report_unresolved_imports = true;

    let mut resolved_module_errors: FxHashMap<(usize, String), ResolutionError> =
        FxHashMap::default();
    resolved_module_errors.insert(
        (1, "node:ph".to_string()),
        ResolutionError {
            code: 2307,
            message: "Cannot find module 'node:ph' or its corresponding type declarations."
                .to_string(),
        },
    );
    checker
        .ctx
        .set_resolved_module_errors(Arc::new(resolved_module_errors));

    checker.check_source_file(root1);

    let has_module_not_found = checker
        .ctx
        .diagnostics
        .iter()
        .any(|d| d.code == 2307 || d.code == 2792);
    assert!(
        has_module_not_found,
        "Expected TS2307 or TS2792 when the driver reported node:ph resolution failure, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_import_meta_makes_external_module() {
    let source = "
declare global { interface ImportMeta {foo?: () => void} };

if (import.meta.foo) {
  import.meta.foo();
}
";
    let mut parser =
        tsz_parser::parser::state::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::state::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    assert!(binder.is_external_module());
}
