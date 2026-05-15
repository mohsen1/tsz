//! Cache keys for repeated optional property chains.

use crate::context::TypingRequest;
use crate::query_boundaries::common::OptionalPropertyChainKey;
use crate::state::CheckerState;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn optional_property_chain_cache_key(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> Option<OptionalPropertyChainKey> {
        if *request != TypingRequest::NONE
            || self.is_js_file()
            || self
                .ctx
                .compiler_options
                .no_property_access_from_index_signature
            || !self.should_skip_property_result_flow_narrowing_for_result(idx)
        {
            return None;
        }

        let mut current = idx;
        let mut reversed_properties: Vec<(Atom, bool)> = Vec::with_capacity(6);
        let mut saw_optional = false;
        let root = loop {
            if reversed_properties.len() >= u64::BITS as usize {
                return None;
            }

            let node = self.ctx.arena.get(current)?;
            if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                return None;
            }
            let access = self.ctx.arena.get_access_expr(node)?;
            let prop_atom = self.property_access_name_atom(access.name_or_argument)?;
            saw_optional |= access.question_dot_token;
            reversed_properties.push((prop_atom, access.question_dot_token));

            let expression_node = self.ctx.arena.get(access.expression)?;
            if expression_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                current = access.expression;
                continue;
            }

            break access.expression;
        };

        if !saw_optional {
            return None;
        }

        let root_node = self.ctx.arena.get(root)?;
        if root_node.kind != SyntaxKind::Identifier as u16
            || self
                .resolve_identifier_symbol_without_tracking(root)
                .is_none()
        {
            return None;
        }

        let root_type = self.get_type_of_write_target_base_expression(root);
        if matches!(root_type, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR) {
            return None;
        }

        let mut properties = Vec::with_capacity(reversed_properties.len());
        let mut optional_mask = 0u64;
        for (position, (atom, optional)) in reversed_properties.into_iter().rev().enumerate() {
            properties.push(atom);
            if optional {
                optional_mask |= 1u64 << position;
            }
        }

        Some(OptionalPropertyChainKey {
            root_type,
            properties,
            optional_mask,
            no_unchecked_indexed_access: self.ctx.compiler_options.no_unchecked_indexed_access,
        })
    }

    fn property_access_name_atom(&mut self, name_idx: NodeIndex) -> Option<Atom> {
        let ident = self.ctx.arena.get_identifier_at(name_idx)?;
        if ident.atom != Atom::none() {
            return Some(ident.atom);
        }
        let property_name = ident.escaped_text.clone();
        Some(self.ctx.types.intern_string(&property_name))
    }
}
