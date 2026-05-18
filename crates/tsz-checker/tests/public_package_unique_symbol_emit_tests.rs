//! Declaration-emit accessibility for `unique symbol` types reachable through a
//! top-level `node_modules/<pkg>` package.
//!
//! When an exported variable's inferred type references a `unique symbol`
//! value, `tsc` only emits TS2527/TS4023 if the value cannot be named at all
//! in declaration emit. A symbol exported from a public package
//! (`node_modules/<pkg>`) is always nameable via `typeof import("<pkg>").<id>`,
//! so the diagnostic must not fire — even if the current file does not import
//! the symbol locally.
//!
//! The negative side of the rule (nested `node_modules/<outer>/node_modules/
//! <inner>` packages remain unreachable) is enforced structurally by the
//! `node_modules_segment_count(...) >= 2` early-return inside
//! `symbol_is_public_package_export`. Unit-testing that branch in isolation
//! would require lib setup; the reported conformance corpus covers it.

use tsz_checker::context::CheckerOptions;
use tsz_common::common::{ModuleKind, ScriptTarget};

fn diagnostic_codes(files: &[(&str, &str)], entry_file: &str) -> Vec<u32> {
    tsz_checker::test_utils::check_multi_file(
        files,
        entry_file,
        CheckerOptions {
            target: ScriptTarget::ES2015,
            module: ModuleKind::NodeNext,
            strict: true,
            emit_declarations: true,
            no_lib: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

/// Reported repro: `declarationEmitUsingTypeAlias2`. The inferred type of
/// `bar` references the public package's `unique symbol` values via
/// `typeof reuseDepName` / `typeof import('./other').shouldLookupName`. `tsc`
/// emits no error because the printer can name each symbol via
/// `typeof import("some-dep").<id>`.
#[test]
fn unique_symbol_from_public_package_does_not_emit_ts2527() {
    let inner = r#"
export declare type Other = { other: string };
"#;
    let other = r#"
export declare const shouldLookupName: unique symbol;
export declare const shouldReuseLocalName: unique symbol;
export declare const reuseDepName: unique symbol;
export declare const shouldBeElided: unique symbol;
"#;
    let index = r#"
import { Other } from './inner';
import { shouldLookupName, reuseDepName, shouldReuseLocalName, shouldBeElided } from './other';
export declare const goodDeclaration: <T>() => () => {
  shouldPrintResult: T extends Other ? "O" : "N",
  shouldPrintResult2: T extends typeof shouldBeElided ? Other : "N",
  shouldLookupName: typeof import('./other').shouldLookupName,
  shouldReuseLocalName: typeof shouldReuseLocalName,
  reuseDepName: typeof reuseDepName,
};
export { shouldLookupName, shouldReuseLocalName, reuseDepName, shouldBeElided };
"#;
    let entry = r#"
import { goodDeclaration, shouldReuseLocalName, shouldBeElided } from "some-dep";
export const bar = goodDeclaration<{}>;
"#;

    let codes = diagnostic_codes(
        &[
            ("/node_modules/some-dep/dist/inner.d.ts", inner),
            ("/node_modules/some-dep/dist/other.d.ts", other),
            ("/node_modules/some-dep/dist/index.d.ts", index),
            ("/src/index.ts", entry),
        ],
        "/src/index.ts",
    );

    assert!(
        !codes.contains(&2527),
        "TS2527 must not fire for public-package unique symbols: {codes:?}"
    );
    assert!(
        !codes.contains(&4023),
        "TS4023 must not fire for public-package unique symbols: {codes:?}"
    );
}

/// Adjacent case: rename every user-chosen identifier. The structural rule —
/// "public-package exports remain nameable" — must not depend on the names
/// `some-dep`, `goodDeclaration`, `shouldLookupName`, etc.
#[test]
fn unique_symbol_from_public_package_does_not_emit_ts2527_with_renamed_identifiers() {
    let inner = r#"
export declare type Base = { kind: string };
"#;
    let other = r#"
export declare const FIRST_KEY: unique symbol;
export declare const SECOND_KEY: unique symbol;
export declare const THIRD_KEY: unique symbol;
export declare const HIDDEN_KEY: unique symbol;
"#;
    let index = r#"
import { Base } from './inner';
import { FIRST_KEY, SECOND_KEY, THIRD_KEY, HIDDEN_KEY } from './other';
export declare const factory: <T>() => () => {
  branch: T extends Base ? "yes" : "no",
  branch2: T extends typeof HIDDEN_KEY ? Base : "no",
  first: typeof import('./other').FIRST_KEY,
  second: typeof SECOND_KEY,
  third: typeof THIRD_KEY,
};
export { FIRST_KEY, SECOND_KEY, THIRD_KEY, HIDDEN_KEY };
"#;
    let entry = r#"
import { factory, SECOND_KEY, HIDDEN_KEY } from "another-pkg";
export const baz = factory<{}>;
"#;

    let codes = diagnostic_codes(
        &[
            ("/node_modules/another-pkg/dist/inner.d.ts", inner),
            ("/node_modules/another-pkg/dist/other.d.ts", other),
            ("/node_modules/another-pkg/dist/index.d.ts", index),
            ("/src/index.ts", entry),
        ],
        "/src/index.ts",
    );

    assert!(
        !codes.contains(&2527),
        "rename of identifiers must not bring TS2527 back: {codes:?}"
    );
    assert!(
        !codes.contains(&4023),
        "rename of identifiers must not bring TS4023 back: {codes:?}"
    );
}

/// Adjacent case: the same shape but with a direct (not instantiation)
/// expression — `export const x = pkgFn()` whose return type references a
/// public-package unique symbol. Same rule applies.
#[test]
fn unique_symbol_from_public_package_in_direct_call_return_does_not_emit_ts2527() {
    let pkg = r#"
export declare const K: unique symbol;
export declare function build(): { key: typeof K };
"#;
    let entry = r#"
import { build } from "pkg-call";
export const out = build();
"#;
    let codes = diagnostic_codes(
        &[
            ("/node_modules/pkg-call/index.d.ts", pkg),
            ("/src/index.ts", entry),
        ],
        "/src/index.ts",
    );
    assert!(
        !codes.contains(&2527),
        "TS2527 must not fire for public-package unique symbol via direct call: {codes:?}"
    );
}

/// Adjacent case: a package whose index re-exports a unique symbol both
/// under its original name and under a renamed alias. The consumer never
/// imports either, but the inferred type of an exported `make<{}>` still
/// references the original symbol via `typeof K`. The public-package
/// suppression must apply regardless of the rename, because the symbol's
/// declaring file (`other.d.ts`) sits inside the same single-`node_modules`
/// segment package and exposes it in its own module-exports table.
#[test]
fn unique_symbol_in_public_package_with_renamed_re_export_does_not_emit_ts2527() {
    let other = r#"
export declare const K: unique symbol;
"#;
    let index = r#"
import { K } from './other';
export declare const make: <T>() => () => { value: typeof K };
export { K, K as Renamed };
"#;
    let entry = r#"
import { make } from "renamer-pkg";
export const result = make<{}>;
"#;
    let codes = diagnostic_codes(
        &[
            ("/node_modules/renamer-pkg/dist/other.d.ts", other),
            ("/node_modules/renamer-pkg/dist/index.d.ts", index),
            ("/src/index.ts", entry),
        ],
        "/src/index.ts",
    );
    assert!(
        !codes.contains(&2527),
        "renamed re-export must still keep public-package symbols nameable: {codes:?}"
    );
}

/// Adjacent positive case: the unique symbol value is imported in the
/// current file but its *type* surface still flows through the inferred
/// type. `local_value_name_resolves_to` already covers the value reference;
/// the `symbol_is_public_package_export` path picks up the remaining cross-
/// module type-query references the previous check could miss.
#[test]
fn locally_imported_public_package_unique_symbol_does_not_emit_ts2527() {
    let pkg = r#"
export declare const K: unique symbol;
export declare const make: <T>() => () => { k: typeof K };
"#;
    let entry = r#"
import { make, K } from "local-pkg";
export const out = make<{}>;
"#;
    let codes = diagnostic_codes(
        &[
            ("/node_modules/local-pkg/index.d.ts", pkg),
            ("/src/index.ts", entry),
        ],
        "/src/index.ts",
    );
    assert!(
        !codes.contains(&2527),
        "locally-imported public-package symbols must not emit TS2527: {codes:?}"
    );
}
