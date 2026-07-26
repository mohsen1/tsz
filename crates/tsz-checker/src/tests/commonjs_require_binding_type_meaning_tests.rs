//! When a `require()`-imported binding may be used as a JSDoc **type**.
//!
//! `tsc` allows it only when the module assigns a class directly to that
//! export. A plain value, a function, or a value reached through another object
//! carries only a value meaning, and using the imported name as a type is
//! TS2749.
//!
//! The discriminator has to be **syntactic**: `exports.K = class {}` and
//! `var NS = {}; NS.K = class {}; exports.K = NS.K` resolve to the *same* type,
//! yet tsc accepts only the first. Verified against the pinned tsc 7.0.2.

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

const VALUE_USED_AS_TYPE: u32 = 2749;

const USE: &str =
    "const { K } = require('./mod.js');\n/** @param {K} k */\nfunction f(k) { return k }\nf\n";

fn reports(module_src: &str) -> bool {
    js_codes(&[("mod.js", module_src), ("use.js", USE)], "use.js").contains(&VALUE_USED_AS_TYPE)
}

// --- Direct class export: usable as a type. ---

#[test]
fn direct_class_expression_export_is_a_type() {
    assert!(!reports("exports.K = class { m() { } };\n"));
}

#[test]
fn direct_class_declaration_export_is_a_type() {
    assert!(!reports("class K { m() { } }\nexports.K = K;\n"));
}

#[test]
fn module_exports_receiver_is_also_accepted() {
    assert!(!reports("class K { m() { } }\nmodule.exports.K = K;\n"));
}

// The indirect shapes (`var NS = {}; NS.K = class {}; exports.K = NS.K`) are
// covered by the conformance witness `commonJSImportNestedClassTypeReference`
// and by direct CLI verification against tsc; this multi-file unit harness does
// not resolve that cross-file expando chain, so asserting it here would pass
// vacuously.

// --- Non-class exports are value-only however they are written. ---

#[test]
fn direct_value_export_is_value_only() {
    assert!(reports("exports.K = 1;\n"));
}

#[test]
fn indirect_value_export_is_value_only() {
    assert!(reports("var NS = {}\nNS.K = 1;\nexports.K = NS.K;\n"));
}

#[test]
fn function_export_is_value_only() {
    assert!(reports("exports.K = function () { };\n"));
}

// --- A renamed binding element resolves through its property name. ---

/// The direct form still resolves under a renamed binding.
#[test]
fn renamed_binding_element_accepts_a_direct_class() {
    let module_src = "exports.K = class { m() { } };\n";
    let use_src = "const { K: Local } = require('./mod.js');\n/** @param {Local} k */\nfunction f(k) { return k }\nf\n";
    let codes = js_codes(&[("mod.js", module_src), ("use.js", use_src)], "use.js");
    assert!(!codes.contains(&VALUE_USED_AS_TYPE));
}
