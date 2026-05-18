//! TS2527 / TS4023 false-positive guard for `unique symbol` references
//! reached through re-exported packages.
//!
//! When the inferred type of an exported value references a `unique symbol`
//! declared in a sibling file of a package that the current file already
//! imports something from, tsc treats the symbol as accessible because dts
//! emit can synthesize a `typeof import("<package>").<name>` reference (or
//! qualify through the existing alias).
//!
//! Before this fix, tsz's accessibility check only accepted a symbol when a
//! direct local alias resolved to it. Symbols reached via re-export chains
//! through an imported module triggered a spurious TS2527 ("inferred type
//! references an inaccessible 'unique symbol' type") or TS4023 ("has or is
//! using name from external module but cannot be named"). Both gates share
//! the relaxation, so every fixture asserts neither code fires.
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

/// Counts of every diagnostic this PR's accessibility relaxation gates:
/// TS2527 (`The_inferred_type_of_0_references_an_inaccessible_1_type_*`)
/// and TS4023 (`Exported_variable_0_has_or_is_using_name_1_from_external_*`).
/// Asserting both per fixture prevents a future regression that re-tightens
/// just one path from slipping past these tests.
fn count_accessibility_diagnostics(diags: &[Diagnostic]) -> (usize, usize) {
    let ts2527 = diags.iter().filter(|d| d.code == 2527).count();
    let ts4023 = diags.iter().filter(|d| d.code == 4023).count();
    (ts2527, ts4023)
}

fn assert_no_accessibility_diagnostics(diags: &[Diagnostic], context: &str) {
    let (ts2527, ts4023) = count_accessibility_diagnostics(diags);
    assert_eq!(
        (ts2527, ts4023),
        (0, 0),
        "{context}: expected no TS2527 (inaccessible unique symbol) and no TS4023 (unnameable external module name); \
         got TS2527={ts2527} TS4023={ts4023}. \
         Diagnostics: {diags:#?}",
    );
}

#[test]
fn unique_symbol_reachable_through_named_reexport_does_not_emit_ts2527_or_ts4023() {
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
    assert_no_accessibility_diagnostics(&diags, "unique symbol re-exported from imported package");
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
    assert_no_accessibility_diagnostics(
        &diags,
        "renamed local import alias resolving through package re-export",
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
    assert_no_accessibility_diagnostics(
        &diags,
        "export-side rename `export { internal as external }`",
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
    assert_no_accessibility_diagnostics(&diags, "wildcard re-export `export * from \"./inner\"`");
}

// Negative-case coverage (symbol not re-exported from any locally imported
// module) is exercised by the existing single-file unique-symbol nameability
// tests and by the conformance suite. Adding it here would require the
// simplified harness to materialise cross-file unique-symbol references in
// inferred types — a known limitation documented at
// `crates/tsz-checker/tests/conformance_issues/types/enum.rs:290` — so the
// negative assertion would flake on harness behaviour rather than this fix.
