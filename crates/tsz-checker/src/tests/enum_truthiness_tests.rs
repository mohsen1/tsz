use crate::diagnostics::diagnostic_codes;
use crate::test_utils::check_source_strict;

fn diagnostic_count_with_code(source: &str, code: u32) -> usize {
    check_source_strict(source)
        .into_iter()
        .filter(|diagnostic| diagnostic.code == code)
        .count()
}

#[test]
fn enum_member_truthiness_checks_only_direct_member_expressions() {
    let source = r#"
enum First { A = 0, B = 1 }
enum Second { Zero = 0, One = 1 }

if (First.A) {}
if (Second.One) {}

const firstAlias = First.A;
if (firstAlias) {}

let secondAlias: Second.Zero = Second.Zero;
if (secondAlias) {}
"#;

    assert_eq!(
        diagnostic_count_with_code(source, diagnostic_codes::THIS_CONDITION_WILL_ALWAYS_RETURN,),
        2,
        "TS2845 should match direct enum member condition expressions, not enum-member typed aliases",
    );
}
