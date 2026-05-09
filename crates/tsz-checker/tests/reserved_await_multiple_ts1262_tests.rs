// Tests for issue #2816: top-level reserved `await` must report TS1262 at
// every illegal declaration in an external module, not just the first one.

use tsz_checker::test_utils::check_source_codes;

/// tsc reports TS1262 at every top-level `await` binding, not just the first.
#[test]
fn multiple_top_level_await_decls_each_get_ts1262() {
    let codes = check_source_codes(
        r#"
export {};

const await = 1;
let await = 2;
var await = 3;
"#,
    );

    let ts1262_count = codes.iter().filter(|&&c| c == 1262).count();
    assert_eq!(
        ts1262_count, 3,
        "expected TS1262 for each of the three top-level `await` declarations, got codes: {codes:?}"
    );
}

/// A single top-level `await` declaration still produces exactly one TS1262.
#[test]
fn single_top_level_await_decl_gets_one_ts1262() {
    let codes = check_source_codes(
        r#"
export {};

const await = 1;
"#,
    );

    let ts1262_count = codes.iter().filter(|&&c| c == 1262).count();
    assert_eq!(
        ts1262_count, 1,
        "expected exactly one TS1262 for a single top-level `await` binding, got codes: {codes:?}"
    );
}
