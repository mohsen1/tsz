//! Tests for literal widening of non-spread elements in a spread-containing
//! array literal during `T[]` inference (issue #9741).
//!
//! Structural rule: when inferring the element type `T` of `arr: T[]` from a
//! fresh array literal, the non-spread literal elements are widened to their
//! base primitive (ordinary array-literal element widening), regardless of
//! whether the literal also contains a spread. tsc's `getWidenedLiteralType`
//! recurses into the candidate union and widens each fresh literal member
//! independently, so a non-literal member injected by a spread of `number[]`
//! must NOT suppress widening of its literal siblings.
//!
//! Before the fix tsz only widened when *every* union member was a literal, so
//! `f([...numberArray, "x"])` inferred `T = number | "x"` instead of
//! `T = string | number`, masking a `TS2322` (false negative).

use tsz_checker::test_utils::check_source_code_messages as get_diagnostics;

fn ts2322(source: &str) -> Vec<String> {
    get_diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code == 2322)
        .map(|(_, msg)| msg)
        .collect()
}

#[test]
fn spread_then_literal_widens_during_array_inference() {
    // Reported repro: `T` must widen to `string | number`, so assigning the
    // result to the narrower `number | "x"` is a TS2322.
    let source = r#"
declare function f<T>(arr: T[]): T;
const base = [1, 2];
const r = f([...base, "x"]);
const c: number | "x" = r;
"#;
    let errs = ts2322(source);
    assert!(
        !errs.is_empty(),
        "expected TS2322 (T widened to string | number, not number | \"x\"), got none"
    );
}

#[test]
fn spread_then_literal_accepts_widened_target() {
    // The widened inference (`string | number`) must be accepted by the
    // widened annotation — no spurious error on the boundary control.
    let source = r#"
declare function f<T>(arr: T[]): T;
const base = [1, 2];
const r = f([...base, "x"]);
const c: string | number = r;
"#;
    assert!(
        ts2322(source).is_empty(),
        "string | number target must accept the widened result, got: {:#?}",
        ts2322(source)
    );
}

#[test]
fn literal_then_spread_widens_position_independent() {
    // Spread position must not matter: leading literal + trailing spread.
    let source = r#"
declare function f<T>(arr: T[]): T;
const base = [1, 2];
const r = f(["x", ...base]);
const c: number | "x" = r;
"#;
    assert!(
        !ts2322(source).is_empty(),
        "expected TS2322 with spread last (position independent), got none"
    );
}

#[test]
fn spread_then_multiple_literals_widen() {
    // Multiple trailing literals all widen.
    let source = r#"
declare function f<T>(arr: T[]): T;
const base = [1, 2];
const r = f([...base, "x", "y"]);
const c: number | "x" | "y" = r;
"#;
    assert!(
        !ts2322(source).is_empty(),
        "expected TS2322 with multiple trailing literals, got none"
    );
}

#[test]
fn renamed_param_with_number_literal_member_widens() {
    // Anti-hardcoding: rename the type parameter and use a number literal
    // alongside a string spread. The rule is structural (a literal union
    // member from a fresh array element widens), not tied to a name or to the
    // `string`/`number` spelling in the repro.
    let source = r#"
declare function combine<Elem>(xs: Elem[]): Elem;
const seed = ["a", "b"];
const out = combine([...seed, 1]);
const bad: string | 1 = out;
"#;
    assert!(
        !ts2322(source).is_empty(),
        "expected TS2322 with renamed param + widened number literal, got none"
    );
}

#[test]
fn renamed_param_widened_target_accepted() {
    let source = r#"
declare function combine<Elem>(xs: Elem[]): Elem;
const seed = ["a", "b"];
const out = combine([...seed, 1]);
const ok: string | number = out;
"#;
    assert!(
        ts2322(source).is_empty(),
        "string | number target must accept widened result, got: {:#?}",
        ts2322(source)
    );
}

#[test]
fn plain_literal_array_still_widens_control() {
    // Control: a plain array literal (no spread) already widened correctly and
    // must keep widening — `T = string`, so the literal-union annotation errors.
    let source = r#"
declare function f<T>(arr: T[]): T;
const r = f(["x", "y"]);
const c: "x" | "y" = r;
"#;
    assert!(
        !ts2322(source).is_empty(),
        "plain array literal must still widen to string (T != \"x\" | \"y\"), got none"
    );
}

#[test]
fn plain_literal_array_accepts_string_control() {
    let source = r#"
declare function f<T>(arr: T[]): T;
const r = f(["x", "y"]);
const c: string = r;
"#;
    assert!(
        ts2322(source).is_empty(),
        "plain array literal widened to string must accept `string`, got: {:#?}",
        ts2322(source)
    );
}

#[test]
fn spread_only_literal_array_widens_control() {
    // Control: spread of a literal array with no extra literal element still
    // widens to the element primitive (the trigger is a non-spread literal
    // *alongside* a spread, not the spread alone).
    let source = r#"
declare function f<T>(arr: T[]): T;
const r = f([...["a", "b"]]);
const c: string = r;
"#;
    assert!(
        ts2322(source).is_empty(),
        "spread-only literal array must widen to string and accept `string`, got: {:#?}",
        ts2322(source)
    );
}
