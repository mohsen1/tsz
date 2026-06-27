//! `await` must unwrap a *structural* thenable regardless of how the `then`
//! member was declared. A `then(...)` declared as a **method** (object-literal
//! type, `type` alias, or inline object-literal value) lowers to a bare
//! `Function` shape, while a named `interface` lowers to a `Callable`; the
//! awaited-type extractor previously only recognized the `Callable` form, so the
//! method forms were left un-unwrapped and produced a false `TS2322`.
//!
//! Regression for #14813. Binder names are varied so the fix cannot rely on any
//! identifier/alias string.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_default_lib_files};
use tsz_common::common::ScriptTarget;

fn await_diagnostics(source: &str) -> Vec<(u32, String)> {
    let libs = load_default_lib_files();
    assert!(!libs.is_empty(), "default lib files must be available");
    check_source_with_libs_code_messages(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2022,
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
}

fn assert_no_2322(source: &str, label: &str) {
    let diags = await_diagnostics(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2322),
        "{label}: structural thenable must unwrap in `await` (no false TS2322). Got: {diags:#?}"
    );
}

#[test]
fn await_object_literal_value_thenable_unwraps() {
    // The inline object-literal value form: `then` is a method -> Function shape.
    assert_no_2322(
        r#"
export {};
async function run() {
    const value = await { then(onFulfil: (payload: number) => void) {} };
    const checked: number = value;
    return checked;
}
"#,
        "object-literal value thenable",
    );
}

#[test]
fn await_type_alias_method_thenable_unwraps() {
    assert_no_2322(
        r#"
export {};
type Settler = { then(onFulfil: (payload: number) => void): void };
declare const settler: Settler;
async function run() {
    const value = await settler;
    const checked: number = value;
    return checked;
}
"#,
        "type-alias (method) thenable",
    );
}

#[test]
fn await_type_alias_property_thenable_unwraps() {
    // Property-style `then` (already a function-typed property) must keep working.
    assert_no_2322(
        r#"
export {};
type Settler = { then: (onFulfil: (payload: number) => void) => void };
declare const settler: Settler;
async function run() {
    const value = await settler;
    const checked: number = value;
    return checked;
}
"#,
        "type-alias (property) thenable",
    );
}

#[test]
fn await_anonymous_object_type_annotation_thenable_unwraps() {
    assert_no_2322(
        r#"
export {};
declare const settler: { then(onFulfil: (payload: number) => void): void };
async function run() {
    const value = await settler;
    const checked: number = value;
    return checked;
}
"#,
        "anonymous object-type annotation thenable",
    );
}

#[test]
fn await_promise_of_alias_thenable_unwraps_recursively() {
    // `Promise<Th>` where `Th` is itself an alias-thenable must unwrap to the
    // inner payload, not stop at `Th`.
    assert_no_2322(
        r#"
export {};
type Settler = { then(onFulfil: (payload: number) => void): void };
declare const wrapped: Promise<Settler>;
async function run() {
    const value = await wrapped;
    const checked: number = value;
    return checked;
}
"#,
        "Promise<alias-thenable> recursive unwrap",
    );
}

#[test]
fn await_union_with_object_literal_thenable_unwraps_each_member() {
    assert_no_2322(
        r#"
export {};
declare const settler: { then(onFulfil: (payload: number) => void): void } | string;
async function run() {
    const value = await settler;
    const checked: number | string = value;
    return checked;
}
"#,
        "union member object-literal thenable",
    );
}

#[test]
fn await_named_interface_thenable_still_unwraps_control() {
    // Control: the already-working named-interface form must keep unwrapping.
    assert_no_2322(
        r#"
export {};
interface Settler { then(onFulfil: (payload: number) => void): void }
declare const settler: Settler;
async function run() {
    const value = await settler;
    const checked: number = value;
    return checked;
}
"#,
        "named-interface thenable control",
    );
}

#[test]
fn await_declared_class_instance_thenable_unwraps() {
    // A class-declared thenable: the instance type does NOT classify as an
    // `Object` promise shape, yet it exposes a callable `then(onfulfilled)` and
    // so `await` must unwrap it structurally (mirrors tsc's getAwaitedType).
    // Regression for the neverthrow `class Err implements PromiseLike` family.
    assert_no_2322(
        r#"
export {};
declare class Settler { then(onFulfil: (payload: number) => void): void }
async function run(make: () => Settler) {
    const value = await make();
    const checked: number = value;
    return checked;
}
"#,
        "declared-class instance thenable",
    );
}

#[test]
fn await_class_property_then_thenable_unwraps() {
    // Property-style `then` field on a class instance must also unwrap.
    assert_no_2322(
        r#"
export {};
declare class Settler { then: (onFulfil: (payload: number) => void) => void }
async function run(make: () => Settler) {
    const value = await make();
    const checked: number = value;
    return checked;
}
"#,
        "class property-`then` thenable",
    );
}

#[test]
fn await_class_thenable_arbitrary_names_unwraps() {
    // Same shape with different class / payload-param names: the fix must be
    // structural, not keyed on any identifier.
    assert_no_2322(
        r#"
export {};
declare class Deferred { then(cb: (outcome: string) => void): void }
async function go(produce: () => Deferred) {
    const result = await produce();
    const narrowed: string = result;
    return narrowed;
}
"#,
        "class thenable arbitrary binder names",
    );
}

#[test]
fn await_class_without_then_does_not_unwrap() {
    // Guard: a class instance with NO `then` member is not a thenable; awaiting
    // it yields the instance type, so assigning to `number` must still TS2322.
    let diags = await_diagnostics(
        r#"
export {};
declare class Plain { payload: number }
async function run(make: () => Plain) {
    const value = await make();
    const checked: number = value;
    return checked;
}
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "class without `then` must not unwrap. Got: {diags:#?}"
    );
}

#[test]
fn await_non_callable_then_member_does_not_unwrap() {
    // Guard: a `then` that is not callable is NOT a thenable; the operand type
    // must survive so assigning it to `number` still reports TS2322.
    let diags = await_diagnostics(
        r#"
export {};
declare const notThenable: { then: number };
async function run() {
    const value = await notThenable;
    const checked: number = value;
    return checked;
}
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "non-callable `then` must not be treated as a thenable. Got: {diags:#?}"
    );
}

#[test]
fn await_plain_object_does_not_unwrap() {
    // Guard: a plain object with no `then` must pass through unchanged.
    let diags = await_diagnostics(
        r#"
export {};
declare const plain: { payload: number };
async function run() {
    const value = await plain;
    const checked: number = value;
    return checked;
}
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2322),
        "plain object (no `then`) must not unwrap. Got: {diags:#?}"
    );
}
