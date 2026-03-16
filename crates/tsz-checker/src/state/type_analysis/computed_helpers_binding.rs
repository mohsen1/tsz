use crate::query_boundaries::common::object_shape_for_type;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
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
}
