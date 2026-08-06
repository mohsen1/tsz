//! Regression coverage for #16498: a default-imported `enum` or
//! `function`+`namespace` merge must resolve as a qualified-name ROOT.
//!
//! `tsc` reports `TS2694` for a missing member (the target has namespace
//! meaning: `Enum` is part of `SymbolFlags.Namespace = ValueModule |
//! NamespaceModule | Enum`) or is clean for a present member. #16498 reported
//! tsz instead resolving `TS2503 "Cannot find namespace 'C'."` for both,
//! because the identifier `C` failed to resolve with *any* type meaning at
//! all — before `qualified_names.rs`'s namespace-meaning gate (the one
//! #16486 fixed) is ever reached.
//!
//! **Verified already fixed on current `main` before writing any src change**
//! (checked with both `check_multi_file_with_global_index`, the production-
//! faithful harness the sibling `ts2702_qualifier_namespace_meaning_tests.rs`
//! suite uses, and `check_multi_file_with_libs_stamped`, the harness #16498's
//! own repro specified) — every row below already matches `tsc` 7.0.2. Pinning
//! the adjacent matrix as a regression floor rather than leaving this
//! unverified on the board a second time.

use tsz_checker::test_utils::{
    check_multi_file_with_global_index, check_multi_file_with_libs_stamped,
};
use tsz_common::CheckerOptions;

fn multi_file_diags(files: &[(&str, &str)], entry: &str) -> Vec<(u32, String)> {
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    };
    check_multi_file_with_global_index(files, entry, options)
        .into_iter()
        .map(|diagnostic| (diagnostic.code, diagnostic.message_text))
        .collect()
}

fn multi_file_codes(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    multi_file_diags(files, entry)
        .into_iter()
        .map(|(code, _)| code)
        .collect()
}

/// Like `multi_file_codes`, but through `check_multi_file_with_libs_stamped`
/// (empty lib set) -- the exact harness #16498 used for its own repro.
fn stamped_codes(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    };
    check_multi_file_with_libs_stamped(files, entry, options, &[])
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// Default-imported enum, missing member: `tsc` -> `TS2694`. #16498 reported
/// `TS2503`, because `C` never resolved as a type-position identifier.
#[test]
fn default_imported_enum_missing_member_reports_ts2694_not_ts2503() {
    let diags = multi_file_diags(
        &[
            (
                "e1.ts",
                "export enum Color { Red, Green }\nexport default Color;\n",
            ),
            ("e2.ts", "import C from \"./e1\";\nvar bad: C.Nope;\n"),
        ],
        "e2.ts",
    );
    assert_eq!(
        diags,
        vec![(
            2694,
            "Namespace 'Color' has no exported member 'Nope'.".to_string()
        )],
        "expected TS2694 naming the resolved namespace, got {diags:?}"
    );
}

/// Same shape through the `check_multi_file_with_libs_stamped` harness
/// #16498's repro specified.
#[test]
fn default_imported_enum_missing_member_reports_ts2694_stamped_harness() {
    let codes = stamped_codes(
        &[
            (
                "e1.ts",
                "export enum Color { Red, Green }\nexport default Color;\n",
            ),
            ("e2.ts", "import C from \"./e1\";\nvar bad: C.Nope;\n"),
        ],
        "e2.ts",
    );
    assert_eq!(codes, vec![2694], "got {codes:?}");
}

/// Default-imported enum, present member: `tsc` is clean.
#[test]
fn default_imported_enum_present_member_is_clean() {
    let codes = multi_file_codes(
        &[
            (
                "e1.ts",
                "export enum Color { Red, Green }\nexport default Color;\n",
            ),
            ("e2.ts", "import C from \"./e1\";\nvar ok: C.Red;\n"),
        ],
        "e2.ts",
    );
    assert_eq!(codes, Vec::<u32>::new(), "expected clean, got {codes:?}");
}

#[test]
fn default_imported_enum_present_member_is_clean_stamped_harness() {
    let codes = stamped_codes(
        &[
            (
                "e1.ts",
                "export enum Color { Red, Green }\nexport default Color;\n",
            ),
            ("e2.ts", "import C from \"./e1\";\nvar ok: C.Red;\n"),
        ],
        "e2.ts",
    );
    assert_eq!(codes, Vec::<u32>::new(), "expected clean, got {codes:?}");
}

/// A `function` + `namespace` merge, default-exported: `tsc` is clean for a
/// present member (a `Function+NamespaceModule` merge keeps namespace
/// meaning).
#[test]
fn default_imported_function_namespace_merge_member_is_clean() {
    let codes = multi_file_codes(
        &[
            (
                "f1.ts",
                "export function Decl3() {}\nexport namespace Decl3 { export interface I { q: number } }\nexport default Decl3;\n",
            ),
            ("f2.ts", "import F from \"./f1\";\nvar y: F.I;\n"),
        ],
        "f2.ts",
    );
    assert_eq!(codes, Vec::<u32>::new(), "expected clean, got {codes:?}");
}

#[test]
fn default_imported_function_namespace_merge_member_is_clean_stamped_harness() {
    let codes = stamped_codes(
        &[
            (
                "f1.ts",
                "export function Decl3() {}\nexport namespace Decl3 { export interface I { q: number } }\nexport default Decl3;\n",
            ),
            ("f2.ts", "import F from \"./f1\";\nvar y: F.I;\n"),
        ],
        "f2.ts",
    );
    assert_eq!(codes, Vec::<u32>::new(), "expected clean, got {codes:?}");
}

/// Negative control: a bare default-exported `namespace` (no enum/function
/// merge) already resolves correctly -- must keep working.
#[test]
fn default_imported_bare_namespace_member_is_clean() {
    let codes = multi_file_codes(
        &[
            (
                "n1.ts",
                "namespace m { export interface Foo { q: number } }\nexport default m;\n",
            ),
            ("n2.ts", "import D from \"./n1\";\nvar y: D.Foo;\n"),
        ],
        "n2.ts",
    );
    assert_eq!(codes, Vec::<u32>::new(), "expected clean, got {codes:?}");
}

/// Renamed binders: the rule must key on symbol flags, not identifier
/// spelling.
#[test]
fn default_imported_enum_renamed_binders_missing_member_reports_ts2694() {
    let diags = multi_file_diags(
        &[
            (
                "palette.ts",
                "export enum Hue { Amber, Cerulean }\nexport default Hue;\n",
            ),
            (
                "consumer.ts",
                "import Local from \"./palette\";\nvar wrong: Local.Nonexistent;\n",
            ),
        ],
        "consumer.ts",
    );
    assert_eq!(
        diags,
        vec![(
            2694,
            "Namespace 'Hue' has no exported member 'Nonexistent'.".to_string()
        )],
        "expected TS2694 naming the resolved namespace, got {diags:?}"
    );
}

/// Negative control: a genuinely missing member on a default-imported class
/// must still be an error (not silently accepted) -- classes have no
/// namespace meaning on their own, so this stays TS2702/TS2713, never
/// TS2694/TS2503.
#[test]
fn default_imported_class_qualifier_is_not_a_namespace() {
    let codes = multi_file_codes(
        &[
            ("c1.ts", "export class Widget {}\nexport default Widget;\n"),
            ("c2.ts", "import W from \"./c1\";\nvar bad: W.Nope;\n"),
        ],
        "c2.ts",
    );
    assert!(
        codes.contains(&2702) || codes.contains(&2713),
        "expected TS2702/TS2713 (class has no namespace meaning), got {codes:?}"
    );
    assert!(
        !codes.contains(&2503) && !codes.contains(&2694),
        "class qualifier must not be treated as a namespace, got {codes:?}"
    );
}
