//! Empty named-import / named-export clauses are exempt from the
//! ESM-syntax-in-a-CommonJS-file diagnostic (TS1295 / TS1286 under
//! `verbatimModuleSyntax`, TS1293 under `module: "preserve"` +
//! `isolatedModules`).
//!
//! Structural rule: tsc raises this diagnostic *per binding*. On the import
//! side `checkImportBinding` runs for a default or `* as ns` binding and
//! `forEach(namedBindings.elements, checkImportBinding)` runs per named
//! specifier; on the export side `checkAliasSymbol` is reached only from
//! `forEach(exportClause.elements, checkExportSpecifier)`. A bare
//! `export {};` — the conventional "make this file a module" marker — and a
//! specifier-less `import {} from "./m";` / `export {} from "./m";` carry no
//! binding, so the walk never reaches the check and tsc stays silent. tsz was
//! firing at the clause level regardless of specifier count, over-reporting
//! one extra diagnostic on the empty clause (issue #16845). The fix requires
//! at least one specifier before emitting, without exempting
//! `export { something }` / `import { something }`.
//!
//! Owner: `crates/tsz-checker/src/declarations/import/verbatim.rs`
//! (`import_clause_binds_runtime_name`) and
//! `crates/tsz-checker/src/declarations/module_checker/verbatim_module_syntax.rs`
//! (the `!named_exports.elements.nodes.is_empty()` gate).
//!
//! Oracle: `typescript@7.0.2`, matching the
//! `externalModules/verbatimModuleSyntaxRestrictionsCJS` conformance row,
//! whose `export {};` on `main.ts` reports nothing.

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

const TS1293: u32 = 1293;
const TS1295: u32 = 1295;
const TS1286: u32 = 1286;

/// `verbatimModuleSyntax` + `module: commonjs`. A plain `.ts` file is CJS by
/// config here (not extension-locked), so the adjustable TS1295 variant is
/// the one that would fire.
fn check_vms_cjs(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            verbatim_module_syntax: true,
            ..CheckerOptions::default()
        },
    )
}

/// `module: "preserve"` + `isolatedModules` (no VMS): the TS1293 variant.
fn check_preserve_isolated(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::Preserve,
            isolated_modules: true,
            ..CheckerOptions::default()
        },
    )
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    let mut codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

// ---------------------------------------------------------------------
// The reported bug: bare `export {};` under VMS + CJS is clean.
// ---------------------------------------------------------------------

#[test]
fn empty_export_clause_in_cjs_ts_is_clean_under_vms() {
    let diagnostics = check_vms_cjs(&[("/main.ts", "export {};\n")], "/main.ts");
    assert_eq!(codes(&diagnostics), Vec::<u32>::new(), "{diagnostics:?}");
}

/// `.cts` is extension-locked (TS1286 would be the variant), still clean.
#[test]
fn empty_export_clause_in_cts_is_clean_under_vms() {
    let diagnostics = check_vms_cjs(&[("/main.cts", "export {};\n")], "/main.cts");
    assert_eq!(codes(&diagnostics), Vec::<u32>::new(), "{diagnostics:?}");
}

/// A specifier-less re-export `export {} from "./b"` is also empty — no
/// specifier reaches `checkAliasSymbol`, so tsc stays silent.
#[test]
fn empty_reexport_clause_in_cjs_ts_is_clean_under_vms() {
    let diagnostics = check_vms_cjs(
        &[
            ("/b.ts", "export const y = 2;\n"),
            ("/main.ts", "export {} from \"./b\";\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diagnostics), Vec::<u32>::new(), "{diagnostics:?}");
}

/// Control: a non-empty `export { x }` still reports — the empty-clause gate
/// must not silence a real specifier.
#[test]
fn nonempty_local_export_in_cjs_ts_still_reports_ts1295() {
    let diagnostics = check_vms_cjs(&[("/main.ts", "const x = 1;\nexport { x };\n")], "/main.ts");
    assert_eq!(codes(&diagnostics), vec![TS1295], "{diagnostics:?}");
}

/// Control: `.cts` picks the extension-locked TS1286 variant for a real
/// specifier.
#[test]
fn nonempty_local_export_in_cts_still_reports_ts1286() {
    let diagnostics = check_vms_cjs(
        &[("/main.cts", "const x = 1;\nexport { x };\n")],
        "/main.cts",
    );
    assert_eq!(codes(&diagnostics), vec![TS1286], "{diagnostics:?}");
}

/// Control, renamed binder: the gate keys on specifier presence, never on a
/// fixed identifier name.
#[test]
fn nonempty_renamed_export_in_cjs_ts_still_reports_ts1295() {
    let diagnostics = check_vms_cjs(
        &[(
            "/main.ts",
            "const somethingElse = 1;\nexport { somethingElse as renamed };\n",
        )],
        "/main.ts",
    );
    assert_eq!(codes(&diagnostics), vec![TS1295], "{diagnostics:?}");
}

// ---------------------------------------------------------------------
// Import side: empty `import {} from "./b"` is the mirror case.
// ---------------------------------------------------------------------

#[test]
fn empty_named_import_in_cjs_ts_is_clean_under_vms() {
    let diagnostics = check_vms_cjs(
        &[
            ("/b.ts", "export const y = 2;\n"),
            ("/main.ts", "import {} from \"./b\";\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diagnostics), Vec::<u32>::new(), "{diagnostics:?}");
}

#[test]
fn empty_named_import_in_cts_is_clean_under_vms() {
    let diagnostics = check_vms_cjs(
        &[
            ("/b.ts", "export const y = 2;\n"),
            ("/main.cts", "import {} from \"./b\";\n"),
        ],
        "/main.cts",
    );
    assert_eq!(codes(&diagnostics), Vec::<u32>::new(), "{diagnostics:?}");
}

/// Control: a real named import still reports.
#[test]
fn nonempty_named_import_in_cjs_ts_still_reports_ts1295() {
    let diagnostics = check_vms_cjs(
        &[
            ("/b.ts", "export const y = 2;\n"),
            ("/main.ts", "import { y } from \"./b\";\ny;\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diagnostics), vec![TS1295], "{diagnostics:?}");
}

/// Control: a default import binds a name, so it still reports.
#[test]
fn default_import_in_cjs_ts_still_reports_ts1295() {
    let diagnostics = check_vms_cjs(
        &[
            ("/b.ts", "export default 3;\n"),
            ("/main.ts", "import def from \"./b\";\ndef;\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diagnostics), vec![TS1295], "{diagnostics:?}");
}

/// Control: a namespace import binds `ns`, so it still reports.
#[test]
fn namespace_import_in_cjs_ts_still_reports_ts1295() {
    let diagnostics = check_vms_cjs(
        &[
            ("/b.ts", "export const y = 2;\n"),
            ("/main.ts", "import * as ns from \"./b\";\nns;\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diagnostics), vec![TS1295], "{diagnostics:?}");
}

// ---------------------------------------------------------------------
// preserve + isolatedModules (TS1293) path: same empty-clause exemption.
// ---------------------------------------------------------------------

#[test]
fn empty_export_clause_in_cts_is_clean_under_preserve_isolated() {
    let diagnostics = check_preserve_isolated(&[("/main.cts", "export {};\n")], "/main.cts");
    assert_eq!(codes(&diagnostics), Vec::<u32>::new(), "{diagnostics:?}");
}

#[test]
fn empty_named_import_in_cts_is_clean_under_preserve_isolated() {
    let diagnostics = check_preserve_isolated(
        &[
            ("/b.ts", "export const y = 2;\n"),
            ("/main.cts", "import {} from \"./b\";\n"),
        ],
        "/main.cts",
    );
    assert_eq!(codes(&diagnostics), Vec::<u32>::new(), "{diagnostics:?}");
}

/// Control on the preserve path: a real specifier still reports TS1293.
#[test]
fn nonempty_export_in_cts_still_reports_ts1293_under_preserve_isolated() {
    let diagnostics = check_preserve_isolated(
        &[("/main.cts", "const z = 1;\nexport { z as w };\n")],
        "/main.cts",
    );
    assert_eq!(codes(&diagnostics), vec![TS1293], "{diagnostics:?}");
}
