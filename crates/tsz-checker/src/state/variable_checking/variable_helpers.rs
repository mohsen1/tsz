//! Variable declaration checking helpers: shadowing, TS2403 type computation,
//! unnameable type detection, and symbol resolution utilities.

use crate::query_boundaries::state::checking as query;
use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn is_within_non_ambient_class_body(&self, mut idx: NodeIndex) -> bool {
        let mut guard = 0u32;
        while idx.is_some() {
            guard += 1;
            if guard > 4096 {
                return false;
            }
            let Some(node) = self.ctx.arena.get(idx) else {
                return false;
            };
            if node.kind == syntax_kind_ext::CLASS_EXPRESSION {
                return true;
            }
            if node.kind == syntax_kind_ext::CLASS_DECLARATION {
                if let Some(class_data) = self.ctx.arena.get_class(node) {
                    return !self.has_declare_modifier(&class_data.modifiers);
                }
                return true;
            }
            let Some(ext) = self.ctx.arena.get_extended(idx) else {
                return false;
            };
            if ext.parent.is_none() {
                return false;
            }
            idx = ext.parent;
        }
        false
    }

    /// TS2481: Check if a `var` declaration shadows a block-scoped declaration (`let`/`const`)
    /// in an enclosing scope that is NOT at function/module/source-file level.
    pub(crate) fn check_var_declared_names_not_shadowed(
        &mut self,
        decl_idx: NodeIndex,
        var_decl: &tsz_parser::parser::node::VariableDeclarationData,
    ) {
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::node_flags;

        // Skip block-scoped variables (let/const) and parameters — only var triggers TS2481
        if let Some(ext) = self.ctx.arena.get_extended(decl_idx)
            && let Some(parent_node) = self.ctx.arena.get(ext.parent)
        {
            let parent_flags = parent_node.flags as u32;
            if parent_flags & (node_flags::LET | node_flags::CONST) != 0 {
                return;
            }
        } else {
            return;
        }

        // Only applies to identifier names (not destructuring patterns)
        let Some(name_node) = self.ctx.arena.get(var_decl.name) else {
            return;
        };
        if name_node.kind != SyntaxKind::Identifier as u16 {
            return;
        }
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return;
        };
        let var_name = ident.escaped_text.as_str();

        // Get the symbol for this var declaration itself
        let Some(decl_symbol_id) = self.ctx.binder.get_node_symbol(decl_idx) else {
            return;
        };
        let Some(decl_symbol) = self.ctx.binder.get_symbol(decl_symbol_id) else {
            return;
        };

        // Only check function-scoped variables (var)
        if decl_symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE == 0 {
            return;
        }

        // Walk the scope chain from the var's name position, looking for a block-scoped
        // symbol with the same name in an enclosing scope.
        let Some(start_scope_id) = self
            .ctx
            .binder
            .find_enclosing_scope(self.ctx.arena, var_decl.name)
        else {
            return;
        };

        let mut scope_id = start_scope_id;
        let mut found_block_scoped_symbol = None;
        let mut found_scope_kind = None;
        let mut depth = 0;
        while scope_id.is_some() && depth < 50 {
            let Some(scope) = self.ctx.binder.scopes.get(scope_id.0 as usize) else {
                break;
            };
            if let Some(sym_id) = scope.table.get(var_name)
                && let Some(sym) = self.ctx.binder.get_symbol(sym_id)
                && sym.flags & symbol_flags::BLOCK_SCOPED_VARIABLE != 0
            {
                found_block_scoped_symbol = Some(sym_id);
                found_scope_kind = Some(scope.kind);
                break;
            }
            // If we hit a function scope, var hoists to this level — stop searching
            if scope.is_function_scope() {
                break;
            }
            scope_id = scope.parent;
            depth += 1;
        }

        let Some(_block_sym_id) = found_block_scoped_symbol else {
            return;
        };
        let Some(scope_kind) = found_scope_kind else {
            return;
        };

        let names_share_scope = matches!(
            scope_kind,
            tsz_binder::ContainerKind::SourceFile
                | tsz_binder::ContainerKind::Function
                | tsz_binder::ContainerKind::Module
        );

        if !names_share_scope {
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node_msg(
                var_decl.name,
                diagnostic_codes::CANNOT_INITIALIZE_OUTER_SCOPED_VARIABLE_IN_THE_SAME_SCOPE_AS_BLOCK_SCOPED_DECLAR,
                &[var_name, var_name],
            );
        }
    }

    /// For TS2403 redeclaration checking, compute the "declared type" of an
    /// initializer expression.
    pub(crate) fn initializer_ts2403_type(
        &mut self,
        init_idx: NodeIndex,
        fallback_type: TypeId,
    ) -> TypeId {
        let Some(init_node) = self.ctx.arena.get(init_idx) else {
            return fallback_type;
        };

        if matches!(
            init_node.kind,
            syntax_kind_ext::CALL_EXPRESSION
                | syntax_kind_ext::NEW_EXPRESSION
                | syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
        ) {
            return self.widen_initializer_type_for_mutable_binding(fallback_type);
        }

        if init_node.kind != SyntaxKind::Identifier as u16 {
            return fallback_type;
        }

        if let Some(init_sym_id) = self.resolve_identifier_symbol(init_idx)
            && let Some(symbol) = self.ctx.binder.get_symbol(init_sym_id)
            && (symbol.flags & tsz_binder::symbol_flags::ENUM) != 0
            && (symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER) == 0
            && let Some(enum_obj) = self.enum_object_type(init_sym_id)
        {
            let def_id = self.ctx.get_or_create_def_id(init_sym_id);
            self.ctx
                .definition_store
                .register_type_to_def(enum_obj, def_id);
            return enum_obj;
        }

        fallback_type
    }

    /// For TS2403, when the type annotation is `typeof EnumSymbol`, resolve
    /// to the enum object type.
    pub(crate) fn annotation_ts2403_type(
        &mut self,
        annotation_idx: NodeIndex,
        fallback_type: TypeId,
    ) -> TypeId {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(ann_node) = self.ctx.arena.get(annotation_idx) else {
            return fallback_type;
        };

        if ann_node.kind != syntax_kind_ext::TYPE_QUERY {
            return fallback_type;
        }

        let Some(type_query) = self.ctx.arena.get_type_query(ann_node) else {
            return fallback_type;
        };

        let expr_idx = type_query.expr_name;
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return fallback_type;
        };

        if expr_node.kind != SyntaxKind::Identifier as u16 {
            return fallback_type;
        }

        if let Some(sym_id) = self.resolve_identifier_symbol(expr_idx)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && (symbol.flags & tsz_binder::symbol_flags::ENUM) != 0
            && (symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER) == 0
            && let Some(enum_obj) = self.enum_object_type(sym_id)
        {
            let def_id = self.ctx.get_or_create_def_id(sym_id);
            self.ctx
                .definition_store
                .register_type_to_def(enum_obj, def_id);
            return enum_obj;
        }

        fallback_type
    }

    pub(crate) fn is_bare_var_declaration_node(&self, decl_idx: NodeIndex) -> bool {
        self.ctx
            .arena
            .get(decl_idx)
            .and_then(|node| self.ctx.arena.get_variable_declaration(node))
            .is_some_and(|decl| decl.type_annotation.is_none() && decl.initializer.is_none())
    }

    /// Check if a variable declaration is inside a namespace body and whether
    /// it has an `export` modifier.
    /// Check if a variable declaration is inside a for-in or for-of statement.
    /// E.g., `for (var i in obj)` — the `i` declaration is inside a for-in.
    pub(crate) fn is_var_decl_in_for_in_or_for_of(&self, decl_idx: NodeIndex) -> bool {
        let Some(ext) = self.ctx.arena.get_extended(decl_idx) else {
            return false;
        };
        let decl_list_idx = ext.parent;
        let Some(decl_list_ext) = self.ctx.arena.get_extended(decl_list_idx) else {
            return false;
        };
        let parent_idx = decl_list_ext.parent;
        let Some(parent) = self.ctx.arena.get(parent_idx) else {
            return false;
        };
        use tsz_parser::parser::syntax_kind_ext;
        matches!(
            parent.kind,
            syntax_kind_ext::FOR_IN_STATEMENT | syntax_kind_ext::FOR_OF_STATEMENT
        )
    }

    pub(crate) fn var_decl_namespace_export_status(&self, decl_idx: NodeIndex) -> Option<bool> {
        let ext = self.ctx.arena.get_extended(decl_idx)?;
        let decl_list_idx = ext.parent;
        let decl_list_ext = self.ctx.arena.get_extended(decl_list_idx)?;
        let var_stmt_idx = decl_list_ext.parent;
        let var_stmt = self.ctx.arena.get(var_stmt_idx)?;
        if var_stmt.kind != syntax_kind_ext::VARIABLE_STATEMENT {
            return None;
        }

        let var_stmt_ext = self.ctx.arena.get_extended(var_stmt_idx)?;
        let parent_idx = var_stmt_ext.parent;
        let parent = self.ctx.arena.get(parent_idx)?;

        let is_export_wrapper = parent.kind == syntax_kind_ext::EXPORT_DECLARATION;
        let in_module_block = if parent.kind == syntax_kind_ext::MODULE_BLOCK {
            true
        } else if is_export_wrapper {
            self.ctx
                .arena
                .get_extended(parent_idx)
                .and_then(|ext| self.ctx.arena.get(ext.parent))
                .is_some_and(|gp| gp.kind == syntax_kind_ext::MODULE_BLOCK)
        } else {
            false
        };

        if !in_module_block {
            return None;
        }

        let has_export = if let Some(var_data) = self.ctx.arena.get_variable(var_stmt) {
            self.ctx
                .arena
                .has_modifier_ref(var_data.modifiers.as_ref(), SyntaxKind::ExportKeyword)
        } else {
            false
        };

        Some(has_export || is_export_wrapper)
    }

    /// Check if a `TypeQuery` type transitively leads back to the target symbol
    /// through a chain of typeof references in variable declarations.
    pub(crate) fn check_transitive_type_query_circularity(
        &self,
        type_id: TypeId,
        target_sym: SymbolId,
    ) -> bool {
        use crate::query_boundaries::type_checking_utilities::{
            TypeQueryKind, classify_type_query,
        };

        let mut current = type_id;
        let mut visited = Vec::<u32>::new();

        for _ in 0..8 {
            let sym_id = match classify_type_query(self.ctx.types, current) {
                TypeQueryKind::TypeQuery(sym_ref) => sym_ref.0,
                TypeQueryKind::ApplicationWithTypeQuery { base_sym_ref, .. } => base_sym_ref.0,
                _ => return false,
            };

            if visited.contains(&sym_id) {
                return false;
            }
            visited.push(sym_id);

            let sym_id_binder = SymbolId(sym_id);
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id_binder) else {
                return false;
            };

            for &decl_idx in &symbol.declarations {
                if !decl_idx.is_some() {
                    continue;
                }
                let Some(node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) else {
                    continue;
                };
                if var_decl.type_annotation.is_none() {
                    continue;
                }
                if self
                    .find_circular_reference_in_type_node(
                        var_decl.type_annotation,
                        target_sym,
                        false,
                    )
                    .is_some()
                {
                    return true;
                }
                let Some(ann_node) = self.ctx.arena.get(var_decl.type_annotation) else {
                    continue;
                };
                if ann_node.kind != syntax_kind_ext::TYPE_QUERY {
                    continue;
                }
                let Some(query_data) = self.ctx.arena.get_type_query(ann_node) else {
                    continue;
                };
                let Some(expr_node) = self.ctx.arena.get(query_data.expr_name) else {
                    continue;
                };
                if expr_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
                    continue;
                }
                if let Some(next_sym) = self
                    .ctx
                    .binder
                    .get_node_symbol(query_data.expr_name)
                    .or_else(|| {
                        self.ctx
                            .binder
                            .resolve_identifier(self.ctx.arena, query_data.expr_name)
                    })
                {
                    let factory = self.ctx.types.factory();
                    current = factory.type_query(tsz_solver::SymbolRef(next_sym.0));
                    break;
                }
            }
        }
        false
    }

    /// TS4094: Property '{0}' of exported anonymous class type may not be private or protected.
    ///
    /// When `declaration: true` and a variable is exported with a class expression
    /// initializer, private/protected members of the anonymous class cannot be
    /// represented in a .d.ts file.
    pub(crate) fn maybe_report_exported_anonymous_class_private_members(
        &mut self,
        name_idx: NodeIndex,
        initializer: NodeIndex,
    ) {
        if !self.ctx.emit_declarations() || self.ctx.is_declaration_file() {
            return;
        }
        let Some(init_node) = self.ctx.arena.get(initializer) else {
            return;
        };
        if init_node.kind != syntax_kind_ext::CLASS_EXPRESSION {
            return;
        }
        let Some(class) = self.ctx.arena.get_class(init_node) else {
            return;
        };
        // Anonymous class: name is absent
        if class.name.is_some()
            && let Some(name_node) = self.ctx.arena.get(class.name)
            && name_node.kind == SyntaxKind::Identifier as u16
        {
            return;
        }
        self.report_anonymous_class_private_members(name_idx, &class.members);
    }

    /// Emit TS4094 for each private/protected member in a class member list.
    pub(crate) fn report_anonymous_class_private_members(
        &mut self,
        report_at: NodeIndex,
        members: &tsz_parser::parser::NodeList,
    ) {
        use crate::diagnostics::diagnostic_codes;

        for &member_idx in &members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            match member_node.kind {
                syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    if self.has_private_modifier(&prop.modifiers)
                        || self.has_protected_modifier(&prop.modifiers)
                        || self.is_private_identifier_name(prop.name)
                    {
                        let name = self.get_member_name_text(prop.name).unwrap_or_default();
                        self.error_at_node_msg(
                            report_at,
                            diagnostic_codes::PROPERTY_OF_EXPORTED_ANONYMOUS_CLASS_TYPE_MAY_NOT_BE_PRIVATE_OR_PROTECTED,
                            &[&name],
                        );
                    }
                }
                syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    if self.has_private_modifier(&method.modifiers)
                        || self.has_protected_modifier(&method.modifiers)
                        || self.is_private_identifier_name(method.name)
                    {
                        let name = self.get_member_name_text(method.name).unwrap_or_default();
                        self.error_at_node_msg(
                            report_at,
                            diagnostic_codes::PROPERTY_OF_EXPORTED_ANONYMOUS_CLASS_TYPE_MAY_NOT_BE_PRIVATE_OR_PROTECTED,
                            &[&name],
                        );
                    }
                }
                _ => {}
            }
        }
    }

    pub(crate) fn maybe_report_unnameable_exported_variable_type(
        &mut self,
        name_idx: NodeIndex,
        name: &str,
        inferred_type: TypeId,
    ) {
        if !self.ctx.emit_declarations() || self.ctx.is_declaration_file() || name.is_empty() {
            return;
        }

        let Some((referenced_name, module_specifier)) =
            self.first_unnameable_external_unique_symbol_reference(inferred_type)
        else {
            return;
        };

        let quoted_module = format!("\"{module_specifier}\"");
        self.error_at_node_msg(
            name_idx,
            crate::diagnostics::diagnostic_codes::EXPORTED_VARIABLE_HAS_OR_IS_USING_NAME_FROM_EXTERNAL_MODULE_BUT_CANNOT_BE_NAMED,
            &[name, &referenced_name, &quoted_module],
        );
    }

    fn first_unnameable_external_unique_symbol_reference(
        &self,
        inferred_type: TypeId,
    ) -> Option<(String, String)> {
        let mut result = None;

        tsz_solver::visitor::walk_referenced_types(self.ctx.types, inferred_type, |type_id| {
            if result.is_some() {
                return;
            }

            if let Some(shape) = query::object_shape(self.ctx.types, type_id)
                && let Some(info) = self.inspect_unique_symbol_properties(&shape.properties)
            {
                result = Some(info);
                return;
            }
            if let Some(shape) = query::callable_shape(self.ctx.types, type_id)
                && let Some(info) = self.inspect_unique_symbol_properties(&shape.properties)
            {
                result = Some(info);
            }
        });

        result
    }

    fn inspect_unique_symbol_properties(
        &self,
        properties: &[tsz_solver::PropertyInfo],
    ) -> Option<(String, String)> {
        for prop in properties {
            let prop_name = self.ctx.types.resolve_atom(prop.name);
            let Some(symbol_id) = prop_name.strip_prefix("__unique_") else {
                continue;
            };
            let Ok(symbol_raw) = symbol_id.parse::<u32>() else {
                continue;
            };
            if let Some(info) = self.unique_symbol_emit_nameability_info(SymbolId(symbol_raw)) {
                return Some(info);
            }
        }
        None
    }

    fn unique_symbol_emit_nameability_info(&self, sym_id: SymbolId) -> Option<(String, String)> {
        let (reported_name, root_sym_id, file_idx) = self.unique_symbol_report_target(sym_id)?;
        if file_idx == u32::MAX || file_idx == self.ctx.current_file_idx as u32 {
            return None;
        }

        if !self
            .ctx
            .get_binder_for_file(file_idx as usize)
            .is_some_and(tsz_binder::BinderState::is_external_module)
        {
            return None;
        }

        if self.local_value_name_resolves_to(root_sym_id) {
            return None;
        }

        let module_specifier = self.module_specifier_for_file(file_idx)?;
        Some((reported_name, module_specifier))
    }

    fn unique_symbol_report_target(&self, sym_id: SymbolId) -> Option<(String, SymbolId, u32)> {
        let symbol = self.get_symbol_from_any_binder(sym_id)?;
        let file_idx = symbol.decl_file_idx;
        let owner_binder = self
            .ctx
            .get_binder_for_file(file_idx as usize)
            .unwrap_or(self.ctx.binder);

        let mut decl_candidates = symbol.declarations.clone();
        if symbol.value_declaration.is_some()
            && !decl_candidates.contains(&symbol.value_declaration)
        {
            decl_candidates.push(symbol.value_declaration);
        }

        for decl_idx in decl_candidates {
            if !decl_idx.is_some() {
                continue;
            }

            let mut candidate_arenas: Vec<&tsz_parser::parser::node::NodeArena> = Vec::new();
            if let Some(arenas) = owner_binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                candidate_arenas.extend(arenas.iter().map(std::convert::AsRef::as_ref));
            }
            if let Some(symbol_arena) = owner_binder.symbol_arenas.get(&sym_id) {
                candidate_arenas.push(symbol_arena.as_ref());
            }
            if std::ptr::eq(owner_binder, self.ctx.binder) {
                candidate_arenas.push(self.ctx.arena);
            }

            for arena in candidate_arenas {
                let Some(node) = arena.get(decl_idx) else {
                    continue;
                };
                if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                    continue;
                }

                let mut namespace_names = Vec::new();
                let mut namespace_nodes = Vec::new();
                let mut parent = arena
                    .get_extended(decl_idx)
                    .map_or(NodeIndex::NONE, |info| info.parent);
                while !parent.is_none() {
                    let Some(parent_node) = arena.get(parent) else {
                        break;
                    };
                    if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                        && let Some(module) = arena.get_module(parent_node)
                        && let Some(name_node) = arena.get(module.name)
                        && name_node.kind == SyntaxKind::Identifier as u16
                        && let Some(name_ident) = arena.get_identifier(name_node)
                    {
                        namespace_names.push(name_ident.escaped_text.clone());
                        namespace_nodes.push(parent);
                    }
                    parent = arena
                        .get_extended(parent)
                        .map_or(NodeIndex::NONE, |info| info.parent);
                }

                if !namespace_names.is_empty() {
                    namespace_names.reverse();
                    let display_name = namespace_names.join(".");
                    let root_namespace_idx = *namespace_nodes.last().unwrap_or(&NodeIndex::NONE);
                    let root_sym_id = self
                        .ctx
                        .get_binder_for_arena(arena)
                        .and_then(|binder| binder.get_node_symbol(root_namespace_idx))
                        .unwrap_or(sym_id);
                    return Some((display_name, root_sym_id, file_idx));
                }

                return Some((symbol.escaped_name.clone(), sym_id, file_idx));
            }
        }

        Some((symbol.escaped_name.clone(), sym_id, file_idx))
    }

    pub(crate) fn get_symbol_from_any_binder(
        &self,
        sym_id: SymbolId,
    ) -> Option<&tsz_binder::Symbol> {
        self.ctx
            .binder
            .get_symbol(sym_id)
            .or_else(|| {
                self.ctx
                    .all_binders
                    .as_ref()
                    .and_then(|binders| binders.iter().find_map(|binder| binder.get_symbol(sym_id)))
            })
            .or_else(|| {
                self.ctx
                    .lib_contexts
                    .iter()
                    .find_map(|ctx| ctx.binder.get_symbol(sym_id))
            })
    }

    pub(crate) fn local_value_name_resolves_to(&self, target_sym_id: SymbolId) -> bool {
        self.ctx
            .binder
            .file_locals
            .iter()
            .any(|(_, &local_sym_id)| {
                let Some(symbol) = self.ctx.binder.get_symbol(local_sym_id) else {
                    return false;
                };
                if symbol.is_type_only {
                    return false;
                }
                if local_sym_id == target_sym_id {
                    return true;
                }

                self.ctx.binder.resolve_import_symbol(local_sym_id) == Some(target_sym_id)
            })
    }

    pub(crate) fn module_specifier_for_file(&self, file_idx: u32) -> Option<String> {
        if let Some(specifier) = self.ctx.module_specifiers.get(&file_idx) {
            return Some(specifier.clone());
        }

        let arena = self.ctx.get_arena_for_file(file_idx);
        let source_file = arena.source_files.first()?;
        let file_name = &source_file.file_name;
        let stem = file_name
            .rsplit_once('.')
            .map(|(base, _)| base)
            .unwrap_or(file_name);
        let basename = stem.rsplit_once('/').map(|(_, name)| name).unwrap_or(stem);
        Some(basename.to_string())
    }
}
