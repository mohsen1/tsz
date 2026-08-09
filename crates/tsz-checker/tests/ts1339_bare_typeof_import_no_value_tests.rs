//! TS1339: a bare `typeof import("mod")` — no `.Member` qualifier — names
//! the module's own value. A module with no `export =` always has one (the
//! module object itself is the runtime value, regardless of what it
//! exports), so this only fires when the module's `export =` target is
//! itself type-only: an interface, a type alias, or an uninstantiated
//! namespace (one whose members are all types). `tsc` reports:
//! "Module '{0}' does not refer to a value, but is used as a value here."
//!
//! Sibling of TS1340 (`bare_import_type_names_a_type` /
//! `MODULE_DOES_NOT_REFER_TO_A_TYPE_BUT_IS_USED_AS_A_TYPE_HERE`), which
//! covers the converse: `import("mod")` used in a type position when the
//! `export =` target is value-only. Oracle-verified against
//! `typescript@6.0.2`.
//!
//! Owner: `crates/tsz-checker/src/state/type_resolution/import_type_meaning.rs`
//! (`bare_typeof_import_names_a_value`), consumed from
//! `crates/tsz-checker/src/state/type_analysis/core_type_query.rs`
//! (`resolve_typeof_import_query`) — the same `typeof import(...)`
//! namespace-builder `typeof import("mod").Member` already uses, gated on
//! the qualifier segments being empty.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

const TS1339: u32 = 1339;

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

/// `export = <interface>`: the module's only value-side meaning is gone —
/// the interface has no runtime existence — so `typeof import(...)` cannot
/// name a value.
#[test]
fn export_equals_interface_reports_ts1339() {
    let diags = check(
        &[
            (
                "/mod.ts",
                "export interface Y {\n  a: number;\n}\nexport = Y;\n",
            ),
            ("/main.ts", "type X = typeof import(\"./mod\");\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diags), vec![TS1339], "{diags:?}");
    let (_, message) = diags.iter().find(|(code, _)| *code == TS1339).unwrap();
    assert_eq!(
        message,
        "Module './mod' does not refer to a value, but is used as a value here."
    );
}

/// `export = <type alias>`: same value-less shape as the interface case.
#[test]
fn export_equals_type_alias_reports_ts1339() {
    let diags = check(
        &[
            ("/mod.ts", "export type T = number;\nexport = T;\n"),
            ("/main.ts", "type X = typeof import(\"./mod\");\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diags), vec![TS1339], "{diags:?}");
}

/// `export = <namespace>` where the namespace's members are all types: the
/// namespace itself never instantiates a runtime object, so it has no value.
#[test]
fn export_equals_uninstantiated_namespace_reports_ts1339() {
    let diags = check(
        &[
            (
                "/mod.ts",
                "namespace N {\n  export interface I {}\n}\nexport = N;\n",
            ),
            ("/main.ts", "type X = typeof import(\"./mod\");\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diags), vec![TS1339], "{diags:?}");
}

/// Renamed-binder adjacent case: the interface's name must not be
/// load-bearing.
#[test]
fn renamed_export_equals_interface_reports_ts1339() {
    let diags = check(
        &[
            (
                "/mod.ts",
                "export interface SomethingElse {\n  a: number;\n}\nexport = SomethingElse;\n",
            ),
            ("/main.ts", "type X = typeof import(\"./mod\");\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diags), vec![TS1339], "{diags:?}");
}

/// Negative control: `export = <class>` — a class carries a value meaning
/// (it is constructible at runtime) alongside its type meaning, so a bare
/// `typeof import(...)` is legal.
#[test]
fn export_equals_class_is_clean() {
    let diags = check(
        &[
            ("/mod.ts", "export class C {}\nexport = C;\n"),
            ("/main.ts", "type X = typeof import(\"./mod\");\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diags), Vec::<u32>::new(), "{diags:?}");
}

/// Negative control: `export = <namespace with a value member>` — the
/// namespace instantiates a runtime object, so it has value.
#[test]
fn export_equals_instantiated_namespace_is_clean() {
    let diags = check(
        &[
            (
                "/mod.ts",
                "namespace N {\n  export const a = 1;\n}\nexport = N;\n",
            ),
            ("/main.ts", "type X = typeof import(\"./mod\");\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diags), Vec::<u32>::new(), "{diags:?}");
}

/// Negative control: no `export =` at all — the module object itself is
/// always a value, regardless of whether every export is a type.
#[test]
fn no_export_equals_type_only_module_is_clean() {
    let diags = check(
        &[
            (
                "/mod.ts",
                "export interface Y {}\nexport type Z = number;\n",
            ),
            ("/main.ts", "type X = typeof import(\"./mod\");\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diags), Vec::<u32>::new(), "{diags:?}");
}

/// Qualified form is unaffected: `typeof import("mod").Member` walks through
/// the namespace builder rather than naming the module's own value, so it
/// does not go through the bare-value check at all.
#[test]
fn qualified_typeof_import_on_export_equals_interface_is_not_ts1339() {
    let diags = check(
        &[
            (
                "/mod.ts",
                "export interface Y {\n  a: number;\n}\nexport = Y;\n",
            ),
            ("/main.ts", "type X = typeof import(\"./mod\").a;\n"),
        ],
        "/main.ts",
    );
    assert!(
        !codes(&diags).contains(&TS1339),
        "qualified typeof import must not report TS1339: {diags:?}"
    );
}

/// Plain `import("mod")` (no `typeof`) used as a type is the sibling TS1340
/// diagnostic family, not TS1339 — the two must stay distinct.
#[test]
fn bare_import_type_no_typeof_on_export_equals_value_reports_ts1340_not_ts1339() {
    let diags = check(
        &[
            ("/mod.ts", "export const y = 1;\nexport = y;\n"),
            ("/main.ts", "type X = import(\"./mod\");\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diags), vec![1340], "{diags:?}");
}
