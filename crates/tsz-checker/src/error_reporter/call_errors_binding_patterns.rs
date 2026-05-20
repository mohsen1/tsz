use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    pub(super) fn binding_pattern_parameter_type_display(
        &mut self,
        pattern_idx: NodeIndex,
    ) -> Option<String> {
        let node = self.ctx.arena.get(pattern_idx)?;
        match node.kind {
            k if k == tsz_parser::parser::syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                let pattern = self.ctx.arena.get_binding_pattern(node)?;
                let parts: Vec<_> = pattern
                    .elements
                    .nodes
                    .iter()
                    .filter_map(|&elem_idx| self.binding_pattern_object_property_display(elem_idx))
                    .collect();
                Some(if parts.is_empty() {
                    "{}".to_string()
                } else {
                    format!("{{ {} }}", parts.join(" "))
                })
            }
            k if k == tsz_parser::parser::syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                let pattern = self.ctx.arena.get_binding_pattern(node)?;
                let parts: Vec<_> = pattern
                    .elements
                    .nodes
                    .iter()
                    .filter_map(|&elem_idx| self.binding_pattern_array_element_display(elem_idx))
                    .collect();
                Some(format!("[{}]", parts.join(", ")))
            }
            _ => None,
        }
    }

    fn binding_pattern_object_property_display(
        &mut self,
        element_idx: NodeIndex,
    ) -> Option<String> {
        let node = self.ctx.arena.get(element_idx)?;
        let element = self.ctx.arena.get_binding_element(node)?;
        let property_name = self
            .ctx
            .arena
            .get(element.property_name)
            .map(|_| self.parameter_name_for_error(element.property_name))
            .unwrap_or_else(|| self.parameter_name_for_error(element.name));
        let property_type = self.binding_pattern_property_type_display(
            element.name,
            element.initializer.is_some().then_some(element.initializer),
        )?;
        Some(format!(
            "{}{}: {};",
            property_name,
            if element.initializer.is_some() {
                "?"
            } else {
                ""
            },
            property_type
        ))
    }

    fn binding_pattern_array_element_display(&mut self, element_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(element_idx)?;
        let element = self.ctx.arena.get_binding_element(node)?;
        self.binding_pattern_property_type_display(
            element.name,
            element.initializer.is_some().then_some(element.initializer),
        )
    }

    fn binding_pattern_property_type_display(
        &mut self,
        name_idx: NodeIndex,
        initializer_idx: Option<NodeIndex>,
    ) -> Option<String> {
        let node = self.ctx.arena.get(name_idx)?;
        match node.kind {
            k if k == tsz_parser::parser::syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == tsz_parser::parser::syntax_kind_ext::ARRAY_BINDING_PATTERN =>
            {
                self.binding_pattern_parameter_type_display(name_idx)
            }
            _ => {
                let type_id = if let Some(initializer_idx) = initializer_idx {
                    let initializer_type = crate::query_boundaries::common::widen_type(
                        self.ctx.types,
                        self.get_type_of_node(initializer_idx),
                    );
                    crate::query_boundaries::common::union_with_undefined(
                        self.ctx.types,
                        initializer_type,
                    )
                } else {
                    self.get_type_of_node(name_idx)
                };
                Some(self.format_type_for_assignability_message(type_id))
            }
        }
    }
}
