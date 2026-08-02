//! TS2554 for calls to a JS-declared function documented only with bare
//! `@param name` tags (no `{type}`, no `[bracket]`).
//!
//! A name-only `@param` tag carries no type, so it must not override the
//! "untyped JS parameter is implicitly optional" leniency: tsc treats it the
//! same as no tag at all for arity purposes (`jsFileFunctionParametersAsOptional2.ts`,
//! verified against the pinned `typescript@7.0.2` oracle). Only a tag with an
//! explicit `{type}` pins the parameter's type and makes it required.

use crate::context::CheckerOptions;
use crate::test_utils::{check_multi_file_with_libs_stamped, check_source, load_lib_files};

const TOO_FEW_ARGUMENTS: u32 = 2554;

fn js_codes(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    let options = CheckerOptions {
        allow_js: true,
        ..CheckerOptions::default()
    };
    check_multi_file_with_libs_stamped(files, entry, options, &load_lib_files(&["es5.d.ts"]))
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn same_file_js_codes(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        allow_js: true,
        ..CheckerOptions::default()
    };
    check_source(source, "same.js", options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

// --- Cross-file: JS declares, a .ts file calls with fewer arguments. ---
// Witnesses `jsFileFunctionParametersAsOptional2.ts`.

const BARE_PARAM_DOC: &str =
    "/**\n * @param a\n * @param b\n * @param c\n */\nfunction f(a, b, c) { }\n";

#[test]
fn cross_file_bare_param_tags_allow_fewer_call_arguments() {
    let codes = js_codes(
        &[("foo.js", BARE_PARAM_DOC), ("bar.ts", "f();\n")],
        "bar.ts",
    );
    assert!(!codes.contains(&TOO_FEW_ARGUMENTS));
}

#[test]
fn cross_file_bare_param_tags_allow_one_call_argument() {
    let codes = js_codes(
        &[("foo.js", BARE_PARAM_DOC), ("bar.ts", "f(1);\n")],
        "bar.ts",
    );
    assert!(!codes.contains(&TOO_FEW_ARGUMENTS));
}

#[test]
fn cross_file_bare_param_tags_still_allow_full_arity() {
    let codes = js_codes(
        &[("foo.js", BARE_PARAM_DOC), ("bar.ts", "f(1, 2, 3);\n")],
        "bar.ts",
    );
    assert!(!codes.contains(&TOO_FEW_ARGUMENTS));
}

// --- Renamed binder: the fix must not be keyed on `f`/`a`/`b`/`c`. ---

#[test]
fn cross_file_bare_param_tags_allow_fewer_arguments_renamed() {
    let doc = "/**\n * @param first\n * @param second\n */\nfunction greet(first, second) { }\n";
    let codes = js_codes(
        &[("declare.js", doc), ("call.ts", "greet(\"hi\");\n")],
        "call.ts",
    );
    assert!(!codes.contains(&TOO_FEW_ARGUMENTS));
}

// --- Same-file: the leniency already worked for untyped params; bare
// @param tags on the same function in the same file must not regress it.

#[test]
fn same_file_bare_param_tags_allow_fewer_call_arguments() {
    let codes = same_file_js_codes(&format!("{BARE_PARAM_DOC}f();\nf(1);\nf(1, 2, 3);\n"));
    assert!(!codes.contains(&TOO_FEW_ARGUMENTS));
}

// --- Negative: a genuinely typed `@param {Type} name` tag still pins the
// parameter as required, in both same-file and cross-file forms.

#[test]
fn cross_file_typed_param_tag_still_required() {
    let doc = "/**\n * @param {string} a\n * @param {string} b\n */\nfunction f(a, b) { }\n";
    let codes = js_codes(&[("foo.js", doc), ("bar.ts", "f(\"x\");\n")], "bar.ts");
    assert!(codes.contains(&TOO_FEW_ARGUMENTS));
}

#[test]
fn same_file_typed_param_tag_still_required() {
    let doc =
        "/**\n * @param {string} a\n * @param {string} b\n */\nfunction f(a, b) { }\nf(\"x\");\n";
    let codes = same_file_js_codes(doc);
    assert!(codes.contains(&TOO_FEW_ARGUMENTS));
}

// --- Negative: a bracket-optional typed tag stays optional (unaffected by
// this fix, guarding against a regression in the adjacent branch). ---

#[test]
fn cross_file_bracket_optional_typed_param_stays_optional() {
    let doc = "/**\n * @param {string} a\n * @param {string} [b]\n */\nfunction f(a, b) { }\n";
    let codes = js_codes(&[("foo.js", doc), ("bar.ts", "f(\"x\");\n")], "bar.ts");
    assert!(!codes.contains(&TOO_FEW_ARGUMENTS));
}
