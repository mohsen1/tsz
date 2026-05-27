//! Symbol-key routing helpers for object literal type inference.
//!
//! Object literals with computed property names of the wide `symbol` type
//! (`TypeId::SYMBOL`, i.e. not `unique symbol`, not a literal-resolvable
//! atom, not a well-known `Symbol.xxx` property access) must synthesize a
//! `[k: symbol]: V` index signature in their inferred type per tsc parity
//! (issue #9755). Without this routing, the value is stashed under a
//! synthetic `__symbol_<file>_<sym>` named atom and the inferred type is
//! neither indexable by `symbol` nor surfaces `symbol` in `keyof`.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Route a computed-property value into the appropriate index-signature
    /// bucket for object literal type inference.
    ///
    /// Per tsc, when a computed property name resolves to a literal atom
    /// (string/number literal, `unique symbol`, enum member) it becomes a
    /// named member. Otherwise the key type's widened kind determines the
    /// index signature:
    ///
    /// - `<: number` → number index signature
    /// - `<: symbol` (but not numeric) → symbol index signature
    /// - everything else (`<: string`, `any`, `unknown`, generic) → string
    ///   index signature
    ///
    /// The ordering matters: `any` is assignable to every concrete kind, so
    /// it must fall into the FIRST matching branch (number) to preserve the
    /// legacy behaviour for opaque keys. The dedicated symbol branch fires
    /// for wide `symbol` values that previously degraded into the string
    /// branch via the tautological `is_assignable_to(_, ANY)` check.
    pub(super) fn route_computed_member_value_to_index_signature(
        &mut self,
        prop_name_type: TypeId,
        value_type: TypeId,
        number_index_types: &mut Vec<TypeId>,
        string_index_types: &mut Vec<TypeId>,
        symbol_index_types: &mut Vec<TypeId>,
    ) {
        if self.is_assignable_to(prop_name_type, TypeId::NUMBER) {
            number_index_types.push(value_type);
        } else if self.is_assignable_to(prop_name_type, TypeId::SYMBOL) {
            symbol_index_types.push(value_type);
        } else {
            string_index_types.push(value_type);
        }
    }

    /// True when a property/method/accessor name node is a computed-property
    /// name whose key expression has the wide `symbol` type (`TypeId::SYMBOL`)
    /// — i.e. a non-`unique`, non-literal-resolvable symbol.
    ///
    /// Such keys must contribute a `[k: symbol]: V` index signature in the
    /// inferred object literal type per tsc parity (issue #9755). Without
    /// bypassing the named-property path, the value would be stashed under a
    /// synthetic `__symbol_<file>_<sym>` atom and the inferred type would
    /// neither be indexable by `symbol` nor surface `symbol` in `keyof`.
    ///
    /// Restricted to `TypeId::SYMBOL` exactly so that `unique symbol`,
    /// well-known symbol references (e.g. `[Symbol.iterator]`), literal-
    /// resolvable keys, and generic type parameters keep their existing
    /// named-member semantics.
    ///
    /// Further limited to bare-identifier key expressions: property access
    /// chains like `Symbol.iterator` flow through the well-known-symbol
    /// resolution path in `get_property_name_resolved`, which produces the
    /// canonical `[Symbol.xxx]` named-member key. Those keys must keep
    /// their named-member semantics so that mismatches like
    /// `{ [Symbol.iterator]: 123 }` against `{ [k: symbol]: string }`
    /// still surface TS2418.
    ///
    /// Late-bound binding-identity members (`__symbol_<file>_<sym>`) are
    /// only synthesized by `symbol_valued_binding_property_name` for
    /// identifier expressions whose value declaration is a `const`. That
    /// is precisely the case where tsc emits a `[k: symbol]: V` index
    /// signature instead of a named member.
    /// Compute the `(is_string_named, is_symbol_named, single_quoted_name)`
    /// flags that the synthesized `PropertyInfo` for an object-literal member
    /// must carry. Used by every member form (property assignment, method
    /// shorthand, getter, setter) so flags reflect the name-node shape
    /// regardless of declaration syntax (issue #9763).
    pub(super) fn object_literal_member_naming_flags(
        &mut self,
        name_idx: NodeIndex,
    ) -> (bool, bool, bool) {
        let (string_literal_name, single_quoted_name) =
            self.ctx.arena.string_property_name_flags(name_idx);
        let is_string_named =
            string_literal_name || self.is_computed_string_property_name(name_idx);
        let is_symbol_named = self.is_symbol_property_name(name_idx);
        (is_string_named, is_symbol_named, single_quoted_name)
    }

    pub(super) fn object_literal_computed_key_is_wide_symbol(
        &mut self,
        name_idx: NodeIndex,
    ) -> bool {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }
        let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
            return false;
        };
        let Some(expr_node) = self.ctx.arena.get(computed.expression) else {
            return false;
        };
        let is_supported_wide_symbol_syntax = self.ctx.arena.get_identifier(expr_node).is_some()
            || matches!(
                expr_node.kind,
                k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            );
        if !is_supported_wide_symbol_syntax {
            return false;
        }
        if self
            .declared_unique_symbol_member_property_name(computed.expression)
            .is_some()
        {
            return false;
        }
        self.get_type_of_node(computed.expression) == TypeId::SYMBOL
    }

    pub(super) fn report_contextual_symbol_index_value_mismatch(
        &mut self,
        name_idx: NodeIndex,
        value_idx: Option<NodeIndex>,
        source_value_type: TypeId,
        contextual_type: Option<TypeId>,
    ) -> bool {
        let Some(target_value_type) = self.contextual_symbol_index_value_type(contextual_type)
        else {
            return false;
        };
        if self.diagnostic_relation_boolean_guard(source_value_type, target_value_type) {
            return false;
        }

        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        let source_type = value_idx
            .and_then(|idx| self.literal_type_from_initializer(idx))
            .unwrap_or(source_value_type);
        let source_type = self.widen_literal_type(source_type);
        let source_str = self.format_type_for_assignability_message(source_type);
        let target_str = self.format_type_for_assignability_message(target_value_type);
        let message = format_message(
            diagnostic_messages::TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, &target_str],
        );
        self.error_at_node(
            name_idx,
            &message,
            diagnostic_codes::TYPE_OF_COMPUTED_PROPERTYS_VALUE_IS_WHICH_IS_NOT_ASSIGNABLE_TO_TYPE,
        );
        true
    }

    fn contextual_symbol_index_value_type(
        &mut self,
        contextual_type: Option<TypeId>,
    ) -> Option<TypeId> {
        let contextual_type = contextual_type?;
        let mut value_types = Vec::new();
        self.collect_contextual_symbol_index_value_types(contextual_type, &mut value_types);
        if value_types.is_empty() {
            return None;
        }
        Some(tsz_solver::utils::union_or_single(
            self.ctx.types,
            value_types,
        ))
    }

    fn collect_contextual_symbol_index_value_types(
        &mut self,
        type_id: TypeId,
        value_types: &mut Vec<TypeId>,
    ) {
        let evaluated = self.evaluate_type_with_env(type_id);
        let resolved = self.resolve_lazy_type(evaluated);
        if let Some(shape) =
            crate::query_boundaries::state::checking::object_shape(self.ctx.types, resolved)
        {
            if let Some(index) = &shape.string_index
                && self.resolve_index_signature_key_type_via_env(index.key_type) == TypeId::SYMBOL
            {
                value_types.push(index.value_type);
            }
            return;
        }

        if let Some(members) =
            crate::query_boundaries::state::checking::union_members(self.ctx.types, resolved)
        {
            for member in members {
                self.collect_contextual_symbol_index_value_types(member, value_types);
            }
        }
    }
}
