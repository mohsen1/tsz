//! Variable declaration checking helpers: shadowing, TS2403 type computation,
//! and exported anonymous class private member checks.

use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
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
            if node_flags::is_let_or_const(parent_flags) {
                return;
            }
        } else {
            return;
        }

        // Collect all bound identifier names and their node indices from the
        // declaration name. For simple identifiers this is just the one name;
        // for destructuring patterns we walk the binding elements.
        let Some(name_node) = self.ctx.arena.get(var_decl.name) else {
            return;
        };
        let mut bound_names: Vec<(String, NodeIndex)> = Vec::new();
        if name_node.kind == SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                bound_names.push((ident.escaped_text.clone(), var_decl.name));
            }
        } else {
            // Destructuring pattern — collect all binding element names
            self.collect_binding_identifiers(var_decl.name, &mut bound_names);
        }

        for (var_name, name_node_idx) in &bound_names {
            let var_name: &str = var_name.as_str();

            // Note: We do NOT check the symbol's flags here. When `const x` and `var x`
            // appear in the same block, the binder may map the var node to the block-scoped
            // symbol (since they share a name in the block scope table). The syntactic check
            // above (parent VariableDeclarationList flags) is the reliable guard.

            // Walk the scope chain from the var's name position, looking for a block-scoped
            // symbol with the same name in an enclosing scope.
            let Some(start_scope_id) = self
                .ctx
                .binder
                .find_enclosing_scope(self.ctx.arena, *name_node_idx)
            else {
                continue;
            };

            let mut scope_id = start_scope_id;
            let mut found_block_scoped_symbol = None;
            let mut found_scope_kind = None;
            let mut found_scope_id = tsz_binder::ScopeId::NONE;
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
                    found_scope_id = scope_id;
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
                continue;
            };
            let Some(scope_kind) = found_scope_kind else {
                continue;
            };

            // Check if the found scope is at a function-level boundary.
            // If so, the var hoists to the same level and this is just a
            // TS2451 duplicate, not a TS2481 initialization conflict.
            let names_share_scope = if matches!(
                scope_kind,
                tsz_binder::ContainerKind::SourceFile
                    | tsz_binder::ContainerKind::Function
                    | tsz_binder::ContainerKind::Module
            ) {
                true
            } else if scope_kind == tsz_binder::ContainerKind::Block {
                // A function body creates a Block scope inside the Function scope.
                // When the let/const lives in that function-body Block, the Block's
                // AST container_node should be a direct child of a function-like node.
                // Check if this Block scope is a function body by examining the AST.

                self.ctx
                    .binder
                    .scopes
                    .get(found_scope_id.0 as usize)
                    .and_then(|s| {
                        // Get the AST node that created this scope (the Block node)
                        let block_node_idx = s.container_node;
                        // Get the Block's parent in the AST
                        self.ctx
                            .arena
                            .get_extended(block_node_idx)
                            .map(|ext| ext.parent)
                    })
                    .and_then(|parent_idx| self.ctx.arena.get(parent_idx))
                    .is_some_and(|parent_node| {
                        use tsz_parser::parser::syntax_kind_ext;
                        matches!(
                            parent_node.kind,
                            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                                || k == syntax_kind_ext::METHOD_DECLARATION
                                || k == syntax_kind_ext::CONSTRUCTOR
                                || k == syntax_kind_ext::GET_ACCESSOR
                                || k == syntax_kind_ext::SET_ACCESSOR
                                || k == syntax_kind_ext::ARROW_FUNCTION
                        )
                    })
            } else {
                false
            };

            if names_share_scope {
                // The var hoists to the same scope as the let/const.
                // tsc uses TS2300 ("Duplicate identifier") when the var declaration
                // appears before the block-scoped declaration, and TS2451 ("Cannot
                // redeclare block-scoped variable") when the block-scoped declaration
                // comes first.

                // When the var declaration is in the SAME syntactic scope as the
                // block-scoped declaration (e.g., `let x; var x;` at the same level),
                // check_duplicate_identifiers() already handles the conflict with TS2300.
                // Only emit here when the var is in a NESTED scope that hoists up
                // (e.g., `let x; { var x; }`).
                if start_scope_id == found_scope_id {
                    continue;
                }

                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

                // Check if any block-scoped declaration appears before this var.
                // For cross-file conflicts, skip: duplicate_identifiers handles them.
                let var_pos = self.ctx.arena.get(decl_idx).map_or(u32::MAX, |n| n.pos);
                let has_local_block_scoped =
                    self.ctx
                        .binder
                        .get_symbol(_block_sym_id)
                        .is_some_and(|block_sym| {
                            block_sym.declarations.iter().any(|&block_decl_idx| {
                                block_decl_idx.is_some()
                                    && self.ctx.arena.get(block_decl_idx).is_some()
                                    && block_decl_idx != decl_idx
                            })
                        });
                if !has_local_block_scoped {
                    // All block-scoped declarations are cross-file;
                    // duplicate_identifiers.rs handles this case.
                    continue;
                }
                let block_scoped_first =
                    self.ctx
                        .binder
                        .get_symbol(_block_sym_id)
                        .is_some_and(|block_sym| {
                            block_sym.declarations.iter().any(|&block_decl_idx| {
                                block_decl_idx.is_some()
                                    && self
                                        .ctx
                                        .arena
                                        .get(block_decl_idx)
                                        .is_some_and(|n| n.pos < var_pos)
                            })
                        });

                let (msg, code) = if block_scoped_first {
                    (
                        crate::diagnostics::format_message(
                            diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                            &[var_name],
                        ),
                        diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                    )
                } else {
                    (
                        crate::diagnostics::format_message(
                            diagnostic_messages::DUPLICATE_IDENTIFIER,
                            &[var_name],
                        ),
                        diagnostic_codes::DUPLICATE_IDENTIFIER,
                    )
                };

                // Error on the var declaration name
                self.error_at_node(*name_node_idx, &msg, code);
                // Error on the block-scoped declaration (let/const)
                if let Some(block_sym) = self.ctx.binder.get_symbol(_block_sym_id) {
                    for &block_decl_idx in &block_sym.declarations {
                        if !block_decl_idx.is_some() {
                            continue;
                        }
                        let decl_name_node = self
                            .get_declaration_name_node(block_decl_idx)
                            .unwrap_or(block_decl_idx);
                        self.error_at_node(decl_name_node, &msg, code);
                    }
                }
            } else {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node_msg(
                    *name_node_idx,
                    diagnostic_codes::CANNOT_INITIALIZE_OUTER_SCOPED_VARIABLE_IN_THE_SAME_SCOPE_AS_BLOCK_SCOPED_DECLAR,
                    &[var_name, var_name],
                );
            }
        }
    }

    /// Recursively collect all identifier names from a binding pattern (destructuring).
    /// For `{ x, y: z }` collects `x` and `z`. For `[a, b]` collects `a` and `b`.
    fn collect_binding_identifiers(
        &self,
        node_idx: NodeIndex,
        names: &mut Vec<(String, NodeIndex)>,
    ) {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(node_idx) else {
            return;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.ctx.arena.get_identifier(node) {
                names.push((ident.escaped_text.clone(), node_idx));
            }
            return;
        }

        if node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
            || node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
        {
            for child_idx in self.ctx.arena.get_children(node_idx) {
                self.collect_binding_identifiers(child_idx, names);
            }
            return;
        }

        if node.kind == syntax_kind_ext::BINDING_ELEMENT
            && let Some(binding) = self.ctx.arena.get_binding_element(node)
        {
            // The `name` field is the bound identifier or a nested pattern
            self.collect_binding_identifiers(binding.name, names);
        }
    }

    /// Check if a `var` declaration is in a TS2481 situation: it shares a block
    /// scope with a `let`/`const` declaration of the same name. When this is true,
    /// TS2481 applies and TS2451/TS2403 should be suppressed for this declaration.
    ///
    /// This handles the case where the binder merges `const x` and `var x` in the
    /// same block into a single symbol, which would otherwise incorrectly trigger
    /// TS2451 (from duplicate identifier checking) and TS2403 (from var redeclaration
    /// checking).
    pub(crate) fn is_var_shadowing_block_scoped_in_same_scope(&self, decl_idx: NodeIndex) -> bool {
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::node_flags;

        // Must be a var declaration (not let/const)
        let is_var = self
            .ctx
            .arena
            .get_extended(decl_idx)
            .and_then(|ext| self.ctx.arena.get(ext.parent))
            .is_some_and(|parent| {
                let flags = parent.flags as u32;
                parent.kind == tsz_parser::parser::syntax_kind_ext::VARIABLE_DECLARATION_LIST
                    && !node_flags::is_let_or_const(flags)
            });
        if !is_var {
            return false;
        }

        // Get the variable name
        let Some(var_decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(var_decl_node) else {
            return false;
        };
        let Some(name_node) = self.ctx.arena.get(var_decl.name) else {
            return false;
        };
        if name_node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        let var_name = ident.escaped_text.as_str();

        // Find the enclosing scope of the var declaration
        let Some(scope_id) = self
            .ctx
            .binder
            .find_enclosing_scope(self.ctx.arena, var_decl.name)
        else {
            return false;
        };
        let Some(scope) = self.ctx.binder.scopes.get(scope_id.0 as usize) else {
            return false;
        };

        // Check if this scope has a symbol with BLOCK_SCOPED_VARIABLE flag for this name
        if let Some(sym_id) = scope.table.get(var_name)
            && let Some(sym) = self.ctx.binder.get_symbol(sym_id)
            && (sym.flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0
        {
            // The scope has a block-scoped binding for this name.
            // Check that this scope is NOT at function/module/source-file level
            // (those cases are TS2451, not TS2481).
            if matches!(
                scope.kind,
                tsz_binder::ContainerKind::SourceFile
                    | tsz_binder::ContainerKind::Function
                    | tsz_binder::ContainerKind::Module
            ) {
                return false;
            }
            // Check if this Block scope is a function body
            if scope.kind == tsz_binder::ContainerKind::Block {
                let is_function_body = self
                    .ctx
                    .binder
                    .scopes
                    .get(scope_id.0 as usize)
                    .and_then(|s| {
                        self.ctx
                            .arena
                            .get_extended(s.container_node)
                            .map(|ext| ext.parent)
                    })
                    .and_then(|parent_idx| self.ctx.arena.get(parent_idx))
                    .is_some_and(|parent_node| {
                        use tsz_parser::parser::syntax_kind_ext;
                        matches!(
                            parent_node.kind,
                            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                                || k == syntax_kind_ext::METHOD_DECLARATION
                                || k == syntax_kind_ext::CONSTRUCTOR
                                || k == syntax_kind_ext::GET_ACCESSOR
                                || k == syntax_kind_ext::SET_ACCESSOR
                                || k == syntax_kind_ext::ARROW_FUNCTION
                        )
                    });
                if is_function_body {
                    return false;
                }
            }
            return true;
        }
        false
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

        // Handle bare enum identifier: `var x = E`
        if init_node.kind == SyntaxKind::Identifier as u16 {
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
            return fallback_type;
        }

        // Handle property access to enum in namespace: `var x = M.Color`
        if init_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(enum_obj) = self.resolve_property_access_enum_object(init_idx)
        {
            return enum_obj;
        }

        // For TS2403, when strictNullChecks is off, tsc still reports the raw
        // expression type (`undefined`) rather than the widened `any`. Recompute
        // the initializer type without widening for element access expressions
        // that may return `undefined` (e.g., out-of-bounds tuple access).
        if fallback_type == TypeId::ANY
            && init_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            let raw_type = self.get_type_of_node(init_idx);
            if raw_type == TypeId::UNDEFINED || raw_type == TypeId::NULL {
                return raw_type;
            }
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

        // Handle simple identifier: `typeof E`
        if expr_node.kind == SyntaxKind::Identifier as u16 {
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
            return fallback_type;
        }

        // Handle qualified name: `typeof M.Color`
        if expr_node.kind == syntax_kind_ext::QUALIFIED_NAME
            && let Some(enum_obj) = self.resolve_qualified_name_enum_object(expr_idx)
        {
            return enum_obj;
        }

        fallback_type
    }

    /// For TS2403: resolve a property access expression (like `m3.Color` or `M3.Color`)
    /// to an enum object type, if the property refers to an enum in a namespace.
    fn resolve_property_access_enum_object(&mut self, access_idx: NodeIndex) -> Option<TypeId> {
        let access_node = self.ctx.arena.get(access_idx)?;
        let access_data = self.ctx.arena.get_access_expr(access_node)?;

        // Get the property name
        let name_node = self.ctx.arena.get(access_data.name_or_argument)?;
        if name_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let name_ident = self.ctx.arena.get_identifier(name_node)?;
        let prop_name = name_ident.escaped_text.as_str();

        // Resolve the expression (left side) to a symbol
        let expr_sym_id = self.resolve_identifier_symbol(access_data.expression)?;
        let expr_symbol = self.ctx.binder.get_symbol(expr_sym_id)?;

        // Look for the property name in the symbol's exports (for namespaces/modules)
        let enum_sym_id = expr_symbol
            .exports
            .as_ref()
            .and_then(|exports| exports.get(prop_name))?;

        // Check if it's an enum (not an enum member)
        let enum_symbol = self.ctx.binder.get_symbol(enum_sym_id)?;
        if (enum_symbol.flags & tsz_binder::symbol_flags::ENUM) == 0
            || (enum_symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER) != 0
        {
            return None;
        }

        let enum_obj = self.enum_object_type(enum_sym_id)?;
        let def_id = self.ctx.get_or_create_def_id(enum_sym_id);
        self.ctx
            .definition_store
            .register_type_to_def(enum_obj, def_id);
        Some(enum_obj)
    }

    /// For TS2403: resolve a qualified name (like `M3.Color` in `typeof M3.Color`)
    /// to an enum object type, if the qualified name refers to an enum in a namespace.
    fn resolve_qualified_name_enum_object(&mut self, qname_idx: NodeIndex) -> Option<TypeId> {
        let qname_node = self.ctx.arena.get(qname_idx)?;
        let qname_data = self.ctx.arena.get_qualified_name(qname_node)?;

        // Get the right side (property name)
        let right_node = self.ctx.arena.get(qname_data.right)?;
        if right_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let right_ident = self.ctx.arena.get_identifier(right_node)?;
        let prop_name = right_ident.escaped_text.as_str();

        // Resolve the left side to a symbol
        let left_node = self.ctx.arena.get(qname_data.left)?;
        let left_sym_id = if left_node.kind == SyntaxKind::Identifier as u16 {
            self.resolve_identifier_symbol(qname_data.left)?
        } else {
            // Nested qualified names not handled for now
            return None;
        };

        let left_symbol = self.ctx.binder.get_symbol(left_sym_id)?;

        // Look for the property name in the symbol's exports
        let enum_sym_id = left_symbol
            .exports
            .as_ref()
            .and_then(|exports| exports.get(prop_name))?;

        // Check if it's an enum (not an enum member)
        let enum_symbol = self.ctx.binder.get_symbol(enum_sym_id)?;
        if (enum_symbol.flags & tsz_binder::symbol_flags::ENUM) == 0
            || (enum_symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER) != 0
        {
            return None;
        }

        let enum_obj = self.enum_object_type(enum_sym_id)?;
        let def_id = self.ctx.get_or_create_def_id(enum_sym_id);
        self.ctx
            .definition_store
            .register_type_to_def(enum_obj, def_id);
        Some(enum_obj)
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

    /// Get the enclosing `ModuleBlock` for a variable declaration, if any.
    ///
    /// Walks: `VariableDeclaration` → `VariableDeclarationList` → `VariableStatement` → `ModuleBlock`
    /// (accounting for an optional `ExportDeclaration` wrapper between `VariableStatement` and
    /// `ModuleBlock`).
    pub(crate) fn var_decl_enclosing_module_block(&self, decl_idx: NodeIndex) -> Option<NodeIndex> {
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

        if parent.kind == syntax_kind_ext::MODULE_BLOCK {
            return Some(parent_idx);
        }
        // Handle ExportDeclaration wrapper
        if parent.kind == syntax_kind_ext::EXPORT_DECLARATION {
            let gp_ext = self.ctx.arena.get_extended(parent_idx)?;
            let gp = self.ctx.arena.get(gp_ext.parent)?;
            if gp.kind == syntax_kind_ext::MODULE_BLOCK {
                return Some(gp_ext.parent);
            }
        }
        None
    }

    /// Check whether a declaration is inside a non-exported namespace at some
    /// level of its ancestor chain. This is used to distinguish declarations in
    /// separately-scoped namespace bodies from those in merged exported namespaces.
    ///
    /// Returns `true` if there is a `ModuleDeclaration` ancestor that is NOT
    /// exported (i.e., no `export` keyword), meaning the declaration is in a
    /// local (non-exported) namespace body.
    pub(crate) fn is_in_non_exported_namespace_body(&self, decl_idx: NodeIndex) -> bool {
        let mut current = decl_idx;
        for _ in 0..10 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent_idx = ext.parent;
            let Some(parent) = self.ctx.arena.get(parent_idx) else {
                return false;
            };

            if parent.kind == syntax_kind_ext::MODULE_BLOCK {
                // Found a module block. Check its parent (the ModuleDeclaration).
                let Some(mb_ext) = self.ctx.arena.get_extended(parent_idx) else {
                    return false;
                };
                let mod_decl_idx = mb_ext.parent;
                let Some(mod_decl) = self.ctx.arena.get(mod_decl_idx) else {
                    return false;
                };
                if mod_decl.kind == syntax_kind_ext::MODULE_DECLARATION {
                    // Check if this module declaration is exported.
                    // For `namespace X.Y.Z`, each dot-segment is a nested
                    // ModuleDeclaration, and the export keyword is on the
                    // outermost one. For `namespace X { namespace Y { } }`,
                    // Y's export status is determined by its own modifiers.
                    let is_exported = if let Some(mod_data) = self.ctx.arena.get_module(mod_decl) {
                        self.ctx.arena.has_modifier_ref(
                            mod_data.modifiers.as_ref(),
                            SyntaxKind::ExportKeyword,
                        )
                    } else {
                        false
                    };
                    if !is_exported {
                        // Check if this module is a nested part of a dot-notation
                        // declaration like `namespace X.Y.Z`. In that case, the
                        // inner Y/Z are implicitly exported.
                        let Some(mod_ext) = self.ctx.arena.get_extended(mod_decl_idx) else {
                            return false; // Can't determine, assume not non-exported
                        };
                        let Some(mod_parent) = self.ctx.arena.get(mod_ext.parent) else {
                            return false;
                        };
                        // If the parent of this ModuleDeclaration is another
                        // ModuleDeclaration (dot-notation nesting), the inner
                        // module is implicitly exported. Don't flag it.
                        if mod_parent.kind == syntax_kind_ext::MODULE_DECLARATION {
                            // It IS a dot-notation nested module. Continue walking up.
                        } else if mod_parent.kind == syntax_kind_ext::MODULE_BLOCK {
                            // This is a nested namespace inside another namespace
                            // body (e.g. `namespace X { namespace Z { } }`).
                            // It's non-exported and its members don't merge with
                            // exported members.
                            return true;
                        }
                        // If the parent is SourceFile or anything else, this is a
                        // top-level namespace. Top-level namespaces don't need
                        // `export` to participate in merging — they merge by name
                        // with other top-level namespaces.
                    }
                }
            }

            // Stop at source file
            if parent.kind == syntax_kind_ext::SOURCE_FILE {
                return false;
            }
            current = parent_idx;
        }
        false
    }

    /// Check whether two variable declarations are in different `ModuleBlock` nodes
    /// of the same merged namespace. When this is the case, TS2403 should not be
    /// emitted because TSC treats each namespace body as a separate declaration
    /// context for variable identity.
    pub(crate) fn are_decls_in_different_namespace_bodies(
        &self,
        decl_a: NodeIndex,
        decl_b: NodeIndex,
    ) -> bool {
        if let Some(mb_a) = self.var_decl_enclosing_module_block(decl_a)
            && let Some(mb_b) = self.var_decl_enclosing_module_block(decl_b)
        {
            return mb_a != mb_b;
        }
        false
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
        initializer: NodeIndex,
        inferred_type: TypeId,
    ) {
        if !self.ctx.emit_declarations() || self.ctx.is_declaration_file() || name.is_empty() {
            return;
        }

        if name == "globalThis"
            && initializer.is_some()
            && let Some(init_sym_id) = self.exported_variable_initializer_symbol(initializer)
            && self.symbol_initializer_references_builtin_global_this(
                init_sym_id,
                &mut FxHashSet::default(),
            )
        {
            self.error_at_node_msg(
                name_idx,
                crate::diagnostics::diagnostic_codes::EXPORTED_VARIABLE_HAS_OR_IS_USING_PRIVATE_NAME,
                &[name, "globalThis"],
            );
            return;
        }

        if initializer.is_some()
            && let Some(init_sym_id) = self.exported_variable_initializer_symbol(initializer)
            && self.symbol_references_inaccessible_unique_symbol_type(
                init_sym_id,
                &mut FxHashSet::default(),
            )
        {
            self.error_at_node_msg(
                name_idx,
                crate::diagnostics::diagnostic_codes::THE_INFERRED_TYPE_OF_REFERENCES_AN_INACCESSIBLE_TYPE_A_TYPE_ANNOTATION_IS_NECESS,
                &[name, "unique symbol"],
            );
            return;
        }

        if self
            .first_inaccessible_unique_symbol_reference_from_lazy_defs(inferred_type)
            .is_some()
        {
            self.error_at_node_msg(
                name_idx,
                crate::diagnostics::diagnostic_codes::THE_INFERRED_TYPE_OF_REFERENCES_AN_INACCESSIBLE_TYPE_A_TYPE_ANNOTATION_IS_NECESS,
                &[name, "unique symbol"],
            );
            return;
        }

        let resolved_inferred_type = self.resolve_lazy_type(inferred_type);
        if self
            .first_inaccessible_external_unique_symbol_reference(resolved_inferred_type)
            .is_some()
        {
            self.error_at_node_msg(
                name_idx,
                crate::diagnostics::diagnostic_codes::THE_INFERRED_TYPE_OF_REFERENCES_AN_INACCESSIBLE_TYPE_A_TYPE_ANNOTATION_IS_NECESS,
                &[name, "unique symbol"],
            );
            return;
        }

        // Check both the original (possibly Lazy) type and the resolved type.
        // A Lazy(DefId) at the top level gets resolved away, losing the DefId
        // that links back to the non-portable symbol. By checking the original
        // type first, we catch cases like `export const x = getSpecial()` where
        // the return type is a single non-portable interface reference.
        if let Some((from_path, type_name)) = self
            .first_non_portable_type_reference(inferred_type)
            .or_else(|| self.first_non_portable_type_reference(resolved_inferred_type))
        {
            self.error_at_node_msg(
                name_idx,
                crate::diagnostics::diagnostic_codes::THE_INFERRED_TYPE_OF_CANNOT_BE_NAMED_WITHOUT_A_REFERENCE_TO_FROM_THIS_IS_LIKELY,
                &[name, &type_name, &from_path],
            );
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

    pub(crate) fn maybe_report_private_name_in_exported_variable_type_annotation(
        &mut self,
        _name_idx: NodeIndex,
        name: &str,
        type_annotation: NodeIndex,
    ) {
        if !self.ctx.emit_declarations() || self.ctx.is_declaration_file() || name.is_empty() {
            return;
        }

        let Some((report_at, private_name)) =
            self.first_private_value_type_query_name_in_exported_type_annotation(type_annotation)
        else {
            return;
        };

        self.error_at_node_msg(
            report_at,
            crate::diagnostics::diagnostic_codes::EXPORTED_VARIABLE_HAS_OR_IS_USING_PRIVATE_NAME,
            &[name, &private_name],
        );
    }
}
