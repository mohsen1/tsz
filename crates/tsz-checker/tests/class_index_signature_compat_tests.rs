//! Focused coverage for class index signature compatibility in extends checks.

use crate::context::CheckerOptions;
use crate::state::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn compile_and_get_diagnostics(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

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
