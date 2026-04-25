//! Contextual type resolution helpers for hover.
//!
//! These methods resolve contextual types for object literal properties,
//! function parameters, and array elements by walking the AST to find
//! type annotations, type assertions, and interface/type alias declarations.

use super::{HoverInfo, HoverProvider, format};
use tsz_checker::state::CheckerState;
use tsz_common::position::Range;
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;

impl<'a> HoverProvider<'a> {
    pub(crate) fn hover_for_contextual_object_property(
        &self,
        node_idx: NodeIndex,
        type_cache: &mut Option<tsz_checker::TypeCache>,
    ) -> Option<HoverInfo> {
        use tsz_parser::syntax_kind_ext;

        let mut current = node_idx;
        let mut prop_assign_idx = NodeIndex::NONE;
        while current.is_some() {
            let current_node = self.arena.get(current)?;
            if current_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                prop_assign_idx = current;
                break;
            }
            current = self.arena.get_extended(current)?.parent;
        }
        if !prop_assign_idx.is_some() {
            return None;
        }

        let prop_assign_node = self.arena.get(prop_assign_idx)?;
        let prop_assign = self.arena.get_property_assignment(prop_assign_node)?;
        if !prop_assign.initializer.is_some() {
            return None;
        }

        let object_literal_idx = self.arena.get_extended(prop_assign_idx)?.parent;
        let object_literal = self.arena.get(object_literal_idx)?;
        if object_literal.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }

        let prop_name = self
            .arena
            .get_identifier_text(prop_assign.name)
            .map(std::string::ToString::to_string)?;

        let compiler_options = tsz_checker::context::CheckerOptions {
            strict: self.strict,
            no_implicit_any: self.strict,
            no_implicit_returns: false,
            no_implicit_this: self.strict,
            strict_null_checks: self.strict,
            strict_function_types: self.strict,
            strict_property_initialization: self.strict,
            use_unknown_in_catch_variables: self.strict,
            sound_mode: self.sound_mode,
            isolated_modules: false,
            ..Default::default()
        };
        let mut checker = if let Some(cache) = type_cache.take() {
            CheckerState::with_cache(
                self.arena,
                self.binder,
                self.interner,
                self.file_name.clone(),
                cache,
                compiler_options,
            )
        } else {
            CheckerState::new(
                self.arena,
                self.binder,
                self.interner,
                self.file_name.clone(),
                compiler_options,
            )
        };
        self.apply_lib_contexts(&mut checker);

        let contextual_type_idx =
            self.contextual_type_for_object_literal(object_literal_idx, prop_assign_idx);
        let (container_type_id, container_type_text) = if let Some(type_idx) = contextual_type_idx {
            let type_id = checker.get_type_of_node(type_idx);
            let text = checker.format_type(type_id);
            (type_id, text)
        } else {
            let (type_id, text) = self.contextual_container_type_from_binary_assignment(
                object_literal_idx,
                &mut checker,
            )?;
            (type_id, text)
        };
        let mut value_type_id = self
            .contextual_property_type_from_type(container_type_id, &prop_name)
            .unwrap_or(tsz_solver::TypeId::ERROR);
        let value_type_text = if value_type_id == tsz_solver::TypeId::ERROR {
            contextual_type_idx
                .and_then(|type_idx| self.contextual_property_annotation_text(type_idx, &prop_name))
        } else {
            None
        };
        if value_type_id == tsz_solver::TypeId::ERROR && value_type_text.is_none() {
            value_type_id = checker.get_type_of_node(prop_assign.initializer);
            if value_type_id == tsz_solver::TypeId::ERROR {
                value_type_id = checker.get_type_of_node(prop_assign_idx);
            }
        }
        let container_type = container_type_text;
        let value_type = value_type_text.unwrap_or_else(|| checker.format_type(value_type_id));
        *type_cache = Some(checker.extract_cache());

        if container_type.is_empty() || value_type.is_empty() {
            return None;
        }

        let initializer_node = self.arena.get(prop_assign.initializer)?;
        let is_function_like = initializer_node.kind
            == tsz_parser::syntax_kind_ext::FUNCTION_EXPRESSION
            || initializer_node.kind == tsz_parser::syntax_kind_ext::ARROW_FUNCTION;
        let (display_string, kind) = if is_function_like {
            let signature = contextual_type_idx
                .and_then(|type_idx| self.contextual_method_signature_text(type_idx, &prop_name))
                .unwrap_or_else(|| format::arrow_to_colon(&value_type));
            (
                format!("(method) {container_type}.{prop_name}{signature}"),
                "method".to_string(),
            )
        } else {
            (
                format!("(property) {container_type}.{prop_name}: {value_type}"),
                "property".to_string(),
            )
        };
        let name_node = self.arena.get(prop_assign.name)?;
        let start = self
            .line_map
            .offset_to_position(name_node.pos, self.source_text);
        let end = self
            .line_map
            .offset_to_position(name_node.end, self.source_text);
        Some(HoverInfo {
            contents: vec![format!("```typescript\n{display_string}\n```")],
            range: Some(Range::new(start, end)),
            display_string,
            kind,
            kind_modifiers: String::new(),
            documentation: String::new(),
            tags: Vec::new(),
        })
    }

    fn contextual_container_type_from_binary_assignment(
        &self,
        object_literal_idx: NodeIndex,
        checker: &mut CheckerState<'_>,
    ) -> Option<(tsz_solver::TypeId, String)> {
        use tsz_parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let parent_idx = self.arena.get_extended(object_literal_idx)?.parent;
        let parent_node = self.arena.get(parent_idx)?;
        if parent_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let binary = self.arena.get_binary_expr(parent_node)?;
        if binary.operator_token != SyntaxKind::EqualsToken as u16
            || binary.right != object_literal_idx
        {
            return None;
        }

        let left_node = self.arena.get(binary.left)?;
        if left_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(left_node)?;
        let prop_name = self
            .arena
            .get_identifier_text(access.name_or_argument)
            .map(std::string::ToString::to_string)?;
        let receiver_type_id = checker.get_type_of_node(access.expression);
        let container_type_id = self
            .contextual_property_type_from_type(receiver_type_id, &prop_name)
            .unwrap_or(tsz_solver::TypeId::ERROR);
        if container_type_id == tsz_solver::TypeId::ERROR {
            return None;
        }
        let container_type_text = checker.format_type(container_type_id);
        if container_type_text.is_empty() || container_type_text == "error" {
            return None;
        }
        Some((container_type_id, container_type_text))
    }

    pub(crate) fn contextual_property_type_from_type(
        &self,
        container_type_id: tsz_solver::TypeId,
        prop_name: &str,
    ) -> Option<tsz_solver::TypeId> {
        use tsz_solver::visitor;

        if let Some(shape_id) = visitor::object_shape_id(self.interner, container_type_id)
            .or_else(|| visitor::object_with_index_shape_id(self.interner, container_type_id))
        {
            let shape = self.interner.object_shape(shape_id);
            for prop in &shape.properties {
                if self.interner.resolve_atom(prop.name) == prop_name {
                    return Some(prop.type_id);
                }
            }
        }

        if let Some(list_id) = visitor::union_list_id(self.interner, container_type_id)
            .or_else(|| visitor::intersection_list_id(self.interner, container_type_id))
        {
            for &member in self.interner.type_list(list_id).iter() {
                if let Some(member_type) =
                    self.contextual_property_type_from_type(member, prop_name)
                {
                    return Some(member_type);
                }
            }
        }

        if let Some(app_id) = visitor::application_id(self.interner, container_type_id) {
            let app = self.interner.type_application(app_id);
            return self.contextual_property_type_from_type(app.base, prop_name);
        }

        None
    }

    fn contextual_method_signature_text(
        &self,
        contextual_type_idx: NodeIndex,
        prop_name: &str,
    ) -> Option<String> {
        use tsz_parser::syntax_kind_ext;

        let contextual_type_idx = self.unwrap_parenthesized_type_node(contextual_type_idx)?;
        let contextual_node = self.arena.get(contextual_type_idx)?;

        if contextual_node.kind == syntax_kind_ext::TYPE_LITERAL {
            let literal = self.arena.get_type_literal(contextual_node)?;
            for &member_idx in &literal.members.nodes {
                if let Some(sig_text) =
                    self.signature_text_if_matching_member(member_idx, prop_name)
                {
                    return Some(sig_text);
                }
            }
            return None;
        }

        if contextual_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let type_ref = self.arena.get_type_ref(contextual_node)?;
        let target = type_ref.type_name;
        let sym_id = self
            .binder
            .node_symbols
            .get(&target.0)
            .copied()
            .or_else(|| self.binder.resolve_identifier(self.arena, target))?;
        let symbol = self.binder.symbols.get(sym_id)?;

        for &decl_idx in &symbol.declarations {
            let decl_node = self.arena.get(decl_idx)?;
            if decl_node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
                let iface = self.arena.get_interface(decl_node)?;
                for &member_idx in &iface.members.nodes {
                    if let Some(sig_text) =
                        self.signature_text_if_matching_member(member_idx, prop_name)
                    {
                        return Some(sig_text);
                    }
                }
            } else if decl_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                let alias = self.arena.get_type_alias(decl_node)?;
                if let Some(sig_text) =
                    self.signature_text_if_matching_type_literal(alias.type_node, prop_name)
                {
                    return Some(sig_text);
                }
            }
        }

        None
    }

    fn contextual_property_annotation_text(
        &self,
        contextual_type_idx: NodeIndex,
        prop_name: &str,
    ) -> Option<String> {
        use tsz_parser::syntax_kind_ext;

        let contextual_type_idx = self.unwrap_parenthesized_type_node(contextual_type_idx)?;
        let contextual_node = self.arena.get(contextual_type_idx)?;

        if contextual_node.kind == syntax_kind_ext::TYPE_LITERAL {
            let literal = self.arena.get_type_literal(contextual_node)?;
            for &member_idx in &literal.members.nodes {
                if let Some(type_text) =
                    self.property_type_text_if_matching_member(member_idx, prop_name)
                {
                    return Some(type_text);
                }
            }
            return None;
        }

        if contextual_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }

        let type_ref = self.arena.get_type_ref(contextual_node)?;
        let target = type_ref.type_name;
        let sym_id = self
            .binder
            .node_symbols
            .get(&target.0)
            .copied()
            .or_else(|| self.binder.resolve_identifier(self.arena, target))?;
        let symbol = self.binder.symbols.get(sym_id)?;

        for &decl_idx in &symbol.declarations {
            let decl_node = self.arena.get(decl_idx)?;
            if decl_node.kind == syntax_kind_ext::INTERFACE_DECLARATION {
                let iface = self.arena.get_interface(decl_node)?;
                for &member_idx in &iface.members.nodes {
                    if let Some(type_text) =
                        self.property_type_text_if_matching_member(member_idx, prop_name)
                    {
                        return Some(type_text);
                    }
                }
            } else if decl_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                let alias = self.arena.get_type_alias(decl_node)?;
                if let Some(type_text) =
                    self.property_type_text_if_matching_type_literal(alias.type_node, prop_name)
                {
                    return Some(type_text);
                }
            }
        }

        None
    }

    fn property_type_text_if_matching_type_literal(
        &self,
        type_node_idx: NodeIndex,
        prop_name: &str,
    ) -> Option<String> {
        use tsz_parser::syntax_kind_ext;

        let type_node_idx = self.unwrap_parenthesized_type_node(type_node_idx)?;
        let type_node = self.arena.get(type_node_idx)?;
        if type_node.kind != syntax_kind_ext::TYPE_LITERAL {
            return None;
        }
        let literal = self.arena.get_type_literal(type_node)?;
        for &member_idx in &literal.members.nodes {
            if let Some(type_text) =
                self.property_type_text_if_matching_member(member_idx, prop_name)
            {
                return Some(type_text);
            }
        }
        None
    }

    fn property_type_text_if_matching_member(
        &self,
        member_idx: NodeIndex,
        prop_name: &str,
    ) -> Option<String> {
        let member_node = self.arena.get(member_idx)?;
        let signature = self.arena.get_signature(member_node)?;
        let name = self
            .arena
            .get_identifier_text(signature.name)
            .or_else(|| self.arena.get_literal_text(signature.name))?;
        if name != prop_name
            || !signature
                .parameters
                .as_ref()
                .is_none_or(|p| p.nodes.is_empty())
        {
            return None;
        }
        if !signature.type_annotation.is_some() {
            return None;
        }
        self.type_node_text(signature.type_annotation)
            .map(Self::normalize_annotation_text)
    }

    fn signature_text_if_matching_type_literal(
        &self,
        type_node_idx: NodeIndex,
        prop_name: &str,
    ) -> Option<String> {
        use tsz_parser::syntax_kind_ext;

        let type_node_idx = self.unwrap_parenthesized_type_node(type_node_idx)?;
        let type_node = self.arena.get(type_node_idx)?;
        if type_node.kind != syntax_kind_ext::TYPE_LITERAL {
            return None;
        }
        let literal = self.arena.get_type_literal(type_node)?;
        for &member_idx in &literal.members.nodes {
            if let Some(sig_text) = self.signature_text_if_matching_member(member_idx, prop_name) {
                return Some(sig_text);
            }
        }
        None
    }

    fn signature_text_if_matching_member(
        &self,
        member_idx: NodeIndex,
        prop_name: &str,
    ) -> Option<String> {
        let member_node = self.arena.get(member_idx)?;
        let signature = self.arena.get_signature(member_node)?;
        let name = self
            .arena
            .get_identifier_text(signature.name)
            .or_else(|| self.arena.get_literal_text(signature.name))?;
        if name != prop_name {
            return None;
        }
        self.signature_data_to_text(signature)
    }

    fn signature_data_to_text(
        &self,
        signature: &tsz_parser::parser::node::SignatureData,
    ) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(params) = signature.parameters.as_ref() {
            for &param_idx in &params.nodes {
                let param_node = self.arena.get(param_idx)?;
                let param = self.arena.get_parameter(param_node)?;
                let name = self
                    .arena
                    .get_identifier_text(param.name)
                    .or_else(|| self.arena.get_literal_text(param.name))
                    .unwrap_or("arg");
                let ty = if param.type_annotation.is_some() {
                    self.type_node_text(param.type_annotation)
                        .map(Self::normalize_annotation_text)?
                } else {
                    "any".to_string()
                };
                parts.push(format!("{name}: {ty}"));
            }
        }

        let ret = if signature.type_annotation.is_some() {
            self.type_node_text(signature.type_annotation)
                .map(Self::normalize_annotation_text)?
        } else {
            "any".to_string()
        };
        Some(format!("({}): {ret}", parts.join(", ")))
    }

    fn contextual_type_for_object_literal(
        &self,
        object_literal_idx: NodeIndex,
        property_assignment_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        use tsz_parser::syntax_kind_ext;

        let parent_idx = self.arena.get_extended(object_literal_idx)?.parent;
        let parent = self.arena.get(parent_idx)?;

        if parent.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let decl = self.arena.get_variable_declaration(parent)?;
            if decl.initializer == object_literal_idx && decl.type_annotation.is_some() {
                return Some(decl.type_annotation);
            }
        }

        if parent.kind == syntax_kind_ext::TYPE_ASSERTION
            || parent.kind == syntax_kind_ext::AS_EXPRESSION
            || parent.kind == syntax_kind_ext::SATISFIES_EXPRESSION
        {
            let assertion = self.arena.get_type_assertion(parent)?;
            if assertion.expression == object_literal_idx {
                return Some(assertion.type_node);
            }
        }

        if parent.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
            && let Some(grand_parent_idx) = self.arena.get_extended(parent_idx).map(|e| e.parent)
            && grand_parent_idx.is_some()
        {
            return self.contextual_type_for_object_literal(parent_idx, property_assignment_idx);
        }

        if parent.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
            && parent_idx == property_assignment_idx
            && let Some(grand_parent_idx) = self.arena.get_extended(parent_idx).map(|e| e.parent)
            && grand_parent_idx.is_some()
        {
            let grand_parent = self.arena.get(grand_parent_idx)?;
            if grand_parent.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                return self.contextual_type_for_object_literal(grand_parent_idx, parent_idx);
            }
        }

        None
    }

    pub(crate) fn contextual_parameter_annotation_text(
        &self,
        param_decl_idx: NodeIndex,
    ) -> Option<String> {
        use tsz_parser::syntax_kind_ext;

        let param_node = self.arena.get(param_decl_idx)?;
        let param = self.arena.get_parameter(param_node)?;
        if param.type_annotation.is_some() {
            return None;
        }

        let fn_idx = self.arena.get_extended(param_decl_idx)?.parent;
        let fn_node = self.arena.get(fn_idx)?;
        let fn_data = self.arena.get_function(fn_node)?;
        let param_index = fn_data
            .parameters
            .nodes
            .iter()
            .position(|&idx| idx == param_decl_idx)?;

        if let Some(contextual_type_node) =
            self.contextual_function_type_node_for_expression(fn_idx)
        {
            let contextual_node = self.arena.get(contextual_type_node)?;
            let contextual_param_idx =
                if let Some(fn_type) = self.arena.get_function_type(contextual_node) {
                    *fn_type.parameters.nodes.get(param_index)?
                } else if let Some(signature) = self.arena.get_signature(contextual_node) {
                    let params = signature.parameters.as_ref()?;
                    *params.nodes.get(param_index)?
                } else {
                    return None;
                };
            let contextual_param_node = self.arena.get(contextual_param_idx)?;
            let contextual_param = self.arena.get_parameter(contextual_param_node)?;
            if !contextual_param.type_annotation.is_some() {
                return None;
            }

            return self
                .type_node_text(contextual_param.type_annotation)
                .map(Self::normalize_annotation_text);
        }

        if self.function_expression_has_global_function_context(fn_idx) {
            return Some("any".to_string());
        }

        let mut current_fn_expr = fn_idx;
        while current_fn_expr.is_some() {
            let parent_idx = self.arena.get_extended(current_fn_expr)?.parent;
            let parent_node = self.arena.get(parent_idx)?;
            if parent_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                current_fn_expr = parent_idx;
                continue;
            }
            break;
        }

        if let Some(call_argument_type) = self
            .contextual_parameter_type_from_call_argument_expression(current_fn_expr, param_index)
        {
            return Some(call_argument_type);
        }

        self.contextual_parameter_type_from_property_assignment(current_fn_expr, param_index)
    }

    fn function_expression_has_global_function_context(&self, expr_idx: NodeIndex) -> bool {
        use tsz_parser::syntax_kind_ext;

        let mut current = expr_idx;
        while current.is_some() {
            let ext = match self.arena.get_extended(current) {
                Some(ext) => ext,
                None => return false,
            };
            let parent_idx = ext.parent;
            if !parent_idx.is_some() {
                return false;
            }
            let Some(parent) = self.arena.get(parent_idx) else {
                return false;
            };

            if parent.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                current = parent_idx;
                continue;
            }

            let annotation_idx = if parent.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                self.arena
                    .get_variable_declaration(parent)
                    .filter(|decl| decl.initializer == current && decl.type_annotation.is_some())
                    .map(|decl| decl.type_annotation)
            } else if parent.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                self.arena
                    .get_property_decl(parent)
                    .filter(|decl| decl.initializer == current && decl.type_annotation.is_some())
                    .map(|decl| decl.type_annotation)
            } else {
                None
            };

            return annotation_idx
                .and_then(|type_idx| self.type_node_text(type_idx))
                .map(Self::normalize_annotation_text)
                .is_some_and(|text| text == "Function");
        }

        false
    }

    pub(crate) fn property_declaration_annotation_text(
        &self,
        decl_node_idx: NodeIndex,
    ) -> Option<String> {
        if !decl_node_idx.is_some() {
            return None;
        }
        let decl_node = self.arena.get(decl_node_idx)?;
        let property_decl = self.arena.get_property_decl(decl_node)?;
        if !property_decl.type_annotation.is_some() {
            return None;
        }
        self.type_node_text(property_decl.type_annotation)
            .map(Self::normalize_annotation_text)
    }

    pub(crate) fn normalize_annotation_text(text: String) -> String {
        text.trim_end()
            .trim_end_matches([',', ';', '='])
            .trim_end()
            .to_string()
    }

    fn contextual_function_type_node_for_expression(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        use tsz_parser::syntax_kind_ext;

        let mut current = expr_idx;
        while current.is_some() {
            let ext = self.arena.get_extended(current)?;
            let parent_idx = ext.parent;
            if !parent_idx.is_some() {
                return None;
            }
            let parent = self.arena.get(parent_idx)?;

            if parent.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                current = parent_idx;
                continue;
            }

            if parent.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                && let Some(callable_type) =
                    self.contextual_callable_type_for_array_element(parent_idx, current)
            {
                return Some(callable_type);
            }

            if parent.kind == syntax_kind_ext::CALL_EXPRESSION
                && let Some(callable_type) =
                    self.contextual_callable_type_for_call_argument(parent_idx, current)
            {
                return Some(callable_type);
            }

            if parent.kind == syntax_kind_ext::TYPE_ASSERTION
                || parent.kind == syntax_kind_ext::AS_EXPRESSION
                || parent.kind == syntax_kind_ext::SATISFIES_EXPRESSION
            {
                let assertion = self.arena.get_type_assertion(parent)?;
                if assertion.expression == current {
                    let type_node = self.arena.get(assertion.type_node)?;
                    if type_node.kind == syntax_kind_ext::FUNCTION_TYPE {
                        return Some(assertion.type_node);
                    }
                }
            }

            if parent.kind == syntax_kind_ext::VARIABLE_DECLARATION
                && let Some(decl) = self.arena.get_variable_declaration(parent)
                && decl.initializer == current
                && decl.type_annotation.is_some()
                && let Some(type_node) = self.arena.get(decl.type_annotation)
                && type_node.kind == syntax_kind_ext::FUNCTION_TYPE
            {
                return Some(decl.type_annotation);
            }

            if parent.kind == syntax_kind_ext::PROPERTY_DECLARATION
                && let Some(decl) = self.arena.get_property_decl(parent)
                && decl.initializer == current
                && decl.type_annotation.is_some()
                && let Some(type_node) = self.arena.get(decl.type_annotation)
                && type_node.kind == syntax_kind_ext::FUNCTION_TYPE
            {
                return Some(decl.type_annotation);
            }

            current = parent_idx;
        }

        None
    }

    fn contextual_callable_type_for_array_element(
        &self,
        array_literal_idx: NodeIndex,
        element_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        use tsz_parser::syntax_kind_ext;

        let array_literal_node = self.arena.get(array_literal_idx)?;
        let literal_expr = self.arena.get_literal_expr(array_literal_node)?;
        let element_index = literal_expr
            .elements
            .nodes
            .iter()
            .position(|&idx| idx == element_idx)?;

        let array_ext = self.arena.get_extended(array_literal_idx)?;
        let container_idx = array_ext.parent;
        if !container_idx.is_some() {
            return None;
        }
        let container_node = self.arena.get(container_idx)?;

        let annotation_type_idx = if container_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let decl = self.arena.get_variable_declaration(container_node)?;
            if decl.initializer != array_literal_idx || !decl.type_annotation.is_some() {
                return None;
            }
            decl.type_annotation
        } else if container_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
            let decl = self.arena.get_property_decl(container_node)?;
            if decl.initializer != array_literal_idx || !decl.type_annotation.is_some() {
                return None;
            }
            decl.type_annotation
        } else if container_node.kind == syntax_kind_ext::TYPE_ASSERTION
            || container_node.kind == syntax_kind_ext::AS_EXPRESSION
            || container_node.kind == syntax_kind_ext::SATISFIES_EXPRESSION
        {
            let assertion = self.arena.get_type_assertion(container_node)?;
            if assertion.expression != array_literal_idx {
                return None;
            }
            assertion.type_node
        } else {
            return None;
        };

        self.callable_type_from_array_annotation(annotation_type_idx, element_index)
    }

    fn contextual_callable_type_for_call_argument(
        &self,
        call_expr_idx: NodeIndex,
        argument_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        use tsz_parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let call_node = self.arena.get(call_expr_idx)?;
        let call = self.arena.get_call_expr(call_node)?;
        let arg_position = call
            .arguments
            .as_ref()?
            .nodes
            .iter()
            .position(|&idx| idx == argument_idx)?;
        let callee = self.arena.get(call.expression)?;
        if callee.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self
            .binder
            .node_symbols
            .get(&call.expression.0)
            .copied()
            .or_else(|| self.binder.resolve_identifier(self.arena, call.expression))?;
        let symbol = self.binder.symbols.get(sym_id)?;
        for &decl_idx in &symbol.declarations {
            let decl_node = self.arena.get(decl_idx)?;
            if decl_node.kind != syntax_kind_ext::FUNCTION_DECLARATION
                && decl_node.kind != syntax_kind_ext::METHOD_DECLARATION
                && decl_node.kind != syntax_kind_ext::METHOD_SIGNATURE
            {
                continue;
            }
            let params = if let Some(function) = self.arena.get_function(decl_node) {
                &function.parameters.nodes
            } else if let Some(signature) = self.arena.get_signature(decl_node) {
                let list = signature.parameters.as_ref()?;
                &list.nodes
            } else {
                continue;
            };
            let &param_idx = params.get(arg_position)?;
            let param_node = self.arena.get(param_idx)?;
            let param = self.arena.get_parameter(param_node)?;
            if param.type_annotation.is_some() {
                return Some(param.type_annotation);
            }
        }
        None
    }

    fn callable_type_from_array_annotation(
        &self,
        annotation_type_idx: NodeIndex,
        element_index: usize,
    ) -> Option<NodeIndex> {
        use tsz_parser::syntax_kind_ext;

        let annotation_type_idx = self.unwrap_parenthesized_type_node(annotation_type_idx)?;
        let annotation_type_node = self.arena.get(annotation_type_idx)?;

        if annotation_type_node.kind == syntax_kind_ext::ARRAY_TYPE {
            let array_type = self.arena.get_array_type(annotation_type_node)?;
            return self.callable_type_node_from_type_node(array_type.element_type);
        }

        if annotation_type_node.kind == syntax_kind_ext::TUPLE_TYPE {
            let tuple_type = self.arena.get_tuple_type(annotation_type_node)?;
            let element_type = *tuple_type.elements.nodes.get(element_index)?;
            return self.callable_type_node_from_type_node(element_type);
        }

        None
    }

    fn callable_type_node_from_type_node(&self, type_idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::syntax_kind_ext;

        let type_idx = self.unwrap_parenthesized_type_node(type_idx)?;
        let type_node = self.arena.get(type_idx)?;

        if type_node.kind == syntax_kind_ext::FUNCTION_TYPE {
            return Some(type_idx);
        }

        if type_node.kind == syntax_kind_ext::TYPE_LITERAL {
            let type_literal = self.arena.get_type_literal(type_node)?;
            for &member_idx in &type_literal.members.nodes {
                let member_node = self.arena.get(member_idx)?;
                if member_node.kind == syntax_kind_ext::CALL_SIGNATURE {
                    return Some(member_idx);
                }
            }
        }

        None
    }

    fn contextual_parameter_type_from_property_assignment(
        &self,
        rhs_expr_idx: NodeIndex,
        param_index: usize,
    ) -> Option<String> {
        use tsz_parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let parent_idx = self.arena.get_extended(rhs_expr_idx)?.parent;
        let parent_node = self.arena.get(parent_idx)?;
        if parent_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let binary = self.arena.get_binary_expr(parent_node)?;
        if binary.operator_token != SyntaxKind::EqualsToken as u16 || binary.right != rhs_expr_idx {
            return None;
        }

        let left_node = self.arena.get(binary.left)?;
        if left_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(left_node)?;
        let prop_name = self
            .arena
            .get_identifier_text(access.name_or_argument)
            .map(str::to_string)?;

        let compiler_options = tsz_checker::context::CheckerOptions {
            strict: self.strict,
            no_implicit_any: self.strict,
            no_implicit_returns: false,
            no_implicit_this: self.strict,
            strict_null_checks: self.strict,
            strict_function_types: self.strict,
            strict_property_initialization: self.strict,
            use_unknown_in_catch_variables: self.strict,
            sound_mode: self.sound_mode,
            isolated_modules: false,
            ..Default::default()
        };
        let mut checker = CheckerState::new(
            self.arena,
            self.binder,
            self.interner,
            self.file_name.clone(),
            compiler_options,
        );
        self.apply_lib_contexts(&mut checker);

        let expr_type_id = checker.get_type_of_node(access.expression);
        let member_type_id = self
            .contextual_property_type_from_type(expr_type_id, &prop_name)
            .unwrap_or_else(|| checker.get_type_of_node(access.name_or_argument));
        if member_type_id == tsz_solver::TypeId::ERROR {
            return None;
        }

        if let Some(function_shape_id) =
            tsz_solver::visitor::function_shape_id(self.interner, member_type_id)
        {
            let function = self.interner.function_shape(function_shape_id);
            let param = function.params.get(param_index)?;
            let text = checker.format_type(param.type_id);
            return (!text.is_empty() && text != "error").then_some(text);
        }

        if let Some(callable_shape_id) =
            tsz_solver::visitor::callable_shape_id(self.interner, member_type_id)
        {
            let callable = self.interner.callable_shape(callable_shape_id);
            let signature = callable.call_signatures.first()?;
            let param = signature.params.get(param_index)?;
            let text = checker.format_type(param.type_id);
            return (!text.is_empty() && text != "error").then_some(text);
        }

        None
    }

    fn contextual_parameter_type_from_call_argument_expression(
        &self,
        mut fn_expr_idx: NodeIndex,
        param_index: usize,
    ) -> Option<String> {
        use tsz_parser::syntax_kind_ext;

        while fn_expr_idx.is_some() {
            let parent_idx = self.arena.get_extended(fn_expr_idx)?.parent;
            let parent_node = self.arena.get(parent_idx)?;
            if parent_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                fn_expr_idx = parent_idx;
                continue;
            }
            if parent_node.kind != syntax_kind_ext::CALL_EXPRESSION {
                return None;
            }

            let call = self.arena.get_call_expr(parent_node)?;
            let arg_position = call
                .arguments
                .as_ref()?
                .nodes
                .iter()
                .position(|&idx| idx == fn_expr_idx)?;

            let compiler_options = tsz_checker::context::CheckerOptions {
                strict: self.strict,
                no_implicit_any: self.strict,
                no_implicit_returns: false,
                no_implicit_this: self.strict,
                strict_null_checks: self.strict,
                strict_function_types: self.strict,
                strict_property_initialization: self.strict,
                use_unknown_in_catch_variables: self.strict,
                sound_mode: self.sound_mode,
                isolated_modules: false,
                ..Default::default()
            };
            let mut checker = CheckerState::new(
                self.arena,
                self.binder,
                self.interner,
                self.file_name.clone(),
                compiler_options,
            );
            self.apply_lib_contexts(&mut checker);
            let callee_type_id = checker.get_type_of_node(call.expression);
            if let Some(function_shape_id) =
                tsz_solver::visitor::function_shape_id(self.interner, callee_type_id)
            {
                let function = self.interner.function_shape(function_shape_id);
                if let Some(param) = function.params.get(arg_position)
                    && let Some(callable_param) =
                        tsz_solver::visitor::callable_shape_id(self.interner, param.type_id)
                {
                    let callable_param_shape = self.interner.callable_shape(callable_param);
                    if let Some(inner_sig) = callable_param_shape.call_signatures.first()
                        && let Some(inner_param) = inner_sig.params.get(param_index)
                    {
                        let text = checker.format_type(inner_param.type_id);
                        if !text.is_empty() && text != "error" {
                            return Some(text);
                        }
                    }
                }
                if let Some(param) = function.params.get(arg_position)
                    && let Some(function_param) =
                        tsz_solver::visitor::function_shape_id(self.interner, param.type_id)
                {
                    let function_param_shape = self.interner.function_shape(function_param);
                    if let Some(inner_param) = function_param_shape.params.get(param_index) {
                        let text = checker.format_type(inner_param.type_id);
                        if !text.is_empty() && text != "error" {
                            return Some(text);
                        }
                    }
                }
            }
            if let Some(callable_shape_id) =
                tsz_solver::visitor::callable_shape_id(self.interner, callee_type_id)
            {
                let callable = self.interner.callable_shape(callable_shape_id);
                if let Some(signature) = callable
                    .call_signatures
                    .iter()
                    .find(|sig| sig.params.get(arg_position).is_some())
                    .or_else(|| callable.call_signatures.first())
                    && let Some(param) = signature.params.get(arg_position)
                    && let Some(callable_param) =
                        tsz_solver::visitor::callable_shape_id(self.interner, param.type_id)
                {
                    let callable_param_shape = self.interner.callable_shape(callable_param);
                    if let Some(inner_sig) = callable_param_shape.call_signatures.first()
                        && let Some(inner_param) = inner_sig.params.get(param_index)
                    {
                        let text = checker.format_type(inner_param.type_id);
                        if !text.is_empty() && text != "error" {
                            return Some(text);
                        }
                    }
                }
                if let Some(signature) = callable
                    .call_signatures
                    .iter()
                    .find(|sig| sig.params.get(arg_position).is_some())
                    .or_else(|| callable.call_signatures.first())
                    && let Some(param) = signature.params.get(arg_position)
                    && let Some(function_param) =
                        tsz_solver::visitor::function_shape_id(self.interner, param.type_id)
                {
                    let function_param_shape = self.interner.function_shape(function_param);
                    if let Some(inner_param) = function_param_shape.params.get(param_index) {
                        let text = checker.format_type(inner_param.type_id);
                        if !text.is_empty() && text != "error" {
                            return Some(text);
                        }
                    }
                }
            }

            return None;
        }
        None
    }

    pub(crate) fn unwrap_parenthesized_type_node(
        &self,
        mut type_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        use tsz_parser::syntax_kind_ext;

        loop {
            let type_node = self.arena.get(type_idx)?;
            if type_node.kind != syntax_kind_ext::PARENTHESIZED_TYPE {
                return Some(type_idx);
            }
            let wrapped = self.arena.get_wrapped_type(type_node)?;
            type_idx = wrapped.type_node;
        }
    }

    pub(crate) fn type_node_text(&self, type_node_idx: NodeIndex) -> Option<String> {
        let type_node = self.arena.get(type_node_idx)?;
        let start = type_node.pos as usize;
        let end = type_node.end.min(self.source_text.len() as u32) as usize;
        (start < end).then(|| {
            let mut text = self.source_text[start..end].trim().to_string();
            while text.ends_with(')') {
                let opens = text.chars().filter(|&c| c == '(').count();
                let closes = text.chars().filter(|&c| c == ')').count();
                if closes > opens {
                    text.pop();
                    text = text.trim_end().to_string();
                } else {
                    break;
                }
            }
            text
        })
    }
}
