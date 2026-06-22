//! Regression tests for false TS2348 when an imported value binding is named
//! like a global constructor (`Promise`, `Map`, `Object`, ...).
//!
//! Structural rule: a module-scoped import binding lexically shadows the
//! ambient global of the same name. When the imported binding is used in call
//! position, tsc resolves to the *imported* value, not the global constructor.
//! tsz previously let a known-global value-recovery override the import alias
//! (the alias symbol carries `ALIAS` but not `VALUE`), so the call resolved to
//! the non-callable global constructor and emitted a false TS2348.
//!
//! Mined from **typebox**. Issue:
//! <https://github.com/tsz-org/tsz/issues/14263>

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

fn ts2348_count(files: &[(&str, &str)], entry: &str) -> usize {
    check_multi_file_with_libs_stamped(files, entry, opts(), default_libs())
        .iter()
        .filter(|d| d.code == 2348)
        .count()
}

const LIB: &str = "export function Promise(x: number): string { return \"\"; }\n\
    export function Map(x: number): string { return \"\"; }\n\
    export function Object(x: number): string { return \"\"; }\n";

#[test]
fn imported_value_named_like_global_ctor_shadows_global_in_call() {
    let files = &[
        ("lib.ts", LIB),
        (
            "main.ts",
            "import { Promise, Map, Object } from \"./lib\";\n\
             const a: string = Promise(1);\n\
             const b: string = Map(2);\n\
             const c: string = Object(3);\n\
             export {};\n",
        ),
    ];
    assert_eq!(
        ts2348_count(files, "main.ts"),
        0,
        "imported value named like a global constructor must shadow the global in call position"
    );
}

#[test]
fn renamed_import_to_global_ctor_name_shadows_global_in_call() {
    let files = &[
        (
            "lib.ts",
            "export function p(x: number): string { return \"\"; }\n",
        ),
        (
            "main.ts",
            "import { p as Promise } from \"./lib\";\n\
             const a: string = Promise(1);\n\
             export {};\n",
        ),
    ];
    assert_eq!(
        ts2348_count(files, "main.ts"),
        0,
        "renamed import to a global-constructor name must shadow the global in call position"
    );
}

#[test]
fn reexported_import_to_global_ctor_name_shadows_global_in_call() {
    let files = &[
        (
            "lib.ts",
            "export function local(x: number): string { return \"\"; }\n",
        ),
        ("reexport.ts", "export { local as Map } from \"./lib\";\n"),
        (
            "main.ts",
            "import { Map } from \"./reexport\";\n\
             const b: string = Map(2);\n\
             export {};\n",
        ),
    ];
    assert_eq!(
        ts2348_count(files, "main.ts"),
        0,
        "re-exported import to a global-constructor name must shadow the global in call position"
    );
}

#[test]
fn no_import_global_ctor_still_resolves_to_non_callable_global() {
    // Negative control: without an import binding, `Promise` resolves to the
    // global PromiseConstructor, which is not callable → genuine TS2348.
    let files = &[("main.ts", "const a = Promise(1);\nexport {};\n")];
    assert_eq!(
        ts2348_count(files, "main.ts"),
        1,
        "without an import, the global Promise constructor is not callable (genuine TS2348)"
    );
}

#[test]
fn module_local_function_named_like_global_ctor_is_callable_in_same_module() {
    // A module-local `export function Promise` shadows the global value even
    // when referenced directly in the same module. The binder preserves the
    // lib's `interface Promise` TYPE meaning, so the merged symbol carries
    // INTERFACE+FUNCTION, but the value side is the user's function — not the
    // non-callable PromiseConstructor.
    let files = &[(
        "main.ts",
        "export function Promise(x: number): string { return \"\"; }\n\
         const a: string = Promise(1);\n",
    )];
    assert_eq!(
        ts2348_count(files, "main.ts"),
        0,
        "a module-local function named like a global constructor must be callable"
    );
}

#[test]
fn imported_value_named_like_global_ctor_returns_imported_value_type() {
    // The imported binding's return type (`string`) must flow through, proving
    // we resolved the imported function — not the global constructor.
    let files = &[
        (
            "lib.ts",
            "export function Promise(x: number): number { return x; }\n",
        ),
        (
            "main.ts",
            // Assigning the `number` result to a `string` annotation must fail
            // with TS2322 — which only happens if `Promise(1)` typed as the
            // imported function returning `number`.
            "import { Promise } from \"./lib\";\nconst a: string = Promise(1);\nexport {};\n",
        ),
    ];
    let diags = check_multi_file_with_libs_stamped(files, "main.ts", opts(), default_libs());
    assert_eq!(
        diags.iter().filter(|d| d.code == 2348).count(),
        0,
        "imported function must be callable (no TS2348)"
    );
    assert_eq!(
        diags.iter().filter(|d| d.code == 2322).count(),
        1,
        "the imported function's `number` return must not assign to a `string` (TS2322)"
    );
}
