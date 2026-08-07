//! Contextual object-literal index-signature diagnostic display helpers.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

/// One rendered computed-name method/accessor member that folded into an
/// index-signature bucket, plus the facts the caller's wide-key merge
/// bookkeeping needs (see `object_literal_source_type_display`).
pub(in crate::error_reporter) struct ComputedIndexMemberDisplay {
    /// Property-style rendering, e.g. `[ws]: () => number`.
    pub rendered: String,
    /// Index kind (`"string"`/`"number"`) when the key folds into the
    /// target's matching index signature; `None` for `symbol` keys and
    /// targets without one.
    pub computed_index_kind: Option<&'static str>,
    /// Display-widened member value type, for the merged-clause value union.
    pub widened_value: TypeId,
    /// Whether the key expression is a re-spellable entity name.
    pub key_is_entity_name: bool,
}

impl<'a> CheckerState<'a> {
    /// Render a computed-name object-literal method/accessor whose type was
    /// captured into `computed_index_member_display_types` at computation
    /// time — a getter by its return type, a setter by its parameter type, a
    /// method by its function type — in the same `[{expr}]: V` bracket-
    /// property form a computed-key property assignment gets (`tsc` prints
    /// `{ [ws]: () => number; }`, #16662). Returns `None` when the member was
    /// not captured (a named or late-bound member) or its key resolves to a
    /// static name; the caller then falls back to the structural formatter
    /// for the whole literal.
    pub(in crate::error_reporter) fn computed_index_member_source_display(
        &mut self,
        elem_idx: NodeIndex,
        target_shape: Option<&tsz_solver::ObjectShape>,
    ) -> Option<ComputedIndexMemberDisplay> {
        let member_type = *self
            .ctx
            .object_literal_tracking
            .computed_index_member_display_types
            .get(&elem_idx)?;
        let elem_node = self.ctx.arena.get(elem_idx)?;
        let name_idx = if let Some(method) = self.ctx.arena.get_method_decl(elem_node) {
            method.name
        } else {
            self.ctx.arena.get_accessor(elem_node)?.name
        };
        // Only a wide, non-resolvable key takes the re-spelled property form;
        // a resolvable name keeps its named-member display semantics.
        if self.get_property_name(name_idx).is_some() {
            return None;
        }
        let name_node = self.ctx.arena.get(name_idx)?;
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }
        // Same two-step spelling as the property-assignment arm in
        // `object_literal_source_type_display`: the whole `[...]` node's own
        // source text when renderable, else brackets reassembled around the
        // key expression's text.
        let display_name = if let Some(name) = self.get_member_name_display_text(name_idx) {
            name
        } else {
            let computed = self.ctx.arena.get_computed_property(name_node)?;
            let expr = self.node_text(computed.expression)?;
            format!("[{expr}]", expr = expr.trim())
        };
        let computed_index_kind = self.contextual_computed_index_key_kind(name_idx, target_shape);
        let key_is_entity_name = self.computed_key_is_entity_name_reference(name_idx);
        let widened = self.widen_type_for_display(member_type);
        let widened_value = self.widen_function_like_display_type(widened);
        let value_display = self.format_type_for_assignability_message(widened_value);
        Some(ComputedIndexMemberDisplay {
            rendered: format!("{display_name}: {value_display}"),
            computed_index_kind,
            widened_value,
            key_is_entity_name,
        })
    }

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
