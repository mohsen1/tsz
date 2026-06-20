//! Regression tests for issue #14165.
//!
//! `tsc` types each branch of a conditional (`cond ? a : b`) with
//! `checkExpression`, which keeps the fresh literal of a *primitive-literal*
//! branch (`cond ? "a" : "b"` is `"a" | "b"`) but still widens the element
//! types of an *array/object-literal* branch via best-common-type, because those
//! elements go through `checkExpressionForMutableLocation` (widening is
//! independent of the surrounding conditional). tsz previously forced
//! literal preservation for *both* branches, so `cond ? ["a", "b"] : []` kept
//! `("a" | "b")[]` instead of widening to `string[]`, and a later
//! `.push(string)` wrongly failed with TS2345.
//!
//! The fix scopes literal preservation to syntactic primitive-literal branches,
//! exactly as the `&&`/`||`/`??` logical-operator path already does. Binder
//! names are varied across fixtures so the rule stays structural.

use tsz_checker::test_utils::check_source_code_messages as compile_and_get_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    compile_and_get_diagnostics(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect()
}

fn assert_clean(source: &str, why: &str) {
    let found = codes(source);
    assert!(
        found.is_empty(),
        "{why}: expected no diagnostics, got {found:?}"
    );
}

fn assert_has(source: &str, code: u32, why: &str) {
    let found = codes(source);
    assert!(
        found.contains(&code),
        "{why}: expected TS{code}, got {found:?}"
    );
}

// --- The reported witness (immer canary family) ---------------------------
//
// The widening is observed by assigning an *arbitrary*, wider array into the
// inferred `const` type via `typeof`: a wider `string[]` is assignable to the
// inferred type only when the conditional widened its element type to `string`.
// If the literal element union survived (`("x" | "y")[]`), the wider `string[]`
// would be rejected with TS2322. This keeps the test independent of
// `Array.prototype.push` (absent in the unit-test reduced lib).

#[test]
fn issue_repro_mixed_array_branch_widens_elements() {
    let source = r#"
declare const cond: boolean;
const errors = cond
    ? ["frozen error", "circular error", (x: string) => x]
    : [];
const wider: (string | ((x: string) => string))[] = ["Sets cannot have replace patches."];
const sink: typeof errors = wider;
"#;
    assert_clean(
        source,
        "array-literal branch element types widen so an arbitrary string element is accepted",
    );
}

// --- Array-literal branches widen their elements --------------------------

#[test]
fn string_array_branch_widens_to_string_array() {
    let source = r#"
declare const flag: boolean;
const items = flag ? ["x", "y"] : [];
const wider: string[] = ["z"];
const sink: typeof items = wider;
"#;
    assert_clean(source, "string-literal array branch widens to string[]");
}

#[test]
fn both_array_branches_widen() {
    let source = r#"
declare const pick: boolean;
const values = pick ? ["x"] : ["y"];
const wider: string[] = ["z"];
const sink: typeof values = wider;
"#;
    assert_clean(source, "both array-literal branches widen to string[]");
}

#[test]
fn numeric_array_branch_widens_to_number_array() {
    let source = r#"
declare const which: boolean;
const nums = which ? [1, 2, 3] : [];
const wider: number[] = [4];
const sink: typeof nums = wider;
"#;
    assert_clean(source, "numeric-literal array branch widens to number[]");
}

#[test]
fn nested_array_branch_widens() {
    let source = r#"
declare const sel: boolean;
const grid = sel ? [["a"]] : [];
const wider: string[][] = [["z"]];
const sink: typeof grid = wider;
"#;
    assert_clean(source, "nested array-literal branch widens to string[][]");
}

#[test]
fn array_branch_widened_element_assignable_to_annotation() {
    let source = r#"
declare const branch: boolean;
const list: string[] = branch ? ["a", "b"] : [];
"#;
    assert_clean(source, "widened array branch is assignable to string[]");
}

#[test]
fn unwidened_literal_array_rejects_wider_string_array() {
    // Negative control: a genuinely literal-typed array (`as const`) is NOT
    // assignable from a wider `string[]`, proving the assignability probe above
    // actually discriminates widening.
    let source = r#"
declare const cond: boolean;
const frozen = cond ? (["x", "y"] as const) : (["x", "y"] as const);
const wider: string[] = ["z"];
const sink: typeof frozen = wider;
"#;
    assert_has(
        source,
        2322,
        "string[] is not assignable to a readonly literal tuple",
    );
}

// --- Object-literal branches widen their property types -------------------

#[test]
fn object_branch_property_widens() {
    let source = r#"
declare const route: boolean;
const node = route ? { tag: "open" } : { tag: "close" };
const sink: { tag: string } = node;
"#;
    assert_clean(source, "object-literal branch property widens to string");
}

// --- Primitive-literal branches still preserve their fresh literal --------

#[test]
fn scalar_branches_preserve_literal_union() {
    let source = r#"
declare const toggle: boolean;
const mode = toggle ? "a" : "b";
const annotated: "a" | "b" = mode;
"#;
    assert_clean(
        source,
        "primitive-literal branches keep the literal union 'a' | 'b'",
    );
}

#[test]
fn scalar_branch_literal_rejects_wrong_literal_target() {
    // `"a" | "b"` must NOT be assignable to a narrower `"a"` target.
    let source = r#"
declare const swap: boolean;
const mode = swap ? "a" : "b";
const narrow: "a" = mode;
"#;
    assert_has(source, 2322, "'a' | 'b' is not assignable to 'a'");
}

#[test]
fn numeric_scalar_branches_preserve_literal_union() {
    let source = r#"
declare const choose: boolean;
const n = choose ? 1 : 2;
const annotated: 1 | 2 = n;
"#;
    assert_clean(source, "numeric primitive-literal branches keep 1 | 2");
}

// --- `as const` branch keeps its readonly/literal shape (negative control) -

#[test]
fn const_asserted_array_branch_keeps_literals() {
    let source = r#"
declare const guard: boolean;
const tup = guard ? (["x", "y"] as const) : ([] as const);
tup.push("z");
"#;
    // `as const` makes the surviving branch a readonly tuple; `.push` is absent.
    assert_has(source, 2339, "readonly tuple from `as const` has no push");
}
