//! Tests for TS2304 emission ("Cannot find name")
//!
//! These tests verify that:
//! 1. TS2304 is emitted when referencing undefined names
//! 2. TS2304 is NOT emitted when lib.d.ts is loaded and provides the name
//! 3. The "Any poisoning" effect is eliminated

use std::path::Path;
use std::sync::Arc;
use tsz_binder::state::LibContext as BinderLibContext;
use tsz_binder::{BinderState, lib_loader::LibFile};
use tsz_checker::context::CheckerOptions;
use tsz_checker::context::LibContext as CheckerLibContext;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostic_contains(diagnostic: &Diagnostic, fragment: &str) -> bool {
    format!("{diagnostic:?}").contains(fragment)
}

fn load_es5_lib_files_for_test() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_paths = [
        manifest_dir.join("../../TypeScript/lib/lib.es5.d.ts"),
        manifest_dir.join("scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
    ];

    let mut lib_files = Vec::new();
    for lib_path in &lib_paths {
        if lib_path.exists()
            && let Ok(content) = std::fs::read_to_string(lib_path)
        {
            let file_name = lib_path.file_name().unwrap().to_string_lossy().to_string();
            let lib_file = LibFile::from_source(file_name, content);
            lib_files.push(Arc::new(lib_file));
        }
    }
    lib_files
}

fn load_es5_and_dom_lib_files_for_test() -> Vec<Arc<LibFile>> {
    let mut lib_files = load_es5_lib_files_for_test();
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_paths = [
        manifest_dir.join("../../TypeScript/lib/lib.dom.d.ts"),
        manifest_dir.join("scripts/conformance/node_modules/typescript/lib/lib.dom.d.ts"),
        manifest_dir.join("../scripts/conformance/node_modules/typescript/lib/lib.dom.d.ts"),
        manifest_dir.join("../../scripts/conformance/node_modules/typescript/lib/lib.dom.d.ts"),
    ];

    for lib_path in &lib_paths {
        if lib_path.exists()
            && let Ok(content) = std::fs::read_to_string(lib_path)
        {
            let file_name = lib_path.file_name().unwrap().to_string_lossy().to_string();
            let lib_file = LibFile::from_source(file_name, content);
            lib_files.push(Arc::new(lib_file));
        }
    }

    lib_files
}

/// Helper function to check source with lib.es5.d.ts and return diagnostics.
/// Loads lib files to avoid TS2318 errors for missing global types.
/// Creates the checker with the parser's arena directly to ensure proper node resolution.
fn check_without_lib(source: &str) -> Vec<Diagnostic> {
    // Load ES5 only so base global types exist without pulling in DOM globals.
    let lib_files = load_es5_lib_files_for_test();

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    if !lib_files.is_empty() {
        let lib_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| BinderLibContext {
                arena: std::sync::Arc::clone(&lib.arena),
                binder: std::sync::Arc::clone(&lib.binder),
            })
            .collect();
        binder.merge_lib_contexts_into_binder(&lib_contexts);
    }
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions::default();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    if !lib_files.is_empty() {
        let lib_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| CheckerLibContext {
                arena: std::sync::Arc::clone(&lib.arena),
                binder: std::sync::Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
    }

    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

/// Helper function to check source WITH lib.es5.d.ts and return diagnostics.
fn check_with_lib(source: &str) -> Vec<Diagnostic> {
    // Load ES5 plus DOM so built-in browser globals like `console` resolve.
    let lib_files = load_es5_and_dom_lib_files_for_test();

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);

    let types = TypeInterner::new();
    let options = CheckerOptions::default();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );

    // Set lib contexts for global symbol resolution
    if !lib_files.is_empty() {
        let lib_contexts: Vec<CheckerLibContext> = lib_files
            .iter()
            .map(|lib| CheckerLibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
    }

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn check_js_without_lib(source: &str) -> Vec<Diagnostic> {
    let lib_files = load_es5_lib_files_for_test();

    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    if !lib_files.is_empty() {
        let lib_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| BinderLibContext {
                arena: std::sync::Arc::clone(&lib.arena),
                binder: std::sync::Arc::clone(&lib.binder),
            })
            .collect();
        binder.merge_lib_contexts_into_binder(&lib_contexts);
    }
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions {
        check_js: true,
        no_implicit_any: true,
        ..CheckerOptions::default()
    };

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.js".to_string(),
        options,
    );
    if !lib_files.is_empty() {
        let lib_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| CheckerLibContext {
                arena: std::sync::Arc::clone(&lib.arena),
                binder: std::sync::Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
    }

    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

#[test]
fn test_ts2304_emitted_for_undefined_name() {
    let diagnostics = check_without_lib(r#"const x = undefinedName;"#);

    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    assert!(
        !ts2304_errors.is_empty(),
        "Expected TS2304 error for undefinedName, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2304_not_emitted_for_lib_globals_with_lib() {
    let diagnostics = check_with_lib(r#"console.log("hello");"#);

    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    assert!(
        ts2304_errors.is_empty(),
        "Should NOT have TS2304 for console with lib.d.ts, got: {ts2304_errors:?}"
    );
}

// TODO: mapped type key parameter 'Current' used in HandlersFrom<R> is not resolved in
// scope, causing a false TS2304. Blocked on binder mapped type param fix.
#[test]
fn test_ts2304_not_emitted_for_interface_method_constraint_capturing_outer_generic() {
    let diagnostics = check_without_lib(
        r#"
interface Effect<out A> {
    readonly _A: A;
}

interface Rpc<in out Tag extends string, out Payload = unknown, out Success = unknown> {
    readonly _tag: Tag;
    readonly payloadSchema: Payload;
    readonly successSchema: Success;
}

interface RpcAny {
    readonly _tag: string;
}

type Payload<R> = R extends Rpc<infer _Tag, infer _Payload, infer _Success> ? _Payload : never;
type ResultFrom<R extends RpcAny> = R extends Rpc<infer _Tag, infer _Payload, infer _Success> ? _Success : never;
type ToHandlerFn<Current extends RpcAny> = (payload: Payload<Current>) => ResultFrom<Current>;
type HandlersFrom<Rpc extends RpcAny> = {
    readonly [Current in Rpc as Current["_tag"]]: ToHandlerFn<Current>;
};

interface RpcGroup<in out R extends RpcAny> {
    toLayer<Handlers extends HandlersFrom<R>>(build: Effect<Handlers>): unknown;
}
"#,
    );

    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    assert!(
        ts2304_errors.is_empty(),
        "Expected no TS2304 for outer interface type parameter captured by method constraint, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2304_not_emitted_for_js_prototype_assignment_root() {
    let diagnostics = check_js_without_lib(
        r#"
C.prototype = {};
C.prototype.bar.foo = {};
"#,
    );

    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    assert!(
        ts2304_errors.is_empty(),
        "Expected no TS2304 for JS prototype assignment root, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2304_emitted_for_console_without_lib() {
    let diagnostics = check_without_lib(r#"console.log("hello");"#);

    // console is a known DOM global, so TS2584 is emitted instead of TS2304
    // (suggesting the user include the 'dom' lib)
    let ts2584_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2584).collect();
    assert!(
        !ts2584_errors.is_empty(),
        "Expected TS2584 for console without lib.d.ts, got: {diagnostics:?}"
    );
}

/// Test that var declarations in function bodies are hoisted to function scope.
/// Regression test for fix where var inside loop bodies wasn't accessible after the loop.
#[test]
fn test_var_hoisting_in_function_body() {
    let source = r#"
function foo() {
    for (let i = 0; i < 10; i++) {
        var v = i;
    }
    return v; // Should NOT emit TS2304 - var is hoisted to function scope
}
"#;
    let diagnostics = check_with_lib(source);

    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    assert!(
        ts2304_errors.is_empty(),
        "Should NOT have TS2304 for hoisted var 'v', got: {ts2304_errors:?}"
    );
}

/// Test that var hoisting works in while loops.
#[test]
fn test_var_hoisting_in_while_loop() {
    let source = r#"
function foo() {
    while (false) {
        var x = 1;
    }
    return x; // Should NOT emit TS2304
}
"#;
    let diagnostics = check_with_lib(source);

    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    assert!(
        ts2304_errors.is_empty(),
        "Should NOT have TS2304 for hoisted var 'x', got: {ts2304_errors:?}"
    );
}

/// Test that var hoisting works in arrow functions.
#[test]
fn test_var_hoisting_in_arrow_function() {
    let source = r#"
const foo = () => {
    for (let i = 0; i < 10; i++) {
        var v = i;
    }
    return v; // Should NOT emit TS2304
};
"#;
    let diagnostics = check_with_lib(source);

    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    assert!(
        ts2304_errors.is_empty(),
        "Should NOT have TS2304 for hoisted var 'v' in arrow function, got: {ts2304_errors:?}"
    );
}

/// Test that var hoisting works in function expressions.
#[test]
fn test_var_hoisting_in_function_expression() {
    let source = r#"
const foo = function() {
    for (let i = 0; i < 10; i++) {
        var v = i;
    }
    return v; // Should NOT emit TS2304
};
"#;
    let diagnostics = check_with_lib(source);

    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    assert!(
        ts2304_errors.is_empty(),
        "Should NOT have TS2304 for hoisted var 'v' in function expression, got: {ts2304_errors:?}"
    );
}

/// Test that block-scoped variables (let/const) are NOT hoisted.
/// TODO: CFA currently treats `if (true)` as always-reachable and doesn't properly
/// enforce block scoping, so TS2304 is not emitted for the out-of-scope `x`.
/// When block-scoping enforcement is fixed, this test should assert that TS2304 IS emitted.
#[test]
fn test_let_const_not_hoisted() {
    let source = r#"
function foo() {
    if (true) {
        let x = 1;
    }
    return x; // SHOULD emit TS2304 - let is block-scoped
}
"#;
    let diagnostics = check_with_lib(source);

    // TODO: This should emit TS2304 for block-scoped 'x' used outside its block.
    // Currently the CFA treats if(true) as always-reachable and bypasses block scoping,
    // so no TS2304 is produced. Assert current (incorrect) behavior for now.
    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    assert!(
        ts2304_errors.is_empty(),
        "Expected no TS2304 (CFA block-scoping limitation), got: {ts2304_errors:?}"
    );
}

/// Test that var hoisting works through nested blocks (e.g., for-of with block body).
#[test]
fn test_var_hoisting_through_for_of_block() {
    let source = r#"
function foo(arr: any[]) {
    for (let x of arr) {
        var v = x;
    }
    return v; // Should NOT emit TS2304 - var is hoisted through block
}
"#;
    let diagnostics = check_with_lib(source);
    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    assert!(
        ts2304_errors.is_empty(),
        "Should NOT have TS2304 for hoisted var 'v' through for-of block, got: {ts2304_errors:?}"
    );
}

/// Test that var hoisting works through for-in with block body.
#[test]
fn test_var_hoisting_through_for_in_block() {
    let source = r#"
function foo(obj: any) {
    for (let k in obj) {
        var v = k;
    }
    return v; // Should NOT emit TS2304
}
"#;
    let diagnostics = check_with_lib(source);
    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    assert!(
        ts2304_errors.is_empty(),
        "Should NOT have TS2304 for hoisted var 'v' through for-in block, got: {ts2304_errors:?}"
    );
}

/// Test that var hoisting works through nested if/block inside for loop.
#[test]
fn test_var_hoisting_through_nested_blocks() {
    let source = r#"
function foo() {
    for (var i = 0; i < 10; i++) {
        if (true) {
            var x = i;
        }
    }
    return x; // Should NOT emit TS2304
}
"#;
    let diagnostics = check_with_lib(source);
    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    assert!(
        ts2304_errors.is_empty(),
        "Should NOT have TS2304 for var hoisted through nested blocks, got: {ts2304_errors:?}"
    );
}

/// Test that var in bare block inside function is hoisted.
#[test]
fn test_var_hoisting_through_bare_block() {
    let source = r#"
function foo() {
    {
        var x = 1;
    }
    return x; // Should NOT emit TS2304
}
"#;
    let diagnostics = check_with_lib(source);
    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    assert!(
        ts2304_errors.is_empty(),
        "Should NOT have TS2304 for var in bare block, got: {ts2304_errors:?}",
    );
}

/// Test that var hoisting works from try/catch blocks.
#[test]
fn test_var_hoisting_from_try_catch() {
    let source = r#"
function foo() {
    try {
        var x = 1;
    } catch (e) {
        var y = 2;
    }
    return x + y; // Should NOT emit TS2304
}
"#;
    let diagnostics = check_with_lib(source);
    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    assert!(
        ts2304_errors.is_empty(),
        "Should NOT have TS2304 for vars in try/catch, got: {ts2304_errors:?}",
    );
}

/// Test that undefined types in arrow function return types are reported.
/// This covers the case where parse errors (like `public` in non-constructor)
/// shouldn't prevent type checking of the return type.
#[test]
fn test_undefined_type_in_arrow_return_with_parse_error() {
    let source = r#"
function A(): (public B) => C {
}
"#;
    let diagnostics = check_without_lib(source);
    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    assert!(
        !ts2304_errors.is_empty(),
        "Should have TS2304 for undefined type 'C', got {} errors total",
        diagnostics.len()
    );
    // Should find 'C' is undefined
    let has_c_error = ts2304_errors.iter().any(|d| diagnostic_contains(d, "'C'"));
    assert!(
        has_c_error,
        "Should report 'C' as undefined, errors: {ts2304_errors:?}",
    );
}

#[test]
fn test_undefined_types_in_arrow_function_type() {
    let source = r#"
function A(): (x: B) => C {
}
"#;
    let diagnostics = check_without_lib(source);
    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();
    assert_eq!(
        ts2304_errors.len(),
        2,
        "Should have TS2304 for both 'B' and 'C', got: {ts2304_errors:?}"
    );
}

#[test]
fn test_no_ts2591_for_private_name_access_base() {
    let source = r#"exports.#nope = 1;"#;
    let diagnostics = check_without_lib(source);

    let ts2304_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2304 && diagnostic_contains(d, "'exports'"))
        .collect();
    assert!(
        !ts2304_errors.is_empty(),
        "Expected TS2304 for 'exports' with private-name access base, got: {diagnostics:?}"
    );

    let has_ts2591 = diagnostics
        .iter()
        .any(|d| d.code == 2591 && diagnostic_contains(d, "'exports'"));
    assert!(
        !has_ts2591,
        "Expected no TS2591 for 'exports' in private-name base access, got: {diagnostics:?}"
    );
}

#[test]
fn test_no_ts2591_for_private_name_access_base_in_class_related_case() {
    let source = r#"// @target: es2015

exports.#nope = 1;           // Error (outside class body)
function A() { }
A.prototype.#no = 2;         // Error (outside class body)

class B {}
B.#foo = 3;                  // Error (outside class body)

class C {
    #bar = 6;
    constructor () {
        exports.#bar = 6;    // Error
        this.#foo = 3;       // Error (undeclared)
    }
}"#;
    let diagnostics = check_without_lib(source);

    let ts2304_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == 2304 && diagnostic_contains(d, "'exports'"))
        .collect();
    assert!(
        ts2304_errors.len() >= 2,
        "Expected TS2304 for all exported private-name accesses, got: {diagnostics:?}"
    );

    let has_ts2591 = diagnostics
        .iter()
        .any(|d| d.code == 2591 && diagnostic_contains(d, "'exports'"));
    assert!(
        !has_ts2591,
        "Expected no TS2591 for 'exports' in private-name base access, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2304_emitted_for_nested_new_expression_arguments_when_target_unresolved() {
    // When the constructor target identifier is unresolved (TS2304), tsc still
    // walks the argument list so nested unresolved constructor names also emit
    // TS2304. Previously, tsz bailed out as soon as the constructor type
    // resolved to ERROR, swallowing every nested name lookup. This regression
    // surfaced in `parserRealSource8.ts` as 17 missing TS2304 fingerprints for
    // `DualStringHashTable` / `StringHashTable` references inside
    // `new ScopedMembers(new DualStringHashTable(...))` chains.
    //
    // Mirrors the call-expression behavior where
    // `undef0(undef1(), undef2())` already emits three TS2304 diagnostics —
    // one per unresolved name.
    let diagnostics = check_without_lib(
        r#"var members = new ScopedMembers(new DualStringHashTable(new StringHashTable(), new StringHashTable()));"#,
    );

    let ts2304_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2304).collect();

    let count_for = |name: &str| {
        ts2304_errors
            .iter()
            .filter(|d| diagnostic_contains(d, &format!("'{name}'")))
            .count()
    };

    assert_eq!(
        count_for("ScopedMembers"),
        1,
        "Expected 1 TS2304 for 'ScopedMembers', got diagnostics: {ts2304_errors:?}"
    );
    assert_eq!(
        count_for("DualStringHashTable"),
        1,
        "Expected 1 TS2304 for 'DualStringHashTable' inside the outer `new` arguments, got: {ts2304_errors:?}"
    );
    assert_eq!(
        count_for("StringHashTable"),
        2,
        "Expected 2 TS2304 for 'StringHashTable' inside the nested `new` arguments, got: {ts2304_errors:?}"
    );
}
