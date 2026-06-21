//! Coverage for #14228: a named import bound to an `export * as NS` re-export
//! must be recognized as a type-position namespace anchor.
//!
//! Structural rule: when a named import binds a namespace produced by
//! `export * as NS from '...'` and `NS` is used as a qualified type
//! (`NS.Type`), `tsc` treats `NS` as a type-position namespace anchor whose
//! members are the exports of the re-exported module. tsz used to only accept
//! whole-namespace imports (`import * as NS`), so the named-import indirection
//! through `export * as` surfaced a false TS2503 ("Cannot find namespace").
//!
//! The members are resolved by following the named export to the `export * as`
//! re-export and resolving through its backing module — keyed by file index +
//! module specifier so cross-binder raw `SymbolId` collisions cannot interfere.

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

/// Shared scaffold: `globals` declares the type/interface, `index` re-exports
/// it as the `NS` namespace, and `consumer` is the file under test.
fn check_consumer(consumer: &str) -> Vec<(u32, String)> {
    diagnostics(
        &[
            (
                "g/globals.ts",
                "export interface Foo { a: number }\nexport type Bar = { b: string };\n",
            ),
            ("g/index.ts", "export * as NS from './globals';\n"),
            ("consumer.ts", consumer),
        ],
        "consumer.ts",
    )
}

/// The repro from #14228: `export * as NS` consumed via a named import, used as
/// a qualified type in parameter and return position. No TS2503.
#[test]
fn named_import_of_export_star_as_namespace_is_type_anchor() {
    let diags = check_consumer("import { NS } from './g/index';\nlet v: NS.Foo = { a: 1 };\nv;\n");
    assert_eq!(
        count_code(&diags, 2503),
        0,
        "unexpected TS2503; got {diags:#?}"
    );
}

/// The qualified type must resolve to the actual member type, not silently to
/// `any`/error: assigning the wrong shape still reports TS2322. This mirrors the
/// issue's reported shape (`export type TTypeArray = ...`), a type-alias member.
#[test]
fn named_import_of_export_star_as_namespace_resolves_member_type() {
    let diags =
        check_consumer("import { NS } from './g/index';\nlet v: NS.Bar = { b: 123 };\nv;\n");
    assert_eq!(
        count_code(&diags, 2503),
        0,
        "unexpected TS2503; got {diags:#?}"
    );
    assert_eq!(
        count_code(&diags, 2322),
        1,
        "expected TS2322 proving NS.Bar resolved to {{ b: string }}; got {diags:#?}"
    );
}

/// Renamed binder: the named import is aliased to a different local name. The
/// fix keys on the namespace-only resolution of the chain, not the textual
/// identifier.
#[test]
fn renamed_named_import_of_export_star_as_namespace_is_type_anchor() {
    let diags = check_consumer(
        "import { NS as Guard } from './g/index';\nlet v: Guard.Foo = { a: 1 };\nv;\n",
    );
    assert_eq!(
        count_code(&diags, 2503),
        0,
        "unexpected TS2503 on renamed binder; got {diags:#?}"
    );
}

/// The namespace anchor consumed as a type-alias body.
#[test]
fn named_import_of_export_star_as_namespace_in_type_alias_body() {
    let diags = check_consumer(
        "import { NS } from './g/index';\ntype T = NS.Bar;\nlet v: T = { b: 'ok' };\nv;\n",
    );
    assert_eq!(
        count_code(&diags, 2503),
        0,
        "unexpected TS2503 in type-alias body; got {diags:#?}"
    );
}

/// A missing member through the anchor is TS2694 ("no exported member"), not
/// TS2503 — proving the anchor is recognized as a namespace.
#[test]
fn missing_member_through_export_star_as_namespace_anchor_emits_2694() {
    let diags = check_consumer("import { NS } from './g/index';\ntype T = NS.Nope;\n");
    assert_eq!(
        count_code(&diags, 2503),
        0,
        "should not emit TS2503 when anchor resolves; got {diags:#?}"
    );
    assert_eq!(
        count_code(&diags, 2694),
        1,
        "expected TS2694 for missing member; got {diags:#?}"
    );
}

/// Baseline: a direct `import * as NS` of the same module already works and
/// must keep working (no regression on the whole-namespace import path).
#[test]
fn direct_namespace_import_baseline_still_resolves() {
    let diags =
        check_consumer("import * as NS from './g/globals';\nlet v: NS.Foo = { a: 1 };\nv;\n");
    assert_eq!(
        count_code(&diags, 2503),
        0,
        "unexpected TS2503; got {diags:#?}"
    );
    let diags_missing = check_consumer("import * as NS from './g/globals';\ntype T = NS.Nope;\n");
    assert_eq!(
        count_code(&diags_missing, 2694),
        1,
        "expected TS2694 for missing member on direct import; got {diags_missing:#?}"
    );
}
