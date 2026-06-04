//! Helpers for type-alias body validation that stay off the hot lowering path.

use crate::state::CheckerState;
use crate::state_type_analysis::cross_file_direct::is_builtin_lib_declaration_arena;
use std::hash::{Hash, Hasher};
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};

impl<'a> CheckerState<'a> {
    pub(crate) fn active_resolving_alias_set_key(&self) -> u64 {
        if self.ctx.symbol_resolution_set.is_empty() {
            return 0;
        }
        if self.ctx.symbol_resolution_set.len() == 1 {
            let Some(&sym_id) = self.ctx.symbol_resolution_set.iter().next() else {
                return 0;
            };
            return self
                .ctx
                .get_existing_def_id(sym_id)
                .map_or(u64::from(sym_id.0), |def_id| {
                    (1_u64 << 32) | u64::from(def_id.0)
                });
        }

        let mut entries = self
            .ctx
            .symbol_resolution_set
            .iter()
            .map(|&sym_id| {
                self.ctx
                    .get_existing_def_id(sym_id)
                    .map_or(u64::from(sym_id.0), |def_id| {
                        (1_u64 << 32) | u64::from(def_id.0)
                    })
            })
            .collect::<Vec<_>>();
        entries.sort_unstable();

        let mut hasher = rustc_hash::FxHasher::default();
        entries.hash(&mut hasher);
        hasher.finish()
    }

    /// Validate the diagnostics not covered by type-literal construction and
    /// return whether the normal alias-body validation walk can be skipped.
    pub(crate) fn validate_signature_only_type_literal_alias_body(
        &mut self,
        type_node_idx: NodeIndex,
    ) -> bool {
        let Some(type_node) = self.ctx.arena.get(type_node_idx) else {
            return false;
        };
        if type_node.kind != syntax_kind_ext::TYPE_LITERAL {
            return false;
        }
        let Some(type_lit) = self.ctx.arena.get_type_literal(type_node) else {
            return false;
        };
        if type_lit.members.nodes.is_empty() {
            return false;
        }

        let members = type_lit.members.nodes.clone();
        if !members.iter().copied().all(|member_idx| {
            self.ctx.arena.get(member_idx).is_some_and(|member| {
                member.kind == syntax_kind_ext::CALL_SIGNATURE
                    || member.kind == syntax_kind_ext::CONSTRUCT_SIGNATURE
            })
        }) {
            return false;
        }

        for member_idx in members {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            let Some(signature) = self.ctx.arena.get_signature(member_node) else {
                continue;
            };
            if let Some(parameters) = &signature.parameters {
                let (_type_params, type_param_updates) =
                    self.push_type_parameters(&signature.type_parameters);
                self.check_rest_parameter_types(&parameters.nodes);
                self.pop_type_parameters(type_param_updates);
            }
        }

        true
    }

    pub(crate) fn check_explicit_type_reference_for_alias_body_validation(
        &mut self,
        ref_idx: NodeIndex,
    ) -> bool {
        if self.is_inside_type_parameter_declaration(ref_idx)
            || !self.type_reference_is_in_type_alias_body(ref_idx)
            || is_builtin_lib_declaration_arena(self.ctx.arena)
        {
            return false;
        }
        let Some(node) = self.ctx.arena.get(ref_idx) else {
            return false;
        };
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return false;
        };
        let Some(args) = type_ref
            .type_arguments
            .clone()
            .filter(|args| !args.nodes.is_empty())
        else {
            return false;
        };
        let Some(raw) = self.resolve_type_symbol_for_lowering(type_ref.type_name) else {
            return false;
        };

        self.validate_type_reference_type_arguments(tsz_binder::SymbolId(raw), &args, ref_idx);
        true
    }

    pub(crate) fn type_ref_is_bare_scoped_type_parameter(
        &self,
        type_name: NodeIndex,
        type_arguments: Option<&tsz_parser::parser::base::NodeList>,
    ) -> bool {
        if type_arguments.is_some_and(|args| !args.nodes.is_empty()) {
            return false;
        }
        let Some(name_node) = self.ctx.arena.get(type_name) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        self.ctx
            .type_parameter_scope
            .contains_key(ident.escaped_text.as_str())
    }

    fn type_reference_is_in_type_alias_body(&self, ref_idx: NodeIndex) -> bool {
        let mut current = ref_idx;
        while current.is_some() {
            let Some(parent_idx) = self
                .ctx
                .arena
                .get_extended(current)
                .map(|extended| extended.parent)
            else {
                return false;
            };
            if parent_idx.is_none() {
                return false;
            }
            let Some(parent) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            if parent.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                return true;
            }
            current = parent_idx;
        }
        false
    }
}
