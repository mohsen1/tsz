//! An *empty* named import/export clause in a CommonJS file is exempt from the
//! "ESM import/export syntax in a CommonJS file" diagnostic family
//! (TS1286/TS1295 under `verbatimModuleSyntax`, TS1293 under
//! `module: "preserve"` + `isolatedModules`).
//!
//! Structural rule: the diagnostic fires only when the clause carries a
//! runtime binding. `import {} from "./m"` and `export {}` (with or without
//! `from`) carry none — `export {}` is the conventional "make this file a
//! module" marker, and an empty named clause is fully erased in emit — so tsc
//! reports nothing, exactly as it exempts a bare side-effect `import "./m"`. A
//! default (`import x, {} from`) or namespace (`import * as ns from`) binding,
//! or any clause with at least one specifier (`import { x }`, `export { x }`),
//! still counts as ESM syntax and reports.
//!
//! Regression guard for #16845: #16841 added the CJS-forbidden check to the
//! named-export path and it began firing on a bare `export {};`.
//!
//! Oracle-verified against `typescript@7.0.2`:
//! - `export {};`, `export {} from "./m";`, `import {} from "./m";` — clean
//!   in both a `.ts` (module=commonjs, adjustable → TS1295) and a `.cts`
//!   (extension-locked → TS1286) file.
//! - `export { x };`, `export { x } from "./m";`, `import { x } from "./m";`,
//!   `import def, {} from "./m";`, `import * as ns from "./m";` — still fire.
//!
//! Owner: the checker's verbatim import/export CJS gates in
//! `crates/tsz-checker/src/declarations/import/verbatim.rs` and
//! `crates/tsz-checker/src/declarations/module_checker/verbatim_module_syntax.rs`.

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

const TS1286: u32 = 1286; // extension-locked ESM-in-CJS
const TS1293: u32 = 1293; // preserve + isolatedModules ESM-in-CJS
const TS1295: u32 = 1295; // adjustable ESM-in-CJS

/// `verbatimModuleSyntax` with `module: commonjs` — a `.ts` file's CJS-ness is
/// adjustable, so the ESM-in-CJS defect surfaces as TS1295.
fn check_vms_commonjs(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
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

/// `verbatimModuleSyntax` with a `.cts` entry — extension-locked CJS, so the
/// ESM-in-CJS defect surfaces as TS1286.
fn check_vms_cts(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
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

/// `module: "preserve"` + `isolatedModules` (no VMS) — `.cts` entry reports
/// TS1293 for a real ESM clause.
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

// =========================================================================
// Exempt: empty clause carries no runtime binding
// =========================================================================

/// The reported bug: a bare `export {};` module marker under VMS in a CJS file.
#[test]
fn bare_empty_export_in_commonjs_ts_is_clean_vms() {
    let diagnostics = check_vms_commonjs(&[("/main.ts", "export {};\n")], "/main.ts");
    assert_eq!(codes(&diagnostics), Vec::<u32>::new(), "{diagnostics:?}");
}

/// Same shape in an extension-locked `.cts` file — would have been TS1286.
#[test]
fn bare_empty_export_in_cts_is_clean_vms() {
    let diagnostics = check_vms_cts(&[("/main.cts", "export {};\n")], "/main.cts");
    assert_eq!(codes(&diagnostics), Vec::<u32>::new(), "{diagnostics:?}");
}

/// An empty re-export `export {} from "./m"` also carries no binding — clean.
#[test]
fn empty_reexport_with_from_in_commonjs_ts_is_clean_vms() {
    let diagnostics = check_vms_commonjs(
        &[
            ("/mod.ts", "export interface Y {}\n"),
            ("/main.ts", "export {} from \"./mod\";\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diagnostics), Vec::<u32>::new(), "{diagnostics:?}");
}

/// An empty named-imports clause `import {} from "./m"` carries no binding.
#[test]
fn empty_named_import_in_commonjs_ts_is_clean_vms() {
    let diagnostics = check_vms_commonjs(
        &[
            ("/mod.ts", "export interface Y {}\n"),
            ("/main.ts", "import {} from \"./mod\";\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diagnostics), Vec::<u32>::new(), "{diagnostics:?}");
}

/// Same empty import in `.cts` — would have been TS1286.
#[test]
fn empty_named_import_in_cts_is_clean_vms() {
    let diagnostics = check_vms_cts(
        &[
            ("/mod.ts", "export interface Y {}\n"),
            ("/main.cts", "import {} from \"./mod\";\n"),
        ],
        "/main.cts",
    );
    assert_eq!(codes(&diagnostics), Vec::<u32>::new(), "{diagnostics:?}");
}

/// Empty clauses are also exempt under `preserve` + `isolatedModules`.
#[test]
fn bare_empty_export_in_cts_is_clean_preserve_isolated() {
    let diagnostics = check_preserve_isolated(&[("/main.cts", "export {};\n")], "/main.cts");
    assert_eq!(codes(&diagnostics), Vec::<u32>::new(), "{diagnostics:?}");
}

#[test]
fn empty_named_import_in_cts_is_clean_preserve_isolated() {
    let diagnostics = check_preserve_isolated(
        &[
            ("/mod.ts", "export const y = 2;\n"),
            ("/main.cts", "import {} from \"./mod\";\n"),
        ],
        "/main.cts",
    );
    assert_eq!(codes(&diagnostics), Vec::<u32>::new(), "{diagnostics:?}");
}

// =========================================================================
// Controls: a clause that carries a binding still reports
// =========================================================================

/// A local named export with one specifier still reports TS1295.
#[test]
fn local_named_export_in_commonjs_ts_reports_ts1295() {
    let diagnostics = check_vms_commonjs(
        &[("/main.ts", "const z = 1;\nexport { z as w };\n")],
        "/main.ts",
    );
    assert_eq!(codes(&diagnostics), vec![TS1295], "{diagnostics:?}");
}

/// A non-empty re-export still reports TS1295 (renamed binder — the guard must
/// not key off a specific identifier).
#[test]
fn renamed_named_reexport_in_commonjs_ts_reports_ts1295() {
    let diagnostics = check_vms_commonjs(
        &[
            ("/mod.ts", "export const somethingElse = 2;\n"),
            (
                "/main.ts",
                "export { somethingElse as renamedOut } from \"./mod\";\n",
            ),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diagnostics), vec![TS1295], "{diagnostics:?}");
}

/// A non-empty named import still reports TS1295.
#[test]
fn named_import_in_commonjs_ts_reports_ts1295() {
    let diagnostics = check_vms_commonjs(
        &[
            ("/mod.ts", "export const y = 2;\n"),
            ("/main.ts", "import { y } from \"./mod\";\ny;\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diagnostics), vec![TS1295], "{diagnostics:?}");
}

/// A default binding alongside an *empty* named-imports list still reports —
/// the default is a runtime binding.
#[test]
fn default_plus_empty_named_import_in_commonjs_ts_reports_ts1295() {
    let diagnostics = check_vms_commonjs(
        &[
            ("/mod.ts", "export default 3;\n"),
            ("/main.ts", "import def, {} from \"./mod\";\ndef;\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diagnostics), vec![TS1295], "{diagnostics:?}");
}

/// A namespace binding carries content and still reports.
#[test]
fn namespace_import_in_commonjs_ts_reports_ts1295() {
    let diagnostics = check_vms_commonjs(
        &[
            ("/mod.ts", "export const y = 2;\n"),
            ("/main.ts", "import * as ns from \"./mod\";\nns;\n"),
        ],
        "/main.ts",
    );
    assert_eq!(codes(&diagnostics), vec![TS1295], "{diagnostics:?}");
}

/// Extension-locked control: a non-empty clause in `.cts` still reports TS1286.
#[test]
fn named_import_in_cts_reports_ts1286() {
    let diagnostics = check_vms_cts(
        &[
            ("/mod.ts", "export const y = 2;\n"),
            ("/main.cts", "import { y } from \"./mod\";\ny;\n"),
        ],
        "/main.cts",
    );
    assert_eq!(codes(&diagnostics), vec![TS1286], "{diagnostics:?}");
}

/// preserve + isolatedModules control: a non-empty clause still reports TS1293.
#[test]
fn named_import_in_cts_reports_ts1293_preserve_isolated() {
    let diagnostics = check_preserve_isolated(
        &[
            ("/mod.ts", "export const y = 2;\n"),
            ("/main.cts", "import { y } from \"./mod\";\ny;\n"),
        ],
        "/main.cts",
    );
    assert_eq!(codes(&diagnostics), vec![TS1293], "{diagnostics:?}");
}
