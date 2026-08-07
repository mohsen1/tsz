//! Contextual object-literal index-signature diagnostic display helpers.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn contextual_computed_index_key_kind(
        &mut self,
        name_idx: NodeIndex,
        target_shape: Option<&tsz_solver::ObjectShape>,
    ) -> Option<&'static str> {
        let shape = target_shape?;
        let name_node = self.ctx.arena.get(name_idx)?;
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }
        let computed = self.ctx.arena.get_computed_property(name_node)?;
        let key_type = self.get_type_of_node(computed.expression);
        if crate::query_boundaries::common::is_symbol_or_unique_symbol(self.ctx.types, key_type) {
            return None;
        }
        let key_type =
            crate::query_boundaries::common::widen_literal_to_primitive(self.ctx.types, key_type);
        if key_type == TypeId::NUMBER && shape.number_index.is_some() {
            return Some("number");
        }
        if (key_type == TypeId::STRING || key_type == TypeId::ANY) && shape.string_index.is_some() {
            return Some("string");
        }
        None
    }

    /// `tsc`'s printer can only re-spell a computed key it can name as an
    /// entity — a plain identifier or a dotted `a.b.c` chain of identifiers
    /// (`ts.isEntityNameExpression`). For any other expression (a binary
    /// operation, a call, a template literal, ...) it falls back to the
    /// synthesized `[x: kind]: V` index-signature form instead, and doing so
    /// for even ONE member of a homogeneous wide-key group folds every sibling
    /// in that group into the same synthesized clause, entity-named or not.
    /// Oracle-verified against `typescript@7.0.2`: `[ws]`/`[box.key]` keep
    /// their own spelling; `[""+"foo"]`/`[ws.toUpperCase()]`/`` [`${ws}`] ``
    /// do not, even alone.
    pub(crate) fn computed_key_is_entity_name_reference(&self, name_idx: NodeIndex) -> bool {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }
        let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
            return false;
        };
        crate::symbols_domain::name_text::expression_name_text_in_arena(
            self.ctx.arena,
            computed.expression,
        )
        .is_some()
    }

    pub(crate) fn contextual_index_signature_source_display(
        &mut self,
        all_contextual_index_properties: bool,
        contextual_index_key_kind: Option<&'static str>,
        contextual_index_value_types: Vec<TypeId>,
    ) -> Option<String> {
        if !all_contextual_index_properties || contextual_index_value_types.is_empty() {
            return None;
        }
        let key_kind = contextual_index_key_kind?;
        let value_type = crate::query_boundaries::diagnostics::source_display_union_type(
            self.ctx.types,
            contextual_index_value_types,
        );
        let value_display = self.format_type_for_assignability_message(value_type);
        Some(format!("{{ [x: {key_kind}]: {value_display}; }}"))
    }
}
