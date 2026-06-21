//! A renamed re-export (`export { Orig as Exp }`, `Orig != Exp`) must NOT
//! introduce an in-module local binding for `Exp` — it only contributes `Exp` to
//! the module's export surface. tsz's binder was seeding `file_locals`/scope with
//! the renamed name, so in-module references to `Exp` wrongly resolved to the
//! export target instead of a sibling local declaration or a lib intrinsic.
//! (#14216, #14255)

use super::super::core::*;

fn opts() -> tsz_checker::context::CheckerOptions {
    tsz_checker::context::CheckerOptions {
        strict: true,
        strict_null_checks: true,
        module: tsz_common::common::ModuleKind::ESNext,
        ..Default::default()
    }
}

/// #14255 (TS2315): `export { Local as Capitalize }` must not shadow the lib
/// `Capitalize` string-mapping intrinsic in-module. `Capitalize<S>` must keep its
/// generic intrinsic meaning (no false "type is not generic").
#[test]
fn renamed_export_shadowing_intrinsic_no_ts2315() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
export type Cap<S extends string> = Capitalize<S>;
interface Local { tag: 'kind' }
export { Local as Capitalize };
"#,
    );
    assert!(
        !has_error(&diagnostics, 2315),
        "no TS2315 expected — the renamed export `Capitalize` is recorded only on \
         the export surface; in-module `Capitalize<S>` keeps the intrinsic meaning. \
         Actual: {diagnostics:#?}"
    );
}

/// #14216 (TS2552/TS2749): a module declaring a local `class Box` that also
/// re-exports a distinct local binding under the same name via
/// `export { box as Box }`. In-module references to `Box` must keep the local
/// class meaning (value and type).
#[test]
fn renamed_export_collides_local_class_no_ts2552_ts2749() {
    if !lib_files_available() {
        return;
    }
    let files = &[(
        "m.ts",
        "class Box { constructor(public value: number) {} }\n\
         const useType = (b: Box): number => b.value\n\
         const useValue = (): Box => new Box(1)\n\
         const box = (n: number) => new Box(n)\n\
         export { box as Box }",
    )];
    let diagnostics =
        compile_named_files_get_diagnostics_with_lib_and_options(files, "m.ts", opts());
    assert!(
        !has_error(&diagnostics, 2749),
        "no TS2749 expected — in-module `Box` type positions resolve to the local \
         class, not the export alias. Actual: {diagnostics:#?}"
    );
    assert!(
        !has_error(&diagnostics, 2552),
        "no TS2552 expected — in-module `Box` value positions resolve to the local \
         class constructor. Actual: {diagnostics:#?}"
    );
}
