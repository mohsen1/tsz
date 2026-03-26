//! Variable declaration checking helpers: shadowing, TS2403 type computation,
//! unnameable type detection, and symbol resolution utilities.

use rustc_hash::FxHashSet;
use crate::query_boundaries::common::{collect_referenced_types, lazy_def_id};
use crate::query_boundaries::state::checking as query;
use crate::state::CheckerState;
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

        // Note: We do NOT check the symbol's flags here. When `const x` and `var x`
        // appear in the same block, the binder may map the var node to the block-scoped
        // symbol (since they share a name in the block scope table). The syntactic check
        // above (parent VariableDeclarationList flags) is the reliable guard.

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
            return;
        };
        let Some(scope_kind) = found_scope_kind else {
            return;
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
                return;
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
            self.error_at_node(var_decl.name, &msg, code);
            // Error on the block-scoped declaration (let/const)
            if let Some(block_sym) = self.ctx.binder.get_symbol(_block_sym_id) {
                for &block_decl_idx in &block_sym.declarations {
                    if !block_decl_idx.is_some() {
                        continue;
                    }
                    let name_node = self
                        .get_declaration_name_node(block_decl_idx)
                        .unwrap_or(block_decl_idx);
                    self.error_at_node(name_node, &msg, code);
                }
            }
        } else {
            use crate::diagnostics::diagnostic_codes;
            self.error_at_node_msg(
                var_decl.name,
                diagnostic_codes::CANNOT_INITIALIZE_OUTER_SCOPED_VARIABLE_IN_THE_SAME_SCOPE_AS_BLOCK_SCOPED_DECLAR,
                &[var_name, var_name],
            );
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
                    && (flags & (node_flags::LET | node_flags::CONST)) == 0
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

        if let Some((from_path, type_name)) = self.first_non_portable_type_reference(inferred_type)
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

    fn first_private_value_type_query_name_in_exported_type_annotation(
        &self,
        type_annotation: NodeIndex,
    ) -> Option<(NodeIndex, String)> {
        let mut stack = vec![type_annotation];

        while let Some(node_idx) = stack.pop() {
            let Some(node) = self.ctx.arena.get(node_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::TYPE_QUERY
                && let Some(type_query) = self.ctx.arena.get_type_query(node)
                && let Some(root_name) = self.type_query_root_identifier_name(type_query.expr_name)
                && !root_name.is_empty()
            {
                if let Some(sym_id) =
                    self.resolve_type_query_value_symbol_for_emit(type_query.expr_name)
                    && self.value_symbol_is_private_for_exported_type_query(sym_id)
                {
                    return Some((type_query.expr_name, root_name));
                }

                if !self.type_query_value_name_is_accessible(type_query.expr_name)
                    && self.has_inaccessible_current_file_value_name(&root_name)
                {
                    return Some((type_query.expr_name, root_name));
                }
            }

            stack.extend(self.ctx.arena.get_children(node_idx));
        }

        None
    }

    fn type_query_root_identifier_name(&self, expr_name: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(expr_name)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.to_string());
        }

        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qualified = self.ctx.arena.get_qualified_name(node)?;
            return self.type_query_root_identifier_name(qualified.left);
        }

        None
    }

    fn type_query_value_name_is_accessible(&self, expr_name: NodeIndex) -> bool {
        self.resolve_type_query_value_symbol_for_emit(expr_name)
            .is_some()
    }

    fn resolve_type_query_value_symbol_for_emit(&self, expr_name: NodeIndex) -> Option<SymbolId> {
        let node = self.ctx.arena.get(expr_name)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            return self.resolve_identifier_symbol_without_tracking(expr_name);
        }

        if node.kind != syntax_kind_ext::QUALIFIED_NAME {
            return None;
        }

        let qualified = self.ctx.arena.get_qualified_name(node)?;
        let left_sym_id = self.resolve_type_query_value_symbol_for_emit(qualified.left)?;
        let right_name = self.ctx.arena.get_identifier_text(qualified.right)?;
        let left_symbol = self.get_symbol_from_any_binder(left_sym_id)?;

        left_symbol.exports.as_ref().and_then(|exports| {
            exports.iter().find_map(|(name, sym_id)| {
                if name == right_name {
                    Some(*sym_id)
                } else {
                    None
                }
            })
        })
    }

    fn value_symbol_is_private_for_exported_type_query(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.get_symbol_from_any_binder(sym_id) else {
            return false;
        };

        let mut decls = symbol.declarations.clone();
        if symbol.value_declaration.is_some() && !decls.contains(&symbol.value_declaration) {
            decls.push(symbol.value_declaration);
        }

        decls
            .into_iter()
            .any(|decl_idx| self.declaration_is_hidden_from_declaration_emit(decl_idx))
    }

    fn declaration_is_hidden_from_declaration_emit(&self, decl_idx: NodeIndex) -> bool {
        let mut current = decl_idx;

        while current.is_some() {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent_idx = ext.parent;
            let Some(parent) = self.ctx.arena.get(parent_idx) else {
                return false;
            };

            if parent.kind == syntax_kind_ext::SOURCE_FILE
                || parent.kind == syntax_kind_ext::MODULE_BLOCK
            {
                return false;
            }

            if parent.kind == syntax_kind_ext::BLOCK {
                let Some(block_ext) = self.ctx.arena.get_extended(parent_idx) else {
                    return true;
                };
                let Some(block_parent) = self.ctx.arena.get(block_ext.parent) else {
                    return true;
                };

                return !matches!(
                    block_parent.kind,
                    syntax_kind_ext::FUNCTION_DECLARATION
                        | syntax_kind_ext::FUNCTION_EXPRESSION
                        | syntax_kind_ext::ARROW_FUNCTION
                        | syntax_kind_ext::METHOD_DECLARATION
                        | syntax_kind_ext::CONSTRUCTOR
                        | syntax_kind_ext::GET_ACCESSOR
                        | syntax_kind_ext::SET_ACCESSOR
                        | syntax_kind_ext::MODULE_DECLARATION
                        | syntax_kind_ext::MODULE_BLOCK
                );
            }

            current = parent_idx;
        }

        false
    }

    fn has_inaccessible_current_file_value_name(&self, name: &str) -> bool {
        if let Some(local_sym_id) = self.ctx.binder.file_locals.get(name) {
            let is_accessible_value =
                self.ctx
                    .binder
                    .get_symbol(local_sym_id)
                    .is_some_and(|symbol| {
                        !symbol.is_type_only && self.local_value_name_resolves_to(local_sym_id)
                    });
            if is_accessible_value {
                return false;
            }
        }

        self.ctx.binder.symbols.iter().any(|symbol| {
            !symbol.is_type_only
                && symbol.escaped_name == name
                && (symbol.decl_file_idx == u32::MAX
                    || symbol.decl_file_idx == self.ctx.current_file_idx as u32)
        })
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

    fn first_inaccessible_external_unique_symbol_reference(
        &self,
        inferred_type: TypeId,
    ) -> Option<SymbolId> {
        let mut result = None;

        tsz_solver::visitor::walk_referenced_types(self.ctx.types, inferred_type, |type_id| {
            if result.is_some() {
                return;
            }

            let Some(sym_ref) = tsz_solver::visitor::unique_symbol_ref(self.ctx.types, type_id)
            else {
                return;
            };

            let sym_id = SymbolId(sym_ref.0);
            if self.unique_symbol_type_is_inaccessible(sym_id) {
                result = Some(sym_id);
            }
        });

        result
    }

    fn first_inaccessible_unique_symbol_reference_from_lazy_defs(
        &self,
        inferred_type: TypeId,
    ) -> Option<SymbolId> {
        let referenced_types = collect_referenced_types(self.ctx.types, inferred_type);

        for &type_id in &referenced_types {
            let Some(def_id) = lazy_def_id(self.ctx.types, type_id) else {
                continue;
            };
            let Some(sym_id) = self.ctx.def_to_symbol_id_with_fallback(def_id) else {
                continue;
            };

            let mut visited = FxHashSet::default();
            if self.symbol_references_inaccessible_unique_symbol_type(sym_id, &mut visited) {
                return Some(sym_id);
            }
        }

        None
    }

    fn first_non_portable_type_reference(&self, inferred_type: TypeId) -> Option<(String, String)> {
        let referenced_types = collect_referenced_types(self.ctx.types, inferred_type);

        for &type_id in &referenced_types {
            if let Some(def_id) = lazy_def_id(self.ctx.types, type_id)
                && let Some(sym_id) = self.ctx.def_to_symbol_id_with_fallback(def_id)
                && let Some(info) = self.find_non_portable_symbol_reference(sym_id)
            {
                return Some(info);
            }

            if let Some(shape) = query::object_shape(self.ctx.types, type_id)
                && let Some(sym_id) = shape.symbol
                && let Some(info) = self.find_non_portable_symbol_reference(sym_id)
            {
                return Some(info);
            }

            if let Some(shape) = query::callable_shape(self.ctx.types, type_id)
                && let Some(sym_id) = shape.symbol
                && let Some(info) = self.find_non_portable_symbol_reference(sym_id)
            {
                return Some(info);
            }
        }

        None
    }

    fn find_non_portable_symbol_reference(&self, sym_id: SymbolId) -> Option<(String, String)> {
        use std::path::{Component, Path};
        use tsz_binder::symbol_flags;

        let resolved_sym_id = self
            .resolve_alias_symbol(sym_id, &mut Vec::new())
            .unwrap_or(sym_id);

        let symbol = self.get_symbol_from_any_binder(resolved_sym_id)?;
        let type_name = symbol.escaped_name.clone();
        let source_path = self.symbol_source_path(resolved_sym_id)?;

        let components: Vec<_> = Path::new(&source_path).components().collect();
        let nm_positions: Vec<usize> = components
            .iter()
            .enumerate()
            .filter_map(|(i, c)| match c {
                Component::Normal(part) if part.to_str() == Some("node_modules") => Some(i),
                _ => None,
            })
            .collect();

        if !nm_positions.is_empty()
            && symbol.has_any_flags(symbol_flags::ALIAS)
            && let Some(import_module) = &symbol.import_module
            && !import_module.starts_with('.')
            && !import_module.starts_with('/')
        {
            let last_nm = *nm_positions.last().unwrap();
            let pkg_start = last_nm + 1;
            let pkg_len = if components.get(pkg_start).is_some_and(|c| {
                matches!(c, Component::Normal(p) if p.to_str().is_some_and(|s| s.starts_with('@')))
            }) {
                2
            } else {
                1
            };

            let parent_package: Vec<String> = components[pkg_start..pkg_start + pkg_len]
                .iter()
                .filter_map(|c| match c {
                    Component::Normal(part) => part.to_str().map(str::to_string),
                    _ => None,
                })
                .collect();

            if !parent_package.is_empty() {
                let from_path = format!(
                    "{}/node_modules/{}",
                    parent_package.join("/"),
                    import_module
                );
                return Some((from_path, type_name));
            }
        }

        if nm_positions.len() >= 2 {
            let first_nm = nm_positions[0];
            let second_nm = nm_positions[1];

            let parent_parts: Vec<String> = components[first_nm + 1..second_nm]
                .iter()
                .filter_map(|c| match c {
                    Component::Normal(part) => part.to_str().map(str::to_string),
                    _ => None,
                })
                .collect();

            let nested_start = second_nm + 1;
            let nested_len = if components.get(nested_start).is_some_and(|c| {
                matches!(c, Component::Normal(p) if p.to_str().is_some_and(|s| s.starts_with('@')))
            }) {
                2
            } else {
                1
            };

            let nested_parts: Vec<String> = components[nested_start..nested_start + nested_len]
                .iter()
                .filter_map(|c| match c {
                    Component::Normal(part) => part.to_str().map(str::to_string),
                    _ => None,
                })
                .collect();

            if !parent_parts.is_empty() && !nested_parts.is_empty() {
                let from_path = format!(
                    "{}/node_modules/{}",
                    parent_parts.join("/"),
                    nested_parts.join("/")
                );
                return Some((from_path, type_name));
            }
        }

        if nm_positions.len() == 1 {
            let nm_idx = nm_positions[0];
            let pkg_start = nm_idx + 1;
            let pkg_len = if components.get(pkg_start).is_some_and(|c| {
                matches!(c, Component::Normal(p) if p.to_str().is_some_and(|s| s.starts_with('@')))
            }) {
                2
            } else {
                1
            };

            let subpath_start = pkg_start + pkg_len;
            if subpath_start < components.len() {
                let package_root = Path::new(&source_path)
                    .components()
                    .take(nm_idx + 1 + pkg_len)
                    .collect::<std::path::PathBuf>();

                let subpath_parts: Vec<String> = components[subpath_start..]
                    .iter()
                    .filter_map(|c| match c {
                        Component::Normal(part) => part.to_str().map(str::to_string),
                        _ => None,
                    })
                    .collect();
                let relative_path = subpath_parts.join("/");

                if let Some(runtime_path) = self.declaration_runtime_relative_path(&relative_path)
                    && self
                        .reverse_export_specifier_for_runtime_path(&package_root, &runtime_path)
                        .is_none()
                {
                    let pkg_json_path = package_root.join("package.json");
                    if let Ok(pkg_content) = std::fs::read_to_string(&pkg_json_path)
                        && let Ok(pkg_json) =
                            serde_json::from_str::<serde_json::Value>(&pkg_content)
                        && let Some(exports) = pkg_json.get("exports")
                        && Self::exports_has_explicit_subpaths(exports)
                    {
                        let mut from_path =
                            self.calculate_relative_path(&self.ctx.file_name, &source_path);
                        from_path = self.strip_ts_extensions(&from_path);
                        from_path = from_path.trim_end_matches('/').to_string();
                        return Some((from_path, type_name));
                    }
                }
            }
        }

        None
    }

    fn symbol_source_path(&self, sym_id: SymbolId) -> Option<String> {
        if let Some(file_idx) = self.ctx.resolve_symbol_file_index(sym_id) {
            let arena = self.ctx.get_arena_for_file(file_idx as u32);
            if let Some(source_file) = arena.source_files.first() {
                return Some(source_file.file_name.clone());
            }
        }

        let symbol = self.get_symbol_from_any_binder(sym_id)?;
        if symbol.decl_file_idx != u32::MAX {
            let arena = self.ctx.get_arena_for_file(symbol.decl_file_idx);
            if let Some(source_file) = arena.source_files.first() {
                return Some(source_file.file_name.clone());
            }
        }

        if let Some(arena) = self.ctx.binder.symbol_arenas.get(&sym_id)
            && let Some(source_file) = arena.source_files.first()
        {
            return Some(source_file.file_name.clone());
        }

        for binder in self
            .ctx
            .all_binders
            .as_ref()
            .into_iter()
            .flat_map(|binders| binders.iter())
        {
            if let Some(arena) = binder.symbol_arenas.get(&sym_id)
                && let Some(source_file) = arena.source_files.first()
            {
                return Some(source_file.file_name.clone());
            }
            for &decl_idx in &symbol.declarations {
                if let Some(arenas) = binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                    for arena in arenas {
                        if let Some(source_file) = arena.source_files.first() {
                            return Some(source_file.file_name.clone());
                        }
                    }
                }
            }
        }

        for lib_ctx in &self.ctx.lib_contexts {
            let binder = &lib_ctx.binder;
            if let Some(arena) = binder.symbol_arenas.get(&sym_id)
                && let Some(source_file) = arena.source_files.first()
            {
                return Some(source_file.file_name.clone());
            }
            for &decl_idx in &symbol.declarations {
                if let Some(arenas) = binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                    for arena in arenas {
                        if let Some(source_file) = arena.source_files.first() {
                            return Some(source_file.file_name.clone());
                        }
                    }
                }
            }
        }

        None
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

    fn unique_symbol_type_is_inaccessible(&self, sym_id: SymbolId) -> bool {
        let Some((_, root_sym_id, file_idx)) = self.unique_symbol_report_target(sym_id) else {
            return false;
        };
        if file_idx == u32::MAX || file_idx == self.ctx.current_file_idx as u32 {
            return false;
        }

        if !self
            .ctx
            .get_binder_for_file(file_idx as usize)
            .is_some_and(tsz_binder::BinderState::is_external_module)
        {
            return false;
        }

        !self.local_value_name_resolves_to(root_sym_id)
    }

    fn exported_variable_initializer_symbol(&self, expr_idx: NodeIndex) -> Option<SymbolId> {
        let node = self.ctx.arena.get(expr_idx)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            return self.resolve_identifier_symbol_without_tracking(expr_idx);
        }

        None
    }

    fn symbol_references_inaccessible_unique_symbol_type(
        &self,
        sym_id: SymbolId,
        visited: &mut FxHashSet<SymbolId>,
    ) -> bool {
        let sym_id = self.resolve_alias_symbol(sym_id, &mut Vec::new()).unwrap_or(sym_id);
        if !visited.insert(sym_id) {
            return false;
        }

        let Some(symbol) = self.get_symbol_from_any_binder(sym_id) else {
            return false;
        };

        let mut decl_candidates = symbol.declarations.clone();
        if symbol.value_declaration.is_some() && !decl_candidates.contains(&symbol.value_declaration)
        {
            decl_candidates.push(symbol.value_declaration);
        }

        for decl_idx in decl_candidates {
            if !decl_idx.is_some() {
                continue;
            }

            let mut candidate_arenas: Vec<&tsz_parser::parser::node::NodeArena> = Vec::new();
            if let Some(owner_binder) = self
                .ctx
                .resolve_symbol_file_index(sym_id)
                .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
            {
                if let Some(arenas) = owner_binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                    candidate_arenas.extend(arenas.iter().map(std::convert::AsRef::as_ref));
                }
                if let Some(symbol_arena) = owner_binder.symbol_arenas.get(&sym_id) {
                    candidate_arenas.push(symbol_arena.as_ref());
                }
            }
            if let Some(symbol_arena) = self.ctx.binder.symbol_arenas.get(&sym_id) {
                candidate_arenas.push(symbol_arena.as_ref());
            }
            candidate_arenas.push(self.ctx.arena);

            for arena in candidate_arenas {
                if self.node_references_inaccessible_unique_symbol_type(
                    arena,
                    decl_idx,
                    sym_id,
                    visited,
                ) {
                    return true;
                }
            }
        }

        false
    }

    fn node_references_inaccessible_unique_symbol_type(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        node_idx: NodeIndex,
        owner_sym_id: SymbolId,
        visited: &mut FxHashSet<SymbolId>,
    ) -> bool {
        let Some(node) = arena.get(node_idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::TYPE_OPERATOR
            && arena.get_type_operator(node).is_some_and(|op| {
                op.operator == SyntaxKind::UniqueKeyword as u16
                    && self.node_is_symbol_type_reference(arena, op.type_node)
            })
        {
            return self.type_symbol_is_inaccessible(owner_sym_id);
        }

        if node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = arena.get_type_ref(node)
            && let Some(type_sym_id) = self.type_reference_symbol_in_arena(arena, type_ref.type_name)
            && self.symbol_references_inaccessible_unique_symbol_type(type_sym_id, visited)
        {
            return true;
        }

        arena
            .get_children(node_idx)
            .into_iter()
            .any(|child| {
                self.node_references_inaccessible_unique_symbol_type(
                    arena,
                    child,
                    owner_sym_id,
                    visited,
                )
            })
    }

    fn node_is_symbol_type_reference(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        node_idx: NodeIndex,
    ) -> bool {
        let Some(node) = arena.get(node_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = arena.get_type_ref(node) else {
            return false;
        };
        let Some(name_node) = arena.get(type_ref.type_name) else {
            return false;
        };

        arena
            .get_identifier(name_node)
            .is_some_and(|ident| ident.escaped_text == "symbol")
    }

    fn type_reference_symbol_in_arena(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        type_name_idx: NodeIndex,
    ) -> Option<SymbolId> {
        let binder = self.ctx.get_binder_for_arena(arena)?;
        if let Some(sym_id) = binder.get_node_symbol(type_name_idx) {
            return Some(sym_id);
        }

        let node = arena.get(type_name_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let ident = arena.get_identifier(node)?;
        binder.file_locals.get(ident.escaped_text.as_str())
    }

    fn type_symbol_is_inaccessible(&self, sym_id: SymbolId) -> bool {
        let Some(symbol) = self.get_symbol_from_any_binder(sym_id) else {
            return false;
        };
        let file_idx = symbol.decl_file_idx;
        if file_idx == u32::MAX || file_idx == self.ctx.current_file_idx as u32 {
            return false;
        }

        if !self
            .ctx
            .get_binder_for_file(file_idx as usize)
            .is_some_and(tsz_binder::BinderState::is_external_module)
        {
            return false;
        }

        !self.local_name_resolves_to(sym_id)
    }

    fn local_name_resolves_to(&self, target_sym_id: SymbolId) -> bool {
        self.ctx
            .binder
            .file_locals
            .iter()
            .any(|(_, &local_sym_id)| {
                let Some(symbol) = self.ctx.binder.get_symbol(local_sym_id) else {
                    return false;
                };
                let is_from_current_file = symbol.decl_file_idx == u32::MAX
                    || symbol.decl_file_idx == self.ctx.current_file_idx as u32;
                let is_import = symbol.flags & tsz_binder::symbol_flags::ALIAS != 0;
                if !is_from_current_file && !is_import {
                    return false;
                }

                if local_sym_id == target_sym_id {
                    return true;
                }

                self.ctx.binder.resolve_import_symbol(local_sym_id) == Some(target_sym_id)
            })
    }

    fn unique_symbol_report_target(&self, sym_id: SymbolId) -> Option<(String, SymbolId, u32)> {
        let symbol = self.get_symbol_from_any_binder(sym_id)?;
        let file_idx = symbol.decl_file_idx;
        let owner_binder = self
            .ctx
            .get_binder_for_file(file_idx as usize)
            .unwrap_or(self.ctx.binder);

        let mut namespace_names = Vec::new();
        let mut root_namespace_sym = SymbolId::NONE;
        let mut parent_sym_id = symbol.parent;
        while !parent_sym_id.is_none() {
            let Some(parent_symbol) = self.get_symbol_from_any_binder(parent_sym_id) else {
                break;
            };
            if (parent_symbol.flags
                & (tsz_binder::symbol_flags::VALUE_MODULE
                    | tsz_binder::symbol_flags::NAMESPACE_MODULE))
                == 0
            {
                break;
            }
            namespace_names.push(parent_symbol.escaped_name.clone());
            root_namespace_sym = parent_sym_id;
            parent_sym_id = parent_symbol.parent;
        }
        if !namespace_names.is_empty() {
            namespace_names.reverse();
            return Some((namespace_names.join("."), root_namespace_sym, file_idx));
        }

        let matches_symbol = |candidate_sym_id: SymbolId| {
            if candidate_sym_id == sym_id {
                return true;
            }
            let Some(candidate_symbol) = owner_binder.get_symbol(candidate_sym_id) else {
                return false;
            };
            candidate_symbol.escaped_name == symbol.escaped_name
                && (candidate_symbol.value_declaration_span == symbol.value_declaration_span
                    || candidate_symbol.first_declaration_span == symbol.first_declaration_span)
        };

        for candidate in owner_binder.symbols.iter() {
            if (candidate.flags
                & (tsz_binder::symbol_flags::VALUE_MODULE
                    | tsz_binder::symbol_flags::NAMESPACE_MODULE))
                == 0
            {
                continue;
            }
            let Some(exports) = candidate.exports.as_ref() else {
                continue;
            };
            if !exports
                .iter()
                .any(|(_, exported_sym_id)| matches_symbol(*exported_sym_id))
            {
                continue;
            }
            return Some((candidate.escaped_name.clone(), candidate.id, file_idx));
        }

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
                let mut variable_decl_idx = decl_idx;
                let Some(mut node) = arena.get(variable_decl_idx) else {
                    continue;
                };

                if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                    let mut parent = arena
                        .get_extended(variable_decl_idx)
                        .map_or(NodeIndex::NONE, |info| info.parent);
                    while !parent.is_none() {
                        let Some(parent_node) = arena.get(parent) else {
                            break;
                        };
                        if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                            variable_decl_idx = parent;
                            node = parent_node;
                            break;
                        }
                        parent = arena
                            .get_extended(parent)
                            .map_or(NodeIndex::NONE, |info| info.parent);
                    }
                }

                if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                    continue;
                }

                let mut namespace_names = Vec::new();
                let mut namespace_nodes = Vec::new();
                let mut parent = arena
                    .get_extended(variable_decl_idx)
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

    fn exports_has_explicit_subpaths(exports: &serde_json::Value) -> bool {
        match exports {
            serde_json::Value::Object(map) => map.keys().any(|k| k.starts_with("./") || k == "."),
            _ => false,
        }
    }

    fn declaration_runtime_relative_path(&self, relative_path: &str) -> Option<String> {
        let relative_path = relative_path.replace('\\', "/");

        for (decl_ext, runtime_ext) in [
            (".d.ts", ".js"),
            (".d.tsx", ".jsx"),
            (".d.mts", ".mjs"),
            (".d.cts", ".cjs"),
            (".ts", ".js"),
            (".tsx", ".jsx"),
            (".mts", ".mjs"),
            (".cts", ".cjs"),
        ] {
            if let Some(prefix) = relative_path.strip_suffix(decl_ext) {
                return Some(format!("{prefix}{runtime_ext}"));
            }
        }

        Some(relative_path)
    }

    fn calculate_relative_path(&self, current: &str, source: &str) -> String {
        use std::path::{Component, Path};

        let current_path = Path::new(current);
        let source_path = Path::new(source);
        let current_dir = current_path.parent().unwrap_or(current_path);

        let current_components: Vec<_> = current_dir.components().collect();
        let source_components: Vec<_> = source_path.components().collect();

        let common_len = current_components
            .iter()
            .zip(source_components.iter())
            .take_while(|(a, b)| a == b)
            .count();

        let ups = current_components.len() - common_len;
        let mut result = String::new();
        if ups == 0 {
            result.push_str("./");
        } else {
            for _ in 0..ups {
                result.push_str("../");
            }
        }

        let remaining: Vec<_> = source_components[common_len..]
            .iter()
            .filter_map(|component| match component {
                Component::Normal(part) => Some(part.to_str()?),
                _ => None,
            })
            .collect();
        result.push_str(&remaining.join("/"));

        result
    }

    fn reverse_export_specifier_for_runtime_path(
        &self,
        package_root: &std::path::Path,
        runtime_relative_path: &str,
    ) -> Option<String> {
        let package_json_path = package_root.join("package.json");
        let package_json = std::fs::read_to_string(package_json_path).ok()?;
        let package_json: serde_json::Value = serde_json::from_str(&package_json).ok()?;
        let exports = package_json.get("exports")?;
        let runtime_relative_path = format!("./{}", runtime_relative_path.trim_start_matches("./"));
        self.reverse_match_exports_subpath(exports, &runtime_relative_path)
    }

    fn reverse_match_exports_subpath(
        &self,
        exports: &serde_json::Value,
        runtime_path: &str,
    ) -> Option<String> {
        match exports {
            serde_json::Value::String(target) => {
                self.match_export_target(".", target, runtime_path)
            }
            serde_json::Value::Array(entries) => entries
                .iter()
                .find_map(|entry| self.reverse_match_exports_subpath(entry, runtime_path)),
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    if key == "." || key.starts_with("./") {
                        if let Some(specifier) =
                            self.reverse_match_export_entry(key, value, runtime_path)
                        {
                            return Some(specifier);
                        }
                        continue;
                    }

                    if let Some(specifier) = self.reverse_match_exports_subpath(value, runtime_path)
                    {
                        return Some(specifier);
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn reverse_match_export_entry(
        &self,
        subpath_key: &str,
        value: &serde_json::Value,
        runtime_path: &str,
    ) -> Option<String> {
        match value {
            serde_json::Value::String(target) => {
                self.match_export_target(subpath_key, target, runtime_path)
            }
            serde_json::Value::Array(entries) => entries.iter().find_map(|entry| {
                self.reverse_match_export_entry(subpath_key, entry, runtime_path)
            }),
            serde_json::Value::Object(map) => map.values().find_map(|entry| {
                self.reverse_match_export_entry(subpath_key, entry, runtime_path)
            }),
            _ => None,
        }
    }

    fn match_export_target(
        &self,
        subpath_key: &str,
        target: &str,
        runtime_path: &str,
    ) -> Option<String> {
        let target = target.trim();
        let runtime_path = runtime_path.trim();

        if target.contains('*') {
            let wildcard = self.match_export_wildcard(target, runtime_path)?;
            return Some(self.apply_export_wildcard(subpath_key, &wildcard));
        }

        if target.ends_with('/') && subpath_key.ends_with('/') {
            let remainder = runtime_path.strip_prefix(target)?;
            return Some(format!(
                "{}{}",
                subpath_key.trim_start_matches("./"),
                remainder
            ));
        }

        if target != runtime_path {
            return None;
        }

        if subpath_key == "." {
            return Some(String::new());
        }

        Some(subpath_key.trim_start_matches("./").to_string())
    }

    fn match_export_wildcard(&self, pattern: &str, value: &str) -> Option<String> {
        let star_idx = pattern.find('*')?;
        let prefix = &pattern[..star_idx];
        let suffix = &pattern[star_idx + 1..];
        let middle = value.strip_prefix(prefix)?.strip_suffix(suffix)?;
        Some(middle.to_string())
    }

    fn apply_export_wildcard(&self, pattern: &str, wildcard: &str) -> String {
        pattern
            .replace('*', wildcard)
            .trim_start_matches("./")
            .to_string()
    }

    fn strip_ts_extensions(&self, path: &str) -> String {
        for ext in [
            ".d.ts", ".d.tsx", ".d.mts", ".d.cts", ".tsx", ".ts", ".mts", ".cts", ".jsx", ".js",
            ".mjs", ".cjs",
        ] {
            if let Some(path) = path.strip_suffix(ext) {
                return path.to_string();
            }
        }

        path.to_string()
    }

    pub(crate) fn get_symbol_from_any_binder(
        &self,
        sym_id: SymbolId,
    ) -> Option<&tsz_binder::Symbol> {
        self.ctx
            .binder
            .get_symbol(sym_id)
            .or_else(|| {
                // O(1) fast-path via resolve_symbol_file_index
                let file_idx = self.ctx.resolve_symbol_file_index(sym_id);
                if let Some(file_idx) = file_idx
                    && let Some(binder) = self.ctx.get_binder_for_file(file_idx)
                    && let Some(sym) = binder.get_symbol(sym_id)
                {
                    return Some(sym);
                }
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
                // Skip symbols that came from other files via globals merge.
                // In the merged program, file_locals includes globals from all files.
                // For TS4023 "cannot be named" checks, only symbols that are actually
                // declared in or imported into the current file count as accessible.
                // A symbol from another file that ended up in globals is NOT nameable
                // in the current file's declaration emit unless it's explicitly imported.
                let is_from_current_file = symbol.decl_file_idx == u32::MAX
                    || symbol.decl_file_idx == self.ctx.current_file_idx as u32;
                let is_import = symbol.flags & tsz_binder::symbol_flags::ALIAS != 0;
                if !is_from_current_file && !is_import {
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
