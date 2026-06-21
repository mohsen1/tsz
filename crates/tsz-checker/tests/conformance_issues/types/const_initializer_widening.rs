//! `const` initializer widening: tsc preserves a top-level primitive literal
//! under `const`, but still widens the *mutable* element literals of fresh
//! arrays/tuples/objects — array/object element positions are mutable, so
//! `const c = cond ? ["x"] : []` is `string[]`, not `("x")[]`. (#14165)

use super::super::core::*;

/// The remeda witness: a `const` conditional whose true branch is a fresh
/// array literal and whose false branch is `[]` must widen the element literals
/// so a later `.push(string)` type-checks. `tsz` previously kept the literal
/// union element type, contravariantly intersecting the `.push` parameter and
/// emitting a false TS2345.
#[test]
fn const_conditional_array_branches_widen_for_push_no_ts2345() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare const cond: boolean;
const errors = cond
  ? ["frozen error", "circular error", (x: string) => x]
  : [];
errors.push("Sets cannot have replace patches.");
"#,
    );
    assert!(
        !has_error(&diagnostics, 2345),
        "no TS2345 expected — `cond ? [literals] : []` widens to a `string`-element \
         array under `const`, so `.push(string)` type-checks. Actual: {diagnostics:#?}"
    );
}

/// Both branches non-empty arrays: the union of the two array types still widens
/// member-by-member so a `.push` of either element type is accepted.
#[test]
fn const_conditional_two_array_branches_widen_no_ts2345() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare const cond: boolean;
const xs = cond ? ["frozen"] : ["circular"];
xs.push("replace");
"#,
    );
    assert!(
        !has_error(&diagnostics, 2345),
        "no TS2345 expected — both array branches widen their element literals to \
         `string`. Actual: {diagnostics:#?}"
    );
}

/// Negative control: `const` must STILL preserve a top-level primitive literal
/// union — `const c = cond ? "x" : "y"` is `"x" | "y"`, not `string`. Assigning
/// it to the literal-union annotation must stay clean (it would emit TS2322 if
/// the fix wrongly widened the top-level union to `string`).
#[test]
fn const_conditional_primitive_literals_preserved_no_ts2322() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare const cond: boolean;
const c = cond ? "x" : "y";
const t: "x" | "y" = c;
export { t };
"#,
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "no TS2322 expected — `const c = cond ? \"x\" : \"y\"` preserves `\"x\" | \"y\"` \
         (const does not widen top-level primitive literals). Actual: {diagnostics:#?}"
    );
}

/// Negative control: a non-fresh initializer (a function-call result) is NOT a
/// fresh literal expression, so its declared literal type is preserved under
/// `const` — the widening must not fire for non-fresh compounds.
#[test]
fn const_non_fresh_array_initializer_is_not_widened() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare function makeTuple(): ["a", "b"];
const pair = makeTuple();
const first: "a" = pair[0];
export { first };
"#,
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "no TS2322 expected — `makeTuple()` is non-fresh, so `const pair` keeps the \
         declared `[\"a\", \"b\"]` and `pair[0]` is `\"a\"`. Actual: {diagnostics:#?}"
    );
}
