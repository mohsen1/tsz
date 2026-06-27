//! Regression for #14945: `instanceof` of a generic global (`Map`/`Set`) or a
//! generic class inside a loop must narrow to the all-`any` instantiation
//! (`Map<any, any>`), not the bare generic interface (`Map<K, V>`).
//!
//! Outside a loop the `node_types` fast path already produces `Map<any, any>`;
//! inside a loop the flow fixed-point reaches the symbol-based instance-type
//! extraction with the constructor expression still typed as `error`, so the
//! INTERFACE+VARIABLE / CLASS branches must fill the interface's type
//! parameters with `any`. `tsc` narrows `x instanceof Map` to `Map<any, any>`.
//!
//! Binder names are varied (Map/Set/user `Box<T>`) so the fix cannot key on any
//! identifier.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_default_lib_files};
use tsz_common::common::ScriptTarget;

fn codes(source: &str) -> Vec<(u32, String)> {
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

fn assert_no_2345(source: &str, label: &str) {
    let diags = codes(source);
    assert!(
        !diags.iter().any(|(code, _)| *code == 2345),
        "{label}: instanceof-in-loop must narrow the generic instance to `<any>` \
         (no false TS2345). Got: {diags:#?}"
    );
}

#[test]
fn instanceof_map_in_loop_narrows_to_any_args() {
    assert_no_2345(
        r#"
export {};
function f(value: unknown, entries: [string, number][]) {
    for (const [k, v] of entries) {
        if (value instanceof Map) {
            value.set(k, v);
        }
    }
}
"#,
        "Map in for-of loop",
    );
}

#[test]
fn instanceof_set_in_loop_narrows_to_any_args() {
    assert_no_2345(
        r#"
export {};
function f(value: unknown, xs: number[]) {
    for (const x of xs) {
        if (value instanceof Set) {
            value.add(x);
        }
    }
}
"#,
        "Set in for-of loop",
    );
}

#[test]
fn instanceof_user_generic_class_in_loop_narrows_to_any_args() {
    assert_no_2345(
        r#"
export {};
class Box<T> {
    constructor(public v: T) {}
    set(x: T) {}
}
function f(value: unknown, xs: number[]) {
    for (const x of xs) {
        if (value instanceof Box) {
            value.set(x);
        }
    }
}
"#,
        "user generic class in loop",
    );
}

#[test]
fn instanceof_no_loop_control_still_clean() {
    // The no-loop path already narrowed to Map<any, any> via the fast path;
    // it must stay clean.
    assert_no_2345(
        r#"
export {};
function g(value: unknown) {
    if (value instanceof Map) {
        value.set(1, 2);
    }
}
"#,
        "no-loop control",
    );
}

#[test]
fn instanceof_union_source_member_still_errors() {
    // Guard: a concrete union-member source must NOT be widened to `Set<any>`;
    // adding a string to `Set<number>` must still report TS2345.
    let diags = codes(
        r#"
export {};
function neg(value: Set<number> | string) {
    if (value instanceof Set) {
        value.add("nope");
    }
}
"#,
    );
    assert!(
        diags.iter().any(|(code, _)| *code == 2345),
        "union-source `Set<number>` must keep its element type (adding a string errors TS2345). \
         Got: {diags:#?}"
    );
}
