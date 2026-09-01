//! `export = <name>` under `verbatimModuleSyntax`, where `<name>` is a
//! whole-module `import [type] X = require("...")` alias, not a named ES
//! import (`import { X } from "..."`).
//!
//! `check_vms_export_equals`'s TS1282/TS1283 pick reads `import_name()`,
//! falling back to the local binding name to look up a member of that name in
//! the target module. That fallback is correct for a named import (where
//! `import_name()` legitimately means "look up this name") but wrong for
//! `import X = require(module)`: there `X` aliases the module's own
//! `export =` target as a unit, and no member literally called `X` exists in
//! the target — the lookup always misses, so `target_has_value` was always
//! `false` and every whole-module `import type X = require(...)` reported
//! TS1282 ("only refers to a type") even when the target module's `export =`
//! value genuinely has a value (tsc's TS1283, "resolves to a type-only
//! declaration"). tsz-org/tsz#17235.
//!
//! Oracle-confirmed against `typescript@7.0.2`.

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

const EXPORT_EQUALS_MUST_REFERENCE_A_VALUE: u32 = 1282;
const EXPORT_EQUALS_MUST_REFERENCE_A_REAL_VALUE: u32 = 1283;

fn check(files: &[(&str, &str)], entry: &str) -> Vec<Diagnostic> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            module: ModuleKind::CommonJS,
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

/// The exact #17235 repro: a merged `interface I {} / namespace I { export
/// const x = 1 }` has a genuine value (the namespace instantiates), so a
/// whole-module `import type J = require(...)` of it must report TS1283, not
/// TS1282.
#[test]
fn type_only_require_alias_to_merged_interface_namespace_reports_1283_not_1282() {
    let diagnostics = check(
        &[
            (
                "/c.ts",
                "export interface I {}\nnamespace I { export const x = 1; }\nexport = I;\n",
            ),
            ("/d.ts", "import type J = require(\"./c\");\nexport = J;\n"),
        ],
        "/d.ts",
    );

    assert_eq!(
        codes(&diagnostics),
        vec![EXPORT_EQUALS_MUST_REFERENCE_A_REAL_VALUE],
        "value-carrying export= target through a whole-module require alias must report TS1283, got: {diagnostics:?}"
    );
}

/// Control: when the target module's `export =` is a pure type (no merged
/// namespace, no value anywhere), the whole-module require alias still
/// reports TS1282 — the fix must not flip this case to TS1283.
#[test]
fn type_only_require_alias_to_pure_interface_reports_1282_not_1283() {
    let diagnostics = check(
        &[
            ("/c.ts", "export interface I { x: number }\nexport = I;\n"),
            ("/d.ts", "import type J = require(\"./c\");\nexport = J;\n"),
        ],
        "/d.ts",
    );

    assert_eq!(
        codes(&diagnostics),
        vec![EXPORT_EQUALS_MUST_REFERENCE_A_VALUE],
        "pure-type export= target through a whole-module require alias must stay TS1282, got: {diagnostics:?}"
    );
}

/// Control: a non-type-only whole-module require alias (`import J =
/// require(...)`, no `type` keyword) to a value-carrying target is clean —
/// this path never enters the `sym.is_type_only` branch at all.
#[test]
fn plain_require_alias_to_merged_interface_namespace_is_clean() {
    let diagnostics = check(
        &[
            (
                "/c.ts",
                "export interface I {}\nnamespace I { export const x = 1; }\nexport = I;\n",
            ),
            ("/d.ts", "import J = require(\"./c\");\nexport = J;\n"),
        ],
        "/d.ts",
    );

    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == EXPORT_EQUALS_MUST_REFERENCE_A_VALUE
                || d.code == EXPORT_EQUALS_MUST_REFERENCE_A_REAL_VALUE),
        "expected neither TS1282 nor TS1283 for a plain value-carrying require alias, got: {diagnostics:?}"
    );
}

/// Renamed-binder adjacent case: the local alias name differs from the
/// pattern in the primary repro (`Gadget`/`gizmo` instead of `J`/`I`), and the
/// target module export is named differently too — guards against any
/// hidden dependency on the specific identifiers used above.
#[test]
fn type_only_require_alias_renamed_binders_reports_1283_not_1282() {
    let diagnostics = check(
        &[
            (
                "/gizmo.ts",
                "export interface Gizmo {}\nnamespace Gizmo { export const version = 2; }\nexport = Gizmo;\n",
            ),
            (
                "/consumer.ts",
                "import type Gadget = require(\"./gizmo\");\nexport = Gadget;\n",
            ),
        ],
        "/consumer.ts",
    );

    assert_eq!(
        codes(&diagnostics),
        vec![EXPORT_EQUALS_MUST_REFERENCE_A_REAL_VALUE],
        "renamed whole-module require alias to a value-carrying target must report TS1283, got: {diagnostics:?}"
    );
}

/// Named ES import control (not a whole-module require): unaffected by the
/// fix, still resolves via the member-lookup path.
#[test]
fn type_only_named_import_alias_to_value_member_reports_1283_not_1282() {
    let diagnostics = check(
        &[
            (
                "/types.ts",
                "export class Foo {}\nexport interface Bar {}\n",
            ),
            (
                "/main.ts",
                "import type { Foo } from \"./types\";\nexport = Foo;\n",
            ),
        ],
        "/main.ts",
    );

    assert_eq!(
        codes(&diagnostics),
        vec![EXPORT_EQUALS_MUST_REFERENCE_A_REAL_VALUE],
        "named import alias to a value-carrying member must still report TS1283 via the member-lookup path, got: {diagnostics:?}"
    );
}
