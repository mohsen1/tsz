//! Regression tests for back-substituting the fixed inference result into
//! contextual callback-parameter types on the construct (`new`) path.
//!
//! When a generic construct signature's type parameter cannot be fixed from the
//! arguments (no explicit type argument, callback body not yet analyzed), `tsc`
//! defaults the parameter (e.g. `T = unknown`) before pushing the contextual
//! parameter types into a callback argument. Previously tsz left the raw
//! inference placeholder (`__infer_*`) in the callback's parameter type, so
//! reading that parameter as a value produced a tsz-only `TS2322` and leaked the
//! internal placeholder into the user-facing message.

use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::test_utils::{check_source_diagnostics, diagnostic_codes as codes_of};

/// The original witness, reproduced lib-free with the same executor shape as
/// the real `Promise` constructor: `(value: T | PromiseLike<T>) => void`. With
/// no contextual type `T` defaults to `unknown`, so reading `res` out against
/// `(value: unknown) => void` is clean (rather than leaking `__infer_*`).
#[test]
fn promise_shaped_executor_resolve_assigned_out_is_clean() {
    let source = r#"
interface ThenableLike<T> { then(cb: (value: T) => void): void; }
interface ThenableCtor {
    new <T>(executor: (res: (value: T | ThenableLike<T>) => void) => void): ThenableLike<T>;
}
declare const Thenable: ThenableCtor;
let resolve: (value: unknown) => void;
new Thenable((res) => { resolve = res });
"#;
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "promise-shaped executor reading `res` out must be clean: {diagnostics:?}"
    );
}

/// Not Promise-specific: a user-defined generic class whose constructor takes a
/// callback that exposes the type parameter must default `T` the same way.
#[test]
fn user_generic_class_executor_callback_param_read_out_is_clean() {
    let source = r#"
class Box<T> {
    constructor(executor: (res: (value: T) => void) => void) {}
}
let resolve: (value: unknown) => void;
new Box((res) => { resolve = res });
"#;
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "user generic class executor reading `res` out must be clean: {diagnostics:?}"
    );
}

/// The internal `__infer_*` placeholder atom must never surface in a
/// user-facing diagnostic message, regardless of whether a diagnostic fires.
#[test]
fn construct_callback_never_leaks_infer_placeholder_into_messages() {
    let source = r#"
class Box<T> {
    constructor(executor: (res: (value: T) => void) => void) {}
}
let resolve: (value: unknown) => void;
new Box((res) => { resolve = res });
"#;
    for diag in check_source_diagnostics(source) {
        assert!(
            !diag.message_text.contains("__infer"),
            "diagnostic leaked an inference placeholder: {diag:?}"
        );
    }
}

/// Back-substitution must not mask real errors: when the argument genuinely
/// fixes `T` to a concrete type, body errors against that type still fire.
#[test]
fn concrete_construct_inference_still_reports_body_error() {
    let source = r#"
class Holder<T> {
    constructor(cb: (x: T) => void, seed: T) {}
}
new Holder((x) => { x.toFixed(); }, "world");
"#;
    let codes = codes_of(&check_source_diagnostics(source));
    assert!(
        codes.contains(&diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE),
        "expected TS2339 for `.toFixed()` on string-inferred T: {codes:?}"
    );
}

/// An explicit type argument on a generic class fixes `T` directly; the
/// callback parameter is concrete and assigning it out against the matching
/// declared type is clean (the implicit two-pass inference path is bypassed).
#[test]
fn explicit_type_argument_construct_callback_is_clean() {
    let source = r#"
class Box<T> {
    constructor(executor: (res: (value: T) => void) => void) {}
}
let resolve: (value: number) => void;
new Box<number>((res) => { resolve = res });
"#;
    let diagnostics = check_source_diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "explicit `new Box<number>` executor must be clean: {diagnostics:?}"
    );
}
