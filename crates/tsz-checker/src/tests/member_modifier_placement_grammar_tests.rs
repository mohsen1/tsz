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

use crate::test_utils::{check_source_codes_with_parse_health, check_source_diagnostics};

fn codes(source: &str) -> Vec<u32> {
    check_source_codes_with_parse_health(source)
}

const TS1024: u32 = 1024; // 'readonly' modifier can only appear on a property declaration or index signature.
const TS1042: u32 = 1042; // 'async' modifier cannot be used here.
const TS1089: u32 = 1089; // '{0}' modifier cannot appear on a constructor declaration.

/// The modifier-placement grammar codes this suite is about. Anchoring/dedup
/// assertions filter to these so they stay immune to unrelated harness noise —
/// e.g. `check_source_diagnostics` loads no lib, so an `async` member with a
/// `Promise` return type also draws TS1064/TS2583 (`Promise` unresolved) that
/// the real CLI, with the lib present, never emits.
const PLACEMENT_CODES: [u32; 3] = [TS1024, TS1042, TS1089];

/// `(code, anchored source text)` for each modifier-placement diagnostic,
/// sorted, so a test pins both the code and *where* the diagnostic points
/// without hard-coding byte offsets. tsc's `checkGrammarModifiers` anchors a
/// modifier-placement error at the offending modifier keyword itself (not the
/// member name or the declaration start), so the anchored text is exactly
/// `"readonly"` / `"async"`.
fn coded_anchors(source: &str) -> Vec<(u32, String)> {
    let mut v: Vec<(u32, String)> = check_source_diagnostics(source)
        .iter()
        .filter(|d| PLACEMENT_CODES.contains(&d.code))
        .map(|d| {
            let anchor = source
                .get(d.start as usize..(d.start + d.length) as usize)
                .unwrap_or_default()
                .to_string();
            (d.code, anchor)
        })
        .collect();
    v.sort_unstable();
    v
}

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

// --- anchor position: TS1024/TS1042 point at the offending modifier keyword,
// --- not the member name or declaration start, for every modifier ordering ---

#[test]
fn readonly_on_method_anchors_at_readonly_even_when_not_first() {
    // `readonly` is not the first modifier here; tsc still anchors TS1024 at
    // the `readonly` keyword, not at `static`/`public` or the method name.
    for source in [
        "class C { static readonly m(): void {} }",
        "class C { public readonly m(): void {} }",
        "class C { async readonly m(): Promise<void> {} }",
    ] {
        assert_eq!(
            coded_anchors(source),
            vec![(TS1024, "readonly".to_string())],
            "source: {source}"
        );
    }
}

#[test]
fn readonly_on_accessor_anchors_at_readonly_even_when_not_first() {
    for source in [
        "class C { static readonly get x() { return 1; } }",
        "class C { public readonly get x(): number { return 1; } }",
        "class C { static readonly set x(v: number) {} }",
    ] {
        assert_eq!(
            coded_anchors(source),
            vec![(TS1024, "readonly".to_string())],
            "source: {source}"
        );
    }
}

#[test]
fn abstract_readonly_accessor_in_abstract_class_anchors_at_readonly() {
    // `abstract` is legal on an abstract class's accessor, so the lone error is
    // `readonly` (TS1024), anchored at the `readonly` keyword.
    let source = "abstract class C { abstract readonly get x(): number; }";
    assert_eq!(
        coded_anchors(source),
        vec![(TS1024, "readonly".to_string())]
    );
}

#[test]
fn async_on_accessor_anchors_at_async_even_when_not_first() {
    for source in [
        "class C { static async get x() { return 1; } }",
        "class C { public async get x(): number { return 1; } }",
        "class C { static async set x(v: number) {} }",
    ] {
        assert_eq!(
            coded_anchors(source),
            vec![(TS1042, "async".to_string())],
            "source: {source}"
        );
    }
}

#[test]
fn async_on_property_anchors_at_async_even_when_not_first() {
    for source in [
        "class C { static async p = 1; }",
        "class C { readonly async p = 1; }",
        "class C { async readonly p = 1; }",
    ] {
        assert_eq!(
            coded_anchors(source),
            vec![(TS1042, "async".to_string())],
            "source: {source}"
        );
    }
}

// --- first-error-wins: an accessor carrying both `readonly` and `async` is a
// --- single diagnostic (TS1024 at `readonly`), in either source order, since
// --- tsc reports one modifier-grammar diagnostic per member and `readonly`
// --- wins over `async` on an accessor. `readonly` on a *property* is legal, so
// --- that shape keeps its lone TS1042 (asserted above). ----------------------

#[test]
fn readonly_and_async_on_accessor_reports_only_ts1024_at_readonly() {
    for source in [
        "class C { readonly async get x() { return 1; } }",
        "class C { async readonly get x() { return 1; } }",
        "class C { readonly async set x(v: number) {} }",
        "class C { async readonly set x(v: number) {} }",
    ] {
        assert_eq!(
            coded_anchors(source),
            vec![(TS1024, "readonly".to_string())],
            "source: {source}"
        );
    }
}

#[test]
fn readonly_and_async_accessor_dedup_is_independent_of_member_name() {
    for name in ["x", "value", "prop", "data"] {
        let source = format!("class C {{ readonly async get {name}() {{ return 1; }} }}");
        assert_eq!(
            coded_anchors(&source),
            vec![(TS1024, "readonly".to_string())],
            "source: {source}"
        );
    }
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
