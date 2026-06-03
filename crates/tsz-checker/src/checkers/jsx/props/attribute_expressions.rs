use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

impl<'a> CheckerState<'a> {
    pub(in crate::checkers_domain::jsx) fn evaluate_jsx_attribute_expressions(
        &mut self,
        attributes_idx: NodeIndex,
    ) {
        let Some(attrs_node) = self.ctx.arena.get(attributes_idx) else {
            return;
        };
        let Some(attrs) = self.ctx.arena.get_jsx_attributes(attrs_node) else {
            return;
        };
        let attr_nodes = attrs.properties.nodes.clone();
        for attr_idx in attr_nodes {
            let Some(attr_node) = self.ctx.arena.get(attr_idx) else {
                continue;
            };
            if attr_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                if let Some(spread_data) = self.ctx.arena.get_jsx_spread_attribute(attr_node) {
                    self.compute_type_of_node(spread_data.expression);
                }
            } else if attr_node.kind == syntax_kind_ext::JSX_ATTRIBUTE
                && let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node)
                && attr_data.initializer.is_some()
            {
                self.compute_type_of_node(attr_data.initializer);
            }
        }
    }
}
