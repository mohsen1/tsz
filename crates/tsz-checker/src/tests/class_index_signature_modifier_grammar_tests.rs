//! Grammar modifiers on a **class** index signature (TS1071).
//!
//! `tsc`'s `checkGrammarModifiers` accepts only `readonly` and `static` on a
//! class index signature; every other modifier — the accessibility modifiers,
//! `declare`, `abstract`, `async`, `override`, `accessor`, the `in`/`out`
//! variance markers, `const`, `export`, `default` — is rejected with a single
//! `TS1071: '{0}' modifier cannot appear on an index signature.`, anchored at
//! and naming the FIRST offending modifier in the run, after which the walk
//! returns (so a multi-modifier run reports once, not once per modifier).
//!
//! tsz previously covered only `public`/`private`/`protected`/`export` here and
//! reported one diagnostic *per* offending modifier, so `declare [k]`,
//! `abstract [k]`, `async [k]`, `override [k]`, `accessor [k]`, `in`/`out [k]`
//! were silently accepted (the follow-up gap noted in #17276). This suite pins
//! the full rule.
//!
//! Every expectation is oracle-checked against `tsc` (`--noEmit --strict
//! --target es2022 --lib es2022`). Binder/class/parameter names are varied so
//! the diagnostic keys on the modifier position, never on an identifier.

use crate::test_utils::{check_source_codes_with_parse_health, check_source_diagnostics};

const TS1071: u32 = 1071; // '{0}' modifier cannot appear on an index signature.

fn codes(source: &str) -> Vec<u32> {
    check_source_codes_with_parse_health(source)
}

/// `(code, anchored source text)` for the TS1071 diagnostics only, so a test
/// pins both the code and *which* modifier keyword it points at without
/// hard-coding byte offsets.
fn ts1071_anchors(source: &str) -> Vec<(u32, String)> {
    let mut v: Vec<(u32, String)> = check_source_diagnostics(source)
        .iter()
        .filter(|d| d.code == TS1071)
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

// --- the newly-covered single modifiers => exactly one TS1071 ---------------

#[test]
fn each_illegal_single_modifier_reports_one_ts1071() {
    // (modifier keyword, whether the enclosing class must be `abstract`).
    let cases: &[(&str, bool)] = &[
        ("public", false),
        ("private", false),
        ("protected", false),
        ("declare", false),
        ("abstract", true),
        ("async", false),
        ("override", false),
        ("accessor", false),
        ("export", false),
        ("in", false),
        ("out", false),
    ];
    for (modifier, needs_abstract_class) in cases {
        let class_kw = if *needs_abstract_class {
            "abstract class"
        } else {
            "class"
        };
        let source = format!("{class_kw} C {{ {modifier} [k: string]: number; }}");
        assert_eq!(codes(&source), vec![TS1071], "source: {source}");
        assert_eq!(
            ts1071_anchors(&source),
            vec![(TS1071, (*modifier).to_string())],
            "source: {source}"
        );
    }
}

// --- the duplicate-`declare` follow-up from #17276 --------------------------

#[test]
fn duplicate_declare_on_index_signature_reports_single_ts1071_on_first_declare() {
    // `declare declare [k]` — tsc reports TS1071 on the FIRST `declare` and
    // returns, so no TS1030 ("declare modifier already seen") follows.
    let source = "class C { declare declare [k: string]: number; }";
    assert_eq!(codes(source), vec![TS1071]);
    assert_eq!(
        ts1071_anchors(source),
        vec![(TS1071, "declare".to_string())]
    );
}

// --- multi-modifier runs report once, on the first *offending* modifier -----

#[test]
fn static_allowed_prefix_reports_ts1071_on_the_first_illegal_modifier() {
    // `static` is legal on a class index signature, so it is skipped and the
    // diagnostic anchors on the following illegal modifier. (`static declare`
    // is a legal *order* in tsz's parser walk, so no TS1029 ordering error
    // muddies the assertion — the ordering interaction is covered separately in
    // `ordering_conflict_still_reports_ts1071` below.)
    let source = "class C { static declare [k: string]: number; }";
    assert_eq!(codes(source), vec![TS1071]);
    assert_eq!(
        ts1071_anchors(source),
        vec![(TS1071, "declare".to_string())]
    );
}

// --- interaction with the (parser-owned) TS1029 ordering check --------------

#[test]
fn ordering_conflict_still_reports_ts1071_anchored_on_the_illegal_modifier() {
    // `static public [k]` — `public` after `static` is a modifier-*ordering*
    // violation that tsz's parser reports as TS1029, independent of member kind.
    // tsc's single-pass `checkGrammarModifiers` returns at the TS1071 index-
    // signature error before that ordering check runs, so tsc emits only TS1071;
    // tsz still emits the pre-existing parser TS1029 alongside. Suppressing the
    // parser ordering diagnostics for index signatures is a distinct, cross-pass
    // change tracked separately (#17280 "Out of scope"). What this suite owns —
    // that the correct TS1071 now fires, anchored on the illegal modifier — must
    // hold regardless of that residual TS1029.
    let source = "class C { static public [k: string]: number; }";
    let c = codes(source);
    assert!(c.contains(&TS1071), "expected TS1071 in {c:?} for {source}");
    assert_eq!(ts1071_anchors(source), vec![(TS1071, "public".to_string())]);
}

#[test]
fn leading_illegal_modifier_reports_once_even_before_static() {
    // `public static [k]` — tsc reports on `public` (the first modifier) and
    // returns, so the trailing `static` never contributes a second diagnostic.
    let source = "class C { public static [k: string]: number; }";
    assert_eq!(codes(source), vec![TS1071]);
    assert_eq!(ts1071_anchors(source), vec![(TS1071, "public".to_string())]);
}

#[test]
fn readonly_allowed_prefix_reports_ts1071_on_the_following_illegal_modifier() {
    // `readonly` is legal on an index signature; it is skipped like `static`.
    let source = "class C { readonly declare [k: string]: number; }";
    assert_eq!(codes(source), vec![TS1071]);
    assert_eq!(
        ts1071_anchors(source),
        vec![(TS1071, "declare".to_string())]
    );
}

// --- the legal modifiers must NOT draw TS1071 -------------------------------

#[test]
fn readonly_and_static_index_signatures_are_accepted() {
    for source in [
        "class C { readonly [k: string]: number; }",
        "class C { static [k: string]: number; }",
        "class C { static readonly [k: string]: number; }",
        "class C { [k: string]: number; }",
    ] {
        assert!(
            !codes(source).contains(&TS1071),
            "unexpected TS1071 for legal index signature: {source}"
        );
    }
}

// --- independence from the parameter and class identifiers ------------------

#[test]
fn ts1071_is_independent_of_binder_names() {
    for class_name in ["C", "Widget", "Repository", "Zzz"] {
        for param in ["k", "key", "index", "prop"] {
            let source = format!("class {class_name} {{ declare [{param}: string]: number; }}");
            assert_eq!(codes(&source), vec![TS1071], "source: {source}");
            assert_eq!(
                ts1071_anchors(&source),
                vec![(TS1071, "declare".to_string())],
                "source: {source}"
            );
        }
    }
}
