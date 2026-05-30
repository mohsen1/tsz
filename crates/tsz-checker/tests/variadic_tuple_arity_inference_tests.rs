//! Checker integration tests for variadic tuple arity inference.
//!
//! Structural rule: when a generic function parameter is a variadic tuple
//! `[H, ...Tail]`, `[...Init, L]`, or `[H, ...Mid, L]`, tsc aligns fixed
//! elements from the front (prefix) and back (suffix) of the concrete argument
//! tuple, then infers the rest type parameter from the middle slice.
//!
//! These tests verify that tsz infers the correct types and therefore accepts
//! valid assignments and rejects only the deliberately wrong ones.

use tsz_checker::test_utils::check_source_codes;

fn assert_no_errors(source: &str, label: &str) {
    let codes = check_source_codes(source);
    assert!(
        codes.is_empty(),
        "{label}: expected no diagnostics, got {codes:?}"
    );
}

fn assert_only_one_2322(source: &str, label: &str) {
    let codes = check_source_codes(source);
    assert_eq!(
        codes,
        vec![2322],
        "{label}: expected exactly one TS2322, got {codes:?}"
    );
}

// =============================================================================
// Trailing-rest: [H, ...Tail]
// =============================================================================

#[test]
fn head_and_tail_function_infers_correctly() {
    assert_no_errors(
        r#"
declare function head<H, Tail extends unknown[]>(args: [H, ...Tail]): H;
const h: string = head(["hello", 1, true]);
"#,
        "head function: H inferred as string",
    );
}

#[test]
fn tail_function_infers_rest_as_tuple() {
    assert_no_errors(
        r#"
declare function tail<H, Tail extends unknown[]>(args: [H, ...Tail]): Tail;
const t: [number, boolean] = tail(["hello", 1, true]);
"#,
        "tail function: Tail inferred as [number, boolean]",
    );
}

#[test]
fn tail_function_renamed_type_params() {
    // Proves fix is not keyed on param name "Tail"
    assert_no_errors(
        r#"
declare function tail2<X, Y extends unknown[]>(args: [X, ...Y]): Y;
const t: [number, boolean] = tail2(["hello", 1, true]);
"#,
        "tail function with renamed params: Y = [number, boolean]",
    );
}

#[test]
fn tail_wrong_assignment_fails() {
    assert_only_one_2322(
        r#"
declare function tail<H, Tail extends unknown[]>(args: [H, ...Tail]): Tail;
const t: [string, boolean] = tail(["hello", 1, true]);
"#,
        "tail function: bad assignment must produce TS2322",
    );
}

#[test]
fn empty_tail_inferred_as_empty_tuple() {
    assert_no_errors(
        r#"
declare function tail<H, Tail extends unknown[]>(args: [H, ...Tail]): Tail;
const t: [] = tail(["only"]);
"#,
        "tail of single-element source is []",
    );
}

// =============================================================================
// Leading-rest: [...Init, L]
// =============================================================================

#[test]
fn last_function_infers_correctly() {
    assert_no_errors(
        r#"
declare function last<Init extends unknown[], L>(args: [...Init, L]): L;
const l: boolean = last(["hello", 1, true]);
"#,
        "last function: L inferred as boolean",
    );
}

#[test]
fn init_function_infers_rest_as_tuple() {
    assert_no_errors(
        r#"
declare function init<Init extends unknown[], L>(args: [...Init, L]): Init;
const i: [string, number] = init(["hello", 1, true]);
"#,
        "init function: Init = [string, number]",
    );
}

#[test]
fn init_function_renamed_type_params() {
    // Proves fix is not keyed on param name "Init"
    assert_no_errors(
        r#"
declare function init2<P extends unknown[], Q>(args: [...P, Q]): P;
const i: [string, number] = init2(["hello", 1, true]);
"#,
        "init function renamed: P = [string, number]",
    );
}

#[test]
fn last_wrong_assignment_fails() {
    assert_only_one_2322(
        r#"
declare function last<Init extends unknown[], L>(args: [...Init, L]): L;
const l: string = last(["hello", 1, true]);
"#,
        "last function: bad assignment must produce TS2322",
    );
}

// =============================================================================
// Fixed-prefix + rest + fixed-suffix: [H, ...Mid, L]
// =============================================================================

#[test]
fn sandwich_function_infers_prefix_rest_suffix() {
    assert_no_errors(
        r#"
declare function sandwich<H, Mid extends unknown[], L>(
    args: [H, ...Mid, L]
): { head: H; mid: Mid; last: L };
const r = sandwich(["a", 1, true]);
const ok: { head: string; mid: [number]; last: boolean } = r;
"#,
        "sandwich: H=string, Mid=[number], L=boolean",
    );
}

#[test]
fn sandwich_wrong_mid_fails() {
    assert_only_one_2322(
        r#"
declare function sandwich<H, Mid extends unknown[], L>(
    args: [H, ...Mid, L]
): { head: H; mid: Mid; last: L };
const r = sandwich(["a", 1, true]);
const bad: { head: string; mid: [string]; last: boolean } = r;
"#,
        "sandwich: wrong mid-type should fail",
    );
}
