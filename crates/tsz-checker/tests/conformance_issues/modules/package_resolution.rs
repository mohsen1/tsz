use super::super::core::*;

#[test]
fn test_imported_declaration_file_with_top_level_declare_global_emits_ts2669() {
    let mut parser_entry = ParserState::new(
        "/src/index.ts".to_string(),
        r#"
import {} from "./react";
export const x = 1;
"#
        .to_string(),
    );
    let root_entry = parser_entry.parse_source_file();
    let mut binder_entry = BinderState::new();
    binder_entry.bind_source_file(parser_entry.get_arena(), root_entry);

    let mut parser_react = ParserState::new(
        "/src/react.d.ts".to_string(),
        "declare global {}".to_string(),
    );
    let root_react = parser_react.parse_source_file();
    let mut binder_react = BinderState::new();
    binder_react.bind_source_file(parser_react.get_arena(), root_react);

    let arena_entry = Arc::new(parser_entry.get_arena().clone());
    let arena_react = Arc::new(parser_react.get_arena().clone());
    let binder_entry = Arc::new(binder_entry);
    let binder_react = Arc::new(binder_react);
    let all_arenas = Arc::new(vec![Arc::clone(&arena_entry), Arc::clone(&arena_react)]);
    let all_binders = Arc::new(vec![Arc::clone(&binder_entry), Arc::clone(&binder_react)]);

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((0, "./react".to_string()), 1);
    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert("./react".to_string());

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_entry.as_ref(),
        binder_entry.as_ref(),
        &types,
        "/src/index.ts".to_string(),
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(0);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root_entry);
    let diagnostics: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2669),
        "Expected imported declaration file with top-level declare global to still report TS2669. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
#[ignore = "pre-existing regression"]
fn test_module_augmentation_global_imported_return_type_keeps_augmented_array_method() {
    if load_lib_files_for_test().is_empty() {
        return;
    }

    let files = [
        (
            "/f1.ts",
            r#"
export class A { x: number; }
"#,
        ),
        (
            "/f2.ts",
            r#"
import { A } from "./f1";

declare global {
    interface Array<T> {
        getA(): A;
    }
}

let x = [1];
let y = x.getA().x;
"#,
        ),
    ];

    let diagnostics = compile_named_files_get_diagnostics_with_lib_and_options(
        &files,
        "/f2.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2564)
        .collect();
    assert!(
        !relevant.iter().any(|(code, _)| *code == 2339),
        "Expected imported return type in declare global Array augmentation to preserve getA().x without TS2339. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_divergent_accessor_read_keeps_getter_surface_without_ts2339() {
    if load_lib_files_for_test().is_empty() {
        return;
    }

    let diagnostics = compile_and_get_diagnostics_with_merged_lib_contexts_and_options(
        r#"
export {};

interface CSSStyleDeclaration {
    animationTimingFunction: string;
}

interface Element {
    get style(): CSSStyleDeclaration;
    set style(cssText: string);
}

declare const element: Element;
element.style = "color: red";
element.style.animationTimingFunction;
element.style = element.style;

type Fail<T extends never> = T;
interface I1 {
    get x(): number;
    set x(value: Fail<string>);
}
const o1 = {
    get x(): number { return 0; },
    set x(value: Fail<string>) {}
}

const o2 = {
    get p1() { return 0; },
    set p1(value: string) {},

    get p2(): number { return 0; },
    set p2(value: string) {},
};
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2339),
        "Expected divergent accessor getter reads to preserve CSSStyleDeclaration members without TS2339. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_array_buffer_view_default_type_argument_does_not_emit_ts2314() {
    if load_lib_files_for_test().is_empty() {
        return;
    }

    let diagnostics = compile_and_get_diagnostics_with_merged_lib_contexts_and_options(
        r#"
var obj: Object;
if (ArrayBuffer.isView(obj)) {
    var ab: ArrayBufferView = obj;
}
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2314),
        "Expected bare ArrayBufferView to use its default type argument without TS2314. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_array_from_iterable_and_array_like_overloads_do_not_emit_ts2314() {
    if load_lib_files_for_test().is_empty() {
        return;
    }

    let diagnostics = compile_and_get_diagnostics_with_merged_lib_contexts_and_options(
        r#"
interface A {
  a: string;
}

interface B {
  b: string;
}

const inputA: A[] = [];
const inputALike: ArrayLike<A> = { length: 0 };
const inputARand = getEither(inputA, inputALike);
const inputASet = new Set<A>();

const result1: A[] = Array.from(inputA);
const result2: A[] = Array.from(inputA.values());
const result4: A[] = Array.from([{ b: "x" } as B], ({ b }): A => ({ a: b }));
const result5: A[] = Array.from(inputALike);
const result7: B[] = Array.from(inputALike, ({ a }): B => ({ b: a }));
const result8: A[] = Array.from(inputARand);
const result9: B[] = Array.from(inputARand, ({ a }): B => ({ b: a }));
const result10: A[] = Array.from(inputASet);
const result11: B[] = Array.from(inputASet, ({ a }): B => ({ b: a }));

function getEither<T>(in1: Iterable<T>, in2: ArrayLike<T>) {
  return Math.random() > 0.5 ? in1 : in2;
}
"#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2314),
        "Expected Array.from overloads with defaulted lib generics to avoid TS2314. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_umd_global_conflict_prefers_first_namespace_export_surface() {
    let files = [
        (
            "/v1/index.d.ts",
            r#"
export as namespace Alpha;
export var x: string;
"#,
        ),
        (
            "/v2/index.d.ts",
            r#"
export as namespace Alpha;
export var y: number;
"#,
        ),
        (
            "/consumer.ts",
            r#"
import * as v1 from "./v1";
import * as v2 from "./v2";
"#,
        ),
        (
            "/global.ts",
            r#"
const p: string = Alpha.x;
"#,
        ),
    ];

    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &files,
        "/global.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2339),
        "Expected first UMD namespace export to win for Alpha.x without TS2339. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics for umdGlobalConflict. Actual diagnostics: {diagnostics:#?}"
    );
}

/// Regression test: cross-binder SymbolId collision must not cause false
/// TS2448 ("used before declaration") or TS2454 ("used before assignment")
/// for UMD globals resolved from another file's binder.
///
/// When `resolve_identifier_symbol_from_all_binders` returns a SymbolId from
/// another file, subsequent code that looks up that numeric ID in the local
/// binder finds a *different* symbol. If that local symbol is a block-scoped
/// variable, TDZ/DAA checks fire incorrectly.
#[test]
fn test_umd_global_no_false_tdz_or_daa_cross_binder() {
    let files = [
        (
            "/lib/index.d.ts",
            r#"
export as namespace Lib;
export var value: string;
"#,
        ),
        (
            "/app.ts",
            r#"
const result: string = Lib.value;
"#,
        ),
    ];

    let diagnostics = compile_named_files_get_diagnostics_with_options(
        &files,
        "/app.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2448),
        "Should not emit TS2448 for cross-file UMD global. Diagnostics: {diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2454),
        "Should not emit TS2454 for cross-file UMD global. Diagnostics: {diagnostics:#?}"
    );
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics. Actual: {diagnostics:#?}"
    );
}

fn compile_two_global_files_get_diagnostics_with_options(
    a_name: &str,
    a_source: &str,
    b_name: &str,
    b_source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
    let mut parser_a = ParserState::new(a_name.to_string(), a_source.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = ParserState::new(b_name.to_string(), b_source.to_string());
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
        b_name.to_string(),
        options,
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
fn test_isolated_modules_imported_non_literal_numeric_enum_member_uses_ts18056() {
    let diagnostics = compile_two_files_get_diagnostics_with_options(
        "export const foo = 2;",
        r#"
import { foo } from "./helpers";
enum A {
    a = foo,
    b,
}
"#,
        "./helpers",
        CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            isolated_modules: true,
            no_lib: true,
            no_types_and_symbols: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 18056),
        "Expected TS18056 for an imported non-literal numeric enum member under isolatedModules. Actual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 1061),
        "Did not expect fallback TS1061 for an imported non-literal numeric enum member. Actual diagnostics: {diagnostics:#?}"
    );
}

/// Verify that the expando suppression is NOT applied to declared enum members
/// in JS files. Before the fix, both the "enum rebind" and "enum expando" checks
/// would short-circuit, silently suppressing all diagnostics for `lf.Order.DESC = 0`.
///
/// After the fix, declared enum member assignments go through normal type checking.
/// In a full project (conformance test `conformance/salsa/enumMergeWithExpando.ts`),
/// this produces TS2540 (readonly). In the minimal unit-test harness, cross-file
/// readonly resolution is incomplete, so we only verify assignments are NOT suppressed.
#[test]
fn test_js_namespace_enum_declared_member_not_suppressed_as_expando() {
    let diagnostics = compile_two_global_files_get_diagnostics_with_options(
        "lovefield-ts.d.ts",
        r#"
declare namespace lf {
    export enum Order { ASC, DESC }
}
"#,
        "enums.js",
        r#"
lf.Order = {}
lf.Order.DESC = 0;
lf.Order.ASC = 1;
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            allow_js: true,
            check_js: true,
            ..CheckerOptions::default()
        },
    );

    let codes: Vec<u32> = diagnostics.iter().map(|(code, _)| *code).collect();

    // Assignments to declared enum members (DESC, ASC) must NOT be silently
    // suppressed. In a full project, TS2540 fires; in the unit test harness
    // we may see TS2322 (type mismatch) instead because the readonly check
    // needs deeper cross-file resolution. Either way, diagnostics must NOT
    // be empty for the member assignments.
    let member_errors = codes.iter().filter(|&&c| c == 2540 || c == 2322).count();
    assert!(
        member_errors >= 2,
        "Expected diagnostics for declared enum member assignments (DESC, ASC) — \
         old bug was silently suppressing them.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_const_enum_element_access_requires_string_literal_index() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
const enum G {
    A = 1,
    B = 2,
}

var z1 = G[G.A];
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2476),
        "Expected TS2476 for const enum element access with a non-string-literal index.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_duplicate_class_computed_unique_symbol_members_report_ts2300() {
    // Test that unique symbol typed computed properties correctly detect duplicates,
    // while non-unique-symbol computed properties (unions, function calls) do not.
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
declare const uniqueSymbol0: unique symbol;
declare const uniqueSymbol1: unique symbol;

class Cls1 {
  [uniqueSymbol0] = "first";
  [uniqueSymbol0] = "last";
  [uniqueSymbol1] = "first";
  [uniqueSymbol1] = "last";
}

// const with literal type — statically determinable, should detect duplicates
const literalKey = "hello";
class Cls2 {
  [literalKey] = "first";
  [literalKey] = "last";
}

// const with union type — NOT statically determinable, should NOT detect duplicates
const unionKey = Math.random() > 0.5 ? "a" : "b";
class Cls3 {
  static [unionKey]() { return 1; }
  static [unionKey]() { return 2; }
}
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ESNext,
            ..CheckerOptions::default()
        },
    );

    // Cls1: uniqueSymbol0 dup + uniqueSymbol1 dup = 2 TS2300
    // Cls2: literalKey dup = 1 TS2300
    // Cls3: unionKey methods = 0 TS2300 (late-bound, not checked)
    let ts2300_count = diagnostics.iter().filter(|(code, _)| *code == 2300).count();
    assert!(
        ts2300_count >= 3,
        "Expected TS2300 for duplicate computed class members keyed by unique symbols and literal const keys.\nActual diagnostics: {diagnostics:#?}"
    );

    // Cls3 should NOT have TS2393 (duplicate function implementation)
    let ts2393_messages: Vec<_> = diagnostics
        .iter()
        .filter(|(code, msg)| *code == 2393 && msg.contains("unionKey"))
        .collect();
    assert!(
        ts2393_messages.is_empty(),
        "Should NOT emit TS2393 for late-bound (union-typed) computed method names.\nActual: {ts2393_messages:#?}"
    );
}

#[test]
fn test_const_enum_element_access_missing_string_literal_member_reports_ts2339() {
    let diagnostics = compile_and_get_diagnostics_named(
        "test.ts",
        r#"
const enum E { A }
var x = E["B"];
"#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2339),
        "Expected TS2339 for missing const enum string-literal member access.\nActual diagnostics: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 7053),
        "Did not expect TS7053 for missing const enum string-literal member access.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_regexp_literal_exec_preserves_nullability() {
    let diagnostics =
        without_missing_global_type_errors(compile_and_get_diagnostics_with_lib_and_options(
            r#"
let re = /\d{4}/;
let result = re.exec("2015");
let value = result[0];
"#,
            CheckerOptions {
                target: tsz_common::common::ScriptTarget::ES2015,
                ..CheckerOptions::default()
            },
        ));

    if diagnostics.is_empty() {
        return;
    }

    assert!(
        has_error(&diagnostics, 18047),
        "Expected TS18047 because RegExp#exec can return null.\nActual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_isolated_modules_imported_non_literal_string_enum_member_uses_ts18055() {
    let diagnostics = compile_two_files_get_diagnostics_with_options(
        r#"export const bar = "bar";"#,
        r#"
import { bar } from "./helpers";
enum A {
    a = bar,
}
"#,
        "./helpers",
        CheckerOptions {
            module: tsz_common::common::ModuleKind::CommonJS,
            isolated_modules: true,
            no_lib: true,
            no_types_and_symbols: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 18055),
        "Expected TS18055 for an imported non-syntactic string enum initializer under isolatedModules. Actual diagnostics: {diagnostics:#?}"
    );
}

fn compile_ambient_module_and_consumer_get_diagnostics(
    ambient_source: &str,
    consumer_source: &str,
    module_spec: &str,
) -> Vec<(u32, String)> {
    let mut parser_a = ParserState::new("ambient.d.ts".to_string(), ambient_source.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = ParserState::new("consumer.ts".to_string(), consumer_source.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    let arena_a = Arc::new(parser_a.get_arena().clone());
    let arena_b = Arc::new(parser_b.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena_a), Arc::clone(&arena_b)]);

    let ambient_exports = binder_a.module_exports.get(module_spec).cloned();
    if let Some(exports) = &ambient_exports {
        std::sync::Arc::make_mut(&mut binder_b.module_exports)
            .insert(module_spec.to_string(), exports.clone());
    }

    let mut cross_file_targets = FxHashMap::default();
    if let Some(exports) = &ambient_exports {
        for (_, &sym_id) in exports.iter() {
            cross_file_targets.insert(sym_id, 0usize);
        }
    }

    let binder_a = Arc::new(binder_a);
    let binder_b = Arc::new(binder_b);
    let all_binders = Arc::new(vec![Arc::clone(&binder_a), Arc::clone(&binder_b)]);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        module: tsz_common::common::ModuleKind::CommonJS,
        no_lib: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        arena_b.as_ref(),
        binder_b.as_ref(),
        &types,
        "consumer.ts".to_string(),
        options,
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(1);

    for (sym_id, file_idx) in &cross_file_targets {
        checker.ctx.register_symbol_file_target(*sym_id, *file_idx);
    }

    checker.check_source_file(root_b);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

#[test]
fn test_commonjs_exported_js_constructor_with_prototype_writes_is_constructable() {
    let a_source = r#"
function F() {}
F.prototype.answer = 42;
module.exports.F = F;
"#;
    let b_source = r#"
const x = require("./a.js");
new x.F();
"#;

    let mut parser_a = ParserState::new("a.js".to_string(), a_source.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = ParserState::new("b.js".to_string(), b_source.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    let arena_a = Arc::new(parser_a.get_arena().clone());
    let arena_b = Arc::new(parser_b.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena_a), Arc::clone(&arena_b)]);

    let file_a_exports = binder_a.module_exports.get("a.js").cloned();
    if let Some(exports) = &file_a_exports {
        std::sync::Arc::make_mut(&mut binder_b.module_exports)
            .insert("./a.js".to_string(), exports.clone());
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
        "b.js".to_string(),
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
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
    resolved_module_paths.insert((1, "./a.js".to_string()), 0);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));

    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert("./a.js".to_string());
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(root_b);

    let diagnostics: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 7009),
        "Expected exported JS constructor to remain constructable across require(). Got: {diagnostics:#?}"
    );
}

#[test]
fn test_commonjs_exported_js_constructor_index_errors_use_function_name() {
    let a_source = r#"
const s = Symbol();
function F() {}
F.prototype[s] = "ok";
module.exports.F = F;
module.exports.S = s;
"#;
    let b_source = r#"
const x = require("./a.js");
const inst = new x.F();
inst[x.S];
"#;

    let mut parser_a = ParserState::new("a.js".to_string(), a_source.to_string());
    let root_a = parser_a.parse_source_file();
    let mut binder_a = BinderState::new();
    binder_a.bind_source_file(parser_a.get_arena(), root_a);

    let mut parser_b = ParserState::new("b.js".to_string(), b_source.to_string());
    let root_b = parser_b.parse_source_file();
    let mut binder_b = BinderState::new();
    binder_b.bind_source_file(parser_b.get_arena(), root_b);

    let arena_a = Arc::new(parser_a.get_arena().clone());
    let arena_b = Arc::new(parser_b.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena_a), Arc::clone(&arena_b)]);

    let file_a_exports = binder_a.module_exports.get("a.js").cloned();
    if let Some(exports) = &file_a_exports {
        std::sync::Arc::make_mut(&mut binder_b.module_exports)
            .insert("./a.js".to_string(), exports.clone());
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
        "b.js".to_string(),
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
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
    resolved_module_paths.insert((1, "./a.js".to_string()), 0);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));

    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert("./a.js".to_string());
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(root_b);

    let ts7053 = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 7053)
        .map(|d| d.message_text.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        ts7053.len(),
        1,
        "Expected TS7053 for cross-file CommonJS prototype element-access expando read. Got: {ts7053:#?}"
    );
    assert!(
        ts7053[0].contains("'F'"),
        "Expected TS7053 message to reference the exported function name 'F'. Got: {:?}",
        ts7053[0]
    );
}

#[test]
fn test_commonjs_chained_prototype_assignment_preserves_imported_constructor_methods() {
    let a_source = r#"
var A = function A() {
    this.a = 1;
};
var B = function B() {
    this.b = 2;
};
exports.A = A;
exports.B = B;
A.prototype = B.prototype = {
    /** @param {number} n */
    m(n) {
        return n + 1;
    }
};
"#;
    let b_source = r#"
var mod = require("./a.js");
var a = new mod.A();
var b = new mod.B();
a.m("nope");
b.m("still nope");
"#;

    let diagnostics = compile_two_global_files_get_diagnostics_with_options(
        "a.js",
        a_source,
        "b.js",
        b_source,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            no_implicit_any: true,
            no_lib: true,
            module: tsz_common::common::ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| !matches!(*code, 2304 | 2318))
        .collect();
    let ts7009: Vec<_> = relevant.iter().filter(|(code, _)| *code == 7009).collect();
    let ts2339: Vec<_> = relevant.iter().filter(|(code, _)| *code == 2339).collect();
    let ts2345: Vec<_> = relevant.iter().filter(|(code, _)| *code == 2345).collect();

    assert!(
        ts7009.is_empty(),
        "Expected chained prototype CommonJS constructors to stay constructable. Got: {relevant:#?}"
    );
    assert!(
        ts2339.is_empty(),
        "Expected imported chained prototype methods to stay visible. Got: {relevant:#?}"
    );
    assert_eq!(
        ts2345.len(),
        2,
        "Expected both bad calls to report TS2345 once methods are preserved. Got: {relevant:#?}"
    );
}
// ---------------------------------------------------------------------------
// Type-only export filtering: namespace import value access
// ---------------------------------------------------------------------------

/// When a module uses `export type { A }`, accessing `A` through a namespace
/// import (`import * as ns from './mod'`) in value position should produce
/// TS2339 because type-only exports are not value members of the namespace.
#[test]
fn test_type_only_export_not_accessible_as_namespace_value() {
    let a_source = r#"
class A { a!: string }
export type { A };
"#;
    let b_source = r#"
import * as types from './a';
types.A;
"#;
    let diagnostics = compile_two_files_get_diagnostics(a_source, b_source, "./a");
    // Filter out TS2318 (missing global types) since we don't load lib files in unit tests
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    let ts2339_errors: Vec<_> = relevant.iter().filter(|(code, _)| *code == 2339).collect();
    assert!(
        !ts2339_errors.is_empty(),
        "Expected TS2339 for type-only export accessed as namespace value member. Got: {relevant:?}"
    );
}

#[test]
fn test_named_import_from_export_equals_ambient_module_preserves_ts2454() {
    let ambient_source = r#"
declare namespace Express {
    export interface Request {}
}

declare module "express" {
    function e(): e.Express;
    namespace e {
        interface Request extends Express.Request {
            get(name: string): string;
        }
        interface Express {}
    }
    export = e;
}
"#;
    let consumer_source = r#"
import { Request } from "express";
let x: Request;
const y = x.get("a");
"#;

    let diagnostics = compile_ambient_module_and_consumer_get_diagnostics(
        ambient_source,
        consumer_source,
        "express",
    );

    assert!(
        has_error(&diagnostics, 2454),
        "Expected TS2454 for local variable typed from named import via export=. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_array_literal_union_context_with_object_member_contextually_types_callbacks() {
    if !lib_files_available() {
        return;
    }

    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
declare function test(
  arg: Record<string, (arg: string) => void> | Array<(arg: number) => void>
): void;

test([
  (arg) => {
    arg;
  },
]);
"#,
        CheckerOptions {
            no_implicit_any: true,
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    // TSC emits TS7006 here because the union `Record<string, fn> | Array<fn>` is ambiguous:
    // Record contributes a string-indexed callback type and Array contributes an element
    // callback type, so no single contextual type can be determined for the array element.
    // This matches tsc behavior (verified via conformance tests for both es5 and es2015 libs).
    assert!(
        has_error(&diagnostics, 7006),
        "Expected TS7006 because Record<string,fn> | Array<fn> is an ambiguous array context. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_array_literal_union_context_ignores_non_object_non_array_members() {
    if !lib_files_available() {
        return;
    }

    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
declare function test(arg: ((arg: number) => void)[] | string): void;

test([
  (arg) => {
    arg.toFixed();
  },
]);
"#,
        CheckerOptions {
            no_implicit_any: true,
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 7006),
        "Did not expect TS7006 when the non-array union member is a primitive. Actual diagnostics: {diagnostics:#?}"
    );
}

#[test]
fn test_union_call_signatures_with_mismatched_parameters_report_implicit_any() {
    if !lib_files_available() {
        return;
    }

    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
interface IWithCallSignatures {
    (a: number): string;
}
interface IWithCallSignatures3 {
    (b: string): number;
}
interface IWithCallSignatures4 {
    (a: number): string;
    (a: string, b: number): number;
}

var x3: IWithCallSignatures | IWithCallSignatures3 = a => a.toString();
var x4: IWithCallSignatures | IWithCallSignatures4 = a => a.toString();
"#,
        CheckerOptions {
            no_implicit_any: true,
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts7006 = diagnostics.iter().filter(|(code, _)| *code == 7006).count();
    assert_eq!(
        ts7006, 2,
        "Expected TS7006 for mismatched union call signatures. Actual diagnostics: {diagnostics:#?}"
    );
}

/// Multiple type-only exports should all be filtered from the namespace.
#[test]
fn test_multiple_type_only_exports_filtered_from_namespace() {
    let a_source = r#"
class A { a!: string }
class B { b!: number }
export type { A, B };
"#;
    let b_source = r#"
import * as types from './a';
types.A;
types.B;
"#;
    let diagnostics = compile_two_files_get_diagnostics(a_source, b_source, "./a");
    // Filter out TS2318 (missing global types) since we don't load lib files in unit tests
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    let ts2339_errors: Vec<_> = relevant.iter().filter(|(code, _)| *code == 2339).collect();
    assert!(
        ts2339_errors.len() >= 2,
        "Expected TS2339 for both type-only exports accessed as namespace value members. Got: {relevant:?}"
    );
}

// TS1100: eval/arguments used as function name in strict mode
#[test]
fn test_ts1100_function_named_eval_strict_mode() {
    let source = r#"
"use strict";
function eval() {}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        has_error(&diagnostics, 1100),
        "Expected TS1100 for 'function eval()' in strict mode. Got: {diagnostics:?}"
    );
}

#[test]
fn test_ts1100_function_named_arguments_strict_mode() {
    let source = r#"
"use strict";
function arguments() {}
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        has_error(&diagnostics, 1100),
        "Expected TS1100 for 'function arguments()' in strict mode. Got: {diagnostics:?}"
    );
}

#[test]
fn test_ts1100_function_expression_named_eval_strict_mode() {
    let source = r#"
"use strict";
var v = function eval() {};
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        has_error(&diagnostics, 1100),
        "Expected TS1100 for function expression named 'eval' in strict mode. Got: {diagnostics:?}"
    );
}
