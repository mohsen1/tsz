use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

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
        std::sync::Arc::make_mut(&mut binder_b.module_exports)
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

fn check_commonjs_three_files_with_types(
    types_name: &str,
    types_source: &str,
    producer_name: &str,
    producer_source: &str,
    consumer_name: &str,
    consumer_source: &str,
    module_specifier: &str,
) -> Vec<(u32, String)> {
    let mut parser_types = ParserState::new(types_name.to_string(), types_source.to_string());
    let root_types = parser_types.parse_source_file();
    let mut binder_types = BinderState::new();
    binder_types.bind_source_file(parser_types.get_arena(), root_types);

    let mut parser_producer =
        ParserState::new(producer_name.to_string(), producer_source.to_string());
    let root_producer = parser_producer.parse_source_file();
    let mut binder_producer = BinderState::new();
    binder_producer.bind_source_file(parser_producer.get_arena(), root_producer);

    let mut parser_consumer =
        ParserState::new(consumer_name.to_string(), consumer_source.to_string());
    let root_consumer = parser_consumer.parse_source_file();
    let mut binder_consumer = BinderState::new();
    binder_consumer.bind_source_file(parser_consumer.get_arena(), root_consumer);

    let arena_types = Arc::new(parser_types.get_arena().clone());
    let arena_producer = Arc::new(parser_producer.get_arena().clone());
    let arena_consumer = Arc::new(parser_consumer.get_arena().clone());
    let all_arenas = Arc::new(vec![
        Arc::clone(&arena_types),
        Arc::clone(&arena_producer),
        Arc::clone(&arena_consumer),
    ]);

    let file_exports = binder_producer.module_exports.get(producer_name).cloned();
    if let Some(exports) = &file_exports {
        std::sync::Arc::make_mut(&mut binder_consumer.module_exports)
            .insert(module_specifier.to_string(), exports.clone());
    }

    let mut cross_file_targets = FxHashMap::default();
    if let Some(exports) = &file_exports {
        for (_name, &sym_id) in exports.iter() {
            cross_file_targets.insert(sym_id, 1usize);
        }
    }

    let binder_types = Arc::new(binder_types);
    let binder_producer = Arc::new(binder_producer);
    let binder_consumer = Arc::new(binder_consumer);
    let all_binders = Arc::new(vec![
        Arc::clone(&binder_types),
        Arc::clone(&binder_producer),
        Arc::clone(&binder_consumer),
    ]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_consumer.as_ref(),
        binder_consumer.as_ref(),
        &types,
        consumer_name.to_string(),
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
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
    checker.check_source_file(root_consumer);

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
fn import_equals_require_uses_export_equals_object_value_for_property_access() {
    let diagnostics = check_commonjs_two_files(
        "m.ts",
        r#"
const c = { a: 1, b: "ok" };
export = c;
"#,
        "a.ts",
        r#"
import c1 = require("./m");
c1.a;
c1.b;
c1.missing;
"#,
        "./m",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        1,
        "Expected only c1.missing to report TS2339, got: {diagnostics:#?}"
    );
    assert!(
        ts2339[0].1.contains("Property 'missing' does not exist"),
        "Expected TS2339 to target c1.missing, got: {ts2339:#?}"
    );
}

#[test]
fn import_equals_require_uses_direct_export_equals_object_literal_for_property_access() {
    let diagnostics = check_commonjs_two_files(
        "c.ts",
        r#"
export = { a: true, b: "hello" };
"#,
        "file1.ts",
        r#"
import c1 = require("./c");
c1.a;
c1.b;
c1.missing;
"#,
        "./c",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        1,
        "Expected only c1.missing to report TS2339, got: {diagnostics:#?}"
    );
    assert!(
        ts2339[0].1.contains("Property 'missing' does not exist"),
        "Expected TS2339 to target c1.missing, got: {ts2339:#?}"
    );
}

#[test]
fn import_equals_require_uses_direct_export_equals_primitive_for_bare_value() {
    let diagnostics = check_commonjs_two_files(
        "f.ts",
        r#"
export = 10;
"#,
        "file1.ts",
        r#"
import f = require("f");
let fnumber: number = f;
let fstring: string = f;
"#,
        "f",
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected only string assignment to report TS2322, got: {diagnostics:#?}"
    );
    assert!(
        ts2322[0]
            .1
            .contains("Type 'number' is not assignable to type 'string'"),
        "Expected TS2322 to use the export= number value, got: {ts2322:#?}"
    );
}

#[test]
fn import_equals_require_uses_direct_export_equals_class_expression_in_type_position() {
    let diagnostics = check_commonjs_two_files(
        "mod1.ts",
        r#"
export = class {
    chunk = 1;
}
"#,
        "use.ts",
        r#"
import Chunk = require("./mod1");
declare var c: Chunk;
c.chunk;
c.missing;
"#,
        "./mod1",
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2749),
        "Expected Chunk to be accepted in type position, got: {diagnostics:#?}"
    );
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert_eq!(
        ts2339.len(),
        1,
        "Expected only c.missing to report TS2339, got: {diagnostics:#?}"
    );
    assert!(
        ts2339[0].1.contains("Property 'missing' does not exist"),
        "Expected TS2339 to target c.missing, got: {ts2339:#?}"
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
fn test_ts_import_type_does_not_see_commonjs_object_literal_members_as_named_exports() {
    let diagnostics = check_commonjs_two_files(
        "mod.js",
        r#"
class Thing  { x = 1 }
class AnotherThing { y = 2  }
function foo() { return 3 }
function bar() { return 4 }
/** @typedef {() => number} buz */
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
function types(
    a: import('./mod.js').Thing,
    b: import('./mod.js').AnotherThing,
    c: import('./mod.js').foo,
    d: import('./mod.js').qux,
    e: import('./mod.js').baz,
    g: import('./mod.js').literal,
) {
    return a.x + b.y + c() + d() + e() + g.length;
}
"#,
        "./mod.js",
    );

    let ts2694 = diagnostics.iter().filter(|(code, _)| *code == 2694);
    assert_eq!(
        ts2694.count(),
        6,
        "Expected TS2694 for CommonJS object-literal export members in TS import types, got: {diagnostics:#?}"
    );
}

#[test]
fn test_module_exports_chain_assignment_alias_keeps_same_file_reads_in_sync() {
    let diagnostics = check_commonjs_file(
        "npmlog.js",
        r#"
class EE {
    /** @param {string} s */
    on(s) { }
}
var npmlog = module.exports = new EE();

npmlog.on("hi");
module.exports.on("hi");

npmlog.x = 1;
module.exports.y = 2;
npmlog.y;
module.exports.x;
"#,
    );

    // TS7: `module.exports = new EE()` mixed with `module.exports.y = ...`
    // expando writes is an illegal export-assignment combination (TS2309); the
    // module type is exactly `EE`. Reads of the expando properties `x`/`y`
    // through either the `npmlog` alias or the `module.exports` surface resolve
    // against `EE` and surface TS2339, while `on` (a real `EE` member) stays
    // valid on both — i.e. the alias and `module.exports` remain in sync.
    let has_ts2309 = diagnostics.iter().any(|(code, _)| *code == 2309);
    assert!(
        has_ts2309,
        "Expected TS2309 for the mixed export assignment, got: {diagnostics:#?}"
    );
    let on_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, message)| *code == 2339 && message.contains("'on'"))
        .collect();
    assert!(
        on_errors.is_empty(),
        "`on` is a real EE member and must resolve on both the alias and module.exports, got: {on_errors:#?}"
    );
    let x_errors = diagnostics
        .iter()
        .filter(|(code, message)| *code == 2339 && message.contains("'x'"))
        .count();
    let y_errors = diagnostics
        .iter()
        .filter(|(code, message)| *code == 2339 && message.contains("'y'"))
        .count();
    assert!(
        x_errors >= 1 && y_errors >= 1,
        "Expected TS2339 for expando props `x` and `y` that are not members of EE, got: {diagnostics:#?}"
    );
}

#[test]
fn test_module_exports_chain_assignment_alias_keeps_consumer_surface_in_sync() {
    let diagnostics = check_commonjs_two_files(
        "npmlog.js",
        r#"
class EE {
    /** @param {string} s */
    on(s) { }
}
var npmlog = module.exports = new EE();
npmlog.x = 1;
"#,
        "use.js",
        r#"
var npmlog = require("./npmlog");
npmlog.x;
npmlog.on;
"#,
        "./npmlog",
    );

    // TS7: `module.exports = new EE()` types the module as exactly `EE`, so the
    // require() consumer reading `npmlog.x` (a non-EE expando write) surfaces
    // TS2339, while `npmlog.on` (a real EE member) stays valid.
    let x_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, message)| *code == 2339 && message.contains("'x'"))
        .collect();
    assert!(
        !x_errors.is_empty(),
        "Expected TS2339 for consumer read of expando `x` not on EE, got: {diagnostics:#?}"
    );
    let on_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, message)| *code == 2339 && message.contains("'on'"))
        .collect();
    assert!(
        on_errors.is_empty(),
        "`on` is a real EE member and must resolve through the alias, got: {on_errors:#?}"
    );
    // NOTE (tsc divergence, follow-up): with only an alias-property write
    // (`npmlog.x = 1`) and no direct `module.exports.p =` / `exports.p =`
    // sibling, tsc does NOT treat the alias write as an "other exported
    // element" and emits no TS2309, whereas tsz currently does. This is a
    // narrow over-count in the CommonJS export-conflict detection that does not
    // appear in the conformance corpus; tracked separately from the core TS7
    // export-assignment boundary.
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
fn test_module_exports_function_expando_assignments_report_ts2339() {
    // A CommonJS export member never hosts nested expando growth in tsc 7.0.2
    // (a TS6 -> TS7 change): `module.exports.b.cat = ...`, `.c.Cls = class {}`,
    // and `.f.self = ...` are all plain property writes against `b`/`c`/`f`'s
    // own function types and each report TS2339. (tsc 6.0.2 accepted all three.)
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
    assert_eq!(
        ts2339.len(),
        3,
        "Expected TS2339 on each nested write (.cat, .Cls, .self) against the closed function member, got: {diagnostics:#?}"
    );
    assert!(
        ts2339
            .iter()
            .all(|(_, msg)| msg.contains("does not exist on type")),
        "Expected each TS2339 to name the missing nested member, got: {ts2339:#?}"
    );
}

#[test]
fn test_module_exports_nested_class_property_reports_ts2339_cross_file() {
    // tsc 7.0.2: a CommonJS export member (`module.exports.c`, a function) hosts
    // no nested member, so `module.exports.c.Cls = class {}` is illegal and the
    // synthesized export surface must NOT expose `Cls` to consumers. The
    // consumer's `new b.c.Cls()` therefore reports TS2339 (`Cls` not on
    // `() => void`), not a resolved instance whose `x` collides with `string`.
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
        ts2322.is_empty(),
        "Expected no TS2322: the nested class member is not exposed, so `inst` is `any`, got: {diagnostics:#?}"
    );
    assert_eq!(
        ts2339.len(),
        1,
        "Expected TS2339 on the consumer's `new b.c.Cls()` read of the illegal nested member, got: {diagnostics:#?}"
    );
    assert!(
        ts2339[0].1.contains("Cls"),
        "Expected the consumer TS2339 to name the missing `Cls` member, got: {ts2339:#?}"
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

#[test]
fn test_require_binding_beats_ambient_global_dts() {
    let diagnostics = check_commonjs_three_files_with_types(
        "types.d.ts",
        r#"
declare var mod: string;
"#,
        "mod.js",
        r#"
function A() {}
function B() {}
exports.A = A;
exports.B = B;
"#,
        "use.js",
        r#"
var mod = require('./mod');
var a = mod.A;
var b = mod.B;
"#,
        "./mod",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for require() binding beating ambient global d.ts, got: {ts2339:#?}\nAll diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_import_type_uses_commonjs_exported_constructor_instance() {
    let diagnostics = check_commonjs_three_files_with_types(
        "types.d.ts",
        r#"
declare function require(name: string): any;
declare var exports: any;
declare var module: { exports: any };
"#,
        "mod1.js",
        r#"
/// <reference path='./types.d.ts'/>
class Chunk {
    constructor() {
        this.chunk = 1;
    }
}
module.exports = Chunk;
"#,
        "use.js",
        r#"
/// <reference path='./types.d.ts'/>
/** @typedef {import("./mod1")} C
 * @type {C} */
var c;
c.chunk;
"#,
        "./mod1",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for bare JSDoc import() of CommonJS-exported constructor, got: {ts2339:#?}\nAll diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_jsdoc_import_type_uses_commonjs_exported_class_expression_instance() {
    let diagnostics = check_commonjs_three_files_with_types(
        "types.d.ts",
        r#"
declare function require(name: string): any;
declare var exports: any;
declare var module: { exports: any };
"#,
        "mod1.js",
        r#"
/// <reference path='./types.d.ts'/>
module.exports = class Chunk {
    constructor() {
        this.chunk = 1;
    }
};
"#,
        "use.js",
        r#"
/// <reference path='./types.d.ts'/>
/** @typedef {import("./mod1")} C
 * @type {C} */
var c;
c.chunk;
"#,
        "./mod1",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for bare JSDoc import() of CommonJS-exported class expression, got: {ts2339:#?}\nAll diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_import_equals_require_uses_export_equals_class_expression_instance_type() {
    let diagnostics = check_commonjs_two_files(
        "mod1.ts",
        r#"
export = class {
    chunk = 1;
}
"#,
        "use.ts",
        r#"
import Chunk = require("./mod1");
declare var c: Chunk;
c.chunk;
"#,
        "./mod1",
    );

    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339.is_empty(),
        "Expected no TS2339 for import-equals of CommonJS-exported class expression, got: {ts2339:#?}\nAll diagnostics: {diagnostics:#?}"
    );
}

// --- `exports.X = value` declares; chains declare too. ---
//
// The type of a CommonJS export property is the union of every assignment in the
// module, so no write is checked against a type established by a *different*
// write. That holds inside an assignment chain exactly as it does for a
// standalone write. Verified against the pinned tsc 7.0.2, which reports nothing
// for these sources (`salsa/assignmentToVoidZero1`, upstream #38552).

fn assignment_mismatch_codes(source: &str) -> Vec<u32> {
    check_commonjs_file("a.js", source)
        .into_iter()
        .map(|(code, _)| code)
        .filter(|code| *code == 2322)
        .collect()
}

#[test]
fn chained_export_write_declares_rather_than_assigning() {
    let codes = assignment_mismatch_codes(
        "exports.y = exports.x = void 0;\nexports.x = 1;\nexports.y = 2;\n",
    );
    assert!(
        codes.is_empty(),
        "a chained `exports.X =` write declares, so `void 0` is not checked against \
         a literal from a later assignment; got TS2322 codes: {codes:?}"
    );
}

/// Same shape under different property names — the rule is structural, not keyed
/// to `x`/`y`.
#[test]
fn chained_export_write_declares_under_renamed_properties() {
    let codes = assignment_mismatch_codes(
        "exports.beta = exports.alpha = void 0;\nexports.alpha = 'str';\nexports.beta = true;\n",
    );
    assert!(
        codes.is_empty(),
        "renamed binders must behave identically; got TS2322 codes: {codes:?}"
    );
}

/// A standalone write was already a declaration and stays one.
#[test]
fn standalone_export_write_declares() {
    let codes = assignment_mismatch_codes("exports.x = void 0;\nexports.x = 1;\n");
    assert!(
        codes.is_empty(),
        "a standalone `exports.X =` write declares; got TS2322 codes: {codes:?}"
    );
}

/// Negative case: an explicit JSDoc `@type` makes the declared type
/// authoritative, so the write IS checked and still reports.
#[test]
fn jsdoc_annotated_export_write_still_reports_mismatch() {
    let codes = assignment_mismatch_codes("/** @type {number} */\nexports.x = \"hi\";\n");
    assert!(
        codes.contains(&2322),
        "an explicit @type annotation keeps the assignability check; expected TS2322, \
         got: {codes:?}"
    );
}

// ==========================================================================
// A CommonJS export member never hosts nested expando growth (tsc 7.0.2).
//
// `exports.n = {}` / `module.exports.n = {}` makes `n` a closed member; a
// nested write `exports.n.K = <rhs>` is a plain property assignment against
// `n`'s own type and reports TS2339 for EVERY rhs shape (object, function,
// class), naming `n`'s real type (`{}`, `() => void`, ...) as the receiver —
// never the whole-module `typeof import(...)`. A plain local `var NS = {}`
// stays a legitimate expando host, so the rule is specific to export members.
// (tsc 6.0.2 accepted all of these; this is a 6 -> 7 behavior change.)
// ==========================================================================

#[test]
fn exports_member_object_rhs_nested_write_reports_ts2339_on_closed_shape() {
    for prelude in ["exports.n = {};", "module.exports.n = {};"] {
        let src = format!("{prelude}\nexports.n.K = 5;\n");
        let diagnostics = check_commonjs_file("index.js", &src);
        let ts2339: Vec<_> = diagnostics
            .iter()
            .filter(|(code, _)| *code == 2339)
            .collect();
        assert_eq!(
            ts2339.len(),
            1,
            "nested write on the `{{}}` export member must be a single TS2339, got: {diagnostics:#?}"
        );
        assert!(
            ts2339[0].1.contains("'K'") && ts2339[0].1.contains("'{}'"),
            "TS2339 must name the missing `K` and the closed `{{}}` receiver (not `typeof import`), got: {:#?}",
            ts2339[0]
        );
    }
}

#[test]
fn exports_member_function_rhs_nested_write_reports_ts2339_on_function_shape() {
    let diagnostics = check_commonjs_file(
        "index.js",
        "exports.n = {};\nexports.n.K = function () {\n    this.x = 10;\n};\n",
    );
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    // Two TS2339: the nested write `exports.n.K` on `{}`, and `this.x` inside
    // the assigned function — `this` binds to the receiver `exports.n` (`{}`),
    // not to `typeof import(...)`.
    assert_eq!(
        ts2339.len(),
        2,
        "expected TS2339 at the nested write and at `this.x`, got: {diagnostics:#?}"
    );
    assert!(
        ts2339.iter().all(|(_, msg)| msg.contains("'{}'")),
        "both TS2339 must report the closed `{{}}` receiver, not `typeof import(...)`, got: {ts2339:#?}"
    );
}

#[test]
fn local_var_object_host_still_grows_nested_members() {
    // Contrast: a plain local `var NS = {}` is a real expando host, so
    // `NS.K = class {}` and its use stay clean (unchanged from tsc 6 and 7).
    let diagnostics = check_commonjs_file(
        "index.js",
        "var NS = {};\nNS.K = class {\n    m() { return 1; }\n};\nnew NS.K().m();\n",
    );
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339.is_empty(),
        "a local `var NS = {{}}` must still host `NS.K`, got: {ts2339:#?}"
    );
}

#[test]
fn direct_exports_member_write_stays_valid() {
    // Guard the boundary: the bare exports object still hosts DIRECT member
    // declarations. `exports.foo = ...` / `module.exports.bar = ...` must not
    // be swept up by the nested-member rule.
    let diagnostics = check_commonjs_file(
        "index.js",
        "exports.foo = 1;\nmodule.exports.bar = function () {};\n",
    );
    let ts2339: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2339)
        .collect();
    assert!(
        ts2339.is_empty(),
        "direct CommonJS export member writes must stay valid, got: {ts2339:#?}"
    );
}
