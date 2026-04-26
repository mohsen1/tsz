//! Value declaration resolution, TDZ checking, and identifier type computation helpers.

use crate::context::TypingRequest;
use crate::query_boundaries::common;
use crate::state::CheckerState;
use tsz_binder::{Symbol, SymbolId};
use tsz_parser::parser::NodeArena;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn is_fast_path_function_decl(
        &self,
        sym_id: SymbolId,
        symbol: &Symbol,
        decl_idx: Option<NodeIndex>,
        direct_symbol: Option<SymbolId>,
        identifier_text: &str,
    ) -> bool {
        let Some(decl_idx) = decl_idx else {
            return false;
        };
        let Some(decl) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if decl.kind != syntax_kind_ext::FUNCTION_DECLARATION {
            return false;
        }
        self.ctx.arena.get_function(decl).is_some_and(|func| {
            let is_unannotated_impl = func.type_annotation.is_none();
            let is_local_ambient_signature = func.body.is_none()
                && direct_symbol == Some(sym_id)
                && (symbol.decl_file_idx == self.ctx.current_file_idx as u32
                    || symbol.decl_file_idx == u32::MAX);

            symbol.escaped_name == identifier_text
                && (is_unannotated_impl || is_local_ambient_signature)
                && symbol.has_any_flags(tsz_binder::symbol_flags::FUNCTION)
                && symbol.has_any_flags(tsz_binder::symbol_flags::VALUE)
                && !symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS)
                && (symbol.decl_file_idx == u32::MAX
                    || symbol.decl_file_idx == self.ctx.current_file_idx as u32)
        })
    }

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
        emit_unassigned_companion: bool,
    ) -> bool {
        // Skip TDZ checks in cross-arena delegation context.
        // TDZ compares node positions, which are meaningless when the usage node
        // and declaration node come from different files' arenas.
        if Self::is_in_cross_arena_delegation() {
            return false;
        }
        // Skip TDZ checks in declaration files (.d.ts).
        // Declaration files have no runtime ordering, so forward references are valid.
        if self.ctx.is_declaration_file() {
            return false;
        }
        // Skip TDZ for cross-file symbols (resolved from another binder).
        // The SymbolId belongs to a different binder's arena, so looking it up
        // in the local binder would hit a different symbol at the same numeric
        // ID, causing false TS2448/TS2454 (cross-binder SymbolId collision).
        // Cross-file symbols (e.g., UMD global aliases from `export as namespace`)
        // have no same-file TDZ — they are evaluated when their origin file loads.
        if let Some(file_idx) = self.ctx.resolve_symbol_file_index(sym_id)
            && file_idx != self.ctx.current_file_idx {
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
                if sym.has_any_flags(tsz_binder::symbol_flags::CLASS) {
                    (
                        diagnostic_messages::CLASS_USED_BEFORE_ITS_DECLARATION,
                        diagnostic_codes::CLASS_USED_BEFORE_ITS_DECLARATION,
                    )
                } else if sym.has_any_flags(tsz_binder::symbol_flags::ENUM) {
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
            // - Variables typed as `any`/`unknown`/`undefined` → NO companion
            if emit_unassigned_companion
                && !is_tdz_in_property_initializer
                && !is_tdz_in_heritage_clause
                && !self.is_in_static_property_initializer_ast_context(idx)
                && !self.is_in_class_member_decorator_ast_context(idx)
                && !self.is_in_binding_element_default_initializer(idx)
                && self.ctx.strict_null_checks()
                && (!self.ctx.binder.symbols.get(sym_id).is_some_and(|sym| {
                    sym.decl_file_idx != u32::MAX
                        && sym.decl_file_idx != self.ctx.current_file_idx as u32
                }))
                && self.ctx.binder.symbols.get(sym_id).is_some_and(|sym| {
                    sym.has_any_flags(tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE)
                        && !sym.has_any_flags(
                            tsz_binder::symbol_flags::CLASS | tsz_binder::symbol_flags::ENUM,
                        )
                })
                && let Some(usage_node) = self.ctx.arena.get(idx)
            {
                // Check if the variable's declared type is `any`/`unknown`/contains
                // undefined — tsc suppresses the companion TS2454 in these cases.
                let declared_type = self.get_type_of_symbol(sym_id);
                let should_skip = if self.skip_definite_assignment_for_type(declared_type) {
                    // For destructured bindings in TDZ, get_type_of_symbol may return
                    // `any` because the initializer hasn't been processed yet (e.g.,
                    // `let {a} = {a: ''}` where `a` is `string` but inference yields `any`).
                    // Only emit TS2454 for these binding-element cases.
                    !self.symbol_is_destructured_binding_element(sym_id)
                } else {
                    // Self-circular `typeof` annotations like `const fn: typeof fn = ...`
                    // produce TS2502 at the declaration site and reduce to an unresolved
                    // type. tsc does not also emit TS2454 here — the circularity already
                    // signals that the variable's runtime value cannot be reasoned about.
                    let target = sym_id.0;
                    let types = self.ctx.types;
                    crate::query_boundaries::state::checking::has_type_query_for_symbol(
                        types,
                        declared_type,
                        target,
                        |ty| self.resolve_lazy_type(ty),
                    )
                };
                if !should_skip {
                    let key = (usage_node.pos, sym_id);
                    if self.ctx.emitted_ts2454_errors.insert(key) {
                        self.error_variable_used_before_assigned_at(name, idx);
                    }
                }
            }

            // TS2729 companion for property-value TDZ reads that happen during
            // class definition work:
            // - static property initializers
            // - property decorator expressions
            //
            // In `X.Y`, when `X` is in TDZ, tsc also reports that `Y` is used
            // before initialization at the property name site in those contexts.
            // Skip when inside a computed property name — those use A.p1 as a
            // key, not as a value, and tsc doesn't emit TS2729 there.
            if (self.is_in_static_property_initializer_ast_context(idx)
                || self.is_in_property_decorator_ast_context(idx))
                && self.find_enclosing_computed_property(idx).is_none()
                && let Some(ext) = self.ctx.arena.get_extended(idx)
                && ext.parent.is_some()
                && let Some(parent) = self.ctx.arena.get(ext.parent)
                && parent.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && let Some(access) = self.ctx.arena.get_access_expr(parent)
                && access.expression == idx
                && let Some(name_node) = self.ctx.arena.get(access.name_or_argument)
                && let Some(name_ident) = self.ctx.arena.get_identifier(name_node)
            {
                // Methods are hoisted — skip TS2729 for method members.
                let member_is_method = self
                    .ctx
                    .binder
                    .get_symbol(sym_id)
                    .and_then(|base_sym| {
                        let member_name = &name_ident.escaped_text;
                        let member_sym_id = base_sym
                            .exports
                            .as_ref()
                            .and_then(|e| e.get(member_name))
                            .or_else(|| {
                                base_sym.members.as_ref().and_then(|m| m.get(member_name))
                            })?;
                        let member_sym = self.ctx.binder.get_symbol(member_sym_id)?;
                        Some(member_sym.has_any_flags(tsz_binder::symbol_flags::METHOD))
                    })
                    .unwrap_or(false);
                if !member_is_method {
                    self.error_at_node(
                        access.name_or_argument,
                        &format!(
                            "Property '{}' is used before its initialization.",
                            name_ident.escaped_text
                        ),
                        tsz_common::diagnostics::diagnostic_codes::PROPERTY_IS_USED_BEFORE_ITS_INITIALIZATION,
                    );
                }
            }

            // Note: tsc does NOT emit TS2538 for TDZ variables in computed properties.
            // `any` is a valid index type (assignable to string | number | symbol),
            // so the TDZ error (TS2448/TS2449) is sufficient. The solver's
            // `get_invalid_index_type_member` correctly returns None for `any`.
        }
        is_tdz
    }

    /// Returns true when `usage_idx` is lexically inside a decorator expression
    /// that belongs to a class member decorator (including parameter decorators).
    fn is_in_class_member_decorator_ast_context(&self, usage_idx: NodeIndex) -> bool {
        self.decorated_class_member_owner_kind(usage_idx).is_some()
    }

    /// Returns true when `usage_idx` is lexically inside a decorator expression
    /// that belongs to a property declaration.
    fn is_in_property_decorator_ast_context(&self, usage_idx: NodeIndex) -> bool {
        self.decorated_class_member_owner_kind(usage_idx)
            == Some(syntax_kind_ext::PROPERTY_DECLARATION)
    }

    fn decorated_class_member_owner_kind(&self, usage_idx: NodeIndex) -> Option<u16> {
        let mut current = usage_idx;
        let mut decorator_idx = NodeIndex::NONE;
        let mut class_idx = NodeIndex::NONE;
        while current.is_some() {
            let node = self.ctx.arena.get(current)?;

            if node.kind == syntax_kind_ext::DECORATOR {
                decorator_idx = current;
            }

            if node.kind == syntax_kind_ext::CLASS_DECLARATION
                || node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                class_idx = current;
                break;
            }

            if node.is_function_like() && !self.ctx.arena.is_immediately_invoked(current) {
                return None;
            }

            let ext = self.ctx.arena.get_extended(current)?;
            current = ext.parent;
        }

        if decorator_idx.is_none() || class_idx.is_none() {
            return None;
        }

        let class_node = self.ctx.arena.get(class_idx)?;
        let class = self.ctx.arena.get_class(class_node)?;

        class.members.nodes.iter().find_map(|&member_idx| {
            let member_node = self.ctx.arena.get(member_idx)?;
            let member_has_decorator = match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                    .ctx
                    .arena
                    .get_property_decl(member_node)
                    .and_then(|prop| prop.modifiers.as_ref())
                    .is_some_and(|modifiers| modifiers.nodes.contains(&decorator_idx)),
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(member_node)
                    .and_then(|method| method.modifiers.as_ref())
                    .is_some_and(|modifiers| modifiers.nodes.contains(&decorator_idx)),
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    self.ctx
                        .arena
                        .get_accessor(member_node)
                        .and_then(|accessor| accessor.modifiers.as_ref())
                        .is_some_and(|modifiers| modifiers.nodes.contains(&decorator_idx))
                }
                k if k == syntax_kind_ext::CONSTRUCTOR => self
                    .ctx
                    .arena
                    .get_constructor(member_node)
                    .and_then(|ctor| ctor.modifiers.as_ref())
                    .is_some_and(|modifiers| modifiers.nodes.contains(&decorator_idx)),
                _ => false,
            };
            if member_has_decorator {
                return Some(member_node.kind);
            }

            let parameter_lists = match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(member_node)
                    .map(|method| &method.parameters),
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    self.ctx
                        .arena
                        .get_accessor(member_node)
                        .map(|accessor| &accessor.parameters)
                }
                k if k == syntax_kind_ext::CONSTRUCTOR => self
                    .ctx
                    .arena
                    .get_constructor(member_node)
                    .map(|ctor| &ctor.parameters),
                _ => None,
            };

            if let Some(parameters) = parameter_lists {
                for &param_idx in &parameters.nodes {
                    let Some(param_node) = self.ctx.arena.get(param_idx) else {
                        continue;
                    };
                    if self
                        .ctx
                        .arena
                        .get_parameter(param_node)
                        .and_then(|param| param.modifiers.as_ref())
                        .is_some_and(|modifiers| modifiers.nodes.contains(&decorator_idx))
                    {
                        return Some(syntax_kind_ext::PARAMETER);
                    }
                }
            }

            None
        })
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
                    return prop.initializer.is_some() && self.has_static_modifier(&prop.modifiers);
                }
                return false;
            }
            current = parent;
        }
        false
    }

    /// Check if a symbol's declaration is a binding element inside a destructuring
    /// pattern (e.g., `let {a} = expr` or `let [x, y] = expr`).
    ///
    /// For destructured bindings in TDZ, `get_type_of_symbol` returns `any` because
    /// the initializer hasn't been processed yet. The real type would NOT be `any`
    /// once inference completes, so TS2454 should still be emitted despite the `any`.
    fn symbol_is_destructured_binding_element(&self, sym_id: SymbolId) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        let decl_idx = symbol.value_declaration;
        if decl_idx.is_none() {
            return false;
        }

        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };

        // Direct binding elements are always destructured
        if decl_node.kind == syntax_kind_ext::BINDING_ELEMENT {
            return true;
        }

        // An identifier whose parent is a binding element
        if decl_node.kind == tsz_scanner::SyntaxKind::Identifier as u16
            && let Some(ext) = self.ctx.arena.get_extended(decl_idx)
            && ext.parent.is_some()
            && let Some(parent_node) = self.ctx.arena.get(ext.parent)
            && parent_node.kind == syntax_kind_ext::BINDING_ELEMENT
        {
            return true;
        }

        false
    }

    fn is_in_binding_element_default_initializer(&self, idx: NodeIndex) -> bool {
        let mut current = idx;

        for _ in 0..20 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };

            if parent_node.kind == syntax_kind_ext::BINDING_ELEMENT
                && let Some(binding) = self.ctx.arena.get_binding_element(parent_node)
                && binding.initializer.is_some()
                && self.is_node_within(idx, binding.initializer)
            {
                return true;
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
            && var_decl.type_annotation.is_some()
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
            && var_decl.type_annotation.is_some()
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
        self.type_of_value_declaration_with_mode(decl_idx, true)
    }

    pub(crate) fn type_of_value_declaration_without_module_augmentations(
        &mut self,
        decl_idx: NodeIndex,
    ) -> TypeId {
        self.type_of_value_declaration_with_mode(decl_idx, false)
    }

    fn type_of_value_declaration_with_mode(
        &mut self,
        decl_idx: NodeIndex,
        apply_module_augmentations: bool,
    ) -> TypeId {
        if decl_idx.is_none() {
            return TypeId::ERROR;
        }

        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return TypeId::ERROR;
        };
        if let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) {
            if var_decl.type_annotation.is_some() {
                let annotated = self.get_type_from_type_node(var_decl.type_annotation);
                return self.resolve_ref_type(annotated);
            }
            if self.ctx.is_js_file()
                && self.ctx.should_resolve_jsdoc()
                && let Some(jsdoc_type) = self.jsdoc_type_annotation_for_node(decl_idx)
            {
                return jsdoc_type;
            }
            let root_name = self
                .ctx
                .arena
                .get(var_decl.name)
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                .map(|ident| ident.escaped_text.clone());
            // For const declarations without type annotation, preserve the literal type
            // from the initializer (matching tsc behavior where `const x = "foo"` has
            // type `"foo"`, not `string`).
            if var_decl.initializer.is_some()
                && self.is_const_variable_declaration(decl_idx)
                && let Some(literal_type) = self.literal_type_from_initializer(var_decl.initializer)
            {
                if self.ctx.is_js_file()
                    && let Some(root_name) = root_name.as_deref()
                {
                    return self
                        .augment_object_type_with_define_properties(root_name, literal_type);
                }
                return literal_type;
            }
            if var_decl.initializer.is_some() {
                let mut init_type = self.get_type_of_node(var_decl.initializer);
                if self.ctx.is_js_file()
                    && let Some(root_name) = root_name.as_deref()
                {
                    init_type =
                        self.augment_object_type_with_define_properties(root_name, init_type);
                }
                return init_type;
            }
            return TypeId::ANY;
        }

        if self.ctx.arena.get_function(node).is_some() {
            return self.get_type_of_function(decl_idx);
        }

        if let Some(class_data) = self.ctx.arena.get_class(node) {
            return if apply_module_augmentations {
                self.get_class_constructor_type(decl_idx, class_data)
            } else {
                self.get_class_constructor_type_without_module_augmentations(decl_idx, class_data)
            };
        }

        // For expression nodes (e.g. `export default expr`), evaluate
        // the expression type. This handles identifiers, property accesses,
        // literals, and other expression kinds. Since `type_of_value_declaration`
        // is only called for same-arena declarations, `get_type_of_node` has the
        // correct context to evaluate the expression.
        // However, type-only declarations (interfaces, type aliases, etc.) must
        // return UNKNOWN so callers produce the correct TS2693 diagnostic.
        use tsz_parser::parser::syntax_kind_ext;
        let kind = node.kind;
        if kind == syntax_kind_ext::INTERFACE_DECLARATION
            || kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
            || kind == syntax_kind_ext::MODULE_DECLARATION
        {
            return TypeId::UNKNOWN;
        }

        self.get_type_of_node(decl_idx)
    }

    /// Resolve a value declaration type, delegating to the declaration's arena
    /// when the node does not belong to the current checker arena.
    pub(crate) fn type_of_value_declaration_for_symbol(
        &mut self,
        sym_id: SymbolId,
        decl_idx: NodeIndex,
    ) -> TypeId {
        self.type_of_value_declaration_for_symbol_with_mode(sym_id, decl_idx, true)
    }

    pub(crate) fn type_of_value_declaration_for_symbol_without_module_augmentations(
        &mut self,
        sym_id: SymbolId,
        decl_idx: NodeIndex,
    ) -> TypeId {
        self.type_of_value_declaration_for_symbol_with_mode(sym_id, decl_idx, false)
    }

    fn type_of_value_declaration_for_symbol_with_mode(
        &mut self,
        sym_id: SymbolId,
        decl_idx: NodeIndex,
        apply_module_augmentations: bool,
    ) -> TypeId {
        if decl_idx.is_none() {
            return TypeId::ERROR;
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
                return self
                    .type_of_value_declaration_with_mode(decl_idx, apply_module_augmentations);
            }
            Some(std::sync::Arc::clone(da))
        } else if self.ctx.arena.get(decl_idx).is_some() {
            // Node exists in current arena but no declaration_arenas entry.
            // For non-lib symbols: this is the correct arena — use fast path.
            // For lib symbols: this may be a cross-arena collision — use symbol_arenas.
            if !self.ctx.binder.symbol_arenas.contains_key(&sym_id) {
                return self
                    .type_of_value_declaration_with_mode(decl_idx, apply_module_augmentations);
            }
            self.ctx.binder.symbol_arenas.get(&sym_id).cloned()
        } else {
            None
        };
        let Some(decl_arena) = decl_arena else {
            return TypeId::ERROR;
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
            && var_decl.type_annotation.is_some()
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
            return TypeId::ERROR;
        }

        let delegate_file_name = decl_arena
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
            .unwrap_or_else(|| self.ctx.file_name.clone());
        let delegate_file_idx = self.ctx.get_file_idx_for_arena(decl_arena.as_ref());

        // Use the target arena's binder so symbol lookups resolve in the
        // declaration's context. Critical for module augmentation, where the
        // class symbol lives in the declaration's binder, not the parent's.
        let delegate_binder = self
            .ctx
            .get_binder_for_arena(decl_arena.as_ref())
            .unwrap_or(self.ctx.binder);
        let mut checker = Box::new(CheckerState::with_parent_cache(
            decl_arena.as_ref(),
            delegate_binder,
            self.ctx.types,
            delegate_file_name,
            self.ctx.compiler_options.clone(),
            self,
        ));
        checker.ctx.copy_cross_file_state_from(&self.ctx);
        checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
        checker.ctx.current_file_idx = delegate_file_idx.unwrap_or(self.ctx.current_file_idx);
        checker.ctx.symbol_resolution_set = self.ctx.symbol_resolution_set.clone();
        checker.ctx.symbol_resolution_stack = self.ctx.symbol_resolution_stack.clone();
        checker
            .ctx
            .symbol_resolution_depth
            .set(self.ctx.symbol_resolution_depth.get());
        let result =
            checker.type_of_value_declaration_with_mode(decl_idx, apply_module_augmentations);

        if let Some(node) = decl_arena.get(decl_idx)
            && decl_arena.get_class(node).is_some()
        {
            let def_id = self.ctx.get_or_create_def_id(sym_id);
            self.ctx
                .definition_store
                .register_type_to_def(result, def_id);
            if let Some(construct_signatures) =
                crate::query_boundaries::common::construct_signatures_for_type(
                    self.ctx.types,
                    result,
                )
            {
                for signature in construct_signatures {
                    self.ctx
                        .definition_store
                        .register_type_to_def(signature.return_type, def_id);
                }
            }
        }

        // DO NOT merge child's symbol_types back. See delegate_cross_arena_symbol_resolution
        // for the full explanation: node_symbols collisions across arenas cause cache poisoning.

        Self::leave_cross_arena_delegation();
        result
    }

    /// Resolve a declaration node type, delegating to the declaration's arena when needed.
    ///
    /// Unlike `type_of_value_declaration_for_symbol`, this works for non-value declaration
    /// nodes too, such as merged interface methods that live across multiple lib arenas.
    pub(crate) fn type_of_declaration_node_for_symbol(
        &mut self,
        sym_id: SymbolId,
        decl_idx: NodeIndex,
    ) -> TypeId {
        if decl_idx.is_none() {
            return TypeId::ERROR;
        }

        let mut arena_ptrs = rustc_hash::FxHashSet::default();
        let mut candidate_arenas: Vec<&NodeArena> = Vec::new();

        if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
            for arena in arenas {
                let arena_ref = arena.as_ref();
                let arena_ptr = arena_ref as *const NodeArena as usize;
                if arena_ptrs.insert(arena_ptr) {
                    candidate_arenas.push(arena_ref);
                }
            }
        }

        if candidate_arenas.is_empty() {
            if self.ctx.arena.get(decl_idx).is_some()
                && !self.ctx.binder.symbol_arenas.contains_key(&sym_id)
            {
                return self.get_type_of_node(decl_idx);
            }
            if let Some(symbol_arena) = self.ctx.binder.symbol_arenas.get(&sym_id) {
                let arena_ref = symbol_arena.as_ref();
                let arena_ptr = arena_ref as *const NodeArena as usize;
                if arena_ptrs.insert(arena_ptr) {
                    candidate_arenas.push(arena_ref);
                }
            }
            if candidate_arenas.is_empty() && self.ctx.arena.get(decl_idx).is_some() {
                candidate_arenas.push(self.ctx.arena);
            }
        }

        let mut merged = TypeId::ERROR;
        let mut has_type = false;

        for decl_arena in candidate_arenas {
            let decl_type = if std::ptr::eq(decl_arena, self.ctx.arena) {
                self.get_type_of_node(decl_idx)
            } else {
                if !Self::enter_cross_arena_delegation() {
                    continue;
                }

                let delegate_file_name = decl_arena
                    .source_files
                    .first()
                    .map(|sf| sf.file_name.clone())
                    .unwrap_or_else(|| self.ctx.file_name.clone());
                let delegate_file_idx = self.ctx.get_file_idx_for_arena(decl_arena);

                let mut checker = Box::new(CheckerState::with_parent_cache(
                    decl_arena,
                    self.ctx.binder,
                    self.ctx.types,
                    delegate_file_name,
                    self.ctx.compiler_options.clone(),
                    self,
                ));
                checker.ctx.copy_cross_file_state_from(&self.ctx);
                checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
                checker.ctx.current_file_idx =
                    delegate_file_idx.unwrap_or(self.ctx.current_file_idx);
                checker.ctx.symbol_resolution_set = self.ctx.symbol_resolution_set.clone();
                checker.ctx.symbol_resolution_stack = self.ctx.symbol_resolution_stack.clone();
                checker
                    .ctx
                    .symbol_resolution_depth
                    .set(self.ctx.symbol_resolution_depth.get());
                let result = checker.get_type_of_node(decl_idx);
                Self::leave_cross_arena_delegation();
                result
            };

            if matches!(decl_type, TypeId::ERROR | TypeId::UNKNOWN) {
                continue;
            }

            if has_type {
                merged = self.merge_interface_types(merged, decl_type);
            } else {
                merged = decl_type;
                has_type = true;
            }
        }

        if has_type { merged } else { TypeId::ERROR }
    }

    fn prefer_known_global_constructor_companion(
        &mut self,
        name: &str,
        value_type: TypeId,
    ) -> TypeId {
        if !self.is_known_global_value_name(name)
            || value_type == TypeId::UNKNOWN
            || value_type == TypeId::ERROR
        {
            return value_type;
        }

        let constructor_name = format!("{name}Constructor");
        if let Some(constructor_sym_id) = self.find_value_symbol_in_libs(&constructor_name) {
            let constructor_type = self.get_type_of_symbol(constructor_sym_id);
            if constructor_type != TypeId::UNKNOWN && constructor_type != TypeId::ERROR {
                return constructor_type;
            }
        }

        if let Some(constructor_type) = self.resolve_lib_type_by_name(&constructor_name)
            && constructor_type != TypeId::UNKNOWN
            && constructor_type != TypeId::ERROR
        {
            return constructor_type;
        }

        value_type
    }

    /// Resolve a value-side type by global name, preferring value declarations.
    ///
    /// This avoids incorrect type resolution when symbol IDs collide across
    /// binders (current file vs. lib files).
    pub(crate) fn type_of_value_symbol_by_name(&mut self, name: &str) -> TypeId {
        let lib_binders = self.get_lib_binders();

        if self.is_known_global_value_name(name) {
            let constructor_name = format!("{name}Constructor");
            if let Some(constructor_type) = self.resolve_lib_type_by_name(&constructor_name)
                && constructor_type != TypeId::UNKNOWN
                && constructor_type != TypeId::ERROR
            {
                return constructor_type;
            }
        }

        if let Some(value_sym_id) = self.ctx.binder.file_locals.get(name)
            && let Some(symbol) = self
                .ctx
                .binder
                .get_symbol_with_libs(value_sym_id, &lib_binders)
        {
            let value_flags_except_module =
                tsz_binder::symbol_flags::VALUE & !tsz_binder::symbol_flags::VALUE_MODULE;
            if (symbol.flags & value_flags_except_module) != 0 && !symbol.is_type_only {
                // Prefer the merged binder's own value declarations when available.
                // Driver-mode checking rebuilds checker-facing lib binders, so
                // direct SymbolIds from those fresh binders are not stable inputs
                // to type_of_value_declaration_for_symbol.
                for &decl_idx in &symbol.declarations {
                    if decl_idx.is_none() {
                        continue;
                    }
                    let value_type =
                        self.type_of_value_declaration_for_symbol(value_sym_id, decl_idx);
                    if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                        return self.prefer_known_global_constructor_companion(name, value_type);
                    }
                }

                let value_type = self.get_type_of_symbol(value_sym_id);
                if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                    return self.prefer_known_global_constructor_companion(name, value_type);
                }
            }
        }

        if let Some((sym_id, value_decl)) = self.find_value_declaration_in_libs(name) {
            let value_type = self.type_of_value_declaration_for_symbol(sym_id, value_decl);
            if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                return self.prefer_known_global_constructor_companion(name, value_type);
            }
        }

        if let Some(value_sym_id) = self.find_value_symbol_in_libs(name) {
            // For merged TYPE+VALUE symbols (e.g., `interface Symbol` + `declare var Symbol`),
            // get_type_of_symbol returns the interface type. In value context we need the
            // variable's declared type (e.g., SymbolConstructor). Search the symbol's
            // declarations for a variable declaration with a type annotation first.
            if let Some(symbol) = self
                .ctx
                .binder
                .get_symbol_with_libs(value_sym_id, &lib_binders)
            {
                let has_type_side = symbol.has_any_flags(tsz_binder::symbol_flags::TYPE);
                let has_value_side = symbol.has_any_flags(tsz_binder::symbol_flags::VALUE);
                if has_type_side && has_value_side {
                    // Merged symbol: scan declarations for a variable declaration
                    for &decl_idx in &symbol.declarations {
                        if decl_idx.is_none() {
                            continue;
                        }
                        let value_type =
                            self.type_of_value_declaration_for_symbol(value_sym_id, decl_idx);
                        if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                            return self
                                .prefer_known_global_constructor_companion(name, value_type);
                        }
                    }
                }
            }

            let value_type = self.get_type_of_symbol(value_sym_id);
            if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                return self.prefer_known_global_constructor_companion(name, value_type);
            }
        }

        TypeId::UNKNOWN
    }

    /// If `type_id` is an object type with a synthetic `"new"` member, return that member type.
    /// This supports constructor-like interfaces that lower construct signatures as properties.
    pub(crate) fn constructor_type_from_new_property(&self, type_id: TypeId) -> Option<TypeId> {
        let new_atom = self.ctx.types.intern_string("new");
        common::find_property_in_object(self.ctx.types, type_id, new_atom).map(|prop| prop.type_id)
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
        use super::complex::is_contextually_sensitive;

        let mut object_idx = idx;
        let mut wrap_in_zero_arg_function = false;
        let node = self.ctx.arena.get(idx)?;
        match node.kind {
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                object_idx = self.ctx.arena.get_parenthesized(node)?.expression;
            }
            k if k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION =>
            {
                let func = self.ctx.arena.get_function(node)?;
                if !func.parameters.nodes.is_empty() {
                    return None;
                }
                wrap_in_zero_arg_function = true;
                object_idx = func.body;
                if let Some(body_node) = self.ctx.arena.get(object_idx)
                    && body_node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                {
                    object_idx = self.ctx.arena.get_parenthesized(body_node)?.expression;
                }
            }
            _ => {}
        }

        let node = self.ctx.arena.get(object_idx)?;
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
                    let value_type = self
                        .extract_non_sensitive_object_type(prop.initializer)
                        .unwrap_or_else(|| {
                            // Compute type without contextual type
                            self.get_type_of_node_with_request(
                                prop.initializer,
                                &TypingRequest::NONE,
                            )
                        });

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
            // Methods with all params annotated are not context-sensitive
            else if elem_node.kind == syntax_kind_ext::METHOD_DECLARATION
                && !is_contextually_sensitive(self, elem_idx)
                && let Some(method) = self.ctx.arena.get_method_decl(elem_node)
                && let Some(name) = self.property_name_for_error(method.name)
            {
                // Use get_type_of_function for methods — get_type_of_node
                // doesn't handle METHOD_DECLARATION as expression nodes.
                let value_type =
                    self.get_type_of_function_with_request(elem_idx, &TypingRequest::NONE);
                let name_atom = self.ctx.types.intern_string(&name);
                properties.push(tsz_solver::PropertyInfo::new(name_atom, value_type));
            }
            // Accessors are always context-sensitive — skip them
        }

        if properties.is_empty() {
            return None;
        }

        let object_type = self.ctx.types.factory().object_fresh(properties);
        if wrap_in_zero_arg_function {
            Some(
                self.ctx
                    .types
                    .factory()
                    .function(tsz_solver::FunctionShape::new(Vec::new(), object_type)),
            )
        } else {
            Some(object_type)
        }
    }

    /// Enhance a partial Round 1 object type by including sensitive lambda properties
    /// whose contextual parameter types from the generic function shape are concrete
    /// (i.e., they don't depend on the type parameters being inferred).
    ///
    /// This enables "intra-expression inference" for patterns like:
    /// ```ts
    /// declare function callIt<T>(obj: { produce: (n: number) => T, consume: (x: T) => void }): void;
    /// callIt({ produce: _a => 0, consume: n => n.toFixed() });
    /// ```
    /// Here `produce`'s param type `(n: number)` doesn't depend on `T`, so we can
    /// safely type `_a` as `number` and use the return type `0` to infer `T = number`.
    pub(crate) fn extract_inference_contributing_object_type(
        &mut self,
        arg_idx: NodeIndex,
        target_param_type: TypeId,
        type_param_names: &[tsz_common::Atom],
    ) -> Option<TypeId> {
        use super::complex::is_contextually_sensitive;

        let node = self.ctx.arena.get(arg_idx)?;
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let obj = self.ctx.arena.get_literal_expr(node)?;

        // Evaluate the target parameter type when possible, but keep the raw target
        // around so contextual property lookup can still pierce unresolved generic
        // intersections/applications like `{ as?: C } & Elements[C]`.
        let target_type = self.evaluate_type_with_env(target_param_type);
        let target_shape = common::object_shape_for_type(self.ctx.types, target_type);
        let target_props: rustc_hash::FxHashMap<tsz_common::Atom, TypeId> = target_shape
            .map(|shape| {
                shape
                    .properties
                    .iter()
                    .map(|p| (p.name, p.type_id))
                    .collect()
            })
            .unwrap_or_default();

        let mut properties = Vec::new();

        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                let Some(name) = self.get_property_name(prop.name) else {
                    continue;
                };
                let name_atom = self.ctx.types.intern_string(&name);

                if !is_contextually_sensitive(self, prop.initializer) {
                    // Non-sensitive: compute type without context (already handled by
                    // extract_non_sensitive_object_type, but include here for completeness
                    // of the partial type).
                    let value_type =
                        self.get_type_of_node_with_request(prop.initializer, &TypingRequest::NONE);
                    properties.push(tsz_solver::PropertyInfo::new(name_atom, value_type));
                    continue;
                }

                // Sensitive property: check if the contextual function type's params are concrete
                let target_prop_type = target_props.get(&name_atom).copied().or_else(|| {
                    self.contextual_object_literal_property_type(target_param_type, &name)
                });
                let Some(target_prop_type) = target_prop_type else {
                    continue;
                };

                // If the target property type is a bare type parameter being inferred,
                // compute the property type without context. The property's type
                // directly constrains the type parameter.
                // Example: make({ mutations: { foo() {} }, action: (m) => m.foo() })
                // where mutations has target type M (a type param).
                if self.type_contains_any_type_param(target_prop_type, type_param_names)
                    && common::type_param_info(self.ctx.types, target_prop_type).is_some()
                {
                    let value_type =
                        self.speculative_type_of_node(prop.initializer, &TypingRequest::NONE);
                    properties.push(tsz_solver::PropertyInfo::new(name_atom, value_type));
                    continue;
                }

                // If the target property is an object type, try to recursively extract
                // inference-contributing properties from nested object literals.
                // Example: nested({ prop: { produce: (a) => [a], consume: (arg) => arg.join(",") } })
                // where prop has target type { produce: (arg1: number) => T, consume: (arg2: T) => void }
                if common::object_shape_for_type(self.ctx.types, target_prop_type).is_some()
                    && let Some(nested_partial) = self.extract_inference_contributing_object_type(
                        prop.initializer,
                        target_prop_type,
                        type_param_names,
                    )
                {
                    properties.push(tsz_solver::PropertyInfo::new(name_atom, nested_partial));
                    continue;
                }

                // If the target property type is a mapped type like `{ [K in keyof T]: V[K] }`,
                // recursively extract inference from the property value's object literal by
                // instantiating the mapped type template for each nested key.
                // Example: VuexStoreOptions pattern where `modules` has type
                // `{ [k in keyof Modules]: VuexStoreOptions<Modules[k], never> }` and the
                // initializer is `{ foo: { state() {...}, mutations: {...} } }`.
                if let Some(mapped_id) = crate::query_boundaries::common::mapped_type_id(
                    self.ctx.types,
                    target_prop_type,
                )
                    && let Some(nested_partial) = self.extract_inference_from_mapped_type_target(
                        prop.initializer,
                        mapped_id,
                        type_param_names,
                    ) {
                        properties.push(tsz_solver::PropertyInfo::new(name_atom, nested_partial));
                        continue;
                    }

                // Get the function shape for the target property
                let target_fn_shape =
                    common::function_shape_for_type(self.ctx.types, target_prop_type);
                let Some(target_fn) = target_fn_shape else {
                    continue;
                };

                // Check if ALL function parameter types are concrete (don't contain the
                // type parameters being inferred). Return type MAY contain them - that's
                // what we want to infer FROM.
                let params_are_concrete = target_fn.params.iter().all(|param| {
                    !self.type_contains_any_type_param(param.type_id, type_param_names)
                });

                if !params_are_concrete {
                    continue;
                }

                // When the return type contains unresolved type parameters AND the
                // function body has context-sensitive return expressions (e.g., nested
                // arrow functions with unannotated params in block-body returns),
                // skip speculative evaluation. The speculative pass would assign the
                // unresolved type parameter to inner function params, and while
                // diagnostics are rolled back, the resulting cached type pollutes the
                // inference. The full contextual type (with substituted type params)
                // will be applied in Round 2.
                if self.type_contains_any_type_param(target_fn.return_type, type_param_names)
                    && super::contextual::expression_needs_contextual_return_type(
                        self,
                        prop.initializer,
                    )
                {
                    continue;
                }

                // The contextual param types are concrete, so we can safely type this
                // lambda with those contextual types and extract its return type.
                // Use the target function type as contextual type for the lambda.
                // Suppress diagnostics from this speculative evaluation
                // (the params WILL get contextual types in the final pass).
                let value_type = self.speculative_type_of_node(
                    prop.initializer,
                    &TypingRequest::with_contextual_type(target_prop_type),
                );

                // If the speculative result still contains any of the type parameters
                // being inferred, skip it. Including such types in the partial can
                // poison Round 1 inference by creating self-referential constraints
                // (e.g., T appearing in both source and target positions).
                // This happens when a zero-param callback's return type references T
                // through the un-instantiated contextual return type.
                if self.type_contains_any_type_param(value_type, type_param_names) {
                    continue;
                }

                properties.push(tsz_solver::PropertyInfo::new(name_atom, value_type));
            }
            // Shorthand properties are never contextually sensitive
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
            // Method declarations: check similarly to lambda properties
            else if elem_node.kind == syntax_kind_ext::METHOD_DECLARATION
                && let Some(method) = self.ctx.arena.get_method_decl(elem_node)
            {
                let Some(name) = self.property_name_for_error(method.name) else {
                    continue;
                };
                let name_atom = self.ctx.types.intern_string(&name);

                let target_prop_type = target_props.get(&name_atom).copied().or_else(|| {
                    self.contextual_object_literal_property_type(target_param_type, &name)
                });
                let Some(target_prop_type) = target_prop_type else {
                    continue;
                };

                let target_fn_shape =
                    common::function_shape_for_type(self.ctx.types, target_prop_type);
                let Some(target_fn) = target_fn_shape else {
                    continue;
                };

                let params_are_concrete = target_fn.params.iter().all(|param| {
                    !self.type_contains_any_type_param(param.type_id, type_param_names)
                });

                if !params_are_concrete {
                    continue;
                }

                let value_type = self.speculative_type_of_function(
                    elem_idx,
                    &TypingRequest::with_contextual_type(target_prop_type),
                );

                properties.push(tsz_solver::PropertyInfo::new(name_atom, value_type));
            }
        }

        if properties.is_empty() {
            return None;
        }

        Some(self.ctx.types.factory().object_fresh(properties))
    }

    /// Extract inference from an object literal whose target type is a mapped type.
    ///
    /// For patterns like `VuexStoreOptions` where `modules` has type
    /// `{ [k in keyof Modules]: VuexStoreOptions<Modules[k], never> }` and the initializer is
    /// `{ foo: { state() {...}, mutations: {...} } }`, we need to:
    /// 1. For each property key (e.g., `foo`), extract the partial type from the property value
    /// 2. Build a partial object type from the results
    ///
    /// This enables inference from nested "thisless" functions like `state()` even when the
    /// overall object contains context-sensitive parts.
    fn extract_inference_from_mapped_type_target(
        &mut self,
        arg_idx: NodeIndex,
        mapped_id: tsz_solver::MappedTypeId,
        type_param_names: &[tsz_common::Atom],
    ) -> Option<TypeId> {
        let node = self.ctx.arena.get(arg_idx)?;
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let obj = self.ctx.arena.get_literal_expr(node)?;

        let mapped = self.ctx.types.get_mapped(mapped_id);
        let template = mapped.template;
        let type_param_name = mapped.type_param.name;

        let mut properties = Vec::new();

        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Handle property assignments
            if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                let Some(name) = self.get_property_name(prop.name) else {
                    continue;
                };
                let name_atom = self.ctx.types.intern_string(&name);

                // Create a substitution mapping the mapped type's key param to this literal key
                let key_literal = self.ctx.types.literal_string(&name);
                let subst = common::TypeSubstitution::single(type_param_name, key_literal);

                // Instantiate the template with this key
                let instantiated_template =
                    common::instantiate_type(self.ctx.types, template, &subst);

                // Try to recursively extract inference from the property value
                if let Some(nested_partial) = self
                    .extract_inference_contributing_object_type(
                        prop.initializer,
                        instantiated_template,
                        type_param_names,
                    )
                    .or_else(|| {
                        // Fallback: if we couldn't extract against the template (likely because
                        // it contains unresolved type params), try to extract non-sensitive
                        // parts directly from the nested object literal. This handles patterns
                        // like VuexStoreOptions where nested modules have "thisless" state()
                        // methods whose return types should contribute to inference.
                        self.extract_non_sensitive_object_type(prop.initializer)
                    })
                {
                    properties.push(tsz_solver::PropertyInfo::new(name_atom, nested_partial));
                }
            }
            // Handle method declarations - for mapped types, these typically aren't at this level
            // but handle them for completeness
            else if elem_node.kind == syntax_kind_ext::METHOD_DECLARATION
                && let Some(method) = self.ctx.arena.get_method_decl(elem_node) {
                    // Check if this method is "thisless" (no params, no this)
                    let has_params = !method.parameters.nodes.is_empty();
                    if has_params {
                        continue;
                    }

                    let Some(name) = self.property_name_for_error(method.name) else {
                        continue;
                    };
                    let name_atom = self.ctx.types.intern_string(&name);

                    // For thisless methods, compute the return type directly
                    let value_type =
                        self.speculative_type_of_function(elem_idx, &TypingRequest::NONE);
                    properties.push(tsz_solver::PropertyInfo::new(name_atom, value_type));
                }
        }

        if properties.is_empty() {
            return None;
        }

        Some(self.ctx.types.factory().object_fresh(properties))
    }

    /// Like `extract_inference_contributing_object_type` but for array/tuple literals.
    ///
    /// Handles patterns like:
    /// ```ts
    /// declare function callItT<T>(obj: [(n: number) => T, (x: T) => void]): void;
    /// callItT([_a => 0, n => n.toFixed()]);
    /// ```
    /// The first element `_a => 0` has concrete contextual param type `(n: number)`,
    /// so we can type it in Round 1 and use its return type to infer T.
    pub(crate) fn extract_inference_contributing_array_type(
        &mut self,
        arg_idx: NodeIndex,
        target_param_type: TypeId,
        type_param_names: &[tsz_common::Atom],
    ) -> Option<TypeId> {
        use super::complex::is_contextually_sensitive;

        let node = self.ctx.arena.get(arg_idx)?;
        if node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return None;
        }
        let arr = self.ctx.arena.get_literal_expr(node)?;

        // Get the target tuple type
        let target_type = self.evaluate_type_with_env(target_param_type);
        let target_elements = common::tuple_elements(self.ctx.types, target_type)?;

        let mut elements = Vec::new();
        let mut any_contributed = false;

        for (idx, &elem_idx) in arr.elements.nodes.iter().enumerate() {
            let target_elem_type = target_elements
                .get(idx)
                .map(|e| e.type_id)
                .unwrap_or(TypeId::ANY);

            if !is_contextually_sensitive(self, elem_idx) {
                // Non-sensitive: compute type without context
                let value_type = self.get_type_of_node_with_request(elem_idx, &TypingRequest::NONE);
                elements.push(tsz_solver::TupleElement {
                    type_id: value_type,
                    optional: false,
                    rest: false,
                    name: None,
                });
                any_contributed = true;
                continue;
            }

            // Sensitive element: check if contextual function params are concrete
            let target_fn = common::function_shape_for_type(self.ctx.types, target_elem_type)?;

            let params_are_concrete = target_fn
                .params
                .iter()
                .all(|param| !self.type_contains_any_type_param(param.type_id, type_param_names));

            if params_are_concrete {
                let value_type = self.speculative_type_of_node(
                    elem_idx,
                    &TypingRequest::with_contextual_type(target_elem_type),
                );
                elements.push(tsz_solver::TupleElement {
                    type_id: value_type,
                    optional: false,
                    rest: false,
                    name: None,
                });
                any_contributed = true;
            } else {
                // Can't contribute — use ANY as placeholder
                elements.push(tsz_solver::TupleElement {
                    type_id: TypeId::ANY,
                    optional: false,
                    rest: false,
                    name: None,
                });
            }
        }

        if !any_contributed {
            return None;
        }

        Some(self.ctx.types.factory().tuple(elements))
    }

    /// Check if a type contains any of the specified type parameter names.
    fn type_contains_any_type_param(
        &self,
        type_id: TypeId,
        type_param_names: &[tsz_common::Atom],
    ) -> bool {
        type_param_names
            .iter()
            .any(|&name| common::contains_type_parameter_named(self.ctx.types, type_id, name))
    }
}
