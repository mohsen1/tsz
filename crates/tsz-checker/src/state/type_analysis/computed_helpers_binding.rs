use crate::context::TypingRequest;
use crate::query_boundaries::common::{object_shape_for_type, union_members};
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn merged_value_type_for_symbol_if_available(
        &mut self,
        sym_id: SymbolId,
    ) -> Option<TypeId> {
        let symbol = self.get_symbol_globally(sym_id)?;
        let has_interface = symbol.has_any_flags(symbol_flags::INTERFACE);
        let has_variable = symbol.has_any_flags(
            symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::BLOCK_SCOPED_VARIABLE,
        );
        if !has_interface || !has_variable || symbol.value_declaration.is_none() {
            return None;
        }

        let value_type =
            self.type_of_value_declaration_for_symbol(sym_id, symbol.value_declaration);
        (!value_type.is_unknown_or_error()).then_some(value_type)
    }

    pub(crate) fn imported_namespace_display_module_name(&self, module_name: &str) -> String {
        // For relative imports, use the module specifier directly as the display
        // name rather than the resolved file path. This matches tsc, which shows
        // `typeof import("aliasAssignments_moduleA")` not the full resolved path.
        // For non-relative (bare) imports — package names like `"shortid"` —
        // tsc also uses the original specifier in `typeof import("...")` output,
        // not the resolved `node_modules/<pkg>/index` path. Treat both kinds
        // uniformly: the display name is the original specifier with `./` and
        // any known extension stripped.
        let trimmed = trim_namespace_display_path(module_name);
        tsz_common::file_extensions::strip_known_extension(&trimmed).to_string()
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

        if symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS) {
            let mut visited = AliasCycleTracker::new();
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
        let init_type =
            self.get_type_of_node_with_request(var_decl.initializer, &TypingRequest::NONE);
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
                t = self.ctx.types.factory().union2(t, TypeId::UNDEFINED);
            }
            if be_data.initializer.is_some() && self.ctx.strict_null_checks() {
                t = crate::query_boundaries::flow::narrow_destructuring_default(
                    self.ctx.types,
                    t,
                    true,
                );
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
                    t = self.ctx.types.factory().union2(t, TypeId::UNDEFINED);
                }
                if be_data.initializer.is_some() && self.ctx.strict_null_checks() {
                    t = crate::query_boundaries::flow::narrow_destructuring_default(
                        self.ctx.types,
                        t,
                        true,
                    );
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
                    t = self.ctx.types.factory().union2(t, TypeId::UNDEFINED);
                }
                if be_data.initializer.is_some() && self.ctx.strict_null_checks() {
                    t = crate::query_boundaries::flow::narrow_destructuring_default(
                        self.ctx.types,
                        t,
                        true,
                    );
                }
                return Some(t);
            }

            // For union types (e.g., { kind: 'A', payload: number } | { kind: 'B', payload: string }),
            // collect the property type from each union member and return their union.
            // This enables correlated narrowing for dependent destructured variables.
            if let Some(members) = union_members(self.ctx.types, ann_type) {
                let mut prop_types = Vec::new();
                for &member in &members {
                    let evaluated = self.evaluate_type_for_assignability(member);
                    if let Some(shape) = object_shape_for_type(self.ctx.types, evaluated)
                        && let Some(prop) = shape.properties.iter().find(|p| p.name == prop_atom)
                    {
                        let mut t = prop.type_id;
                        if prop.optional && self.ctx.strict_null_checks() {
                            t = self.ctx.types.factory().union2(t, TypeId::UNDEFINED);
                        }
                        prop_types.push(t);
                    }
                }
                if !prop_types.is_empty() {
                    let mut t = tsz_solver::utils::union_or_single(self.ctx.types, prop_types);
                    if be_data.initializer.is_some() && self.ctx.strict_null_checks() {
                        t = crate::query_boundaries::flow::narrow_destructuring_default(
                            self.ctx.types,
                            t,
                            true,
                        );
                    }
                    return Some(t);
                }
            }
        }

        None
    }

    /// Resolve a binding element type when the binding is nested inside another
    /// binding pattern in an annotated function parameter, e.g. `c` in
    /// `function f({ a: { b, c } }: { a: { b: number; c?: number } })`.
    ///
    /// `resolve_binding_element_from_annotated_param` only handles one level
    /// of destructure (Identifier → `BindingElement` → Pattern → Parameter).
    /// Anything deeper has a `Pattern → BindingElement → Pattern → ...` chain
    /// that the single-level walker rejects.  This helper walks the chain
    /// of arbitrary depth, collects the property-name path from innermost
    /// out to the parameter, then applies the path against the parameter's
    /// annotation.  At each step it propagates `| undefined` for optional
    /// properties and strips it for any binding-element default.
    pub(crate) fn resolve_nested_binding_element_from_annotated_param(
        &mut self,
        value_decl: NodeIndex,
        name: &str,
    ) -> Option<TypeId> {
        let node = self.ctx.arena.get(value_decl)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        // Walk: Identifier → BindingElement → Pattern → (BindingElement|Parameter) → ...
        // Collect (property_name, has_initializer) from innermost outward.
        let mut path: Vec<(String, bool)> = Vec::new();
        let mut current = value_decl;
        let param_idx = loop {
            let ext = self.ctx.arena.get_extended(current)?;
            let parent_idx = ext.parent;
            if !parent_idx.is_some() {
                return None;
            }
            let parent_node = self.ctx.arena.get(parent_idx)?;
            if parent_node.kind != syntax_kind_ext::BINDING_ELEMENT {
                return None;
            }
            let be_data = self.ctx.arena.get_binding_element(parent_node)?;
            // Property name resolution:
            //   - explicit property_name (`{ a: {b, c} }` outer element has property_name="a")
            //   - else: shorthand with `name` ident (`{ b, c }` inner element has no
            //     property_name; the property name equals the binding identifier)
            let prop_name_str = if be_data.property_name.is_some() {
                self.get_identifier_text_from_idx(be_data.property_name)?
            } else if path.is_empty() {
                // Innermost: caller provided the binding identifier text as `name`.
                name.to_string()
            } else {
                // Shorthand with no explicit property_name where `name` is itself a
                // sub-pattern. This shape can't occur in valid TS at outer levels,
                // so bail rather than guess.
                return None;
            };
            path.push((prop_name_str, be_data.initializer.is_some()));
            // Advance past BindingElement to the enclosing Pattern.
            let ext2 = self.ctx.arena.get_extended(parent_idx)?;
            let pat_idx = ext2.parent;
            if !pat_idx.is_some() {
                return None;
            }
            let pat_node = self.ctx.arena.get(pat_idx)?;
            if pat_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN
                && pat_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN
            {
                return None;
            }
            // Pattern's parent is either the next BindingElement (nested) or a
            // Parameter (terminal). Anything else means this is not a parameter
            // binding chain (e.g. variable declaration destructuring).
            let ext3 = self.ctx.arena.get_extended(pat_idx)?;
            let pat_parent_idx = ext3.parent;
            if !pat_parent_idx.is_some() {
                return None;
            }
            let pat_parent_node = self.ctx.arena.get(pat_parent_idx)?;
            if pat_parent_node.kind == syntax_kind_ext::PARAMETER {
                break pat_parent_idx;
            }
            if pat_parent_node.kind != syntax_kind_ext::BINDING_ELEMENT {
                return None;
            }
            // Continue walking from the pattern; next iteration's
            // get_extended(current).parent will yield the outer BindingElement.
            current = pat_idx;
        };

        // We need at least 2 levels of path entries to be meaningfully "nested";
        // single-level cases are handled by the existing
        // `resolve_binding_element_from_annotated_param`.
        if path.len() < 2 {
            return None;
        }

        // Resolve the parameter annotation type.
        let param_node = self.ctx.arena.get(param_idx)?;
        let param = self.ctx.arena.get_parameter(param_node)?;
        if !param.type_annotation.is_some() {
            return None;
        }
        let ann_type = self.get_type_from_type_node(param.type_annotation);
        if ann_type == TypeId::ANY || ann_type == TypeId::UNKNOWN || ann_type == TypeId::ERROR {
            return None;
        }

        // Walk the property-name path outermost-first against the annotation,
        // propagating optional `| undefined` for `?:` properties and stripping
        // it for elements that have a default initializer.
        let strict = self.ctx.strict_null_checks();
        let mut current_type = self.evaluate_type_for_assignability(ann_type);
        for (prop_name, has_init) in path.iter().rev() {
            let prop_atom = self.ctx.types.intern_string(prop_name);
            let shape = object_shape_for_type(self.ctx.types, current_type)?;
            let prop = shape.properties.iter().find(|p| p.name == prop_atom)?;
            let mut t = prop.type_id;
            if prop.optional && strict {
                t = self.ctx.types.factory().union2(t, TypeId::UNDEFINED);
            }
            if *has_init && strict {
                t = crate::query_boundaries::flow::narrow_destructuring_default(
                    self.ctx.types,
                    t,
                    true,
                );
            }
            current_type = self.evaluate_type_for_assignability(t);
        }
        Some(current_type)
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
            // Build instance type FIRST so that the constructor type's construct
            // signatures can use the real instance type instead of a rough
            // approximation. This ensures that static methods like
            // `static getInstance() { return new C(); }` infer the correct
            // return type when the class is a class expression.
            let instance_type = self.get_class_instance_type(decl_idx, class);
            // Guard: don't overwrite a valid cached instance type with a degraded
            // value (ERROR/ANY). This happens when compute_type_of_symbol is called
            // re-entrantly from within get_class_instance_type_inner (e.g., during
            // prescan of a method whose return type references the same class).
            // The re-entrant get_class_instance_type hits the in-progress guard and
            // returns ERROR/ANY, which would corrupt the previously-cached correct type.
            if (instance_type == TypeId::ANY || instance_type == TypeId::ERROR)
                && let Some(&existing) = self.ctx.symbol_instance_types.get(&sym_id)
                && existing != TypeId::ANY
                && existing != TypeId::ERROR
            {
                // Keep the existing valid type; skip the degraded overwrite.
                let ctor_type = self.get_class_constructor_type(decl_idx, class);
                self.ctx.symbol_types.insert(sym_id, ctor_type);

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
            self.ctx.symbol_instance_types.insert(sym_id, instance_type);

            let ctor_type = self.get_class_constructor_type(decl_idx, class);
            // Guard against constructor type cache corruption.
            //
            // When an outer `get_class_constructor_type(C)` is already in
            // progress (typical for self-referential static member types
            // like `static instance: C<string>[]`), a nested
            // `get_type_of_symbol(C)` can re-enter this function. The
            // nested call's `get_class_constructor_type(C)` hits the
            // in-progress guard and returns a cycle-fallback: the current
            // `symbol_types[C]`, which at this point is the `Lazy(DefId)`
            // placeholder that `get_type_of_symbol_inner` installed. That
            // Lazy resolves to the class's INSTANCE type (not the
            // constructor type). Storing it in `symbol_types[C]` would
            // corrupt later value-position lookups of `C` (e.g.
            // `C.instance` in an instance method body → false TS2339).
            //
            // Detect the degenerate case (ctor_type is a Lazy pointing at
            // the class's own DefId) and skip the cache overwrite so that
            // the outer resolution, once it completes, keeps providing the
            // correct constructor type on the next lookup.
            let is_degenerate_lazy = {
                use crate::query_boundaries::common as common_query;
                let lazy_def =
                    common_query::lazy_def_id(self.ctx.types.as_type_database(), ctor_type);
                let own_def = self.ctx.get_existing_def_id(sym_id);
                lazy_def.is_some() && lazy_def == own_def
            };
            if !is_degenerate_lazy {
                self.ctx.symbol_types.insert(sym_id, ctor_type);
            }

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

        // Cross-file fallback: the class declaration might be in a different file's
        // arena. This happens when a constructor function in one JS file merges with
        // a class declaration in another JS file (SALSA mode). The merged symbol has
        // CLASS flag but the class node is only accessible in the declaring file's arena.
        // Search all available arenas for the class node, then build the class type
        // using a child checker with the correct arena. We directly call
        // get_class_instance_type/get_class_constructor_type instead of
        // get_type_of_symbol to avoid SymbolId collisions across binders.
        // Only attempt cross-file fallback when the current arena is among the
        // user file arenas. This prevents lib delegation child checkers (whose
        // arena is a lib arena, not in all_arenas) from incorrectly picking up
        // class nodes from user files due to NodeIndex collisions.
        let current_arena_is_user_file = self.ctx.all_arenas.as_ref().is_some_and(|arenas| {
            arenas
                .iter()
                .any(|a| std::ptr::eq(a.as_ref(), self.ctx.arena))
        });
        let sym_name = if current_arena_is_user_file {
            self.get_symbol_globally(sym_id)
                .map(|s| s.escaped_name.clone())
        } else {
            None
        };
        if let Some(all_arenas) = self.ctx.all_arenas.as_ref()
            && let Some(ref sym_name) = sym_name
        {
            for (file_idx, arena) in all_arenas.iter().enumerate() {
                if std::ptr::eq(arena.as_ref(), self.ctx.arena) {
                    continue;
                }
                // Find a class declaration in this arena, verifying the name matches
                // to prevent NodeIndex collisions across arenas.
                let cross_decl_idx = declarations
                    .iter()
                    .chain(std::iter::once(&value_decl))
                    .find(|&&d| {
                        d.is_some()
                            && arena
                                .get(d)
                                .and_then(|n| arena.get_class(n))
                                .and_then(|class| {
                                    let name_node = arena.get(class.name)?;
                                    let ident = arena.get_identifier(name_node)?;
                                    Some(ident.escaped_text == *sym_name)
                                })
                                .unwrap_or(false)
                    })
                    .copied()
                    .unwrap_or(NodeIndex::NONE);
                if cross_decl_idx.is_none() {
                    continue;
                }
                // Fast path: if a parallel worker already resolved this
                // (sym_id, file_idx) pair, the canonical SYMBOL_TYPE bucket
                // has the class type. Short-circuit before building a child
                // checker — but first populate `symbol_instance_types` from
                // the parallel CLASS_INSTANCE_TYPE bucket. Without this,
                // TYPE-position references (e.g. `let x: MyClass`) fall
                // through to `class_instance_type_with_params_from_symbol`,
                // which only searches the *current* arena and returns None
                // for cross-file classes — so the constructor type leaks
                // into the instance position.
                if let Some(cached) = self
                    .ctx
                    .cached_cross_file_symbol_type(sym_id, file_idx as u32)
                {
                    if let Some((inst_type, _)) =
                        self.ctx.definition_store.get_resolved_cross_file_query(
                            super::cross_file::CROSS_FILE_QUERY_CLASS_INSTANCE_TYPE,
                            file_idx as u32,
                            sym_id.0,
                            0,
                            0,
                        )
                        && inst_type != TypeId::ANY
                        && inst_type != TypeId::ERROR
                    {
                        self.ctx.symbol_instance_types.insert(sym_id, inst_type);
                    }
                    return cached;
                }
                // Found class in another file's arena. Create a child checker
                // with that arena and directly compute the class type.
                if !Self::enter_cross_arena_delegation() {
                    return (TypeId::ERROR, Vec::new());
                }
                if !self.ctx.enter_recursion() {
                    Self::leave_cross_arena_delegation();
                    return (TypeId::ERROR, Vec::new());
                }

                let delegate_binder = self
                    .ctx
                    .get_binder_for_file(file_idx)
                    .unwrap_or(self.ctx.binder);
                let delegate_file_name = arena
                    .source_files
                    .first()
                    .map(|sf| sf.file_name.clone())
                    .unwrap_or_else(|| self.ctx.file_name.clone());

                let mut checker = Box::new(CheckerState::with_parent_cache_attributed(
                    arena.as_ref(),
                    delegate_binder,
                    self.ctx.types,
                    delegate_file_name,
                    self.ctx.compiler_options.clone(),
                    self,
                    tsz_common::perf_counters::CheckerCreationReason::BindingHelpers,
                ));
                checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
                checker.ctx.copy_cross_file_state_from(&self.ctx);
                self.ctx.copy_symbol_file_targets_to_attributed(
                    &mut checker.ctx,
                    tsz_common::perf_counters::CheckerCreationReason::BindingHelpers,
                );
                checker.ctx.current_file_idx = file_idx;
                for &id in &self.ctx.class_instance_resolution_set {
                    checker.ctx.class_instance_resolution_set.insert(id);
                }
                for &id in &self.ctx.class_constructor_resolution_set {
                    checker.ctx.class_constructor_resolution_set.insert(id);
                }

                // Directly compute the class type using the cross-arena class node.
                // We must re-fetch the class reference between calls because
                // get_class_instance_type/get_class_constructor_type take &mut self.
                let (result, cross_instance_type) = if checker
                    .ctx
                    .arena
                    .get(cross_decl_idx)
                    .and_then(|n| checker.ctx.arena.get_class(n))
                    .is_some()
                {
                    // Phase 1: compute instance type
                    let class_ref = checker
                        .ctx
                        .arena
                        .get(cross_decl_idx)
                        .expect("cross_decl_idx presence was verified by is_some() above");
                    let class = checker
                        .ctx
                        .arena
                        .get_class(class_ref)
                        .expect("cross_decl class shape verified by is_some() above");
                    let instance_type = checker.get_class_instance_type(cross_decl_idx, class);
                    // Phase 2: compute constructor type (re-fetch class reference)
                    let class_ref = checker
                        .ctx
                        .arena
                        .get(cross_decl_idx)
                        .expect("cross_decl_idx presence was verified by is_some() above");
                    let class = checker
                        .ctx
                        .arena
                        .get_class(class_ref)
                        .expect("cross_decl class shape verified by is_some() above");
                    let ctor_type = checker.get_class_constructor_type(cross_decl_idx, class);
                    (ctor_type, Some(instance_type))
                } else {
                    (TypeId::UNKNOWN, None)
                };

                // Collect child data before dropping (checker borrows from self)
                let child_instance_types: Vec<(SymbolId, TypeId)> =
                    checker.ctx.symbol_instance_types.iter().collect();
                drop(checker);

                // Now safe to mutate self
                if let Some(inst) = cross_instance_type
                    && inst != TypeId::ANY
                    && inst != TypeId::ERROR
                {
                    self.ctx.symbol_instance_types.insert(sym_id, inst);
                }
                for (k, v) in child_instance_types {
                    self.ctx.symbol_instance_types.entry_or_insert(k, v);
                }

                Self::leave_cross_arena_delegation();
                self.ctx.leave_recursion();

                if result != TypeId::UNKNOWN && result != TypeId::ERROR {
                    return (result, Vec::new());
                }
            }
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
            string_index: shape.string_index,
            number_index: shape.number_index,
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

        while parent.is_some() {
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
    ///
    /// For pure namespace symbols (not merged with class/function/enum),
    /// builds a structural object type from the namespace's value exports.
    /// This is critical for assignability checking between `typeof Namespace`
    /// types: without a structural type, `resolve_lazy(DefId)` returns
    /// `Lazy(DefId)` (circular), and the subtype checker's cycle detection
    /// incorrectly assumes compatibility (TS2741 is suppressed).
    pub(super) fn compute_namespace_symbol_type(
        &mut self,
        sym_id: SymbolId,
        flags: u32,
    ) -> (TypeId, Vec<tsz_solver::TypeParamInfo>) {
        // Compute the interface/type-alias types for type-position resolution.
        // Note: compute_interface_type_from_declarations has the side effect of
        // inserting the interface type into symbol_instance_types. We need to
        // temporarily remove it before calling build_namespace_object_type,
        // because that function uses symbol_instance_types as a cache and would
        // return the interface type instead of building the structural namespace
        // object type. This fixes false TS2403 errors for merged
        // namespace+interface symbols like `typeof M2.Point`.
        let interface_type = if flags & symbol_flags::INTERFACE != 0 {
            let it = self.compute_interface_type_from_declarations(sym_id);
            Some(it)
        } else {
            None
        };

        if flags & symbol_flags::TYPE_ALIAS != 0
            && let Some(alias_type) = self.compute_type_alias_body(sym_id)
        {
            self.ctx.symbol_instance_types.insert(sym_id, alias_type);
        }

        let def_id = self.ctx.get_or_create_def_id(sym_id);
        let factory = self.ctx.types.factory();
        let lazy = factory.lazy(def_id);

        // Build a structural object type from the namespace's value exports.
        // Only materialize if the namespace has instantiated value exports.
        // Type-only namespaces (only interfaces/type aliases) must keep Lazy(DefId)
        // so property access goes through namespace export resolution.
        if self.namespace_has_value_exports(sym_id) {
            // Clear any stale symbol_instance_types entry (from
            // compute_interface_type_from_declarations side effect) so that
            // build_namespace_object_type builds the real structural object type
            // instead of returning the cached interface type.
            if interface_type.is_some() {
                self.ctx.symbol_instance_types.remove(&sym_id);
            }
            let ns_obj = self.build_namespace_object_type(sym_id);
            // Restore the interface type for type-position resolution.
            // The namespace object type is returned as the value-position type
            // (stored in symbol_types by the caller), while the interface type
            // is the instance type used for type annotations like `M2.Point`.
            if let Some(it) = interface_type {
                self.ctx.symbol_instance_types.insert(sym_id, it);
            }
            (ns_obj, Vec::new())
        } else {
            (lazy, Vec::new())
        }
    }

    /// Check if a namespace has any instantiated value-level exports.
    fn namespace_has_value_exports(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let Some(exports) = symbol.exports.as_ref() else {
            return false;
        };
        for (_name, member_id) in exports.iter() {
            let Some(member_symbol) = self.ctx.binder.get_symbol(*member_id) else {
                continue;
            };
            let value_flags_except_module = symbol_flags::VALUE & !symbol_flags::VALUE_MODULE;
            if member_symbol.has_any_flags(value_flags_except_module) {
                return true;
            }
            // Namespace-only members: check if instantiated
            if member_symbol.has_any_flags(symbol_flags::VALUE_MODULE)
                && member_symbol.has_any_flags(symbol_flags::NAMESPACE_MODULE)
            {
                for &decl_idx in &member_symbol.declarations {
                    if self.is_namespace_declaration_instantiated(decl_idx) {
                        return true;
                    }
                }
            }
        }
        false
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

        // `export default C.B` needs the dedicated value-side property access
        // resolver in `compute_local_export_value_wrapper_type`. Returning the
        // property-access node symbol directly here can pick up the type-side
        // merged member (`interface B`) instead of the runtime static property.
        if clause_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }

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

        if let Some(ident) = self.ctx.arena.get_identifier(clause_node) {
            if let Some(sym_id) = self.ctx.binder.file_locals.get(&ident.escaped_text) {
                return Some(sym_id);
            }

            if ident.escaped_text != escaped_name
                && let Some(sym_id) = self.ctx.binder.file_locals.get(&ident.escaped_text)
            {
                return Some(sym_id);
            }
        }

        self.ctx.binder.get_node_symbol(clause_idx)
    }

    fn default_export_wrapper_expression(
        &self,
        value_decl: NodeIndex,
    ) -> Option<(NodeIndex, NodeIndex)> {
        let export_decl_idx =
            self.ctx
                .arena
                .get(value_decl)
                .filter(|node| node.kind == syntax_kind_ext::EXPORT_DECLARATION)
                .map(|_| value_decl)
                .or_else(|| {
                    self.ctx
                        .arena
                        .get_extended(value_decl)
                        .map(|ext| ext.parent)
                        .filter(|parent| {
                            self.ctx.arena.get(*parent).is_some_and(|node| {
                                node.kind == syntax_kind_ext::EXPORT_DECLARATION
                            })
                        })
                })?;

        let export_decl = self
            .ctx
            .arena
            .get(export_decl_idx)
            .and_then(|node| self.ctx.arena.get_export_decl(node))?;

        if !export_decl.is_default_export || export_decl.export_clause.is_none() {
            return None;
        }

        // Do NOT treat declaration-form export defaults (interface, type alias, enum)
        // as wrapper expressions. These are named declarations, not expressions, and
        // should be resolved via their declared name in file_locals instead.
        if let Some(clause_node) = self.ctx.arena.get(export_decl.export_clause) {
            match clause_node.kind {
                syntax_kind_ext::INTERFACE_DECLARATION
                | syntax_kind_ext::TYPE_ALIAS_DECLARATION
                | syntax_kind_ext::ENUM_DECLARATION => return None,
                _ => {}
            }
        }

        Some((export_decl.export_clause, export_decl_idx))
    }

    fn default_export_expression_is_directly_deferred(&self, expr_idx: NodeIndex) -> bool {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        self.ctx.arena.get(expr_idx).is_some_and(|node| {
            matches!(
                node.kind,
                syntax_kind_ext::FUNCTION_DECLARATION
                    | syntax_kind_ext::FUNCTION_EXPRESSION
                    | syntax_kind_ext::ARROW_FUNCTION
                    | syntax_kind_ext::CLASS_DECLARATION
                    | syntax_kind_ext::CLASS_EXPRESSION
            )
        })
    }

    /// Check if the export default expression contains an identifier that is an
    /// import alias from the same file (self-import). This indicates genuine
    /// circular self-reference for TS7022 purposes.
    ///
    /// Example: in `QSpinner.js`, `import DefaultSpinner from './QSpinner'`
    /// followed by `export default { mixins: [DefaultSpinner] }` is self-referential.
    fn expression_has_self_file_import(&self, node_idx: NodeIndex) -> bool {
        let file_stem = self.current_file_stem();
        self.expression_has_self_file_import_inner(node_idx, &file_stem)
    }

    /// Extract the file stem (base name without extension) from the current file.
    fn current_file_stem(&self) -> String {
        let file_name = &self.ctx.file_name;
        let base = file_name.rsplit('/').next().unwrap_or(file_name);
        // Also handle Windows path separators
        let base = base.rsplit('\\').next().unwrap_or(base);
        tsz_common::file_extensions::strip_known_extension(base).to_string()
    }

    fn expression_has_self_file_import_inner(&self, node_idx: NodeIndex, file_stem: &str) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        // Check if this identifier references an import alias from the same file.
        // NOTE: We look up by identifier text in file_locals rather than using
        // get_node_symbol/resolve_identifier, because the binder may resolve
        // import aliases to their target symbols (e.g., DefaultSpinner → default
        // export symbol), losing the import_module information we need.
        if node.kind == SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.ctx.arena.get_identifier(node) {
                let ident_text = &ident.escaped_text;
                if let Some(local_sym_id) = self.ctx.binder.file_locals.get(ident_text)
                    && let Some(symbol) = self.ctx.binder.get_symbol(local_sym_id)
                    && symbol.has_any_flags(symbol_flags::ALIAS)
                    && let Some(ref import_module) = symbol.import_module
                {
                    let last_segment = import_module.rsplit('/').next().unwrap_or(import_module);
                    if last_segment == file_stem {
                        return true;
                    }
                }
            }
            return false;
        }

        // Stop at deferred boundaries — self-references inside these are benign
        if matches!(
            node.kind,
            syntax_kind_ext::FUNCTION_EXPRESSION
                | syntax_kind_ext::ARROW_FUNCTION
                | syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::METHOD_DECLARATION
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::CLASS_EXPRESSION
        ) {
            return false;
        }

        // Recurse into children
        for child_idx in self.ctx.arena.get_children(node_idx) {
            if self.expression_has_self_file_import_inner(child_idx, file_stem) {
                return true;
            }
        }

        false
    }

    pub(super) fn compute_local_export_value_wrapper_type(
        &mut self,
        sym_id: SymbolId,
        value_decl: NodeIndex,
        escaped_name: &str,
    ) -> Option<TypeId> {
        if let Some((expr_idx, export_decl_idx)) =
            self.default_export_wrapper_expression(value_decl)
            && self.jsdoc_type_annotation_for_node(value_decl).is_none()
            && !self.has_satisfies_jsdoc_comment(expr_idx)
        {
            let snap = self.ctx.snapshot_diagnostics();
            let wrapped_type =
                if let Some(val_type) = self.resolve_property_access_value_type(expr_idx) {
                    val_type
                } else {
                    self.type_of_value_declaration_for_symbol(sym_id, expr_idx)
                };

            // Detect genuine circular self-reference for TS7022.
            //
            // We're inside compute_type_of_symbol, which always sets an ERROR
            // placeholder for non-named-entity symbols before computing. This means
            // `sym_cached_as_error` is unreliable (always true). Instead, check
            // whether the expression contains an identifier that imports from the
            // same file — the defining characteristic of a self-referential export
            // default (e.g., `import X from './SameFile'; export default { x: X }`).
            //
            // This avoids false-positive TS7022 for:
            //   - Type-only exports: `export default InterfaceName`
            //   - Non-circular imports: `export default wrapClass(0)`
            //   - Ambient declarations: `export default 2 + 2` in .d.ts
            if self.ctx.no_implicit_any()
                && self.expression_has_self_file_import(expr_idx)
                && !self.default_export_expression_is_directly_deferred(expr_idx)
            {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

                self.suppress_circular_initializer_relation_diagnostics(&snap, expr_idx);
                let message = format_message(
                    diagnostic_messages::IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION_AND_IS_REFERE,
                    &["default"],
                );

                self.error_at_node(
                    export_decl_idx,
                    &message,
                    diagnostic_codes::IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION_AND_IS_REFERE,
                );

                return Some(TypeId::ANY);
            }

            return Some(wrapped_type);
        }

        if value_decl.is_none() {
            return None;
        }

        if let Some(local_name) = self.get_declaration_name_text(value_decl)
            && local_name != escaped_name
            && let Some(local_sym_id) = self.ctx.binder.file_locals.get(&local_name)
            && local_sym_id != sym_id
        {
            return Some(
                self.merged_value_type_for_symbol_if_available(local_sym_id)
                    .unwrap_or_else(|| self.get_type_of_symbol(local_sym_id)),
            );
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
            return Some(
                self.merged_value_type_for_symbol_if_available(local_sym_id)
                    .unwrap_or_else(|| self.get_type_of_symbol(local_sym_id)),
            );
        }

        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }

        // For PropertyAccessExpression value declarations (e.g., `export default C.B`),
        // resolve the VALUE member specifically.  When a class+namespace merge has both
        // a static property and a namespace-exported interface with the same name,
        // `get_type_of_node` may return the interface type (type meaning) instead of
        // the static property type (value meaning).  We resolve the base symbol's VALUE
        // member directly to avoid this ambiguity.
        if node.kind == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(val_type) = self.resolve_property_access_value_type(value_decl)
        {
            return Some(val_type);
        }

        Some(self.type_of_value_declaration_for_symbol(sym_id, value_decl))
    }

    /// Resolve the VALUE meaning of a `PropertyAccessExpression`.
    ///
    /// For `C.B` where `C` is a class merged with a namespace and `B` is both a
    /// static property and a namespace-exported interface, the expression evaluator
    /// may return the interface type (type meaning). This helper resolves the base
    /// symbol's exports to find a VALUE-flagged member and return its type.
    fn resolve_property_access_value_type(
        &mut self,
        expr_idx: NodeIndex,
    ) -> Option<tsz_solver::TypeId> {
        let node = self.ctx.arena.get(expr_idx)?;
        let access = self.ctx.arena.get_access_expr(node)?;
        let name_node = self.ctx.arena.get(access.name_or_argument)?;
        let name_ident = self.ctx.arena.get_identifier(name_node)?;
        let member_name = &name_ident.escaped_text;

        let base_node = self.ctx.arena.get(access.expression)?;
        let base_ident = self.ctx.arena.get_identifier(base_node)?;
        let base_name = &base_ident.escaped_text;

        let base_sym_id = self.ctx.binder.file_locals.get(base_name)?;
        let lib_binders = self.get_lib_binders();
        let base_symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(base_sym_id, &lib_binders)?;

        // Only apply this fix for merged class+namespace symbols where the
        // member has both a value and a type meaning.
        let is_merged = base_symbol.has_any_flags(symbol_flags::CLASS)
            && base_symbol
                .has_any_flags(symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE);
        if !is_merged {
            return None;
        }

        let exports = base_symbol.exports.as_ref()?;
        // Look for VALUE-flagged members only — skip INTERFACE/TYPE_ALIAS.
        // When both a static property and a namespace-exported type share the
        // same name, the binder stores them as separate symbols in the
        // export table; we want the PROPERTY/VARIABLE one.
        let member_sym_id = exports.get(member_name)?;
        let member_symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(member_sym_id, &lib_binders)?;
        if member_symbol.has_any_flags(symbol_flags::TYPE)
            && !member_symbol.has_any_flags(symbol_flags::VALUE)
        {
            // The export is type-only; look for a sibling value member.
            // Check the class's own members for a static property with the same name.
            // In merged class+namespace, the class stores static properties as
            // class members, while namespace stores the interface in exports.
            let class_members = base_symbol.members.as_ref()?;
            let static_sym_id = class_members.get(member_name)?;
            let static_sym = self
                .ctx
                .binder
                .get_symbol_with_libs(static_sym_id, &lib_binders)?;
            if static_sym.has_any_flags(symbol_flags::PROPERTY) {
                return Some(self.get_type_of_symbol(static_sym_id));
            }
            return None;
        }

        // If the export itself is a value (e.g., the binder merged static prop
        // into exports), return its type.
        if member_symbol.has_any_flags(symbol_flags::VALUE) {
            return Some(self.get_type_of_symbol(member_sym_id));
        }

        None
    }

    /// Resolve `TypeQuery` references in a type alias body using flow narrowing.
    ///
    /// When a type alias contains `typeof expr` inside a narrowed scope (e.g.
    /// inside `if (typeof c === 'string')`), the initial lowering creates
    /// `TypeQuery(SymbolRef)` which resolves to the declared type, not the
    /// flow-narrowed type. This method re-resolves such references by:
    /// 1. Finding `TYPE_QUERY` nodes in the AST
    /// 2. Resolving each query's expression with flow narrowing applied
    /// 3. Re-lowering the type node with the narrowed types cached
    pub(super) fn resolve_type_queries_with_flow(
        &mut self,
        alias_type: TypeId,
        type_node: NodeIndex,
    ) -> TypeId {
        if crate::query_boundaries::common::collect_type_queries(self.ctx.types, alias_type)
            .is_empty()
        {
            return alias_type;
        }

        let mut type_query_nodes = Vec::new();
        self.collect_type_query_nodes(type_node, &mut type_query_nodes);

        if type_query_nodes.is_empty() {
            return alias_type;
        }

        let mut any_changed = false;
        for tq_idx in &type_query_nodes {
            // Resolve the type query's expression with flow narrowing.
            // The standard get_type_from_type_query delegates to TypeNodeChecker
            // which doesn't apply flow narrowing. Instead, resolve the expression
            // identifier directly using get_type_of_node which applies flow analysis.
            let narrowed = self.resolve_type_query_with_flow(*tq_idx);
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

    /// Resolve a single `TYPE_QUERY` node with flow narrowing applied.
    ///
    /// For simple identifiers (e.g. `typeof c`), resolves the identifier's type
    /// using `get_type_of_node` which applies control-flow narrowing. For other
    /// forms, falls back to the standard `get_type_from_type_query`.
    fn resolve_type_query_with_flow(&mut self, tq_idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(tq_idx) else {
            return self.get_type_from_type_query(tq_idx);
        };
        let Some(type_query) = self.ctx.arena.get_type_query(node) else {
            return self.get_type_from_type_query(tq_idx);
        };

        let expr_name = type_query.expr_name;
        let Some(expr_node) = self.ctx.arena.get(expr_name) else {
            return self.get_type_from_type_query(tq_idx);
        };

        // Only apply flow-aware resolution for simple identifiers
        if expr_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return self.get_type_from_type_query(tq_idx);
        }

        // Resolve the identifier's type with flow narrowing enabled.
        let expr_type = self.get_type_of_node_with_request(expr_name, &TypingRequest::NONE);

        // If we got a useful type (not ANY/ERROR), use it.
        // Otherwise fall back to the standard non-flow path.
        if expr_type != TypeId::ANY && expr_type != TypeId::ERROR {
            expr_type
        } else {
            self.get_type_from_type_query(tq_idx)
        }
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

    /// Check if a symbol is a type-only export (excludable from namespace value type).
    pub(crate) fn is_type_only_export_symbol(&self, sym_id: SymbolId) -> bool {
        let symbol = self.get_cross_file_symbol(sym_id);
        let Some(symbol) = symbol else {
            return false;
        };
        if !symbol.is_type_only {
            return false;
        }
        if symbol.has_any_flags(symbol_flags::ALIAS) && symbol.has_any_flags(symbol_flags::VALUE) {
            return false;
        }
        true
    }

    /// Check if an export symbol has no value component (type-only).
    pub(crate) fn export_symbol_has_no_value(&self, sym_id: SymbolId) -> bool {
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders));
        let Some(symbol) = symbol else {
            return false;
        };

        let flags = symbol.flags;
        if (flags & symbol_flags::VALUE) != 0 {
            if (flags & symbol_flags::VALUE_MODULE) != 0
                && (flags & symbol_flags::NAMESPACE_MODULE) != 0
                && self.is_module_uninstantiated(sym_id)
            {
                return true;
            }
            return false;
        }
        if (flags & symbol_flags::NAMESPACE_MODULE) != 0 {
            return true;
        }
        if (flags & symbol_flags::TYPE) != 0 {
            return true;
        }
        if flags & symbol_flags::ALIAS != 0 {
            let mut visited = AliasCycleTracker::new();
            if let Some(target) = self.resolve_alias_symbol(sym_id, &mut visited) {
                let target_sym = self
                    .get_cross_file_symbol(target)
                    .or_else(|| self.ctx.binder.get_symbol_with_libs(target, &lib_binders));
                if let Some(target_sym) = target_sym {
                    let tf = target_sym.flags;
                    if (tf & symbol_flags::VALUE) != 0 {
                        if (tf & symbol_flags::VALUE_MODULE) != 0
                            && (tf & symbol_flags::NAMESPACE_MODULE) != 0
                            && self.is_module_uninstantiated(target)
                        {
                            return true;
                        }
                        return false;
                    }
                    if (tf & symbol_flags::NAMESPACE_MODULE) != 0 {
                        return true;
                    }
                    return (tf & symbol_flags::TYPE) != 0;
                }
            }
        }
        false
    }

    fn is_module_uninstantiated(&self, sym_id: SymbolId) -> bool {
        let lib_binders = self.get_lib_binders();
        let symbol = self
            .get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders));
        let Some(symbol) = symbol else {
            return false;
        };
        let Some(exports) = &symbol.exports else {
            return true;
        };
        for (_, &export_sym_id) in exports.iter() {
            let export_sym = self.get_cross_file_symbol(export_sym_id).or_else(|| {
                self.ctx
                    .binder
                    .get_symbol_with_libs(export_sym_id, &lib_binders)
            });
            let Some(export_sym) = export_sym else {
                continue;
            };
            let ef = export_sym.flags;
            if (ef & (symbol_flags::VALUE & !symbol_flags::VALUE_MODULE)) != 0 {
                return false;
            }
            if (ef & symbol_flags::VALUE_MODULE) != 0
                && !self.is_module_uninstantiated(export_sym_id)
            {
                return false;
            }
        }
        true
    }

    /// Check if a named export was reached through a `export type *` wildcard chain.
    pub(crate) fn is_export_from_type_only_wildcard(
        &self,
        module_name: &str,
        export_name: &str,
    ) -> bool {
        let Some(target_file_idx) = self.ctx.resolve_import_target(module_name) else {
            return false;
        };
        let Some(target_binder) = self.ctx.get_binder_for_file(target_file_idx) else {
            return false;
        };
        let target_file_name = self
            .ctx
            .get_arena_for_file(target_file_idx as u32)
            .source_files
            .first()
            .map(|sf| sf.file_name.as_str());
        let Some(file_name) = target_file_name else {
            return false;
        };
        if let Some((sym_id, true)) =
            target_binder.resolve_import_with_reexports_type_only(file_name, export_name)
        {
            if let Some(sym) = target_binder.symbols.get(sym_id)
                && sym.has_any_flags(symbol_flags::ALIAS)
                && sym.has_any_flags(symbol_flags::VALUE)
            {
                return false;
            }
            true
        } else {
            false
        }
    }

    pub(crate) fn report_private_identifier_outside_class(
        &mut self,
        name_idx: NodeIndex,
        property_name: &str,
        object_type: TypeId,
        object_expr: NodeIndex,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        let class_name = self.get_private_identifier_declaring_class_name(
            object_type,
            object_expr,
            property_name,
        );
        let message = format_message(
            diagnostic_messages::PROPERTY_IS_NOT_ACCESSIBLE_OUTSIDE_CLASS_BECAUSE_IT_HAS_A_PRIVATE_IDENTIFIER,
            &[property_name, &class_name],
        );
        self.error_at_node(
            name_idx,
            &message,
            diagnostic_codes::PROPERTY_IS_NOT_ACCESSIBLE_OUTSIDE_CLASS_BECAUSE_IT_HAS_A_PRIVATE_IDENTIFIER,
        );
    }

    pub(crate) fn report_private_identifier_shadowed(
        &mut self,
        name_idx: NodeIndex,
        property_name: &str,
        object_type: TypeId,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        let type_string = self
            .get_class_display_name_from_type(object_type)
            .unwrap_or_else(|| self.format_type_diagnostic(object_type));
        let message = format_message(
            diagnostic_messages::THE_PROPERTY_CANNOT_BE_ACCESSED_ON_TYPE_WITHIN_THIS_CLASS_BECAUSE_IT_IS_SHADOWED,
            &[property_name, &type_string],
        );
        self.error_at_node(
            name_idx,
            &message,
            diagnostic_codes::THE_PROPERTY_CANNOT_BE_ACCESSED_ON_TYPE_WITHIN_THIS_CLASS_BECAUSE_IT_IS_SHADOWED,
        );
    }

    /// Returns true if `sym_id` is a merged interface+value symbol.
    pub(crate) fn is_merged_interface_value_symbol(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let flags = symbol.flags;
        (flags & symbol_flags::INTERFACE) != 0 && (flags & symbol_flags::VALUE) != 0
    }
}

/// Normalize a resolved file path to the display form used in `typeof import("...")`.
///
/// Rules (mirroring tsc):
/// - Virtual-FS-root `node_modules` (`/node_modules/pkg/…`, `node_modules_idx == 0`):
///   keep the full root-relative path so the message reads
///   `import("node_modules/pkg/index")`.
/// - Paths with a virtual-root prefix (`/p123/node_modules/…`):
///   strip the absolute prefix but keep from the `p123` segment onwards.
/// - Deeper project paths (`/home/user/project/node_modules/pkg/…`):
///   strip the host/project prefix and keep the package subpath
///   (`node_modules/pkg/...`) so resolved declaration packages match tsc's
///   stable display form.
/// - No `node_modules` segment: return the trimmed path as-is.
pub(crate) fn trim_namespace_display_path(resolved_name: &str) -> String {
    let trimmed = resolved_name
        .strip_prefix("./")
        .unwrap_or(resolved_name)
        .trim_start_matches('/');

    let components: Vec<_> = trimmed
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    if let Some(node_modules_idx) = components
        .iter()
        .position(|segment| *segment == "node_modules")
    {
        if node_modules_idx > 0 {
            let previous = components[node_modules_idx - 1];
            let looks_like_virtual_root =
                previous.starts_with('p') && previous[1..].chars().all(|ch| ch.is_ascii_digit());
            if looks_like_virtual_root {
                return components[node_modules_idx - 1..].join("/");
            }
        }
        // Resolved declaration packages display from their stable
        // package path, not the original bare specifier. Drop any
        // host temp/project prefix before node_modules, but preserve
        // the package subpath that tsc includes in diagnostics.
        return components[node_modules_idx..].join("/");
    }

    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::trim_namespace_display_path;

    #[test]
    fn virtual_fs_root_node_modules_keeps_full_path() {
        // `/node_modules/pkg/index.d.ts` → `node_modules/pkg/index.d.ts`
        // (caller strips extension; we keep the full path including node_modules)
        assert_eq!(
            trim_namespace_display_path("/node_modules/mdast-util-to-string/index.d.ts"),
            "node_modules/mdast-util-to-string/index.d.ts"
        );
    }

    #[test]
    fn virtual_fs_root_scoped_package_keeps_full_path() {
        assert_eq!(
            trim_namespace_display_path("/node_modules/@scope/pkg/index.d.ts"),
            "node_modules/@scope/pkg/index.d.ts"
        );
    }

    #[test]
    fn deep_project_path_keeps_package_subpath() {
        // Real project: /home/user/project/node_modules/shortid/index.d.ts →
        // "node_modules/shortid/index.d.ts" (host/project prefix dropped, package
        // subpath preserved to match tsc's stable display form).
        assert_eq!(
            trim_namespace_display_path("/home/user/project/node_modules/shortid/index.d.ts"),
            "node_modules/shortid/index.d.ts"
        );
    }

    #[test]
    fn deep_project_scoped_package_keeps_full_subpath() {
        assert_eq!(
            trim_namespace_display_path("/home/user/project/node_modules/@types/react/index.d.ts"),
            "node_modules/@types/react/index.d.ts"
        );
    }

    #[test]
    fn virtual_root_prefix_path_kept() {
        // /p123/node_modules/csv-parse/lib/index.d.ts → "p123/node_modules/csv-parse/lib/index.d.ts"
        assert_eq!(
            trim_namespace_display_path("/p123/node_modules/csv-parse/lib/index.d.ts"),
            "p123/node_modules/csv-parse/lib/index.d.ts"
        );
    }

    #[test]
    fn no_node_modules_returns_trimmed() {
        assert_eq!(trim_namespace_display_path("/src/utils.ts"), "src/utils.ts");
        assert_eq!(
            trim_namespace_display_path("./src/utils.ts"),
            "src/utils.ts"
        );
        assert_eq!(trim_namespace_display_path("server.d.ts"), "server.d.ts");
    }

    #[test]
    fn relative_prefix_stripped() {
        assert_eq!(trim_namespace_display_path("./mod.d.ts"), "mod.d.ts");
    }
}
