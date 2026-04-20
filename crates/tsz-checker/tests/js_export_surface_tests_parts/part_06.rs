#[test]
fn test_commonjs_direct_export_property_overlap_is_union_typed_cross_file() {
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
module.exports.bothBefore = "string";
A.justExport = 4;
A.bothBefore = 2;
A.bothAfter = 3;
module.exports = A;
function A() {
    this.p = 1;
}
module.exports.bothAfter = "string";
module.exports.justProperty = "string";
"#,
        "consumer.ts",
        r#"
import mod1 = require("./mod1");
declare function takesNumber(value: number): void;
takesNumber(mod1.justExport);
takesNumber(mod1.bothBefore);
takesNumber(mod1.bothAfter);
"#,
        "./mod1",
    );

    let number_mismatch_errors: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2345)
        .collect();
    assert!(
        number_mismatch_errors.len() >= 2,
        "Expected overlapping CommonJS exports to stay union-typed and reject number-only consumers, got: {diagnostics:#?}"
    );
}

#[test]
fn test_commonjs_direct_export_property_overlap_reports_ts2323_in_js_file() {
    let diagnostics = check_commonjs_single_file(
        "mod1.js",
        r#"
module.exports.bothBefore = "string";
A.justExport = 4;
A.bothBefore = 2;
A.bothAfter = 3;
module.exports = A;
function A() {
    this.p = 1;
}
module.exports.bothAfter = "string";
module.exports.justProperty = "string";
"#,
    );

    let ts2323: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2323)
        .collect();
    assert_eq!(
        ts2323.len(),
        4,
        "Expected TS2323 on overlapping CommonJS exported property declarations, got: {diagnostics:#?}"
    );
}

#[test]
fn test_commonjs_direct_export_property_overlap_rejects_number_only_js_require_consumers() {
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
module.exports.bothBefore = "string";
A.justExport = 4;
A.bothBefore = 2;
A.bothAfter = 3;
module.exports = A;
function A() {
    this.p = 1;
}
module.exports.bothAfter = "string";
"#,
        "consumer.js",
        r#"
/** @param {number} value */
function takesNumber(value) {}
var mod1 = require("./mod1");
takesNumber(mod1.justExport);
takesNumber(mod1.bothBefore);
takesNumber(mod1.bothAfter);
"#,
        "./mod1",
    );

    let ts2345: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2345)
        .collect();
    assert!(
        ts2345.len() >= 2,
        "Expected JS require() consumer to see overlapping CommonJS exports as non-number-only, got: {diagnostics:#?}"
    );
}

#[test]
fn test_commonjs_object_literal_overlap_rejects_number_only_js_require_consumers() {
    let diagnostics = check_commonjs_two_files(
        "mod1.js",
        r#"
module.exports.bothBefore = "string";
module.exports = {
    justExport: 1,
    bothBefore: 2,
    bothAfter: 3,
};
module.exports.bothAfter = "string";
module.exports.justProperty = "string";
"#,
        "consumer.js",
        r#"
/** @param {number} value */
function takesNumber(value) {}
var mod1 = require("./mod1");
takesNumber(mod1.justExport);
takesNumber(mod1.bothBefore);
takesNumber(mod1.bothAfter);
"#,
        "./mod1",
    );

    let ts2345: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2345)
        .collect();
    assert!(
        ts2345.len() >= 2,
        "Expected object-literal CommonJS overlap to stay union-typed for JS require() consumers, got: {diagnostics:#?}"
    );
}

// --- Mixed patterns: module.exports + exports.foo + prototype ---

#[test]
fn test_full_commonjs_pattern_mix() {
    // All three patterns in one file:
    // 1. Constructor function as module.exports
    // 2. Static property via module.exports.prop
    // 3. Prototype method
    let diagnostics = check_commonjs_two_files(
        "lib.js",
        r#"
function Parser() { this.input = ""; }
Parser.prototype.parse = function(s) { this.input = s; return {}; };
Parser.defaultOptions = { strict: true };
module.exports = Parser;
module.exports.VERSION = "2.0";
"#,
        "consumer.ts",
        r#"
import Parser = require("./lib.js");
var p = new Parser();
"#,
        "./lib.js",
    );

    let ts2351: Vec<_> = diagnostics.iter().filter(|(c, _)| *c == 2351).collect();
    assert!(
        ts2351.is_empty(),
        "Expected no TS2351 for full CommonJS pattern mix, got: {ts2351:#?}"
    );
}
