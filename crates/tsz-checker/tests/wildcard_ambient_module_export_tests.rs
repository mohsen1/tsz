//! Regression coverage for #14851: exports of *wildcard / pattern* ambient
//! modules (`declare module "*.ext"` / `declare module "prefix/*"`) must keep
//! their declared types when imported through a concrete specifier.
//!
//! Structural rule: when an import specifier is satisfied by a wildcard ambient
//! module rather than an exact `declare module "name"`, the export-table lookup
//! (`CheckerContext::module_exports_for_module`) resolves the concrete specifier
//! onto the matching *pattern* module's export table — the `*.svg` / `prefix/*`
//! key — rather than the concrete specifier key (which never matches). Without
//! this every imported binding degraded to `any` and all downstream
//! assignability errors were silently dropped; tsc resolves the specifier onto
//! the pattern module and types each export from its declaration.
//!
//! Scope note: the `default`-export *value* of an ambient module (wildcard or
//! not) is resolved through a separate path with a pre-existing defect that is
//! out of scope here — it reproduces identically for a plain
//! `declare module "name"`, so it is not a wildcard regression. These tests
//! therefore exercise named exports, namespace imports, prefix patterns, and
//! exact-vs-wildcard precedence, which the export-table routing fully covers.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;

fn diagnostics(files: &[(&str, &str)], entry: &str) -> Vec<(u32, String)> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            strict: true,
            ..Default::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn count_code(diags: &[(u32, String)], expected: u32) -> usize {
    diags.iter().filter(|(code, _)| *code == expected).count()
}

#[test]
fn wildcard_ext_module_named_export_keeps_declared_type_both_directions() {
    // The named binding `width: number` is the issue's core repro. Both
    // assignability directions must surface, proving the binding is `number`
    // (not `any`).
    let diags = diagnostics(
        &[
            (
                "globals.d.ts",
                r#"
declare module "*.svg" {
  export const width: number;
}
"#,
            ),
            (
                "main.ts",
                r#"
import { width } from "./logo.svg";
const ok: number = width;
const bad: string = width;  // number -> string
"#,
            ),
        ],
        "main.ts",
    );

    assert_eq!(
        count_code(&diags, 2322),
        1,
        "expected exactly one TS2322 (number -> string); got {diags:#?}"
    );
}

#[test]
fn wildcard_ext_module_named_object_export_property_access() {
    // A named object export resolves structurally, so a missing property is a
    // real TS2339 rather than being swallowed by an `any` binding.
    let diags = diagnostics(
        &[
            (
                "globals.d.ts",
                r#"
declare module "*.svg" {
  export const meta: { width: number };
}
"#,
            ),
            (
                "main.ts",
                r#"
import { meta } from "./logo.svg";
meta.height; // not on { width: number }
"#,
            ),
        ],
        "main.ts",
    );

    assert!(
        count_code(&diags, 2339) >= 1,
        "expected TS2339 for a missing property on a wildcard named export; got {diags:#?}"
    );
}

#[test]
fn wildcard_prefix_module_named_export_keeps_declared_type() {
    // Vary the binder/property names and the pattern shape (`prefix/*`) so the
    // fix can't be a name-specific or extension-specific fast path.
    let diags = diagnostics(
        &[
            (
                "shims.d.ts",
                r#"
declare module "prefix/*" {
  export const handle: number;
}
"#,
            ),
            (
                "main.ts",
                r#"
import { handle } from "prefix/anything";
const ok: number = handle;
const bad: string = handle; // number -> string
"#,
            ),
        ],
        "main.ts",
    );

    assert_eq!(
        count_code(&diags, 2322),
        1,
        "expected one TS2322 (number -> string) for a prefix-pattern module; got {diags:#?}"
    );
}

#[test]
fn wildcard_module_namespace_import_property_access_uses_declared_type() {
    let diags = diagnostics(
        &[
            (
                "assets.d.ts",
                r#"
declare module "*.css" {
  export const classes: { root: string };
}
"#,
            ),
            (
                "main.ts",
                r#"
import * as styles from "./app.css";
const ok: string = styles.classes.root;
styles.classes.missing; // not on { root: string }
"#,
            ),
        ],
        "main.ts",
    );

    assert!(
        count_code(&diags, 2339) >= 1,
        "expected TS2339 through a wildcard namespace import; got {diags:#?}"
    );
}

#[test]
fn wildcard_module_named_import_assignment_to_mismatched_object() {
    // A missing target property on assignment must surface (TS2741), proving the
    // imported object binding carries its declared shape, not `any`.
    let diags = diagnostics(
        &[
            (
                "assets.d.ts",
                r#"
declare module "*.json" {
  export const value: { expected: number };
}
"#,
            ),
            (
                "main.ts",
                r#"
import { value } from "./data.json";
const assigned: { nope: number } = value;
"#,
            ),
        ],
        "main.ts",
    );

    assert!(
        count_code(&diags, 2741) >= 1 || count_code(&diags, 2322) >= 1,
        "expected a TS2741/TS2322 for a mismatched wildcard object import; got {diags:#?}"
    );
}

#[test]
fn exact_ambient_module_still_preferred_over_wildcard() {
    // An exact `declare module "name"` must win over a same-project wildcard so
    // the pattern fallback never shadows a precise declaration.
    let diags = diagnostics(
        &[
            (
                "globals.d.ts",
                r#"
declare module "*.svg" {
  export const value: string;
}
declare module "exact.svg" {
  export const value: number;
}
"#,
            ),
            (
                "main.ts",
                r#"
import { value } from "exact.svg";
const bad: string = value; // exact module wins: value is number -> TS2322
"#,
            ),
        ],
        "main.ts",
    );

    assert_eq!(
        count_code(&diags, 2322),
        1,
        "expected the exact ambient module to win (number -> string TS2322); got {diags:#?}"
    );
}

#[test]
fn longest_prefix_wildcard_pattern_wins() {
    // When two patterns match the same specifier, the longer literal prefix wins
    // (tsc's `findBestPatternMatch`). `vendor/*` is more specific than `*`.
    let diags = diagnostics(
        &[
            (
                "globals.d.ts",
                r#"
declare module "*" {
  export const member: string;
}
declare module "vendor/*" {
  export const member: number;
}
"#,
            ),
            (
                "main.ts",
                r#"
import { member } from "vendor/thing";
const bad: string = member; // vendor/* wins: member is number -> TS2322
"#,
            ),
        ],
        "main.ts",
    );

    assert_eq!(
        count_code(&diags, 2322),
        1,
        "expected the longest-prefix pattern (vendor/*) to win (number -> string); got {diags:#?}"
    );
}
