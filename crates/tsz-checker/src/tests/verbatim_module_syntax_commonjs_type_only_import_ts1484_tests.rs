//! `check_verbatim_module_syntax_imports` fires the CJS-file ESM-syntax
//! diagnostic (TS1286/TS1295) for any binding-carrying import clause in a
//! CommonJS file under `verbatimModuleSyntax`, then used to `return`
//! immediately — unconditionally skipping the VMS-exclusive TS1484/TS1485/
//! TS2748 checks a few lines below it for the same clause's specifiers.
//!
//! tsc reports both: the CJS-file defect and the type-only-import defect are
//! independent checks that both anchor on the same specifier. Oracle-verified
//! against `typescript@7.0.2` (`--module commonjs --verbatimModuleSyntax`):
//! `main.ts(1,10): TS1295` AND `main.ts(1,10): TS1484` on the same line for
//! `import { Foo } from "./mod"; export type Bar = Foo;` where `mod.ts`
//! exports a pure interface.
//!
//! Structural rule: TS1484/TS1485/TS2748 are `verbatimModuleSyntax`-exclusive
//! checks orthogonal to which CJS-diagnostic variant (TS1286 extension-locked
//! vs TS1295 adjustable) fired for the same clause; a CJS file is not exempt
//! from them. `preserve` + `isolatedModules` (TS1293, no VMS) has no such
//! exclusive checks and must keep returning early — covered here as a
//! negative control.
//!
//! tsz-org/tsz#17098.
//!
//! Owner: `crates/tsz-checker/src/declarations/import/verbatim.rs`
//! (`check_verbatim_module_syntax_imports`).

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

const TS1286: u32 = 1286;
const TS1293: u32 = 1293;
const TS1295: u32 = 1295;
const TS1484: u32 = 1484;

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

/// The reported bug: a direct type-only named import in a `.ts` CommonJS
/// file under `verbatimModuleSyntax` must report both TS1295 (adjustable
/// CJS-ness) and TS1484 (direct type import), not just the former.
#[test]
fn direct_type_import_in_commonjs_ts_reports_ts1295_and_ts1484() {
    let diagnostics = check_vms_commonjs(
        &[
            ("/mod.ts", "export interface Foo { x: number }\n"),
            (
                "/main.ts",
                "import { Foo } from \"./mod\";\nexport type Bar = Foo;\n",
            ),
        ],
        "/main.ts",
    );
    assert_eq!(
        codes(&diagnostics),
        vec![TS1295, TS1484],
        "expected both TS1295 and TS1484 for a direct type-only import in a \
         CommonJS file under verbatimModuleSyntax, got: {diagnostics:?}"
    );
}

/// Renamed on import (`Foo as Baz`) — the type-only pick must key off the
/// resolved import target, not the local binding name.
#[test]
fn renamed_type_import_in_commonjs_ts_reports_ts1295_and_ts1484() {
    let diagnostics = check_vms_commonjs(
        &[
            ("/mod.ts", "export interface Foo { x: number }\n"),
            (
                "/main.ts",
                "import { Foo as Baz } from \"./mod\";\nexport type Bar = Baz;\n",
            ),
        ],
        "/main.ts",
    );
    assert_eq!(
        codes(&diagnostics),
        vec![TS1295, TS1484],
        "expected both TS1295 and TS1484 for a renamed type-only import in a \
         CommonJS file under verbatimModuleSyntax, got: {diagnostics:?}"
    );
}

/// Extension-locked control: the same shape in a `.cts` file reports TS1286
/// (not TS1295) alongside TS1484 — the CJS-diagnostic variant changes, the
/// fall-through to TS1484 does not.
#[test]
fn direct_type_import_in_cts_reports_ts1286_and_ts1484() {
    let diagnostics = check_vms_commonjs(
        &[
            ("/mod.ts", "export interface Foo { x: number }\n"),
            (
                "/main.cts",
                "import { Foo } from \"./mod\";\nexport type Bar = Foo;\n",
            ),
        ],
        "/main.cts",
    );
    assert_eq!(
        codes(&diagnostics),
        vec![TS1286, TS1484],
        "expected both TS1286 and TS1484 for a direct type-only import in an \
         extension-locked .cts file under verbatimModuleSyntax, got: {diagnostics:?}"
    );
}

/// Negative control: a named import of a real VALUE (not a type) in the same
/// CommonJS+VMS configuration must still report only TS1295 — the
/// type-only-import checks below the fall-through correctly find nothing.
#[test]
fn value_import_in_commonjs_ts_reports_only_ts1295() {
    let diagnostics = check_vms_commonjs(
        &[
            ("/mod.ts", "export const y = 2;\n"),
            ("/main.ts", "import { y } from \"./mod\";\ny;\n"),
        ],
        "/main.ts",
    );
    assert_eq!(
        codes(&diagnostics),
        vec![TS1295],
        "expected only TS1295 for a value import — no type-only diagnostic \
         should appear, got: {diagnostics:?}"
    );
}

/// Negative control: `module: "preserve"` + `isolatedModules` (TS1293, no
/// `verbatimModuleSyntax`) has no VMS-exclusive TS1484 check and must keep
/// returning before it — the `preserve_isolated` early return right after
/// this fall-through point is untouched by this fix.
#[test]
fn direct_type_import_in_cts_preserve_isolated_reports_only_ts1293() {
    let diagnostics = check_preserve_isolated(
        &[
            ("/mod.ts", "export interface Foo { x: number }\n"),
            (
                "/main.cts",
                "import { Foo } from \"./mod\";\nexport type Bar = Foo;\n",
            ),
        ],
        "/main.cts",
    );
    assert_eq!(
        codes(&diagnostics),
        vec![TS1293],
        "expected only TS1293 under preserve+isolatedModules (no VMS) — TS1484 \
         is VMS-exclusive, got: {diagnostics:?}"
    );
}
