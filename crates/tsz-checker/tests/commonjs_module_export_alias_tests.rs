use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn check_commonjs_file(file_name: &str, source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            no_lib: true,
            module: tsz_common::common::ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

/// Helper to set up a two-file CommonJS checker test.
/// Returns diagnostics from checking the consumer file.
fn check_commonjs_two_files(
    producer_name: &str,
    producer_source: &str,
    consumer_name: &str,
    consumer_source: &str,
    module_specifier: &str,
) -> Vec<(u32, String)> {
    let mut parser_a = ParserState::new(producer_name.to_string(), producer_source.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = ParserState::new(consumer_name.to_string(), consumer_source.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    let arena_a = Arc::new(parser_a.get_arena().clone());
    let arena_b = Arc::new(parser_b.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena_a), Arc::clone(&arena_b)]);

    let file_a_exports = binder_a.module_exports.get(producer_name).cloned();
    if let Some(exports) = &file_a_exports {
        binder_b
            .module_exports
            .insert(module_specifier.to_string(), exports.clone());
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
        consumer_name.to_string(),
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: false,
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
    resolved_module_paths.insert((1, module_specifier.to_string()), 0);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));

    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert(module_specifier.to_string());
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(root_b);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn test_exports_alias_property_assignment() {
    // var exportsAlias = exports; exportsAlias.func1 = function() {};
    let diagnostics = check_commonjs_two_files(
        "b.js",
        r#"
var exportsAlias = exports;
exportsAlias.func1 = function () { };
exports.func2 = function () { };
"#,
        "a.ts",
        r#"
import b = require("./b.js");
b.func1;
b.func2;
"#,
        "./b.js",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for exports alias property access, got: {ts2339:#?}"
    );
}

#[test]
fn test_module_exports_alias_property_assignment() {
    // var moduleExportsAlias = module.exports; moduleExportsAlias.func3 = function() {};
    let diagnostics = check_commonjs_two_files(
        "b.js",
        r#"
var moduleExportsAlias = module.exports;
moduleExportsAlias.func3 = function () { };
module.exports.func4 = function () { };
"#,
        "a.ts",
        r#"
import b = require("./b.js");
b.func3;
b.func4;
"#,
        "./b.js",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for module.exports alias property access, got: {ts2339:#?}"
    );
}

#[test]
fn test_chain_assignment_alias() {
    // var multipleDeclarationAlias1 = exports = module.exports;
    let diagnostics = check_commonjs_two_files(
        "b.js",
        r#"
var alias1 = exports = module.exports;
alias1.func5 = function () { };
var alias2 = module.exports = exports;
alias2.func6 = function () { };
"#,
        "a.ts",
        r#"
import b = require("./b.js");
b.func5;
b.func6;
"#,
        "./b.js",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for chain assignment alias property access, got: {ts2339:#?}"
    );
}

#[test]
fn test_chain_with_intermediate_variable() {
    let diagnostics = check_commonjs_two_files(
        "b.js",
        r#"
var someOtherVariable;
var alias3 = someOtherVariable = exports;
alias3.func7 = function () { };
var alias4 = someOtherVariable = module.exports;
alias4.func8 = function () { };
"#,
        "a.ts",
        r#"
import b = require("./b.js");
b.func7;
b.func8;
"#,
        "./b.js",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for chain with intermediate variable, got: {ts2339:#?}"
    );
}

#[test]
fn test_module_exports_equals_empty_then_alias_property() {
    let diagnostics = check_commonjs_two_files(
        "b.js",
        r#"
var alias5 = module.exports = exports = {};
alias5.func9 = function () { };
var alias6 = exports = module.exports = {};
alias6.func10 = function () { };
"#,
        "a.ts",
        r#"
import b = require("./b.js");
b.func9;
b.func10;
"#,
        "./b.js",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for module.exports = {{}} alias pattern, got: {ts2339:#?}"
    );
}

#[test]
fn test_exports_reassignment_then_property_assignment() {
    let diagnostics = check_commonjs_two_files(
        "b.js",
        r#"
exports = module.exports = {};
exports.func11 = function () { };
module.exports.func12 = function () { };
"#,
        "a.ts",
        r#"
import b = require("./b.js");
b.func11;
b.func12;
"#,
        "./b.js",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for exports reassignment + property assignment, got: {ts2339:#?}"
    );
}

#[test]
fn test_module_exports_equals_empty_then_direct_property() {
    let diagnostics = check_commonjs_two_files(
        "b.js",
        r#"
module.exports = {};
exports.func19 = function () { };
module.exports.func20 = function () { };
"#,
        "a.ts",
        r#"
import b = require("./b.js");
b.func19;
b.func20;
"#,
        "./b.js",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for module.exports = {{}} + direct property, got: {ts2339:#?}"
    );
}

#[test]
fn test_module_exports_function_expando_assignments_no_ts2339() {
    let diagnostics = check_commonjs_file(
        "index.js",
        r#"
module.exports.b = function b() {};
module.exports.b.cat = "cat";

module.exports.c = function c() {};
module.exports.c.Cls = class {};

module.exports.f = function f(a) {
    return a;
};
module.exports.f.self = module.exports.f;
"#,
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    let ts2565: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2565)
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for CommonJS exported function expandos, got: {ts2339:#?}"
    );
    assert!(
        ts2565.is_empty(),
        "Expected no TS2565 for already-assigned CommonJS export reads, got: {ts2565:#?}"
    );
}

#[test]
fn test_module_exports_nested_class_property_preserves_instance_member_types() {
    let diagnostics = check_commonjs_two_files(
        "b.js",
        r#"
module.exports.c = function c() {};
module.exports.c.Cls = class {
    constructor() {
        this.x = 1;
    }
};
"#,
        "a.ts",
        r#"
import b = require("./b.js");
const inst = new b.c.Cls();
const s: string = inst.x;
"#,
        "./b.js",
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected nested CommonJS class property instance members to stay visible, got: {diagnostics:#?}"
    );
    assert!(
        !ts2322.is_empty(),
        "Expected nested CommonJS class property instance member to keep number type, got: {diagnostics:#?}"
    );
}

#[test]
fn test_module_exports_forward_read_reports_ts2565() {
    let diagnostics = check_commonjs_file(
        "index.js",
        r#"
module.exports.jj = module.exports.j;
module.exports.j = function j() {};
"#,
    );

    let ts2565: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2565)
        .collect();
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2565.len(),
        1,
        "Expected one TS2565 for forward CommonJS export read, got: {diagnostics:#?}"
    );
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for forward CommonJS export read, got: {ts2339:#?}"
    );
}
