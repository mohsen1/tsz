//! Regression tests for #14489 and its nested-wrapper residual.
//!
//! Structural rule: when a conditional's **extends-type is a generic wrapper
//! *alias*** that carries the `infer` (`X extends AB<infer U> ? U : never`,
//! where `type AB<T> = Promise<T[]>`) and the **check-type is the wrapper's
//! expanded structural form** (`Promise<number[]>`), the alias pattern must be
//! reduced to its application form (`Promise<(infer U)[]>`) before structural
//! infer matching so `U` binds. `tsc` infers `U = number`; tsz previously
//! collapsed the conditional to its false branch (`never`) — a false `TS2322`
//! at the use site.
//!
//! The single-level reduction landed in #14496. These tests additionally cover
//! the *repeated-same-wrapper* nesting that #14496 left as a residual
//! (`Nest<T> = Promise<Promise<T[]>>`), which previously bound `U` one wrapper
//! level early (`U = Promise<number[]>`) because the positional base-subtype
//! shortcuts in the `Application` arm accepted a wrapper-alias base. Refusing
//! that shortcut for wrapper-alias bases routes the pattern through the
//! head-only alias reduction so `U` binds correctly at every level.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
        &libs,
    )
    .iter()
    .map(|diagnostic| (diagnostic.code, diagnostic.message_text.clone()))
    .collect()
}

fn assert_clean(source: &str, label: &str) {
    let diagnostics = diagnostics(source);
    assert!(
        diagnostics.is_empty(),
        "[{label}] expected clean, got {diagnostics:?}"
    );
}

fn assert_ts2322(source: &str, label: &str) {
    let diagnostics = diagnostics(source);
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2322),
        "[{label}] expected TS2322, got {diagnostics:?}"
    );
}

#[test]
fn wrapper_alias_extends_over_expanded_source_binds_infer() {
    // Core repro: extends-type is the alias `AB<infer U>`; check-type is the
    // expanded `Promise<number[]>`. `U` must bind to `number`.
    let source = r#"
type AB<T> = Promise<T[]>;
type Ex<X> = X extends AB<infer U> ? U : never;
type R = Ex<Promise<number[]>>;
const a: R = 7;
"#;
    assert_clean(source, "AB<infer U> over Promise<number[]>");
}

#[test]
fn wrapper_alias_extends_is_structural_under_renamed_binders() {
    // Same shape, every binder renamed — the fix must be structural, not keyed
    // on `AB`/`U`/`X`.
    let source = r#"
type Wrapper<Inner> = Promise<Inner[]>;
type Unwrap<Src> = Src extends Wrapper<infer Out> ? Out : never;
type Result = Unwrap<Promise<string[]>>;
const a: Result = "x";
"#;
    assert_clean(source, "renamed wrapper-alias binders");
}

#[test]
fn wrapper_alias_object_payload_shape_binds_infer() {
    // Wrapper body is `Promise<{ payload: T[] }>` — a structural object inside
    // the wrapped interface still reduces to bind `U`.
    let source = r#"
type ObjW<T> = Promise<{ payload: T[] }>;
type Ex<X> = X extends ObjW<infer U> ? U : never;
type R = Ex<Promise<{ payload: boolean[] }>>;
const a: R = true;
"#;
    assert_clean(source, "Promise<{ payload: T[] }> wrapper alias");
}

#[test]
fn non_promise_wrapper_alias_binds_infer() {
    // The reduction is not Promise-specific: `Set<T[]>` reduces the same way.
    let source = r#"
type SetW<T> = Set<T[]>;
type Ex<X> = X extends SetW<infer U> ? U : never;
type R = Ex<Set<number[]>>;
const a: R = 5;
"#;
    assert_clean(source, "Set<T[]> wrapper alias");
}

#[test]
fn nested_repeated_wrapper_alias_binds_infer_all_levels() {
    // Repeated-same-wrapper nesting must not stop one level early: the alias
    // base (`Nest`) must be peeled to `Promise<Promise<(infer U)[]>>` rather
    // than rebuilt over the source's outer argument (which would bind
    // `U = Promise<number[]>`).
    let source = r#"
type Nest<T> = Promise<Promise<T[]>>;
type Ex<X> = X extends Nest<infer U> ? U : never;
type R = Ex<Promise<Promise<number[]>>>;
const a: R = 9;
"#;
    assert_clean(source, "nested Promise<Promise<T[]>> wrapper alias");
}

#[test]
fn nested_repeated_wrapper_alias_no_inner_array_binds_infer() {
    // Same repeated-wrapper nesting without the inner array layer.
    let source = r#"
type Nest<T> = Promise<Promise<T>>;
type Ex<X> = X extends Nest<infer U> ? U : never;
type R = Ex<Promise<Promise<number>>>;
const a: R = 9;
"#;
    assert_clean(source, "nested Promise<Promise<T>> wrapper alias");
}

#[test]
fn wrapper_alias_extends_over_returntype_source_binds_infer() {
    // Check-type arrives as `ReturnType<typeof asyncFn>` (a reduced
    // `Promise<number[]>`), exercising the structural / display-alias recovery
    // path into the same reduction.
    let source = r#"
async function asyncFn(): Promise<number[]> { return [1]; }
type ABp<T> = Promise<T[]>;
type Ex<X> = X extends ABp<infer U> ? U : never;
type R = Ex<ReturnType<typeof asyncFn>>;
const a: R = 3;
"#;
    assert_clean(source, "ReturnType<typeof asyncFn> over wrapper alias");
}

#[test]
fn wrapper_alias_extends_false_branch_when_source_does_not_match() {
    // Negative control: a source that genuinely does not match the wrapper
    // keeps the false branch, so assigning an out-of-domain value still errors.
    let source = r#"
type AB<T> = Promise<T[]>;
type Ex<X> = X extends AB<infer U> ? U : "fallback";
type R = Ex<string>;
const a: R = "other";
"#;
    assert_ts2322(source, "non-matching source keeps false branch");
}
