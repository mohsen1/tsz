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

/// Assert no TS2339 (`Property does not exist`) — i.e. inherited members are
/// present on the imported interface. This is the #13554 symptom on its own:
/// the dropped-heritage bug surfaced as TS2339 on every inherited member.
fn assert_no_missing_property(diagnostics: &[Diagnostic]) {
    let missing: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE)
        .map(|d| (d.start, d.message_text.to_string()))
        .collect();
    assert!(
        missing.is_empty(),
        "expected all inherited members to be present (no TS2339), got: {missing:?}",
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

/// Regression (#13554): a *generic* derived interface that `extends` a generic
/// base declared in the same foreign module. The generic reference resolves
/// through `type_reference_symbol_type_with_params`, whose arena-bound heritage
/// merge cannot read the owner module's `extends` clause; before the fix every
/// inherited member tripped TS2339. The non-generic derived form already worked
/// (it delegates), so this covers the distinct generic path.
#[test]
fn imported_generic_interface_inherits_generic_base_members() {
    let diags = check(
        r#"
export interface Base<T> { body?: T; tag: string; }
export interface Derived<T> extends Base<T> { extra?: number; }
"#,
        r#"
import type { Derived } from "./types";
declare const d: Derived<string>;
const b: string | undefined = d.body;
const t: string = d.tag;
const e: number | undefined = d.extra;
"#,
    );
    assert_clean(&diags);
}

/// The inherited member must carry the *instantiated* type, and a wrong-typed
/// use of it must still error — the fix recovers members without widening them.
#[test]
fn imported_generic_interface_inherited_member_keeps_instantiated_type() {
    let diags = check(
        r#"
export interface Base<T> { body?: T; }
export interface Derived<T> extends Base<T> { extra?: number; }
"#,
        r#"
import type { Derived } from "./types";
declare const d: Derived<string>;
const bad: number = d.body ?? 0;
"#,
    );
    let errors = assignability_and_property_errors(&diags);
    assert_eq!(
        errors.len(),
        1,
        "expected exactly one assignability error for the inherited string member, got: {errors:?}",
    );
    assert_eq!(
        errors[0].0,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    );
}

/// A multi-level generic chain across the same foreign module. The transitive
/// bases (`B`, `A`) must contribute their members through the importing file,
/// not just the direct base — every inherited member is present (no TS2339).
///
/// Only member *presence* is asserted here: the exact instantiated type of a
/// transitively-inherited member additionally depends on the driver's
/// `declaration_arenas`/`symbol_arenas` (built by the project driver, not the
/// minimal multi-file unit harness), so full instantiation parity for nested
/// generic chains is covered by the CLI/project path rather than this harness.
#[test]
fn imported_generic_interface_multi_level_chain_members_present() {
    let diags = check(
        r#"
export interface A<T> { a: T; }
export interface B<U> extends A<U[]> { b: U; }
export interface C<V> extends B<V> { c: number; }
"#,
        r#"
import type { C } from "./types";
declare const x: C<string>;
const a = x.a;
const b = x.b;
const c = x.c;
"#,
    );
    assert_no_missing_property(&diags);
}

/// Resolution must follow the interface shape, not the parameter spelling: a
/// renamed/reordered generic base still contributes its members (no TS2339).
#[test]
fn imported_generic_interface_renamed_reordered_members_present() {
    let diags = check(
        r#"
export interface Pair<First, Second> { first: First; second: Second; }
export interface Flipped<Elem, Other> extends Pair<Other, Elem> { own: boolean; }
"#,
        r#"
import type { Flipped } from "./types";
declare const f: Flipped<number, string>;
const a = f.first;
const b = f.second;
const c = f.own;
"#,
    );
    assert_no_missing_property(&diags);
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
