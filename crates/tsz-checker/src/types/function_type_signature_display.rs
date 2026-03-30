use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    pub(crate) fn binding_name_for_signature_display(
        &self,
        name_idx: NodeIndex,
    ) -> Option<tsz_common::interner::Atom> {
        let text = self.format_binding_name_for_signature(name_idx)?;
        Some(self.ctx.types.intern_string(&text))
    }

    fn format_binding_name_for_signature(&self, node_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(node_idx)?;
        match node.kind {
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                let pattern = self.ctx.arena.get_binding_pattern(node)?;
                let parts: Vec<_> = pattern
                    .elements
                    .nodes
                    .iter()
                    .filter_map(|&elem_idx| {
                        self.format_binding_element_for_signature(elem_idx, true)
                    })
                    .collect();
                Some(if parts.is_empty() {
                    "{}".to_string()
                } else if pattern.elements.has_trailing_comma {
                    format!("{{ {}, }}", parts.join(", "))
                } else {
                    format!("{{ {} }}", parts.join(", "))
                })
            }
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                let pattern = self.ctx.arena.get_binding_pattern(node)?;
                let parts: Vec<_> = pattern
                    .elements
                    .nodes
                    .iter()
                    .filter_map(|&elem_idx| {
                        self.format_binding_element_for_signature(elem_idx, false)
                    })
                    .collect();
                Some(format!("[{}]", parts.join(", ")))
            }
            _ => Some(self.parameter_name_for_error(node_idx)),
        }
    }

    fn format_binding_element_for_signature(
        &self,
        element_idx: NodeIndex,
        is_object_pattern: bool,
    ) -> Option<String> {
        let node = self.ctx.arena.get(element_idx)?;
        let element = self.ctx.arena.get_binding_element(node)?;
        let mut text = String::new();
        if element.dot_dot_dot_token {
            text.push_str("...");
        }

        let name_text = self.format_binding_name_for_signature(element.name)?;
        let property_text = self
            .ctx
            .arena
            .get(element.property_name)
            .map(|_| self.parameter_name_for_error(element.property_name));

        if is_object_pattern && let Some(property_text) = property_text {
            if property_text != name_text {
                text.push_str(&property_text);
                text.push_str(": ");
                text.push_str(&name_text);
            } else {
                text.push_str(&name_text);
            }
        } else {
            text.push_str(&name_text);
        }

        Some(text)
    }
}
