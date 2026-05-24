use crate::flow::flow_flags;
use crate::state::BinderState;
use tsz_parser::parser::ParserState;
use tsz_scanner::SyntaxKind;

fn parse_and_bind(source: &str) -> (BinderState, ParserState) {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);
    (binder, parser)
}

#[test]
fn while_true_post_loop_uses_break_edges_only() {
    let (binder, parser) = parse_and_bind(
        r#"
function f(x: string | number) {
    while (true) {
        if (typeof x === "string") {
            break;
        }
        return;
    }
    x;
}
"#,
    );

    let arena = parser.get_arena();
    let false_true_conditions = binder
        .flow_nodes
        .iter()
        .filter(|flow| flow.has_flags(flow_flags::FALSE_CONDITION))
        .filter(|flow| {
            arena
                .get(flow.node)
                .is_some_and(|node| node.kind == SyntaxKind::TrueKeyword as u16)
        })
        .count();

    assert_eq!(
        false_true_conditions, 0,
        "`while (true)` should not add an impossible false-condition edge to the post-loop flow"
    );
}
