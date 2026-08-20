//! `TS1029`/`TS1243` co-emitted alongside a checker/checker-owned modifier
//! diagnostic where `tsc` reports only one, single-sourced code per member.
//!
//! `tsc`'s `checkGrammarModifiers` walks a member's modifier list in source
//! order and returns at the first error, so a member reports **at most one**
//! modifier-grammar diagnostic. tsz splits this walk across three owners
//! (parser: `TS1029`/`TS1243` ordering and pairing; checker:
//! `check_modifier_combinations`'s `abstract` + `static`/`private`/`async`
//! pairing; checker: `class_private_name_modifiers`'s `TS18010`/`TS18019`
//! walk), and none of them originally knew about the other two. Every
//! expectation here is pinned against `typescript@7.0.2`
//! (`--noEmit --strict --lib es2022 --target es2022`).

use crate::test_utils::check_source_codes_with_parse_health;

/// `TS1029`/`TS1243` and friends can be parser- or checker-owned depending on
/// the shape (see `check_source_codes_with_parse_health`'s doc comment on its
/// sibling `check_source_with_parse_health`); combine both sources so a test
/// doesn't need to know which layer emits which code.
fn codes(source: &str) -> Vec<u32> {
    check_source_codes_with_parse_health(source)
}

const TS1029: u32 = 1029;
const TS1243: u32 = 1243;
const TS1244: u32 = 1244;
const TS1253: u32 = 1253;
const TS18010: u32 = 18010;
const TS18019: u32 = 18019;

// --- container-abstractness (TS1244/TS1253) preempts the checker's
// --- order-blind abstract+static/private/async pairing check --------------

#[test]
fn abstract_static_method_in_non_abstract_class_reports_only_ts1244() {
    let source = "class C { abstract static m(): void; }\n";
    assert_eq!(codes(source), vec![TS1244], "codes: {:?}", codes(source));
}

#[test]
fn abstract_private_method_in_non_abstract_class_reports_only_ts1244() {
    let source = "class C { abstract private m(): void; }\n";
    assert_eq!(codes(source), vec![TS1244], "codes: {:?}", codes(source));
}

#[test]
fn abstract_static_property_in_non_abstract_class_reports_only_ts1253() {
    let source = "class C { abstract static x: number; }\n";
    assert_eq!(codes(source), vec![TS1253], "codes: {:?}", codes(source));
}

// --- abstract+static still fires TS1243 (order-independent) when the
// --- container genuinely is abstract, regardless of which side is first ---

#[test]
fn static_then_abstract_in_abstract_class_reports_ts1243() {
    let source = "abstract class C { static abstract m(): void; }\n";
    assert_eq!(codes(source), vec![TS1243], "codes: {:?}", codes(source));
}

#[test]
fn abstract_then_static_in_abstract_class_reports_ts1243() {
    let source = "abstract class C { abstract static m(): void; }\n";
    assert_eq!(codes(source), vec![TS1243], "codes: {:?}", codes(source));
}

// --- a private-named member's abstract conflict is claimed by the
// --- TS18010/TS18019 walk, not the checker's pairwise check ---------------

#[test]
fn abstract_static_private_named_method_reports_only_ts18019() {
    let source = "abstract class C { abstract static #m(): void; }\n";
    assert_eq!(codes(source), vec![TS18019], "codes: {:?}", codes(source));
}

#[test]
fn abstract_async_private_named_method_reports_only_ts18019() {
    let source = "abstract class C { abstract async #m(); }\n";
    assert_eq!(codes(source), vec![TS18019], "codes: {:?}", codes(source));
}

#[test]
fn private_keyword_before_abstract_private_named_property_reports_only_ts18010() {
    let source = "abstract class C { private abstract #x: number; }\n";
    assert_eq!(codes(source), vec![TS18010], "codes: {:?}", codes(source));
}

// --- but when the walk itself yields (static precedes abstract), the
// --- checker's pairwise check is still the true single-diagnostic owner ---

#[test]
fn static_then_abstract_private_named_method_still_reports_ts1243() {
    let source = "abstract class C { static abstract #m(): void; }\n";
    assert_eq!(codes(source), vec![TS1243], "codes: {:?}", codes(source));
}

// --- accessibility + abstract is a "cannot be used with" pair (TS1243), not
// --- an ordering rule (TS1029), regardless of which comes first -----------

#[test]
fn private_before_abstract_reports_only_ts1243() {
    let source = "abstract class C { private abstract m(): void; }\n";
    assert_eq!(codes(source), vec![TS1243], "codes: {:?}", codes(source));
}

#[test]
fn abstract_before_private_reports_only_ts1243_not_ts1029() {
    let source = "abstract class C { abstract private m(): void; }\n";
    assert_eq!(codes(source), vec![TS1243], "codes: {:?}", codes(source));
}

// --- accessibility ordering (TS1029) is unaffected for the modifiers that
// --- genuinely have to precede an accessibility keyword --------------------

#[test]
fn accessibility_after_static_still_reports_ts1029() {
    let source = "class C { static public x = 1; }\n";
    assert_eq!(codes(source), vec![TS1029], "codes: {:?}", codes(source));
}

// --- `async`+`abstract` TS1243 always anchors at `async`, in EITHER order —
// --- unlike `private`/`static`, which anchor at whichever modifier is
// --- written second (oracle: `classAbstractMixedWithModifiers.ts`, both
// --- `abstract async` and `async abstract` point at `async`) --------------

#[test]
fn abstract_then_async_anchors_ts1243_at_async_not_abstract() {
    let source = "abstract class C { abstract async m(): Promise<void>; }\n";
    let diags = crate::test_utils::check_source_non_strict(source);
    let ts1243 = diags
        .iter()
        .find(|d| d.code == TS1243)
        .unwrap_or_else(|| panic!("expected TS1243: {diags:?}"));
    let async_offset = source.find("async").unwrap() as u32;
    assert_eq!(
        ts1243.start, async_offset,
        "`abstract async` TS1243 must anchor at `async`, not `abstract`"
    );
}

#[test]
fn async_then_abstract_anchors_ts1243_at_async_not_abstract() {
    let source = "abstract class C { async abstract m(): Promise<void>; }\n";
    let diags = crate::test_utils::check_source_non_strict(source);
    let ts1243 = diags
        .iter()
        .find(|d| d.code == TS1243)
        .unwrap_or_else(|| panic!("expected TS1243: {diags:?}"));
    let async_offset = source.find("async").unwrap() as u32;
    assert_eq!(
        ts1243.start, async_offset,
        "`async abstract` TS1243 must anchor at `async` (the FIRST modifier here), not `abstract`"
    );
}

// --- regression guard: `private`/`static` keep the "whichever comes
// --- second" anchor, unaffected by the `async` special-case ---------------

#[test]
fn static_then_abstract_anchors_ts1243_at_abstract_not_static() {
    let source = "abstract class C { static abstract m(): void; }\n";
    let diags = crate::test_utils::check_source_non_strict(source);
    let ts1243 = diags
        .iter()
        .find(|d| d.code == TS1243)
        .unwrap_or_else(|| panic!("expected TS1243: {diags:?}"));
    // `rfind`: the class header's own `abstract` modifier precedes the
    // member's, so the member-level occurrence is the last one.
    let abstract_offset = source.rfind("abstract").unwrap() as u32;
    assert_eq!(
        ts1243.start, abstract_offset,
        "`static abstract` TS1243 must anchor at `abstract` (the second modifier)"
    );
}

// --- declare/accessor pairing (TS1243) yields to the private-identifier
// --- walk (TS18019) for a private name, in both source orders -------------

#[test]
fn declare_accessor_private_named_property_reports_only_ts18019() {
    let source = "class C { declare accessor #x: number; }\n";
    assert_eq!(codes(source), vec![TS18019], "codes: {:?}", codes(source));
}

#[test]
fn accessor_declare_private_named_property_reports_only_ts18019() {
    let source = "class C { accessor declare #x: number; }\n";
    assert_eq!(codes(source), vec![TS18019], "codes: {:?}", codes(source));
}

// --- but the ordinary (non-private) declare/accessor pairing keeps
// --- reporting TS1243 exactly as before -------------------------------------

#[test]
fn declare_accessor_ordinary_named_property_still_reports_ts1243() {
    let source = "class C { declare accessor x: number; }\n";
    assert_eq!(codes(source), vec![TS1243], "codes: {:?}", codes(source));
}

// --- readonly+accessor is a distinct rule tsc does NOT suppress for a
// --- private name — regression control for over-broad suppression ---------

#[test]
fn readonly_accessor_private_named_property_still_reports_ts1243() {
    let source = "class C { readonly accessor #x: number = 1; }\n";
    assert!(
        codes(source).contains(&TS1243),
        "codes: {:?}",
        codes(source)
    );
}
