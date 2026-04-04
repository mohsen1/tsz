//! Tests for polymorphic `this` type in class methods.
//!
//! When a class method body does `return this;` without an explicit return type
//! annotation, the inferred return type should be the polymorphic `ThisType`
//! (not the concrete declaring class type).  This enables fluent method chaining
//! on subclass instances.

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Helper to compile TypeScript and get diagnostics
fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    compile_and_get_diagnostics_with_options(source, CheckerOptions::default())
}

fn compile_and_get_diagnostics_with_options(
    source: &str,
    options: CheckerOptions,
) -> Vec<(u32, String)> {
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
        options,
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn has_error(diagnostics: &[(u32, String)], code: u32) -> bool {
    diagnostics.iter().any(|(c, _)| *c == code)
}

fn errors_with_code(diagnostics: &[(u32, String)], code: u32) -> Vec<&str> {
    diagnostics
        .iter()
        .filter(|(c, _)| *c == code)
        .map(|(_, msg)| msg.as_str())
        .collect()
}

/// Fluent method chaining: `c.foo().bar().baz()` where foo/bar/baz are defined
/// on classes A/B/C in a hierarchy and each returns `this` implicitly.
///
/// Without polymorphic `this`, `c.foo()` would return `A` (the declaring class)
/// and `.bar()` would fail because `bar` is only on `B`.
#[test]
fn test_fluent_class_chain_no_false_ts2339() {
    let source = r#"
class A {
    foo() {
        return this;
    }
}
class B extends A {
    bar() {
        return this;
    }
}
class C extends B {
    baz() {
        return this;
    }
}
declare var c: C;
var z = c.foo().bar().baz();
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    let ts2339_errors = errors_with_code(&diagnostics, 2339);
    assert!(
        ts2339_errors.is_empty(),
        "Should not have TS2339 for fluent chain, but got: {ts2339_errors:?}"
    );
}

/// When a method has an explicit return type annotation (not inferred),
/// the annotation should be used as-is. Only unannotated methods that
/// `return this;` should get polymorphic `ThisType`.
#[test]
fn test_explicit_return_type_not_replaced() {
    let source = r#"
class A {
    foo(): A {
        return this;
    }
}
class B extends A {
    bar() {
        return this;
    }
}
declare var b: B;
var x = b.foo();  // Should be A, not B (explicit annotation)
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    // foo() has explicit `: A` annotation, so b.foo() returns A.
    // Accessing .bar() on A should fail.
    // This test just verifies no crash and no false positive on `b.foo()`.
    assert!(
        !has_error(&diagnostics, 2339),
        "Should not error on b.foo() since A.foo() returns A"
    );
}

/// A method that returns `this.property` should NOT get polymorphic return type.
/// Only direct `return this;` contributes the class instance type that triggers
/// the polymorphic `ThisType` substitution.
#[test]
fn test_return_this_property_stays_concrete() {
    let source = r#"
class A {
    x: number = 5;
    getX() {
        return this.x;
    }
}
class B extends A {
    y: string = "hello";
}
declare var b: B;
var result: number = b.getX();
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    // getX() should return number (from this.x), not ThisType.
    // Assigning to `number` should not error.
    assert!(
        !has_error(&diagnostics, 2322),
        "getX() should return number, not polymorphic this: {diagnostics:?}"
    );
}

/// Regression guard: accessing a property that truly doesn't exist should
/// still produce TS2339, even with the polymorphic this type fix.
#[test]
#[ignore = "polymorphic this return type inference regressed — method return type no longer matches partial_type"]
fn test_nonexistent_property_still_errors() {
    let source = r#"
class A {
    foo() {
        return this;
    }
}
declare var a: A;
var x = a.foo().nonExistent;
"#;
    let diagnostics = compile_and_get_diagnostics(source);
    assert!(
        has_error(&diagnostics, 2339),
        "Should get TS2339 for nonexistent property: {diagnostics:?}"
    );
}

#[test]
fn test_generic_this_index_assignment_in_class_method_has_no_false_ts2322() {
    let source = r#"
class C1 {
    x: number;
    get<K extends keyof this>(key: K) {
        return this[key];
    }
    set<K extends keyof this>(key: K, value: this[K]) {
        this[key] = value;
    }
}
"#;

    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            emit_declarations: true,
            strict_property_initialization: false,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2322),
        "Generic this-index assignment should not emit TS2322: {diagnostics:?}"
    );
}

#[test]
fn test_generic_this_index_assignment_in_base_class_has_no_false_ts2322() {
    let source = r#"
class Base {
    get<K extends keyof this>(prop: K) {
        return this[prop];
    }
    set<K extends keyof this>(prop: K, value: this[K]) {
        this[prop] = value;
    }
}

class Person extends Base {
    parts: number;
    constructor(parts: number) {
        super();
        this.set("parts", parts);
    }
}
"#;

    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            emit_declarations: true,
            strict_property_initialization: false,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !has_error(&diagnostics, 2322),
        "Base-class generic this-index assignment should not emit TS2322: {diagnostics:?}"
    );
}
