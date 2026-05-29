impl<'a> CheckerState<'a> {
    fn collect_template_literal_infer_type_param_names(
        arena: &NodeArena,
        root: NodeIndex,
        names: &mut Vec<String>,
    ) {
        let mut stack = vec![root];
        while let Some(idx) = stack.pop() {
            let Some(node) = arena.get(idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::TEMPLATE_LITERAL_TYPE
                && let Some(template) = arena.get_template_literal_type(node)
            {
                let mut previous_literal_consumes =
                    Self::template_literal_segment_consumes(arena, template.head);
                for span_idx in template.template_spans.nodes.iter().copied() {
                    let Some(span_node) = arena.get(span_idx) else {
                        continue;
                    };
                    let Some(span) = arena.get_template_span(span_node) else {
                        continue;
                    };
                    let next_literal_consumes =
                        Self::template_literal_segment_consumes(arena, span.literal);
                    if previous_literal_consumes || next_literal_consumes {
                        Self::collect_infer_type_param_names(arena, span.expression, names);
                    }
                    previous_literal_consumes = next_literal_consumes;
                }
            }
            stack.extend(arena.get_children(idx));
        }
    }

    fn template_literal_segment_consumes(arena: &NodeArena, node_idx: NodeIndex) -> bool {
        arena
            .get(node_idx)
            .and_then(|node| arena.get_literal(node))
            .is_some_and(|literal| !literal.text.is_empty())
    }
}
