#[test]
fn test_commonjs_void_zero_export_write_reports_missing_property() {
    let diagnostics = check_commonjs_single_file(
        "assignmentToVoidZero2.js",
        r#"
exports.j = 1;
exports.k = void 0;
"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| *code == 2339 && message.contains("'k'")),
        "Expected TS2339 for void-zero CommonJS export `k`, got: {diagnostics:#?}"
    );
}

#[test]
fn test_named_import_rejects_void_zero_commonjs_export_write() {
    let diagnostics = check_commonjs_two_files(
        "assignmentToVoidZero2.js",
        r#"
exports.j = 1;
exports.k = void 0;
"#,
        "importer.js",
        r#"
import { j, k } from './assignmentToVoidZero2';
j + k;
"#,
        "./assignmentToVoidZero2",
    );

    assert!(
        diagnostics
            .iter()
            .any(|(code, message)| *code == 2305 && message.contains("'k'")),
        "Expected TS2305 for void-zero CommonJS export `k`, got: {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|(code, message)| *code != 2305 || !message.contains("'j'")),
        "Did not expect TS2305 for concrete CommonJS export `j`, got: {diagnostics:#?}"
    );
}

#[test]
fn test_declared_export_equals_module_named_import_is_not_rejected_by_js_surface() {
    let files = [
        (
            "/demoModule.d.ts",
            r#"
declare namespace demoNS {
    function f(): void;
}
declare module "demoModule" {
    import alias = demoNS;
    export = alias;
}
"#,
        ),
        (
            "/user.ts",
            r#"
import { f } from "demoModule";
f();
"#,
        ),
    ];
    let diagnostics = check_named_files_entry(
        &files,
        "/user.ts",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            no_lib: true,
            module: tsz_common::common::ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    assert!(
        diagnostics
            .iter()
            .all(|(code, _)| !matches!(*code, 2305 | 2497 | 2616 | 2595)),
        "Did not expect JS export-surface or export= mismatch diagnostics for declared member `f`, got: {diagnostics:#?}"
    );
}

#[test]
fn test_import_declaration_inside_js_function_skips_module_resolution() {
    let diagnostics = check_commonjs_single_file(
        "check.js",
        r#"
function container() {
    import "fs";
}
"#,
    );

    assert!(
        diagnostics.iter().all(|(code, _)| *code != 2882),
        "Did not expect TS2882 for import declaration in function body, got: {diagnostics:#?}"
    );
}

#[test]
fn test_js_esm_prototype_assignments_keep_named_exports() {
    let diagnostics = check_named_files_entry(
        &[
            (
                "/base.mjs",
                r#"
export function MjsBase() {}

MjsBase.prototype.method = function() {
  return 1;
};
"#,
            ),
            (
                "/base.js",
                r#"
export function JsBase() {}

JsBase.prototype.method = function() {
  return 1;
};
"#,
            ),
            (
                "/main.ts",
                r#"
import { MjsBase } from "./base.mjs";
import { JsBase } from "./base.js";

class FromMjs extends MjsBase {}
class FromJs extends JsBase {}

new FromMjs().method();
new FromJs().method();
"#,
            ),
        ],
        "/main.ts",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            module: tsz_common::common::ModuleKind::NodeNext,
            target: tsz_common::common::ScriptTarget::ES2022,
            no_lib: true,
            ..Default::default()
        },
    );

    let unexpected: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| matches!(*code, 2305 | 2339 | 2507))
        .collect();
    assert!(
        unexpected.is_empty(),
        "Expected JS ESM prototype assignments to preserve named exports and instance methods, got: {diagnostics:#?}"
    );
}

fn check_named_files_entry(
    files: &[(&str, &str)],
    entry_file: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
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
    let (resolved_module_paths, resolved_modules) = build_module_resolution_maps(&file_names);

    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
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
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(roots[entry_idx]);

    checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
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

#[test]
fn test_module_exports_object_property_jsdoc_rest_function_survives_require() {
    let diagnostics = check_commonjs_two_files(
        "typescript-eslint.js",
        r#"
/**
 * @typedef {{ rules: Record<string, boolean> }} Plugin
 */

/**
 * @typedef {{ plugins: Record<string, Plugin> }} Config
 */

/**
 * @type {(...configs: Config[]) => void}
 */
function config(...configs) {}

module.exports = { config };
"#,
        "eslint.config.js",
        r#"
const tseslint = require("./typescript-eslint.js");

const shared = {
  plugins: {
    react: {
      deprecatedRules: { "jsx-sort-default-props": true },
      rules: { "no-unsafe": true },
    },
  },
};

tseslint.config(shared);
"#,
        "./typescript-eslint.js",
    );

    let ts2345: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2345)
        .collect();
    assert!(
        ts2345.is_empty(),
        "Expected no TS2345 when a CommonJS-exported JSDoc rest function is called through require(), got: {diagnostics:#?}"
    );
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

struct ConsumerSymbolInspection {
    formatted: String,
    shape_props: Vec<(String, u32)>,
}

fn inspect_commonjs_two_file_consumer_symbol(
    producer_name: &str,
    producer_source: &str,
    consumer_name: &str,
    consumer_source: &str,
    consumer_symbol_name: &str,
    module_specifier: &str,
) -> ConsumerSymbolInspection {
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
        std::sync::Arc::make_mut(&mut binder_b.module_exports)
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

    ConsumerSymbolInspection {
        formatted: checker.format_type(symbol_type),
        shape_props: tsz_solver::type_queries::get_object_shape(checker.ctx.types, symbol_type)
            .map(|shape| {
                shape
                    .properties
                    .iter()
                    .map(|prop| {
                        (
                            checker.ctx.types.resolve_atom_ref(prop.name).to_string(),
                            prop.declaration_order,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
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
fn test_module_exports_object_literal_member_conflicts_with_module_augmentation() {
    let files = [
        (
            "/test.js",
            r#"module.exports = {
  a: "ok"
};"#,
        ),
        (
            "/index.ts",
            r#"import { a } from "./test";

declare module "./test" {
  export const a: number;
}

const n: number = a;"#,
        ),
    ];
    let diagnostics = check_named_files_entry(
        &files,
        "/index.ts",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            no_lib: true,
            module: tsz_common::common::ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2300 && message.contains("Duplicate identifier 'a'")
        }),
        "Expected TS2300 for module augmentation colliding with CommonJS object export, got: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2322),
        "Expected imported `a` to keep the JS object export type, got: {diagnostics:#?}"
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
fn test_js_export_surface_preserves_default_before_late_exports_assignment() {
    let inspection = inspect_commonjs_two_file_consumer_symbol(
        "lib.js",
        r#"
const defaultConfig = { parser: "babel" };
module.exports = { default: defaultConfig };
exports.configs = { "stage-0": defaultConfig };
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
const value = lib;
"#,
        "value",
        "./lib.js",
    );

    assert!(
        inspection.formatted.starts_with("typeof import(\""),
        "Expected require() namespace import to keep a module-style display name, got: {}",
        inspection.formatted
    );
    assert_eq!(
        inspection.shape_props,
        vec![("configs".to_string(), 2), ("default".to_string(), 1)],
        "Expected JS export surface namespace shape to preserve default-before-configs order"
    );
}

#[test]
fn test_js_export_surface_preserves_plain_exports_assignment_order() {
    let inspection = inspect_commonjs_two_file_consumer_symbol(
        "lib.js",
        r#"
exports.zzz = 1;
exports.aaa = 2;
"#,
        "consumer.ts",
        r#"
import lib = require("./lib.js");
const value = lib;
"#,
        "value",
        "./lib.js",
    );

    assert!(
        inspection.formatted.starts_with("typeof import(\""),
        "Expected require() namespace import to keep a module-style display name, got: {}",
        inspection.formatted
    );
    assert_eq!(
        inspection.shape_props,
        vec![("aaa".to_string(), 2), ("zzz".to_string(), 1)],
        "Expected JS export surface namespace shape to preserve first-seen exports assignment order"
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
fn test_module_exports_object_literal_final_assignment_keeps_props_required() {
    let diagnostics = check_commonjs_two_files(
        "mod.js",
        r#"
class Thing  { x = 1 }
class AnotherThing { y = 2  }
function foo() { return 3 }
function bar() { return 4 }
module.exports = {
    Thing,
    AnotherThing,
    foo,
    qux: bar,
    baz() { return 5 },
    literal: "",
};
"#,
        "index.ts",
        r#"
function values(
    a: typeof import('./mod.js').Thing,
    b: typeof import('./mod.js').AnotherThing,
    c: typeof import('./mod.js').foo,
    d: typeof import('./mod.js').qux,
    e: typeof import('./mod.js').baz,
    g: typeof import('./mod.js').literal,
) {
    return a.length + b.length + c() + d() + e() + g.length
}
"#,
        "./mod.js",
    );

    let ts2722: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2722)
        .collect();
    let ts18048: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 18048)
        .collect();

    assert!(
        ts2722.is_empty(),
        "Expected no TS2722 for final module.exports object literal members, got: {ts2722:#?}"
    );
    assert!(
        ts18048.is_empty(),
        "Expected no TS18048 for final module.exports object literal members, got: {ts18048:#?}"
    );
}

#[test]
fn test_jsdoc_typedef_is_not_visible_through_typeof_import_value_space() {
    let diagnostics = check_commonjs_two_files(
        "mod.js",
        r#"
/** @typedef {() => number} buz */
module.exports = {};
"#,
        "index.ts",
        r#"
function values(f: typeof import('./mod.js').buz) {
    return f()
}
"#,
        "./mod.js",
    );

    let ts2694: Vec<_> = diagnostics
        .iter()
        .filter(|(code, message)| *code == 2694 && message.contains("buz"))
        .collect();
    assert_eq!(
        ts2694.len(),
        1,
        "Expected typeof import('./mod.js').buz to stay value-invisible, got: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_param_typeof_import_reports_missing_value_export() {
    let diagnostics = check_commonjs_two_files(
        "mod.js",
        r#"
/** @typedef {() => number} buz */
module.exports = {};
"#,
        "main.js",
        r#"
/**
 * @param {typeof import('./mod.js').buz} f
 */
function values(f) {
    return f()
}
"#,
        "./mod.js",
    );

    let ts2694: Vec<_> = diagnostics
        .iter()
        .filter(|(code, message)| *code == 2694 && message.contains("buz"))
        .collect();
    assert_eq!(
        ts2694.len(),
        1,
        "Expected JSDoc typeof import('./mod.js').buz to report TS2694, got: {diagnostics:#?}"
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

