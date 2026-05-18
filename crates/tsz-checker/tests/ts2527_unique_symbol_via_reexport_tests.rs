//! TS2527 false-positive guard for `unique symbol` references reached through
//! re-exported packages.
//!
//! When the inferred type of an exported value references a `unique symbol`
//! declared in a sibling file of a package that the current file already
//! imports something from, tsc treats the symbol as accessible because dts
//! emit can synthesize a `typeof import("<package>").<name>` reference (or
//! qualify through the existing alias).
//!
//! Before this fix, tsz's accessibility check only accepted a symbol when a
//! direct local alias resolved to it. Symbols reached via re-export chains
//! through an imported module triggered a spurious
//! `TS2527: The inferred type of '<x>' references an inaccessible 'unique
//! symbol' type.` This file pins the structural rule with adjacent-case
//! coverage so a future refactor can't reintroduce the bug.
//!
//! Tracks: <https://github.com/mohsen1/tsz/issues/7642>.

use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_multi_file;
use tsz_common::ModuleKind;

fn opts() -> CheckerOptions {
    CheckerOptions {
        emit_declarations: true,
        strict: true,
        target: ScriptTarget::ES2015,
        module: ModuleKind::CommonJS,
        no_lib: true,
        ..Default::default()
    }
}

fn count_ts2527(diags: &[Diagnostic]) -> usize {
    diags.iter().filter(|d| d.code == 2527).count()
}

#[test]
fn unique_symbol_reachable_through_named_reexport_does_not_emit_ts2527() {
    // `consumer.ts` imports only `getValue` from `./pkg`. `pkg` re-exports
    // `sym` from `./inner`, so `typeof sym` inside the inferred type of
    // `getValue<{}>` is reachable from `consumer.ts` via
    // `typeof import("./pkg").sym`. tsc accepts this. tsz must accept it too,
    // matching the rule and not the package's particular spelling.
    let files = [
        ("inner.ts", "export declare const sym: unique symbol;\n"),
        (
            "pkg.ts",
            r#"
export { sym } from "./inner";
export declare const getValue: <T>() => { tag: typeof import("./inner").sym };
"#,
        ),
        (
            "consumer.ts",
            r#"
import { getValue } from "./pkg";
export const bound = getValue<{}>;
"#,
        ),
    ];
    let diags = check_multi_file(&files, "consumer.ts", opts());
    assert_eq!(
        count_ts2527(&diags),
        0,
        "unique symbol re-exported from imported package must not trigger TS2527 (rule is structural, not name-based). \
         Diagnostics: {diags:#?}",
    );
}

#[test]
fn unique_symbol_with_renamed_consumer_alias_still_accepted() {
    // §25: the fix must not depend on the consumer's local alias spelling.
    // Renaming `getValue` to `renamed` in the import must not change the
    // outcome — accessibility is decided structurally by re-export tables.
    let files = [
        (
            "innerA.ts",
            "export declare const kSentinel: unique symbol;\n",
        ),
        (
            "pkgA.ts",
            r#"
export { kSentinel } from "./innerA";
export declare const renamed: <T>() => { mark: typeof import("./innerA").kSentinel };
"#,
        ),
        (
            "consumerA.ts",
            r#"
import { renamed as locallyRenamed } from "./pkgA";
export const target = locallyRenamed<number>;
"#,
        ),
    ];
    let diags = check_multi_file(&files, "consumerA.ts", opts());
    assert_eq!(
        count_ts2527(&diags),
        0,
        "renamed local import alias must still resolve the unique symbol through the package's re-export. \
         Diagnostics: {diags:#?}",
    );
}

#[test]
fn unique_symbol_with_renamed_package_export_still_accepted() {
    // Renaming the export side as `internal as external` — the unique symbol
    // is still reachable via the slow path (enumerate exports of the
    // imported module).
    let files = [
        (
            "innerB.ts",
            "export declare const internalSym: unique symbol;\n",
        ),
        (
            "pkgB.ts",
            r#"
export { internalSym as externalSym } from "./innerB";
export declare const make: <T>() => { id: typeof import("./innerB").internalSym };
"#,
        ),
        (
            "consumerB.ts",
            r#"
import { make } from "./pkgB";
export const v = make<{}>;
"#,
        ),
    ];
    let diags = check_multi_file(&files, "consumerB.ts", opts());
    assert_eq!(
        count_ts2527(&diags),
        0,
        "Export-side renaming must still let the consumer reach the symbol via the imported package. \
         Diagnostics: {diags:#?}",
    );
}

#[test]
fn unique_symbol_reached_through_wildcard_reexport_is_accepted() {
    // `export * from "./inner"` is the same accessibility story as a named
    // re-export — the package's index transparently re-exports the symbol.
    let files = [
        (
            "innerC.ts",
            "export declare const wildSym: unique symbol;\n",
        ),
        (
            "pkgC.ts",
            r#"
export * from "./innerC";
export declare const fromWild: <T>() => { p: typeof import("./innerC").wildSym };
"#,
        ),
        (
            "consumerC.ts",
            r#"
import { fromWild } from "./pkgC";
export const out = fromWild<string>;
"#,
        ),
    ];
    let diags = check_multi_file(&files, "consumerC.ts", opts());
    assert_eq!(
        count_ts2527(&diags),
        0,
        "Wildcard re-export must let the consumer reach the symbol. \
         Diagnostics: {diags:#?}",
    );
}

// Negative-case coverage (symbol not re-exported from any locally imported
// module) is exercised by the existing single-file unique-symbol nameability
// tests and by the conformance suite. Adding it here would require the
// simplified harness to materialise cross-file unique-symbol references in
// inferred types — a known limitation documented at
// `crates/tsz-checker/tests/conformance_issues/types/enum.rs:290` — so the
// negative assertion would flake on harness behaviour rather than this fix.
