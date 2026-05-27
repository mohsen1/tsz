//! Cross-file interface property access coverage.
//!
//! Property access on an imported interface alias resolves through a bare
//! `Lazy(DefId)` base. When the cached property evaluator cannot resolve that
//! base it falls back to `any`; the checker then re-queries through the solver
//! evaluator with its own `TypeResolver` so member types are resolved
//! structurally in the solver rather than by checker-local AST walking.
//!
//! These cases vary the member kind, heritage, and type-parameter spelling to
//! prove the behavior follows the type shape rather than any particular
//! identifier name.

use crate::context::CheckerOptions;
use crate::diagnostics::{Diagnostic, diagnostic_codes};
use crate::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

fn check(types_src: &str, main_src: &str) -> Vec<Diagnostic> {
    check_multi_file(
        &[("./types.ts", types_src), ("./main.ts", main_src)],
        "./main.ts",
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            ..CheckerOptions::default()
        },
    )
}

fn assignability_and_property_errors(diagnostics: &[Diagnostic]) -> Vec<(u32, u32, String)> {
    diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                || d.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE
        })
        .map(|d| (d.code, d.start, d.message_text.to_string()))
        .collect()
}

fn assert_clean(diagnostics: &[Diagnostic]) {
    let relevant = assignability_and_property_errors(diagnostics);
    assert!(
        relevant.is_empty(),
        "expected cross-file interface members to resolve, got: {relevant:?}",
    );
}

#[test]
fn imported_interface_own_members_resolve() {
    let diags = check(
        r#"export interface Plain { value: number; tag: string; }"#,
        r#"
import type { Plain } from "./types";
declare const p: Plain;
const v: number = p.value;
const t: string = p.tag;
"#,
    );
    assert_clean(&diags);
}

#[test]
fn imported_interface_inherited_generic_members_resolve() {
    let diags = check(
        r#"
export interface Box<T> { value: T; tag: string; }
export interface NumBox extends Box<number> { extra: boolean; }
"#,
        r#"
import type { NumBox } from "./types";
declare const b: NumBox;
const v: number = b.value;
const t: string = b.tag;
const e: boolean = b.extra;
"#,
    );
    assert_clean(&diags);
}

/// The fix must follow the interface shape, not the type-parameter spelling.
/// Renaming the bound parameter (`T` -> `Elem`) must not change resolution.
#[test]
fn imported_interface_resolution_is_type_param_name_agnostic() {
    let diags = check(
        r#"
export interface Box<Elem> { value: Elem; tag: string; }
export interface StrBox extends Box<string> { extra: number; }
"#,
        r#"
import type { StrBox } from "./types";
declare const b: StrBox;
const v: string = b.value;
const e: number = b.extra;
"#,
    );
    assert_clean(&diags);
}

#[test]
fn imported_interface_index_signature_member_resolves() {
    let diags = check(
        r#"export interface Bag { [key: string]: number; }"#,
        r#"
import type { Bag } from "./types";
declare const bag: Bag;
const v: number = bag.anything;
"#,
    );
    assert_clean(&diags);
}

/// Negative case: a genuinely missing property still reports TS2339 rather than
/// being silently resolved to `any`. The resolver re-query only replaces the
/// `any` fallback with an *improved* result, so `PropertyNotFound` is preserved.
#[test]
fn imported_interface_missing_member_reports_ts2339() {
    let diags = check(
        r#"export interface Plain { value: number; }"#,
        r#"
import type { Plain } from "./types";
declare const p: Plain;
const bad = p.missing;
"#,
    );
    let property_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE)
        .collect();
    assert_eq!(
        property_errors.len(),
        1,
        "expected exactly one TS2339 for the missing member, got: {:?}",
        assignability_and_property_errors(&diags),
    );
}
