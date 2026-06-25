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

// ── Template literal type operands ───────────────────────────────────────────
//
// A template literal type's value set is a subset of `string`, so for TS2367 it
// overlaps another operand exactly when a value can belong to both: it overlaps
// `string`, the open object type `{}`, and a string literal that matches the
// pattern; it is disjoint from every non-string type (number/boolean/bigint/
// symbol, the `object` keyword, arrays/functions, a numeric enum, and a string
// literal or nominal string enum whose value falls outside the pattern). Before
// this fix the structural overlap routine reported "overlaps" whenever exactly
// one operand was a template literal type, suppressing every TS2367 below.

#[test]
fn test_template_literal_vs_number_keeps_ts2367() {
    assert!(
        has_ts2367(
            r#"
declare const greeting: `x${string}`;
declare const count: number;
if (greeting === count) {}
"#
        ),
        "Expected TS2367: `x${{string}}` (string subtype) is disjoint from number"
    );
}

#[test]
fn test_template_literal_vs_boolean_and_bigint_keeps_ts2367() {
    // Binder-name variation (`tag`/`flag`, `slug`/`big`) proves no name-dependence.
    assert!(
        has_ts2367(
            r#"
declare const tag: `id-${string}`;
declare const flag: boolean;
if (tag === flag) {}
"#
        ),
        "Expected TS2367: template literal type vs boolean"
    );
    assert!(
        has_ts2367(
            r#"
declare const slug: `id-${string}`;
declare const big: bigint;
if (slug !== big) {}
"#
        ),
        "Expected TS2367: template literal type vs bigint"
    );
}

#[test]
fn test_template_literal_vs_object_keyword_and_array_keeps_ts2367() {
    assert!(
        has_ts2367(
            r#"
declare const route: `/${string}`;
declare const obj: object;
if (route === obj) {}
"#
        ),
        "Expected TS2367: a string value is never assignable to the `object` keyword"
    );
    assert!(
        has_ts2367(
            r#"
declare const route: `/${string}`;
declare const list: number[];
if (route === list) {}
"#
        ),
        "Expected TS2367: template literal type vs array"
    );
}

#[test]
fn test_template_literal_vs_empty_object_no_ts2367() {
    // `{}` accepts every non-nullish value, strings included, so it overlaps.
    assert!(
        !has_ts2367(
            r#"
declare const route: `/${string}`;
declare const anything: {};
if (route === anything) {}
"#
        ),
        "Expected NO TS2367: a template literal type is assignable to the empty object type"
    );
}

#[test]
fn test_template_literal_vs_string_no_ts2367() {
    assert!(
        !has_ts2367(
            r#"
declare const route: `/${string}`;
declare const text: string;
if (route === text) {}
"#
        ),
        "Expected NO TS2367: a template literal type is a subtype of `string`"
    );
}

#[test]
fn test_template_literal_vs_matching_and_nonmatching_literal() {
    assert!(
        !has_ts2367(
            r#"
declare const route: `x${string}`;
declare const lit: "xyz";
if (route === lit) {}
"#
        ),
        "Expected NO TS2367: \"xyz\" matches `x${{string}}`"
    );
    assert!(
        has_ts2367(
            r#"
declare const route: `x${string}`;
declare const lit: "abc";
if (route === lit) {}
"#
        ),
        "Expected TS2367: \"abc\" does not match `x${{string}}`"
    );
}

#[test]
fn test_template_literal_vs_union_with_one_matching_member_no_ts2367() {
    assert!(
        !has_ts2367(
            r#"
declare const route: `x${string}`;
declare const choices: "abc" | "xyz" | 7;
if (route === choices) {}
"#
        ),
        "Expected NO TS2367: the \"xyz\" member matches the pattern"
    );
}

#[test]
fn test_numeric_placeholder_template_vs_numeric_string_only() {
    assert!(
        !has_ts2367(
            r#"
declare const port: `${number}`;
declare const digits: "123";
if (port === digits) {}
"#
        ),
        "Expected NO TS2367: \"123\" matches `${{number}}`"
    );
    assert!(
        has_ts2367(
            r#"
declare const port: `${number}`;
declare const word: "abc";
if (port === word) {}
"#
        ),
        "Expected TS2367: \"abc\" is not a numeric string, so it cannot match `${{number}}`"
    );
}

#[test]
fn test_template_literal_vs_string_enum_value_aware() {
    // Non-matching members: nominal string enum disjoint from the pattern.
    assert!(
        has_ts2367(
            r#"
enum Color { Red = "red", Blue = "blue" }
declare const route: `x${string}`;
declare const c: Color;
if (route === c) {}
"#
        ),
        "Expected TS2367: no Color member matches `x${{string}}`"
    );
    // Matching member: `Tag.Xy = \"xylophone\"` matches `x${string}` → overlap.
    assert!(
        !has_ts2367(
            r#"
enum Tag { Xy = "xylophone", Other = "z" }
declare const route: `x${string}`;
declare const t: Tag;
if (route === t) {}
"#
        ),
        "Expected NO TS2367: Tag.Xy (\"xylophone\") matches `x${{string}}`"
    );
}

#[test]
fn test_template_literal_vs_numeric_enum_keeps_ts2367() {
    assert!(
        has_ts2367(
            r#"
enum Size { Small, Large }
declare const route: `x${string}`;
declare const s: Size;
if (route === s) {}
"#
        ),
        "Expected TS2367: numeric enum members are numbers, never matching string values"
    );
}
