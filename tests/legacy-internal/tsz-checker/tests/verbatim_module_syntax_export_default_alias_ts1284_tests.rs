//! `export default <name>` under `verbatimModuleSyntax`, where `<name>` is an
//! import alias (not a local declaration) resolving to a pure type.
//!
//! `check_verbatim_module_syntax_export_default`'s early TS1284/TS1285 branch
//! only inspects the local symbol's own `is_type_only`/`PURE_TYPE`
//! (`INTERFACE`/`TYPE_ALIAS`) flags. A plain `import { Foo } from "./m"`
//! binding never carries those flags itself — only the resolved target does,
//! which is exactly what the later TS1292 branch (`lookup_imported_target_flags`)
//! computes. So the early branch always falls through, and tsz emitted only
//! TS1292, never TS1284.
//!
//! Oracle-confirmed against `typescript@7.0.2`: for this shape tsc reports
//! **both** TS1284 and TS1292 at the same position under `verbatimModuleSyntax`.
//! `isolatedModules` alone (without `verbatimModuleSyntax`) reports only
//! TS1292 — TS1284 is verbatimModuleSyntax-only, matching the existing direct
//! (non-alias) branch's gate.

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

const EXPORT_DEFAULT_ONLY_REFERS_TO_A_TYPE: u32 = 1284;
const EXPORT_DEFAULT_REAL_VALUE: u32 = 1285;
const IMPORT_MUST_BE_TYPE_ONLY: u32 = 1484;
const RESOLVES_TO_A_TYPE: u32 = 1292;

fn check(files: &[(&str, &str)], entry: &str, verbatim_module_syntax: bool) -> Vec<Diagnostic> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::ESNext,
            strict: true,
            verbatim_module_syntax,
            isolated_modules: !verbatim_module_syntax,
            ..CheckerOptions::default()
        },
    )
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<u32> {
    let mut codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

/// A plain import alias to an interface-only module member, referenced by
/// `export default`, reports both TS1284 and TS1292 under
/// `verbatimModuleSyntax`.
#[test]
fn import_alias_to_interface_export_default_reports_both_verbatim() {
    let diagnostics = check(
        &[
            ("/types.ts", "export interface Foo { x: number }\n"),
            (
                "/main.ts",
                "import { Foo } from \"./types\";\nexport default Foo;\n",
            ),
        ],
        "/main.ts",
        true,
    );

    assert_eq!(
        codes(&diagnostics),
        vec![
            EXPORT_DEFAULT_ONLY_REFERS_TO_A_TYPE,
            RESOLVES_TO_A_TYPE,
            IMPORT_MUST_BE_TYPE_ONLY,
        ],
        "expected TS1284 and TS1292 (plus the pre-existing TS1484 on the import \
         statement itself) for an import-alias export default resolving to a pure \
         type under verbatimModuleSyntax, got: {diagnostics:?}"
    );
}

/// Same shape, renamed on import (`Foo as Baz`) — the alias-resolution path
/// must key off the resolved import target, not the local binding name.
#[test]
fn renamed_import_alias_to_interface_export_default_reports_both_verbatim() {
    let diagnostics = check(
        &[
            ("/types.ts", "export interface Foo { x: number }\n"),
            (
                "/main.ts",
                "import { Foo as Baz } from \"./types\";\nexport default Baz;\n",
            ),
        ],
        "/main.ts",
        true,
    );

    assert_eq!(
        codes(&diagnostics),
        vec![
            EXPORT_DEFAULT_ONLY_REFERS_TO_A_TYPE,
            RESOLVES_TO_A_TYPE,
            IMPORT_MUST_BE_TYPE_ONLY,
        ],
        "expected TS1284 and TS1292 (plus the pre-existing TS1484 on the import \
         statement itself) for a renamed import-alias export default resolving to \
         a pure type under verbatimModuleSyntax, got: {diagnostics:?}"
    );
}

/// Same alias shape, but only `isolatedModules` is enabled: tsc reports only
/// TS1292 (TS1284 is verbatimModuleSyntax-only). Negative control against the
/// same code path so the fix stays gated correctly.
#[test]
fn import_alias_to_interface_export_default_reports_only_1292_under_isolated_modules() {
    let diagnostics = check(
        &[
            ("/types.ts", "export interface Foo { x: number }\n"),
            (
                "/main.ts",
                "import { Foo } from \"./types\";\nexport default Foo;\n",
            ),
        ],
        "/main.ts",
        false,
    );

    assert_eq!(
        codes(&diagnostics),
        vec![RESOLVES_TO_A_TYPE],
        "expected only TS1292 under isolatedModules (no verbatimModuleSyntax), got: {diagnostics:?}"
    );
}

/// A LOCAL interface declared and exported default directly (no import alias
/// in between) must keep going through the pre-existing direct `PURE_TYPE`
/// branch — single TS1284, not the alias-resolution TS1292 path, and not a
/// double report.
#[test]
fn local_interface_export_default_reports_only_1284_verbatim() {
    let diagnostics = check(
        &[(
            "/main.ts",
            "interface Foo { x: number }\nexport default Foo;\n",
        )],
        "/main.ts",
        true,
    );

    assert_eq!(
        codes(&diagnostics),
        vec![EXPORT_DEFAULT_ONLY_REFERS_TO_A_TYPE],
        "expected only TS1284 for a directly-declared local interface export default, got: {diagnostics:?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == EXPORT_DEFAULT_REAL_VALUE),
        "did not expect TS1285 here, got: {diagnostics:?}"
    );
}

/// An import alias whose target has BOTH a type and a value meaning (a class)
/// must not trigger either TS1284 or TS1292.
#[test]
fn import_alias_to_class_export_default_reports_neither_verbatim() {
    let diagnostics = check(
        &[
            ("/impl.ts", "export class Bar { x = 1; }\n"),
            (
                "/main.ts",
                "import { Bar } from \"./impl\";\nexport default Bar;\n",
            ),
        ],
        "/main.ts",
        true,
    );

    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == EXPORT_DEFAULT_ONLY_REFERS_TO_A_TYPE
                || d.code == EXPORT_DEFAULT_REAL_VALUE
                || d.code == RESOLVES_TO_A_TYPE),
        "expected neither TS1284 nor TS1292 for a value-carrying import alias export default, got: {diagnostics:?}"
    );
}
