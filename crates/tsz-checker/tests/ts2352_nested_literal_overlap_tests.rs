//! Tests for TS2352 overlap of literal-typed object properties / elements.
//!
//! Structural rule: tsc's `checkAssertionWorker` widens only the *top-level*
//! assertion source via `getWidenedType`. Two distinct literals that are the
//! direct operands of an assertion (`"x" as "y"`) therefore overlap, but two
//! distinct literals that appear as a shared *property* or *element* type
//! (`{ k: "a" } as { k: "b" }`, `["a"] as ["b"]`) are not widened and do not
//! overlap, so tsc reports TS2352. A literal property is still comparable to a
//! base-primitive property (`{ n: 1 } as { n: number }`) and to a literal-union
//! property that contains it (`{ k: "a" } as { k: "a" | "b" }`).
//!
//! Regression for the `satisfies` + `as` family: a value preserved as a literal
//! by `satisfies` (e.g. `{ mode: "dev" } satisfies { mode: "dev" | "prod" }`)
//! keeps `{ mode: "dev" }`, so casting it to a non-overlapping literal union
//! must report TS2352.

use tsz_checker::test_utils::check_source_strict_codes as check_strict;

fn ts2352_count(source: &str) -> usize {
    check_strict(source).iter().filter(|&&c| c == 2352).count()
}

// ---------------------------------------------------------------------------
// Positive: nested distinct literals do NOT overlap -> TS2352
// ---------------------------------------------------------------------------

/// Direct object literal with a distinct string-literal property. Varying the
/// property name and the literal spelling proves the rule is structural, not
/// keyed on a particular identifier.
#[test]
fn object_property_distinct_string_literal_reports_ts2352() {
    for (key, src_lit, tgt_lit) in [
        ("mode", "dev", "prod"),
        ("kind", "a", "b"),
        ("tag", "left", "right"),
    ] {
        let source = format!("const v = {{ {key}: '{src_lit}' }} as {{ {key}: '{tgt_lit}' }};");
        assert_eq!(
            ts2352_count(&source),
            1,
            "[{key}: '{src_lit}' as '{tgt_lit}'] expected exactly one TS2352. Codes: {:?}",
            check_strict(&source)
        );
    }
}

/// Distinct numeric-literal property — same rule, different primitive kind.
#[test]
fn object_property_distinct_number_literal_reports_ts2352() {
    let source = "const v = { n: 1 } as { n: 2 };";
    assert_eq!(ts2352_count(source), 1, "Codes: {:?}", check_strict(source));
}

/// The property's literal does not appear in the target's literal *union*.
#[test]
fn object_property_literal_not_in_target_union_reports_ts2352() {
    for (key, value, union) in [("mode", "'dev'", "'prod' | 'test'"), ("size", "1", "2 | 3")] {
        let source = format!("const v = {{ {key}: {value} }} as {{ {key}: {union} }};");
        assert_eq!(
            ts2352_count(&source),
            1,
            "[{key}: {value} as {union}] expected TS2352. Codes: {:?}",
            check_strict(&source)
        );
    }
}

/// The reported `satisfies` + `as` repro: `satisfies` preserves the literal, so
/// the subsequent cast to a non-overlapping literal union must report TS2352.
/// Renaming the iteration property and literals proves it is not hardcoded.
#[test]
fn satisfies_preserved_literal_then_incompatible_cast_reports_ts2352() {
    for (key, lit, sat_union, cast_union) in [
        ("mode", "dev", "'dev' | 'prod'", "'test' | 'prod'"),
        ("kind", "a", "'a' | 'b'", "'c' | 'd'"),
    ] {
        let source = format!(
            "const config = {{ {key}: '{lit}' }} satisfies {{ {key}: {sat_union} }};\n\
             const bad = config as {{ {key}: {cast_union} }};"
        );
        assert_eq!(
            ts2352_count(&source),
            1,
            "[satisfies {key}: '{lit}'] expected TS2352 on the incompatible cast. Codes: {:?}",
            check_strict(&source)
        );
    }
}

/// Distinct literal tuple elements do not overlap either.
#[test]
fn tuple_element_distinct_literal_reports_ts2352() {
    let source = "const v = ['a'] as ['b'];";
    assert_eq!(ts2352_count(source), 1, "Codes: {:?}", check_strict(source));
}

// ---------------------------------------------------------------------------
// Negative: cases that DO overlap must NOT report TS2352
// ---------------------------------------------------------------------------

/// Top-level distinct literals overlap (tsc widens the top-level source).
#[test]
fn top_level_distinct_literals_no_ts2352() {
    for source in [
        "const a = 'x' as 'y';",
        "const b = 1 as 2;",
        "const c = 'x' as 'y' | 'z';",
    ] {
        assert_eq!(
            ts2352_count(source),
            0,
            "[{source}] top-level literal assertion must not report TS2352. Codes: {:?}",
            check_strict(source)
        );
    }
}

/// A literal property is comparable to the same literal inside a target union.
#[test]
fn object_property_literal_in_target_union_no_ts2352() {
    let source = "const v = { mode: 'dev' } as { mode: 'dev' | 'prod' };";
    assert_eq!(ts2352_count(source), 0, "Codes: {:?}", check_strict(source));
}

/// A literal property is comparable to a base-primitive target property.
#[test]
fn object_property_literal_to_base_primitive_no_ts2352() {
    for source in [
        "const v = { n: 1 } as { n: number };",
        "const w = { s: 'a' } as { s: string };",
    ] {
        assert_eq!(
            ts2352_count(source),
            0,
            "[{source}] literal-to-base property must not report TS2352. Codes: {:?}",
            check_strict(source)
        );
    }
}

/// A property widened to its base primitive (a plain `const` object) overlaps a
/// literal-union target.
#[test]
fn widened_property_to_literal_union_no_ts2352() {
    let source = "const cobj = { mode: 'dev' };\n\
                  const v = cobj as { mode: 'prod' };";
    assert_eq!(ts2352_count(source), 0, "Codes: {:?}", check_strict(source));
}
