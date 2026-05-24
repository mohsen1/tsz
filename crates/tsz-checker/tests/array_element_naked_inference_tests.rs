//! Regression tests for issue #9667.
//!
//! When a generic signature combines a `T[]` parameter (inferred from an array
//! literal) with a naked `T` parameter, tsc fixes `T` to the array element type
//! via `getCommonSupertype` (leftmost-wins) and reports a `TS2345` on the
//! conflicting naked argument. tsz previously unioned the array-element
//! candidate with the naked-arg candidate (`T = string | number`), masking the
//! error. These tests pin the corrected behaviour for several structurally
//! equivalent shapes (not just the reported spelling).

use tsz_checker::test_utils::check_source_code_messages as compile_and_get_diagnostics;

fn ts2345_count(source: &str) -> usize {
    compile_and_get_diagnostics(source)
        .iter()
        .filter(|(code, _)| *code == 2345)
        .count()
}

#[test]
fn multi_element_string_array_with_number_arg_reports_ts2345() {
    // Reported repro: T fixed to `string`, the `1` argument conflicts.
    let source = r#"
declare function f<T>(a: T[], b: T): void;
f(["a", "b"], 1);
"#;
    assert_eq!(
        ts2345_count(source),
        1,
        "multi-element array element type should fix T = string and reject the number argument"
    );
}

#[test]
fn multi_element_number_array_with_string_arg_reports_ts2345() {
    // Swap the element/argument types: T fixed to `number`, `"x"` conflicts.
    let source = r#"
declare function f<T>(a: T[], b: T): void;
f([1, 2], "x");
"#;
    assert_eq!(
        ts2345_count(source),
        1,
        "swapped element/argument types must also report TS2345"
    );
}

#[test]
fn three_element_array_with_number_arg_reports_ts2345() {
    // Element count is not the trigger; any multi-element array behaves the same.
    let source = r#"
declare function f<T>(a: T[], b: T): void;
f(["a", "b", "c"], 1);
"#;
    assert_eq!(
        ts2345_count(source),
        1,
        "three-element array must behave like the two-element case"
    );
}

#[test]
fn renamed_type_parameter_and_params_reports_ts2345() {
    // The rule is structural, not tied to the identifier names `T`/`a`/`b`.
    let source = r#"
declare function combine<Elem>(xs: Elem[], y: Elem): void;
combine(["a", "b"], 1);
"#;
    assert_eq!(
        ts2345_count(source),
        1,
        "renamed type parameter / params must reproduce the fix identically"
    );
}

#[test]
fn single_element_array_still_reports_ts2345() {
    // Control: the single-element case already worked and must keep working.
    let source = r#"
declare function f<T>(a: T[], b: T): void;
f(["a"], 1);
"#;
    assert_eq!(
        ts2345_count(source),
        1,
        "single-element array must keep reporting TS2345"
    );
}

#[test]
fn matching_naked_argument_reports_no_ts2345() {
    // Negative control: when the naked argument matches the element type, the
    // call is valid and must not be rejected.
    let source = r#"
declare function f<T>(a: T[], b: T): void;
f(["a", "b"], "c");
"#;
    assert_eq!(
        ts2345_count(source),
        0,
        "a naked argument compatible with the array element type must not be rejected"
    );
}

#[test]
fn inference_probe_fixes_t_from_array_element() {
    // The inference probe from the issue: `T` must be fixed to `string` (the
    // array element type), so the conflicting `1` argument is rejected at the
    // call rather than leaking a `string | number` return type.
    let source = r#"
declare function g<T>(a: T[], b: T): T;
const r = g(["a", "b"], 1);
"#;
    assert_eq!(
        ts2345_count(source),
        1,
        "T must be fixed from the array element so the naked argument conflicts"
    );
}

#[test]
fn rest_parameter_element_inference_reports_ts2345() {
    // Adjacent family: a rest parameter `...args: T[]` is the same array-element
    // inference position. tsc fixes `T` from the leftmost element (number) and
    // reports the conflicting `""` argument — see the checked-in tsc baseline
    // `genericRestArgs.errors.txt` (`makeArrayG(1, "")` → TS2345).
    let source = r#"
declare function f<T>(...args: T[]): T;
f(1, "a");
"#;
    assert_eq!(
        ts2345_count(source),
        1,
        "rest-parameter element inference must fix T from the leftmost element and report the conflict"
    );
}

#[test]
fn keyof_literal_union_candidates_are_not_first_wins() {
    // Guard against over-narrowing: when `K` is inferred from `keyof`-derived
    // string-literal-union candidates (not bare primitive intrinsics), the
    // leftmost-wins path must NOT engage — those candidates union, matching tsc.
    // A spurious first-wins here would drop keys and surface a false TS2345.
    let source = r#"
declare function pick<T, K extends keyof T>(obj: T, ...keys: K[]): Pick<T, K>;
const o = { a: 1, b: "x", c: true };
const r = pick(o, "a", "b");
const ra: number = r.a;
const rb: string = r.b;
"#;
    assert_eq!(
        ts2345_count(source),
        0,
        "keyof-derived literal-union key candidates must union, not first-win"
    );
}
