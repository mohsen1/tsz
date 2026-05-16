use crate::test_utils::check_source_diagnostics;

fn diagnostic_messages<'a>(
    diagnostics: impl Iterator<Item = &'a crate::diagnostics::Diagnostic>,
) -> Vec<&'a str> {
    diagnostics
        .map(|diagnostic| diagnostic.message_text.as_str())
        .collect()
}

/// Direct single-parameter generic calls should preserve wrapped returns even
/// when the caller has an outer type parameter with the same name.
#[test]
fn generic_call_direct_inference_preserves_wrapped_return() {
    let diags = check_source_diagnostics(
        r#"
interface Box<T> { value: T }
interface Shape { id: number }
declare function wrap<T>(value: T): Box<T>;

function forward<T extends Shape>(input: T): Box<T> {
    return wrap(input);
}

function renamed<U extends Shape>(input: U): Box<U> {
    return wrap(input);
}
"#,
    );

    let errors: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected wrapped direct generic return to preserve caller type params, got: {:?}",
        diagnostic_messages(errors.iter().copied())
    );
}

/// `Promise.resolve<T>(value: T): Promise<Awaited<T>>` is another direct
/// single-parameter generic shape; the wrapped return must still await to the
/// caller's concrete type parameter.
#[test]
fn generic_call_direct_inference_preserves_promise_resolve_return() {
    let diags = check_source_diagnostics(
        r#"
/// <reference lib="es2015.promise" />

interface Shape { id: number }

async function forward<T extends Shape>(input: T): Promise<T> {
    return await Promise.resolve(input);
}

async function renamed<U extends Shape>(input: U): Promise<U> {
    return await Promise.resolve(input);
}
"#,
    );

    let errors: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected Promise.resolve direct generic return to preserve caller type params, got: {:?}",
        diagnostic_messages(errors.iter().copied())
    );
}

/// A direct union parameter such as `T | Wrapper<T>` can still infer `T`
/// directly from an argument that matches the bare `T` arm.
#[test]
fn generic_call_direct_union_parameter_preserves_wrapped_return() {
    let diags = check_source_diagnostics(
        r#"
interface Box<T> { value: T }
interface MaybeBox<T> { boxed: T }
interface Shape { id: number }
declare function wrap<T>(value: T | MaybeBox<T>): Box<T>;

function forward<T extends Shape>(input: T): Box<T> {
    return wrap(input);
}

function renamed<U extends Shape>(input: U): Box<U> {
    return wrap(input);
}
"#,
    );

    let errors: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected direct union generic parameter to preserve caller type params, got: {:?}",
        diagnostic_messages(errors.iter().copied())
    );
}

/// Wrapper-arm inference for `T | Wrapper<T>` still needs the full generic
/// inference path; the direct fast path only handles bare type-parameter args.
#[test]
fn generic_call_direct_union_parameter_falls_back_for_wrapper_arm() {
    let diags = check_source_diagnostics(
        r#"
interface Box<T> { value: T }
interface MaybeBox<T> { boxed: T }
interface Shape { id: number }
declare function wrap<T>(value: T | MaybeBox<T>): Box<T>;

function forward(input: MaybeBox<Shape>): Box<Shape> {
    return wrap(input);
}
"#,
    );

    let errors: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected wrapper-arm union inference to fall back to full inference, got: {:?}",
        diagnostic_messages(errors.iter().copied())
    );
}
