//! Coverage checks for duplicate type-alias missing-name validation.

use crate::query_boundaries::name_resolution::NameLookupKind;
use crate::state::CheckerState;
use crate::symbol_resolver::TypeSymbolResolution;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    pub(crate) fn type_alias_body_missing_names_covered_by_type_node_checking(
        &self,
        root: NodeIndex,
    ) -> bool {
        let mut stack = vec![root];
        while let Some(node_idx) = stack.pop() {
            if node_idx == NodeIndex::NONE {
                continue;
            }
            let Some(node) = self.ctx.arena.get(node_idx) else {
                return false;
            };
            match node.kind {
                k if k == syntax_kind_ext::TYPE_REFERENCE => {
                    let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
                        return false;
                    };
                    if let Some(args) = &type_ref.type_arguments {
                        stack.extend(args.nodes.iter().copied());
                    }
                }
                k if k == syntax_kind_ext::UNION_TYPE
                    || k == syntax_kind_ext::INTERSECTION_TYPE =>
                {
                    let Some(composite) = self.ctx.arena.get_composite_type(node) else {
                        return false;
                    };
                    stack.extend(composite.types.nodes.iter().copied());
                }
                k if k == syntax_kind_ext::ARRAY_TYPE => {
                    let Some(array) = self.ctx.arena.get_array_type(node) else {
                        return false;
                    };
                    stack.push(array.element_type);
                }
                k if k == syntax_kind_ext::OPTIONAL_TYPE
                    || k == syntax_kind_ext::REST_TYPE
                    || k == syntax_kind_ext::PARENTHESIZED_TYPE =>
                {
                    let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) else {
                        return false;
                    };
                    stack.push(wrapped.type_node);
                }
                k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                    let Some(indexed) = self.ctx.arena.get_indexed_access_type(node) else {
                        return false;
                    };
                    stack.push(indexed.object_type);
                    stack.push(indexed.index_type);
                }
                _ => return false,
            }
        }
        true
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

    fn type_ref_is_bare_scoped_type_parameter(
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
}
