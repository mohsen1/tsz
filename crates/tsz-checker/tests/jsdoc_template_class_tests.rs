//! Tests for JSDoc @template tag support on JS class declarations.
//!
//! Verifies that @template type parameters on JS classes are recognized
//! and used for generic type checking, matching tsc behavior.

use crate::test_utils::check_js_source_diagnostics;
use tsz_checker::context::CheckerOptions;
use tsz_checker::query_boundaries::common::PropertyAccessResult;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::{TypeId, TypeInterner};

fn symbol_property_type_strings(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
    names: &[&str],
    property_name: &str,
) -> Vec<String> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);

    names
        .iter()
        .map(|name| {
            let sym_id = binder
                .file_locals
                .get(name)
                .unwrap_or_else(|| panic!("missing symbol {name}"));
            let symbol_type = checker.get_type_of_symbol(sym_id);
            let property_type =
                match checker.resolve_property_access_with_env(symbol_type, property_name) {
                    PropertyAccessResult::Success { type_id, .. } => type_id,
                    _ => TypeId::ERROR,
                };
            checker.format_type(property_type)
        })
        .collect()
}

fn symbol_construct_signature_snapshot(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
    name: &str,
) -> Vec<(usize, Vec<String>)> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);

    let sym_id = binder
        .file_locals
        .get(name)
        .unwrap_or_else(|| panic!("missing symbol {name}"));
    let type_id = checker.get_type_of_symbol(sym_id);
    tsz_solver::type_queries::get_construct_signatures(checker.ctx.types, type_id)
        .unwrap_or_default()
        .into_iter()
        .map(|sig| {
            let params = sig
                .params
                .into_iter()
                .map(|param| checker.format_type(param.type_id))
                .collect();
            (sig.type_params.len(), params)
        })
        .collect()
}

fn class_instance_property_type_string(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
    class_name: &str,
    property_name: &str,
) -> String {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);

    let sym_id = binder
        .file_locals
        .get(class_name)
        .unwrap_or_else(|| panic!("missing symbol {class_name}"));
    let _ = checker.get_type_of_symbol(sym_id);
    let instance_type = checker
        .ctx
        .symbol_instance_types
        .get(&sym_id)
        .copied()
        .unwrap_or(TypeId::ERROR);
    let property_type = match checker.resolve_property_access_with_env(instance_type, property_name)
    {
        PropertyAccessResult::Success { type_id, .. } => type_id,
        _ => TypeId::ERROR,
    };
    checker.format_type(property_type)
}

fn instantiated_constructor_return_property_type_string(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
    class_name: &str,
    type_args: &[TypeId],
    property_name: &str,
) -> String {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);

    let sym_id = binder
        .file_locals
        .get(class_name)
        .unwrap_or_else(|| panic!("missing symbol {class_name}"));
    let type_id = checker.get_type_of_symbol(sym_id);
    let construct_sig =
        tsz_solver::type_queries::get_construct_signatures(checker.ctx.types, type_id)
            .and_then(|sigs| sigs.first().cloned())
            .unwrap_or_else(|| panic!("missing construct signature for {class_name}"));
    let instantiated = checker.instantiate_signature(&construct_sig, type_args);
    let property_type =
        match checker.resolve_property_access_with_env(instantiated.return_type, property_name) {
            PropertyAccessResult::Success { type_id, .. } => type_id,
            _ => TypeId::ERROR,
        };
    checker.format_type(property_type)
}

fn first_construct_signature_param_is_type_parameter(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
    class_name: &str,
) -> bool {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);

    let sym_id = binder
        .file_locals
        .get(class_name)
        .unwrap_or_else(|| panic!("missing symbol {class_name}"));
    let type_id = checker.get_type_of_symbol(sym_id);
    let Some(construct_sig) =
        tsz_solver::type_queries::get_construct_signatures(checker.ctx.types, type_id)
            .and_then(|sigs| sigs.first().cloned())
    else {
        return false;
    };
    let Some(first_param) = construct_sig.params.first() else {
        return false;
    };
    tsz_solver::type_queries::get_type_parameter_info(checker.ctx.types, first_param.type_id)
        .is_some()
}

fn resolve_new_result_property_type_string(
    source: &str,
    file_name: &str,
    options: CheckerOptions,
    class_name: &str,
    arg_types: &[TypeId],
    property_name: &str,
) -> String {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.check_source_file(root);

    let sym_id = binder
        .file_locals
        .get(class_name)
        .unwrap_or_else(|| panic!("missing symbol {class_name}"));
    let constructor_type = checker.get_type_of_symbol(sym_id);
    let return_type =
        match checker.resolve_new_with_checker_adapter(constructor_type, arg_types, false, None) {
            tsz_checker::query_boundaries::common::CallResult::Success(return_type) => return_type,
            other => panic!("resolve_new_with_checker_adapter did not succeed: {other:?}"),
        };
    let property_type = match checker.resolve_property_access_with_env(return_type, property_name) {
        PropertyAccessResult::Success { type_id, .. } => type_id,
        _ => TypeId::ERROR,
    };
    checker.format_type(property_type)
}

/// @template T on a class makes it generic — constructor infers T from argument.
/// Assigning incompatible generic instances should produce TS2322.
#[test]
fn test_jsdoc_template_class_type_mismatch() {
    let source = r#"
/** @template T */
class Foo {
    /** @param {T} x */
    constructor(x) {
        this.a = x;
    }
}
var f = new Foo(1);
var g = new Foo(false);
f.a = g.a;
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert!(
        ts2322 >= 1,
        "Expected TS2322 for assigning boolean to number via @template class, got codes: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @template T on a class — accessing this.prop should not produce TS2339.
/// The constructor's this.a = x should define property 'a' on the instance type.
#[test]
fn test_jsdoc_template_class_no_false_ts2339() {
    let source = r#"
/** @template T */
class Box {
    /** @param {T} val */
    constructor(val) {
        this.value = val;
    }
}
var b = new Box("hello");
b.value;
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2339 = diagnostics.iter().filter(|d| d.code == 2339).count();
    assert_eq!(
        ts2339,
        0,
        "Expected no TS2339 for property access on @template class instance, got codes: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// @template T on a class — multiple type parameters should all be in scope.
#[test]
fn test_jsdoc_template_class_multiple_type_params() {
    let source = r#"
/**
 * @template K
 * @template V
 */
class Pair {
    /**
     * @param {K} key
     * @param {V} val
     */
    constructor(key, val) {
        this.key = key;
        this.val = val;
    }
}
var p = new Pair("name", 42);
p.key;
p.val;
"#;
    let diagnostics = check_js_source_diagnostics(source);
    let ts2339 = diagnostics.iter().filter(|d| d.code == 2339).count();
    assert_eq!(
        ts2339,
        0,
        "Expected no TS2339 for multi-param @template class, got codes: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// Non-JS class with syntax type params should NOT be affected by JSDoc @template.
/// This test ensures we don't break TS files.
#[test]
fn test_ts_class_with_syntax_type_params_unaffected() {
    use crate::test_utils::check_source_diagnostics;
    let source = r#"
class Box<T> {
    value: T;
    constructor(val: T) {
        this.value = val;
    }
}
var b = new Box(1);
var c = new Box("hello");
b.value = c.value;
"#;
    let diagnostics = check_source_diagnostics(source);
    let ts2322 = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert!(
        ts2322 >= 1,
        "Expected TS2322 for incompatible generic assignment in TS class, got codes: {:?}",
        diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

#[test]
fn test_jsdoc_template_class_property_types_specialize_per_new_call() {
    let source = r#"
/** @template T */
class Foo {
    /** @param {T} x */
    constructor(x) {
        this.a = x;
    }
}
var f = new Foo(1);
var g = new Foo(false);
"#;

    let rendered = symbol_property_type_strings(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            ..CheckerOptions::default()
        },
        &["f", "g"],
        "a",
    );
    assert!(
        rendered[0].contains("number"),
        "expected f.a to be specialized to number, got {rendered:?}"
    );
    assert!(
        rendered[1].contains("boolean"),
        "expected g.a to be specialized to boolean, got {rendered:?}"
    );
}

#[test]
fn test_ts_generic_class_property_types_specialize_per_new_call_baseline() {
    let source = r#"
class Box<T> {
    value: T;
    constructor(value: T) {
        this.value = value;
    }
}
var n = new Box(1);
var b = new Box(false);
"#;

    let rendered = symbol_property_type_strings(
        source,
        "test.ts",
        CheckerOptions::default(),
        &["n", "b"],
        "value",
    );
    assert!(
        rendered[0].contains("number"),
        "expected n.value to be specialized to number, got {rendered:?}"
    );
    assert!(
        rendered[1].contains("boolean"),
        "expected b.value to be specialized to boolean, got {rendered:?}"
    );
}

#[test]
fn test_jsdoc_template_class_constructor_signature_is_generic() {
    let source = r#"
/** @template T */
class Foo {
    /** @param {T} x */
    constructor(x) {
        this.a = x;
    }
}
"#;

    let snapshot = symbol_construct_signature_snapshot(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            ..CheckerOptions::default()
        },
        "Foo",
    );
    assert!(
        snapshot.iter().any(|(type_param_count, params)| {
            *type_param_count > 0 && params.iter().any(|param| param.contains('T'))
        }),
        "expected the JSDoc template class constructor to stay generic, got {snapshot:?}"
    );
}

#[test]
fn test_jsdoc_template_class_raw_instance_property_keeps_type_parameter() {
    let source = r#"
/** @template T */
class Foo {
    /** @param {T} x */
    constructor(x) {
        this.a = x;
    }
}
"#;

    let rendered = class_instance_property_type_string(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            ..CheckerOptions::default()
        },
        "Foo",
        "a",
    );
    assert!(
        rendered.contains('T'),
        "expected the raw instance property to preserve the class template parameter, got {rendered}"
    );
}

#[test]
fn test_jsdoc_template_class_instantiated_constructor_return_specializes() {
    let source = r#"
/** @template T */
class Foo {
    /** @param {T} x */
    constructor(x) {
        this.a = x;
    }
}
"#;

    let rendered = instantiated_constructor_return_property_type_string(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            ..CheckerOptions::default()
        },
        "Foo",
        &[TypeId::NUMBER],
        "a",
    );
    assert!(
        rendered.contains("number"),
        "expected instantiated constructor return type to specialize property a to number, got {rendered}"
    );
}

#[test]
fn test_jsdoc_template_class_constructor_param_uses_template_type_param() {
    let source = r#"
/** @template T */
class Foo {
    /** @param {T} x */
    constructor(x) {
        this.a = x;
    }
}
"#;

    assert!(
        first_construct_signature_param_is_type_parameter(
            source,
            "test.js",
            CheckerOptions {
                check_js: true,
                ..CheckerOptions::default()
            },
            "Foo",
        ),
        "expected the first JSDoc constructor param to resolve to a type parameter"
    );
}

#[test]
fn test_jsdoc_template_class_direct_new_resolution_specializes() {
    let source = r#"
/** @template T */
class Foo {
    /** @param {T} x */
    constructor(x) {
        this.a = x;
    }
}
"#;

    let rendered = resolve_new_result_property_type_string(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            ..CheckerOptions::default()
        },
        "Foo",
        &[TypeId::NUMBER],
        "a",
    );
    assert!(
        rendered.contains("number"),
        "expected direct new resolution to specialize property a to number, got {rendered}"
    );
}
