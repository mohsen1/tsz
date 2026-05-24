use crate::flow::{FlowNodeId, flow_flags};
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

#[test]
fn conditionless_for_post_loop_uses_break_edges_only() {
    let (binder, _parser) = parse_and_bind(
        r#"
function f(x: string | number) {
    for (;;) {
        if (typeof x === "string") {
            break;
        }
        return;
    }
    x;
}
"#,
    );

    let loop_labels: Vec<_> = (0..binder.flow_nodes.len())
        .map(|idx| FlowNodeId(idx as u32))
        .filter(|&flow_id| {
            binder
                .flow_nodes
                .get(flow_id)
                .is_some_and(|flow| flow.has_any_flags(flow_flags::LOOP_LABEL))
        })
        .collect();
    assert!(
        !loop_labels.is_empty(),
        "conditionless for loop should create a loop label"
    );

    let mut saw_break_edge = false;
    for branch_id in (0..binder.flow_nodes.len()).map(|idx| FlowNodeId(idx as u32)) {
        let Some(branch) = binder.flow_nodes.get(branch_id) else {
            continue;
        };
        if !branch.has_any_flags(flow_flags::BRANCH_LABEL) {
            continue;
        }

        assert!(
            !loop_labels
                .iter()
                .any(|loop_label| branch.antecedent.contains(loop_label)),
            "conditionless for post-loop merge must not add the loop label as synthetic fallthrough"
        );

        saw_break_edge |= branch.antecedent.iter().any(|&antecedent| {
            binder.flow_nodes.get(antecedent).is_some_and(|flow| {
                flow.has_any_flags(flow_flags::TRUE_CONDITION) && flow.node.is_some()
            })
        });
    }

    assert!(
        saw_break_edge,
        "conditionless for post-loop merge should be reachable through the conditional break edge"
    );
}
