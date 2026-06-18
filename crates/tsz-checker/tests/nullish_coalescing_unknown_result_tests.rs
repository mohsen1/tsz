//! Checker result-type tests for `??` whose left operand is `unknown`.
//!
//! Structural rule:
//!
//! > When the left operand of `??` is `unknown` (= `{} | null | undefined`),
//! > tsc's non-nullish operand is `getNonNullableType(unknown)` = the empty
//! > object `{}`. So `unknown ?? X` is `{} | X`, and `unknown ?? {}` is `{}`.
//!
//! The nullish split keeps `unknown` whole for flow `!= null` narrowing, so the
//! result-type computation substitutes the empty-object non-nullable form. Before
//! the fix the result stayed `unknown`, raising a false TS2769/TS2322 on
//! `Object.entries(data ?? {})` (ts-rest `standard-schema-utils.ts`).

use crate::test_utils::check_source_strict_codes as check_strict;

/// `unknown ?? {}` is assignable to `{}` (the result is `{}`, not `unknown`).
#[test]
fn unknown_nullish_empty_object_is_empty_object() {
    let source = r#"
declare const data: unknown;
const x = data ?? {};
const probe: {} = x;
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&2322),
        "`unknown ?? {{}}` must be `{{}}`, assignable to `{{}}`, got: {codes:?}"
    );
}

/// The ts-rest witness: `Object.entries(data ?? {})` with `data: unknown` must
/// type-check (no TS2769 no-overload). `Object.entries` accepts `{}`.
#[test]
fn object_entries_over_unknown_nullish_empty_object() {
    let source = r#"
declare const data: unknown;
const headersMap = new Map<string, unknown>(Object.entries(data ?? {}));
void headersMap;
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&2769),
        "Object.entries(data ?? {{}}) over `unknown` must not raise TS2769, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2345),
        "Object.entries(data ?? {{}}) over `unknown` must not raise TS2345, got: {codes:?}"
    );
}

/// `unknown ?? X` for a non-`{}` right operand is `{} | X`; both members assign.
#[test]
fn unknown_nullish_union_admits_both_members() {
    let source = r#"
declare const u: unknown;
const a = u ?? "hi";
const pa: {} | string = a;
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&2322),
        "`unknown ?? \"hi\"` must be `{{}} | string`, got: {codes:?}"
    );
}

/// Non-`unknown` left operands are unaffected: `string | null ?? number` stays
/// `string | number` (regression guard for the scoped substitution).
#[test]
fn nullable_union_left_unchanged_by_unknown_substitution() {
    let source = r#"
declare const s: string | null;
const a = s ?? 5;
const pa: string | number = a;
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&2322),
        "`(string | null) ?? 5` must stay `string | number`, got: {codes:?}"
    );
}
