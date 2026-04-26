use super::super::core::*;

#[test]
fn test_direct_null_equality_reports_null_not_number() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function f(x: number | null) {
    if (x === null) {
        const s: string = x;
    }
}
        "#,
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected one TS2322, got: {diagnostics:#?}"
    );
    assert!(
        ts2322[0]
            .1
            .contains("Type 'null' is not assignable to type 'string'"),
        "Expected null-based TS2322 for direct null equality, got: {ts2322:#?}"
    );
}

#[test]
fn test_class_constructor_assignment_reports_typeof_names() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
abstract class A {}
class B extends A {
    constructor(x: number) {
        super();
    }
}
const b: typeof A = B;
        "#,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected one TS2322, got: {diagnostics:#?}"
    );
    assert!(
        ts2322[0]
            .1
            .contains("Type 'typeof B' is not assignable to type 'typeof A'"),
        "Expected constructor-space TS2322, got: {ts2322:#?}"
    );
}

#[test]
fn test_control_flow_explicit_any_loop_incrementor_stays_any() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
function f() {
    let iNext: any;
    for (let i = 0; i < 10; i = iNext) {
        if (i == 5) {
            iNext = "bad";
            continue;
        }
        iNext = i + 1;
    }
}
        "#,
    );

    assert!(
        !has_error(&diagnostics, 2322),
        "Explicit any should not evolve through control flow, got: {diagnostics:#?}"
    );
}

/// TS7034/TS7005 should fire for block-scoped `let` variables when captured by nested functions
/// before they become definitely assigned on all paths.
/// From: controlFlowNoImplicitAny.ts (f10)
#[test]
fn test_ts7034_emitted_for_let_captured_by_arrow_function() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
declare let cond: boolean;
function f10() {
    let x;
    if (cond) {
        x = 1;
    }
    if (cond) {
        x = 'hello';
    }
    const y = x;
    const f = () => { const z = x; };
}
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7034),
        "Should emit TS7034 for block-scoped `let` variable.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 7005),
        "Should emit TS7005 at the captured `let` reference.\nActual errors: {diagnostics:#?}"
    );
}

/// TS7034/TS7005 should NOT fire for block-scoped `let` variables that are assigned
/// before the closure is created and remain definitely assigned at the capture point.
#[test]
fn test_ts7034_not_emitted_for_let_assigned_before_capture() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function f() {
    let x;
    x = 'hello';
    const f = () => { x; };
}
        ",
        opts,
    );
    assert!(
        !has_error(&diagnostics, 7034),
        "Should NOT emit TS7034 once the captured `let` is definitely assigned.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 7005),
        "Should NOT emit TS7005 at the captured reference once the `let` is definitely assigned.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7034_emitted_for_let_captured_before_last_assignment() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function action(f: any) {}
function f() {
    let x;
    x = 'abc';
    action(() => { x; });
    x = 42;
}
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7034),
        "Should emit TS7034 when a captured `let` is read before its last assignment.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 7005),
        "Should emit TS7005 at the captured reference before the last assignment.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7034_not_emitted_for_contextually_typed_for_of_capture() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        target: ScriptTarget::ES2015,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function f() {
    for (let x of [1, 2, 3]) {
        const g = () => x;
    }
}
        ",
        opts,
    );
    assert!(
        !has_error(&diagnostics, 7034),
        "Should NOT emit TS7034 for contextually typed `for...of` loop variables.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 7005),
        "Should NOT emit TS7005 for contextually typed `for...of` loop variables.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_import_equals_in_namespace_emits_ts1147_and_ts2307() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        module: ModuleKind::CommonJS,
        ..CheckerOptions::default()
    };
    let source = r#"
namespace myModule {
    import foo = require("test2");
}
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
        opts,
    );
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let diagnostics: Vec<(u32, String)> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    assert!(
        has_error(&diagnostics, 1147),
        "Expected TS1147 for import = require inside namespace. Actual: {diagnostics:#?}"
    );
    // TS2307 should NOT be emitted when inside a namespace - tsc only emits TS1147
    assert!(
        !has_error(&diagnostics, 2307),
        "TS2307 should not be emitted when import is inside a namespace (only TS1147). Actual: {diagnostics:#?}"
    );
}

#[test]
fn test_import_equals_in_namespace_no_ts1147_for_ambient_module() {
    // When the required module is an ambient module (declared with `declare module "..."`),
    // TS1147 should NOT be emitted — the import is a valid reference to a global ambient module.
    // See: privacyImportParseErrors.ts
    let opts = CheckerOptions {
        no_implicit_any: true,
        module: ModuleKind::CommonJS,
        ..CheckerOptions::default()
    };
    let source = r#"
export namespace m1 {
    export declare module "m1_M3_public" {
        export function f1(): void;
    }
    import m1_im3 = require("m1_M3_public");
}
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
        opts,
    );
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let diagnostics: Vec<(u32, String)> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    assert!(
        !has_error(&diagnostics, 1147),
        "TS1147 should not be emitted when import = require references an ambient module. Actual: {diagnostics:#?}"
    );
}

#[test]
fn test_import_aliases_in_global_augmentation_emit_ts2667_and_ts2591() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
export { }

namespace A {
    export const y = 34;
    export interface y { s: string }
}

declare global {
    export import x = A.y;

    // Should still error
    import f = require("fs");
}

const m: number = x;
let s: x = { s: "" };
void s.s;
        "#,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            target: ScriptTarget::ES2015,
            no_lib: true,
            ..Default::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2667),
        "Expected TS2667 for import in global augmentation. Actual: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 2591),
        "Expected TS2591 for unresolved require module in global augmentation. Actual: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 1147),
        "TS1147 should not be emitted for global augmentation import. Actual: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "TS2322 should be suppressed once global augmentation import errors are emitted. Actual: {diagnostics:#?}"
    );
}

#[test]
fn test_import_with_non_relative_ts_extension_emits_ts2877() {
    let importer_source = r#"import {} from "foo.ts";"#;
    let exported_source = "export {};";
    let files = [
        ("index.ts".to_string(), importer_source.to_string()),
        ("foo.ts".to_string(), exported_source.to_string()),
    ];
    let entry_idx = 0usize;
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| name.clone()).collect();

    for (file_name, source) in &files {
        let mut parser = ParserState::new(file_name.clone(), source.clone());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let (mut resolved_module_paths, mut resolved_modules) =
        build_module_resolution_maps(&file_names);
    resolved_module_paths.insert((entry_idx, "foo.ts".to_string()), 1);
    resolved_modules.insert("foo.ts".to_string());
    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &types,
        file_names[entry_idx].clone(),
        CheckerOptions {
            rewrite_relative_import_extensions: true,
            ..CheckerOptions::default()
        },
    );

    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);
    checker.ctx.report_unresolved_imports = true;

    checker.check_source_file(roots[entry_idx]);
    let diagnostics: Vec<(u32, String)> = checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect();

    assert!(
        has_error(&diagnostics, 2877),
        "Expected TS2877 for non-relative `.ts` imports when rewriteRelativeImportExtensions is enabled. Actual: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2307),
        "Should not also emit TS2307 for this resolved import. Actual: {diagnostics:#?}"
    );
}

#[test]
fn test_exported_var_without_type_or_initializer_emits_ts7005() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options("export var $;", opts);

    assert!(
        has_error(&diagnostics, 7005),
        "Expected TS7005 for exported bare var declaration. Actual: {diagnostics:#?}"
    );
}

#[test]
fn test_exported_var_without_type_or_initializer_emits_ts7005_in_dts() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_named("test.d.ts", "export var React;", opts);

    assert!(
        has_error(&diagnostics, 7005),
        "Expected TS7005 for exported bare var in .d.ts. Actual: {diagnostics:#?}"
    );
}

#[test]
fn test_binding_pattern_callback_does_not_infer_generic_parameter() {
    let diagnostics = compile_and_get_diagnostics_with_lib_and_options(
        r#"
declare function trans<T>(f: (x: T) => string): number;
trans(({a}) => a);
trans(([b,c]) => 'foo');
trans(({d: [e,f]}) => 'foo');
trans(([{g},{h}]) => 'foo');
trans(({a, b = 10}) => a);
        "#,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts2345_count = diagnostics.iter().filter(|(code, _)| *code == 2345).count();
    assert!(
        ts2345_count >= 1,
        "Expected TS2345 for binding-pattern callback inference fallback. Actual: {diagnostics:#?}"
    );
}

/// Nested destructured aliases should not participate in sibling discriminant correlation.
/// From: controlFlowAliasedDiscriminants.ts
#[test]
fn test_nested_destructured_alias_does_not_correlate_with_sibling_discriminant() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
type Nested = {
    type: 'string';
    resp: {
        data: string
    }
} | {
    type: 'number';
    resp: {
        data: number;
    }
};

let resp!: Nested;
const { resp: { data }, type } = resp;
if (type === 'string') {
    data satisfies string;
}
        "#,
        CheckerOptions::default(),
    );

    assert!(
        has_error(&diagnostics, 1360),
        "Nested destructured aliases should still fail `satisfies` after sibling narrowing.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_unknown_catch_variable_reassignment_does_not_narrow_alias() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
try {} catch (e) {
    const isString = typeof e === "string";
    e = 1;
    if (isString) {
        e.toUpperCase();
    }
}
        "#,
        CheckerOptions {
            strict_null_checks: true,
            use_unknown_in_catch_variables: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 18046),
        "Expected TS18046 after reassigned unknown catch variable alias invalidation, got: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2339),
        "Expected unknown catch variable access to report TS18046 instead of TS2339, got: {diagnostics:#?}"
    );
}

#[test]
fn test_unknown_catch_variable_can_be_renarrowed_after_reassignment() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
try {} catch (e) {
    e = 1;
    if (typeof e === "string") {
        let n: never = e;
    }
}
        "#,
        CheckerOptions {
            strict_null_checks: true,
            use_unknown_in_catch_variables: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2322),
        "Expected direct typeof re-check to narrow unknown catch variable to string, got: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 18046),
        "Expected no TS18046 in re-narrowed unknown catch variable branch, got: {diagnostics:#?}"
    );
}

#[test]
fn test_any_catch_variable_can_be_renarrowed_after_reassignment() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
try {} catch (e) {
    e = 1;
    if (typeof e === "string") {
        let n: never = e;
    }
}
        "#,
        CheckerOptions {
            strict_null_checks: true,
            use_unknown_in_catch_variables: false,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 2322),
        "Expected direct typeof re-check to narrow any catch variable to string, got: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 18046),
        "Expected no TS18046 for any catch variable branch, got: {diagnostics:#?}"
    );
}

/// TS7034 SHOULD fire for function-scoped `var` variables captured by arrow functions.
#[test]
fn test_ts7034_emitted_for_var_captured_by_arrow_function() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function f10() {
    var x;
    x = 'hello';
    const f = () => { x; };
}
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7034),
        "Should emit TS7034 for function-scoped `var` variable captured by arrow function.\nActual errors: {diagnostics:#?}"
    );
}

/// Conditional expressions assigned into literal unions should preserve their
/// literal branch types instead of widening to `number`.
/// From: controlFlowNoIntermediateErrors.ts
#[test]
fn test_conditional_expression_preserves_literal_union_context() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
function f1() {
    let code: 0 | 1 | 2 = 0;
    const otherCodes: (0 | 1 | 2)[] = [2, 0, 1];
    for (const code2 of otherCodes) {
        if (code2 === 0) {
            code = code === 2 ? 1 : 0;
        } else {
            code = 2;
        }
    }
}

function f2() {
    let code: 0 | 1 = 0;
    while (true) {
        code = code === 1 ? 0 : 1;
    }
}
        "#,
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );
    let ts2322_count = diagnostics.iter().filter(|(code, _)| *code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 for ternaries under literal-union context, got diagnostics: {diagnostics:#?}"
    );
}

// TS2882: Cannot find module or type declarations for side-effect import

/// TS2882 should fire by default (tsc 6.0 default: noUncheckedSideEffectImports = true).
#[test]
fn test_ts2882_side_effect_import_default_on() {
    // Default CheckerOptions has no_unchecked_side_effect_imports: true (matching tsc 6.0)
    let diagnostics = compile_imports_and_get_diagnostics(
        r#"import 'nonexistent-module';"#,
        CheckerOptions::default(),
    );
    assert!(
        has_error(&diagnostics, 2882),
        "Should emit TS2882 by default (noUncheckedSideEffectImports defaults to true in tsc 6.0).\nActual errors: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2307),
        "Should NOT emit TS2307 for side-effect import (should use TS2882 instead).\nActual errors: {diagnostics:#?}"
    );
}

/// TS2882 should fire when noUncheckedSideEffectImports is explicitly true.
#[test]
fn test_ts2882_side_effect_import_option_true() {
    let opts = CheckerOptions {
        no_unchecked_side_effect_imports: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_imports_and_get_diagnostics(r#"import 'nonexistent-module';"#, opts);
    assert!(
        has_error(&diagnostics, 2882),
        "Should emit TS2882 when noUncheckedSideEffectImports is true.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2307),
        "Should NOT emit TS2307 for side-effect import (should use TS2882 instead).\nActual errors: {diagnostics:#?}"
    );
}

/// Side-effect imports should NOT emit any error when noUncheckedSideEffectImports is false.
#[test]
fn test_ts2882_side_effect_import_option_false() {
    let opts = CheckerOptions {
        no_unchecked_side_effect_imports: false,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_imports_and_get_diagnostics(r#"import 'nonexistent-module';"#, opts);
    assert!(
        !has_error(&diagnostics, 2882),
        "Should NOT emit TS2882 when noUncheckedSideEffectImports is false.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2307),
        "Should NOT emit TS2307 for side-effect import.\nActual errors: {diagnostics:#?}"
    );
}

/// Regular imports should still emit TS2307 even when noUncheckedSideEffectImports is enabled.
#[test]
fn test_ts2882_regular_import_still_emits_ts2307() {
    let diagnostics = compile_imports_and_get_diagnostics(
        r#"import { foo } from 'nonexistent-module';"#,
        CheckerOptions::default(),
    );
    assert!(
        has_error(&diagnostics, 2307) || has_error(&diagnostics, 2792),
        "Should emit TS2307 or TS2792 for regular import.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2882),
        "Should NOT emit TS2882 for regular import (only for side-effect imports).\nActual errors: {diagnostics:#?}"
    );
}

/// Node.js built-in modules should NOT trigger TS2882 when using Node module resolution.
/// TSC resolves these via @types/node; we suppress them for known builtins.
#[test]
fn test_ts2882_node_builtin_suppressed() {
    let opts = CheckerOptions {
        module: tsz_common::common::ModuleKind::Node16,
        no_unchecked_side_effect_imports: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_imports_and_get_diagnostics(r#"import "fs";"#, opts);
    assert!(
        !has_error(&diagnostics, 2882),
        "Should NOT emit TS2882 for Node.js built-in 'fs'.\nActual: {diagnostics:?}"
    );
    assert!(
        !has_error(&diagnostics, 2307),
        "Should NOT emit TS2307 for Node.js built-in 'fs'.\nActual: {diagnostics:?}"
    );
}

/// Node.js built-in modules with node: prefix should also be suppressed.
#[test]
fn test_ts2882_node_builtin_prefix_suppressed() {
    let opts = CheckerOptions {
        module: tsz_common::common::ModuleKind::Node16,
        no_unchecked_side_effect_imports: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_imports_and_get_diagnostics(r#"import "node:fs";"#, opts);
    assert!(
        !has_error(&diagnostics, 2882),
        "Should NOT emit TS2882 for Node.js built-in 'node:fs'.\nActual: {diagnostics:?}"
    );
}

// TS7051: Parameter has a name but no type. Did you mean 'arg0: string'?
// TS7006: Parameter 'x' implicitly has an 'any' type.

/// TS7051 should fire for type-keyword parameter names without type annotation.
/// From: noImplicitAnyNamelessParameter.ts
#[test]
fn test_ts7051_type_keyword_name() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function f(string, number) { }
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7051),
        "Should emit TS7051 for type-keyword parameter name.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 7006),
        "Should NOT emit TS7006 for type-keyword parameter name (should be TS7051).\nActual errors: {diagnostics:#?}"
    );
}

/// TS7051 should fire for rest parameters with type-keyword names.
/// e.g., `function f(...string)` should suggest `...args: string[]`
#[test]
fn test_ts7051_rest_type_keyword_name() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function f(...string) { }
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7051),
        "Should emit TS7051 for rest param with type-keyword name.\nActual errors: {diagnostics:#?}"
    );
    let ts7051 = diagnostics
        .iter()
        .find(|(code, _)| *code == 7051)
        .expect("expected TS7051 diagnostic");
    assert!(
        ts7051.1.contains("arg0: string[]"),
        "Rest TS7051 should suggest an array type, got: {ts7051:?}"
    );
}

/// TS7051 should fire for uppercase-starting parameter names.
/// e.g., `function f(MyType)` looks like a missing type annotation.
#[test]
fn test_ts7051_uppercase_name() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function f(MyType) { }
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7051),
        "Should emit TS7051 for uppercase parameter name.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 7006),
        "Should NOT emit TS7006 for uppercase parameter name (should be TS7051).\nActual errors: {diagnostics:#?}"
    );
}

/// TS7051 should NOT fire (and TS7006 SHOULD fire) for parameters with modifiers.
/// e.g., `constructor(public A)` - the modifier makes it clear A is the parameter name.
/// From: ParameterList4.ts, ParameterList5.ts, ParameterList6.ts
#[test]
fn test_ts7006_not_ts7051_with_modifier() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
class C {
    constructor(public A) { }
}
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7006),
        "Should emit TS7006 for modified parameter 'A'.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 7051),
        "Should NOT emit TS7051 when parameter has modifier.\nActual errors: {diagnostics:#?}"
    );
}

/// TS7006 should fire for lowercase parameter names without contextual type.
/// This verifies we don't regress on the basic case.
#[test]
fn test_ts7006_basic_untyped_parameter() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function f(x) { }
        ",
        opts,
    );
    assert!(
        has_error(&diagnostics, 7006),
        "Should emit TS7006 for untyped parameter 'x'.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_ts7006_reserved_word_parameter_in_generator_strict_mode() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        "function*foo(yield) {}",
        CheckerOptions {
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        has_error(&diagnostics, 1212),
        "Expected strict-mode reserved-word diagnostic for generator parameter.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        has_error(&diagnostics, 7006),
        "Expected TS7006 alongside strict-mode reserved-word diagnostic.\nActual errors: {diagnostics:#?}"
    );
}

/// TS7006 should NOT fire when parameter has explicit type annotation.
#[test]
fn test_no_ts7006_with_type_annotation() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function f(x: number) { }
        ",
        opts,
    );
    assert!(
        !has_error(&diagnostics, 7006),
        "Should NOT emit TS7006 for typed parameter.\nActual errors: {diagnostics:#?}"
    );
}

/// TS7006 should NOT fire when noImplicitAny is disabled.
#[test]
fn test_no_ts7006_without_no_implicit_any() {
    let opts = CheckerOptions {
        no_implicit_any: false,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function f(x) { }
        ",
        opts,
    );
    assert!(
        !has_error(&diagnostics, 7006),
        "Should NOT emit TS7006 when noImplicitAny is off.\nActual errors: {diagnostics:#?}"
    );
}

/// Tagged template expressions should contextually type substitutions.
/// From: taggedTemplateContextualTyping1.ts
#[test]
fn test_tagged_template_contextual_typing() {
    let opts = CheckerOptions {
        no_implicit_any: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_and_get_diagnostics_with_options(
        r"
function tag(strs: TemplateStringsArray, f: (x: number) => void) { }
tag `${ x => x }`;
        ",
        opts,
    );
    let relevant: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code != 2318)
        .cloned()
        .collect();
    assert!(
        !has_error(&relevant, 7006),
        "Should NOT emit TS7006 - 'x' should be contextually typed from tag parameter.\nActual errors: {relevant:#?}"
    );
}

#[test]
fn test_tagged_template_generic_literal_argument_uses_call_inference() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
function someGenerics9<T>(strs: TemplateStringsArray, a: T, b: T, c: T): T {
    return null as any;
}
var a9a = someGenerics9 `${ "" }${ 0 }${ [] }`;
var a9a: {};
        "#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2345
                && message
                    .contains("Argument of type '0' is not assignable to parameter of type '\"\"'")
        }),
        "Expected tagged-template TS2345 to match normal generic call inference. Actual: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2403
                && message.contains("Variable 'a9a' must be of type 'string'")
                && !message.contains("\"\" | 0")
        }),
        "Expected tagged-template result display to widen to string. Actual: {diagnostics:#?}"
    );
}

#[test]
fn test_tagged_template_generic_object_union_result_is_widened() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
function someGenerics9<T>(strs: TemplateStringsArray, a: T, b: T, c: T): T {
    return null as any;
}
var a9e = someGenerics9 `${ undefined }${ { x: 6, z: new Date() } }${ { x: 6, y: "" } }`;
var a9e: {};
        "#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2403
                && message.contains("Variable 'a9e' must be of type")
                && message.contains("x: number")
                && message.contains("z: Date")
                && message.contains("z?: undefined")
                && message.contains("y: string")
                && message.contains("y?: undefined")
                && !message.contains("x: 6")
                && !message.contains("y: \"\"")
        }),
        "Expected tagged-template object union result display to widen literals. Actual: {diagnostics:#?}"
    );
}

/// Mixed-arity generic overloads on a tagged template tag must select the
/// matching overload by arity for contextual typing of substitution
/// expressions, the same way regular call expressions do. Previously the
/// tagged-template path called `get_contextual_signature` without an arity,
/// which returned `None` for mixed-arity overload sets and forced a
/// signature-less single-pass that left the type parameter `T` un-inferred,
/// producing a spurious TS2345 on later substitutions.
///
/// From: parenthesizedContexualTyping3.ts
#[test]
fn test_tagged_template_overload_arity_selects_signature_for_inference() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
function tempFun<T>(tempStrs: TemplateStringsArray, g: (x: T) => T, x: T): T;
function tempFun<T>(tempStrs: TemplateStringsArray, g: (x: T) => T, h: (y: T) => T, x: T): T;
function tempFun<T>(tempStrs: TemplateStringsArray, g: (x: T) => T, x: T): T {
    return g(x);
}

var a = tempFun `${ x => x }  ${ 10 }`;
var b = tempFun `${ (x => x) }  ${ 10 }`;
var c = tempFun `${ ((x => x)) } ${ 10 }`;
var d = tempFun `${ x => x } ${ x => x } ${ 10 }`;
var e = tempFun `${ x => x } ${ (x => x) } ${ 10 }`;
var f = tempFun `${ x => x } ${ ((x => x)) } ${ 10 }`;
var g = tempFun `${ (x => x) } ${ (((x => x))) } ${ 10 }`;
var h = tempFun `${ (x => x) } ${ (((x => x))) } ${ undefined }`;
        "#,
    );

    assert!(
        !has_error(&diagnostics, 2345),
        "Tagged template against mixed-arity generic overloads should select \
         the correct overload by arity and infer T from later substitutions, \
         producing no TS2345.\nActual errors: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 7006),
        "Substitution arrow parameters should be contextually typed from the \
         arity-matched overload, so no TS7006 should fire.\nActual errors: {diagnostics:#?}"
    );
}

#[test]
fn test_tagged_template_generic_unresolved_type_params_display_as_unknown() {
    let diagnostics = compile_and_get_diagnostics_with_lib(
        r#"
function someGenerics4<T, U>(strs: TemplateStringsArray, n: T, f: (x: U) => void) { }
someGenerics4 `${ null }${ null }`;

function someGenerics7<A, B, C>(strs: TemplateStringsArray, a: (a: A) => A, b: (b: B) => B, c: (c: C) => C) { }
function someGenerics8<T>(strs: TemplateStringsArray, n: T): T { return n; }
var x = someGenerics8 `${ someGenerics7 }`;
x `${ null }${ null }${ null }`;
        "#,
    );

    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2345
                && message
                    .contains("Argument of type 'null' is not assignable to parameter of type '(x: unknown) => void'")
        }),
        "Expected unresolved callback parameter type to display as unknown. Actual: {diagnostics:#?}"
    );
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2345
                && message
                    .contains("Argument of type 'null' is not assignable to parameter of type '(a: unknown) => unknown'")
        }),
        "Expected returned generic tag parameter type to display as unknown. Actual: {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|(_, message)| !message.contains("(x: U)") && !message.contains("(a: A)")),
        "Tagged-template generic diagnostics should not leak unresolved type parameter names. Actual: {diagnostics:#?}"
    );
}
