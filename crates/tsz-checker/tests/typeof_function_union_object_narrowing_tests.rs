//! `typeof x === "function"` narrowing of a union that contains the bare
//! non-primitive `object` (or an empty `{}` shape) must keep that constituent as
//! `Function`, not drop it.
//!
//! Regression for #14324 (arktype): function values inhabit `object`, so
//! `typeof x === "function"` narrows `object` to the global `Function`. The
//! single-type path did this, but the union-member path dropped the `object`
//! constituent, so `object | symbol` narrowed to `never` and a member access on
//! the result produced a spurious TS2339.

use tsz_checker::test_utils::check_source_code_messages;

fn ts2339_count(source: &str) -> usize {
    check_source_code_messages(source)
        .into_iter()
        .filter(|(code, _)| *code == 2339)
        .count()
}

#[test]
fn typeof_function_narrows_object_in_union_no_ts2339() {
    let src = r#"
function a(value: object | symbol) {
  if (typeof value === "function") return value.name;
  return "";
}
"#;
    assert_eq!(
        ts2339_count(src),
        0,
        "`typeof value === 'function'` on `object | symbol` must narrow `object` to `Function`, not `never`"
    );
}

#[test]
fn typeof_function_narrows_empty_object_in_union_no_ts2339() {
    let src = r#"
function a(value: {} | string) {
  if (typeof value === "function") return value.name;
  return "";
}
"#;
    assert_eq!(
        ts2339_count(src),
        0,
        "an empty object-literal constituent must narrow to `Function` under a typeof-function guard"
    );
}

// Binder-name variation: the fix is structural (object includes function
// values), not keyed on any identifier.
#[test]
fn typeof_function_narrows_object_in_union_renamed_binder_no_ts2339() {
    let src = r#"
function inspect(candidate: object | number) {
  if (typeof candidate === "function") return candidate.name;
  return "";
}
"#;
    assert_eq!(
        ts2339_count(src),
        0,
        "renamed binder: narrowing `object` to `Function` must not depend on the parameter name"
    );
}

// Negative control: a concrete non-callable object shape is NOT a function, so
// the function branch is `never` and the access still errors (resolution must
// not blanket-narrow every object-ish member to `Function`).
#[test]
fn typeof_function_concrete_object_shape_in_union_still_ts2339() {
    let src = r#"
function a(value: { x: number } | symbol) {
  if (typeof value === "function") return value.name;
  return "";
}
"#;
    assert!(
        ts2339_count(src) >= 1,
        "a concrete non-callable object constituent leaves the function branch `never`, so the access must still emit TS2339"
    );
}
