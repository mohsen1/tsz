//! Class-member modifier *placement* grammar for `readonly` and `async`
//! (issue #16291, the two positions surfaced 2026-08-07): `readonly` on a
//! **constructor** is TS1024, and `async` on a **property declaration** is
//! TS1042. tsz already covered the rest of each family (readonly on
//! method/accessor, async on accessors), so these are missing-position gaps,
//! not missing codes.
//!
//! Every expectation is pinned against `typescript@7.0.2`
//! (`--noEmit --strict --target es2022 --lib es2022`). Binder names are varied
//! so the diagnostic is keyed on the modifier position, not any identifier.

use crate::test_utils::check_source_codes_with_parse_health;

fn codes(source: &str) -> Vec<u32> {
    check_source_codes_with_parse_health(source)
}

const TS1024: u32 = 1024; // 'readonly' modifier can only appear on a property declaration or index signature.
const TS1042: u32 = 1042; // 'async' modifier cannot be used here.
const TS1089: u32 = 1089; // '{0}' modifier cannot appear on a constructor declaration.

// --- readonly on a constructor => TS1024 (the newly-covered position) -------

#[test]
fn readonly_constructor_reports_ts1024_independent_of_class_name() {
    for name in ["C", "Widget", "Repository", "Zzz"] {
        let source = format!("class {name} {{ readonly constructor() {{}} }}");
        assert_eq!(codes(&source), vec![TS1024], "source: {source}");
    }
}

// --- async on a property declaration => TS1042 (the newly-covered position) --

#[test]
fn async_property_without_initializer_reports_ts1042_alongside_ts2564() {
    // tsc reports both: the grammar TS1042 and the strict-init TS2564.
    let mut c = codes("class C { async p: number; }");
    c.sort_unstable();
    assert_eq!(c, vec![TS1042, 2564]);
}

#[test]
fn async_property_is_independent_of_property_name() {
    for name in ["p", "value", "data", "field"] {
        let source = format!("class C {{ async {name}: number = 1; }}");
        assert_eq!(codes(&source), vec![TS1042], "source: {source}");
    }
}

// --- combinations: still exactly one code, matching tsc's first-error walk ---

#[test]
fn readonly_async_property_reports_only_ts1042() {
    // `readonly` on a property is legal, so only the `async` placement fires.
    assert_eq!(codes("class C { readonly async p = 1; }"), vec![TS1042]);
}

// --- adjacent positions that must keep their existing (correct) behavior ----

#[test]
fn readonly_method_and_accessors_still_report_ts1024() {
    assert_eq!(codes("class C { readonly m() {} }"), vec![TS1024]);
    assert_eq!(
        codes("class C { readonly get x() { return 1; } }"),
        vec![TS1024]
    );
    assert_eq!(
        codes("class C { readonly set x(v: number) {} }"),
        vec![TS1024]
    );
}

#[test]
fn async_accessors_still_report_ts1042() {
    assert_eq!(
        codes("class C { async get x() { return 1; } }"),
        vec![TS1042]
    );
    assert_eq!(codes("class C { async set x(v: number) {} }"), vec![TS1042]);
}

#[test]
fn async_constructor_still_reports_ts1089_not_ts1042() {
    // `async` on a constructor is TS1089, a distinct code, and must not be
    // reclassified by the property/accessor async placement check.
    assert_eq!(codes("class C { async constructor() {} }"), vec![TS1089]);
}

#[test]
fn valid_members_stay_clean() {
    for source in [
        "class C { async m() {} }",
        "class C { readonly p: number = 1; }",
        "class C { p = 1; m() {} constructor() {} get g() { return 1; } }",
        "class C { readonly p = 1; constructor() {} }",
    ] {
        assert!(
            codes(source).is_empty(),
            "expected no diagnostics for `{source}`, got {:?}",
            codes(source)
        );
    }
}
