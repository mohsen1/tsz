//! TS18016 for a private-identifier property/method/accessor name in an
//! object literal inside a JS file.
//!
//! Structural rule: `tsc` raises `checkGrammarPrivateIdentifierExpression`'s
//! object-literal case via `grammarErrorOnNode` — a checker-side grammar
//! check that fires for JS files exactly like the rest of the `TS8xxx`
//! family (parse-driven, independent of `checkJs`). tsz's parser already has
//! an equivalent check
//! (`crates/tsz-parser/src/parser/state_expressions_literals/object_members.rs`)
//! that correctly reports this for `.ts` files, but the CLI driver filters
//! every JS file's *parser* diagnostics through `is_ts1xxx_allowed_in_js`
//! (`crates/tsz-cli/src/driver/check_utils.rs`), which does not list
//! TS18016 — so the parser's copy was silently dropped for `.js` files. The
//! checker's own JS-grammar pass (`check_js_grammar_statements`, gated only
//! on `is_js_file()`, not `checkJs`) is not subject to that filter, so this
//! adds the same object-literal check there.

use tsz_checker::context::{CheckerOptions, ScriptTarget};
use tsz_checker::test_utils::check_source;

const TS18016: u32 = 18016;

fn codes(source: &str) -> Vec<u32> {
    check_source(
        source,
        "a.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    )
    .iter()
    .map(|d| d.code)
    .collect()
}

#[test]
fn object_literal_private_property_reports_ts18016_in_js() {
    let diags = codes("const obj = {\n  #x: 1,\n};\n");
    assert_eq!(
        diags.iter().filter(|&&c| c == TS18016).count(),
        1,
        "expected exactly one TS18016, got: {diags:?}"
    );
}

#[test]
fn object_literal_private_method_reports_ts18016_in_js() {
    let diags = codes("const obj = {\n  #m() {},\n};\n");
    assert_eq!(
        diags.iter().filter(|&&c| c == TS18016).count(),
        1,
        "expected exactly one TS18016, got: {diags:?}"
    );
}

#[test]
fn object_literal_private_get_accessor_reports_ts18016_in_js() {
    let diags = codes("const obj = {\n  get #p() { return 1; },\n};\n");
    assert_eq!(
        diags.iter().filter(|&&c| c == TS18016).count(),
        1,
        "expected exactly one TS18016, got: {diags:?}"
    );
}

#[test]
fn object_literal_private_set_accessor_reports_ts18016_in_js() {
    let diags = codes("const obj = {\n  set #p(v) {},\n};\n");
    assert_eq!(
        diags.iter().filter(|&&c| c == TS18016).count(),
        1,
        "expected exactly one TS18016, got: {diags:?}"
    );
}

/// Multiple private-identifier keys in the same object literal each get
/// their own TS18016 (not deduplicated into one).
#[test]
fn object_literal_multiple_private_names_report_one_ts18016_each() {
    let diags = codes(
        "function A() {}\nA.prototype = {\n  #x: 1,\n  #m() {},\n  get #p() { return \"\"; }\n};\n",
    );
    assert_eq!(
        diags.iter().filter(|&&c| c == TS18016).count(),
        3,
        "expected one TS18016 per private-identifier key, got: {diags:?}"
    );
}

/// Renamed binders (not `x`/`m`/`p`) exercise the same structural rule, not
/// a name-specific special case.
#[test]
fn object_literal_private_property_renamed_binder_reports_ts18016_in_js() {
    let diags = codes("const widget = {\n  #zorp: 42,\n};\n");
    assert_eq!(
        diags.iter().filter(|&&c| c == TS18016).count(),
        1,
        "expected exactly one TS18016, got: {diags:?}"
    );
}

/// Negative control: a legal (non-private) object literal key never reports
/// TS18016.
#[test]
fn object_literal_public_property_stays_clean_in_js() {
    let diags = codes("const obj = {\n  x: 1,\n};\n");
    assert!(
        !diags.contains(&TS18016),
        "did not expect TS18016 for a public key, got: {diags:?}"
    );
}

/// Negative control: a private field/method declared inside an actual class
/// body (not an object literal) is legal and must not report TS18016 —
/// the fix is scoped to object-literal member names, not every private
/// identifier declaration in a JS file.
#[test]
fn class_body_private_member_stays_clean_in_js() {
    let diags = codes("class C {\n  #x = 1;\n  #m() {}\n  get #p() { return this.#x; }\n}\n");
    assert!(
        !diags.contains(&TS18016),
        "did not expect TS18016 for a legal class-body private member, got: {diags:?}"
    );
}

/// Negative control: a `.ts` file keeps reporting TS18016 through the
/// parser's existing check — this fix only restores the missing JS path, it
/// does not change or duplicate the TS-file behavior.
///
/// `check_source` only returns checker-emitted diagnostics (not raw parser
/// diagnostics — see `test_utils.rs`), so the `.ts` parser-owned path can't
/// be exercised through this harness. Verified instead via a direct CLI
/// build against `typescript@6.0.2`: `.ts` still reports exactly one
/// TS18016 for the same source, unchanged by this fix.
#[test]
fn object_literal_private_property_in_ts_is_not_touched_by_js_grammar_pass() {
    let diags: Vec<u32> = check_source(
        "const obj = {\n  #x: 1,\n};\n",
        "a.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    )
    .iter()
    .map(|d| d.code)
    .collect();
    assert!(
        !diags.contains(&TS18016),
        "the checker's JS-only grammar pass must not run for .ts files \
         (is_js_file() gate); got: {diags:?}"
    );
}
