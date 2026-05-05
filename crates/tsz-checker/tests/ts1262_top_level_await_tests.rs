//! TS1262: "Identifier expected. 'await' is a reserved word at the top-level of a module."
//!
//! tsc emits TS1262 at *every* top-level `await`-named declaration in an external
//! module, not just the first one.  A previous bug caused tsz to break out of the
//! declaration loop after the first hit, and the text-scan fallback bailed out early
//! once any TS1262 existed in diagnostics.  Both shortcuts together suppressed every
//! diagnostic beyond the first.
//!
//! Regression test for GitHub issue #2816.

use tsz_checker::test_utils::check_source_codes;

/// Multiple top-level `await` declarations in the same external module must each
/// produce a TS1262, regardless of how many declarations precede them.
#[test]
fn ts1262_emitted_for_every_top_level_await_declaration() {
    // `export {}` makes this an external module so TS1262 applies.
    let codes = check_source_codes(
        r#"
export {};

const await = 1;
let await = 2;
var await = 3;
"#,
    );

    let count_1262 = codes.iter().filter(|&&c| c == 1262).count();
    assert_eq!(
        count_1262, 3,
        "expected TS1262 for each of the three top-level `await` declarations; got codes: {codes:?}"
    );
}

/// A single top-level `await` declaration must still produce exactly one TS1262.
/// This guards against accidentally emitting zero or more than one.
#[test]
fn ts1262_emitted_once_for_single_top_level_await() {
    let codes = check_source_codes(
        r#"
export {};

const await = 1;
"#,
    );

    let count_1262 = codes.iter().filter(|&&c| c == 1262).count();
    assert_eq!(
        count_1262, 1,
        "expected exactly one TS1262 for a single top-level `await` declaration; got codes: {codes:?}"
    );
}

/// Variable named `await` in a non-module script (no export/import) must NOT get TS1262,
/// since the restriction only applies inside external modules.
#[test]
fn ts1262_not_emitted_in_script_scope() {
    let codes = check_source_codes(
        r#"
const await = 1;
"#,
    );

    assert!(
        !codes.contains(&1262),
        "TS1262 must not be emitted for `await` in a script-scope file; got codes: {codes:?}"
    );
}
