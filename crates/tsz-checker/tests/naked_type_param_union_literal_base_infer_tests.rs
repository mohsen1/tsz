//! Regression tests for issue #16948.
//!
//! When `tsc` infers a naked type parameter `T` from an array-literal argument
//! against a declared element type `T | number | string` (a union of a naked
//! variable plus fixed primitive members), it pairs each fixed union member
//! against the matching array element and infers `T` from the single leftover
//! element. The pairing follows `isTypeOrBaseIdenticalTo`: a *number literal*
//! source element matches a fixed `number` target and a *string literal* source
//! element matches a fixed `string` target.
//!
//! tsz previously matched fixed union members by type identity only, so the
//! unwidened literal element types `13` and `"12"` failed to pair with the
//! fixed `number`/`string` targets and leaked into `T`'s candidate set. Because
//! `"12"` (a string) violates `T extends Numeric`, inference discarded the whole
//! candidate set and fell back to `T`'s constraint (`Numeric`) instead of the
//! correct `NumCoercible`, producing a spurious `TS2741`/`TS2322`.
//!
//! These tests pin the corrected behaviour across structurally equivalent
//! shapes (renamed binders, differing union arity, the tuple-return form), not
//! just the reported spelling.

use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_compiled_lib_files};

const NUMERIC_PRELUDE: &str = r#"
interface Numeric { valueOf(): number; }
class NumCoercible {
    public a: number;
    constructor(a: number) { this.a = a; }
    public valueOf() { return this.a; }
}
"#;

fn es5_lib_files() -> Vec<Arc<LibFile>> {
    // `Array<T>` and the array-literal element inference this test exercises
    // come from `lib.es5.d.ts`; the default lib-less helper cannot type them.
    load_compiled_lib_files(&["lib.es5.d.ts"])
}

fn assignment_error_codes(source: &str) -> Vec<u32> {
    check_source_with_libs_code_messages(
        source,
        "test.ts",
        CheckerOptions::default(),
        &es5_lib_files(),
    )
    .into_iter()
    // TS2741 (property missing) and TS2322 (not assignable) are the two
    // shapes the constraint-fallback bug surfaced through.
    .filter(|(code, _)| *code == 2741 || *code == 2322)
    .map(|(code, _)| code)
    .collect()
}

#[test]
fn three_member_union_three_distinct_elements_infers_leftover_not_constraint() {
    // The reported repro: `T | number | string` with `[NumCoercible, 13, "12"]`.
    // `13` pairs with `number`, `"12"` pairs with `string`, and `T` is inferred
    // from the leftover `NumCoercible` — not the `Numeric` constraint.
    let source = format!(
        "{NUMERIC_PRELUDE}
function extent<T extends Numeric>(array: Array<T | number | string>): T {{ return array[0] as T; }}
let extentMixed = extent([new NumCoercible(10), 13, \"12\"]);
let check: NumCoercible = extentMixed;
"
    );
    assert!(
        assignment_error_codes(&source).is_empty(),
        "T must infer to NumCoercible, not its Numeric constraint; got {:?}",
        assignment_error_codes(&source)
    );
}

#[test]
fn renamed_binders_reproduce_the_fix_identically() {
    // The rule is structural, not tied to the identifier names
    // `T`/`Numeric`/`NumCoercible`/`extent`.
    let source = r#"
interface HasVal { valueOf(): number; }
class Widget {
    public w: number;
    constructor(w: number) { this.w = w; }
    public valueOf() { return this.w; }
}
function pick<Elem extends HasVal>(xs: Array<Elem | number | string>): Elem { return xs[0] as Elem; }
let got = pick([new Widget(1), 7, "z"]);
let chk: Widget = got;
"#;
    assert!(
        assignment_error_codes(source).is_empty(),
        "renamed binders must reproduce the fix identically; got {:?}",
        assignment_error_codes(source)
    );
}

#[test]
fn two_member_union_control_still_clean() {
    // Control: the 2-member union (1 fixed + T) already worked and must stay
    // clean after the fix.
    let source = format!(
        "{NUMERIC_PRELUDE}
function extent<T extends Numeric>(array: Array<T | number>): T {{ return array[0] as T; }}
let m = extent([new NumCoercible(10), 13]);
let check: NumCoercible = m;
"
    );
    assert!(
        assignment_error_codes(&source).is_empty(),
        "2-member union control must stay clean; got {:?}",
        assignment_error_codes(&source)
    );
}

#[test]
fn three_member_union_with_string_element_absent_still_clean() {
    // Boundary: 3-member union but only 2 distinct element types
    // (`string` member unmatched by the array). Already clean; must stay clean.
    let source = format!(
        "{NUMERIC_PRELUDE}
function extent<T extends Numeric>(array: Array<T | number | string>): T {{ return array[0] as T; }}
let m = extent([new NumCoercible(10), 13]);
let check: NumCoercible = m;
"
    );
    assert!(
        assignment_error_codes(&source).is_empty(),
        "3-member union with an unmatched string member must stay clean; got {:?}",
        assignment_error_codes(&source)
    );
}

#[test]
fn tuple_return_form_with_boolean_member_infers_leftover() {
    // The tuple-return shape from the issue's original discovery context. The
    // return-type structure (`[T | ... , T | ...] | [undefined, undefined]`)
    // does not change the array/param inference: `T` still resolves so the
    // contextual assignment is clean.
    let source = r#"
interface Numeric { valueOf(): number; }
class NumCoercible {
    public a: number;
    constructor(a: number) { this.a = a; }
    public valueOf() { return this.a; }
}
function extent<T extends Numeric>(
    array: Array<T | number | string | boolean>
): [T | number | string | boolean, T | number | string | boolean] | [undefined, undefined] {
    return [undefined, undefined];
}
let extentMixed: [number | string | boolean | NumCoercible, number | string | boolean | NumCoercible] | [undefined, undefined];
extentMixed = extent([new NumCoercible(10), 13, "12", true]);
"#;
    assert!(
        assignment_error_codes(source).is_empty(),
        "tuple-return form must infer T without falling back to the constraint; got {:?}",
        assignment_error_codes(source)
    );
}

#[test]
fn string_literal_element_still_reaches_naked_var_without_fixed_string_target() {
    // Negative guard against over-matching: when the union has NO fixed `string`
    // member, a string-literal element must NOT be consumed as if it matched a
    // fixed target — it flows into `T`. With array-literal widening `T` becomes
    // `string`, so assigning to a `"hi"` literal target is a genuine error
    // (matches tsc). This proves the literal->base pairing is gated on the fixed
    // primitive actually being present in the target union.
    let source = r#"
function pick<T>(xs: Array<T | number>): T { return xs[0] as T; }
let got = pick(["hi", 13]);
let chk: "hi" = got;
"#;
    let codes = assignment_error_codes(source);
    assert_eq!(
        codes,
        vec![2322],
        "string literal must reach T (widening to `string`) and error against the `\"hi\"` target; got {codes:?}"
    );
}
