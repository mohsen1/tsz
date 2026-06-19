//! Regression tests: a generic interface or class that reaches a file through a
//! barrel re-export (`export { X } from` / `export type { X } from`) and is then
//! used in a **type-reference position** (`const x: X<number>`, or a property
//! annotation) must instantiate its body — i.e. substitute the supplied type
//! argument for the re-exported declaration's type parameter — exactly like a
//! direct single-hop import.
//!
//! Structural rule: lowering a type reference builds
//! `Application(Lazy(DefId), args)`. The `DefId` must be keyed to the
//! *declaring* `(SymbolId, file)`. When the name reaches the file through a
//! barrel, the generic name-resolver previously derived the `DefId`'s file from
//! `decl_file_idx` / current-file-local heuristics and a raw-`SymbolId` cache
//! (raw ids collide across binders), so it could attribute the `DefId` to the
//! intermediate re-export file instead of the declaration. That non-canonical
//! `DefId` has no generic body registered, so the application stayed opaque and
//! its type parameter was never substituted — `{ value: number }` then failed
//! to assign to `Base<number>` with a false TS2322. The fix routes the
//! re-export chain to the declaration together with its file index and keys the
//! `DefId` to that declaration (mirroring the heritage path, #13803).
//!
//! Witnessed in the kysely / valibot rows (#10663 / #13212), where
//! barrel-re-exported generics used in annotation/property positions produced
//! false TS2322 (the "non-canonical instantiation" family).

use tsz_checker::test_utils::check_multi_file;
use tsz_common::CheckerOptions;

fn strict_opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        module: tsz_common::common::ModuleKind::CommonJS,
        ..Default::default()
    }
}

fn codes(diags: &[tsz_checker::diagnostics::Diagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

fn assert_no_2322(diags: &[tsz_checker::diagnostics::Diagnostic], ctx: &str) {
    assert!(
        !codes(diags).contains(&2322),
        "{ctx}: expected no TS2322 (type argument must be substituted into the \
         re-exported generic body); got {diags:#?}"
    );
}

/// Core witness: `const x: Base<number>` where the generic interface `Base<T>`
/// is re-exported through a type-only barrel and a matching object literal is
/// assigned. Must be clean (was a false TS2322).
#[test]
fn type_only_reexported_generic_interface_annotation_is_clean() {
    let diags = check_multi_file(
        &[
            ("base.ts", "export interface Base<T> { value: T }\n"),
            ("barrel.ts", "export type { Base } from './base';\n"),
            (
                "use.ts",
                r#"
import { Base } from './barrel';
export const x: Base<number> = { value: 1 };
"#,
            ),
        ],
        "use.ts",
        strict_opts(),
    );
    assert_no_2322(&diags, "type-only barrel re-export, annotation position");
}

/// `export { X } from` (value re-export) must behave identically — the trigger
/// is the barrel hop, not the type-only modifier. Renamed binders.
#[test]
fn value_reexported_generic_interface_annotation_is_clean() {
    let diags = check_multi_file(
        &[
            ("shapes.ts", "export interface Shape<E> { item: E }\n"),
            ("hub.ts", "export { Shape } from './shapes';\n"),
            (
                "consumer.ts",
                r#"
import { Shape } from './hub';
export const s: Shape<string> = { item: "ok" };
"#,
            ),
        ],
        "consumer.ts",
        strict_opts(),
    );
    assert_no_2322(&diags, "value barrel re-export, annotation position");
}

/// A re-exported generic *class* used in annotation position.
#[test]
fn reexported_generic_class_annotation_is_clean() {
    let diags = check_multi_file(
        &[
            ("core.ts", "export class Container<T> { contents!: T }\n"),
            ("pkg.ts", "export { Container } from './core';\n"),
            (
                "app.ts",
                r#"
import { Container } from './pkg';
export function take(c: Container<number>): number { return c.contents }
"#,
            ),
        ],
        "app.ts",
        strict_opts(),
    );
    assert_no_2322(&diags, "re-exported generic class, annotation position");
}

/// Multi-hop barrel chain `origin -> mid -> top`, used in annotation position.
#[test]
fn multi_hop_reexported_generic_annotation_is_clean() {
    let diags = check_multi_file(
        &[
            ("origin.ts", "export interface Node2<T> { data: T }\n"),
            ("mid.ts", "export type { Node2 } from './origin';\n"),
            ("top.ts", "export type { Node2 } from './mid';\n"),
            (
                "use.ts",
                r#"
import { Node2 } from './top';
export const n: Node2<number> = { data: 5 };
"#,
            ),
        ],
        "use.ts",
        strict_opts(),
    );
    assert_no_2322(&diags, "multi-hop barrel re-export, annotation position");
}

/// Re-exported generic in a property-type position inside another object type.
#[test]
fn reexported_generic_in_property_position_is_clean() {
    let diags = check_multi_file(
        &[
            ("model.ts", "export interface Cell<T> { content: T }\n"),
            ("barrel.ts", "export { Cell } from './model';\n"),
            (
                "use.ts",
                r#"
import { Cell } from './barrel';
export interface Row { first: Cell<number> }
export const r: Row = { first: { content: 1 } };
"#,
            ),
        ],
        "use.ts",
        strict_opts(),
    );
    assert_no_2322(&diags, "re-exported generic in property position");
}

/// Negative control: the body really is instantiated, so an argument that
/// violates the substituted member type still reports a precise TS2322 (the fix
/// must not paper over real mismatches by widening to `any`).
#[test]
fn reexported_generic_mismatched_member_still_errors() {
    let diags = check_multi_file(
        &[
            ("base.ts", "export interface Base<T> { value: T }\n"),
            ("barrel.ts", "export type { Base } from './base';\n"),
            (
                "use.ts",
                r#"
import { Base } from './barrel';
export const bad: Base<number> = { value: "not a number" };
"#,
            ),
        ],
        "use.ts",
        strict_opts(),
    );
    assert!(
        codes(&diags).contains(&2322),
        "a string assigned to the substituted `value: number` member must still \
         report TS2322; got {diags:#?}"
    );
}

/// Direct single-hop import (control): already correct, must stay clean.
#[test]
fn direct_import_generic_annotation_is_clean() {
    let diags = check_multi_file(
        &[
            ("base.ts", "export interface Base<T> { value: T }\n"),
            (
                "use.ts",
                r#"
import { Base } from './base';
export const x: Base<number> = { value: 1 };
"#,
            ),
        ],
        "use.ts",
        strict_opts(),
    );
    assert_no_2322(&diags, "direct import control");
}
