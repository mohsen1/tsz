use crate::context::TypingRequest;
use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Format a single property fragment "name: type" used inside the synthesized
    /// JSX-attributes source-type display. Mirrors tsc's per-property display:
    /// shorthand attrs render as `name: true`, others use the formatted value type.
    pub(super) fn format_jsx_synthesized_prop_fragment(
        &mut self,
        name: &str,
        type_id: TypeId,
    ) -> String {
        let display_name = {
            let mut chars = name.chars();
            let is_ident = chars.next().is_some_and(|first| {
                (first == '_' || first == '$' || first.is_ascii_alphabetic())
                    && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
            });
            if is_ident {
                name.to_string()
            } else {
                format!("\"{name}\"")
            }
        };
        let type_str = if type_id == TypeId::BOOLEAN_TRUE {
            "true".to_string()
        } else {
            self.format_type(type_id)
        };
        format!("{display_name}: {type_str}")
    }

    /// Walks the attributes once and produces a formatted object-type string with
    /// explicit (non-spread) attrs first (in source order), then spread-derived
    /// props that aren't shadowed by an explicit attr (in spread source order).
    /// This matches tsc's display for elements like `<X {...{p: v}} q />` where
    /// the printed source type is `{ q: true; p: v; }`.
    pub(super) fn format_jsx_attrs_synthesized_source_for_excess(
        &mut self,
        attributes_idx: NodeIndex,
        props_type: TypeId,
        request: &TypingRequest,
    ) -> Option<String> {
        let attrs_node = self.ctx.arena.get(attributes_idx)?;
        let attrs = self.ctx.arena.get_jsx_attributes(attrs_node)?;

        let mut explicit: Vec<(String, TypeId)> = Vec::new();
        let mut spread_props: Vec<(String, TypeId)> = Vec::new();

        for &attr_idx in &attrs.properties.nodes {
            let Some(attr_node) = self.ctx.arena.get(attr_idx) else {
                continue;
            };

            if attr_node.kind == syntax_kind_ext::JSX_ATTRIBUTE {
                let Some(attr_data) = self.ctx.arena.get_jsx_attribute(attr_node) else {
                    continue;
                };
                let Some(name_node) = self.ctx.arena.get(attr_data.name) else {
                    continue;
                };
                let Some(attr_name) = self.get_jsx_attribute_name(name_node) else {
                    continue;
                };
                if attr_name == "key" || attr_name == "ref" {
                    continue;
                }

                let attr_value_type = if attr_data.initializer.is_none() {
                    TypeId::BOOLEAN_TRUE
                } else {
                    self.compute_jsx_attr_value_type_without_context(attr_data.initializer)
                };

                if let Some(existing) = explicit.iter_mut().find(|(n, _)| n == &attr_name) {
                    existing.1 = attr_value_type;
                } else {
                    explicit.push((attr_name, attr_value_type));
                }
            } else if attr_node.kind == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE {
                let Some(spread_data) = self.ctx.arena.get_jsx_spread_attribute(attr_node) else {
                    continue;
                };
                let spread_request = request.read().normal_origin().contextual(props_type);
                let spread_type = self.compute_normalized_jsx_spread_type_with_request(
                    spread_data.expression,
                    &spread_request,
                );
                if matches!(spread_type, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN) {
                    continue;
                }
                if let Some(shape) = crate::query_boundaries::common::object_shape_for_type(
                    self.ctx.types,
                    spread_type,
                ) {
                    // shape.properties is sorted by atom for canonical interning;
                    // walk in declaration order so the synthesized source-type
                    // mirrors tsc's display, which preserves source order.
                    let mut props_by_decl: Vec<&tsz_solver::PropertyInfo> =
                        shape.properties.iter().collect();
                    props_by_decl.sort_by_key(|p| p.declaration_order);
                    for prop in props_by_decl {
                        let name = self.ctx.types.resolve_atom(prop.name).to_string();
                        if name == "key" || name == "ref" {
                            continue;
                        }
                        if let Some(existing) = spread_props.iter_mut().find(|(n, _)| *n == name) {
                            existing.1 = prop.type_id;
                        } else {
                            spread_props.push((name, prop.type_id));
                        }
                    }
                }
            }
        }

        if explicit.is_empty() && spread_props.is_empty() {
            return None;
        }

        let explicit_names: rustc_hash::FxHashSet<String> =
            explicit.iter().map(|(n, _)| n.clone()).collect();
        let mut fragments: Vec<String> = Vec::with_capacity(explicit.len() + spread_props.len());
        for (name, type_id) in &explicit {
            fragments.push(self.format_jsx_synthesized_prop_fragment(name, *type_id));
        }
        for (name, type_id) in &spread_props {
            if explicit_names.contains(name) {
                continue;
            }
            fragments.push(self.format_jsx_synthesized_prop_fragment(name, *type_id));
        }

        if fragments.is_empty() {
            return None;
        }
        Some(format!("{{ {}; }}", fragments.join("; ")))
    }
}
