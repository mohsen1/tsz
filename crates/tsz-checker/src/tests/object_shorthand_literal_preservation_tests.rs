//! Regression tests for #6152: object literal shorthand and named properties must
//! preserve non-widening literal types from `as const` declarations and
//! explicitly literal-annotated declarations.
//!
//! Structural rule: when an object literal property's value is an identifier
//! (shorthand `{ x }` or named `{ key: x }`) that resolves to a declaration
//! carrying a non-widening literal type (via `as const` or an explicit literal
//! type annotation), the property type must NOT be widened to the primitive.
//! Only unannotated, non-const-asserted declarations produce fresh (widening)
//! literals in this position.

use crate::test_utils::check_source_codes;

// ── shorthand, `as const` ─────────────────────────────────────────────────────

/// `{ x }` where `const x = "v" as const` — shorthand preserves `"v"`.
///
/// Two variable names (`x` and `val`) prove the fix is structural, not name-bound.
#[test]
fn shorthand_as_const_string_preserves_literal_with_name_x() {
    let source = r#"
const x = "value" as const;
const obj = { x };
const check: "value" = obj.x;
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "shorthand `{{ x }}` where `const x = \"value\" as const` must preserve \
         the literal type; got: {codes:?}",
    );
}

#[test]
fn shorthand_as_const_string_preserves_literal_with_name_val() {
    let source = r#"
const val = "hello" as const;
const obj = { val };
const check: "hello" = obj.val;
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "shorthand `{{ val }}` where `const val = \"hello\" as const` must preserve \
         the literal type; got: {codes:?}",
    );
}

/// Numeric literal via `as const` — shorthand preserves `42`.
#[test]
fn shorthand_as_const_number_preserves_literal() {
    let source = r#"
const count = 42 as const;
const obj = { count };
const check: 42 = obj.count;
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "shorthand `{{ count }}` where `const count = 42 as const` must preserve \
         the literal type; got: {codes:?}",
    );
}

// ── shorthand, explicit literal type annotation ───────────────────────────────

/// `{ x }` where `const x: "v" = "v"` — shorthand preserves `"v"`.
#[test]
fn shorthand_literal_annotation_preserves_literal_with_name_x() {
    let source = r#"
const x: "value" = "value";
const obj = { x };
const check: "value" = obj.x;
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "shorthand `{{ x }}` where `const x: \"value\" = \"value\"` must preserve \
         the literal type; got: {codes:?}",
    );
}

#[test]
fn shorthand_literal_annotation_preserves_literal_with_name_tag() {
    let source = r#"
const tag: "start" = "start";
const obj = { tag };
const check: "start" = obj.tag;
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "shorthand `{{ tag }}` where `const tag: \"start\" = \"start\"` must \
         preserve the literal type; got: {codes:?}",
    );
}

// ── named property, `as const` ────────────────────────────────────────────────

/// `{ key: x }` where `const x = "v" as const` — named property preserves `"v"`.
#[test]
fn named_property_as_const_string_preserves_literal_with_name_x() {
    let source = r#"
const x = "value" as const;
const obj = { key: x };
const check: "value" = obj.key;
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "named property `{{ key: x }}` where `const x = \"value\" as const` must \
         preserve the literal type; got: {codes:?}",
    );
}

#[test]
fn named_property_as_const_string_preserves_literal_with_name_item() {
    let source = r#"
const item = "world" as const;
const obj = { name: item };
const check: "world" = obj.name;
"#;
    let codes = check_source_codes(source);
    assert!(
        !codes.contains(&2322),
        "named property `{{ name: item }}` where `const item = \"world\" as const` \
         must preserve the literal type; got: {codes:?}",
    );
}

// ── negative: unannotated `const` still widens ───────────────────────────────

/// `const x = "v"` (no annotation, no `as const`) — property type widens to `string`.
#[test]
fn shorthand_unannotated_const_widens_to_primitive_with_name_x() {
    let source = r#"
const x = "value";
const obj = { x };
const check: "value" = obj.x;
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2322),
        "shorthand `{{ x }}` where `const x = \"value\"` (unannotated) must widen \
         to `string`; assignment to `\"value\"` must fail. Got: {codes:?}",
    );
}

#[test]
fn named_property_unannotated_const_widens_to_primitive_with_name_token() {
    let source = r#"
const token = "abc";
const obj = { key: token };
const check: "abc" = obj.key;
"#;
    let codes = check_source_codes(source);
    assert!(
        codes.contains(&2322),
        "named property `{{ key: token }}` where `const token = \"abc\"` (unannotated) \
         must widen to `string`; assignment to `\"abc\"` must fail. Got: {codes:?}",
    );
}
