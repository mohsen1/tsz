//! Focused coverage for class index signature compatibility in extends checks.

use crate::test_utils::check_source_code_messages as compile_and_get_diagnostics;

#[test]
fn class_extends_reports_incompatible_string_index_signature() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Base {
  [key: string]: number;
}

class Derived extends Base {
  [key: string]: string;
}
"#,
    );

    let matching: Vec<_> = diagnostics
        .iter()
        .filter(|(code, msg)| {
            *code == 2415 && msg.contains("'string' index signatures are incompatible")
        })
        .collect();

    assert!(
        !matching.is_empty(),
        "Expected TS2415 with string index signature incompatibility, got: {diagnostics:#?}"
    );
}
