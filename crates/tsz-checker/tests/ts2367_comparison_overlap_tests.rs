//! Tests for TS2367 comparison-operator overlap (§23).
//!
//! `x === y` must emit TS2367 when the operand types have empty overlap.
//! The key rule: when `never` appears as a union member, it must not
//! contribute to the overlap check (never is the empty set, it overlaps nothing).

use tsz_checker::test_utils::check_source_codes;

fn has_ts2367(source: &str) -> bool {
    check_source_codes(source).contains(&2367)
}

// ── Basic shapes ─────────────────────────────────────────────────────────────

#[test]
fn test_basic_num_union_vs_str_union() {
    assert!(
        has_ts2367(
            r#"
declare const a: 1 | 2 | 3;
declare const b: "x" | "y";
if (a === b) {}
"#
        ),
        "Expected TS2367 for 1|2|3 === \"x\"|\"y\""
    );
}

#[test]
fn test_cast_any_suppresses_ts2367() {
    assert!(
        !has_ts2367(
            r#"
declare const a: 1 | 2 | 3;
declare const b: "x" | "y";
if (a === (b as any)) {}
"#
        ),
        "Expected NO TS2367 when cast to any"
    );
}

#[test]
fn test_single_num_literal_vs_str_union() {
    assert!(
        has_ts2367(
            r#"
declare const a: 1;
declare const b: "x" | "y";
if (a === b) {}
"#
        ),
        "Expected TS2367 for 1 === \"x\"|\"y\""
    );
}

#[test]
fn test_number_vs_string_wide() {
    assert!(
        has_ts2367(
            r#"
declare const a: number;
declare const b: string;
if (a === b) {}
"#
        ),
        "Expected TS2367 for number vs string"
    );
}

#[test]
fn test_number_vs_string_literal() {
    assert!(
        has_ts2367(
            r#"
declare const a: number;
declare const b: "x" | "y";
if (a === b) {}
"#
        ),
        "Expected TS2367 for number vs string literals"
    );
}

// ── never in union ───────────────────────────────────────────────────────────

#[test]
fn test_never_in_left_union_is_ignored() {
    assert!(
        has_ts2367(
            r#"
declare const a: 1 | 2 | 3 | never;
declare const b: "x" | "y";
if (a === b) {}
"#
        ),
        "Expected TS2367 for (1|2|3|never) vs string — never must not add overlap"
    );
}

#[test]
fn test_never_in_right_union_is_ignored() {
    assert!(
        has_ts2367(
            r#"
declare const a: 1 | 2 | 3;
declare const b: "x" | "y" | never;
if (a === b) {}
"#
        ),
        "Expected TS2367 for numbers vs (\"x\"|\"y\"|never) — never must not add overlap"
    );
}

// ── Conditional types with a never branch ─────────────────────────────────────

#[test]
fn test_custom_extract_partial_never_branch() {
    // `MyExtract<1|2|"str", number>` distributes to `1 | 2 | never` → `1 | 2`.
    // Two name choices (T/K) prove the rule is not name-dependent.
    assert!(
        has_ts2367(
            r#"
type MyExtract<T, U> = T extends U ? T : never;
declare const a: MyExtract<1 | 2 | "str", number>;
declare const b: "x" | "y";
if (a === b) {}
"#
        ),
        "Expected TS2367 for MyExtract<1|2|\"str\", number> vs string union (param T)"
    );
}

#[test]
fn test_custom_extract_alternate_param_names() {
    // Same semantics with K/V param names — proves no name-dependence
    assert!(
        has_ts2367(
            r#"
type MyExtract<K, V> = K extends V ? K : never;
declare const a: MyExtract<1 | 2 | "str", number>;
declare const b: "x" | "y";
if (a === b) {}
"#
        ),
        "Expected TS2367 for MyExtract<> (params K/V) vs string union"
    );
}

#[test]
fn test_custom_exclude_conditional_type() {
    assert!(
        has_ts2367(
            r#"
type MyExclude<T, U> = T extends U ? never : T;
declare const a: MyExclude<1 | 2 | 3, 4>;
declare const b: "x" | "y";
if (a === b) {}
"#
        ),
        "Expected TS2367 for MyExclude<1|2|3, 4> vs string union (param T/U)"
    );
}

#[test]
fn test_custom_exclude_alternate_param_names() {
    assert!(
        has_ts2367(
            r#"
type MyExclude<A, B> = A extends B ? never : A;
declare const a: MyExclude<1 | 2 | 3, 4>;
declare const b: "x" | "y";
if (a === b) {}
"#
        ),
        "Expected TS2367 for MyExclude<> (params A/B) vs string union"
    );
}

#[test]
fn test_conditional_type_all_never_no_ts2367() {
    assert!(
        !has_ts2367(
            r#"
type MyExtract<T, U> = T extends U ? T : never;
declare const a: MyExtract<"a" | "b", number>;
declare const b: "x" | "y";
if (a === b) {}
"#
        ),
        "Expected NO TS2367 when left type fully resolves to `never` (unreachable branch)"
    );
}

// ── Type alias shapes ─────────────────────────────────────────────────────────

#[test]
fn test_type_alias_union() {
    assert!(
        has_ts2367(
            r#"
type NumLits = 1 | 2 | 3;
type StrLits = "x" | "y";
declare const a: NumLits;
declare const b: StrLits;
if (a === b) {}
"#
        ),
        "Expected TS2367 for aliased union literals"
    );
}

// ── Flow narrowing shapes ─────────────────────────────────────────────────────

#[test]
fn test_narrowed_union_vs_disjoint_type() {
    assert!(
        has_ts2367(
            r#"
declare const x: 1 | 2 | 3 | string;
declare const b: "x" | "y";
if (typeof x === "number") {
    if (x === b) {}
}
"#
        ),
        "Expected TS2367 for typeof-narrowed number literals vs string literals"
    );
}

// ── Same-family / genuine overlap: must NOT emit TS2367 ───────────────────────

#[test]
fn test_overlapping_number_literals_no_ts2367() {
    assert!(
        !has_ts2367(
            r#"
declare const a: 1 | 2 | 3;
declare const b: 1 | 4;
if (a === b) {}
"#
        ),
        "Expected NO TS2367 for 1|2|3 vs 1|4 (overlap at 1)"
    );
}

#[test]
fn test_number_type_vs_number_literal_no_ts2367() {
    assert!(
        !has_ts2367(
            r#"
declare const a: number;
declare const b: 42;
if (a === b) {}
"#
        ),
        "Expected NO TS2367 for number vs 42"
    );
}

#[test]
fn test_any_suppresses_ts2367() {
    assert!(
        !has_ts2367(
            r#"
declare const a: any;
declare const b: "x" | "y";
if (a === b) {}
"#
        ),
        "Expected NO TS2367 when left is any"
    );
}

#[test]
fn test_same_enum_member_no_ts2367() {
    assert!(
        !has_ts2367(
            r#"
declare const a: 1 | 2 | 3;
declare const b: 1 | 2 | 3;
if (a === b) {}
"#
        ),
        "Expected NO TS2367 for identical types"
    );
}

// ── Deferred conditional operands (§ default constraint) ─────────────────────
//
// A deferred conditional (`Exclude`/`Extract`, or a bare `T extends U ? X : Y`)
// has no value form until instantiated; tsc decides overlap through its default
// constraint (`getDefaultConstraintOfConditionalType`). tsz must do the same and
// must not treat the unresolved conditional as having empty overlap.

#[test]
fn test_exclude_conditional_vs_literal_no_ts2367() {
    assert!(
        !has_ts2367(
            r#"
function noTrue<T extends boolean>(subject: Exclude<T, false>): void {
  if (subject === true) throw new TypeError("subject is true");
}
"#
        ),
        "Expected NO TS2367: Exclude<T, false> constraint (boolean) overlaps with true"
    );
}

#[test]
fn test_extract_conditional_vs_literal_no_ts2367() {
    assert!(
        !has_ts2367(
            r#"
function noFalse<U extends boolean>(subject: Extract<U, boolean>): void {
  if (subject === false) throw new TypeError("subject is false");
}
"#
        ),
        "Expected NO TS2367: Extract<U, boolean> constraint (boolean) overlaps with false"
    );
}

#[test]
fn test_bare_deferred_conditional_vs_literal_no_ts2367() {
    // Binder-name variation: type parameter renamed to `Elem`, no utility alias.
    assert!(
        !has_ts2367(
            r#"
function pick<Elem extends number>(value: Elem extends 0 ? never : Elem): void {
  if (value === 1) {}
}
"#
        ),
        "Expected NO TS2367: (Elem extends 0 ? never : Elem) constraint (number) overlaps with 1"
    );
}

#[test]
fn test_deferred_conditional_disjoint_constraint_keeps_ts2367() {
    // Negative control: the conditional's default constraint (number) is genuinely
    // disjoint from a string literal, so TS2367 must still fire.
    assert!(
        has_ts2367(
            r#"
function f<T>(value: T extends string ? number : 1): void {
  if (value === ("foo" as "foo")) {}
}
"#
        ),
        "Expected TS2367: number constraint does not overlap with \"foo\""
    );
}
