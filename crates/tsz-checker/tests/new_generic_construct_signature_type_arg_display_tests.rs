//! Regression tests for type-argument display of `new G<...>()` when the
//! constructor is a generic construct signature whose return type is already a
//! generic application (e.g. `interface C { new <K, V>(): G<K, V> }`).
//!
//! Structural rule: when a generic construct signature is invoked with explicit
//! type arguments (`new G<string, number>()`), the result type is already the
//! instantiated application `G<string, number>`. The checker must not wrap it in
//! a second display alias `G<string, number><string, number>`. tsc prints the
//! single application `G<string, number>`.
//!
//! The fix lives in `get_type_of_new_expression_with_request`: the synthesized
//! `G<args>` display alias is only created for bare references that would
//! otherwise omit their arguments (e.g. `new D<string>()` for a class whose
//! pre-instantiated construct signature dropped the args), never for a type that
//! is already a generic application.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn strict() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        strict_null_checks: true,
        strict_function_types: true,
        ..CheckerOptions::default()
    }
}

/// Reported shape: a two-parameter generic construct signature. The source type
/// must render as `Foo<string, number>`, not `Foo<string, number><string,
/// number>`.
#[test]
fn new_generic_construct_signature_does_not_double_print_type_args() {
    let source = r#"
interface Foo<K, V> { k: K; v: V }
interface FooCtor { new <K, V>(): Foo<K, V> }
declare const Foo: FooCtor;

const f = new Foo<string, number>();
const bad: Foo<string, string> = f;
"#;
    let diagnostics = check_source(source, "test.ts", strict());
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2322)
        .expect("expected TS2322 assigning Foo<string, number> to Foo<string, string>");
    assert!(
        diag.message_text.contains(
            "Type 'Foo<string, number>' is not assignable to type 'Foo<string, string>'."
        ),
        "should print the single application form, got: {}",
        diag.message_text
    );
    assert!(
        !diag
            .message_text
            .contains("Foo<string, number><string, number>"),
        "type arguments must not be printed twice, got: {}",
        diag.message_text
    );
}

/// The bug must not depend on the spelling of the construct signature's type
/// parameters: a single parameter named `T` reproduces the same way.
#[test]
fn new_generic_construct_signature_renamed_single_param_no_double_print() {
    let source = r#"
interface Bar<T> { x: T }
interface BarCtor { new <T>(): Bar<T> }
declare const Bar: BarCtor;

const b = new Bar<number>();
const bad: Bar<string> = b;
"#;
    let diagnostics = check_source(source, "test.ts", strict());
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2322)
        .expect("expected TS2322 assigning Bar<number> to Bar<string>");
    assert!(
        diag.message_text
            .contains("Type 'Bar<number>' is not assignable to type 'Bar<string>'."),
        "should print the single application form, got: {}",
        diag.message_text
    );
    assert!(
        !diag.message_text.contains("Bar<number><number>"),
        "type arguments must not be printed twice, got: {}",
        diag.message_text
    );
}

/// Three type parameters with different names (`P`, `Q`, `R`) — proves the fix
/// is arity- and name-independent.
#[test]
fn new_generic_construct_signature_three_params_no_double_print() {
    let source = r#"
interface Triple<P, Q, R> { p: P; q: Q; r: R }
interface TripleCtor { new <P, Q, R>(): Triple<P, Q, R> }
declare const Triple: TripleCtor;

const t = new Triple<number, string, boolean>();
const bad: Triple<string, string, boolean> = t;
"#;
    let diagnostics = check_source(source, "test.ts", strict());
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2322)
        .expect("expected TS2322 for the Triple mismatch");
    assert!(
        diag.message_text
            .contains("Type 'Triple<number, string, boolean>' is not assignable to type 'Triple<string, string, boolean>'."),
        "should print the single application form, got: {}",
        diag.message_text
    );
    assert!(
        !diag.message_text.contains(">>") && !diag.message_text.contains("><"),
        "type arguments must not be printed twice, got: {}",
        diag.message_text
    );
}

/// Preservation case: a generic *class* invoked with explicit type arguments
/// (`new D<string>()`) must still display as `D<string>` — the display alias is
/// still synthesized for the bare-reference path the fix deliberately preserves.
#[test]
fn new_generic_class_still_displays_explicit_type_args() {
    let source = r#"
class D<T> { x!: T }

const d = new D<string>();
const bad: D<number> = d;
"#;
    let diagnostics = check_source(source, "test.ts", strict());
    let diag = diagnostics
        .iter()
        .find(|diag| diag.code == 2322)
        .expect("expected TS2322 assigning D<string> to D<number>");
    assert!(
        diag.message_text
            .contains("Type 'D<string>' is not assignable to type 'D<number>'."),
        "generic class new-expression should still display its explicit type args, got: {}",
        diag.message_text
    );
    assert!(
        !diag.message_text.contains("D<string><string>"),
        "type arguments must not be printed twice, got: {}",
        diag.message_text
    );
}

/// Negative case: an exactly-matching assignment must remain clean — the fix is
/// display-only and must not change the underlying instantiated type, which is a
/// structurally correct `Foo<string, number>`.
#[test]
fn new_generic_construct_signature_exact_match_has_no_diagnostic() {
    let source = r#"
interface Foo<K, V> { k: K; v: V }
interface FooCtor { new <K, V>(): Foo<K, V> }
declare const Foo: FooCtor;

const f = new Foo<string, number>();
const ok: Foo<string, number> = f;
"#;
    let diagnostics = check_source(source, "test.ts", strict());
    assert!(
        !diagnostics.iter().any(|diag| diag.code == 2322),
        "matching instantiation must not produce a TS2322, got: {diagnostics:?}"
    );
}
