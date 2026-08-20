//! Regression + parity tests for the re-export resolution memo.
//!
//! `resolve_export_in_file` walks `export *` / named-re-export chains across
//! binder boundaries. Barrel-heavy programs resolve the *same* export name from
//! many import/usage sites; without a memo each resolution re-walks the entire
//! `export *` graph (`O(names × export-edges)` — the `ts-morph` canary timeout,
//! issue #13508 root cause A). The fix memoizes root resolutions keyed by
//! `(file_idx, export_name)`.
//!
//! These tests pin the *behavior* the memo must preserve: a name is resolved to
//! the same target however many times it is requested, the cache never collapses
//! a re-exported binding to `any`, a genuinely absent name still reports
//! `TS2305`, and the result is identical whether or not the `export *` graph
//! contains a cycle (the case where a naive memo would otherwise poison itself
//! with the cycle-break sentinel). Binder names are varied across cases so the
//! behavior follows structure, not identifier text.

use crate::context::CheckerOptions;
use crate::diagnostics::{Diagnostic, diagnostic_codes};
use crate::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

fn check(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    )
}

fn codes(diags: &[Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

fn assert_no_code(diags: &[Diagnostic], code: u32) {
    assert!(
        !codes(diags).contains(&code),
        "did not expect diagnostic {code}, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

fn assert_has_code(diags: &[Diagnostic], code: u32) {
    assert!(
        codes(diags).contains(&code),
        "expected diagnostic {code}, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

/// A name re-exported through a barrel `export *` resolves to its real declared
/// type — repeatedly, from several usage sites in the same file — and never
/// collapses to `any`. The first usage misuse is a genuine `TS2322`; if the memo
/// served `any` for later usages those mismatches would be silently dropped.
#[test]
fn barrel_export_star_resolves_named_type_every_usage() {
    let diags = check(
        &[
            (
                "./index.ts",
                r#"export * from "./shapes";
export * from "./colors";
"#,
            ),
            (
                "./shapes.ts",
                r#"export interface Shape { sides: number; }
"#,
            ),
            (
                "./colors.ts",
                r#"export type Color = "red" | "green";
"#,
            ),
            (
                "./consumer.ts",
                r#"import { Shape, Color } from "./index";
const a: Shape = { sides: 3 };
const b: Shape = { sides: 4 };
const c: Color = "red";
const bad: Shape = { sides: "no" };
const wrongColor: Color = "purple";
"#,
            ),
        ],
        "./consumer.ts",
    );
    // Real declared types resolve every time (not `any`): both misuses report.
    assert_has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    let assignability_errors = codes(&diags)
        .iter()
        .filter(|&&c| c == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .count();
    assert!(
        assignability_errors >= 2,
        "both the bad Shape and the bad Color should report; got {assignability_errors}: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>(),
    );
}

/// `export *` cycle (the immer/ts-morph shape): `barrel.ts` re-exports `leaf.ts`
/// while `leaf.ts` imports back from `barrel.ts`. The re-exported name must still
/// resolve to its real type. A memo that cached the cycle-break sentinel `None`
/// would make the import report a spurious `TS2305`.
#[test]
fn cyclic_export_star_still_resolves_reexported_name() {
    let diags = check(
        &[
            (
                "./barrel.ts",
                r#"export * from "./leaf";
export const VERSION = 1;
"#,
            ),
            (
                "./leaf.ts",
                r#"import { VERSION } from "./barrel";
export interface Widget { id: number; }
export const tag = VERSION;
"#,
            ),
            (
                "./app.ts",
                r#"import { Widget } from "./barrel";
const w: Widget = { id: 7 };
const w2: Widget = { id: 8 };
const broken: Widget = { id: "x" };
"#,
            ),
        ],
        "./app.ts",
    );
    assert_no_code(&diags, diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER);
    // The cycle does not erase the type: the bad assignment still reports.
    assert_has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
}

/// A genuinely absent name still reports `TS2305` even when the barrel sits over
/// a cycle — i.e. the memo caches a real "not exported" miss, not a transient
/// cycle-break `None`, and never invents an export.
#[test]
fn cyclic_barrel_missing_name_reports_no_exported_member() {
    let diags = check(
        &[
            (
                "./hub.ts",
                r#"export * from "./node";
export const SEED = 0;
"#,
            ),
            (
                "./node.ts",
                r#"import { SEED } from "./hub";
export interface Known { ok: boolean; }
export const seedEcho = SEED;
"#,
            ),
            (
                "./client.ts",
                r#"import { Known, Missing } from "./hub";
const k: Known = { ok: true };
"#,
            ),
        ],
        "./client.ts",
    );
    assert_has_code(&diags, diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER);
}

/// Renamed binders: the resolution/memo follows the `export *` structure, not the
/// `Shape`/`index` identifier text used above.
#[test]
fn renamed_binders_resolve_through_nested_barrels() {
    let diags = check(
        &[
            (
                "./top.ts",
                r#"export * from "./mid";
"#,
            ),
            (
                "./mid.ts",
                r#"export * from "./bottom";
"#,
            ),
            (
                "./bottom.ts",
                r#"export interface Gizmo { weight: number; }
"#,
            ),
            (
                "./use.ts",
                r#"import { Gizmo } from "./top";
const g: Gizmo = { weight: 1 };
const g2: Gizmo = { weight: 2 };
const wrong: Gizmo = { weight: true };
"#,
            ),
        ],
        "./use.ts",
    );
    assert_no_code(&diags, diagnostic_codes::MODULE_HAS_NO_EXPORTED_MEMBER);
    assert_has_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
}
