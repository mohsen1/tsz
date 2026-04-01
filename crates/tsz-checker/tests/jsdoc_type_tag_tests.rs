//! Tests for JSDoc @type tag on class properties, function declarations,
//! object literal properties, and braceless @type syntax.
//!
//! Verifies that @type annotations are used for type checking initializers
//! and that @type function types provide parameter types in JS files.

use rustc_hash::FxHashSet;
use std::path::Path;
use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::context::LibContext;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

struct Diag {
    code: u32,
}

fn load_lib_files_for_test() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lib_roots = [
        manifest_dir.join("../../crates/tsz-core/src/lib-assets"),
        manifest_dir.join("../../crates/tsz-core/src/lib-assets-stripped"),
        manifest_dir.join("../../TypeScript/src/lib"),
    ];
    let lib_names = [
        "es5.d.ts",
        "es2015.d.ts",
        "es2015.core.d.ts",
        "es2015.collection.d.ts",
        "es2015.iterable.d.ts",
        "es2015.generator.d.ts",
        "es2015.promise.d.ts",
        "es2015.proxy.d.ts",
        "es2015.reflect.d.ts",
        "es2015.symbol.d.ts",
        "es2015.symbol.wellknown.d.ts",
        "dom.d.ts",
        "dom.generated.d.ts",
        "dom.iterable.d.ts",
        "esnext.d.ts",
    ];

    let mut lib_files = Vec::new();
    let mut seen_files = FxHashSet::default();
    for file_name in lib_names {
        for root in &lib_roots {
            let lib_path = root.join(file_name);
            if lib_path.exists()
                && let Ok(content) = std::fs::read_to_string(&lib_path)
            {
                if !seen_files.insert(file_name.to_string()) {
                    break;
                }
                lib_files.push(Arc::new(LibFile::from_source(file_name.to_string(), content)));
                break;
            }
        }
    }

    lib_files
}

fn check_js_internal(source: &str, with_libs: bool) -> Vec<Diag> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        strict: true,
        ..CheckerOptions::default()
    };

    let mut parser = ParserState::new("test.js".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let lib_files = if with_libs {
        load_lib_files_for_test()
    } else {
        Vec::new()
    };

    let mut binder = BinderState::new();
    if lib_files.is_empty() {
        binder.bind_source_file(parser.get_arena(), root);
    } else {
        binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);
    }

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.js".to_string(),
        options,
    );

    if lib_files.is_empty() {
        checker.ctx.set_lib_contexts(Vec::new());
    } else {
        let lib_contexts: Vec<LibContext> = lib_files
            .iter()
            .map(|lib| LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
        checker.ctx.set_actual_lib_file_count(lib_files.len());
    }

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d: &Diagnostic| Diag { code: d.code })
        .collect()
}

fn check_js(source: &str) -> Vec<Diag> {
    check_js_internal(source, false)
}

fn check_js_with_libs(source: &str) -> Vec<Diag> {
    check_js_internal(source, true)
}

/// @type {boolean} on class field with incompatible initializer → TS2322
#[test]
fn test_jsdoc_type_on_class_field_initializer_mismatch() {
    let source = r#"
class A {
    /** @type {boolean} */
    foo = 3
}
"#;
    let diagnostics = check_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert!(
        ts2322 >= 1,
        "Expected TS2322 for number assigned to boolean @type field, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @type {string} on class field with compatible initializer → no error
#[test]
fn test_jsdoc_type_on_class_field_compatible_initializer() {
    let source = r#"
class A {
    /** @type {string} */
    foo = "hello"
}
"#;
    let diagnostics = check_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322,
        0,
        "Expected no TS2322 for string assigned to string @type field, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// function(string): void Closure syntax is parsed correctly
#[test]
fn test_jsdoc_function_closure_syntax_contextual_typing() {
    let source = r#"
/** @type {function(string): void} */
var f = function(value) {
    value = 1
}
"#;
    let diagnostics = check_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert!(
        ts2322 >= 1,
        "Expected TS2322 for number assigned to string parameter from @type function, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Broad @type {Function} should not suppress implicit-any on function expressions.
#[test]
fn test_jsdoc_type_function_object_does_not_contextually_type_params() {
    let source = r#"
/** @type {Function} */
const x = (a) => a + 1;
x(1);
"#;
    let diagnostics = check_js(source);
    let ts7006 = diagnostics.iter().filter(|d| d.code == 7006).count();
    assert!(
        ts7006 >= 1,
        "Expected TS7006 for broad @type {{Function}} on function expression, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Broad @type {function} should not suppress implicit-any on function expressions.
#[test]
fn test_jsdoc_type_lowercase_function_does_not_contextually_type_params() {
    let source = r#"
/** @type {function} */
const y = (a) => a + 1;
y(1);
"#;
    let diagnostics = check_js(source);
    let ts7006 = diagnostics.iter().filter(|d| d.code == 7006).count();
    assert!(
        ts7006 >= 1,
        "Expected TS7006 for broad @type {{function}} on function expression, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @type function type on function declaration provides parameter types
#[test]
fn test_jsdoc_type_on_function_declaration_provides_param_types() {
    let source = r#"
/** @type {(s: string) => void} */
function g(s) {
    s = 1
}
"#;
    let diagnostics = check_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert!(
        ts2322 >= 1,
        "Expected TS2322 for number assigned to string parameter from @type on function decl, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// =============================================================================
// JSDoc @type on object literal properties
// =============================================================================

/// @type {string|undefined} on object property uses declared type, not initializer
#[test]
fn test_jsdoc_type_on_object_property_overrides_initializer_type() {
    let source = r##"
var obj = {
    /** @type {string|undefined} */
    foo: undefined
};
obj.foo = 'hello';
"##;
    let diagnostics = check_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322,
        0,
        "Expected no TS2322 when assigning string to @type {{string|undefined}} property, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @type {string|undefined} on object property: incompatible initializer → TS2322
#[test]
fn test_jsdoc_type_on_object_property_checks_initializer() {
    let source = r#"
var obj = {
    /** @type {string|undefined} */
    bar: 42
};
"#;
    let diagnostics = check_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert!(
        ts2322 >= 1,
        "Expected TS2322 for number initializer on @type {{string|undefined}} property, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Concrete function @type on object-literal function properties should still contextually type parameters.
#[test]
fn test_jsdoc_type_on_object_function_property_provides_callable_context() {
    let source = r##"
const obj = {
    /** @type {function(number): number} */
    method1: (n1) => {
        n1 = "42";
        return 1;
    },
};
"##;
    let diagnostics = check_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert!(
        ts2322 >= 1,
        "Expected TS2322 for string assigned to number parameter under @type {{function(number): number}}, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @type {"a"} literal on object property: literal value is compatible
#[test]
fn test_jsdoc_type_literal_on_object_property_preserves_literal() {
    let source = r##"
var obj = {
    /** @type {"a"} */
    a: "a"
};
"##;
    let diagnostics = check_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322,
        0,
        "Expected no TS2322 for literal \"a\" assigned to @type {{\"a\"}} property, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

// =============================================================================
// Braceless @type support
// =============================================================================

/// Braceless @type string on variable declaration
#[test]
fn test_braceless_jsdoc_type_simple_type() {
    let source = r#"
/** @type string */
var x = 42;
"#;
    let diagnostics = check_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert!(
        ts2322 >= 1,
        "Expected TS2322 for number assigned to braceless @type string, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Braceless @type with compatible value → no error
#[test]
fn test_braceless_jsdoc_type_compatible() {
    let source = r#"
/** @type number */
var x = 42;
"#;
    let diagnostics = check_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322,
        0,
        "Expected no TS2322 for number assigned to braceless @type number, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Braceless JSDoc intersections should contextually re-check object literal initializers.
#[test]
fn test_braceless_jsdoc_intersection_object_initializer_reports_ts2322() {
    let source = r#"
/** @type ({ type: 'foo' } | { type: 'bar' }) & { prop: number } */
const obj = { type: "other", prop: 10 };
"#;
    let diagnostics = check_js(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert!(
        ts2322 >= 1,
        "Expected TS2322 for incompatible discriminant under braceless JSDoc intersection, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Broad Function/function tags should still report TS7006 in the full mixed JSDoc file.
#[test]
fn test_jsdoc_type_tag_broad_function_full_file_regression() {
    let source = r##"
// @ts-check
/** @type {String} */
var S = "hello world";

/** @type {number} */
var n = 10;

/** @type {*} */
var anyT = 2;
anyT = "hello";

/** @type {?} */
var anyT1 = 2;
anyT1 = "hi";

/** @type {Function} */
const x = (a) => a + 1;
x(1);

/** @type {function} */
const y = (a) => a + 1;
y(1);

/** @type {function (number)} */
const x1 = (a) => a + 1;
x1(0);

/** @type {function (number): number} */
const x2 = (a) => a + 1;
x2(0);

/**
 * @type {object}
 */
var props = {};

/**
 * @type {Object}
 */
var props = {};
"##;
    let diagnostics = check_js_with_libs(source);
    let ts7006 = diagnostics.iter().filter(|d| d.code == 7006).count();
    let ts2403 = diagnostics.iter().filter(|d| d.code == 2403).count();
    assert!(
        ts7006 >= 2,
        "Expected two TS7006 diagnostics in the mixed JSDoc file, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
    assert!(
        ts2403 >= 1,
        "Expected TS2403 for @type {{object}} vs @type {{Object}} redeclaration, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn test_jsdoc_object_and_object_interface_redeclaration_emit_ts2403() {
    let source = r#"
// @ts-check
/** @type {object} */
var props = {};

/** @type {Object} */
var props = {};
"#;
    let diagnostics = check_js_with_libs(source);
    let ts2403 = diagnostics.iter().filter(|d| d.code == 2403).count();
    assert!(
        ts2403 >= 1,
        "Expected TS2403 for @type {{object}} vs @type {{Object}} redeclaration, got: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}
