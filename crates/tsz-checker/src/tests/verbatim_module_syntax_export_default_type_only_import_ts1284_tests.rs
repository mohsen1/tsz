//! `export default <name>` under `verbatimModuleSyntax`, where `<name>` is
//! imported with an explicit `import type` (so the local symbol itself is
//! `is_type_only`).
//!
//! `check_verbatim_module_syntax_export_default`'s `sym.is_type_only` branch
//! used to emit TS1285 ("resolves to a type-only declaration")
//! unconditionally whenever the local binding was a type-only import,
//! without checking whether the import's resolved target actually carries a
//! Value meaning anywhere. Real tsc's `checkExportAssignment` only reports
//! TS1285 when the merged symbol (`getSymbolFlags(sym) & SymbolFlags.Value`)
//! still has Value; when the target is a pure type (interface/type alias, no
//! value anywhere), tsc reports TS1284 ("only refers to a type") instead —
//! the same code the direct local-declaration branch uses.
//!
//! Oracle-confirmed against `typescript@7.0.2`.

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

const EXPORT_DEFAULT_ONLY_REFERS_TO_A_TYPE: u32 = 1284;
const EXPORT_DEFAULT_REAL_VALUE: u32 = 1285;

fn check(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            verbatim_module_syntax: true,
            ..CheckerOptions::default()
        },
    )
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    let mut codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

/// `import type { Foo }` where `Foo` resolves to a pure type (interface):
/// the resolved target never has a Value meaning, so tsc reports TS1284, not
/// TS1285.
#[test]
fn type_only_import_of_interface_export_default_reports_1284_not_1285() {
    let diagnostics = check(
        &[
            ("/types.ts", "export interface Foo { x: number }\n"),
            (
                "/main.ts",
                "import type { Foo } from \"./types\";\nexport default Foo;\n",
            ),
        ],
        "/main.ts",
    );

    assert_eq!(
        codes(&diagnostics),
        vec![EXPORT_DEFAULT_ONLY_REFERS_TO_A_TYPE],
        "expected only TS1284 for a type-only import of a pure-type target, got: {diagnostics:?}"
    );
}

/// Same shape, renamed on import — the fix must key off the resolved import
/// target, not the local binding name.
#[test]
fn type_only_import_of_interface_renamed_export_default_reports_1284_not_1285() {
    let diagnostics = check(
        &[
            ("/types.ts", "export interface Foo { x: number }\n"),
            (
                "/main.ts",
                "import type { Foo as Baz } from \"./types\";\nexport default Baz;\n",
            ),
        ],
        "/main.ts",
    );

    assert_eq!(
        codes(&diagnostics),
        vec![EXPORT_DEFAULT_ONLY_REFERS_TO_A_TYPE],
        "expected only TS1284 for a renamed type-only import of a pure-type target, got: {diagnostics:?}"
    );
}

/// Same shape via a type-only default import.
#[test]
fn type_only_default_import_of_interface_export_default_reports_1284_not_1285() {
    let diagnostics = check(
        &[
            ("/types.ts", "export default interface Foo { x: number }\n"),
            (
                "/main.ts",
                "import type Foo from \"./types\";\nexport default Foo;\n",
            ),
        ],
        "/main.ts",
    );

    assert_eq!(
        codes(&diagnostics),
        vec![EXPORT_DEFAULT_ONLY_REFERS_TO_A_TYPE],
        "expected only TS1284 for a type-only default import of a pure-type target, got: {diagnostics:?}"
    );
}

/// Positive control: `import type` of a name that resolves to a merged
/// value+type declaration (a class also declaring a same-named interface)
/// still carries Value in its full merged meaning, so tsc reports TS1285,
/// not TS1284. This is the case the original (pre-fix) code accidentally got
/// right by always assuming Value survives.
#[test]
fn type_only_import_of_merged_class_and_interface_export_default_reports_1285_not_1284() {
    let diagnostics = check(
        &[
            (
                "/types.ts",
                "export class Foo { x = 1; }\nexport interface Foo { y: number }\n",
            ),
            (
                "/main.ts",
                "import type { Foo } from \"./types\";\nexport default Foo;\n",
            ),
        ],
        "/main.ts",
    );

    assert_eq!(
        codes(&diagnostics),
        vec![EXPORT_DEFAULT_REAL_VALUE],
        "expected only TS1285 for a type-only import whose merged target still carries Value, got: {diagnostics:?}"
    );
}

/// Negative control: a plain (non-type-only) import of a pure-type target
/// does not go through the `is_type_only` branch at all — this stays on the
/// pre-existing alias-resolution TS1284-and-TS1292 path (issue #16633),
/// unaffected by this fix.
#[test]
fn non_type_only_import_of_interface_export_default_unaffected() {
    let diagnostics = check(
        &[
            ("/types.ts", "export interface Foo { x: number }\n"),
            (
                "/main.ts",
                "import { Foo } from \"./types\";\nexport default Foo;\n",
            ),
        ],
        "/main.ts",
    );

    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == EXPORT_DEFAULT_REAL_VALUE),
        "did not expect TS1285 for a non-type-only import, got: {diagnostics:?}"
    );
}
