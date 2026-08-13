//! Void return exception (Lawyer layer, `AnyPropagationRules`): a function
//! returning any type `T` is assignable to a function type expecting
//! `void` — `tsc` treats the return value as intentionally discarded rather
//! than checking it structurally.
//!
//! All diagnostics oracle-verified against `typescript@7.0.2` (`--strict`).
//! Uses the real bundled `lib.d.ts` (via [`check_source_with_libs`] +
//! [`load_default_lib_files`]) rather than hand-rolled interface mocks, so
//! `Promise<void>` assignability is checked for real instead of only
//! reaching the "used as a value" placeholder diagnostic a mock `Promise`
//! interface would produce.

use tsz_checker::test_utils::{
    check_source_with_libs, diagnostic_codes, load_default_lib_files, strict_checker_options,
};

fn strict_codes_with_libs(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    diagnostic_codes(&check_source_with_libs(
        source,
        "test.ts",
        strict_checker_options(),
        &libs,
    ))
}

/// `() => T` is assignable to `() => void` for any concrete return type.
#[test]
fn callback_returning_value_assignable_to_void_return() {
    let codes = strict_codes_with_libs(
        r#"
function takesCallback(cb: () => void) {
    cb();
}
takesCallback(() => "hello");
takesCallback(() => 42);
takesCallback(() => ({ x: 1 }));
takesCallback(function () {
    return "ignored";
});
takesCallback(() => {
    return "ignored";
});
"#,
    );
    assert!(
        codes.is_empty(),
        "a callback returning any value is assignable to a `void`-returning \
         parameter type; got: {codes:?}"
    );
}

/// The void return exception is specific to `void`: it does not extend to
/// `undefined`, a concrete value type that still requires structural
/// assignability.
#[test]
fn callback_returning_string_not_assignable_to_undefined_return() {
    let codes = strict_codes_with_libs(
        r#"
type Callback = () => undefined;
const f: Callback = () => "hello";
"#,
    );
    assert!(
        codes.contains(&2322),
        "`() => string` must not be assignable to `() => undefined` — the \
         void return exception only applies to `void`, not `undefined`; \
         got: {codes:?}"
    );
}

/// The exception does not distribute into `Promise<void>`: an async
/// callback's resolved value is still checked structurally against
/// `Promise<void>`'s resolved type.
#[test]
fn async_callback_returning_promise_string_not_assignable_to_promise_void_return() {
    let codes = strict_codes_with_libs(
        r#"
type AsyncCallback = () => Promise<void>;
const f: AsyncCallback = () => Promise.resolve("hello");
"#,
    );
    assert!(
        codes.contains(&2322),
        "`() => Promise<string>` must not be assignable to \
         `() => Promise<void>` — the void return exception does not apply \
         inside a `Promise`'s resolved type; got: {codes:?}"
    );
}

/// The exception is directional: a `void`-returning function is NOT
/// assignable to a signature expecting a concrete return type.
#[test]
fn void_returning_callback_not_assignable_to_string_return() {
    let codes = strict_codes_with_libs(
        r#"
type StringCallback = () => string;
const f: StringCallback = () => {};
"#,
    );
    assert!(
        codes.contains(&2322),
        "`() => void` must not be assignable to `() => string` — the void \
         return exception is one-directional; got: {codes:?}"
    );
}

/// The exception applies to a method-shaped signature in an object type,
/// not just bare function types.
#[test]
fn interface_method_returning_value_assignable_to_void_method() {
    let codes = strict_codes_with_libs(
        r#"
interface VoidCallback {
    method(): void;
}
const impl_: VoidCallback = {
    method: () => "returns value but ignored",
};
"#,
    );
    assert!(
        codes.is_empty(),
        "an object literal's method returning a value is assignable to an \
         interface method typed `void`; got: {codes:?}"
    );
}

/// The exception applies element-wise inside an array of callbacks, each
/// independently returning a different (or no) value.
#[test]
fn array_of_mixed_return_callbacks_assignable_to_void_callback_array() {
    let codes = strict_codes_with_libs(
        r#"
const callbacks: Array<() => void> = [() => 1, () => "hello", () => ({ x: 1 })];
"#,
    );
    assert!(
        codes.is_empty(),
        "an array literal of callbacks with differing concrete return types \
         is assignable to `Array<() => void>`; got: {codes:?}"
    );
}
