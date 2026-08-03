//! TS1233 vs TS1474: an export declaration nested outside a module's top
//! level (function body, block, arrow body) reports different wording in a
//! `.ts` file (`"...top level of a namespace or module."`) than in a `.js`
//! file (`"...top level of a module."`, no namespace phrase since JS has no
//! namespaces). `check_grammar_module_element_context`
//! (`state/state_checking_members/statement_callback_bridge.rs`) already made
//! this exact TS-vs-JS distinction for the sibling `IMPORT_DECLARATION` case
//! (TS1232/TS1473) but always emitted the namespace-flavored TS1233 message
//! for `EXPORT_DECLARATION`, leaving TS1474 permanently unreachable.
//!
//! Verified against the pinned tsc 7.0.2 (`--strict --target es2022
//! --module esnext`, plus `--allowJs --checkJs` for the `.js` witnesses).

use crate::context::CheckerOptions;
use crate::test_utils::check_source;

const EXPORT_DECL_TOP_LEVEL_OF_NAMESPACE_OR_MODULE: u32 = 1233;
const EXPORT_DECL_TOP_LEVEL_OF_MODULE: u32 = 1474;
const DEFAULT_EXPORT_TOP_LEVEL: u32 = 1258;
const MODIFIERS_CANNOT_APPEAR_HERE: u32 = 1184;

fn ts_codes(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.ts", options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn js_codes(source: &str) -> Vec<u32> {
    let options = CheckerOptions {
        allow_js: true,
        check_js: true,
        strict: true,
        ..CheckerOptions::default()
    };
    check_source(source, "test.js", options)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

// --- A named-export list nested in a function body. ---

#[test]
fn named_export_list_in_function_body_reports_ts1233_in_ts_file() {
    let source = "function f() {\n  export { y };\n}\nvar y = 1;\n";
    let codes = ts_codes(source);
    assert!(codes.contains(&EXPORT_DECL_TOP_LEVEL_OF_NAMESPACE_OR_MODULE));
    assert!(!codes.contains(&EXPORT_DECL_TOP_LEVEL_OF_MODULE));
}

#[test]
fn named_export_list_in_function_body_reports_ts1474_in_js_file() {
    let source = "function f() {\n  export { y };\n}\nvar y = 1;\n";
    let codes = js_codes(source);
    assert!(codes.contains(&EXPORT_DECL_TOP_LEVEL_OF_MODULE));
    assert!(!codes.contains(&EXPORT_DECL_TOP_LEVEL_OF_NAMESPACE_OR_MODULE));
}

// --- Nesting depth varies: a bare block and an arrow function body. ---

#[test]
fn named_export_list_in_bare_block_reports_ts1233_in_ts_file() {
    let source = "{\n  export { y };\n}\nvar y = 1;\n";
    let codes = ts_codes(source);
    assert!(codes.contains(&EXPORT_DECL_TOP_LEVEL_OF_NAMESPACE_OR_MODULE));
    assert!(!codes.contains(&EXPORT_DECL_TOP_LEVEL_OF_MODULE));
}

#[test]
fn named_export_list_in_bare_block_reports_ts1474_in_js_file() {
    let source = "{\n  export { y };\n}\nvar y = 1;\n";
    let codes = js_codes(source);
    assert!(codes.contains(&EXPORT_DECL_TOP_LEVEL_OF_MODULE));
    assert!(!codes.contains(&EXPORT_DECL_TOP_LEVEL_OF_NAMESPACE_OR_MODULE));
}

#[test]
fn named_export_list_in_arrow_body_reports_ts1233_in_ts_file() {
    let source = "const f = () => {\n  export { y };\n};\nvar y = 1;\n";
    let codes = ts_codes(source);
    assert!(codes.contains(&EXPORT_DECL_TOP_LEVEL_OF_NAMESPACE_OR_MODULE));
    assert!(!codes.contains(&EXPORT_DECL_TOP_LEVEL_OF_MODULE));
}

#[test]
fn named_export_list_in_arrow_body_reports_ts1474_in_js_file() {
    let source = "const f = () => {\n  export { y };\n};\nvar y = 1;\n";
    let codes = js_codes(source);
    assert!(codes.contains(&EXPORT_DECL_TOP_LEVEL_OF_MODULE));
    assert!(!codes.contains(&EXPORT_DECL_TOP_LEVEL_OF_NAMESPACE_OR_MODULE));
}

// --- Positive control: a top-level export list is clean in both file kinds. ---

#[test]
fn named_export_list_at_top_level_is_clean_in_ts_file() {
    let source = "var y = 1;\nexport { y };\n";
    let codes = ts_codes(source);
    assert!(!codes.contains(&EXPORT_DECL_TOP_LEVEL_OF_NAMESPACE_OR_MODULE));
    assert!(!codes.contains(&EXPORT_DECL_TOP_LEVEL_OF_MODULE));
}

#[test]
fn named_export_list_at_top_level_is_clean_in_js_file() {
    let source = "var y = 1;\nexport { y };\n";
    let codes = js_codes(source);
    assert!(!codes.contains(&EXPORT_DECL_TOP_LEVEL_OF_NAMESPACE_OR_MODULE));
    assert!(!codes.contains(&EXPORT_DECL_TOP_LEVEL_OF_MODULE));
}

// --- Fallback controls: sibling grammar codes on the same statement kind do
// --- NOT gain a JS-file variant, so they must stay byte-identical across
// --- both file kinds (regression net for the two other arms this match
// --- expression shares with the fix).

#[test]
fn default_export_nested_reports_ts1258_regardless_of_file_kind() {
    let source = "function f() {\n  export default 5;\n}\n";
    let ts = ts_codes(source);
    let js = js_codes(source);
    assert!(ts.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(js.contains(&DEFAULT_EXPORT_TOP_LEVEL));
    assert!(!ts.contains(&EXPORT_DECL_TOP_LEVEL_OF_MODULE));
    assert!(!js.contains(&EXPORT_DECL_TOP_LEVEL_OF_NAMESPACE_OR_MODULE));
}

#[test]
fn exported_function_declaration_nested_reports_ts1184_regardless_of_file_kind() {
    let source = "function f() {\n  export function g() {}\n}\n";
    let ts = ts_codes(source);
    let js = js_codes(source);
    assert!(ts.contains(&MODIFIERS_CANNOT_APPEAR_HERE));
    assert!(js.contains(&MODIFIERS_CANNOT_APPEAR_HERE));
    assert!(!ts.contains(&EXPORT_DECL_TOP_LEVEL_OF_NAMESPACE_OR_MODULE));
    assert!(!js.contains(&EXPORT_DECL_TOP_LEVEL_OF_MODULE));
}
