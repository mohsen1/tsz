//! Code selection for value re-exports / imports of type-only bindings under
//! `verbatimModuleSyntax` and `isolatedModules`.
//!
//! tsc distinguishes purely by whether the resolved target carries a runtime
//! value (oracle-verified against `typescript@7.0.2`):
//!
//! | shape                                             | re-export | import  |
//! | ------------------------------------------------- | --------- | ------- |
//! | pure type (interface / type alias / `export type T`) | TS1205 | TS1484  |
//! | value reached type-only (`export type { Cls }` /  |           |         |
//! | `import type`)                                     | TS1448    | TS1485  |
//! | value reached normally                            | (clean)   | (clean) |
//!
//! Before this was fixed, tsz treated any type-only-marked binding as an
//! inherent type and emitted TS1205/TS1484 even when the target was a value,
//! and the re-export path only reached TS1448 under `isolatedModules`, never
//! under `verbatimModuleSyntax`. The code is now decided by
//! `lookup_imported_target_flags` (runtime-value resolution, following an
//! intermediate `import type` alias) and is identical for both modes.

use crate::context::CheckerOptions;
use crate::diagnostics::Diagnostic;
use crate::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

const RE_EXPORTING_A_TYPE: u32 = 1205;
const RESOLVES_TO_TYPE_ONLY_REEXPORT: u32 = 1448;
const IS_A_TYPE_IMPORT: u32 = 1484;
const RESOLVES_TO_TYPE_ONLY_IMPORT: u32 = 1485;

fn opts(verbatim: bool) -> CheckerOptions {
    CheckerOptions {
        module: ModuleKind::ESNext,
        strict: true,
        verbatim_module_syntax: verbatim,
        isolated_modules: !verbatim,
        ..CheckerOptions::default()
    }
}

fn codes(files: &[(&str, &str)], entry: &str, verbatim: bool) -> Vec<u32> {
    let mut codes: Vec<u32> = check_multi_file(files, entry, opts(verbatim))
        .iter()
        .map(|d: &Diagnostic| d.code)
        .collect();
    codes.sort_unstable();
    codes
}

// --- re-export via `export { X } from "./a"` (from-clause) -------------------

#[test]
fn from_reexport_of_type_only_exported_class_is_1448_not_1205_verbatim() {
    let files = &[
        ("/a.ts", "class Foo {}\nexport type { Foo };\n"),
        ("/b.ts", "export { Foo } from \"./a\";\n"),
    ];
    assert_eq!(
        codes(files, "/b.ts", true),
        vec![RESOLVES_TO_TYPE_ONLY_REEXPORT]
    );
}

#[test]
fn from_reexport_of_type_only_exported_class_is_1448_isolated_modules() {
    let files = &[
        ("/a.ts", "class Foo {}\nexport type { Foo };\n"),
        ("/b.ts", "export { Foo } from \"./a\";\n"),
    ];
    assert_eq!(
        codes(files, "/b.ts", false),
        vec![RESOLVES_TO_TYPE_ONLY_REEXPORT]
    );
}

#[test]
fn from_reexport_of_pure_interface_is_1205() {
    let files = &[
        ("/a.ts", "interface I {}\nexport type { I };\n"),
        ("/b.ts", "export { I } from \"./a\";\n"),
    ];
    assert_eq!(codes(files, "/b.ts", true), vec![RE_EXPORTING_A_TYPE]);
    assert_eq!(codes(files, "/b.ts", false), vec![RE_EXPORTING_A_TYPE]);
}

#[test]
fn from_reexport_of_plain_type_alias_is_1205() {
    let files = &[
        ("/a.ts", "export type T = number;\n"),
        ("/b.ts", "export { T } from \"./a\";\n"),
    ];
    assert_eq!(codes(files, "/b.ts", true), vec![RE_EXPORTING_A_TYPE]);
}

#[test]
fn from_reexport_of_normal_value_is_clean() {
    let files = &[
        ("/a.ts", "export class V {}\n"),
        ("/b.ts", "export { V } from \"./a\";\n"),
    ];
    assert!(codes(files, "/b.ts", true).is_empty());
    assert!(codes(files, "/b.ts", false).is_empty());
}

// --- local `import { X } from "./a"; export { X }` (no from-clause) ----------

#[test]
fn local_import_then_reexport_of_type_only_class_is_1448_and_1485_verbatim() {
    let files = &[
        ("/a.ts", "class Foo {}\nexport type { Foo };\n"),
        ("/b.ts", "import { Foo } from \"./a\";\nexport { Foo };\n"),
    ];
    // The import specifier -> TS1485, the local re-export -> TS1448.
    assert_eq!(
        codes(files, "/b.ts", true),
        vec![RESOLVES_TO_TYPE_ONLY_REEXPORT, RESOLVES_TO_TYPE_ONLY_IMPORT]
    );
}

#[test]
fn local_import_then_reexport_of_type_only_class_is_1448_isolated_modules() {
    let files = &[
        ("/a.ts", "class Foo {}\nexport type { Foo };\n"),
        ("/b.ts", "import { Foo } from \"./a\";\nexport { Foo };\n"),
    ];
    // isolatedModules does not check imports, so only the re-export fires.
    assert_eq!(
        codes(files, "/b.ts", false),
        vec![RESOLVES_TO_TYPE_ONLY_REEXPORT]
    );
}

// --- import specifier codes (verbatimModuleSyntax only) ----------------------

#[test]
fn import_of_type_only_exported_class_is_1485_not_1484() {
    let files = &[
        ("/a.ts", "class Foo {}\nexport type { Foo };\n"),
        ("/b.ts", "import { Foo } from \"./a\";\nnew Foo();\n"),
    ];
    let codes = codes(files, "/b.ts", true);
    assert!(
        codes.contains(&RESOLVES_TO_TYPE_ONLY_IMPORT),
        "expected TS1485, got {codes:?}"
    );
    assert!(
        !codes.contains(&IS_A_TYPE_IMPORT),
        "must not emit TS1484: {codes:?}"
    );
}

#[test]
fn import_of_pure_interface_is_1484() {
    let files = &[
        ("/a.ts", "interface I {}\nexport type { I };\n"),
        (
            "/b.ts",
            "import { I } from \"./a\";\nconst z: I = null as unknown as I;\n",
        ),
    ];
    let codes = codes(files, "/b.ts", true);
    assert!(
        codes.contains(&IS_A_TYPE_IMPORT),
        "expected TS1484, got {codes:?}"
    );
    assert!(
        !codes.contains(&RESOLVES_TO_TYPE_ONLY_IMPORT),
        "must not emit TS1485: {codes:?}"
    );
}

// --- intermediate `import type` chain (deep value resolution) ----------------

#[test]
fn from_reexport_through_intermediate_import_type_alias_is_1448() {
    // x declares the class; a re-exports it through `import type`; b's
    // from-clause re-export must still see the runtime value -> TS1448.
    let files = &[
        ("/x.ts", "export class X {}\n"),
        ("/a.ts", "import type { X } from \"./x\";\nexport { X };\n"),
        ("/b.ts", "export { X } from \"./a\";\n"),
    ];
    assert_eq!(
        codes(files, "/b.ts", true),
        vec![RESOLVES_TO_TYPE_ONLY_REEXPORT]
    );
}

// --- anti-hardcoding: binder names must not matter ---------------------------

#[test]
fn renamed_binders_from_reexport_of_type_only_class_is_1448() {
    let files = &[
        ("/dep.ts", "class Widget {}\nexport type { Widget };\n"),
        ("/idx.ts", "export { Widget } from \"./dep\";\n"),
    ];
    assert_eq!(
        codes(files, "/idx.ts", true),
        vec![RESOLVES_TO_TYPE_ONLY_REEXPORT]
    );
}

#[test]
fn renamed_binders_import_of_type_only_class_is_1485() {
    let files = &[
        ("/dep.ts", "class Gadget {}\nexport type { Gadget };\n"),
        (
            "/idx.ts",
            "import { Gadget as G } from \"./dep\";\nnew G();\n",
        ),
    ];
    let codes = codes(files, "/idx.ts", true);
    assert!(
        codes.contains(&RESOLVES_TO_TYPE_ONLY_IMPORT) && !codes.contains(&IS_A_TYPE_IMPORT),
        "expected TS1485 (not TS1484), got {codes:?}"
    );
}
