//! Value declaration resolution, TDZ checking, and identifier type computation helpers.

use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Check for TDZ violations: variable used before its declaration in a
    /// static block, computed property, or heritage clause; or class/enum
    /// used before its declaration anywhere in the same scope.
    /// Emits TS2448 (variable), TS2449 (class), or TS2450 (enum) and returns
    /// `true` if a violation is found.
    pub(crate) fn check_tdz_violation(
        &mut self,
        sym_id: SymbolId,
        idx: NodeIndex,
        name: &str,
    ) -> bool {
        // Skip TDZ checks in cross-arena delegation context.
        // TDZ compares node positions, which are meaningless when the usage node
        // and declaration node come from different files' arenas.
        if Self::is_in_cross_arena_delegation() {
            return false;
        }
        let is_tdz_in_static_block =
            self.is_variable_used_before_declaration_in_static_block(sym_id, idx);
        let is_tdz_in_property_initializer =
            self.is_variable_used_before_declaration_in_computed_property(sym_id, idx);
        let is_tdz_in_heritage_clause =
            self.is_variable_used_before_declaration_in_heritage_clause(sym_id, idx);
        let is_tdz = is_tdz_in_static_block
            || is_tdz_in_property_initializer
            || is_tdz_in_heritage_clause
            || self.is_class_or_enum_used_before_declaration(sym_id, idx);
        if is_tdz {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            // Emit the correct diagnostic based on symbol kind:
            // TS2449 for classes, TS2450 for enums, TS2448 for variables
            let (msg_template, code) = if let Some(sym) = self.ctx.binder.symbols.get(sym_id) {
                if sym.flags & tsz_binder::symbol_flags::CLASS != 0 {
                    (
                        diagnostic_messages::CLASS_USED_BEFORE_ITS_DECLARATION,
                        diagnostic_codes::CLASS_USED_BEFORE_ITS_DECLARATION,
                    )
                } else if sym.flags & tsz_binder::symbol_flags::REGULAR_ENUM != 0 {
                    (
                        diagnostic_messages::ENUM_USED_BEFORE_ITS_DECLARATION,
                        diagnostic_codes::ENUM_USED_BEFORE_ITS_DECLARATION,
                    )
                } else {
                    (
                        diagnostic_messages::BLOCK_SCOPED_VARIABLE_USED_BEFORE_ITS_DECLARATION,
                        diagnostic_codes::BLOCK_SCOPED_VARIABLE_USED_BEFORE_ITS_DECLARATION,
                    )
                }
            } else {
                (
                    diagnostic_messages::BLOCK_SCOPED_VARIABLE_USED_BEFORE_ITS_DECLARATION,
                    diagnostic_codes::BLOCK_SCOPED_VARIABLE_USED_BEFORE_ITS_DECLARATION,
                )
            };
            let message = format_message(msg_template, &[name]);
            self.error_at_node(idx, &message, code);

            // TypeScript also reports TS2454 ("used before being assigned") as a
            // companion to TDZ errors in strict-null mode, but ONLY for pure
            // block-scoped variables in non-deferred contexts:
            // - Static blocks and regular code → emit companion
            // - Computed property names → NO companion
            // - Static property initializers → NO companion
            // - Heritage clauses → NO companion
            // - Class/enum declarations → NO companion (they get TS2449/TS2450)
            if !is_tdz_in_property_initializer
                && !is_tdz_in_heritage_clause
                && !self.is_in_static_property_initializer_ast_context(idx)
                && self.ctx.strict_null_checks()
                && (!self.ctx.binder.symbols.get(sym_id).is_some_and(|sym| {
                    sym.decl_file_idx != u32::MAX
                        && sym.decl_file_idx != self.ctx.current_file_idx as u32
                }))
                && self.ctx.binder.symbols.get(sym_id).is_some_and(|sym| {
                    sym.flags & tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE != 0
                        && sym.flags
                            & (tsz_binder::symbol_flags::CLASS
                                | tsz_binder::symbol_flags::REGULAR_ENUM)
                            == 0
                })
                && let Some(usage_node) = self.ctx.arena.get(idx)
            {
                let key = (usage_node.pos, sym_id);
                if self.ctx.emitted_ts2454_errors.insert(key) {
                    self.error_variable_used_before_assigned_at(name, idx);
                }
            }

            // TS2729 companion for static property initializers:
            // in `X.Y`, when `X` is in TDZ, tsc also reports that `Y` is used
            // before initialization at the property name site.
            if self.is_in_static_property_initializer_ast_context(idx)
                && let Some(ext) = self.ctx.arena.get_extended(idx)
                && !ext.parent.is_none()
                && let Some(parent) = self.ctx.arena.get(ext.parent)
                && parent.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && let Some(access) = self.ctx.arena.get_access_expr(parent)
                && access.expression == idx
                && let Some(name_node) = self.ctx.arena.get(access.name_or_argument)
                && let Some(name_ident) = self.ctx.arena.get_identifier(name_node)
            {
                self.error_at_node(
                    access.name_or_argument,
                    &format!(
                        "Property '{}' is used before its initialization.",
                        name_ident.escaped_text
                    ),
                    tsz_common::diagnostics::diagnostic_codes::PROPERTY_IS_USED_BEFORE_ITS_INITIALIZATION,
                );
            }

            // TS2538: When a variable is used before declaration in a computed property,
            // it has implicit type 'any', which cannot be used as an index type.
            // Emit this additional error for computed property contexts.
            let is_in_computed_property =
                self.is_variable_used_before_declaration_in_computed_property(sym_id, idx);
            if is_in_computed_property {
                let message = format_message(
                    diagnostic_messages::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                    &["any"],
                );
                self.error_at_node(
                    idx,
                    &message,
                    diagnostic_codes::TYPE_CANNOT_BE_USED_AS_AN_INDEX_TYPE,
                );
            }
        }
        is_tdz
    }

    /// Returns true when `usage_idx` is lexically inside a static class property
    /// initializer (`static x = ...`).
    pub(crate) fn is_in_static_property_initializer_ast_context(
        &self,
        usage_idx: NodeIndex,
    ) -> bool {
        let mut current = usage_idx;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            if ext.parent.is_none() {
                break;
            }
            let parent = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                break;
            };
            if parent_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                if let Some(prop) = self.ctx.arena.get_property_decl(parent_node) {
                    return !prop.initializer.is_none()
                        && self.has_static_modifier(&prop.modifiers);
                }
                return false;
            }
            current = parent;
        }
        false
    }

    /// Resolve the value-side type from a symbol's value declaration node.
    ///
    /// This is used for merged interface+value globals where value position must
    /// use the constructor/variable declaration type, not the interface type.
    /// Check if a value declaration has a self-referential type annotation.
    /// For example, `declare var Math: Math` has type annotation "Math"
    /// which matches the symbol name "Math". This pattern is common for
    /// lib globals that follow the `declare var X: X` pattern.
    pub(crate) fn is_self_referential_var_type(
        &self,
        _sym_id: SymbolId,
        value_decl: NodeIndex,
        name: &str,
    ) -> bool {
        // Try to find the value declaration in the current arena first
        if let Some(node) = self.ctx.arena.get(value_decl)
            && let Some(var_decl) = self.ctx.arena.get_variable_declaration(node)
            && !var_decl.type_annotation.is_none()
            && let Some(type_node) = self.ctx.arena.get(var_decl.type_annotation)
            && let Some(type_ref) = self.ctx.arena.get_type_ref(type_node)
            && let Some(name_node) = self.ctx.arena.get(type_ref.type_name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            return ident.escaped_text == name;
        }

        // For declarations in other arenas (lib files), check via declaration_arenas
        if let Some(decl_arena) = self
            .ctx
            .binder
            .declaration_arenas
            .get(&(_sym_id, value_decl))
            .and_then(|v| v.first())
            && let Some(node) = decl_arena.get(value_decl)
            && let Some(var_decl) = decl_arena.get_variable_declaration(node)
            && !var_decl.type_annotation.is_none()
            && let Some(type_node) = decl_arena.get(var_decl.type_annotation)
            && let Some(type_ref) = decl_arena.get_type_ref(type_node)
            && let Some(name_node) = decl_arena.get(type_ref.type_name)
            && let Some(ident) = decl_arena.get_identifier(name_node)
        {
            return ident.escaped_text == name;
        }

        false
    }

    pub(crate) fn type_of_value_declaration(&mut self, decl_idx: NodeIndex) -> TypeId {
        if decl_idx.is_none() {
            return TypeId::UNKNOWN;
        }

        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return TypeId::UNKNOWN;
        };

        if let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) {
            if !var_decl.type_annotation.is_none() {
                let annotated = self.get_type_from_type_node(var_decl.type_annotation);
                return self.resolve_ref_type(annotated);
            }
            if !var_decl.initializer.is_none() {
                return self.get_type_of_node(var_decl.initializer);
            }
            return TypeId::ANY;
        }

        if self.ctx.arena.get_function(node).is_some() {
            return self.get_type_of_function(decl_idx);
        }

        if let Some(class_data) = self.ctx.arena.get_class(node) {
            return self.get_class_constructor_type(decl_idx, class_data);
        }

        TypeId::UNKNOWN
    }

    /// Resolve a value declaration type, delegating to the declaration's arena
    /// when the node does not belong to the current checker arena.
    pub(crate) fn type_of_value_declaration_for_symbol(
        &mut self,
        sym_id: SymbolId,
        decl_idx: NodeIndex,
    ) -> TypeId {
        if decl_idx.is_none() {
            return TypeId::UNKNOWN;
        }

        // Check declaration_arenas FIRST for the precise arena mapping.
        // This is critical for lib symbols where the same NodeIndex can exist
        // in both the lib arena and the main arena (cross-arena collision).
        // If we checked arena.get() first, we'd read a wrong node from the
        // main arena instead of the correct node from the lib arena.
        let decl_arena = if let Some(da) = self
            .ctx
            .binder
            .declaration_arenas
            .get(&(sym_id, decl_idx))
            .and_then(|v| v.first())
        {
            if std::ptr::eq(da.as_ref(), self.ctx.arena) {
                return self.type_of_value_declaration(decl_idx);
            }
            Some(std::sync::Arc::clone(da))
        } else if self.ctx.arena.get(decl_idx).is_some() {
            // Node exists in current arena but no declaration_arenas entry.
            // For non-lib symbols: this is the correct arena — use fast path.
            // For lib symbols: this may be a cross-arena collision — use symbol_arenas.
            if !self.ctx.binder.symbol_arenas.contains_key(&sym_id) {
                return self.type_of_value_declaration(decl_idx);
            }
            self.ctx.binder.symbol_arenas.get(&sym_id).cloned()
        } else {
            None
        };
        let Some(decl_arena) = decl_arena else {
            return TypeId::UNKNOWN;
        };
        if std::ptr::eq(decl_arena.as_ref(), self.ctx.arena) {
            return self.type_of_value_declaration(decl_idx);
        }

        // For lib declarations, check if the type annotation is a simple type reference
        // to a global lib type. If so, use resolve_lib_type_by_name directly instead of
        // creating a child checker. The child checker inherits the parent's merged binder,
        // which can have wrong symbol IDs for lib types, causing incorrect type resolution.
        if let Some(node) = decl_arena.get(decl_idx)
            && let Some(var_decl) = decl_arena.get_variable_declaration(node)
            && !var_decl.type_annotation.is_none()
        {
            // Try to extract the type name from a simple type reference
            if let Some(type_annotation_node) = decl_arena.get(var_decl.type_annotation)
                && let Some(type_ref) = decl_arena.get_type_ref(type_annotation_node)
            {
                // Check if this is a simple identifier (not qualified name)
                if let Some(type_name_node) = decl_arena.get(type_ref.type_name)
                    && let Some(ident) = decl_arena.get_identifier(type_name_node)
                {
                    let type_name = ident.escaped_text.as_str();
                    // Use resolve_lib_type_by_name for global lib types
                    if let Some(lib_type) = self.resolve_lib_type_by_name(type_name)
                        && lib_type != TypeId::UNKNOWN
                        && lib_type != TypeId::ERROR
                    {
                        return self.resolve_ref_type(lib_type);
                    }
                }
            }
        }

        // Guard against deep cross-arena recursion (shared with all delegation points)
        if !Self::enter_cross_arena_delegation() {
            return TypeId::UNKNOWN;
        }

        let mut checker = Box::new(CheckerState::with_parent_cache(
            decl_arena.as_ref(),
            self.ctx.binder,
            self.ctx.types,
            self.ctx.file_name.clone(),
            self.ctx.compiler_options.clone(),
            self,
        ));
        checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
        checker.ctx.symbol_resolution_set = self.ctx.symbol_resolution_set.clone();
        checker.ctx.symbol_resolution_stack = self.ctx.symbol_resolution_stack.clone();
        checker
            .ctx
            .symbol_resolution_depth
            .set(self.ctx.symbol_resolution_depth.get());
        let result = checker.type_of_value_declaration(decl_idx);

        // DO NOT merge child's symbol_types back. See delegate_cross_arena_symbol_resolution
        // for the full explanation: node_symbols collisions across arenas cause cache poisoning.

        Self::leave_cross_arena_delegation();
        result
    }

    /// Resolve a value-side type by global name, preferring value declarations.
    ///
    /// This avoids incorrect type resolution when symbol IDs collide across
    /// binders (current file vs. lib files).
    pub(crate) fn type_of_value_symbol_by_name(&mut self, name: &str) -> TypeId {
        if let Some((sym_id, value_decl)) = self.find_value_declaration_in_libs(name) {
            let value_type = self.type_of_value_declaration_for_symbol(sym_id, value_decl);
            if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                return value_type;
            }
        }

        if let Some(value_sym_id) = self.find_value_symbol_in_libs(name) {
            let value_type = self.get_type_of_symbol(value_sym_id);
            if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                return value_type;
            }
        }

        TypeId::UNKNOWN
    }

    /// If `type_id` is an object type with a synthetic `"new"` member, return that member type.
    /// This supports constructor-like interfaces that lower construct signatures as properties.
    pub(crate) fn constructor_type_from_new_property(&self, type_id: TypeId) -> Option<TypeId> {
        let new_atom = self.ctx.types.intern_string("new");
        tsz_solver::type_queries::find_property_in_object(self.ctx.types, type_id, new_atom)
            .map(|prop| prop.type_id)
    }

    /// Extract a partial object type from non-sensitive properties of an object literal.
    ///
    /// Used during Round 1 of two-pass generic inference to get type information
    /// from concrete properties (like `state: 100`) while skipping context-sensitive
    /// properties (like `actions: { foo: s => s }`).
    ///
    /// This lets inference learn e.g. `State = number` from `state: 100` even when
    /// the overall object literal is context-sensitive.
    pub(crate) fn extract_non_sensitive_object_type(&mut self, idx: NodeIndex) -> Option<TypeId> {
        use crate::type_computation_complex::is_contextually_sensitive;

        let node = self.ctx.arena.get(idx)?;
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let obj = self.ctx.arena.get_literal_expr(node)?;

        let mut properties = Vec::new();

        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Property assignment: { x: value }
            if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                // Skip sensitive property initializers (lambdas, nested sensitive objects)
                if is_contextually_sensitive(self, prop.initializer) {
                    continue;
                }
                if let Some(name) = self.get_property_name(prop.name) {
                    // Compute type without contextual type
                    let prev_context = self.ctx.contextual_type;
                    self.ctx.contextual_type = None;
                    let value_type = self.get_type_of_node(prop.initializer);
                    self.ctx.contextual_type = prev_context;

                    let name_atom = self.ctx.types.intern_string(&name);
                    properties.push(tsz_solver::PropertyInfo::new(name_atom, value_type));
                }
            }
            // Shorthand property: { x }
            else if elem_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
                && let Some(shorthand) = self.ctx.arena.get_shorthand_property(elem_node)
                && let Some(name_node) = self.ctx.arena.get(shorthand.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                let name = ident.escaped_text.clone();
                let value_type = self.get_type_of_node(shorthand.name);
                let name_atom = self.ctx.types.intern_string(&name);
                properties.push(tsz_solver::PropertyInfo::new(name_atom, value_type));
            }
            // Methods and accessors are always context-sensitive — skip them
        }

        if properties.is_empty() {
            return None;
        }

        Some(self.ctx.types.factory().object(properties))
    }
}
