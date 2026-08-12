//! TS1071 grammar checks for the modifier run on a **class** index signature.
//!
//! Structural rule: on a class index signature only `readonly` and `static`
//! are legal modifiers; every other modifier draws a single TS1071
//! (`'{0}' modifier cannot appear on an index signature.`). tsc's
//! `checkGrammarModifiers` returns at the FIRST offending modifier, so exactly
//! one TS1071 fires per signature — never one-per-modifier.
//!
//! Before the fix the checker hardcoded the illegal set to the accessibility
//! modifiers (`public`/`private`/`protected`) plus `export`, so `declare`,
//! `abstract`, `async`, `override`, `accessor`, `in`/`out`, and `const` were
//! silently accepted (a false negative), and a multi-modifier run emitted one
//! TS1071 per offender instead of one.
//!
//! Oracle: `typescript@6.0.2`, cross-checked against `7.0.2`.
//!
//! Rules are expressed on the token kind, not on user-chosen names, so every
//! assertion sweeps a matrix of class names and index-key parameter names to
//! prove the checker is not pattern-matching identifiers.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

fn ts_options() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    }
}

fn diags(source: &str) -> Vec<u32> {
    check_source(source, "a.ts", ts_options())
        .iter()
        .map(|d| d.code)
        .collect()
}

/// Names varied across every assertion so no test depends on a particular
/// class/key identifier.
const CLASS_NAMES: [&str; 3] = ["C", "Widget", "Zqx"];
const KEY_NAMES: [&str; 3] = ["k", "key", "propName"];

/// Assert that `<class_kw> Name { <prefix>[<key>: string]: number; }` produces
/// exactly `expected` TS1071 diagnostics, sweeping the class-name × key-name
/// matrix so the outcome can't hinge on a chosen identifier. `class_kw` lets a
/// case use `abstract class` for the `abstract`-member form.
fn assert_ts1071_count(class_kw: &str, prefix: &str, expected: usize) {
    for name in CLASS_NAMES {
        for key in KEY_NAMES {
            let source = format!("{class_kw} {name} {{ {prefix}[{key}: string]: number; }}");
            let all = diags(&source);
            let count = all.iter().filter(|&&c| c == 1071).count();
            assert_eq!(
                count, expected,
                "expected {expected} TS1071 for prefix `{prefix}` in `{source}`, \
                 got {count} (all codes: {all:?})"
            );
        }
    }
}

// =========================================================================
// Illegal modifiers — each must produce exactly one TS1071.
// =========================================================================

#[test]
fn every_illegal_single_modifier_emits_exactly_one_ts1071() {
    // (modifier text, requires an `abstract class` container)
    let illegal: [(&str, bool); 11] = [
        ("declare", false),
        ("abstract", true),
        ("async", false),
        ("override", false),
        ("accessor", false),
        ("in", false),
        ("out", false),
        ("const", false),
        ("public", false),
        ("private", false),
        ("protected", false),
    ];
    for (modifier, needs_abstract_class) in illegal {
        let class_kw = if needs_abstract_class {
            "abstract class"
        } else {
            "class"
        };
        assert_ts1071_count(class_kw, &format!("{modifier} "), 1);
    }
}

#[test]
fn double_declare_still_reports_a_single_ts1071() {
    // Two copies of the same illegal modifier: tsc returns at the first, so
    // still exactly one TS1071 (regression guard against one-per-modifier).
    assert_ts1071_count("class", "declare declare ", 1);
}

#[test]
fn two_distinct_illegal_modifiers_report_a_single_ts1071() {
    // `public private [k]`: tsc returns TS1071 at the first modifier (`public`)
    // before the duplicate-accessibility check even runs, so exactly one
    // TS1071 fires. Any TS1028 duplicate-modifier diagnostic is a separate,
    // parser-owned concern and is not asserted here (we count only TS1071).
    assert_ts1071_count("class", "public private ", 1);
}

// =========================================================================
// Legal modifiers — `readonly` and `static` never draw TS1071.
// =========================================================================

#[test]
fn readonly_or_static_index_signature_has_no_ts1071() {
    // Both modifiers are individually legal, in either order (any ordering
    // diagnostic is a separate concern and is not a TS1071).
    for prefix in [
        "readonly ",
        "static ",
        "static readonly ",
        "readonly static ",
    ] {
        assert_ts1071_count("class", prefix, 0);
    }
}

#[test]
fn bare_index_signature_has_no_ts1071() {
    assert_ts1071_count("class", "", 0);
}
