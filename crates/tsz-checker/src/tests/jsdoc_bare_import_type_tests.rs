//! TS1340 for a bare `import('mod')` used as a type in JSDoc.
//!
//! A bare import type names the module's `export =` type. The module's own
//! namespace is not a type, so a module exporting only values — or one with
//! named type exports and no `export =` — is TS1340, verified against the pinned
//! tsc 7.0.2:
//!
//! ```text
//! ex.d.ts : export var config: {}
//! test.js : /** @param {import('./ex')} a */ function demo(a) {}
//!           -> TS1340 Module './ex' does not refer to a type ...
//! ```
//!
//! `import('mod').Member` is unaffected: it is not a bare import type, and is
//! how named type exports are meant to be reached.
//!
//! The TypeScript import-type resolver already reported this; JSDoc reaches the
//! question through a separate path and now asks the same shared predicate.

use crate::context::CheckerOptions;
use crate::test_utils::{check_multi_file_with_libs_stamped, load_lib_files};

fn js_codes(files: &[(&str, &str)]) -> Vec<u32> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        ..CheckerOptions::default()
    };
    let entry = files
        .iter()
        .map(|(name, _)| *name)
        .find(|name| name.ends_with(".js"))
        .unwrap_or(files[0].0);
    check_multi_file_with_libs_stamped(files, entry, options, &load_lib_files(&["es5.d.ts"]))
        .into_iter()
        .map(|d| d.code)
        .collect()
}

const VALUES_ONLY: &str = "export var config: {}\n";
const NAMED_TYPE: &str = "export interface Thing { a: number }\n";
const EXPORT_EQUALS_TYPE: &str = "interface Thing { a: number }\nexport = Thing;\n";

// --- Bare import of a module with no `export =` type: TS1340. ---

#[test]
fn bare_import_of_values_only_module_reports() {
    let files = [
        ("ex.d.ts", VALUES_ONLY),
        (
            "test.js",
            "/** @param {import('./ex')} a */\nfunction demo(a) { return a }\ndemo\n",
        ),
    ];
    assert!(js_codes(&files).contains(&1340));
}

/// Named type exports do not make the module itself a type — the case most
/// likely to be got wrong.
#[test]
fn bare_import_of_named_type_export_module_reports() {
    let files = [
        ("ty.d.ts", NAMED_TYPE),
        (
            "test.js",
            "/** @param {import('./ty')} b */\nfunction demo(b) { return b }\ndemo\n",
        ),
    ];
    assert!(js_codes(&files).contains(&1340));
}

/// `@returns` takes the same path as `@param`.
#[test]
fn bare_import_in_returns_tag_reports() {
    let files = [
        ("ex.d.ts", VALUES_ONLY),
        (
            "test.js",
            "/** @returns {import('./ex')} */\nfunction demo() { return null }\ndemo\n",
        ),
    ];
    assert!(js_codes(&files).contains(&1340));
}

/// A renamed module and parameter: the rule is structural.
#[test]
fn bare_import_rule_is_not_name_specific() {
    let files = [
        ("helpers.d.ts", "export var helper: {}\n"),
        (
            "consumer.js",
            "/** @param {import('./helpers')} h */\nfunction use(h) { return h }\nuse\n",
        ),
    ];
    assert!(js_codes(&files).contains(&1340));
}

// --- Negatives: these must stay silent. ---

#[test]
fn bare_import_of_export_equals_type_is_accepted() {
    let files = [
        ("ty.d.ts", EXPORT_EQUALS_TYPE),
        (
            "test.js",
            "/** @param {import('./ty')} b */\nfunction demo(b) { return b }\ndemo\n",
        ),
    ];
    assert!(!js_codes(&files).contains(&1340));
}

/// Member access reaches named type exports and is not a bare import type.
#[test]
fn member_access_import_type_is_accepted() {
    let files = [
        ("ty.d.ts", NAMED_TYPE),
        (
            "test.js",
            "/** @param {import('./ty').Thing} c */\nfunction demo(c) { return c }\ndemo\n",
        ),
    ];
    assert!(!js_codes(&files).contains(&1340));
}

/// A plain named type keeps working — the new branch must not swallow ordinary
/// `@param` handling.
#[test]
fn non_import_param_type_is_unaffected() {
    let files = [(
        "test.js",
        "/** @param {string} s */\nfunction demo(s) { return s }\ndemo\n",
    )];
    let codes = js_codes(&files);
    assert!(!codes.contains(&1340) && !codes.contains(&2304));
}

/// An unresolvable plain name still reports TS2304 from the same scan.
#[test]
fn unresolvable_param_type_still_reports_2304() {
    let files = [(
        "test.js",
        "/** @param {NoSuchTypeHere} a */\nfunction demo(a) { return a }\ndemo\n",
    )];
    assert!(js_codes(&files).contains(&2304));
}

// --- `export =` must supply a TYPE, not merely exist. ---

/// `declare var config: {...}; export = config` exports a VALUE, so the module
/// is not usable as a bare import type (witness:
/// `jsdocImportTypeReferenceToCommonjsModule`).
#[test]
fn bare_import_of_export_equals_value_reports() {
    let files = [
        (
            "ex.d.ts",
            "declare var config: { fix: boolean }\nexport = config;\n",
        ),
        (
            "test.js",
            "/** @param {import('./ex')} a */\nfunction demo(a) { return a }\ndemo\n",
        ),
    ];
    assert!(js_codes(&files).contains(&1340));
}

/// A class carries a type meaning alongside its value meaning, so
/// `class Conn {} export = Conn` IS a valid bare import type. This is the case
/// that a naive "export = must be a pure type" rule gets wrong — it regressed
/// `declarationImportTypeAliasInferredAndEmittable` until classes were counted.
#[test]
fn bare_import_of_export_equals_class_is_accepted() {
    let files = [
        (
            "foo.d.ts",
            "declare class Conn { item: number }\nexport = Conn;\n",
        ),
        (
            "test.js",
            "/** @param {import('./foo')} c */\nfunction demo(c) { return c }\ndemo\n",
        ),
    ];
    assert!(!js_codes(&files).contains(&1340));
}

/// An enum likewise declares a type.
#[test]
fn bare_import_of_export_equals_enum_is_accepted() {
    let files = [
        ("e.d.ts", "declare enum E { A, B }\nexport = E;\n"),
        (
            "test.js",
            "/** @param {import('./e')} e */\nfunction demo(e) { return e }\ndemo\n",
        ),
    ];
    assert!(!js_codes(&files).contains(&1340));
}
