use crate::query_boundaries::common::object_shape_for_type;
use crate::state::CheckerState;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn imported_namespace_display_module_name(&self, module_name: &str) -> String {
        let trimmed = module_name.strip_prefix("./").unwrap_or(module_name);
        for ext in &[
            ".d.ts", ".d.tsx", ".d.mts", ".d.cts", ".ts", ".tsx", ".mts", ".cts", ".js", ".jsx",
            ".mjs", ".cjs",
        ] {
            if let Some(stripped) = trimmed.strip_suffix(ext) {
                return stripped.to_string();
            }
        }
        trimmed.to_string()
    }

    /// Resolve the display module name for namespace `typeof import("...")`.
    pub(crate) fn resolve_namespace_display_module_name(
        &self,
        exports_table: &tsz_binder::SymbolTable,
        fallback: &str,
    ) -> String {
        exports_table
            .get("export=")
            .or_else(|| self.resolve_cross_file_export(fallback, "export="))
            .and_then(|export_eq_sym| self.namespace_display_module_name_for_symbol(export_eq_sym))
            .unwrap_or_else(|| self.imported_namespace_display_module_name(fallback))
    }

    fn namespace_display_module_name_for_symbol(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<String> {
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders))?;

        let is_namespace_import =
            symbol.import_name.is_none() || symbol.import_name.as_deref() == Some("*");
        if is_namespace_import && let Some(module_name) = symbol.import_module.as_deref() {
            return Some(self.imported_namespace_display_module_name(module_name));
        }

        if symbol.flags & tsz_binder::symbol_flags::ALIAS != 0 {
            let mut visited = Vec::new();
            if let Some(target_sym_id) = self.resolve_alias_symbol(sym_id, &mut visited)
                && target_sym_id != sym_id
            {
                return self.namespace_display_module_name_for_symbol(target_sym_id);
            }
        }

        None
    }

    /// Resolve binding element type from a variable declaration initializer.
    ///
    /// For `let { a, ...rest } = expr`, this resolves the type of each binding
    /// element from the initializer expression's type. For rest elements
    /// (`dot_dot_dot_token`), the type is the initializer type with named
    /// sibling properties excluded. For named elements, the type is the
    /// corresponding property type from the initializer.
    ///
    /// This is critical for return type inference of generic functions:
    /// without it, destructured variables like `rest` in
    /// `let { a, ...rest } = obj` resolve to `any` during inference,
    /// causing the function's inferred return type to lose type parameter
    /// references and breaking instantiation at call sites.
    pub(crate) fn resolve_binding_element_from_variable_initializer(
        &mut self,
        value_decl: NodeIndex,
        name: &str,
    ) -> Option<TypeId> {
        let node = self.ctx.arena.get(value_decl)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        // Walk up: Identifier → BindingElement
        let ext = self.ctx.arena.get_extended(value_decl)?;
        let be_idx = ext.parent;
        if !be_idx.is_some() {
            return None;
        }
        let be_node = self.ctx.arena.get(be_idx)?;
        if be_node.kind != syntax_kind_ext::BINDING_ELEMENT {
            return None;
        }
        let be_data = self.ctx.arena.get_binding_element(be_node)?;

        // Walk up: BindingElement → ObjectBindingPattern
        let ext2 = self.ctx.arena.get_extended(be_idx)?;
        let pat_idx = ext2.parent;
        if !pat_idx.is_some() {
            return None;
        }
        let pat_node = self.ctx.arena.get(pat_idx)?;
        if pat_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN {
            return None;
        }

        // Walk up: ObjectBindingPattern → VariableDeclaration
        let ext3 = self.ctx.arena.get_extended(pat_idx)?;
        let var_decl_idx = ext3.parent;
        if !var_decl_idx.is_some() {
            return None;
        }
        let var_decl_node = self.ctx.arena.get(var_decl_idx)?;
        let var_decl = self.ctx.arena.get_variable_declaration(var_decl_node)?;

        // Get the initializer type
        if !var_decl.initializer.is_some() {
            return None;
        }
        let init_type = self.get_type_of_node(var_decl.initializer);
        if init_type == TypeId::ANY || init_type == TypeId::ERROR {
            return None;
        }

        if be_data.dot_dot_dot_token {
            // Rest element: compute the rest type (parent type minus excluded properties).
            // For type parameters, compute_object_rest_type preserves the type parameter.
            let rest_type = self.compute_object_rest_type(pat_idx, init_type);
            return Some(rest_type);
        }

        // Named property element: get the property type from the initializer type.
        let prop_name_str = if be_data.property_name.is_some() {
            self.get_identifier_text_from_idx(be_data.property_name)
        } else {
            Some(name.to_string())
        }?;

        let evaluated = self.evaluate_type_for_assignability(init_type);
        let prop_atom = self.ctx.types.intern_string(&prop_name_str);

        if let Some(shape) = object_shape_for_type(self.ctx.types, evaluated)
            && let Some(prop) = shape.properties.iter().find(|p| p.name == prop_atom)
        {
            let mut t = prop.type_id;
            if prop.optional && self.ctx.strict_null_checks() {
                t = self.ctx.types.factory().union(vec![t, TypeId::UNDEFINED]);
            }
            if be_data.initializer.is_some() && self.ctx.strict_null_checks() {
                t = tsz_solver::remove_undefined(self.ctx.types, t);
            }
            return Some(t);
        }

        // For type parameters, get the property from the constraint
        if let Some(constraint) =
            crate::query_boundaries::state::checking::type_parameter_constraint(
                self.ctx.types,
                evaluated,
            )
        {
            let constraint = self.evaluate_type_for_assignability(constraint);
            if let Some(shape) = object_shape_for_type(self.ctx.types, constraint)
                && let Some(prop) = shape.properties.iter().find(|p| p.name == prop_atom)
            {
                let mut t = prop.type_id;
                if prop.optional && self.ctx.strict_null_checks() {
                    t = self.ctx.types.factory().union(vec![t, TypeId::UNDEFINED]);
                }
                if be_data.initializer.is_some() && self.ctx.strict_null_checks() {
                    t = tsz_solver::remove_undefined(self.ctx.types, t);
                }
                return Some(t);
            }
        }

        None
    }

    /// Resolve binding element type from annotated destructured function parameter.
    pub(crate) fn resolve_binding_element_from_annotated_param(
        &mut self,
        value_decl: NodeIndex,
        name: &str,
    ) -> Option<TypeId> {
        let node = self.ctx.arena.get(value_decl)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let ext = self.ctx.arena.get_extended(value_decl)?;
        let be_idx = ext.parent;
        if !be_idx.is_some() {
            return None;
        }
        let be_node = self.ctx.arena.get(be_idx)?;
        if be_node.kind != syntax_kind_ext::BINDING_ELEMENT {
            return None;
        }
        let be_data = self.ctx.arena.get_binding_element(be_node)?;

        let ext2 = self.ctx.arena.get_extended(be_idx)?;
        let pat_idx = ext2.parent;
        if !pat_idx.is_some() {
            return None;
        }
        let pat_node = self.ctx.arena.get(pat_idx)?;
        let is_obj = pat_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN;
        let is_arr = pat_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN;
        if !is_obj && !is_arr {
            return None;
        }

        let ext3 = self.ctx.arena.get_extended(pat_idx)?;
        let param_idx = ext3.parent;
        if !param_idx.is_some() {
            return None;
        }
        let param_node = self.ctx.arena.get(param_idx)?;
        let param = self.ctx.arena.get_parameter(param_node)?;
        if !param.type_annotation.is_some() {
            return None;
        }

        let ann_type = self.get_type_from_type_node(param.type_annotation);
        if ann_type == TypeId::ANY || ann_type == TypeId::UNKNOWN || ann_type == TypeId::ERROR {
            return None;
        }

        let ann_type = self.evaluate_type_for_assignability(ann_type);
        if is_obj {
            let prop_name_str = if be_data.property_name.is_some() {
                self.get_identifier_text_from_idx(be_data.property_name)
            } else {
                Some(name.to_string())
            }?;
            let prop_atom = self.ctx.types.intern_string(&prop_name_str);

            // Try direct object shape first
            if let Some(shape) = object_shape_for_type(self.ctx.types, ann_type)
                && let Some(prop) = shape.properties.iter().find(|p| p.name == prop_atom)
            {
                let mut t = prop.type_id;
                if prop.optional && self.ctx.strict_null_checks() {
                    t = self.ctx.types.factory().union(vec![t, TypeId::UNDEFINED]);
                }
                if be_data.initializer.is_some() && self.ctx.strict_null_checks() {
                    t = tsz_solver::remove_undefined(self.ctx.types, t);
                }
                return Some(t);
            }

            // For union types (e.g., { kind: 'A', payload: number } | { kind: 'B', payload: string }),
            // collect the property type from each union member and return their union.
            // This enables correlated narrowing for dependent destructured variables.
            if let Some(members) =
                tsz_solver::type_queries::get_union_members(self.ctx.types, ann_type)
            {
                let mut prop_types = Vec::new();
                for &member in &members {
                    let evaluated = self.evaluate_type_for_assignability(member);
                    if let Some(shape) = object_shape_for_type(self.ctx.types, evaluated)
                        && let Some(prop) = shape.properties.iter().find(|p| p.name == prop_atom)
                    {
                        let mut t = prop.type_id;
                        if prop.optional && self.ctx.strict_null_checks() {
                            t = self.ctx.types.factory().union(vec![t, TypeId::UNDEFINED]);
                        }
                        prop_types.push(t);
                    }
                }
                if !prop_types.is_empty() {
                    let mut t = tsz_solver::utils::union_or_single(self.ctx.types, prop_types);
                    if be_data.initializer.is_some() && self.ctx.strict_null_checks() {
                        t = tsz_solver::remove_undefined(self.ctx.types, t);
                    }
                    return Some(t);
                }
            }
        }

        None
    }

    /// Compute the type of a class symbol.
    ///
    /// Returns the class constructor type, merging with namespace exports
    /// when the class is merged with a namespace. Also caches the instance
    /// type for TYPE position resolution.
    pub(super) fn compute_class_symbol_type(
        &mut self,
        sym_id: SymbolId,
        flags: u32,
        value_decl: NodeIndex,
        declarations: &[NodeIndex],
    ) -> (TypeId, Vec<tsz_solver::TypeParamInfo>) {
        let decl_idx = if value_decl.is_some()
            && self
                .ctx
                .arena
                .get(value_decl)
                .and_then(|n| self.ctx.arena.get_class(n))
                .is_some()
        {
            value_decl
        } else {
            declarations
                .iter()
                .find(|&&d| {
                    d.is_some()
                        && self
                            .ctx
                            .arena
                            .get(d)
                            .and_then(|n| self.ctx.arena.get_class(n))
                            .is_some()
                })
                .copied()
                .unwrap_or(NodeIndex::NONE)
        };

        if decl_idx.is_some()
            && let Some(node) = self.ctx.arena.get(decl_idx)
            && let Some(class) = self.ctx.arena.get_class(node)
        {
            let ctor_type = self.get_class_constructor_type(decl_idx, class);
            self.ctx.symbol_types.insert(sym_id, ctor_type);
            let instance_type = self.get_class_instance_type(decl_idx, class);

            self.ctx.symbol_instance_types.insert(sym_id, instance_type);

            let ctor_type = if flags & symbol_flags::FUNCTION != 0 {
                self.merge_function_call_signatures_into_class(ctor_type, declarations)
            } else {
                ctor_type
            };

            if flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0 {
                let merged = self.merge_namespace_exports_into_constructor(sym_id, ctor_type);
                return (merged, Vec::new());
            }
            return (ctor_type, Vec::new());
        }
        (TypeId::UNKNOWN, Vec::new())
    }

    /// Merge function call signatures into a class constructor type.
    fn merge_function_call_signatures_into_class(
        &mut self,
        ctor_type: TypeId,
        declarations: &[NodeIndex],
    ) -> TypeId {
        use crate::query_boundaries::state::type_analysis::{
            call_signatures_for_type, callable_shape_for_type,
        };

        let mut call_signatures = Vec::new();
        for &decl_idx in declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(func) = self.ctx.arena.get_function(node) else {
                continue;
            };
            if func.body.is_none() {
                call_signatures.push(self.call_signature_from_function(func, decl_idx));
            }
        }

        if call_signatures.is_empty() {
            for &decl_idx in declarations {
                let Some(node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                if self.ctx.arena.get_function(node).is_some() {
                    let func_type = self.get_type_of_function(decl_idx);
                    if let Some(signatures) = call_signatures_for_type(self.ctx.types, func_type) {
                        call_signatures = signatures;
                    }
                    break;
                }
            }
        }

        if call_signatures.is_empty() {
            return ctor_type;
        }

        let Some(shape) = callable_shape_for_type(self.ctx.types, ctor_type) else {
            return ctor_type;
        };

        let factory = self.ctx.types.factory();
        factory.callable(tsz_solver::CallableShape {
            call_signatures,
            construct_signatures: shape.construct_signatures.clone(),
            properties: shape.properties.clone(),
            string_index: shape.string_index.clone(),
            number_index: shape.number_index.clone(),
            symbol: None,
            is_abstract: false,
        })
    }

    /// Compute the type of an enum member symbol.
    pub(super) fn compute_enum_member_symbol_type(
        &mut self,
        sym_id: SymbolId,
        value_decl: NodeIndex,
    ) -> (TypeId, Vec<tsz_solver::TypeParamInfo>) {
        let member_def_id = self.ctx.get_or_create_def_id(sym_id);

        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
            let parent_sym_id = symbol.parent;
            let parent_def_id = self.ctx.get_or_create_def_id(parent_sym_id);
            if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
                env.register_enum_parent(member_def_id, parent_def_id);
            }
        }

        let literal_type = self.enum_member_type_from_decl(value_decl);

        let factory = self.ctx.types.factory();
        let enum_type = factory.enum_type(member_def_id, literal_type);
        (enum_type, Vec::new())
    }

    /// Compute the body type of a namespace-merged type alias.
    pub(super) fn compute_type_alias_body(&mut self, sym_id: SymbolId) -> Option<TypeId> {
        let symbol = self.get_symbol_globally(sym_id)?;
        let escaped_name = symbol.escaped_name.clone();
        let declarations = symbol.declarations.clone();

        let decl_idx = declarations.iter().copied().find(|&d| {
            self.ctx
                .arena
                .get(d)
                .and_then(|n| {
                    if n.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                        let type_alias = self.ctx.arena.get_type_alias(n)?;
                        let name_node = self.ctx.arena.get(type_alias.name)?;
                        let ident = self.ctx.arena.get_identifier(name_node)?;
                        let name = self.ctx.arena.resolve_identifier_text(ident);
                        Some(name == escaped_name)
                    } else {
                        Some(false)
                    }
                })
                .unwrap_or(false)
        })?;

        let node = self.ctx.arena.get(decl_idx)?;
        let type_alias = self.ctx.arena.get_type_alias(node)?;
        let (params, updates) = self.push_type_parameters(&type_alias.type_parameters);
        let alias_type = self.get_type_from_type_node(type_alias.type_node);
        self.pop_type_parameters(updates);

        let def_id = self.ctx.get_or_create_def_id(sym_id);
        if !params.is_empty() {
            self.ctx.insert_def_type_params(def_id, params);
        }

        Some(alias_type)
    }

    pub(super) fn declaration_namespace_prefix(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
    ) -> Option<String> {
        let mut parent = arena
            .get_extended(node_idx)
            .map_or(NodeIndex::NONE, |info| info.parent);
        let mut prefixes = Vec::new();

        while !parent.is_none() {
            let parent_node = arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module) = arena.get_module(parent_node)
                && let Some(name_node) = arena.get(module.name)
                && name_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                && let Some(name_ident) = arena.get_identifier(name_node)
            {
                prefixes.push(name_ident.escaped_text.clone());
            }

            parent = arena
                .get_extended(parent)
                .map_or(NodeIndex::NONE, |info| info.parent);
        }

        if prefixes.is_empty() {
            None
        } else {
            Some(prefixes.into_iter().rev().collect::<Vec<_>>().join("."))
        }
    }

    /// Compute the type of a namespace or module symbol.
    pub(super) fn compute_namespace_symbol_type(
        &mut self,
        sym_id: SymbolId,
        flags: u32,
    ) -> (TypeId, Vec<tsz_solver::TypeParamInfo>) {
        if flags & symbol_flags::INTERFACE != 0 {
            let interface_type = self.compute_interface_type_from_declarations(sym_id);
            self.ctx
                .symbol_instance_types
                .insert(sym_id, interface_type);
        }

        if flags & symbol_flags::TYPE_ALIAS != 0
            && let Some(alias_type) = self.compute_type_alias_body(sym_id)
        {
            self.ctx.symbol_instance_types.insert(sym_id, alias_type);
        }

        let def_id = self.ctx.get_or_create_def_id(sym_id);
        let factory = self.ctx.types.factory();
        (factory.lazy(def_id), Vec::new())
    }

    pub(super) fn resolve_export_value_wrapper_target_symbol(
        &self,
        value_decl: NodeIndex,
        escaped_name: &str,
    ) -> Option<SymbolId> {
        if value_decl.is_none() {
            return None;
        }
        let node = self.ctx.arena.get(value_decl)?;
        if node.kind != syntax_kind_ext::EXPORT_DECLARATION {
            return None;
        }
        let export_decl = self.ctx.arena.get_export_decl(node)?;
        if export_decl.export_clause.is_none() {
            return None;
        }

        let clause_idx = export_decl.export_clause;
        let clause_node = self.ctx.arena.get(clause_idx)?;

        if clause_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
            && let Some(var_stmt) = self.ctx.arena.get_variable(clause_node)
        {
            for &list_idx in &var_stmt.declarations.nodes {
                let Some(list_node) = self.ctx.arena.get(list_idx) else {
                    continue;
                };
                let Some(decl_list) = self.ctx.arena.get_variable(list_node) else {
                    continue;
                };
                for &decl_idx in &decl_list.declarations.nodes {
                    let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                        continue;
                    };
                    let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                        continue;
                    };
                    let Some(name_node) = self.ctx.arena.get(var_decl.name) else {
                        continue;
                    };
                    let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                        continue;
                    };
                    if ident.escaped_text == escaped_name
                        && let Some(&sym_id) = self.ctx.binder.node_symbols.get(&decl_idx.0)
                    {
                        return Some(sym_id);
                    }
                }
            }
        }

        self.ctx.binder.node_symbols.get(&clause_idx.0).copied()
    }

    pub(super) fn compute_local_export_value_wrapper_type(
        &mut self,
        sym_id: SymbolId,
        value_decl: NodeIndex,
        escaped_name: &str,
    ) -> Option<TypeId> {
        if value_decl.is_none() {
            return None;
        }

        if let Some(local_name) = self.get_declaration_name_text(value_decl)
            && local_name != escaped_name
            && let Some(local_sym_id) = self.ctx.binder.file_locals.get(&local_name)
            && local_sym_id != sym_id
        {
            return Some(self.get_type_of_symbol(local_sym_id));
        }

        let node = self.ctx.arena.get(value_decl)?;
        if let Some(exported_ident) = self.ctx.arena.get_identifier(node)
            && exported_ident.escaped_text != escaped_name
            && let Some(local_sym_id) = self
                .ctx
                .binder
                .file_locals
                .get(&exported_ident.escaped_text)
            && local_sym_id != sym_id
        {
            return Some(self.get_type_of_symbol(local_sym_id));
        }

        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }

        Some(self.type_of_value_declaration_for_symbol(sym_id, value_decl))
    }

    /// Resolve `TypeQuery` references in a type alias body using flow narrowing.
    pub(super) fn resolve_type_queries_with_flow(
        &mut self,
        alias_type: TypeId,
        type_node: NodeIndex,
    ) -> TypeId {
        if tsz_solver::collect_type_queries(self.ctx.types, alias_type).is_empty() {
            return alias_type;
        }

        let mut type_query_nodes = Vec::new();
        self.collect_type_query_nodes(type_node, &mut type_query_nodes);

        if type_query_nodes.is_empty() {
            return alias_type;
        }

        let mut any_changed = false;
        for tq_idx in &type_query_nodes {
            let narrowed = self.get_type_from_type_query(*tq_idx);
            let existing = self.ctx.node_types.get(&tq_idx.0).copied();
            if existing != Some(narrowed) {
                self.ctx.node_types.insert(tq_idx.0, narrowed);
                any_changed = true;
            }
        }

        if !any_changed {
            return alias_type;
        }

        self.ctx.node_types.remove(&type_node.0);
        self.get_type_from_type_node(type_node)
    }

    /// Recursively collect `TYPE_QUERY` node indices from a type node subtree.
    fn collect_type_query_nodes(&self, idx: NodeIndex, out: &mut Vec<NodeIndex>) {
        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::TYPE_QUERY {
            out.push(idx);
            return;
        }

        if node.kind == syntax_kind_ext::TYPE_LITERAL {
            if let Some(data) = self.ctx.arena.get_type_literal(node) {
                for &member_idx in &data.members.nodes {
                    self.collect_type_query_nodes(member_idx, out);
                }
            }
            return;
        }

        if node.kind == syntax_kind_ext::INDEX_SIGNATURE {
            if let Some(data) = self.ctx.arena.get_index_signature(node)
                && data.type_annotation.is_some()
            {
                self.collect_type_query_nodes(data.type_annotation, out);
            }
            return;
        }

        if node.kind == syntax_kind_ext::PROPERTY_SIGNATURE
            || node.kind == syntax_kind_ext::PROPERTY_DECLARATION
        {
            if let Some(data) = self.ctx.arena.get_property_decl(node)
                && data.type_annotation.is_some()
            {
                self.collect_type_query_nodes(data.type_annotation, out);
            }
            return;
        }

        if node.kind == syntax_kind_ext::UNION_TYPE
            || node.kind == syntax_kind_ext::INTERSECTION_TYPE
        {
            if let Some(data) = self.ctx.arena.get_composite_type(node) {
                for &member_idx in &data.types.nodes {
                    self.collect_type_query_nodes(member_idx, out);
                }
            }
            return;
        }

        if node.kind == syntax_kind_ext::ARRAY_TYPE
            && let Some(data) = self.ctx.arena.get_array_type(node)
        {
            self.collect_type_query_nodes(data.element_type, out);
        }
    }
}
