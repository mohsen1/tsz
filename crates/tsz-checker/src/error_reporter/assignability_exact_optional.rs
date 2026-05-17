//! Exact optional property assignability diagnostics.

use crate::state::CheckerState;
use crate::symbols_domain::name_text;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
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
        let target_includes_undefined =
            crate::query_boundaries::class_type::type_includes_undefined(self.ctx.types, target);

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

        if write_target_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(write_target_node)
            && self
                .get_literal_index_from_node(access.name_or_argument)
                .is_some()
        {
            let object_type = self.get_type_of_write_target_base_expression(access.expression);
            let object_type = self.evaluate_application_type(object_type);
            let object_type = self.resolve_type_for_property_access(object_type);
            if crate::query_boundaries::common::tuple_elements(self.ctx.types, object_type)
                .is_some()
            {
                return false;
            }
        }

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
        if self.same_property_self_assignment_in_presence_true_branch(write_target_idx, anchor_idx)
        {
            return false;
        }
        if target_includes_undefined
            && !self.write_target_is_declared_optional_property(write_target_idx)
        {
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

    fn write_target_is_declared_optional_property(&mut self, write_target_idx: NodeIndex) -> bool {
        let Some(write_target_node) = self.ctx.arena.get(write_target_idx) else {
            return false;
        };
        let Some(access) = self.ctx.arena.get_access_expr(write_target_node) else {
            return false;
        };
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
        };
        let Some(property_name) = property_name else {
            return false;
        };

        let object_type = self.get_type_of_node_with_request(
            access.expression,
            &crate::context::TypingRequest::for_write_context(),
        );
        let object_type = self.evaluate_application_type(object_type);
        let object_type = self.resolve_type_for_property_access(object_type);
        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, object_type)
        else {
            return false;
        };
        let atom = self.ctx.types.intern_string(&property_name);
        shape
            .properties
            .iter()
            .any(|prop| prop.name == atom && prop.optional)
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

    fn same_property_self_assignment_in_presence_true_branch(
        &self,
        write_target_idx: NodeIndex,
        anchor_idx: NodeIndex,
    ) -> bool {
        let Some((base_text, property_name)) = self.property_access_base_and_name(write_target_idx)
        else {
            return false;
        };
        let Some((binary_idx, rhs_idx)) = self.assignment_binary_and_rhs(anchor_idx) else {
            return false;
        };
        if self.property_access_base_and_name(rhs_idx).as_ref()
            != Some(&(base_text.clone(), property_name.clone()))
        {
            return false;
        }
        self.is_inside_presence_branch(binary_idx, &base_text, &property_name, true)
    }

    pub(crate) fn same_property_self_assignment_in_presence_false_branch(
        &self,
        anchor_idx: NodeIndex,
    ) -> bool {
        let Some((binary_idx, rhs_idx)) = self.assignment_binary_and_rhs(anchor_idx) else {
            return false;
        };
        let Some(binary_node) = self.ctx.arena.get(binary_idx) else {
            return false;
        };
        let Some(binary) = self.ctx.arena.get_binary_expr(binary_node) else {
            return false;
        };
        let Some((base_text, property_name)) = self.property_access_base_and_name(binary.left)
        else {
            return false;
        };
        if self.property_access_base_and_name(rhs_idx).as_ref()
            != Some(&(base_text.clone(), property_name.clone()))
        {
            return false;
        }
        self.is_inside_presence_branch(binary_idx, &base_text, &property_name, false)
    }

    pub(crate) fn same_property_self_assignment_in_presence_true_branch_for_anchor(
        &self,
        anchor_idx: NodeIndex,
    ) -> bool {
        let Some((binary_idx, rhs_idx)) = self.assignment_binary_and_rhs(anchor_idx) else {
            return false;
        };
        let Some(binary_node) = self.ctx.arena.get(binary_idx) else {
            return false;
        };
        let Some(binary) = self.ctx.arena.get_binary_expr(binary_node) else {
            return false;
        };
        let Some((base_text, property_name)) = self.property_access_base_and_name(binary.left)
        else {
            return false;
        };
        if self.property_access_base_and_name(rhs_idx).as_ref()
            != Some(&(base_text.clone(), property_name.clone()))
        {
            return false;
        }
        self.is_inside_presence_branch(binary_idx, &base_text, &property_name, true)
    }

    fn assignment_binary_and_rhs(&self, anchor_idx: NodeIndex) -> Option<(NodeIndex, NodeIndex)> {
        let mut current = anchor_idx;
        for _ in 0..crate::state::MAX_TREE_WALK_ITERATIONS {
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
                && let Some(stmt) = self.ctx.arena.get_expression_statement(node)
                && let Some(expr_node) = self.ctx.arena.get(stmt.expression)
                && expr_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            {
                let binary = self.ctx.arena.get_binary_expr(expr_node)?;
                if self.is_assignment_operator(binary.operator_token) {
                    return Some((stmt.expression, binary.right));
                }
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                let binary = self.ctx.arena.get_binary_expr(node)?;
                if self.is_assignment_operator(binary.operator_token) {
                    return Some((current, binary.right));
                }
            }
            current = self.ctx.arena.parent_of(current)?;
            if current.is_none() {
                return None;
            }
        }
        None
    }

    fn property_access_base_and_name(&self, idx: NodeIndex) -> Option<(String, String)> {
        let idx = self.ctx.arena.skip_parenthesized_and_assertions(idx);
        let node = self.ctx.arena.get(idx)?;
        let access = self.ctx.arena.get_access_expr(node)?;
        let property_name = if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
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
        let base_text =
            name_text::property_access_chain_text_in_arena(self.ctx.arena, access.expression)?;
        Some((base_text, property_name))
    }

    fn is_inside_presence_branch(
        &self,
        mut child: NodeIndex,
        base_text: &str,
        property_name: &str,
        true_branch: bool,
    ) -> bool {
        for _ in 0..crate::state::MAX_TREE_WALK_ITERATIONS {
            let Some(parent) = self.ctx.arena.parent_of(child) else {
                return false;
            };
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };
            if parent_node.kind == syntax_kind_ext::IF_STATEMENT {
                let Some(if_stmt) = self.ctx.arena.get_if_statement(parent_node) else {
                    return false;
                };
                let branch_matches = if true_branch {
                    if_stmt.then_statement == child
                } else {
                    if_stmt.else_statement == child
                };
                return branch_matches
                    && self.condition_is_property_presence(
                        if_stmt.expression,
                        base_text,
                        property_name,
                    );
            }
            child = parent;
        }
        false
    }

    fn condition_is_property_presence(
        &self,
        condition_idx: NodeIndex,
        base_text: &str,
        property_name: &str,
    ) -> bool {
        let condition_idx = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(condition_idx);
        let Some(condition_node) = self.ctx.arena.get(condition_idx) else {
            return false;
        };
        if condition_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(condition_node)
            && binary.operator_token == SyntaxKind::InKeyword as u16
        {
            return self
                .get_literal_string_from_node(binary.left)
                .is_some_and(|name| name == property_name)
                && name_text::property_access_chain_text_in_arena(self.ctx.arena, binary.right)
                    .is_some_and(|text| text == base_text);
        }
        if condition_node.kind == syntax_kind_ext::CALL_EXPRESSION
            && let Some(call) = self.ctx.arena.get_call_expr(condition_node)
            && let Some(callee_node) = self.ctx.arena.get(call.expression)
            && let Some(access) = self.ctx.arena.get_access_expr(callee_node)
            && self
                .ctx
                .arena
                .get_identifier_at(access.name_or_argument)
                .is_some_and(|ident| ident.escaped_text == "hasOwnProperty")
            && name_text::property_access_chain_text_in_arena(self.ctx.arena, access.expression)
                .is_some_and(|text| text == base_text)
            && let Some(args) = call.arguments.as_ref()
            && args.nodes.len() == 1
        {
            return self
                .get_literal_string_from_node(args.nodes[0])
                .is_some_and(|name| name == property_name);
        }
        false
    }
}
