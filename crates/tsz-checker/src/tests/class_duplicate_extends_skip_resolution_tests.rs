//! `class C extends A extends B` is a TS1172 parser error (`'extends' clause
//! already seen`). The checker must NOT try to resolve type names inside the
//! duplicate extends clause -- otherwise the names cascade into spurious
//! TS2304 ("Cannot find name") on top of the parser error. tsc only reports
//! TS2304 for `A` (the first extends operand) and TS1172 for the second
//! `extends` keyword; `B` is not surfaced.

use crate::test_utils::check_source_diagnostics;

#[test]
fn duplicate_extends_keeps_only_first_unresolved_name() {
    let diags = check_source_diagnostics(
        r#"
class C extends A extends B {}
"#,
    );

    let ts2304_a: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2304 && d.message_text.contains("'A'"))
        .collect();
    let ts2304_b: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2304 && d.message_text.contains("'B'"))
        .collect();

    assert_eq!(
        ts2304_a.len(),
        1,
        "Expected one TS2304 for 'A'; got: {diags:?}"
    );
    assert!(
        ts2304_b.is_empty(),
        "Expected NO TS2304 for 'B' (duplicate-extends operand); got: {diags:?}"
    );
}
