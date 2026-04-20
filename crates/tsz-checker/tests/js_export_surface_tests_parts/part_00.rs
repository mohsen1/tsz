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

#[allow(dead_code)]
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

