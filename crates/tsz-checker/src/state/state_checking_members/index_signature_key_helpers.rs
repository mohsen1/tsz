//! Helpers for decomposing index-signature key annotations.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn property_type_assignable_to_index_type(
        &mut self,
        prop_type: TypeId,
        index_value_type: TypeId,
    ) -> bool {
        if let Some(list_id) = crate::query_boundaries::common::union_list_id(
            self.ctx.types,
            self.resolve_lazy_type(prop_type),
        ) {
            let members: Vec<TypeId> = self.ctx.types.type_list(list_id).to_vec();
            return members
                .into_iter()
                .all(|member| self.is_assignable_to(member, index_value_type));
        }

        self.is_assignable_to(prop_type, index_value_type)
    }

    pub(crate) fn format_ts2411_type(&mut self, type_id: TypeId) -> String {
        let type_queries =
            crate::query_boundaries::common::collect_type_queries(self.ctx.types, type_id);
        let mut replacements = Vec::new();
        for symbol_ref in type_queries {
            let sym_id = tsz_binder::SymbolId(symbol_ref.0);
            let value_type = self.get_type_of_symbol(sym_id);
            if value_type != TypeId::ANY
                && value_type != TypeId::ERROR
                && let Ok(mut env) = self.ctx.type_env.try_borrow_mut()
            {
                env.insert(symbol_ref, value_type);
            }
            if value_type != TypeId::ANY
                && value_type != TypeId::ERROR
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            {
                let mut value_display = self.format_type(value_type);
                if value_display == symbol.escaped_name {
                    let constructor_name = format!("{}Constructor", symbol.escaped_name);
                    let has_constructor_symbol =
                        self.ctx.binder.file_locals.get(&constructor_name).is_some()
                            || self.ctx.lib_contexts.iter().any(|lib_ctx| {
                                lib_ctx.binder.file_locals.get(&constructor_name).is_some()
                            });
                    if has_constructor_symbol {
                        value_display = constructor_name;
                    }
                }
                replacements.push((format!("typeof {}", symbol.escaped_name), value_display));
            }
        }
        let evaluated = self.evaluate_type_with_env(type_id);
        let resolved = self.resolve_type_query_type(evaluated);
        let mut formatted = self.format_type(resolved);
        for (from, to) in replacements {
            if from != to {
                formatted = formatted.replace(&from, &to);
            }
        }
        formatted
    }

    pub(crate) fn index_signature_key_components(
        &mut self,
        type_annotation_idx: NodeIndex,
    ) -> Vec<TypeId> {
        let Some(type_node) = self.ctx.arena.get(type_annotation_idx) else {
            return Vec::new();
        };
        let type_node_kind = type_node.kind;

        if type_node_kind == syntax_kind_ext::UNION_TYPE {
            let members: Vec<NodeIndex> = self
                .ctx
                .arena
                .get(type_annotation_idx)
                .and_then(|node| self.ctx.arena.get_composite_type(node))
                .map(|composite| composite.types.nodes.to_vec())
                .unwrap_or_default();

            let mut keys = Vec::new();
            for member_idx in members {
                for key in self.index_signature_key_components(member_idx) {
                    if !key.is_error() && key != TypeId::NONE && !keys.contains(&key) {
                        keys.push(key);
                    }
                }
            }
            return keys;
        }

        if type_node_kind == syntax_kind_ext::INTERSECTION_TYPE {
            let generic_or_literal_members: Vec<NodeIndex> = self
                .ctx
                .arena
                .get(type_annotation_idx)
                .and_then(|node| self.ctx.arena.get_composite_type(node))
                .map(|composite| {
                    composite
                        .types
                        .nodes
                        .iter()
                        .copied()
                        .filter(|&member_idx| {
                            self.ctx.arena.get(member_idx).is_some_and(|member_node| {
                                self.is_type_param_or_literal_in_index_sig(
                                    member_node.kind,
                                    member_idx,
                                )
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            if !generic_or_literal_members.is_empty() {
                let mut keys = Vec::new();
                for member_idx in generic_or_literal_members {
                    let key = self.get_type_from_type_node(member_idx);
                    if !key.is_error() && key != TypeId::NONE && !keys.contains(&key) {
                        keys.push(key);
                    }
                }
                return keys;
            }
        }

        let key = self.get_type_from_type_node(type_annotation_idx);
        if key.is_error() || key == TypeId::NONE {
            Vec::new()
        } else {
            vec![key]
        }
    }

    pub(crate) fn report_duplicate_other_index_signatures(
        &mut self,
        entries: &[(TypeId, NodeIndex)],
    ) {
        let mut reported_keys: Vec<TypeId> = Vec::new();
        for &(key_type, _) in entries {
            if reported_keys.contains(&key_type) {
                continue;
            }
            reported_keys.push(key_type);

            let nodes: Vec<NodeIndex> = entries
                .iter()
                .filter_map(|&(entry_key, node_idx)| (entry_key == key_type).then_some(node_idx))
                .collect();
            if nodes.len() <= 1 {
                continue;
            }

            let key_type_str = self.format_type(key_type);
            for node_idx in nodes {
                self.error_at_node_msg(
                    node_idx,
                    crate::diagnostics::diagnostic_codes::DUPLICATE_INDEX_SIGNATURE_FOR_TYPE,
                    &[&key_type_str],
                );
            }
        }
    }

    pub(crate) fn template_pattern_key_is_subset(&self, source: TypeId, target: TypeId) -> bool {
        let Some((source_prefix, source_suffix)) = self.template_pattern_bounds(source) else {
            return false;
        };
        let Some((target_prefix, target_suffix)) = self.template_pattern_bounds(target) else {
            return false;
        };

        source_prefix.starts_with(&target_prefix) && source_suffix.ends_with(&target_suffix)
    }

    fn template_pattern_bounds(&self, type_id: TypeId) -> Option<(String, String)> {
        let template_id = tsz_solver::query::template_literal_id(self.ctx.types, type_id)?;
        let spans = self.ctx.types.template_list(template_id);
        let mut first_type_index = None;
        let mut last_type_index = None;
        for (index, span) in spans.iter().enumerate() {
            if let tsz_solver::TemplateSpan::Type(hole_type) = span {
                if !matches!(*hole_type, TypeId::STRING | TypeId::ANY) {
                    return None;
                }
                first_type_index.get_or_insert(index);
                last_type_index = Some(index);
            }
        }

        let first_type_index = first_type_index?;
        let last_type_index = last_type_index?;

        let prefix = spans[..first_type_index]
            .iter()
            .map(|span| match span {
                tsz_solver::TemplateSpan::Text(atom) => self.ctx.types.resolve_atom(*atom),
                tsz_solver::TemplateSpan::Type(_) => String::new(),
            })
            .collect::<String>();
        let suffix = spans[last_type_index + 1..]
            .iter()
            .map(|span| match span {
                tsz_solver::TemplateSpan::Text(atom) => self.ctx.types.resolve_atom(*atom),
                tsz_solver::TemplateSpan::Type(_) => String::new(),
            })
            .collect::<String>();

        Some((prefix, suffix))
    }
}
