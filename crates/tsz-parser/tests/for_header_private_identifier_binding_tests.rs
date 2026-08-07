//! TS18029: a private identifier used as a `for`-header binding.
//!
//! `parse_variable_declaration_with_flags_pre_checks`
//! (`state_variable_declarations.rs`) already reports TS18029 for a plain
//! `let #x = 1;`-style statement, but a `for`-header declaration entry
//! (`for (const #x of arr)`, `for (var #x in obj)`, `for (var #x = 0; ...)`)
//! is parsed by a wholly separate function, `parse_for_variable_declaration_entry`
//! (`state_declarations_exports.rs`) — which did not carry the same check.
//! Before this fix `#x` there silently parsed as an ordinary binding name: no
//! TS18029, no diagnostic at all, `is_identifier_or_keyword()` (true for a
//! `PrivateIdentifier` token, since it sorts after `Identifier` in
//! `SyntaxKind`) let it through as plain identifier text.
//!
//! A `catch (#x) {}` clause was never affected — its own binding-name parse
//! path already goes through `parse_variable_declaration_name`, which has an
//! explicit `PrivateIdentifier` arm alongside the pre-checks call.
//!
//! Every case here is oracle-verified against `typescript@7.0.2`
//! (`scripts/conformance/typescript-versions.json`'s pinned `current`).

use crate::parser::test_fixture::parse_source;

fn codes_and_starts(source: &str) -> Vec<(u32, u32)> {
    let (parser, _root) = parse_source(source);
    let mut diags: Vec<(u32, u32)> = parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.start))
        .collect();
    diags.sort_unstable();
    diags
}

const TS18029: u32 = 18029;

#[test]
fn for_of_const_private_binding_reports_ts18029() {
    // tsc: exactly one TS18029 at the `#x` token.
    let source = "class C {\n  m(arr: number[]) {\n    for (const #x of arr) {}\n  }\n}\n";
    let diags = codes_and_starts(source);
    assert!(
        diags.contains(&(TS18029, 46)),
        "expected TS18029 at the `#x` token, got {diags:?}"
    );
}

#[test]
fn for_in_const_private_binding_reports_ts18029() {
    let source = "class C {\n  m(obj: any) {\n    for (const #x in obj) {}\n  }\n}\n";
    let diags = codes_and_starts(source);
    assert!(
        diags.contains(&(TS18029, 41)),
        "expected TS18029 at the `#x` token, got {diags:?}"
    );
}

#[test]
fn for_of_var_private_binding_reports_ts18029() {
    let source = "class C {\n  m(arr: number[]) {\n    for (var #x of arr) {}\n  }\n}\n";
    let diags = codes_and_starts(source);
    assert!(
        diags.contains(&(TS18029, 44)),
        "expected TS18029 at the `#x` token, got {diags:?}"
    );
}

#[test]
fn for_of_let_private_binding_reports_ts18029() {
    let source = "class C {\n  m(arr: number[]) {\n    for (let #x of arr) {}\n  }\n}\n";
    let diags = codes_and_starts(source);
    assert!(
        diags.contains(&(TS18029, 44)),
        "expected TS18029 at the `#x` token, got {diags:?}"
    );
}

#[test]
fn c_style_for_var_private_binding_reports_ts18029() {
    // C-style `for (var #x = 0; ...)` — a different call path within
    // `parse_for_variable_declaration_entry` from the for-in/for-of head
    // (both share this function; the initializer-vs-condition split happens
    // in the caller), so pinned separately.
    let source = "class C {\n  m() {\n    for (var #x = 0; ; ) {}\n  }\n}\n";
    let diags = codes_and_starts(source);
    assert!(
        diags.contains(&(TS18029, 31)),
        "expected TS18029 at the `#x` token, got {diags:?}"
    );
}

#[test]
fn catch_clause_private_binding_already_reported_ts18029_control() {
    // Control: the catch-clause path was never broken — confirms this test
    // module's expectations are pinned against the right mechanism rather
    // than a change in scanning/token classification.
    let source = "class C {\n  m() {\n    try {} catch (#x) {}\n  }\n}\n";
    let diags = codes_and_starts(source);
    assert!(
        diags.contains(&(TS18029, 36)),
        "expected TS18029 at the `#x` token, got {diags:?}"
    );
}

#[test]
fn for_of_normal_binding_stays_clean_control() {
    // Control: an ordinary identifier binding in the same for-header shape
    // must not regress to spuriously reporting TS18029.
    let source = "class C {\n  m(arr: number[]) {\n    for (const x of arr) {}\n  }\n}\n";
    let diags = codes_and_starts(source);
    assert!(
        !diags.iter().any(|&(code, _)| code == TS18029),
        "ordinary binding must not report TS18029, got {diags:?}"
    );
}
