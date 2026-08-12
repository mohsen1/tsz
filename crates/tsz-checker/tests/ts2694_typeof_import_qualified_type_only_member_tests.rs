//! TS2694: `typeof import("mod").Member` is a VALUE query. When `Member`
//! exists on the `export =` target but carries no value meaning (an
//! interface or a type alias), `tsc` reports TS2694 — the same diagnostic a
//! genuinely missing member gets, because a type-only member has nothing to
//! offer a value position. Drop the `typeof` and the identical `Member`
//! reference is a perfectly good type, so this is not a member-resolution bug
//! in general; it is the value-meaning filter missing specifically on the
//! qualified `typeof import(...).Member` path (the bare, unqualified form
//! already gets this filter via TS1339, see
//! `ts1339_bare_typeof_import_no_value_tests.rs`).
//!
//! Oracle-verified against pinned `typescript@7.0.2` for diagnostic CODE and
//! member name. The namespace-name text now also matches tsc: a named
//! `export = N` target renders the target symbol `N` (not `"m".export=`),
//! fixed in #17208.
//!
//! Sibling of #17076/TS1339 — same underlying question ("does this
//! import-type reference name a value?") asked at the qualified position
//! instead of the bare one.
//!
//! Owner: `crates/tsz-checker/src/state/type_analysis/core_type_query.rs`
//! (`try_resolve_typeof_import_segment_via_export_equals`), the export=-target
//! fallback `resolve_typeof_import_query` reaches for a first segment that
//! the ordinary namespace-member path (`resolve_namespace_typeof_member`,
//! which already applies this same value-only filter) does not resolve.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

const TS2694: u32 = 2694;

fn check(files: &[(&str, &str)], entry: &str) -> Vec<(u32, String)> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn codes(diagnostics: &[(u32, String)]) -> Vec<u32> {
    let mut codes: Vec<u32> = diagnostics.iter().map(|(code, _)| *code).collect();
    codes.sort_unstable();
    codes
}

/// `export = <namespace>` whose member is an interface: the member exists but
/// has no value meaning, so `typeof import(...).Q` cannot name it.
#[test]
fn export_equals_namespace_interface_member_reports_ts2694() {
    let diags = check(
        &[
            (
                "/m.ts",
                "namespace N {\n  export interface Q {}\n}\nexport = N;\n",
            ),
            ("/main.ts", "type T = typeof import(\"./m\").Q;\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diags), vec![TS2694], "{diags:?}");
    let (_, message) = diags.iter().find(|(code, _)| *code == TS2694).unwrap();
    // Named `export = N` target: tsc names the target symbol `N`, not
    // `"m".export=` (#17208).
    assert_eq!(message, "Namespace 'N' has no exported member 'Q'.");
}

/// Same value-less shape via a type alias member instead of an interface.
#[test]
fn export_equals_namespace_type_alias_member_reports_ts2694() {
    let diags = check(
        &[
            (
                "/m.ts",
                "namespace N {\n  export type Q = string;\n}\nexport = N;\n",
            ),
            ("/main.ts", "type T = typeof import(\"./m\").Q;\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diags), vec![TS2694], "{diags:?}");
    let (_, message) = diags.iter().find(|(code, _)| *code == TS2694).unwrap();
    assert_eq!(message, "Namespace 'N' has no exported member 'Q'.");
}

/// Negative control that keeps the fix honest: a genuinely missing member on
/// the same type-only namespace must still report TS2694 with the same
/// message shape — a fix keyed on "member not found" would coincidentally
/// pass the two tests above without fixing the real gap, so this pins that
/// the code path is not accidentally suppressing all output.
#[test]
fn export_equals_namespace_missing_member_reports_ts2694() {
    let diags = check(
        &[
            (
                "/m.ts",
                "namespace N {\n  export interface Q {}\n}\nexport = N;\n",
            ),
            ("/main.ts", "type T = typeof import(\"./m\").Nope;\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diags), vec![TS2694], "{diags:?}");
    let (_, message) = diags.iter().find(|(code, _)| *code == TS2694).unwrap();
    assert_eq!(message, "Namespace 'N' has no exported member 'Nope'.");
}

/// Negative control: a value member (`const v`) on the same shape of
/// namespace resolves cleanly — the filter must not over-suppress value
/// members reached through this same export=-target fallback path.
#[test]
fn export_equals_namespace_value_member_is_clean() {
    let diags = check(
        &[
            (
                "/m.ts",
                "namespace N {\n  export const v = 1;\n}\nexport = N;\n",
            ),
            ("/main.ts", "type T = typeof import(\"./m\").v;\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diags), Vec::<u32>::new(), "{diags:?}");
}

/// Negative control: a missing member on a value-instantiated namespace still
/// reports TS2694 — this shape resolves through the ordinary namespace-member
/// path (not the export=-target fallback this PR touches), so it must be
/// unaffected.
#[test]
fn export_equals_namespace_value_member_missing_sibling_reports_ts2694() {
    let diags = check(
        &[
            (
                "/m.ts",
                "namespace N {\n  export const v = 1;\n}\nexport = N;\n",
            ),
            ("/main.ts", "type T = typeof import(\"./m\").Nope;\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diags), vec![TS2694], "{diags:?}");
    let (_, message) = diags.iter().find(|(code, _)| *code == TS2694).unwrap();
    assert_eq!(message, "Namespace 'N' has no exported member 'Nope'.");
}

/// Negative control: a plain module (no `export =` at all) reports TS2694 for
/// a missing member too, unaffected by this fix (it never reaches the
/// export=-target fallback).
#[test]
fn plain_module_missing_member_reports_ts2694() {
    let diags = check(
        &[
            ("/m.ts", "export const v = 1;\n"),
            ("/main.ts", "type T = typeof import(\"./m\").Nope;\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diags), vec![TS2694], "{diags:?}");
}

/// Negative control: a plain module's actual value export resolves cleanly.
#[test]
fn plain_module_value_member_is_clean() {
    let diags = check(
        &[
            ("/m.ts", "export const v = 1;\n"),
            ("/main.ts", "type T = typeof import(\"./m\").v;\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diags), Vec::<u32>::new(), "{diags:?}");
}

/// Nested-namespace adjacent case: only the FIRST unqualified segment goes
/// through the export=-target fallback this PR changes (`resolved_segments`
/// must be empty); a second-segment miss inside an already-resolved nested
/// namespace takes a different path entirely and must stay unaffected.
#[test]
fn nested_namespace_missing_member_reports_ts2694() {
    let diags = check(
        &[
            (
                "/m.ts",
                "namespace N {\n  export namespace M {\n    export const v = 1;\n  }\n}\nexport = N;\n",
            ),
            ("/main.ts", "type T = typeof import(\"./m\").M.Nope;\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diags), vec![TS2694], "{diags:?}");
    let (_, message) = diags.iter().find(|(code, _)| *code == TS2694).unwrap();
    assert_eq!(message, "Namespace 'N.M' has no exported member 'Nope'.");
}

/// The discriminator: drop `typeof` and the identical `Q` reference is a
/// perfectly good TYPE, so it must stay clean. This is what rules out any fix
/// keyed on "member not found" — `Q` is found, and correctly so; it just has
/// no value meaning for the `typeof`-qualified value position.
#[test]
fn qualified_import_type_without_typeof_on_interface_member_is_clean() {
    let diags = check(
        &[
            (
                "/m.ts",
                "namespace N {\n  export interface Q {}\n}\nexport = N;\n",
            ),
            ("/main.ts", "type T = import(\"./m\").Q;\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diags), Vec::<u32>::new(), "{diags:?}");
}

/// Renamed-binder adjacent case: the interface's name must not be
/// load-bearing.
#[test]
fn renamed_export_equals_namespace_interface_member_reports_ts2694() {
    let diags = check(
        &[
            (
                "/m.ts",
                "namespace SomethingElse {\n  export interface Whatever {}\n}\nexport = SomethingElse;\n",
            ),
            ("/main.ts", "type T = typeof import(\"./m\").Whatever;\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diags), vec![TS2694], "{diags:?}");
}
