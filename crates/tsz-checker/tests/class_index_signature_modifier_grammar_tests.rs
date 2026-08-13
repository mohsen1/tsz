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

// =========================================================================
// TS4112 — `override` on a class index signature also draws the
// "containing class does not extend another class" diagnostic.
//
// `override` is never legal on a class index signature (TS1071 above), but
// tsc reports TS4112 alongside it exactly as it does for an ordinary member,
// because the index signature is still a member of a class that extends
// nothing. Verified against typescript@7.0.2:
//
//   class K { override [k: string]: number; }
//             ^ (1,11) TS1071   (1,11) TS4112
//   class L extends B { override [k: string]: number; }   -> TS1071 only
//   class M { declare override [k: string]: number; }     -> TS1071 only
//
// Index signatures are type-resolution metadata rather than members, so they
// never reach the own-member summary that carries the ordinary-member TS4112
// walk; before this fix the shape produced TS1071 alone.
// =========================================================================

/// Count of `code` for `<decl>` swept over the class-name × key-name matrix.
fn assert_code_count(decl: &str, code: u32, expected: usize) {
    for name in CLASS_NAMES {
        for key in KEY_NAMES {
            let source = decl.replace("$NAME", name).replace("$KEY", key);
            let count = diags(&source).iter().filter(|&&c| c == code).count();
            assert_eq!(
                count,
                expected,
                "expected {expected} TS{code} in `{source}`, got {:?}",
                diags(&source)
            );
        }
    }
}

#[test]
fn override_index_signature_without_base_reports_ts4112() {
    assert_code_count("class $NAME { override [$KEY: string]: number; }", 4112, 1);
}

#[test]
fn override_index_signature_with_base_does_not_report_ts4112() {
    // With a base class the `override` is still illegal on an index signature
    // (TS1071), but TS4112 is specifically about extending nothing.
    assert_code_count(
        "class Base0 {} class $NAME extends Base0 { override [$KEY: string]: number; }",
        4112,
        0,
    );
}

#[test]
fn declare_override_index_signature_suppresses_ts4112() {
    // `declare` + `override` already produced its own grammar diagnostic, and
    // tsc reports only TS1071 here — the same suppression the ordinary-member
    // walk applies via its `has_declare` guard.
    assert_code_count(
        "class $NAME { declare override [$KEY: string]: number; }",
        4112,
        0,
    );
}

#[test]
fn override_index_signature_still_reports_its_ts1071() {
    // The new TS4112 must not displace the grammar diagnostic: tsc emits both,
    // anchored at the same `override` token.
    assert_ts1071_count("class", "override ", 1);
}

#[test]
fn readonly_and_static_index_signatures_never_report_ts4112() {
    // Legal modifiers, no `override` — the new walk must stay silent.
    for prefix in ["", "readonly ", "static ", "static readonly "] {
        assert_code_count(
            &format!("class $NAME {{ {prefix}[$KEY: string]: number; }}"),
            4112,
            0,
        );
    }
}

#[test]
fn interface_index_signature_never_reports_ts4112() {
    // TS4112 is a class-member rule; an interface index signature is untouched.
    assert_code_count("interface $NAME { [$KEY: string]: number; }", 4112, 0);
}
