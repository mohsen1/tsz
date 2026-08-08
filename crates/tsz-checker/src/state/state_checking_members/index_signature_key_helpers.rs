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
        // Materialize the property type before the constraint decision. tsc
        // checks `getTypeOfSymbol(prop)`, which is the *reduced* type, against
        // the index type; the raw annotation union may still carry constituents
        // that a sibling constituent subsumes. In particular a numeric enum is a
        // subtype of `number`, so `E | number` reduces to `number` — but the
        // union arrives here as `Lazy(E) | number`, and union subtype reduction
        // skips lazy members at intern time, so the collapse only happens on
        // evaluation. Judging the unmaterialized union member-by-member makes the
        // absorbed `E` constituent spuriously fail `E → indexType` and emit a
        // false TS2411 (the diagnostic then even renders the *reduced* type,
        // because `format_ts2411_type` evaluates while this check did not). This
        // is the apparent-type materialize-before-decide gateway (#15396):
        // evaluate first so the decision and the rendered type agree, and match
        // tsc's reduced operand.
        let prop_type = self.evaluate_type_with_env(prop_type);
        if let Some(list_id) = crate::query_boundaries::common::union_list_id(
            self.ctx.types,
            self.resolve_lazy_type(prop_type),
        ) {
            let members: Vec<TypeId> = self.ctx.types.type_list(list_id).to_vec();
            return members.into_iter().all(|member| {
                self.index_signature_relation_outcome(member, index_value_type)
                    .related
            });
        }

        self.index_signature_relation_outcome(prop_type, index_value_type)
            .related
    }

    pub(crate) fn format_ts2411_type(&mut self, type_id: TypeId) -> String {
        let type_queries = self.ctx.collect_type_queries_cached(type_id);
        let mut replacements = Vec::new();
        for symbol_ref in type_queries.iter().copied() {
            let sym_id =
                crate::query_boundaries::definition_identity::symbol_ref_to_symbol_id(symbol_ref);
            let value_type = self.get_type_of_symbol(sym_id);
            if value_type != TypeId::ANY && value_type != TypeId::ERROR {
                // Route through the env-write authority (dual-write + defer on
                // borrow race instead of silently skipping; #14348).
                self.ctx
                    .register_symbol_type_in_envs(symbol_ref, value_type, Vec::new());
            }
            if value_type != TypeId::ANY
                && value_type != TypeId::ERROR
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            {
                let mut value_display = self.format_type(value_type);
                if self.ctx.resolve_type_to_symbol_id(value_type) == Some(sym_id) {
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

    /// Does a property named `prop_name` fall under a `string`-slot index
    /// signature whose key type is `key_type`?
    ///
    /// A plain `string` (or `any`) key matches every string-named property. A
    /// template-literal *pattern* key (`[k: `id_${number}`]`) is stored in the
    /// same `string_index` slot, but only constrains property names assignable
    /// to the pattern — mirroring tsc, where a property participates in an index
    /// signature's `TS2411` check only when its name type is assignable to the
    /// index's key type. Without this gate a pattern index wrongly constrains
    /// every string-named property (a `TS2411` false positive on `size` for
    /// `interface D { [k: `id_${number}`]: string; size: number }`).
    pub(crate) fn property_name_matches_index_key(
        &mut self,
        prop_name: &str,
        key_type: TypeId,
    ) -> bool {
        if key_type == TypeId::STRING || key_type == TypeId::ANY {
            return true;
        }
        let name_literal = self.ctx.types.literal_string(prop_name);
        self.index_signature_relation_outcome(name_literal, key_type)
            .related
    }

    /// Display text for the *kind* of a `string`-slot index signature key in a
    /// `TS2411` message: `string` for a plain string key, otherwise the key
    /// type's own rendering (e.g. the template-literal pattern `` `id_${number}` ``,
    /// which tsc shows in place of `string`).
    pub(crate) fn index_signature_key_display(&mut self, key_type: TypeId) -> String {
        if key_type == TypeId::STRING {
            "string".to_string()
        } else {
            self.format_type(key_type)
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
