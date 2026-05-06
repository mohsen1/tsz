//! Exact optional property assignability diagnostics.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Check if the assignment failure is due to exact optional property types.
    ///
    /// When `exactOptionalPropertyTypes` is enabled, optional properties don't
    /// implicitly include `undefined`. If the source has `undefined` for properties
    /// that are optional in the target, this is an exact optional property mismatch
    /// and should produce TS2375 instead of TS2322.
    ///
    /// Mirrors tsc's `getExactOptionalUnassignableProperties` + `isExactOptionalPropertyMismatch`.
    pub(crate) fn has_exact_optional_property_mismatch(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        if !self.ctx.compiler_options.exact_optional_property_types {
            return false;
        }
        // Evaluate types to resolve Lazy(DefId) references to concrete Object shapes
        let target_eval = self.evaluate_type_for_assignability(target);
        let source_eval = self.evaluate_type_for_assignability(source);
        let Some(target_shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, target_eval)
        else {
            return false;
        };
        let source_shape =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, source_eval);
        let has_optional_source_for_required_target = source_shape.as_ref().is_some_and(|source| {
            target_shape.properties.iter().any(|target_prop| {
                let target_name = self.ctx.types.resolve_atom_ref(target_prop.name);
                !target_prop.optional
                    && !crate::query_boundaries::class_type::type_includes_undefined(
                        self.ctx.types,
                        target_prop.write_type,
                    )
                    && source
                        .properties
                        .iter()
                        .find(|source_prop| {
                            self.ctx.types.resolve_atom_ref(source_prop.name).as_ref()
                                == target_name.as_ref()
                        })
                        .is_some_and(|source_prop| source_prop.optional)
            })
        });
        for target_prop in &target_shape.properties {
            if !target_prop.optional {
                continue;
            }
            let target_write_includes_undefined =
                crate::query_boundaries::class_type::type_includes_undefined(
                    self.ctx.types,
                    target_prop.write_type,
                );
            if target_write_includes_undefined {
                continue;
            }
            // Check if the source has a property with the same name that includes undefined
            let target_name = self.ctx.types.resolve_atom_ref(target_prop.name);
            let source_prop = source_shape.as_ref().and_then(|s| {
                s.properties.iter().find(|source_prop| {
                    self.ctx.types.resolve_atom_ref(source_prop.name).as_ref()
                        == target_name.as_ref()
                })
            });
            if let Some(source_prop) = source_prop {
                let source_type_includes_undefined =
                    crate::query_boundaries::class_type::type_includes_undefined(
                        self.ctx.types,
                        source_prop.type_id,
                    );
                if source_type_includes_undefined
                    || (source_prop.optional && has_optional_source_for_required_target)
                {
                    return true;
                }
            }
        }
        false
    }

    /// Detect assignment-to-optional-slot mismatches that should produce TS2412.
    ///
    /// TS2412 applies for exact-optional write targets (e.g. `obj.a = value`)
    /// where the write type excludes `undefined` but the source includes it.
    pub(super) fn has_exact_optional_write_target_mismatch(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> bool {
        if !self.ctx.compiler_options.exact_optional_property_types {
            return false;
        }
        if !crate::query_boundaries::class_type::type_includes_undefined(self.ctx.types, source) {
            return false;
        }
        if crate::query_boundaries::class_type::type_includes_undefined(self.ctx.types, target) {
            return false;
        }

        let anchor_idx = self.ctx.arena.skip_parenthesized_and_assertions(anchor_idx);
        let Some(anchor_node) = self.ctx.arena.get(anchor_idx) else {
            return false;
        };

        let mut write_target_idx = if anchor_node.kind
            == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || anchor_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            Some(anchor_idx)
        } else if anchor_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT {
            self.ctx
                .arena
                .get_expression_statement(anchor_node)
                .and_then(|stmt| self.ctx.arena.get(stmt.expression))
                .and_then(|expr_node| {
                    if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                        return None;
                    }
                    let binary = self.ctx.arena.get_binary_expr(expr_node)?;
                    if !self.is_assignment_operator(binary.operator_token) {
                        return None;
                    }
                    let lhs = self
                        .ctx
                        .arena
                        .skip_parenthesized_and_assertions(binary.left);
                    let lhs_node = self.ctx.arena.get(lhs)?;
                    (lhs_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        || lhs_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                        .then_some(lhs)
                })
        } else if anchor_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            self.ctx
                .arena
                .get_binary_expr(anchor_node)
                .and_then(|binary| {
                    if !self.is_assignment_operator(binary.operator_token) {
                        return None;
                    }
                    let lhs = self
                        .ctx
                        .arena
                        .skip_parenthesized_and_assertions(binary.left);
                    let lhs_node = self.ctx.arena.get(lhs)?;
                    (lhs_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        || lhs_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                        .then_some(lhs)
                })
        } else {
            None
        };

        if write_target_idx.is_none()
            && let Some(ext) = self.ctx.arena.get_extended(anchor_idx)
            && let Some(parent_node) = self.ctx.arena.get(ext.parent)
            && (parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || parent_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            && let Some(access) = self.ctx.arena.get_access_expr(parent_node)
            && access.name_or_argument == anchor_idx
        {
            write_target_idx = Some(ext.parent);
        }

        let Some(write_target_idx) = write_target_idx else {
            return false;
        };
        let Some(write_target_node) = self.ctx.arena.get(write_target_idx) else {
            return false;
        };

        let is_property_like_write =
            if write_target_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                true
            } else if write_target_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
                self.ctx
                    .arena
                    .get_access_expr(write_target_node)
                    .is_some_and(|access| {
                        self.get_literal_index_from_node(access.name_or_argument)
                            .is_some()
                            || self
                                .get_literal_string_from_node(access.name_or_argument)
                                .is_some()
                    })
            } else {
                false
            };
        if !is_property_like_write {
            return false;
        }

        let declared_read_target =
            self.declared_property_read_type_for_write_target(write_target_idx);
        let read_target = if let Some(declared) = declared_read_target.filter(|&declared| {
            crate::query_boundaries::class_type::type_includes_undefined(self.ctx.types, declared)
        }) {
            declared
        } else {
            self.get_type_of_node_with_request(
                write_target_idx,
                &crate::context::TypingRequest::NONE,
            )
        };
        if !crate::query_boundaries::class_type::type_includes_undefined(
            self.ctx.types,
            read_target,
        ) {
            return false;
        }

        // TS2412 only applies to true `?`-optional properties. When the read
        // type includes `undefined` purely because `noUncheckedIndexedAccess`
        // widened an index-signature lookup, the regular TS2322 path is the
        // correct diagnostic — fall through. Detect this by checking whether
        // `target | undefined` equals the read target: that pattern is the
        // signature of NUIA-widening on a non-optional slot.
        //
        // Restrict the bail-out to ELEMENT_ACCESS writes, since NUIA only
        // widens index-signature lookups; named PROPERTY_ACCESS writes always
        // see `| undefined` from the property's own `?` optionality marker
        // (or not at all), so the `target | undefined == read_target`
        // signature is the *normal* shape there and shouldn't disable TS2412.
        if self.ctx.compiler_options.no_unchecked_indexed_access
            && write_target_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            let target_with_undef = self
                .ctx
                .types
                .factory()
                .union(vec![target, TypeId::UNDEFINED]);
            if target_with_undef == read_target {
                return false;
            }
        }

        true
    }

    fn declared_property_read_type_for_write_target(
        &mut self,
        write_target_idx: NodeIndex,
    ) -> Option<TypeId> {
        let write_target_node = self.ctx.arena.get(write_target_idx)?;
        let access = self.ctx.arena.get_access_expr(write_target_node)?;
        let property_name = if write_target_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        {
            self.ctx
                .arena
                .get_identifier_at(access.name_or_argument)
                .map(|ident| ident.escaped_text.to_string())
        } else {
            self.get_literal_string_from_node(access.name_or_argument)
                .or_else(|| {
                    self.get_literal_index_from_node(access.name_or_argument)
                        .map(|index| index.to_string())
                })
        }?;

        let object_type = self.get_type_of_node_with_request(
            access.expression,
            &crate::context::TypingRequest::for_write_context(),
        );
        let object_type = self.evaluate_application_type(object_type);
        let object_type = self.resolve_type_for_property_access(object_type);
        match self.resolve_property_access_with_env(object_type, &property_name) {
            crate::query_boundaries::common::PropertyAccessResult::Success { type_id, .. } => {
                Some(type_id)
            }
            crate::query_boundaries::common::PropertyAccessResult::PossiblyNullOrUndefined {
                property_type: Some(type_id),
                ..
            } => Some(type_id),
            _ => None,
        }
    }
}
