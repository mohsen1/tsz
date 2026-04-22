//! Property access semantic helpers: prototype reads, write target detection,
//! flow analysis, scope helpers, class/object member checks, union/type-parameter
//! property checks, strict bind/call/apply method synthesis, import.meta CJS
//! checks, and const expando key resolution.

use crate::FlowAnalyzer;
use crate::state::CheckerState;
use crate::symbols_domain::name_text::property_access_chain_text_in_arena;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::types_domain::property_access_helpers) fn is_js_prototype_read_root(
        &self,
        object_expr_idx: NodeIndex,
        property_name: &str,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(object_expr_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return false;
        };
        let Some(member_node) = self.ctx.arena.get(access.name_or_argument) else {
            return false;
        };
        let is_prototype = member_node.kind == SyntaxKind::Identifier as u16
            && self
                .ctx
                .arena
                .get_identifier(member_node)
                .is_some_and(|ident| ident.escaped_text == "prototype");
        if !is_prototype {
            return false;
        }

        let Some(root_name) = self.expression_text(access.expression) else {
            return false;
        };

        if self.class_has_instance_member(&root_name, property_name) {
            return false;
        }

        let Some(sym_id) = self.resolve_identifier_symbol(access.expression) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        (symbol.flags & (symbol_flags::FUNCTION | symbol_flags::CLASS)) != 0
    }

    pub(crate) fn property_access_is_write_target_or_base(
        &self,
        property_access_idx: NodeIndex,
    ) -> bool {
        let mut current = property_access_idx;

        loop {
            let Some(prop_ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent_idx = prop_ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };

            if (parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || parent_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                && let Some(access) = self.ctx.arena.get_access_expr(parent_node)
                && access.expression == current
            {
                current = parent_idx;
                continue;
            }

            if parent_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                if (parent_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                    || parent_node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION)
                    && let Some(unary) = self.ctx.arena.get_unary_expr(parent_node)
                {
                    return unary.operator == SyntaxKind::PlusPlusToken as u16
                        || unary.operator == SyntaxKind::MinusMinusToken as u16;
                }
                return false;
            }

            let Some(binary) = self.ctx.arena.get_binary_expr(parent_node) else {
                return false;
            };
            return binary.left == current && self.is_assignment_operator(binary.operator_token);
        }
    }

    pub(crate) fn property_access_is_direct_write_target(
        &self,
        property_access_idx: NodeIndex,
    ) -> bool {
        let Some(prop_ext) = self.ctx.arena.get_extended(property_access_idx) else {
            return false;
        };
        let parent_idx = prop_ext.parent;
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };

        if (parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || parent_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            && let Some(access) = self.ctx.arena.get_access_expr(parent_node)
            && access.expression == property_access_idx
        {
            return false;
        }

        if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(parent_node)
        {
            return binary.left == property_access_idx
                && self.is_assignment_operator(binary.operator_token);
        }

        if (parent_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || parent_node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION)
            && let Some(unary) = self.ctx.arena.get_unary_expr(parent_node)
        {
            return unary.operator == SyntaxKind::PlusPlusToken as u16
                || unary.operator == SyntaxKind::MinusMinusToken as u16;
        }

        false
    }

    pub(in crate::types_domain) fn flow_node_for_reference_usage(
        &self,
        idx: NodeIndex,
    ) -> Option<tsz_binder::FlowNodeId> {
        if let Some(flow) = self.ctx.binder.get_node_flow(idx) {
            return Some(flow);
        }

        let mut current = self.ctx.arena.get_extended(idx).map(|ext| ext.parent);
        while let Some(parent) = current {
            if parent.is_none() {
                break;
            }
            if let Some(flow) = self.ctx.binder.get_node_flow(parent) {
                return Some(flow);
            }
            current = self.ctx.arena.get_extended(parent).map(|ext| ext.parent);
        }

        None
    }

    pub(in crate::types_domain) fn flow_analyzer_for_property_reads(&self) -> FlowAnalyzer<'_> {
        FlowAnalyzer::with_node_types(
            self.ctx.arena,
            self.ctx.binder,
            self.ctx.types,
            &self.ctx.node_types,
        )
        .with_flow_cache(&self.ctx.flow_analysis_cache)
        .with_switch_reference_cache(&self.ctx.flow_switch_reference_cache)
        .with_numeric_atom_cache(&self.ctx.flow_numeric_atom_cache)
        .with_reference_match_cache(&self.ctx.flow_reference_match_cache)
        .with_type_environment(&self.ctx.type_environment)
        .with_narrowing_cache(&self.ctx.narrowing_cache)
        .with_call_type_predicates(&self.ctx.call_type_predicates)
        .with_flow_buffers(
            &self.ctx.flow_worklist,
            &self.ctx.flow_in_worklist,
            &self.ctx.flow_visited,
            &self.ctx.flow_results,
        )
        .with_destructured_bindings(&self.ctx.destructured_bindings)
    }

    pub(in crate::types_domain::property_access_helpers) fn expando_read_is_within_initializing_scope(
        &self,
        property_access_idx: NodeIndex,
        object_expr_idx: NodeIndex,
    ) -> bool {
        let use_owner = self.scope_owner_node(property_access_idx);
        let Some(root_ident) = self.root_identifier_index(object_expr_idx) else {
            return use_owner.is_none();
        };
        let Some(root_sym) = self.resolve_identifier_symbol(root_ident) else {
            return use_owner.is_none();
        };
        let Some(symbol) = self.ctx.binder.get_symbol(root_sym) else {
            return use_owner.is_none();
        };
        let decl_idx = symbol.primary_declaration().unwrap_or(NodeIndex::NONE);
        self.declaration_scope_owner_node(decl_idx) == use_owner
    }

    fn root_identifier_index(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return Some(idx);
        }
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.ctx.arena.get_access_expr(node)?;
            return self.root_identifier_index(access.expression);
        }
        None
    }

    fn scope_owner_node(&self, idx: NodeIndex) -> NodeIndex {
        let mut current = Some(idx);
        while let Some(node_idx) = current {
            if node_idx.is_none() {
                return NodeIndex::NONE;
            }
            let Some(node) = self.ctx.arena.get(node_idx) else {
                return NodeIndex::NONE;
            };
            if self.is_scope_owner_kind(node.kind) {
                return node_idx;
            }
            current = self.ctx.arena.get_extended(node_idx).map(|ext| ext.parent);
        }
        NodeIndex::NONE
    }

    fn declaration_scope_owner_node(&self, decl_idx: NodeIndex) -> NodeIndex {
        let current = self
            .ctx
            .arena
            .get_extended(decl_idx)
            .map(|ext| ext.parent)
            .unwrap_or(NodeIndex::NONE);
        self.scope_owner_node(current)
    }

    pub(in crate::types_domain::property_access_helpers) const fn is_scope_owner_kind(
        &self,
        kind: u16,
    ) -> bool {
        kind == syntax_kind_ext::FUNCTION_DECLARATION
            || kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || kind == syntax_kind_ext::ARROW_FUNCTION
            || kind == syntax_kind_ext::METHOD_DECLARATION
            || kind == syntax_kind_ext::CONSTRUCTOR
            || kind == syntax_kind_ext::GET_ACCESSOR
            || kind == syntax_kind_ext::SET_ACCESSOR
    }

    pub(in crate::types_domain::property_access_helpers) fn expando_read_is_self_default_initializer(
        &self,
        property_access_idx: NodeIndex,
    ) -> bool {
        let mut current = property_access_idx;
        loop {
            let Some(parent_idx) = self.ctx.arena.get_extended(current).map(|ext| ext.parent)
            else {
                return false;
            };
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };

            if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(binary) = self.ctx.arena.get_binary_expr(parent_node)
            {
                if matches!(
                    binary.operator_token,
                    op if op == SyntaxKind::BarBarToken as u16
                        || op == SyntaxKind::QuestionQuestionToken as u16
                ) && binary.left == current
                {
                    current = parent_idx;
                    continue;
                }

                return binary.operator_token == SyntaxKind::EqualsToken as u16
                    && binary.right == current
                    && self.same_reference(binary.left, property_access_idx);
            }

            return false;
        }
    }

    fn same_reference(&self, left: NodeIndex, right: NodeIndex) -> bool {
        let analyzer = self.flow_analyzer_for_property_reads();
        analyzer.is_matching_reference(left, right)
    }

    /// Check if a class has an instance member (property, method, or accessor) with the given name.
    /// Used to prevent expando property detection from masking TS2339 errors when accessing
    /// instance members on the class constructor type.
    pub(in crate::types_domain::property_access_helpers) fn class_has_instance_member(
        &self,
        obj_key: &str,
        property_name: &str,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        // Only check simple identifiers (not qualified chains like `a.B`)
        let root_name = obj_key.split('.').next().unwrap_or_default();
        if root_name != obj_key {
            return false;
        }

        let Some(sym_id) = self.ctx.binder.file_locals.get(root_name) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // Only check class declarations
        if (symbol.flags & symbol_flags::CLASS) == 0 {
            return false;
        }

        // Check the class's members table for the property name.
        // Members table stores instance members by name, so a match here
        // means the property is a declared instance member.
        if let Some(ref members) = symbol.members
            && members.get(property_name).is_some()
        {
            return true;
        }

        // Also check the class AST for accessor declarations (get/set),
        // which may not always be in the members table.
        for &decl_idx in &symbol.declarations {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if decl_node.kind != syntax_kind_ext::CLASS_DECLARATION
                && decl_node.kind != syntax_kind_ext::CLASS_EXPRESSION
            {
                continue;
            }
            let Some(class) = self.ctx.arena.get_class(decl_node) else {
                continue;
            };
            for &member_idx in &class.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let is_instance_member = match member_node.kind {
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                        .ctx
                        .arena
                        .get_property_decl(member_node)
                        .is_some_and(|p| {
                            !self.has_static_modifier(&p.modifiers)
                                && self
                                    .get_property_name(p.name)
                                    .is_some_and(|n| n == property_name)
                        }),
                    k if k == syntax_kind_ext::METHOD_DECLARATION => self
                        .ctx
                        .arena
                        .get_method_decl(member_node)
                        .is_some_and(|m| {
                            !self.has_static_modifier(&m.modifiers)
                                && self
                                    .get_property_name(m.name)
                                    .is_some_and(|n| n == property_name)
                        }),
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        self.ctx.arena.get_accessor(member_node).is_some_and(|a| {
                            !self.has_static_modifier(&a.modifiers)
                                && self
                                    .get_property_name(a.name)
                                    .is_some_and(|n| n == property_name)
                        })
                    }
                    _ => false,
                };
                if is_instance_member {
                    return true;
                }
            }
        }

        false
    }

    pub(in crate::types_domain::property_access_helpers) fn object_literal_root_declares_property(
        &self,
        object_expr_idx: NodeIndex,
        property_name: &str,
    ) -> bool {
        let Some(root_ident) = self.root_identifier_index(object_expr_idx) else {
            return false;
        };
        let Some(sym_id) = self.resolve_identifier_symbol(root_ident) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        if (symbol.flags & symbol_flags::VARIABLE) == 0 {
            return false;
        }

        let decl_idx = symbol.value_declaration;
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        let Some(init_node) = self.ctx.arena.get(var_decl.initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(obj_lit) = self.ctx.arena.get_literal_expr(init_node) else {
            return false;
        };

        obj_lit.elements.nodes.iter().copied().any(|elem_idx| {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                return false;
            };

            let elem_prop_name = match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_property_assignment(elem_node)
                    .and_then(|prop| self.get_property_name(prop.name)),
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_shorthand_property(elem_node)
                    .and_then(|prop| self.get_property_name(prop.name)),
                syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(elem_node)
                    .and_then(|method| self.get_property_name(method.name)),
                syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => self
                    .ctx
                    .arena
                    .get_accessor(elem_node)
                    .and_then(|accessor| self.get_property_name(accessor.name)),
                _ => None,
            };

            elem_prop_name.is_some_and(|name| name == property_name)
        })
    }

    pub(in crate::types_domain) fn union_has_explicit_property_member(
        &mut self,
        object_type: TypeId,
        prop_name: &str,
    ) -> bool {
        use crate::query_boundaries::common::PropertyAccessResult;

        let members =
            crate::query_boundaries::state::checking::union_members(self.ctx.types, object_type)
                .or_else(|| {
                    crate::query_boundaries::state::checking::intersection_members(
                        self.ctx.types,
                        object_type,
                    )
                });
        let Some(members) = members else {
            return false;
        };

        members.iter().copied().any(|member| {
            let resolved_member = self.resolve_type_for_property_access(member);
            matches!(
                self.resolve_property_access_with_env(resolved_member, prop_name),
                PropertyAccessResult::Success {
                    from_index_signature: false,
                    ..
                }
            )
        })
    }

    pub(in crate::types_domain) fn type_parameter_constraint_has_explicit_property(
        &mut self,
        object_type: TypeId,
        prop_name: &str,
    ) -> bool {
        use crate::query_boundaries::common::PropertyAccessResult;

        let Some(constraint) = crate::query_boundaries::state::checking::type_parameter_constraint(
            self.ctx.types,
            object_type,
        ) else {
            return false;
        };

        let resolved_constraint = self.resolve_type_for_property_access(constraint);
        matches!(
            self.resolve_property_access_with_env(resolved_constraint, prop_name),
            PropertyAccessResult::Success {
                from_index_signature: false,
                ..
            }
        )
    }

    fn mapped_type_has_explicit_property(
        &self,
        mapped_id: tsz_solver::MappedTypeId,
        prop_name: &str,
    ) -> bool {
        let mapped = self.ctx.types.mapped_type(mapped_id);
        crate::query_boundaries::state::checking::get_finite_mapped_property_type(
            self.ctx.types,
            mapped_id,
            prop_name,
        )
        .is_some()
            || crate::query_boundaries::state::checking::collect_finite_mapped_property_names(
                self.ctx.types,
                mapped_id,
            )
            .is_some_and(|names| names.contains(&self.ctx.types.intern_string(prop_name)))
            || crate::query_boundaries::state::checking::extract_string_literal_keys(
                self.ctx.types,
                mapped.constraint,
            )
            .iter()
            .any(|name| self.ctx.types.resolve_atom(*name) == prop_name)
    }

    fn mapped_explicit_property_names(&self, mapped_id: tsz_solver::MappedTypeId) -> Vec<String> {
        let mapped = self.ctx.types.mapped_type(mapped_id);
        let mut names: Vec<String> =
            crate::query_boundaries::state::checking::collect_finite_mapped_property_names(
                self.ctx.types,
                mapped_id,
            )
            .into_iter()
            .flatten()
            .map(|name| self.ctx.types.resolve_atom(name))
            .collect();

        let preserves_source_names = mapped.name_type.is_none()
            || crate::query_boundaries::state::checking::is_identity_name_mapping(
                self.ctx.types,
                &mapped,
            );
        if preserves_source_names {
            for name in crate::query_boundaries::state::checking::extract_string_literal_keys(
                self.ctx.types,
                mapped.constraint,
            ) {
                let name = self.ctx.types.resolve_atom(name);
                if !names.iter().any(|existing| existing == &name) {
                    names.push(name);
                }
            }
        }

        names
    }

    fn generic_mapped_application_lacks_explicit_property(
        &mut self,
        object_type: TypeId,
        prop_name: &str,
        use_known_finite_names: bool,
    ) -> Option<bool> {
        use crate::query_boundaries::common::{
            TypeSubstitution, application_info, instantiate_type,
        };

        let (base, args) = application_info(self.ctx.types, object_type)?;
        let sym_id = self.ctx.resolve_type_to_symbol_id(base)?;
        let (body_type, type_params) = self.type_reference_symbol_type_with_params(sym_id);
        let mapped_id = crate::query_boundaries::common::mapped_type_id(self.ctx.types, body_type)?;
        let mapped = self.ctx.types.mapped_type(mapped_id);
        if !crate::query_boundaries::common::contains_type_parameters(
            self.ctx.types,
            mapped.constraint,
        ) {
            return None;
        }

        let substitution = TypeSubstitution::from_args(self.ctx.types, &type_params, &args);
        let instantiated = instantiate_type(self.ctx.types, body_type, &substitution);
        let instantiated_mapped_id =
            crate::query_boundaries::common::mapped_type_id(self.ctx.types, instantiated)?;
        let instantiated_mapped = self.ctx.types.mapped_type(instantiated_mapped_id);
        let names = self.mapped_explicit_property_names(instantiated_mapped_id);
        let has_explicit_name = names.iter().any(|name| name == prop_name)
            || self.mapped_type_has_explicit_property(instantiated_mapped_id, prop_name);
        if has_explicit_name {
            return Some(false);
        }
        let preserves_source_names = instantiated_mapped.name_type.is_none()
            || crate::query_boundaries::state::checking::is_identity_name_mapping(
                self.ctx.types,
                &instantiated_mapped,
            );
        if preserves_source_names {
            if use_known_finite_names && !names.is_empty() {
                return Some(true);
            }
            return None;
        }
        Some(true)
    }

    pub(crate) fn generic_mapped_receiver_explicit_property_names(
        &mut self,
        object_type: TypeId,
    ) -> Vec<String> {
        use crate::query_boundaries::common::{
            TypeSubstitution, application_info, instantiate_type,
        };

        if let Some((base, args)) = application_info(self.ctx.types, object_type)
            && let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(base)
        {
            let (body_type, type_params) = self.type_reference_symbol_type_with_params(sym_id);
            if let Some(mapped_id) =
                crate::query_boundaries::common::mapped_type_id(self.ctx.types, body_type)
            {
                let mapped = self.ctx.types.mapped_type(mapped_id);
                if crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    mapped.constraint,
                ) {
                    let substitution =
                        TypeSubstitution::from_args(self.ctx.types, &type_params, &args);
                    let instantiated = instantiate_type(self.ctx.types, body_type, &substitution);
                    if let Some(instantiated_mapped_id) =
                        crate::query_boundaries::common::mapped_type_id(
                            self.ctx.types,
                            instantiated,
                        )
                    {
                        return self.mapped_explicit_property_names(instantiated_mapped_id);
                    }
                }
            }
        }

        if let Some(mapped_id) =
            crate::query_boundaries::common::mapped_type_id(self.ctx.types, object_type)
        {
            return self.mapped_explicit_property_names(mapped_id);
        }

        Vec::new()
    }

    pub(crate) fn generic_mapped_receiver_lacks_explicit_property(
        &mut self,
        object_type: TypeId,
        prop_name: &str,
    ) -> bool {
        use crate::query_boundaries::common as common_query;

        if let Some(lacks_explicit_property) =
            self.generic_mapped_application_lacks_explicit_property(object_type, prop_name, false)
        {
            return lacks_explicit_property;
        }

        let resolved = self.resolve_type_for_property_access(object_type);
        let evaluated = self.evaluate_type_with_env(resolved);

        for candidate in [resolved, evaluated] {
            if !common_query::contains_type_parameters(self.ctx.types, candidate) {
                continue;
            }

            let Some(mapped_id) = common_query::mapped_type_id(self.ctx.types, candidate) else {
                continue;
            };

            return !self.mapped_type_has_explicit_property(mapped_id, prop_name);
        }

        false
    }

    pub(crate) fn generic_mapped_receiver_lacks_property_access_name(
        &mut self,
        object_type: TypeId,
        prop_name: &str,
    ) -> bool {
        use crate::query_boundaries::common as common_query;

        if let Some(lacks_explicit_property) =
            self.generic_mapped_application_lacks_explicit_property(object_type, prop_name, true)
        {
            return lacks_explicit_property;
        }

        let resolved = self.resolve_type_for_property_access(object_type);
        let evaluated = self.evaluate_type_with_env(resolved);

        for candidate in [resolved, evaluated] {
            if !common_query::contains_type_parameters(self.ctx.types, candidate) {
                continue;
            }

            let Some(mapped_id) = common_query::mapped_type_id(self.ctx.types, candidate) else {
                continue;
            };

            return !self.mapped_type_has_explicit_property(mapped_id, prop_name);
        }

        false
    }

    pub(in crate::types_domain) fn strict_bind_call_apply_method_type(
        &mut self,
        object_type: TypeId,
        object_expr_idx: NodeIndex,
        property_name: &str,
    ) -> Option<TypeId> {
        if !matches!(property_name, "apply" | "bind" | "call") {
            return None;
        }

        fn method_this_arg_type(
            sig: &tsz_solver::CallSignature,
            is_constructor: bool,
            receiver_this_type: Option<TypeId>,
        ) -> TypeId {
            if is_constructor {
                sig.return_type
            } else if sig.this_type.is_some() {
                receiver_this_type.unwrap_or_else(|| sig.this_type.unwrap_or(TypeId::ANY))
            } else {
                TypeId::ANY
            }
        }

        fn bind_this_arg_type(
            sig: &tsz_solver::CallSignature,
            is_constructor: bool,
            receiver_this_type: Option<TypeId>,
        ) -> TypeId {
            if is_constructor {
                TypeId::ANY
            } else if sig.this_type.is_some() {
                receiver_this_type.unwrap_or_else(|| sig.this_type.unwrap_or(TypeId::ANY))
            } else {
                TypeId::ANY
            }
        }

        fn signature_to_call_signature(
            shape: &tsz_solver::FunctionShape,
        ) -> tsz_solver::CallSignature {
            tsz_solver::CallSignature {
                type_params: shape.type_params.clone(),
                params: shape.params.clone(),
                this_type: shape.this_type,
                return_type: shape.return_type,
                type_predicate: shape.type_predicate,
                is_method: shape.is_method,
            }
        }

        fn signature_params_as_tuple(
            factory: tsz_solver::TypeFactory<'_>,
            params: &[tsz_solver::ParamInfo],
        ) -> TypeId {
            let tuple_elements: Vec<tsz_solver::TupleElement> = params
                .iter()
                .map(|param| tsz_solver::TupleElement {
                    type_id: param.type_id,
                    name: param.name,
                    optional: param.optional && !param.rest,
                    rest: param.rest,
                })
                .collect();
            factory.tuple(tuple_elements)
        }

        fn bound_callable_return_type(
            factory: tsz_solver::TypeFactory<'_>,
            sig: &tsz_solver::CallSignature,
            remaining_params: Vec<tsz_solver::ParamInfo>,
            is_constructor: bool,
        ) -> TypeId {
            if is_constructor {
                return factory.callable(tsz_solver::CallableShape {
                    call_signatures: Vec::new(),
                    construct_signatures: vec![tsz_solver::CallSignature {
                        type_params: sig.type_params.clone(),
                        params: remaining_params,
                        this_type: None,
                        return_type: sig.return_type,
                        type_predicate: None,
                        is_method: false,
                    }],
                    properties: Vec::new(),
                    string_index: None,
                    number_index: None,
                    symbol: None,
                    is_abstract: false,
                });
            }

            factory.function(tsz_solver::FunctionShape {
                type_params: sig.type_params.clone(),
                params: remaining_params,
                this_type: None,
                return_type: sig.return_type,
                type_predicate: sig.type_predicate,
                is_constructor: false,
                is_method: false,
            })
        }

        let mut candidates = vec![object_type];
        if let Some(sym_id) = self.resolve_identifier_symbol(object_expr_idx) {
            let sym_type = self.get_type_of_symbol(sym_id);
            if sym_type != TypeId::ERROR && !candidates.contains(&sym_type) {
                candidates.push(sym_type);
            }
        }

        let receiver_this_type = self
            .ctx
            .arena
            .get(object_expr_idx)
            .and_then(|node| self.ctx.arena.get_access_expr(node))
            .map(|access| self.get_type_of_node(access.expression))
            .filter(|ty| *ty != TypeId::ERROR);

        let mut call_targets = Vec::new();
        let mut construct_targets = Vec::new();
        for candidate in candidates {
            if let Some(shape) =
                crate::query_boundaries::property_access::function_shape(self.ctx.types, candidate)
            {
                let sig = signature_to_call_signature(&shape);
                if !call_targets.contains(&sig) {
                    call_targets.push(sig);
                }
            }

            if let Some(shape) =
                crate::query_boundaries::property_access::callable_shape(self.ctx.types, candidate)
            {
                for sig in &shape.call_signatures {
                    if !call_targets.contains(sig) {
                        call_targets.push(sig.clone());
                    }
                }
                for sig in &shape.construct_signatures {
                    if !construct_targets.contains(sig) {
                        construct_targets.push(sig.clone());
                    }
                }
            }
        }

        let factory = self.ctx.types.factory();
        let mut method_signatures = Vec::new();

        for (sig, is_constructor) in call_targets
            .iter()
            .map(|sig| (sig, false))
            .chain(construct_targets.iter().map(|sig| (sig, true)))
        {
            match property_name {
                "apply" => {
                    let method_sig = tsz_solver::CallSignature {
                        type_params: sig.type_params.clone(),
                        params: vec![
                            tsz_solver::ParamInfo {
                                name: Some(self.ctx.types.intern_string("thisArg")),
                                type_id: method_this_arg_type(
                                    sig,
                                    is_constructor,
                                    receiver_this_type,
                                ),
                                optional: false,
                                rest: false,
                            },
                            tsz_solver::ParamInfo {
                                name: Some(self.ctx.types.intern_string("args")),
                                type_id: signature_params_as_tuple(factory, &sig.params),
                                optional: true,
                                rest: false,
                            },
                        ],
                        this_type: None,
                        return_type: if is_constructor {
                            TypeId::VOID
                        } else {
                            sig.return_type
                        },
                        type_predicate: None,
                        is_method: false,
                    };
                    if !method_signatures.contains(&method_sig) {
                        method_signatures.push(method_sig);
                    }
                }
                "call" => {
                    let mut params = Vec::with_capacity(1 + sig.params.len());
                    params.push(tsz_solver::ParamInfo {
                        name: Some(self.ctx.types.intern_string("thisArg")),
                        type_id: method_this_arg_type(sig, is_constructor, receiver_this_type),
                        optional: false,
                        rest: false,
                    });
                    params.extend(sig.params.clone());

                    let method_sig = tsz_solver::CallSignature {
                        type_params: sig.type_params.clone(),
                        params,
                        this_type: None,
                        return_type: if is_constructor {
                            TypeId::VOID
                        } else {
                            sig.return_type
                        },
                        type_predicate: None,
                        is_method: false,
                    };
                    if !method_signatures.contains(&method_sig) {
                        method_signatures.push(method_sig);
                    }
                }
                "bind" => {
                    let fixed_prefix_count =
                        sig.params.iter().take_while(|param| !param.rest).count();
                    for prefix_len in 0..=fixed_prefix_count {
                        let this_arg_type =
                            bind_this_arg_type(sig, is_constructor, receiver_this_type);
                        let mut params = Vec::with_capacity(1 + prefix_len);
                        params.push(tsz_solver::ParamInfo {
                            name: Some(self.ctx.types.intern_string("thisArg")),
                            type_id: this_arg_type,
                            optional: false,
                            rest: false,
                        });
                        params.extend(sig.params.iter().take(prefix_len).cloned());

                        let remaining_params =
                            sig.params.iter().skip(prefix_len).cloned().collect();
                        let method_sig = tsz_solver::CallSignature {
                            type_params: sig.type_params.clone(),
                            params,
                            this_type: None,
                            return_type: bound_callable_return_type(
                                factory,
                                sig,
                                remaining_params,
                                is_constructor,
                            ),
                            type_predicate: None,
                            is_method: false,
                        };
                        if !method_signatures.contains(&method_sig) {
                            method_signatures.push(method_sig);
                        }

                        if prefix_len == 0 && sig.this_type.is_some() && !is_constructor {
                            let generic_this_param = tsz_solver::TypeParamInfo {
                                name: self.ctx.types.intern_string("TThis"),
                                constraint: Some(this_arg_type),
                                default: None,
                                is_const: false,
                            };
                            let generic_this_type = factory.type_param(generic_this_param);
                            let generic_bind_sig = tsz_solver::CallSignature {
                                type_params: std::iter::once(generic_this_param)
                                    .chain(sig.type_params.clone())
                                    .collect(),
                                params: vec![tsz_solver::ParamInfo {
                                    name: Some(self.ctx.types.intern_string("thisArg")),
                                    type_id: generic_this_type,
                                    optional: false,
                                    rest: false,
                                }],
                                this_type: None,
                                return_type: bound_callable_return_type(
                                    factory,
                                    sig,
                                    sig.params.clone(),
                                    is_constructor,
                                ),
                                type_predicate: None,
                                is_method: false,
                            };
                            if !method_signatures.contains(&generic_bind_sig) {
                                method_signatures.push(generic_bind_sig);
                            }
                        }
                    }
                }
                _ => return None,
            }
        }

        match method_signatures.len() {
            0 => None,
            1 => Some(factory.function(tsz_solver::FunctionShape {
                type_params: method_signatures[0].type_params.clone(),
                params: method_signatures[0].params.clone(),
                this_type: None,
                return_type: method_signatures[0].return_type,
                type_predicate: method_signatures[0].type_predicate,
                is_constructor: false,
                is_method: false,
            })),
            _ => Some(factory.callable(tsz_solver::CallableShape {
                call_signatures: method_signatures,
                construct_signatures: Vec::new(),
                properties: Vec::new(),
                string_index: None,
                number_index: None,
                symbol: None,
                is_abstract: false,
            })),
        }
    }

    /// Emit TS1470 if `import.meta` appears in a file that builds to CommonJS output.
    ///
    /// TSC logic: in Node16/NodeNext module modes, the per-file format determines
    /// whether the file outputs CJS (TS1470). For older module modes (< ES2020,
    /// excluding System), ALL files produce CJS output so TS1470 always fires.
    pub(in crate::types_domain) fn check_import_meta_in_cjs(&mut self, node_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_common::common::ModuleKind;

        let module_kind = self.ctx.compiler_options.module;
        let should_error = if module_kind.is_node_module() {
            // Node16/Node18/Node20/NodeNext: per-file CJS/ESM determination
            let current_file = &self.ctx.file_name;
            let is_commonjs_file = current_file.ends_with(".cts") || current_file.ends_with(".cjs");
            let is_esm_file = current_file.ends_with(".mts") || current_file.ends_with(".mjs");
            if is_commonjs_file {
                true
            } else if is_esm_file {
                false
            } else if let Some(is_esm) = self.ctx.file_is_esm {
                !is_esm
            } else {
                false
            }
        } else if module_kind == ModuleKind::System
            || (module_kind as u32) >= (ModuleKind::ES2020 as u32)
        {
            // System and ES2020+ support import.meta natively
            false
        } else {
            // CommonJS, AMD, UMD, ES2015, None -> always CJS output
            true
        };

        if should_error {
            self.error_at_node(
                node_idx,
                diagnostic_messages::THE_IMPORT_META_META_PROPERTY_IS_NOT_ALLOWED_IN_FILES_WHICH_WILL_BUILD_INTO_COMM,
                diagnostic_codes::THE_IMPORT_META_META_PROPERTY_IS_NOT_ALLOWED_IN_FILES_WHICH_WILL_BUILD_INTO_COMM,
            );
        }
    }

    /// Mirror the binder's `resolved_const_expando_key` logic so that the checker
    /// resolves element-access keys using the same approach the binder used when
    /// it stored the expando property.
    pub(crate) fn resolved_const_expando_key_from_binder(
        &self,
        sym_id: tsz_binder::SymbolId,
        depth: u8,
    ) -> Option<String> {
        if depth > 8 {
            return None;
        }

        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .iter()
                .copied()
                .find(|decl| decl.is_some())?
        };
        if !self.ctx.arena.is_const_variable_declaration(decl_idx) {
            return None;
        }

        let decl_node = self.ctx.arena.get(decl_idx)?;
        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        let init_idx = var_decl.initializer;
        if init_idx.is_none() {
            return None;
        }
        let init_node = self.ctx.arena.get(init_idx)?;

        match init_node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                self.ctx
                    .arena
                    .get_literal(init_node)
                    .map(|lit| lit.text.clone())
            }
            k if k == tsz_parser::parser::syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.ctx.arena.get_unary_expr(init_node)?;
                let operand = self.ctx.arena.get(unary.operand)?;
                if operand.kind != SyntaxKind::NumericLiteral as u16 {
                    return None;
                }
                let lit = self.ctx.arena.get_literal(operand)?;
                match unary.operator {
                    k if k == SyntaxKind::MinusToken as u16 => Some(format!("-{}", lit.text)),
                    k if k == SyntaxKind::PlusToken as u16 => Some(lit.text.clone()),
                    _ => None,
                }
            }
            k if k == SyntaxKind::Identifier as u16 => {
                let name = self
                    .ctx
                    .arena
                    .get_identifier(init_node)?
                    .escaped_text
                    .clone();
                let next_sym = self.ctx.binder.file_locals.get(&name)?;
                self.resolved_const_expando_key_from_binder(next_sym, depth + 1)
            }
            k if k == tsz_parser::parser::syntax_kind_ext::CALL_EXPRESSION => {
                Self::is_symbol_call_in_arena(self.ctx.arena, init_idx)
                    .then(|| format!("__unique_{}", sym_id.0))
            }
            _ => None,
        }
    }

    /// Check if a node is a `Symbol()` or `Symbol("desc")` call expression (pure AST check).
    pub(crate) fn is_symbol_call_in_arena(
        arena: &tsz_parser::parser::node::NodeArena,
        idx: NodeIndex,
    ) -> bool {
        let Some(node) = arena.get(idx) else {
            return false;
        };
        if node.kind != tsz_parser::parser::syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }
        let Some(call) = arena.get_call_expr(node) else {
            return false;
        };
        let Some(expr_node) = arena.get(call.expression) else {
            return false;
        };
        arena
            .get_identifier(expr_node)
            .is_some_and(|ident| ident.escaped_text == "Symbol")
    }

    /// Check if the object expression has any unique-symbol-keyed expando properties
    /// recorded by the binder (i.e., any `__unique_*` entry in `expando_properties`).
    pub(crate) fn object_has_unique_symbol_expandos(&self, object_expr_idx: NodeIndex) -> bool {
        let Some(obj_key) = property_access_chain_text_in_arena(self.ctx.arena, object_expr_idx)
        else {
            return false;
        };
        let mut candidate_keys = vec![obj_key];
        if let Some(node) = self.ctx.arena.get(object_expr_idx)
            && node.kind == SyntaxKind::Identifier as u16
            && let Some(sym_id) = self.resolve_identifier_symbol_without_tracking(object_expr_idx)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && let Some(decl_node) = self.ctx.arena.get(symbol.value_declaration)
            && decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
            && let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node)
            && let Some(init_node) = self.ctx.arena.get(var_decl.initializer)
            && init_node.kind == syntax_kind_ext::NEW_EXPRESSION
            && let Some(new_expr) = self.ctx.arena.get_call_expr(init_node)
            && let Some(ctor_key) =
                property_access_chain_text_in_arena(self.ctx.arena, new_expr.expression)
        {
            candidate_keys.push(format!("{ctor_key}.prototype"));
            if let Some(ctor_node) = self.ctx.arena.get(new_expr.expression)
                && ctor_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && let Some(ctor_access) = self.ctx.arena.get_access_expr(ctor_node)
                && let Some(name_node) = self.ctx.arena.get(ctor_access.name_or_argument)
                && let Some(name_ident) = self.ctx.arena.get_identifier(name_node)
            {
                candidate_keys.push(format!("{}.prototype", name_ident.escaped_text));
            }
        }

        let has_unique =
            |expandos: &rustc_hash::FxHashMap<String, rustc_hash::FxHashSet<String>>, key: &str| {
                expandos
                    .get(key)
                    .is_some_and(|props| props.iter().any(|p| p.starts_with("__unique_")))
            };

        for key in &candidate_keys {
            if has_unique(&self.ctx.binder.expando_properties, key) {
                return true;
            }
        }
        // Use global expando index for O(1) lookup instead of O(N) binder scan
        if let Some(expando_idx) = &self.ctx.global_expando_index {
            for key in &candidate_keys {
                if has_unique(expando_idx, key) {
                    return true;
                }
            }
        } else if let Some(all_binders) = &self.ctx.all_binders {
            for binder in all_binders.iter() {
                for key in &candidate_keys {
                    if has_unique(&binder.expando_properties, key) {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub(crate) fn object_expr_is_new_constructor_instance(
        &self,
        object_expr_idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(object_expr_idx) else {
            return false;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(sym_id) = self.resolve_identifier_symbol_without_tracking(object_expr_idx) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let Some(decl_node) = self.ctx.arena.get(symbol.value_declaration) else {
            return false;
        };
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return false;
        }
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        let Some(init_node) = self.ctx.arena.get(var_decl.initializer) else {
            return false;
        };
        init_node.kind == syntax_kind_ext::NEW_EXPRESSION
    }
}
