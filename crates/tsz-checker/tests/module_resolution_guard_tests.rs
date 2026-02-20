//! Guard tests for module-resolution/binder regressions taken from TS conformance cases.

use crate::context::CheckerOptions;
use crate::state::CheckerState;
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
