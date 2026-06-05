//! Coverage checks for duplicate type-alias missing-name validation.

use crate::query_boundaries::name_resolution::NameLookupKind;
use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use crate::types_domain::unique_symbol_arena::is_unique_symbol_type_annotation_unwrapped;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    pub(crate) fn type_alias_body_missing_names_syntax_covered_by_type_node_checking(
        &self,
        root: NodeIndex,
    ) -> bool {
        self.type_alias_body_missing_names_syntax_covered_inner(root)
    }

    fn type_alias_body_missing_names_syntax_covered_inner(&self, node_idx: NodeIndex) -> bool {
        if node_idx == NodeIndex::NONE {
            return true;
        }
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
                    return false;
                };
                type_ref.type_arguments.as_ref().is_none_or(|args| {
                    args.nodes.iter().copied().all(|arg_idx| {
                        self.type_alias_body_missing_names_syntax_covered_inner(arg_idx)
                    })
                })
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                let Some(composite) = self.ctx.arena.get_composite_type(node) else {
                    return false;
                };
                composite.types.nodes.iter().copied().all(|member_idx| {
                    self.type_alias_body_missing_names_syntax_covered_inner(member_idx)
                })
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                self.ctx.arena.get_array_type(node).is_some_and(|array| {
                    self.type_alias_body_missing_names_syntax_covered_inner(array.element_type)
                })
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                let Some(tuple) = self.ctx.arena.get_tuple_type(node) else {
                    return false;
                };
                tuple.elements.nodes.iter().copied().all(|element_idx| {
                    self.type_alias_body_missing_names_syntax_covered_inner(element_idx)
                })
            }
            k if k == syntax_kind_ext::NAMED_TUPLE_MEMBER => self
                .ctx
                .arena
                .get_named_tuple_member(node)
                .is_some_and(|member| {
                    self.type_alias_body_missing_names_syntax_covered_inner(member.type_node)
                }),
            k if k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE
                || k == syntax_kind_ext::PARENTHESIZED_TYPE =>
            {
                self.ctx
                    .arena
                    .get_wrapped_type(node)
                    .is_some_and(|wrapped| {
                        self.type_alias_body_missing_names_syntax_covered_inner(wrapped.type_node)
                    })
            }
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                let Some(indexed) = self.ctx.arena.get_indexed_access_type(node) else {
                    return false;
                };
                self.type_alias_body_missing_names_syntax_covered_inner(indexed.object_type)
                    && self.type_alias_body_missing_names_syntax_covered_inner(indexed.index_type)
            }
            k if Self::primitive_or_literal_type_kind_is_covered(k) => true,
            _ => false,
        }
    }

    pub(crate) fn type_alias_body_missing_names_covered_by_type_node_checking(
        &self,
        root: NodeIndex,
    ) -> bool {
        self.type_alias_body_missing_names_covered_inner(root, false, &mut Vec::new())
    }

    fn type_alias_body_missing_names_covered_inner(
        &self,
        node_idx: NodeIndex,
        in_conditional_extends: bool,
        scoped_names: &mut Vec<String>,
    ) -> bool {
        if node_idx == NodeIndex::NONE {
            return true;
        }
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
                    return false;
                };
                if !self.type_reference_name_is_resolved_for_missing_name_coverage(
                    node_idx,
                    scoped_names,
                ) {
                    return false;
                }
                if let Some(args) = &type_ref.type_arguments {
                    return args.nodes.iter().copied().all(|arg_idx| {
                        self.type_alias_body_missing_names_covered_inner(
                            arg_idx,
                            in_conditional_extends,
                            scoped_names,
                        )
                    });
                }
                true
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                let Some(composite) = self.ctx.arena.get_composite_type(node) else {
                    return false;
                };
                composite.types.nodes.iter().copied().all(|member_idx| {
                    self.type_alias_body_missing_names_covered_inner(
                        member_idx,
                        in_conditional_extends,
                        scoped_names,
                    )
                })
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                self.ctx.arena.get_array_type(node).is_some_and(|array| {
                    self.type_alias_body_missing_names_covered_inner(
                        array.element_type,
                        in_conditional_extends,
                        scoped_names,
                    )
                })
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                let Some(tuple) = self.ctx.arena.get_tuple_type(node) else {
                    return false;
                };
                tuple.elements.nodes.iter().copied().all(|element_idx| {
                    self.type_alias_body_missing_names_covered_inner(
                        element_idx,
                        in_conditional_extends,
                        scoped_names,
                    )
                })
            }
            k if k == syntax_kind_ext::NAMED_TUPLE_MEMBER => self
                .ctx
                .arena
                .get_named_tuple_member(node)
                .is_some_and(|member| {
                    self.type_alias_body_missing_names_covered_inner(
                        member.type_node,
                        in_conditional_extends,
                        scoped_names,
                    )
                }),
            k if k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE
                || k == syntax_kind_ext::PARENTHESIZED_TYPE =>
            {
                self.ctx
                    .arena
                    .get_wrapped_type(node)
                    .is_some_and(|wrapped| {
                        self.type_alias_body_missing_names_covered_inner(
                            wrapped.type_node,
                            in_conditional_extends,
                            scoped_names,
                        )
                    })
            }
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                let Some(indexed) = self.ctx.arena.get_indexed_access_type(node) else {
                    return false;
                };
                self.type_alias_body_missing_names_covered_inner(
                    indexed.object_type,
                    in_conditional_extends,
                    scoped_names,
                ) && self.type_alias_body_missing_names_covered_inner(
                    indexed.index_type,
                    in_conditional_extends,
                    scoped_names,
                )
            }
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                let Some(cond) = self.ctx.arena.get_conditional_type(node) else {
                    return false;
                };
                if is_unique_symbol_type_annotation_unwrapped(self.ctx.arena, cond.extends_type)
                    || self.conditional_extends_may_need_infer_constraint_consistency(
                        cond.extends_type,
                    )
                {
                    return false;
                }
                if !self.type_alias_body_missing_names_covered_inner(
                    cond.check_type,
                    false,
                    scoped_names,
                ) || !self.type_alias_body_missing_names_covered_inner(
                    cond.extends_type,
                    true,
                    scoped_names,
                ) {
                    return false;
                }

                let infer_names = self.collect_infer_type_parameters(cond.extends_type);
                let old_len = scoped_names.len();
                for name in infer_names {
                    if !scoped_names.contains(&name) {
                        scoped_names.push(name);
                    }
                }
                let true_covered = self.type_alias_body_missing_names_covered_inner(
                    cond.true_type,
                    false,
                    scoped_names,
                );
                scoped_names.truncate(old_len);
                true_covered
                    && self.type_alias_body_missing_names_covered_inner(
                        cond.false_type,
                        false,
                        scoped_names,
                    )
            }
            k if k == syntax_kind_ext::INFER_TYPE => {
                if !in_conditional_extends {
                    return false;
                }
                let Some(infer) = self.ctx.arena.get_infer_type(node) else {
                    return false;
                };
                self.type_parameter_missing_name_coverage_is_safe(
                    infer.type_parameter,
                    in_conditional_extends,
                    scoped_names,
                )
            }
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                let Some(op) = self.ctx.arena.get_type_operator(node) else {
                    return false;
                };
                if op.operator == SyntaxKind::ReadonlyKeyword as u16
                    && let Some(operand_node) = self.ctx.arena.get(op.type_node)
                    && operand_node.kind != syntax_kind_ext::ARRAY_TYPE
                    && operand_node.kind != syntax_kind_ext::TUPLE_TYPE
                {
                    return false;
                }
                self.type_alias_body_missing_names_covered_inner(
                    op.type_node,
                    in_conditional_extends,
                    scoped_names,
                )
            }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                let Some(mapped) = self.ctx.arena.get_mapped_type(node) else {
                    return false;
                };
                if (self.ctx.no_implicit_any() && mapped.type_node.is_none())
                    || mapped.members.is_some()
                    || !self.type_parameter_missing_name_coverage_is_safe(
                        mapped.type_parameter,
                        in_conditional_extends,
                        scoped_names,
                    )
                {
                    return false;
                }
                let Some(name) =
                    self.type_parameter_name_for_missing_name_coverage(mapped.type_parameter)
                else {
                    return false;
                };
                let old_len = scoped_names.len();
                if !scoped_names.contains(&name) {
                    scoped_names.push(name);
                }
                let name_type_covered = mapped.name_type.is_none()
                    || self.type_alias_body_missing_names_covered_inner(
                        mapped.name_type,
                        in_conditional_extends,
                        scoped_names,
                    );
                let type_node_covered = mapped.type_node.is_none()
                    || self.type_alias_body_missing_names_covered_inner(
                        mapped.type_node,
                        in_conditional_extends,
                        scoped_names,
                    );
                scoped_names.truncate(old_len);
                name_type_covered && type_node_covered
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                let Some(type_lit) = self.ctx.arena.get_type_literal(node) else {
                    return false;
                };
                type_lit.members.nodes.iter().copied().all(|member_idx| {
                    self.type_member_missing_name_coverage_is_safe(
                        member_idx,
                        in_conditional_extends,
                        scoped_names,
                    )
                })
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                let Some(template) = self.ctx.arena.get_template_literal_type(node) else {
                    return false;
                };
                template
                    .template_spans
                    .nodes
                    .iter()
                    .copied()
                    .all(|span_idx| {
                        let Some(span_node) = self.ctx.arena.get(span_idx) else {
                            return false;
                        };
                        let Some(span) = self.ctx.arena.get_template_span(span_node) else {
                            return false;
                        };
                        self.type_alias_body_missing_names_covered_inner(
                            span.expression,
                            in_conditional_extends,
                            scoped_names,
                        )
                    })
            }
            k if k == syntax_kind_ext::TYPE_PREDICATE => {
                let Some(pred) = self.ctx.arena.get_type_predicate(node) else {
                    return false;
                };
                pred.type_node.is_none()
                    || self.type_alias_body_missing_names_covered_inner(
                        pred.type_node,
                        in_conditional_extends,
                        scoped_names,
                    )
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                self.function_type_missing_name_coverage_is_safe(
                    node,
                    in_conditional_extends,
                    scoped_names,
                )
            }
            k if Self::primitive_or_literal_type_kind_is_covered(k) => true,
            _ => false,
        }
    }

    fn type_reference_name_is_resolved_for_missing_name_coverage(
        &self,
        type_idx: NodeIndex,
        scoped_names: &[String],
    ) -> bool {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return false;
        };
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return false;
        };
        if self.type_ref_is_bare_scoped_type_parameter(
            type_ref.type_name,
            type_ref.type_arguments.as_ref(),
        ) {
            return true;
        }
        let Some(name_node) = self.ctx.arena.get(type_ref.type_name) else {
            return self
                .resolve_type_symbol_for_lowering(type_ref.type_name)
                .is_some();
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return self
                .resolve_type_symbol_for_lowering(type_ref.type_name)
                .is_some();
        };
        let name = ident.escaped_text.as_str();
        if type_ref.type_arguments.is_none() && scoped_names.iter().any(|scoped| scoped == name) {
            return true;
        }
        let shadows_managed_array = matches!(name, "Array" | "ReadonlyArray")
            && self.ctx.file_local_type_shadow_for_lib_name(name);
        let primitive_type = matches!(
            name,
            "any"
                | "unknown"
                | "never"
                | "void"
                | "undefined"
                | "null"
                | "boolean"
                | "number"
                | "bigint"
                | "string"
                | "symbol"
                | "object"
        );
        if (tsz_solver::is_compiler_managed_type(name) || primitive_type) && !shadows_managed_array
        {
            return true;
        }
        self.resolve_type_symbol_for_lowering(type_ref.type_name)
            .is_some()
    }

    fn type_parameter_missing_name_coverage_is_safe(
        &self,
        param_idx: NodeIndex,
        in_conditional_extends: bool,
        scoped_names: &mut Vec<String>,
    ) -> bool {
        let Some(param_node) = self.ctx.arena.get(param_idx) else {
            return false;
        };
        let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
            return false;
        };
        let Some(name_node) = self.ctx.arena.get(param.name) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        if crate::error_reporter::assignability::is_reserved_type_name(&ident.escaped_text) {
            return false;
        }
        (param.constraint.is_none()
            || self.type_alias_body_missing_names_covered_inner(
                param.constraint,
                in_conditional_extends,
                scoped_names,
            ))
            && (param.default.is_none()
                || self.type_alias_body_missing_names_covered_inner(
                    param.default,
                    in_conditional_extends,
                    scoped_names,
                ))
    }

    fn type_parameter_name_for_missing_name_coverage(
        &self,
        param_idx: NodeIndex,
    ) -> Option<String> {
        let param_node = self.ctx.arena.get(param_idx)?;
        let param = self.ctx.arena.get_type_parameter(param_node)?;
        let name_node = self.ctx.arena.get(param.name)?;
        let ident = self.ctx.arena.get_identifier(name_node)?;
        Some(ident.escaped_text.clone())
    }

    fn type_member_missing_name_coverage_is_safe(
        &self,
        member_idx: NodeIndex,
        in_conditional_extends: bool,
        scoped_names: &mut Vec<String>,
    ) -> bool {
        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return false;
        };
        if let Some(prop) = self.ctx.arena.get_property_decl(member_node) {
            return prop.type_annotation.is_some()
                && self.type_alias_body_missing_names_covered_inner(
                    prop.type_annotation,
                    in_conditional_extends,
                    scoped_names,
                );
        }
        if let Some(sig) = self.ctx.arena.get_signature(member_node) {
            let old_len = scoped_names.len();
            if let Some(type_params) = &sig.type_parameters {
                for &tp_idx in &type_params.nodes {
                    if !self.type_parameter_missing_name_coverage_is_safe(
                        tp_idx,
                        in_conditional_extends,
                        scoped_names,
                    ) {
                        scoped_names.truncate(old_len);
                        return false;
                    }
                    let Some(name) = self.type_parameter_name_for_missing_name_coverage(tp_idx)
                    else {
                        scoped_names.truncate(old_len);
                        return false;
                    };
                    if !scoped_names.contains(&name) {
                        scoped_names.push(name);
                    }
                }
            }
            if let Some(params) = &sig.parameters {
                for &param_idx in &params.nodes {
                    if !self.parameter_missing_name_coverage_is_safe(
                        param_idx,
                        in_conditional_extends,
                        scoped_names,
                    ) {
                        scoped_names.truncate(old_len);
                        return false;
                    }
                }
            }
            let covered = sig.type_annotation.is_none()
                || self.type_alias_body_missing_names_covered_inner(
                    sig.type_annotation,
                    in_conditional_extends,
                    scoped_names,
                );
            scoped_names.truncate(old_len);
            return covered;
        }
        false
    }

    fn function_type_missing_name_coverage_is_safe(
        &self,
        node: &tsz_parser::parser::node::Node,
        in_conditional_extends: bool,
        scoped_names: &mut Vec<String>,
    ) -> bool {
        let Some(func_type) = self.ctx.arena.get_function_type(node) else {
            return false;
        };
        let old_len = scoped_names.len();
        if let Some(type_params) = &func_type.type_parameters {
            for &tp_idx in &type_params.nodes {
                if !self.type_parameter_missing_name_coverage_is_safe(
                    tp_idx,
                    in_conditional_extends,
                    scoped_names,
                ) {
                    scoped_names.truncate(old_len);
                    return false;
                }
                let Some(name) = self.type_parameter_name_for_missing_name_coverage(tp_idx) else {
                    scoped_names.truncate(old_len);
                    return false;
                };
                if !scoped_names.contains(&name) {
                    scoped_names.push(name);
                }
            }
        }
        for &param_idx in &func_type.parameters.nodes {
            if !self.parameter_missing_name_coverage_is_safe(
                param_idx,
                in_conditional_extends,
                scoped_names,
            ) {
                scoped_names.truncate(old_len);
                return false;
            }
        }
        let covered = func_type.type_annotation.is_none()
            || self.type_alias_body_missing_names_covered_inner(
                func_type.type_annotation,
                in_conditional_extends,
                scoped_names,
            );
        scoped_names.truncate(old_len);
        covered
    }

    fn parameter_missing_name_coverage_is_safe(
        &self,
        param_idx: NodeIndex,
        in_conditional_extends: bool,
        scoped_names: &mut Vec<String>,
    ) -> bool {
        let Some(param_node) = self.ctx.arena.get(param_idx) else {
            return false;
        };
        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
            return false;
        };
        param.type_annotation.is_none()
            || self.type_alias_body_missing_names_covered_inner(
                param.type_annotation,
                in_conditional_extends,
                scoped_names,
            )
    }

    fn conditional_extends_may_need_infer_constraint_consistency(
        &self,
        extends_type: NodeIndex,
    ) -> bool {
        let infer_decls = self.collect_infer_type_params_with_constraints(extends_type);
        for (idx, (name, constraint, _)) in infer_decls.iter().enumerate() {
            if constraint.is_none() {
                continue;
            }
            if infer_decls
                .iter()
                .skip(idx + 1)
                .any(|(other, other_constraint, _)| other == name && other_constraint.is_some())
            {
                return true;
            }
        }
        false
    }

    const fn primitive_or_literal_type_kind_is_covered(kind: u16) -> bool {
        matches!(
            kind,
            k if k == SyntaxKind::AnyKeyword as u16
                || k == SyntaxKind::UnknownKeyword as u16
                || k == SyntaxKind::NeverKeyword as u16
                || k == SyntaxKind::VoidKeyword as u16
                || k == SyntaxKind::UndefinedKeyword as u16
                || k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::BooleanKeyword as u16
                || k == SyntaxKind::NumberKeyword as u16
                || k == SyntaxKind::BigIntKeyword as u16
                || k == SyntaxKind::StringKeyword as u16
                || k == SyntaxKind::SymbolKeyword as u16
                || k == SyntaxKind::ObjectKeyword as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == syntax_kind_ext::LITERAL_TYPE
        )
    }

    pub(crate) fn check_type_alias_body_for_missing_names_after_type_node_check(
        &mut self,
        type_idx: NodeIndex,
    ) {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                self.check_type_alias_body_type_reference_name(type_idx);
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(array) = self.ctx.arena.get_array_type(node) {
                    self.check_type_alias_body_for_missing_names_after_type_node_check(
                        array.element_type,
                    );
                }
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                if let Some(tuple) = self.ctx.arena.get_tuple_type(node) {
                    let elements = tuple.elements.nodes.clone();
                    for element_idx in elements {
                        self.check_type_alias_body_for_missing_names_after_type_node_check(
                            element_idx,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE
                || k == syntax_kind_ext::PARENTHESIZED_TYPE =>
            {
                if let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) {
                    self.check_type_alias_body_for_missing_names_after_type_node_check(
                        wrapped.type_node,
                    );
                }
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                    let members = composite.types.nodes.clone();
                    for member_idx in members {
                        self.check_type_alias_body_for_missing_names_after_type_node_check(
                            member_idx,
                        );
                    }
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                if let Some(cond) = self.ctx.arena.get_conditional_type(node) {
                    let check_type = cond.check_type;
                    let extends_type = cond.extends_type;
                    let true_type = cond.true_type;
                    let false_type = cond.false_type;

                    self.check_type_alias_body_for_missing_names_after_type_node_check(check_type);

                    self.ctx.in_conditional_extends_depth += 1;
                    self.check_type_alias_body_for_missing_names_after_type_node_check(
                        extends_type,
                    );
                    self.ctx.in_conditional_extends_depth -= 1;
                    self.check_unique_symbol_in_conditional_extends(extends_type);
                    self.check_infer_constraint_consistency(extends_type);

                    let param_bindings = self.push_infer_bindings_from_extends(extends_type);
                    self.check_type_alias_body_for_missing_names_after_type_node_check(true_type);
                    for (name, previous) in param_bindings.into_iter().rev() {
                        if let Some(prev_type) = previous {
                            self.ctx.type_parameter_scope.insert(name, prev_type);
                        } else {
                            self.ctx.type_parameter_scope.remove(&name);
                        }
                    }
                    self.check_type_alias_body_for_missing_names_after_type_node_check(false_type);
                }
            }
            k if k == syntax_kind_ext::INFER_TYPE => {
                if let Some(infer) = self.ctx.arena.get_infer_type(node) {
                    if self.ctx.in_conditional_extends_depth == 0 {
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                        self.error_at_node(
                            type_idx,
                            diagnostic_messages::INFER_DECLARATIONS_ARE_ONLY_PERMITTED_IN_THE_EXTENDS_CLAUSE_OF_A_CONDITIONAL_TYP,
                            diagnostic_codes::INFER_DECLARATIONS_ARE_ONLY_PERMITTED_IN_THE_EXTENDS_CLAUSE_OF_A_CONDITIONAL_TYP,
                        );
                    }
                    self.check_type_parameter_node_for_missing_names(infer.type_parameter);
                }
            }
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                if let Some(op) = self.ctx.arena.get_type_operator(node) {
                    if op.operator == SyntaxKind::ReadonlyKeyword as u16
                        && let Some(operand_node) = self.ctx.arena.get(op.type_node)
                        && operand_node.kind != syntax_kind_ext::ARRAY_TYPE
                        && operand_node.kind != syntax_kind_ext::TUPLE_TYPE
                    {
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                        self.ctx.error(
                            node.pos,
                            node.end.saturating_sub(node.pos),
                            diagnostic_messages::READONLY_TYPE_MODIFIER_IS_ONLY_PERMITTED_ON_ARRAY_AND_TUPLE_LITERAL_TYPES.to_string(),
                            diagnostic_codes::READONLY_TYPE_MODIFIER_IS_ONLY_PERMITTED_ON_ARRAY_AND_TUPLE_LITERAL_TYPES,
                        );
                    }
                    self.check_type_alias_body_for_missing_names_after_type_node_check(
                        op.type_node,
                    );
                }
            }
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                if let Some(indexed) = self.ctx.arena.get_indexed_access_type(node) {
                    self.check_type_alias_body_for_missing_names_after_type_node_check(
                        indexed.object_type,
                    );
                    self.check_type_alias_body_for_missing_names_after_type_node_check(
                        indexed.index_type,
                    );
                }
            }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                if let Some(mapped) = self.ctx.arena.get_mapped_type(node) {
                    if self.ctx.no_implicit_any() && mapped.type_node.is_none() {
                        self.ctx.error(
                            node.pos,
                            node.end.saturating_sub(node.pos),
                            "Mapped object type implicitly has an 'any' template type.".to_string(),
                            7039,
                        );
                    }
                    let param_binding =
                        self.push_mapped_type_param_provisional(mapped.type_parameter);
                    self.check_type_parameter_node_for_missing_names(mapped.type_parameter);
                    if mapped.name_type.is_some() {
                        self.check_type_alias_body_for_missing_names_after_type_node_check(
                            mapped.name_type,
                        );
                    }
                    if mapped.type_node.is_some() {
                        self.check_type_alias_body_for_missing_names_after_type_node_check(
                            mapped.type_node,
                        );
                    } else if self.ctx.no_implicit_any() {
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                        self.error_at_node(
                            type_idx,
                            diagnostic_messages::MAPPED_OBJECT_TYPE_IMPLICITLY_HAS_AN_ANY_TEMPLATE_TYPE,
                            diagnostic_codes::MAPPED_OBJECT_TYPE_IMPLICITLY_HAS_AN_ANY_TEMPLATE_TYPE,
                        );
                    }
                    if let Some(ref members) = mapped.members {
                        let member_nodes = members.nodes.clone();
                        for member_idx in member_nodes {
                            self.check_type_member_for_missing_names(member_idx);
                        }
                    }
                    self.pop_mapped_type_param_provisional(param_binding);
                }
            }
            k if k == syntax_kind_ext::TYPE_PREDICATE => {
                if let Some(pred) = self.ctx.arena.get_type_predicate(node)
                    && pred.type_node.is_some()
                {
                    self.check_type_alias_body_for_missing_names_after_type_node_check(
                        pred.type_node,
                    );
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                if let Some(template) = self.ctx.arena.get_template_literal_type(node) {
                    let spans = template.template_spans.nodes.clone();
                    for span_idx in spans {
                        let Some(span_node) = self.ctx.arena.get(span_idx) else {
                            continue;
                        };
                        let Some(span) = self.ctx.arena.get_template_span(span_node) else {
                            continue;
                        };
                        self.check_type_alias_body_for_missing_names_after_type_node_check(
                            span.expression,
                        );
                    }
                }
            }
            _ => self.check_type_for_missing_names(type_idx),
        }
    }

    fn check_type_alias_body_type_reference_name(&mut self, type_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(type_idx) else {
            return;
        };
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return;
        };
        let type_name = type_ref.type_name;
        let type_arguments = type_ref.type_arguments.clone();
        if self.type_ref_is_bare_scoped_type_parameter(type_name, type_arguments.as_ref()) {
            return;
        }

        if !self.ctx.symbol_resolution_set.is_empty()
            && let Some(sym_id) = self
                .resolve_type_symbol_for_lowering(type_name)
                .map(tsz_binder::SymbolId)
            && self.ctx.symbol_resolution_set.contains(&sym_id)
        {
            self.check_type_alias_body_type_reference_args(type_arguments.as_ref());
            return;
        }

        let Some(name_node) = self.ctx.arena.get(type_name) else {
            return;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            let _ = self.get_type_from_type_reference(type_idx);
            return;
        };
        let name = ident.escaped_text.clone();
        let shadows_managed_array = matches!(name.as_str(), "Array" | "ReadonlyArray")
            && self.ctx.file_local_type_shadow_for_lib_name(name.as_str());
        let primitive_type = matches!(
            name.as_str(),
            "any"
                | "unknown"
                | "never"
                | "void"
                | "undefined"
                | "null"
                | "boolean"
                | "number"
                | "bigint"
                | "string"
                | "symbol"
                | "object"
        );
        if (tsz_solver::is_compiler_managed_type(name.as_str()) || primitive_type)
            && !shadows_managed_array
        {
            self.check_type_alias_body_type_reference_args(type_arguments.as_ref());
            return;
        }

        match self.resolve_identifier_symbol_in_type_position(type_name) {
            TypeSymbolResolution::Type(sym_id) => {
                if let Some(args) = type_arguments.as_ref()
                    && !self.is_inside_type_parameter_declaration(type_idx)
                {
                    self.validate_type_reference_type_arguments(sym_id, args, type_idx);
                }
            }
            TypeSymbolResolution::ValueOnly(_) => {
                if self
                    .resolve_type_symbol_for_lowering(type_name)
                    .map(tsz_binder::SymbolId)
                    .or_else(|| self.resolve_type_only_import_alias_target_symbol(&name))
                    .is_none()
                {
                    self.report_wrong_meaning_diagnostic(&name, type_name, NameLookupKind::Value);
                }
            }
            TypeSymbolResolution::NotFound => {
                if !self.is_unresolved_import_symbol(type_name) {
                    let _ = self.resolve_type_name_or_report(&name, type_name);
                }
            }
        }

        self.check_type_alias_body_type_reference_args(type_arguments.as_ref());
    }

    fn check_type_alias_body_type_reference_args(
        &mut self,
        args: Option<&tsz_parser::parser::base::NodeList>,
    ) {
        if let Some(args) = args {
            let arg_nodes = args.nodes.clone();
            for arg_idx in arg_nodes {
                self.check_type_alias_body_for_missing_names_after_type_node_check(arg_idx);
            }
        }
    }
}
