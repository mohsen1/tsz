//! TS1340 for a bare `import('mod')` as a `@typedef` base.
//!
//! A bare import type names the module's exported type. A TypeScript module
//! supplies one through `export =`; a JS module does so by assigning a class to
//! `module.exports`. A module that exports a plain function or a value does not,
//! and `tsc` reports TS1340.
//!
//! `@import` tags also carry a module specifier but are value imports, not type
//! references, and must never be flagged.

use crate::context::CheckerOptions;
use crate::test_utils::{check_multi_file_with_libs_stamped, load_lib_files};

fn js_codes(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..CheckerOptions::default()
    };
    check_multi_file_with_libs_stamped(files, entry, options, &load_lib_files(&["es5.d.ts"]))
        .into_iter()
        .map(|d| d.code)
        .collect()
}

const MODULE_IS_NOT_A_TYPE: u32 = 1340;

const TYPEDEF_USE: &str =
    "/** @typedef {import('./mod.js')} M */\n/** @param {M} m */\nfunction f(m) { return m }\nf\n";

fn reports(module_src: &str) -> bool {
    js_codes(&[("mod.js", module_src), ("use.js", TYPEDEF_USE)], "use.js")
        .contains(&MODULE_IS_NOT_A_TYPE)
}

// --- A JS module exporting a class supplies a type. ---

#[test]
fn module_exports_class_expression_is_a_type() {
    assert!(!reports("module.exports = class { m() { } };\n"));
}

#[test]
fn module_exports_class_declaration_is_a_type() {
    assert!(!reports("class C { m() { } }\nmodule.exports = C;\n"));
}

// --- A function or value export does not. ---

/// Witness `jsdocTypeReferenceToImportOfFunctionExpression`.
#[test]
fn module_exports_function_expression_is_not_a_type() {
    assert!(reports("module.exports = function MC() { };\n"));
}

#[test]
fn module_exports_value_is_not_a_type() {
    assert!(reports("module.exports = 1;\n"));
}

// --- `@import` tags are value imports and must not be flagged. ---

#[test]
fn import_tag_is_never_flagged() {
    let files = [
        ("types.js", "module.exports = 1;\n"),
        (
            "use.js",
            "/** @import { Thing } from './types.js' */\nvar x = 1;\nx\n",
        ),
    ];
    assert!(!js_codes(&files, "use.js").contains(&MODULE_IS_NOT_A_TYPE));
}

/// A typedef whose base is not an import is unaffected.
#[test]
fn non_import_typedef_base_is_unaffected() {
    let files = [(
        "use.js",
        "/** @typedef {number} Num */\n/** @param {Num} n */\nfunction f(n) { return n }\nf\n",
    )];
    assert!(!js_codes(&files, "use.js").contains(&MODULE_IS_NOT_A_TYPE));
}
