//! Helper methods for property access type resolution.
//!
//! Contains expando function/property detection, union/type-parameter property
//! checks, strict bind/call/apply method synthesis, and import.meta CJS checks.
//!
//! Extracted from `property_access_type.rs` to keep module size manageable.

use crate::FlowAnalyzer;
use crate::state::CheckerState;
use std::rc::Rc;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn synthesized_array_iterator_method_type(
        &mut self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        if !matches!(property_name, "values" | "keys" | "entries") {
            return None;
        }

        let element_type = tsz_solver::type_queries::get_array_element_type(
            self.ctx.types,
            object_type,
        )
        .or_else(|| {
            tsz_solver::type_queries::get_tuple_element_type_union(self.ctx.types, object_type)
        })?;

        let iterator_base = self
            .resolve_entity_name_text_to_def_id_for_lowering("ArrayIterator")
            .map(|def_id| self.ctx.types.lazy(def_id))
            .or_else(|| {
                self.resolve_entity_name_text_to_def_id_for_lowering("IterableIterator")
                    .map(|def_id| self.ctx.types.lazy(def_id))
            })?;

        let return_arg = match property_name {
            "values" => element_type,
            "keys" => TypeId::NUMBER,
            "entries" => self.ctx.types.tuple(vec![
                tsz_solver::TupleElement {
                    type_id: TypeId::NUMBER,
                    name: None,
                    optional: false,
                    rest: false,
                },
                tsz_solver::TupleElement {
                    type_id: element_type,
                    name: None,
                    optional: false,
                    rest: false,
                },
            ]),
            _ => return None,
        };

        let return_type = self.ctx.types.application(iterator_base, vec![return_arg]);

        Some(self.ctx.types.function(tsz_solver::FunctionShape {
            type_params: Vec::new(),
            params: Vec::new(),
            this_type: None,
            return_type,
            type_predicate: None,
            is_constructor: false,
            is_method: true,
        }))
    }

    /// Check if a property access is an expando function assignment pattern.
    ///
    /// TypeScript allows assigning properties to function and class declarations:
    /// ```typescript
    /// function foo() {}
    /// foo.bar = 1;  // OK - expando pattern, no TS2339
    /// ```
    ///
    /// Returns true if:
    /// 1. The property access is the LHS of a `=` assignment
    /// 2. The object expression is an identifier bound to a function or class declaration
    /// 3. The object type is a function type
    pub(super) fn is_expando_function_assignment(
        &self,
        property_access_idx: NodeIndex,
        object_expr_idx: NodeIndex,
        object_type: TypeId,
    ) -> bool {
        use tsz_solver::visitor::is_function_type;

        // Check if object type is a function type
        if !is_function_type(self.ctx.types, object_type) {
            return false;
        }

        // Check if property access is LHS of a `=` assignment
        let parent_idx = match self.ctx.arena.get_extended(property_access_idx) {
            Some(ext) if ext.parent.is_some() => ext.parent,
            _ => return false,
        };
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        let Some(binary) = self.ctx.arena.get_binary_expr(parent_node) else {
            return false;
        };
        if binary.operator_token != SyntaxKind::EqualsToken as u16
            || binary.left != property_access_idx
        {
            return false;
        }

        // Resolve object symbol for both simple identifiers and qualified chains.
        let sym_id = self
            .resolve_identifier_symbol(object_expr_idx)
            .or_else(|| self.resolve_qualified_symbol(object_expr_idx));

        if let Some(sym_id) = sym_id
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            if (symbol.flags & (symbol_flags::FUNCTION | symbol_flags::CLASS)) == 0 {
                return false;
            }
            // For class declarations, don't treat as expando if the property
            // exists as an instance member. Accessing instance members on the
            // constructor type (e.g., `Base.instanceProp = 2`) should produce
            // TS2339, not be silently accepted as an expando.
            if (symbol.flags & symbol_flags::CLASS) != 0 {
                let prop_name = self
                    .ctx
                    .arena
                    .get(property_access_idx)
                    .and_then(|n| self.ctx.arena.get_access_expr(n))
                    .and_then(|a| {
                        self.ctx
                            .arena
                            .get(a.name_or_argument)
                            .and_then(|n| self.ctx.arena.get_identifier(n))
                            .map(|id| id.escaped_text.as_str())
                    });
                if let Some(prop_name) = prop_name {
                    let obj_key = symbol.escaped_name.as_str();
                    if self.class_has_instance_member(obj_key, prop_name) {
                        return false;
                    }
                }
            }
            return true;
        }

        // Namespace member fallback: allow expando assignment for function-typed
        // members accessed through namespace/value-module chains (e.g., `app.foo.bar = ...`).
        // Binder tracks these expandos by chain key, so reads can observe them later.
        fn root_identifier(
            arena: &tsz_parser::parser::node::NodeArena,
            idx: NodeIndex,
        ) -> Option<String> {
            let node = arena.get(idx)?;
            if node.kind == SyntaxKind::Identifier as u16 {
                return arena.get_identifier(node).map(|id| id.escaped_text.clone());
            }
            if node.kind == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let access = arena.get_access_expr(node)?;
                return root_identifier(arena, access.expression);
            }
            None
        }

        if let Some(root_name) = root_identifier(self.ctx.arena, object_expr_idx)
            && let Some(root_sym) = self.ctx.binder.file_locals.get(&root_name)
            && let Some(root_symbol) = self.ctx.binder.get_symbol(root_sym)
            && (root_symbol.flags & (symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE))
                != 0
        {
            return true;
        }

        false
    }

    /// Check if a property access reads an expando property assigned via `X.prop = value`.
    ///
    /// Checks the current file's binder first, then all other binders in multi-file
    /// mode (for global-scope cross-file expando access). Also handles import chains
    /// like `a.C1.staticProp` by resolving the object expression to its source symbol
    /// and checking the source file's binder.
    pub(super) fn is_expando_property_read(
        &self,
        object_expr_idx: NodeIndex,
        property_name: &str,
    ) -> bool {
        fn property_access_chain(
            arena: &tsz_parser::parser::node::NodeArena,
            idx: NodeIndex,
        ) -> Option<String> {
            let node = arena.get(idx)?;
            if node.kind == SyntaxKind::Identifier as u16 {
                return arena.get_identifier(node).map(|id| id.escaped_text.clone());
            }
            if node.kind == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let access = arena.get_access_expr(node)?;
                let left = property_access_chain(arena, access.expression)?;
                let right_node = arena.get(access.name_or_argument)?;
                let right = arena.get_identifier(right_node)?.escaped_text.clone();
                return Some(format!("{left}.{right}"));
            }
            None
        }

        let Some(obj_key) = property_access_chain(self.ctx.arena, object_expr_idx) else {
            return false;
        };

        // Don't treat as expando if the object is a class and the property exists
        // as an instance member of that class. In that case, accessing it on the
        // constructor type (typeof ClassName) should produce TS2339, not silently
        // succeed as an expando. This distinguishes `Base.a = 2` where `a` is an
        // instance getter/setter (should error) from `Base.newProp = 2` where
        // `newProp` is a genuine expando (should succeed).
        if self.class_has_instance_member(&obj_key, property_name) {
            return false;
        }

        // 1. Check current file's binder
        if self
            .ctx
            .binder
            .expando_properties
            .get(&obj_key)
            .is_some_and(|props| props.contains(property_name))
        {
            return true;
        }

        // 2. Check all other binders (cross-file global scope access)
        if let Some(all_binders) = &self.ctx.all_binders {
            for binder in all_binders.iter() {
                if binder
                    .expando_properties
                    .get(&obj_key)
                    .is_some_and(|props| props.contains(property_name))
                {
                    return true;
                }
            }
        }

        // 3. For qualified access chains like `a.C1` where `a` is an import namespace,
        //    the source file's binder stores the expando under just "C1" (the original
        //    symbol name), not "a.C1". Extract the last segment and check all binders.
        if let Some(last_dot) = obj_key.rfind('.') {
            let last_segment = &obj_key[last_dot + 1..];
            if let Some(all_binders) = &self.ctx.all_binders {
                for binder in all_binders.iter() {
                    if binder
                        .expando_properties
                        .get(last_segment)
                        .is_some_and(|props| props.contains(property_name))
                    {
                        return true;
                    }
                }
            }
        }

        false
    }

    pub(super) fn js_expando_property_read_before_assignment(
        &self,
        property_access_idx: NodeIndex,
        object_expr_idx: NodeIndex,
        property_name: &str,
    ) -> bool {
        if !self.is_js_file() || !self.ctx.compiler_options.check_js {
            return false;
        }
        if self.property_access_is_write_target_or_base(property_access_idx) {
            return false;
        }
        if self.is_commonjs_module_exports_root(object_expr_idx) {
            return false;
        }
        if !self.is_js_expando_capable_read_root(object_expr_idx, property_name) {
            return false;
        }

        let Some(flow_node) = self.flow_node_for_reference_usage(property_access_idx) else {
            return false;
        };

        !self
            .flow_analyzer_for_property_reads()
            .is_definitely_assigned(property_access_idx, flow_node)
    }

    fn is_js_expando_capable_read_root(
        &self,
        object_expr_idx: NodeIndex,
        property_name: &str,
    ) -> bool {
        self.is_expando_property_read(object_expr_idx, property_name)
            || self.is_js_prototype_read_root(object_expr_idx)
    }

    fn is_commonjs_module_exports_root(&self, object_expr_idx: NodeIndex) -> bool {
        self.expression_text(object_expr_idx).as_deref() == Some("module.exports")
    }

    fn is_js_prototype_read_root(&self, object_expr_idx: NodeIndex) -> bool {
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

        let Some(sym_id) = self.resolve_identifier_symbol(access.expression) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        (symbol.flags & (symbol_flags::FUNCTION | symbol_flags::CLASS)) != 0
    }

    fn property_access_is_write_target_or_base(&self, property_access_idx: NodeIndex) -> bool {
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

    fn flow_node_for_reference_usage(&self, idx: NodeIndex) -> Option<tsz_binder::FlowNodeId> {
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

    fn flow_analyzer_for_property_reads(&self) -> FlowAnalyzer<'_> {
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
        .with_type_environment(Rc::clone(&self.ctx.type_environment))
        .with_narrowing_cache(&self.ctx.narrowing_cache)
        .with_call_type_predicates(&self.ctx.call_type_predicates)
        .with_flow_buffers(
            &self.ctx.flow_worklist,
            &self.ctx.flow_in_worklist,
            &self.ctx.flow_visited,
            &self.ctx.flow_results,
        )
    }

    /// Check if a class has an instance member (property, method, or accessor) with the given name.
    /// Used to prevent expando property detection from masking TS2339 errors when accessing
    /// instance members on the class constructor type.
    fn class_has_instance_member(&self, obj_key: &str, property_name: &str) -> bool {
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

    pub(super) fn union_has_explicit_property_member(
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

    pub(super) fn type_parameter_constraint_has_explicit_property(
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

    pub(super) fn strict_bind_call_apply_method_type(
        &mut self,
        object_type: TypeId,
        object_expr_idx: NodeIndex,
        property_name: &str,
    ) -> Option<TypeId> {
        if property_name != "apply" {
            return None;
        }

        let factory = self.ctx.types.factory();
        let mut candidates = vec![object_type];
        if let Some(sym_id) = self.resolve_identifier_symbol(object_expr_idx) {
            let sym_type = self.get_type_of_symbol(sym_id);
            if sym_type != TypeId::ERROR && !candidates.contains(&sym_type) {
                candidates.push(sym_type);
            }
        }

        let mut resolved_shape = None;
        for candidate in candidates {
            if let Some(shape) =
                crate::query_boundaries::property_access::function_shape(self.ctx.types, candidate)
            {
                resolved_shape = Some((shape.params.clone(), shape.return_type));
                break;
            }
            if let Some(shape) =
                crate::query_boundaries::property_access::callable_shape(self.ctx.types, candidate)
                && let Some(sig) = shape.call_signatures.first()
            {
                resolved_shape = Some((sig.params.clone(), sig.return_type));
                break;
            }
        }

        let (params, return_type) = resolved_shape?;

        let tuple_elements: Vec<tsz_solver::TupleElement> = params
            .iter()
            .map(|param| tsz_solver::TupleElement {
                type_id: param.type_id,
                name: param.name,
                optional: param.optional && !param.rest,
                rest: param.rest,
            })
            .collect();
        let args_tuple = factory.tuple(tuple_elements);
        let method_shape = tsz_solver::FunctionShape {
            params: vec![
                tsz_solver::ParamInfo {
                    name: Some(self.ctx.types.intern_string("thisArg")),
                    type_id: TypeId::ANY,
                    optional: false,
                    rest: false,
                },
                tsz_solver::ParamInfo {
                    name: Some(self.ctx.types.intern_string("args")),
                    type_id: args_tuple,
                    optional: true,
                    rest: false,
                },
            ],
            this_type: None,
            return_type,
            type_params: vec![],
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        };

        Some(factory.function(method_shape))
    }

    /// Emit TS1470 if `import.meta` appears in a file that builds to CommonJS output.
    ///
    /// TSC logic: in Node16/NodeNext module modes, the per-file format determines
    /// whether the file outputs CJS (TS1470). For older module modes (< ES2020,
    /// excluding System), ALL files produce CJS output so TS1470 always fires.
    pub(super) fn check_import_meta_in_cjs(&mut self, node_idx: NodeIndex) {
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
        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .iter()
                .copied()
                .find(|decl| !decl.is_none())?
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
        fn property_access_chain(
            arena: &tsz_parser::parser::node::NodeArena,
            idx: NodeIndex,
        ) -> Option<String> {
            let node = arena.get(idx)?;
            if node.kind == SyntaxKind::Identifier as u16 {
                return arena.get_identifier(node).map(|id| id.escaped_text.clone());
            }
            if node.kind == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let access = arena.get_access_expr(node)?;
                let left = property_access_chain(arena, access.expression)?;
                let right_node = arena.get(access.name_or_argument)?;
                let right = arena.get_identifier(right_node)?.escaped_text.clone();
                return Some(format!("{left}.{right}"));
            }
            None
        }

        let Some(obj_key) = property_access_chain(self.ctx.arena, object_expr_idx) else {
            return false;
        };

        let has_unique =
            |expandos: &rustc_hash::FxHashMap<String, rustc_hash::FxHashSet<String>>| {
                expandos
                    .get(&obj_key)
                    .is_some_and(|props| props.iter().any(|p| p.starts_with("__unique_")))
            };

        if has_unique(&self.ctx.binder.expando_properties) {
            return true;
        }
        if let Some(all_binders) = &self.ctx.all_binders {
            for binder in all_binders.iter() {
                if has_unique(&binder.expando_properties) {
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CheckerOptions;
    use crate::query_boundaries::type_construction::TypeInterner;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;

    fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            CheckerOptions {
                no_property_access_from_index_signature: true,
                ..CheckerOptions::default()
            },
        );

        checker.check_source_file(root);

        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect()
    }

    #[test]
    fn explicit_property_in_intersection_suppresses_ts4111() {
        let diagnostics = get_diagnostics(
            r#"
type Bag = { foo: string } & { [k: string]: string };
declare const bag: Bag;
bag.foo;
"#,
        );

        let ts4111 = diagnostics
            .iter()
            .filter(|(code, _)| *code == 4111)
            .collect::<Vec<_>>();
        assert!(
            ts4111.is_empty(),
            "Explicit properties in intersections should not be treated as pure index-signature access. Got: {diagnostics:?}"
        );
    }
}
