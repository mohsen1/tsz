//! Tests for TS2322/TS2345 source-type display when the source expression is a
//! `new expr()` call.
//!
//! **Structural rule**: When `new expr()` returns a type that has no named
//! identity (no display alias — e.g. an anonymous structural type, a type
//! parameter, or a plain primitive), show the actual result type in the error
//! message, not the constructor expression name. A variable/parameter name like
//! `ctor` is not a type name and must not appear as the diagnostic source type.
//!
//! When the result type IS a named class (or generic class with inferred args),
//! show the class name (or `ClassName<InferredArg>`), not the variable's name.
//!
//! See: <https://github.com/mohsen1/tsz/issues/6373>

use crate::test_utils::check_source_code_messages as diagnostics;

fn ts2322_messages(diags: &[(u32, String)]) -> Vec<&str> {
    diags
        .iter()
        .filter(|(c, _)| *c == 2322)
        .map(|(_, m)| m.as_str())
        .collect()
}

// ---------------------------------------------------------------------------
// Regression: constructor-variable name must NOT appear as source type (#6373)
// ---------------------------------------------------------------------------

/// `new ctor()` where `ctor` is a generic constructor parameter returning `T`.
/// Source type in TS2322 must be the type parameter `T`, not the variable `ctor`.
#[test]
fn ts2322_new_generic_ctor_param_shows_type_param_not_variable_name() {
    let diags = diagnostics(
        r#"
function create<T>(ctor: new () => T): string {
    return new ctor();
}
"#,
    );
    let msgs = ts2322_messages(&diags);
    assert!(!msgs.is_empty(), "Expected TS2322");
    for msg in &msgs {
        assert!(
            !msg.contains("'ctor'"),
            "Source type must not be the variable name 'ctor'; got: {msg}"
        );
        assert!(msg.contains("'T'"), "Source type should be 'T'; got: {msg}");
    }
}

/// Same rule applies when the parameter is named differently (not `ctor`).
/// Shows the structural rule is not hardcoded to the name "ctor".
#[test]
fn ts2322_new_generic_ctor_param_name_independent() {
    let diags = diagnostics(
        r#"
function build<V>(factory: new () => V): number {
    return new factory();
}
"#,
    );
    let msgs = ts2322_messages(&diags);
    assert!(!msgs.is_empty(), "Expected TS2322");
    for msg in &msgs {
        assert!(
            !msg.contains("'factory'"),
            "Source type must not be the variable name 'factory'; got: {msg}"
        );
        assert!(msg.contains("'V'"), "Source type should be 'V'; got: {msg}");
    }
}

/// When the constructor returns a concrete type (`string`), the result type is
/// shown in the error message, not the constructor parameter name.
#[test]
fn ts2322_new_concrete_returning_ctor_shows_return_type() {
    let diags = diagnostics(
        r#"
function wrap(maker: new () => string): number {
    return new maker();
}
"#,
    );
    let msgs = ts2322_messages(&diags);
    assert!(!msgs.is_empty(), "Expected TS2322");
    for msg in &msgs {
        assert!(
            !msg.contains("'maker'"),
            "Source type must not be the variable name 'maker'; got: {msg}"
        );
        assert!(
            msg.contains("'string'"),
            "Source type should be 'string' (the concrete return type); got: {msg}"
        );
    }
}

// ---------------------------------------------------------------------------
// Named class constructor: should still show the class name
// ---------------------------------------------------------------------------

/// `new Foo()` where `Foo` is a declared class — should show `'Foo'` as source
/// type, not regress to the structural object shape.
#[test]
fn ts2322_new_named_class_still_shows_class_name() {
    let diags = diagnostics(
        r#"
class Foo {}
function wrap(): number {
    return new Foo();
}
"#,
    );
    let msgs = ts2322_messages(&diags);
    assert!(!msgs.is_empty(), "Expected TS2322");
    for msg in &msgs {
        assert!(
            msg.contains("'Foo'"),
            "Source type should be 'Foo' for a named class; got: {msg}"
        );
    }
}

/// Regression for #6991: generic type arguments that are class instance types
/// must display as `ClassName`, not `typeof ClassName`.
#[test]
fn ts2322_generic_class_instance_type_args_do_not_display_typeof() {
    let diags = diagnostics(
        r#"
class Animal {
  name: string = "";
}

class Dog extends Animal {
  bark(): void {}
}

interface Box<T> {
  value: T;
}

declare let animalBox: Box<Animal>;
declare let dogBox: Box<Dog>;

dogBox = animalBox;
"#,
    );
    let msgs = ts2322_messages(&diags);
    assert_eq!(msgs.len(), 1, "expected one TS2322, got: {diags:?}");
    let msg = msgs[0];
    assert!(
        msg.contains("Type 'Box<Animal>' is not assignable to type 'Box<Dog>'."),
        "generic class instance args should display without typeof, got: {msg}"
    );
    assert!(
        !msg.contains("Box<typeof Animal>") && !msg.contains("Box<typeof Dog>"),
        "instance type arguments must not display as typeof class constructors, got: {msg}"
    );
}
