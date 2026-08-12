//! Tests for TS1030 (`'declare' modifier already seen.`) on a repeated
//! `declare` modifier of a class member.
//!
//! Structural rule (mirrors tsc's `checkGrammarModifiers`): the walk records
//! `declare` as `ModifierFlags.Ambient` and, on a second `declare`, reports
//! `_0_modifier_already_seen` at that keyword and `return`s — so it is the only
//! grammar diagnostic on the member. This fires only when the duplicate is the
//! FIRST grammar error on the modifier list (no earlier duplicate/order error)
//! and no modifier that conflicts with `declare` (`override`/`accessor`/
//! `async`) precedes it — tsc reports that conflict first. Because tsc reaches
//! the second `declare` only on a property-like member (a method / accessor /
//! index signature errors on the FIRST `declare`, and a private-named member
//! hits the private-identifier check first), TS1030 is emitted from the property
//! construction path; the recorded duplicate also suppresses the declare/
//! override (TS1243) and declare/async (TS1040) conflicts, matching tsc's
//! post-`return` walk.
//!
//! The same "first error, then `return`" discipline is why a trailing duplicate
//! `declare` must not re-report a preceding modifier's ambient conflict: only
//! the first `declare` reports `override`/`accessor` incompatibility.
//!
//! Tests vary the binder name (`x`, `value`, `count`, …) to prove the behavior
//! is structural and not keyed to a specific identifier spelling. Oracle:
//! `typescript@7.0.2`, `--strict --target es2022 --lib es2022`.

use crate::parser::test_fixture::parse_source;

const TS1030: u32 = 1030;
const TS1040: u32 = 1040;
const TS1243: u32 = 1243;
const TS1031: u32 = 1031;

/// All `(code, start_offset, message)` triples the parser reports for `source`.
fn diags(source: &str) -> Vec<(u32, u32, String)> {
    let (parser, _root) = parse_source(source);
    parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.start, d.message.clone()))
        .collect()
}

/// The `(start_offset, message)` pairs at which `code` was reported.
fn hits(source: &str, code: u32) -> Vec<(u32, String)> {
    diags(source)
        .into_iter()
        .filter(|(c, _, _)| *c == code)
        .map(|(_, start, message)| (start, message))
        .collect()
}

fn count(source: &str, code: u32) -> usize {
    diags(source).iter().filter(|(c, _, _)| *c == code).count()
}

// ---------------------------------------------------------------------------
// Positive: the duplicate is the first grammar error on a property.
// ---------------------------------------------------------------------------

#[test]
fn plain_property_duplicate_declare_reports_ts1030_at_second_keyword() {
    // `class C { declare declare x: number; }` — the second `declare` starts at
    // byte offset 18 (`class C { declare ` is eighteen chars).
    let h = hits("class C { declare declare x: number; }", TS1030);
    assert_eq!(h.len(), 1, "exactly one TS1030, got {h:?}");
    assert_eq!(h[0].0, 18, "anchored at the second `declare` keyword");
    assert_eq!(h[0].1, "'declare' modifier already seen.");
}

#[test]
fn duplicate_declare_is_structural_across_binder_names() {
    // The rule keys on the modifier, not the member name.
    for name in ["x", "value", "count", "field"] {
        let src = format!("class C {{ declare declare {name}: number; }}");
        assert_eq!(count(&src, TS1030), 1, "one TS1030 for `{name}`");
    }
}

#[test]
fn untyped_optional_and_initialized_duplicate_declare_still_single_ts1030() {
    // No type annotation, optional, and initializer variants each report exactly
    // one TS1030 — the initializer's ambient-context grammar check (TS1039) is
    // suppressed by tsc once the modifier grammar error fires.
    for src in [
        "class C { declare declare x; }",
        "class C { declare declare x?: number; }",
        "class C { declare declare x = 1; }",
    ] {
        assert_eq!(count(src, TS1030), 1, "one TS1030 for `{src}`");
    }
}

#[test]
fn benign_leading_modifiers_do_not_hide_the_duplicate() {
    // Accessibility / static / readonly around the two `declare`s are legal and
    // must not stop TS1030 from firing at the second `declare`.
    for src in [
        "class C { readonly declare declare x: number; }",
        "class C { declare readonly declare x: number; }",
        "class C { static declare declare x: number; }",
        "class C { declare static declare x: number; }",
        "class C { declare declare static x: number; }",
        "class C { private declare declare x: number; }",
        "class C { public declare declare x: number; }",
    ] {
        assert_eq!(count(src, TS1030), 1, "one TS1030 for `{src}`");
    }
}

#[test]
fn triple_declare_reports_ts1030_once() {
    // tsc `return`s at the second `declare`; the third is never reached.
    assert_eq!(
        count("class C { declare declare declare x: number; }", TS1030),
        1
    );
}

#[test]
fn duplicate_declare_in_ambient_and_abstract_classes() {
    for src in [
        "declare class C { declare declare x: number; }",
        "abstract class C { declare declare x: number; }",
        "abstract class C { abstract declare declare x: number; }",
    ] {
        assert_eq!(count(src, TS1030), 1, "one TS1030 for `{src}`");
    }
}

// ---------------------------------------------------------------------------
// Adjacent leading `declare declare` wins over a following conflict modifier.
// ---------------------------------------------------------------------------

#[test]
fn adjacent_duplicate_declare_precedes_and_suppresses_following_conflict() {
    // `declare declare override|accessor|async x` — tsc reports only the
    // duplicate TS1030 and `return`s, so the declare/override (TS1243),
    // declare/async (TS1040) conflicts are not reported.
    for src in [
        "class C { declare declare override x: number; }",
        "class C { declare declare accessor x: number; }",
        "class C { declare declare async x: number; }",
    ] {
        assert_eq!(count(src, TS1030), 1, "one TS1030 for `{src}`");
        assert_eq!(count(src, TS1040), 0, "no TS1040 for `{src}`");
        assert_eq!(count(src, TS1243), 0, "no TS1243 for `{src}`");
    }
}

// ---------------------------------------------------------------------------
// A conflict modifier BETWEEN/BEFORE the declares wins; no TS1030, and the
// conflict is reported exactly once (not once per `declare`).
// ---------------------------------------------------------------------------

#[test]
fn conflict_between_declares_reports_once_and_not_ts1030() {
    // `declare override declare x` — override sits between the declares, so tsc
    // reports the declare/override conflict (TS1243) once and no TS1030.
    let src = "class C { declare override declare x: number; }";
    assert_eq!(count(src, TS1030), 0, "no TS1030");
    assert_eq!(count(src, TS1243), 1, "exactly one TS1243");
    assert_eq!(count(src, TS1040), 0, "no ambient TS1040");
}

#[test]
fn override_before_duplicate_declare_reports_ts1040_once() {
    // `override declare declare x` — tsc reports the override-in-ambient TS1040
    // once (at the first `declare`), not once per `declare`.
    let src = "class C { override declare declare x: number; }";
    assert_eq!(count(src, TS1030), 0, "no TS1030");
    assert_eq!(count(src, TS1040), 1, "exactly one TS1040");
}

#[test]
fn accessor_adjacent_to_declares_reports_conflict_once_and_not_ts1030() {
    // `declare accessor declare` and `accessor declare declare` — the accessor
    // sits between/before the declares, so tsc reports the declare/accessor
    // conflict (TS1243) once and no TS1030.
    for src in [
        "class C { declare accessor declare x: number; }",
        "class C { accessor declare declare x: number; }",
    ] {
        assert_eq!(count(src, TS1030), 0, "no TS1030 for `{src}`");
        assert_eq!(count(src, TS1243), 1, "exactly one TS1243 for `{src}`");
    }
}

// ---------------------------------------------------------------------------
// Negative: non-property members error on the FIRST `declare`, never TS1030.
// ---------------------------------------------------------------------------

#[test]
fn duplicate_declare_on_method_or_accessor_is_ts1031_not_ts1030() {
    // A method / getter is not a valid `declare` host: the FIRST `declare`
    // reports TS1031 and the duplicate is never reached.
    for src in [
        "class C { declare declare m(): void; }",
        "class C { declare declare get x() { return 1; } }",
    ] {
        assert_eq!(count(src, TS1030), 0, "no TS1030 for `{src}`");
        assert_eq!(count(src, TS1031), 1, "one TS1031 for `{src}`");
    }
}

#[test]
fn duplicate_declare_on_private_named_property_is_not_ts1030() {
    // A private-named member reaches the private-identifier modifier check
    // before the duplicate, so tsc does not report TS1030 here.
    let src = "class C { declare declare #p: number; }";
    assert_eq!(count(src, TS1030), 0, "no TS1030 on a private-named member");
}
