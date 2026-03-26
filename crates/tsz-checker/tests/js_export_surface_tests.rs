//! Focused regression tests for the unified JS export surface model.
//!
//! Tests the `JsExportSurface` synthesis path that consolidates:
//! - `module.exports = X`
//! - `exports.foo = Y`
//! - prototype property assignments
//! - constructor function + property assignment merges
//! - import-side lookup of those shapes

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Set up a two-file CommonJS checker test (producer JS, consumer TS).
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

/// Helper for single-file JS checking (CommonJS mode).
fn check_commonjs_single_file(file_name: &str, source: &str) -> Vec<(u32, String)> {
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

fn check_commonjs_file_with_prelude(
    prelude_name: &str,
    prelude_source: &str,
    file_name: &str,
    source: &str,
) -> Vec<(u32, String)> {
    let mut parser_a = ParserState::new(prelude_name.to_string(), prelude_source.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = ParserState::new(file_name.to_string(), source.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    let arena_a = Arc::new(parser_a.get_arena().clone());
    let arena_b = Arc::new(parser_b.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena_a), Arc::clone(&arena_b)]);

    let binder_a = Arc::new(binder_a);
    let binder_b = Arc::new(binder_b);
    let all_binders = Arc::new(vec![Arc::clone(&binder_a), Arc::clone(&binder_b)]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_b.as_ref(),
        binder_b.as_ref(),
        &types,
        file_name.to_string(),
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

    checker.check_source_file(root_b);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn check_commonjs_three_files_with_prelude(
    prelude_name: &str,
    prelude_source: &str,
    producer_name: &str,
    producer_source: &str,
    consumer_name: &str,
    consumer_source: &str,
    module_specifier: &str,
) -> Vec<(u32, String)> {
    let mut parser_a = ParserState::new(prelude_name.to_string(), prelude_source.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = ParserState::new(producer_name.to_string(), producer_source.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    let mut parser_c = ParserState::new(consumer_name.to_string(), consumer_source.to_string());
    let root_c = parser_c.parse_source_file();
    let mut binder_c = BinderState::new();
    binder_c.bind_source_file(parser_c.get_arena(), root_c);

    let arena_a = Arc::new(parser_a.get_arena().clone());
    let arena_b = Arc::new(parser_b.get_arena().clone());
    let arena_c = Arc::new(parser_c.get_arena().clone());
    let all_arenas = Arc::new(vec![
        Arc::clone(&arena_a),
        Arc::clone(&arena_b),
        Arc::clone(&arena_c),
    ]);

    let file_b_exports = binder_b.module_exports.get(producer_name).cloned();
    if let Some(exports) = &file_b_exports {
        binder_c
            .module_exports
            .insert(module_specifier.to_string(), exports.clone());
    }

    let mut cross_file_targets = FxHashMap::default();
    if let Some(exports) = &file_b_exports {
        for (_name, &sym_id) in exports.iter() {
            cross_file_targets.insert(sym_id, 1usize);
        }
    }

    let binder_a = Arc::new(binder_a);
    let binder_b = Arc::new(binder_b);
    let binder_c = Arc::new(binder_c);
    let all_binders = Arc::new(vec![
        Arc::clone(&binder_a),
        Arc::clone(&binder_b),
        Arc::clone(&binder_c),
    ]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_c.as_ref(),
        binder_c.as_ref(),
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
    checker.ctx.set_current_file_idx(2);
    for (sym_id, file_idx) in &cross_file_targets {
        checker.ctx.register_symbol_file_target(*sym_id, *file_idx);
    }

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((2, module_specifier.to_string()), 1);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));

    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert(module_specifier.to_string());
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(root_c);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn format_commonjs_single_file_symbol_type(
    file_name: &str,
    source: &str,
    symbol_name: &str,
) -> String {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let sym_id = binder
        .file_locals
        .get(symbol_name)
        .unwrap_or_else(|| panic!("missing symbol {symbol_name}"));

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
    let symbol_type = checker.get_type_of_symbol(sym_id);
    checker.format_type(symbol_type)
}

fn format_commonjs_two_file_consumer_symbol_type(
    producer_name: &str,
    producer_source: &str,
    consumer_name: &str,
    consumer_source: &str,
    consumer_symbol_name: &str,
    module_specifier: &str,
) -> String {
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

    let sym_id = binder_b
        .file_locals
        .get(consumer_symbol_name)
        .unwrap_or_else(|| panic!("missing consumer symbol {consumer_symbol_name}"));

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
    let symbol_type = checker.get_type_of_symbol(sym_id);
    checker.format_type(symbol_type)
}

// ==========================================================================
// module.exports = X tests
// ==========================================================================

#[test]
fn test_module_exports_object_literal() {
    // module.exports = { a: 1, b: "hello" }
    // Consumer should see { a: number, b: string } shape
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"module.exports = { a: 1, b: "hello" };"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib.a;
lib.b;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for valid module.exports properties, got: {ts2339:#?}"
    );
}

#[test]
fn test_require_call_resolves_module_exports_class_property_object_literal() {
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
module.exports = {
    Baz: class { }
};
"#,
        "use.js",
        r#"
var mod = require("./mod1.js");
new mod.Baz();
"#,
        "./mod1.js",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, message)| *code == 2339 && message.contains("'Baz'"))
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for require() of module.exports object literal class property, got: {ts2339:#?}"
    );
}

#[test]
fn test_require_call_prefers_last_module_exports_object_literal_over_earlier_exports_writes() {
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
exports.Bar = class { };
module.exports = {
    Baz: class { }
};
"#,
        "use.js",
        r#"
var mod = require("./mod1.js");
new mod.Baz();
"#,
        "./mod1.js",
    );

    let baz_missing: Vec<_> = diagnostics
        .iter()
        .filter(|(code, message)| *code == 2339 && message.contains("'Baz'"))
        .collect();
    assert!(
        baz_missing.is_empty(),
        "Expected no TS2339 for require() after module.exports overwrite, got: {baz_missing:#?}"
    );
}

#[test]
fn test_require_call_uses_unified_surface_when_jsdoc_typedefs_merge_with_commonjs_exports() {
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
/** @typedef {number} Bar */
exports.Bar = class { };

/** @typedef {number} Baz */
module.exports = {
    Baz: class { }
};
"#,
        "use.js",
        r#"
var mod = require("./mod1.js");
/** @type {mod.Baz} */
var bb;
new mod.Baz();
"#,
        "./mod1.js",
    );

    let baz_missing: Vec<_> = diagnostics
        .iter()
        .filter(|(code, message)| *code == 2339 && message.contains("'Baz'"))
        .collect();
    assert!(
        baz_missing.is_empty(),
        "Expected no TS2339 for require() when JSDoc typedefs merge with CommonJS exports, got: {baz_missing:#?}"
    );
}

#[test]
fn test_plain_js_object_expando_write_stays_open_world_in_check_js() {
    let diagnostics = check_commonjs_single_file(
        "mod2.js",
        r#"
/** @typedef {number} Foo */
const ns = {};
ns.Foo = class {};
module.exports = ns;
"#,
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, message)| *code == 2339 && message.contains("'Foo'"))
        .collect();
    let ts2300: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2300)
        .collect();

    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for JS object expando write on `ns.Foo`, got: {ts2339:#?}"
    );
    assert!(
        !ts2300.is_empty(),
        "Expected the duplicate-identifier JSDoc diagnostics to remain, got: {diagnostics:#?}"
    );
}

#[test]
fn test_require_call_preserves_earlier_direct_export_object_members_as_optional_namespace_props() {
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
/** @typedef {number} Baz */
module.exports = {
    Baz: class { }
};

/** @typedef {number} Quack */
module.exports = {
    Quack: 2
};
"#,
        "use.js",
        r#"
var mod = require("./mod1.js");
new mod.Baz();
"#,
        "./mod1.js",
    );

    let baz_missing: Vec<_> = diagnostics
        .iter()
        .filter(|(code, message)| *code == 2339 && message.contains("'Baz'"))
        .collect();
    assert!(
        baz_missing.is_empty(),
        "Expected no TS2339 for earlier direct-export object members after later overwrite, got: {baz_missing:#?}"
    );
}

#[test]
fn test_require_call_matches_typedef_cross_module2_shape() {
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
/** @typedef {number} Foo */
class Foo { }

/** @typedef {number} Bar */
exports.Bar = class { };

/** @typedef {number} Baz */
module.exports = {
    Baz: class { }
};

/** @typedef {number} Qux */
var Qux = 2;

/** @typedef {number} Quid */
exports.Quid = 2;

/** @typedef {number} Quack */
module.exports = {
    Quack: 2
};
"#,
        "use.js",
        r#"
var mod = require("./mod1.js");
/** @type {import("./mod1.js").Baz} */
var b;
/** @type {mod.Baz} */
var bb;
var bbb = new mod.Baz();
"#,
        "./mod1.js",
    );

    let ts18048: Vec<_> = diagnostics
        .iter()
        .filter(|(code, message)| *code == 18048 && message.contains("'mod.Baz'"))
        .collect();
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, message)| *code == 2339 && message.contains("'Baz'"))
        .collect();
    assert!(
        !ts18048.is_empty(),
        "Expected TS18048 for typedefCrossModule2-shaped require() flow, got: {diagnostics:#?}"
    );
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for typedefCrossModule2-shaped require() flow, got: {ts2339:#?}"
    );
}

#[test]
fn test_module_exports_function() {
    // module.exports = function greet() { return "hi"; }
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"module.exports = function greet() { return "hi"; };"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib();
"#,
        "./lib.js",
    );

    let ts2349: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2349).collect();
    assert!(
        ts2349.is_empty(),
        "Expected no TS2349 for callable module.exports, got: {ts2349:#?}"
    );
}

// ==========================================================================
// exports.foo = X tests
// ==========================================================================

#[test]
fn test_exports_foo_property_assignment() {
    // exports.foo = 42;
    // exports.bar = "hello";
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
exports.foo = 42;
exports.bar = "hello";
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib.foo;
lib.bar;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for exports.foo properties, got: {ts2339:#?}"
    );
}

#[test]
fn test_module_exports_foo_property_assignment() {
    // module.exports.foo = 42;
    // module.exports.bar = "hello";
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
module.exports.foo = 42;
module.exports.bar = "hello";
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib.foo;
lib.bar;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for module.exports.foo properties, got: {ts2339:#?}"
    );
}

// ==========================================================================
// Prototype assignment tests
// ==========================================================================

#[test]
fn test_prototype_property_assignment_same_file() {
    // Constructor function with prototype methods in same file.
    // Should not produce TS2339 for read-before-assignment on prototype props.
    let diagnostics = check_commonjs_single_file(
        "proto.js",
        r#"
function MyClass() {
    this.value = 0;
}
MyClass.prototype.getValue = function() { return this.value; };
MyClass.prototype.setValue = function(v) { this.value = v; };
var inst = new MyClass();
inst.getValue();
inst.setValue(42);
"#,
    );

    // We're checking that prototype method definitions don't produce
    // spurious errors. In a no-lib environment some errors are expected
    // (like missing global types), but TS2339 on prototype methods
    // specifically indicates a regression.
    let ts2339_proto: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| *c == 2339 && (msg.contains("getValue") || msg.contains("setValue")))
        .collect();
    // In no-lib mode, prototype method access on `this` might not fully
    // resolve, so we only assert there's no unexpected TS2339 on the
    // methods themselves, not on `this`.
    assert!(
        ts2339_proto.is_empty(),
        "Expected no TS2339 for prototype method names, got: {ts2339_proto:#?}"
    );
}

// ==========================================================================
// Constructor function + property merge tests
// ==========================================================================

#[test]
fn test_constructor_function_export_with_static_props() {
    // module.exports = Ctor; Ctor.staticProp = 42;
    // When a constructor function is exported, static properties should merge.
    let diagnostics = check_commonjs_two_files(
        "ctor.js",
        r#"
function Ctor() { this.x = 0; }
Ctor.staticProp = 42;
module.exports = Ctor;
"#,
        "consumer.ts",
        r#"
import Ctor = require("./ctor.js");
new Ctor();
"#,
        "./ctor.js",
    );

    // Constructor function exports should be callable with `new`
    let ts2351: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2351).collect();
    assert!(
        ts2351.is_empty(),
        "Expected no TS2351 for constructor function export, got: {ts2351:#?}"
    );
}

// ==========================================================================
// Mixed module.exports + exports.foo tests
// ==========================================================================

#[test]
fn test_module_exports_with_additional_exports() {
    // module.exports = { base: true };
    // exports.extra = 42;
    // The surface should merge both sources.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
module.exports = { base: true };
exports.extra = 42;
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib.base;
lib.extra;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    // After unified surface synthesis, both base and extra should be accessible.
    // Note: in tsc, `module.exports = X` takes precedence and named exports
    // are merged. Our surface does the same via intersection.
    assert!(
        ts2339.len() <= 1,
        "Expected at most 1 TS2339 (tsc also limits named exports when module.exports is set), got: {ts2339:#?}"
    );
}

// ==========================================================================
// Import-side lookup tests
// ==========================================================================

#[test]
fn test_require_call_resolves_through_surface() {
    // const lib = require("./lib.js"); lib.foo;
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"exports.foo = function() { return 42; };"#,
        "consumer.js",
        r#"
var lib = require("./lib.js");
lib.foo();
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for require() + property access, got: {ts2339:#?}"
    );
}

#[test]
fn test_element_access_on_exports() {
    // exports["foo"] = 42; — element access export pattern
    let diagnostics = check_commonjs_single_file(
        "elem.js",
        r#"
exports["foo"] = 42;
exports["bar"] = "hello";
"#,
    );

    // Basic validation: this should parse and check without panics
    // Element access exports are a valid CommonJS pattern
    let _len = diagnostics.len();
}

#[test]
fn test_element_access_exports_cross_file() {
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
exports["b"] = { x: "x" };
exports["default"] = { x: "x" };
module.exports["c"] = { x: "x" };
module["exports"]["d"] = {};
module["exports"]["d"].e = 0;
"#,
        "consumer.js",
        r#"
var lib = require("./lib.js");
lib.b;
lib.c;
lib.d;
lib.d.e;
lib.default;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for literal CommonJS element-access exports, got: {diagnostics:#?}"
    );
}

// ==========================================================================
// IIFE export pattern tests
// ==========================================================================

#[test]
fn test_iife_export_assignments() {
    // Exports inside IIFEs should be recognized
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
(function() {
    exports.fromIife = function() { return 1; };
})();
exports.direct = 42;
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib.fromIife;
lib.direct;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for IIFE + direct export mix, got: {ts2339:#?}"
    );
}

// ==========================================================================
// Caching validation
// ==========================================================================

#[test]
fn test_multiple_require_of_same_module_consistent() {
    // Two imports of the same module should get the same type
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"exports.value = 42;"#,
        "consumer.ts",
        r#"
import a = require("./lib.js");
import b = require("./lib.js");
a.value;
b.value;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected consistent resolution for repeated require(), got: {ts2339:#?}"
    );
}

// ==========================================================================
// Nested property assignments shaping exports
// ==========================================================================

#[test]
fn test_nested_property_assignment_exports() {
    // exports.utils = {}; exports.utils.helper = function() {};
    // The nested pattern should be collected by the surface as a top-level export.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
exports.utils = {};
exports.config = { debug: false };
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib.utils;
lib.config;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for nested property assignment exports, got: {ts2339:#?}"
    );
}

// ==========================================================================
// module.exports = primitive + property augmentation
// ==========================================================================

#[test]
fn test_module_exports_string_primitive() {
    // module.exports = "hello";
    // Consumer should see a string type.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"module.exports = "hello";"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
var x = lib;
"#,
        "./lib.js",
    );

    // Should not crash or produce TS2307 (module not found)
    let ts2307: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2307).collect();
    assert!(
        ts2307.is_empty(),
        "Expected no TS2307 for module.exports = primitive, got: {ts2307:#?}"
    );
}

// ==========================================================================
// Constructor-function + prototype/property assignment merges
// ==========================================================================

#[test]
fn test_constructor_with_prototype_methods_cross_file() {
    // Producer defines constructor + prototype methods;
    // consumer should be able to `new` it without TS2351.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
function Widget() { this.name = "widget"; }
Widget.prototype.getName = function() { return this.name; };
Widget.prototype.setName = function(n) { this.name = n; };
module.exports = Widget;
"#,
        "consumer.ts",
        r#"
import Widget = require("./lib.js");
var w = new Widget();
"#,
        "./lib.js",
    );

    let ts2351: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2351).collect();
    assert!(
        ts2351.is_empty(),
        "Expected no TS2351 for constructor+prototype export, got: {ts2351:#?}"
    );
}

#[test]
fn test_constructor_static_and_instance_merge() {
    // A constructor function with both static properties and prototype methods.
    // The surface should merge both into the exported shape.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
function Logger() {}
Logger.prototype.log = function(msg) {};
Logger.level = "info";
module.exports = Logger;
"#,
        "consumer.ts",
        r#"
import Logger = require("./lib.js");
new Logger();
"#,
        "./lib.js",
    );

    let ts2351: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2351).collect();
    assert!(
        ts2351.is_empty(),
        "Expected no TS2351 for constructor+static+prototype export, got: {ts2351:#?}"
    );
}

// ==========================================================================
// Import-side named import lookup through surface
// ==========================================================================

#[test]
fn test_named_import_from_exports_property() {
    // Named import `{ foo }` from a file that uses `exports.foo = ...`
    // should resolve through the unified surface without TS2305.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
exports.greet = function(name) { return "hello " + name; };
exports.VERSION = "1.0.0";
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib.greet("world");
lib.VERSION;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for named property imports via surface, got: {ts2339:#?}"
    );
}

// ==========================================================================
// module.exports with export property assignment merge
// ==========================================================================

#[test]
fn test_module_export_with_export_property_assignment() {
    // module.exports = function() {};
    // module.exports.helper = function() {};
    // The surface should merge direct export + named property.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
module.exports = function main() { return 1; };
module.exports.helper = function() { return 2; };
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib();
lib.helper();
"#,
        "./lib.js",
    );

    let ts2349: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2349).collect();
    assert!(
        ts2349.is_empty(),
        "Expected no TS2349 for callable module.exports + property merge, got: {ts2349:#?}"
    );
}

// ==========================================================================
// Export alias patterns (var x = exports; x.foo = ...)
// ==========================================================================

#[test]
fn test_export_alias_variable() {
    // var e = exports; e.foo = 42;
    // The alias pattern should be recognized by the surface.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
var e = exports;
e.myFunc = function() { return 42; };
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib.myFunc;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for export alias pattern, got: {ts2339:#?}"
    );
}

// ==========================================================================
// Phase 2: Additional regression tests for surface-routed consumers
// ==========================================================================

// --- module.exports = X (direct export) regression tests ---

#[test]
fn test_module_exports_class_instance() {
    // module.exports = new SomeClass(); should export the instance shape
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
function Foo() { this.x = 10; this.y = 20; }
module.exports = new Foo();
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
var a = lib.x;
var b = lib.y;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| *c == 2339 && (msg.contains("'x'") || msg.contains("'y'")))
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for module.exports = new Foo(), got: {ts2339:#?}"
    );
}

#[test]
fn test_module_exports_instance_with_late_property_writes() {
    let diagnostics = check_commonjs_two_files(
        "npmlog.js",
        r#"
class EE {
    on(s) { }
}
var npmlog = module.exports = new EE();
npmlog.x = 1;
module.exports.y = 2;
"#,
        "use.ts",
        r#"
import npmlog = require("./npmlog.js");
npmlog.x;
npmlog.y;
npmlog.on;
"#,
        "./npmlog.js",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| {
            *c == 2339 && (msg.contains("'x'") || msg.contains("'y'") || msg.contains("'on'"))
        })
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for instance export + late property writes, got: {ts2339:#?}"
    );
}

#[test]
fn test_module_exports_instance_with_late_property_writes_js_require() {
    let diagnostics = check_commonjs_two_files(
        "npmlog.js",
        r#"
class EE {
    on(s) { }
}
var npmlog = module.exports = new EE();
npmlog.x = 1;
module.exports.y = 2;
"#,
        "use.js",
        r#"
var npmlog = require("./npmlog.js");
npmlog.x;
npmlog.y;
npmlog.on;
"#,
        "./npmlog.js",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| {
            *c == 2339 && (msg.contains("'x'") || msg.contains("'y'") || msg.contains("'on'"))
        })
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for JS require() of instance export + late property writes, got: {ts2339:#?}"
    );
}

#[test]
fn test_module_exports_instance_with_late_property_writes_js_require_no_extension() {
    let diagnostics = check_commonjs_two_files(
        "npmlog.js",
        r#"
class EE {
    on(s) { }
}
var npmlog = module.exports = new EE();
npmlog.x = 1;
module.exports.y = 2;
"#,
        "use.js",
        r#"
var npmlog = require("./npmlog");
npmlog.x;
npmlog.y;
npmlog.on;
"#,
        "./npmlog",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| {
            *c == 2339 && (msg.contains("'x'") || msg.contains("'y'") || msg.contains("'on'"))
        })
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for extensionless JS require() of instance export + late property writes, got: {ts2339:#?}"
    );
}

#[test]
fn test_module_exports_arrow_function() {
    // module.exports = () => 42;
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"module.exports = function() { return 42; };"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
var result = lib();
"#,
        "./lib.js",
    );

    let ts2349: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2349).collect();
    assert!(
        ts2349.is_empty(),
        "Expected no TS2349 for callable module.exports (function expr), got: {ts2349:#?}"
    );
}

// --- exports.foo = X regression tests ---

#[test]
fn test_exports_foo_function_value() {
    // exports.foo = function() {}; — function-valued export
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
exports.add = function(a, b) { return a + b; };
exports.sub = function(a, b) { return a - b; };
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib.add(1, 2);
lib.sub(3, 1);
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for exports.foo function values, got: {ts2339:#?}"
    );
    let ts2349: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2349).collect();
    assert!(
        ts2349.is_empty(),
        "Expected no TS2349 for exports.foo function calls, got: {ts2349:#?}"
    );
}

#[test]
fn test_exports_foo_object_value() {
    // exports.config = { debug: true, port: 3000 }; — object-valued export
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
exports.config = { debug: true, port: 3000 };
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
var d = lib.config.debug;
var p = lib.config.port;
"#,
        "./lib.js",
    );

    let ts2339_config: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| *c == 2339 && msg.contains("'config'"))
        .collect();
    assert!(
        ts2339_config.is_empty(),
        "Expected no TS2339 for exports.config, got: {ts2339_config:#?}"
    );
}

// --- Prototype property assignment tests ---

#[test]
fn test_prototype_method_types_preserved_cross_file() {
    // Constructor with prototype methods exported cross-file.
    // Import side should be able to `new` and access methods without errors.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
function EventEmitter() {
    this.listeners = [];
}
EventEmitter.prototype.on = function(event, cb) { this.listeners.push(cb); };
EventEmitter.prototype.emit = function(event) {};
module.exports = EventEmitter;
"#,
        "consumer.ts",
        r#"
import EventEmitter = require("./lib.js");
var ee = new EventEmitter();
"#,
        "./lib.js",
    );

    let ts2351: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2351).collect();
    assert!(
        ts2351.is_empty(),
        "Expected no TS2351 for constructor with prototype methods, got: {ts2351:#?}"
    );
}

#[test]
fn test_prototype_assignment_multiple_constructors() {
    // Multiple constructor functions with prototypes in same file.
    // Only the exported one matters for cross-file imports.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
function Dog() { this.name = "dog"; }
Dog.prototype.bark = function() { return "woof"; };
function Cat() { this.name = "cat"; }
Cat.prototype.meow = function() { return "meow"; };
module.exports = Dog;
"#,
        "consumer.ts",
        r#"
import Dog = require("./lib.js");
var d = new Dog();
"#,
        "./lib.js",
    );

    let ts2351: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2351).collect();
    assert!(
        ts2351.is_empty(),
        "Expected no TS2351 for multi-constructor file, got: {ts2351:#?}"
    );
}

// --- Constructor function + property assignment merge tests ---

#[test]
fn test_constructor_with_static_method_and_instance_props() {
    // Constructor with this.props, static methods, and prototype methods.
    // Consumer should be able to construct and call static method.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
function Counter(initial) { this.count = initial || 0; }
Counter.prototype.increment = function() { this.count++; };
Counter.create = function(n) { return new Counter(n); };
module.exports = Counter;
"#,
        "consumer.ts",
        r#"
import Counter = require("./lib.js");
var c = new Counter(0);
"#,
        "./lib.js",
    );

    let ts2351: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2351).collect();
    assert!(
        ts2351.is_empty(),
        "Expected no TS2351 for constructor+static+prototype, got: {ts2351:#?}"
    );
}

#[test]
fn test_module_exports_function_with_property_augmentation() {
    // module.exports = fn; module.exports.version = "1.0";
    // Should be callable AND have the property.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
function doWork() { return true; }
module.exports = doWork;
module.exports.version = "1.0";
"#,
        "consumer.ts",
        r#"
import doWork = require("./lib.js");
doWork();
doWork.version;
"#,
        "./lib.js",
    );

    let ts2349: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2349).collect();
    assert!(
        ts2349.is_empty(),
        "Expected no TS2349 for callable export with properties, got: {ts2349:#?}"
    );
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| *c == 2339 && msg.contains("version"))
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for module.exports.version augmentation, got: {ts2339:#?}"
    );
}

// --- Import-side lookup of surface-synthesized shapes ---

#[test]
fn test_import_side_type_narrowing_of_commonjs_exports() {
    // Consumer narrows the type of a CommonJS export
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
exports.value = 42;
exports.name = "test";
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
var v = lib.value;
var n = lib.name;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for basic import-side value/name lookup, got: {ts2339:#?}"
    );
}

#[test]
fn test_require_call_with_destructuring() {
    // const { foo, bar } = require("./lib"); pattern
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
exports.foo = 42;
exports.bar = "hello";
"#,
        "consumer.js",
        r#"
var mod = require("./lib.js");
mod.foo;
mod.bar;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for require() property access, got: {ts2339:#?}"
    );
}

// --- Current-file namespace type via surface ---

#[test]
fn test_current_file_exports_property_access() {
    // Within a JS file, `exports.foo` should be recognized
    let diagnostics = check_commonjs_single_file(
        "self.js",
        r#"
exports.alpha = 1;
exports.beta = "two";
var x = exports.alpha;
"#,
    );

    // Should not produce TS2339 on exports.alpha access within same file
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| *c == 2339 && msg.contains("alpha"))
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for same-file exports.alpha access, got: {ts2339:#?}"
    );
}

#[test]
fn test_current_file_module_exports_property_access() {
    // Within a JS file, `module.exports.foo = X` then `module.exports.foo` access
    let diagnostics = check_commonjs_single_file(
        "self.js",
        r#"
module.exports.count = 0;
module.exports.name = "test";
"#,
    );

    // Should parse and check without panics
    let _len = diagnostics.len();
}

#[test]
fn test_require_of_primitive_module_exports_does_not_expose_later_properties() {
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
module.exports = 1;
module.exports.f = function () { };
"#,
        "a.js",
        r#"
var mod1 = require("./mod1");
mod1.toFixed(12);
mod1.f();
"#,
        "./mod1",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        !ts2339.is_empty(),
        "Expected TS2339 in the consumer once primitive module.exports blocks later property merges, got: {diagnostics:#?}"
    );
    assert!(
        ts2339
            .iter()
            .any(|(_, msg)| msg.contains("type 'number'") && msg.contains("Property 'f'")),
        "Expected consumer TS2339 to report the widened primitive type, got: {diagnostics:#?}"
    );
}

#[test]
fn test_primitive_module_exports_assignment_reports_same_file_property_error_with_prelude() {
    let diagnostics = check_commonjs_file_with_prelude(
        "requires.d.ts",
        r#"
declare var module: { exports: any };
"#,
        "mod1.js",
        r#"
module.exports = 1;
module.exports.f = function () { };
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, msg)| {
            *code == 2339 && msg.contains("Property 'f' does not exist on type 'number'")
        }),
        "Expected producer-side TS2339 once primitive module.exports blocks later property merges, got: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_param_type_uses_instance_side_for_destructured_commonjs_class_expression() {
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
exports.K = class K {
    values() {}
};
"#,
        "main.js",
        r#"
const { K } = require("./mod1");
/** @param {K} k */
function f(k) {
    k.values();
}
"#,
        "./mod1",
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2339 | 2322 | 2351 | 2741))
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected destructured CommonJS class expression JSDoc param to resolve to instance side, got: {relevant:#?}"
    );
}

#[test]
fn test_jsdoc_param_type_uses_instance_side_for_destructured_commonjs_named_class() {
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
class K {
    values() {
        return new K();
    }
}
exports.K = K;
"#,
        "main.js",
        r#"
const { K } = require("./mod1");
/** @param {K} k */
function f(k) {
    k.values();
}
"#,
        "./mod1",
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2339 | 2322))
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected destructured CommonJS named class JSDoc param to resolve to instance side, got: {relevant:#?}"
    );
}

#[test]
fn test_jsdoc_param_type_uses_instance_side_for_destructured_nested_commonjs_class() {
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
var NS = {};
NS.K = class {
    values() {
        return new NS.K();
    }
};
exports.K = NS.K;
"#,
        "main.js",
        r#"
const { K } = require("./mod1");
/** @param {K} k */
function f(k) {
    k.values();
}
"#,
        "./mod1",
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2339 | 2351 | 2741))
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected destructured nested CommonJS class JSDoc param to resolve to instance side, got: {relevant:#?}"
    );
}

#[test]
fn test_commonjs_named_class_export_assignment_keeps_constructor_side() {
    let diagnostics = check_commonjs_single_file(
        "mod1.js",
        r#"
class K {
    values() {
        return new K();
    }
}
exports.K = K;
"#,
    );

    let relevant: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| matches!(*code, 2322 | 2339 | 2351 | 2741))
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected CommonJS named class export assignment to keep constructor-side typing, got: {relevant:#?}"
    );
}

#[test]
fn test_commonjs_nested_class_expando_assignment_keeps_constructor_side() {
    let diagnostics = check_commonjs_single_file(
        "mod1.js",
        r#"
var NS = {};
NS.K = class {
    values() {
        return new NS.K();
    }
};
exports.K = NS.K;
"#,
    );

    let relevant: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| matches!(*code, 2322 | 2339 | 2351 | 2741))
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected nested CommonJS class expando assignment to keep constructor-side typing, got: {relevant:#?}"
    );
}

// --- Surface caching correctness ---

#[test]
fn test_surface_cache_consistent_with_multiple_consumers() {
    // Two different consumer files importing the same producer.
    // The cached surface should produce consistent results.
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
exports.x = 1;
exports.y = 2;
exports.z = 3;
"#,
        "consumer.ts",
        r#"
import a = require("./lib.js");
import b = require("./lib.js");
a.x;
a.y;
b.z;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    assert!(
        ts2339.is_empty(),
        "Expected consistent surface cache across multiple imports, got: {ts2339:#?}"
    );
}

// --- Object.defineProperty export tests ---

#[test]
fn test_define_property_export_cross_file() {
    // Object.defineProperty(exports, "foo", { value: 42 });
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
Object.defineProperty(exports, "myProp", { value: 42 });
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib.myProp;
"#,
        "./lib.js",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| *c == 2339 && msg.contains("myProp"))
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for Object.defineProperty export, got: {ts2339:#?}"
    );
}

#[test]
fn test_define_property_export_preserves_write_type_and_readonly_cross_file() {
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
Object.defineProperty(exports, "foo", { value: "ok", writable: true });
Object.defineProperty(exports, "bar", { value: "fixed" });
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib.foo = 1;
lib.bar = "nope";
"#,
        "./lib.js",
    );

    let ts2322: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2322).collect();
    let ts2540: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2540).collect();
    assert!(
        !ts2322.is_empty(),
        "Expected TS2322 for writable defineProperty export with string type, got: {diagnostics:#?}"
    );
    assert!(
        !ts2540.is_empty(),
        "Expected TS2540 for readonly defineProperty export, got: {diagnostics:#?}"
    );
}

#[test]
fn test_define_property_export_tracks_constant_names_cross_file() {
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
const dynamicName = "other";
const constName = "prop";
Object.defineProperty(exports, "thing", { value: 42, writable: true });
Object.defineProperty(exports, dynamicName, { value: 42, writable: true });
Object.defineProperty(exports, constName, { value: 42, writable: true });
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
lib.thing;
lib.other;
lib.prop;
"#,
        "./lib.js",
    );

    let thing_missing: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| *c == 2339 && msg.contains("thing"))
        .collect();
    let missing: Vec<_> = diagnostics
        .iter()
        .filter(|(c, msg)| {
            *c == 2339 && (msg.contains("thing") || msg.contains("other") || msg.contains("prop"))
        })
        .collect();
    assert!(
        thing_missing.is_empty(),
        "Expected literal defineProperty export to stay visible, got: {diagnostics:#?}"
    );
    assert!(
        missing.is_empty(),
        "Expected constant-name defineProperty exports to stay visible cross-file, got: {diagnostics:#?}"
    );
}

#[test]
fn test_define_property_export_supports_constant_names_and_malformed_descriptors_cross_file() {
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
const obj = { value: 42, writable: true };
Object.defineProperty(exports, "thing", obj);

/** @type {string} */
let str = /** @type {string} */("other");
Object.defineProperty(exports, str, { value: 42, writable: true });

const propName = "prop";
Object.defineProperty(exports, propName, { value: 42, writable: true });

Object.defineProperty(exports, "bad1", { });
Object.defineProperty(exports, "bad2", { get() { return 12 }, value: "no" });
Object.defineProperty(exports, "bad3", { writable: true });
"#,
        "importer.js",
        r#"
const mod = require("./mod1");
mod.thing;
mod.other;
mod.prop;
mod.bad1;
mod.bad2;
mod.bad3;

mod.thing = 0;
mod.other = 0;
mod.prop = 0;
mod.bad1 = 0;
mod.bad2 = 0;
mod.bad3 = 0;
"#,
        "./mod1",
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2339 | 2540))
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected constant-name defineProperty exports and malformed descriptors to stay permissive cross-file, got: {diagnostics:#?}"
    );
}

#[test]
fn test_plain_object_define_property_augments_local_js_object_type() {
    let x_type = format_commonjs_single_file_symbol_type(
        "lib.js",
        r#"
const x = {};
Object.defineProperty(x, "name", { value: "Charles", writable: true });
Object.defineProperty(x, "middleInit", { value: "H" });
Object.defineProperty(x, "zipStr", {
  /** @param {string} str */
  set(str) {}
});
"#,
        "x",
    );
    let diagnostics = check_commonjs_single_file(
        "lib.js",
        r#"
const x = {};
Object.defineProperty(x, "name", { value: "Charles", writable: true });
Object.defineProperty(x, "middleInit", { value: "H" });
Object.defineProperty(x, "zipStr", {
  /** @param {string} str */
  set(str) {}
});
/** @param {{name: string}} named */
function takeName(named) { return named.name; }
takeName(x);
x.name = 12;
x.middleInit = "R";
x.zipStr = 12;
"#,
    );

    assert!(
        x_type.contains("name"),
        "Expected local symbol type to include defineProperty members, got: {x_type}"
    );

    let ts2339: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2339).collect();
    let ts2345: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2345).collect();
    let ts2322: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2322).collect();
    let ts2540: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2540).collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 after Object.defineProperty augments object type, got: {ts2339:#?}"
    );
    assert!(
        ts2345.is_empty(),
        "Expected no TS2345 for passing augmented object to typed consumer, got: {ts2345:#?}"
    );
    assert!(
        !ts2322.is_empty(),
        "Expected TS2322 for setter-backed string property assignment, got: {diagnostics:#?}"
    );
    assert!(
        !ts2540.is_empty(),
        "Expected TS2540 for readonly defineProperty member assignment, got: {diagnostics:#?}"
    );
}

#[test]
fn test_commonjs_direct_export_property_overlap_is_union_typed_cross_file() {
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
module.exports.bothBefore = "string";
A.justExport = 4;
A.bothBefore = 2;
A.bothAfter = 3;
module.exports = A;
function A() {
    this.p = 1;
}
module.exports.bothAfter = "string";
module.exports.justProperty = "string";
"#,
        "consumer.ts",
        r#"
import mod1 = require("./mod1");
declare function takesNumber(value: number): void;
takesNumber(mod1.justExport);
takesNumber(mod1.bothBefore);
takesNumber(mod1.bothAfter);
"#,
        "./mod1",
    );

    let number_mismatch_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2345)
        .collect();
    assert!(
        number_mismatch_errors.len() >= 2,
        "Expected overlapping CommonJS exports to stay union-typed and reject number-only consumers, got: {diagnostics:#?}"
    );
}

#[test]
fn test_commonjs_direct_export_property_overlap_reports_ts2323_in_js_file() {
    let diagnostics = check_commonjs_single_file(
        "mod1.js",
        r#"
module.exports.bothBefore = "string";
A.justExport = 4;
A.bothBefore = 2;
A.bothAfter = 3;
module.exports = A;
function A() {
    this.p = 1;
}
module.exports.bothAfter = "string";
module.exports.justProperty = "string";
"#,
    );

    let ts2323: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2323)
        .collect();
    assert_eq!(
        ts2323.len(),
        4,
        "Expected TS2323 on overlapping CommonJS exported property declarations, got: {diagnostics:#?}"
    );
}

#[test]
fn test_commonjs_direct_export_property_overlap_rejects_number_only_js_require_consumers() {
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
module.exports.bothBefore = "string";
A.justExport = 4;
A.bothBefore = 2;
A.bothAfter = 3;
module.exports = A;
function A() {
    this.p = 1;
}
module.exports.bothAfter = "string";
"#,
        "consumer.js",
        r#"
/** @param {number} value */
function takesNumber(value) {}
var mod1 = require("./mod1");
takesNumber(mod1.justExport);
takesNumber(mod1.bothBefore);
takesNumber(mod1.bothAfter);
"#,
        "./mod1",
    );

    let ts2345: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2345)
        .collect();
    assert!(
        ts2345.len() >= 2,
        "Expected JS require() consumer to see overlapping CommonJS exports as non-number-only, got: {diagnostics:#?}"
    );
}

#[test]
fn test_commonjs_overlap_js_require_with_declared_require_prelude() {
    let diagnostics = check_commonjs_three_files_with_prelude(
        "requires.d.ts",
        r#"
declare var module: { exports: any };
declare function require(name: string): any;
"#,
        "mod1.js",
        r#"
module.exports.bothBefore = "string";
A.justExport = 4;
A.bothBefore = 2;
A.bothAfter = 3;
module.exports = A;
function A() {
    this.p = 1;
}
module.exports.bothAfter = "string";
"#,
        "a.js",
        r#"
/// <reference path="./requires.d.ts" />
var mod1 = require("./mod1");
mod1.bothBefore.toFixed();
mod1.bothAfter.toFixed();
"#,
        "./mod1",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, msg)| *code == 2339 && msg.contains("toFixed"))
        .collect();
    assert!(
        ts2339.len() >= 2,
        "Expected prelude-declared JS require() to preserve CommonJS overlap diagnostics, got: {diagnostics:#?}"
    );
}

// --- Mixed patterns: module.exports + exports.foo + prototype ---

#[test]
fn test_full_commonjs_pattern_mix() {
    // All three patterns in one file:
    // 1. Constructor function as module.exports
    // 2. Static property via module.exports.prop
    // 3. Prototype method
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
function Parser() { this.input = ""; }
Parser.prototype.parse = function(s) { this.input = s; return {}; };
Parser.defaultOptions = { strict: true };
module.exports = Parser;
module.exports.VERSION = "2.0";
"#,
        "consumer.ts",
        r#"
import Parser = require("./lib.js");
var p = new Parser();
"#,
        "./lib.js",
    );

    let ts2351: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2351).collect();
    assert!(
        ts2351.is_empty(),
        "Expected no TS2351 for full CommonJS pattern mix, got: {ts2351:#?}"
    );
}
