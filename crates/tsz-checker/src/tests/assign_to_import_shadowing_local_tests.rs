//! Regression tests for false TS2632 ("Cannot assign to 'X' because it is an
//! import") when a `let`/`const`/parameter lexically shadows a same-named
//! module import and is used as an assignment target.
//!
//! Structural rule: an assignment whose target identifier resolves, in the
//! innermost scope, to a local `let`/`const`/parameter that SHADOWS a same-named
//! import must bind to the local. The "cannot assign to import" (TS2632) check
//! applies only when the target genuinely resolves to the import binding. tsz
//! previously consulted `file_locals` directly for the import alias, which
//! always returned the module-scoped import even when an inner local shadowed
//! it, producing a false TS2632.
//!
//! Owner: checker assignment-to-import check (`check_function_assignment`),
//! which now walks the scope chain to the innermost raw binding before testing
//! the ALIAS flag.
//!
//! Mined from **tanstack-router** (`router.ts`: a function-local `let redirect`
//! shadows `import { redirect }`).

use std::sync::{Arc, OnceLock};
use tsz_binder::lib_loader::LibFile;
use tsz_common::common::ModuleKind;

use crate::CheckerOptions;
use crate::test_utils::{check_multi_file_with_libs_stamped, load_default_lib_files};

fn default_libs() -> &'static [Arc<LibFile>] {
    static DEFAULT_LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
    DEFAULT_LIBS.get_or_init(load_default_lib_files)
}

fn opts() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        module: ModuleKind::ESNext,
        ..CheckerOptions::default()
    }
}

fn code_count(files: &[(&str, &str)], entry: &str, code: u32) -> usize {
    check_multi_file_with_libs_stamped(files, entry, opts(), default_libs())
        .iter()
        .filter(|d| d.code == code)
        .count()
}

const LIB: &str = "export const redirect = (n: number) => n;\n";

#[test]
fn function_scope_let_shadowing_import_is_not_an_import_assignment() {
    let files = &[
        ("lib.ts", LIB),
        (
            "main.ts",
            "import { redirect } from \"./lib\";\n\
             export function use() {\n\
               let redirect: { x: number } | undefined;\n\
               redirect = { x: 1 };\n\
               return redirect.x;\n\
             }\n\
             export const _ = redirect;\n",
        ),
    ];
    assert_eq!(
        code_count(files, "main.ts", 2632),
        0,
        "a function-scope `let` shadowing an import binds the assignment to the local, not the import"
    );
}

#[test]
fn nested_block_let_shadowing_import_is_not_an_import_assignment() {
    // Vary the binder name (`navigate`) to prove the fix is structural, not
    // keyed on a specific identifier string.
    let files = &[
        ("lib.ts", "export const navigate = (n: number) => n;\n"),
        (
            "main.ts",
            "import { navigate } from \"./lib\";\n\
             export function go() {\n\
               {\n\
                 let navigate: { x: number } | undefined;\n\
                 navigate = { x: 1 };\n\
                 return navigate?.x;\n\
               }\n\
             }\n\
             export const _ = navigate;\n",
        ),
    ];
    assert_eq!(
        code_count(files, "main.ts", 2632),
        0,
        "a nested-block `let` shadowing an import binds the assignment to the local, not the import"
    );
}

#[test]
fn nested_function_let_shadowing_import_is_not_an_import_assignment() {
    let files = &[
        ("lib.ts", "export const cursor = (n: number) => n;\n"),
        (
            "main.ts",
            "import { cursor } from \"./lib\";\n\
             export function outer() {\n\
               function inner() {\n\
                 let cursor: { x: number } | undefined;\n\
                 cursor = { x: 1 };\n\
                 return cursor.x;\n\
               }\n\
               return inner();\n\
             }\n\
             export const _ = cursor;\n",
        ),
    ];
    assert_eq!(
        code_count(files, "main.ts", 2632),
        0,
        "a `let` in a nested function shadowing an import binds the assignment to the local"
    );
}

#[test]
fn const_local_shadowing_import_reports_const_not_import() {
    // Assigning to a shadowing `const` local must report TS2588 (constant),
    // NOT TS2632 (import) — the assignment binds to the local.
    let files = &[
        ("lib.ts", "export const handle = (n: number) => n;\n"),
        (
            "main.ts",
            "import { handle } from \"./lib\";\n\
             export function use() {\n\
               const handle: { x: number } = { x: 1 };\n\
               handle = { x: 2 };\n\
               return handle.x;\n\
             }\n\
             export const _ = handle;\n",
        ),
    ];
    assert_eq!(
        code_count(files, "main.ts", 2632),
        0,
        "assigning to a shadowing `const` local must not report TS2632"
    );
    assert_eq!(
        code_count(files, "main.ts", 2588),
        1,
        "assigning to a shadowing `const` local reports TS2588 (constant)"
    );
}

#[test]
fn assigning_to_actual_import_without_shadow_still_reports_import() {
    // Negative control: no shadowing local — the assignment target genuinely
    // resolves to the import binding, so TS2632 must still fire.
    let files = &[
        ("lib.ts", "export const beacon = (n: number) => n;\n"),
        (
            "main.ts",
            "import { beacon } from \"./lib\";\n\
             export function use() {\n\
               beacon = (() => 0) as any;\n\
               return beacon;\n\
             }\n",
        ),
    ];
    assert_eq!(
        code_count(files, "main.ts", 2632),
        1,
        "assigning to a genuine import binding (no shadow) must still report TS2632"
    );
}

#[test]
fn assigning_to_reexported_import_without_shadow_still_reports_import() {
    // Negative control: a re-exported import is still an import binding at the
    // use site, so assigning to it (no shadow) must report TS2632.
    let files = &[
        ("lib.ts", "export const anchor = (n: number) => n;\n"),
        ("reexport.ts", "export { anchor } from \"./lib\";\n"),
        (
            "main.ts",
            "import { anchor } from \"./reexport\";\n\
             export function use() {\n\
               anchor = (() => 0) as any;\n\
             }\n",
        ),
    ];
    assert_eq!(
        code_count(files, "main.ts", 2632),
        1,
        "assigning to a re-exported import binding (no shadow) must still report TS2632"
    );
}
