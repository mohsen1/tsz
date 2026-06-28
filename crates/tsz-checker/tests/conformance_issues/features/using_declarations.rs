//! Ambient-initializer parity for `using` declarations (checker phase).
//!
//! A `using` / `await using` declaration is `const`-like, so the ambient
//! const-initializer restriction (TS1254) applies — not the generic TS1039.
//! Plain `using` does not carry the `CONST` flag, so it previously mis-routed
//! to TS1039; this locks in the const-like routing.
//!
//! (The companion TS1491 "modifier cannot appear on a 'using' declaration"
//! grammar error is parser-emitted and covered by the `tsz-parser` tests; this
//! harness returns checker-phase diagnostics only.)

use super::super::core::*;

fn codes(diagnostics: &[(u32, String)]) -> Vec<u32> {
    diagnostics.iter().map(|(code, _)| *code).collect()
}

#[test]
fn ambient_using_with_non_literal_initializer_reports_ts1254_not_ts1039() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare const d: number;
declare using f = d;
"#,
    );
    let codes = codes(&diagnostics);
    assert!(
        codes.contains(&1254),
        "expected TS1254 (const-like ambient initializer) for `declare using f = d`, got {diagnostics:#?}"
    );
    assert!(
        !codes.contains(&1039),
        "must not emit the generic TS1039 for a const-like `using` declaration, got {diagnostics:#?}"
    );
}

#[test]
fn ambient_await_using_with_non_literal_initializer_reports_ts1254() {
    // `await using` already carried the CONST bit; assert parity holds for it too.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare const d: number;
declare await using f = d;
"#,
    );
    let codes = codes(&diagnostics);
    assert!(
        codes.contains(&1254) && !codes.contains(&1039),
        "expected TS1254 (not TS1039) for `declare await using f = d`, got {diagnostics:#?}"
    );
}

#[test]
fn ambient_using_with_literal_initializer_does_not_report_ts1254_or_ts1039() {
    // A literal initializer is a valid ambient const-like initializer.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare using g = 1;
"#,
    );
    let codes = codes(&diagnostics);
    assert!(
        !codes.contains(&1254) && !codes.contains(&1039),
        "literal ambient initializer must not report TS1254/TS1039, got {diagnostics:#?}"
    );
}

#[test]
fn non_ambient_using_does_not_report_ambient_initializer_error() {
    // Outside an ambient context there is no initializer restriction at all.
    let diagnostics = compile_and_get_diagnostics(
        r#"
declare const d: number;
using a = d;
"#,
    );
    let codes = codes(&diagnostics);
    assert!(
        !codes.contains(&1254) && !codes.contains(&1039),
        "non-ambient `using` must not report TS1254/TS1039, got {diagnostics:#?}"
    );
}
