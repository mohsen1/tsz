//! Value declaration resolution, TDZ checking, and identifier type computation helpers.

use crate::context::TypingRequest;
use crate::query_boundaries::checkers::call::is_type_parameter_type;
use crate::query_boundaries::common;
use crate::query_boundaries::common::CallResult;
use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeArena;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

use super::call_inference::should_preserve_contextual_application_shape;

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
                } else if sym.flags & tsz_binder::symbol_flags::ENUM != 0 {
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
                    sym.flags & tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE != 0
                        && sym.flags
                            & (tsz_binder::symbol_flags::CLASS | tsz_binder::symbol_flags::ENUM)
                            == 0
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
                    false
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
                self.error_at_node(
                    access.name_or_argument,
                    &format!(
                        "Property '{}' is used before its initialization.",
                        name_ident.escaped_text
                    ),
                    tsz_common::diagnostics::diagnostic_codes::PROPERTY_IS_USED_BEFORE_ITS_INITIALIZATION,
                );
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
            return self.get_class_constructor_type(decl_idx, class_data);
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

                let mut checker = Box::new(CheckerState::with_parent_cache(
                    decl_arena,
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
            // For merged TYPE+VALUE symbols (e.g., `interface Symbol` + `declare var Symbol`),
            // get_type_of_symbol returns the interface type. In value context we need the
            // variable's declared type (e.g., SymbolConstructor). Search the symbol's
            // declarations for a variable declaration with a type annotation first.
            let lib_binders = self.get_lib_binders();
            if let Some(symbol) = self
                .ctx
                .binder
                .get_symbol_with_libs(value_sym_id, &lib_binders)
            {
                let has_type_side = (symbol.flags & tsz_binder::symbol_flags::TYPE) != 0;
                let has_value_side = (symbol.flags & tsz_binder::symbol_flags::VALUE) != 0;
                if has_type_side && has_value_side {
                    // Merged symbol: scan declarations for a variable declaration
                    for &decl_idx in &symbol.declarations {
                        if decl_idx.is_none() {
                            continue;
                        }
                        let value_type =
                            self.type_of_value_declaration_for_symbol(value_sym_id, decl_idx);
                        if value_type != TypeId::UNKNOWN && value_type != TypeId::ERROR {
                            return value_type;
                        }
                    }
                }
            }

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

impl<'a> CheckerState<'a> {
    pub(crate) fn is_unshadowed_commonjs_require_identifier(&mut self, idx: NodeIndex) -> bool {
        // JavaScript/checkJs files use CommonJS-style `require(...)` value resolution
        // even when the `module` compiler option stays at its default script mode.
        // Keep the special module-value path available there so property presence,
        // assignment compatibility, and call diagnostics all see the same module type.
        if !self.ctx.compiler_options.module.is_commonjs() && !self.is_js_file() {
            return false;
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(node) else {
            return false;
        };
        if ident.escaped_text != "require" {
            return false;
        }

        if self.is_js_file() {
            if let Some(sym_id) = self.ctx.binder.file_locals.get("require")
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && symbol.decl_file_idx == self.ctx.current_file_idx as u32
                && symbol
                    .declarations
                    .iter()
                    .any(|&decl_idx| self.ctx.arena.get(decl_idx).is_some())
            {
                return false;
            }
            return true;
        }

        let resolved_symbol = self
            .ctx
            .binder
            .node_symbols
            .get(&idx.0)
            .copied()
            .or_else(|| self.resolve_identifier_symbol(idx));
        let Some(sym_id) = resolved_symbol else {
            return true;
        };

        let lib_binders = self.get_lib_binders();
        let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
            return true;
        };

        !symbol
            .declarations
            .iter()
            .any(|decl_idx| self.ctx.binder.node_symbols.contains_key(&decl_idx.0))
    }

    pub(crate) fn normalize_contextual_call_param_type(&mut self, param_type: TypeId) -> TypeId {
        if common::is_callable_type(self.ctx.types, param_type)
            || should_preserve_contextual_application_shape(self.ctx.types, param_type)
        {
            return param_type;
        }

        if let Some(members) = common::union_members(self.ctx.types, param_type) {
            let evaluated_members: Vec<_> = members
                .iter()
                .map(|&member| {
                    if should_preserve_contextual_application_shape(self.ctx.types, member) {
                        member
                    } else {
                        self.evaluate_type_with_env(member)
                    }
                })
                .collect();
            if evaluated_members
                .iter()
                .zip(members.iter())
                .all(|(evaluated, original)| evaluated == original)
            {
                return param_type;
            }

            let reduced = self.ctx.types.union_literal_reduce(evaluated_members);
            if reduced != param_type
                && let Some(def_id) = self.ctx.definition_store.find_def_for_type(param_type)
            {
                self.ctx
                    .definition_store
                    .register_type_to_def(reduced, def_id);
            }
            return reduced;
        }

        self.evaluate_type_with_env(param_type)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn finalize_generic_call_result(
        &mut self,
        callee_type_for_call: TypeId,
        generic_instantiated_params: Option<&Vec<tsz_solver::ParamInfo>>,
        args: &[NodeIndex],
        arg_types: &[TypeId],
        result: CallResult,
        sanitized_generic_inference: bool,
        needs_real_type_recheck: bool,
        _shape_this_type: Option<TypeId>,
    ) -> (CallResult, bool) {
        if let Some(instantiated_params) = generic_instantiated_params {
            self.propagate_generic_constructor_display_defs(
                callee_type_for_call,
                args.len(),
                instantiated_params,
            );
        }

        let mut allow_contextual_mismatch_deferral = true;
        let result = if let Some(instantiated_params) = generic_instantiated_params {
            let expected_param_types = self.contextual_param_types_from_instantiated_params(
                instantiated_params,
                arg_types.len(),
            );
            let result = if sanitized_generic_inference || needs_real_type_recheck {
                self.recheck_generic_call_arguments_with_real_types(
                    result,
                    instantiated_params,
                    args,
                    arg_types,
                )
            } else {
                result
            };
            let recovered_mismatch = matches!(
                &result,
                CallResult::ArgumentTypeMismatch {
                    fallback_return,
                    ..
                } if *fallback_return != TypeId::ERROR
            );
            let (result, should_epc) = match result {
                CallResult::Success(return_type) => (CallResult::Success(return_type), true),
                CallResult::ArgumentTypeMismatch {
                    index,
                    actual,
                    expected,
                    fallback_return,
                } => {
                    if let Some(param) = instantiated_params.get(index).or_else(|| {
                        let last = instantiated_params.last()?;
                        last.rest.then_some(last)
                    }) {
                        let evaluated_param = self.evaluate_type_with_env(param.type_id);
                        let expected_param = expected_param_types
                            .get(index)
                            .copied()
                            .flatten()
                            .unwrap_or_else(|| {
                                if param.rest {
                                    self.rest_argument_element_type_with_env(evaluated_param)
                                } else {
                                    evaluated_param
                                }
                            });
                        let arg_type = args
                            .get(index)
                            .copied()
                            .map(|arg_idx| {
                                self.refreshed_generic_call_arg_type_with_context(
                                    arg_idx,
                                    arg_types.get(index).copied().unwrap_or(TypeId::UNKNOWN),
                                    Some(expected_param),
                                )
                            })
                            .unwrap_or(TypeId::UNKNOWN);
                        let fresh_assignable = self
                            .is_assignable_to_with_env(arg_type, expected_param)
                            || self
                                .is_assignable_via_contextual_signatures(arg_type, expected_param);
                        let excess_property_recovery = if !fresh_assignable {
                            args.get(index)
                                .copied()
                                .filter(|&arg_idx| {
                                    self.ctx.arena.get(arg_idx).is_some_and(|arg_node| {
                                        arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                    })
                                })
                                .is_some_and(|arg_idx| {
                                    if self
                                        .ctx
                                        .generic_excess_skip
                                        .as_ref()
                                        .is_some_and(|skip| index < skip.len() && skip[index])
                                    {
                                        return false;
                                    }
                                    if is_type_parameter_type(self.ctx.types, expected_param) {
                                        return false;
                                    }
                                    if self.contextual_type_is_unresolved_for_argument_refresh(
                                        expected_param,
                                    ) {
                                        return false;
                                    }
                                    let excess_snap = self.ctx.snapshot_diagnostics();
                                    self.check_object_literal_excess_properties(
                                        arg_type,
                                        expected_param,
                                        arg_idx,
                                    );
                                    self.ctx.has_speculative_diagnostics(&excess_snap)
                                })
                        } else {
                            false
                        };
                        if !fresh_assignable && !excess_property_recovery {
                            allow_contextual_mismatch_deferral = false;
                        }
                        (
                            CallResult::ArgumentTypeMismatch {
                                index,
                                expected: expected_param,
                                actual: arg_type,
                                fallback_return,
                            },
                            fresh_assignable || excess_property_recovery,
                        )
                    } else {
                        (
                            CallResult::ArgumentTypeMismatch {
                                index,
                                actual,
                                expected,
                                fallback_return,
                            },
                            false,
                        )
                    }
                }
                other => (other, false),
            };
            if should_epc {
                for (i, &arg_idx) in args.iter().enumerate() {
                    if let Some(arg_node) = self.ctx.arena.get(arg_idx)
                        && arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        && let Some(param) = instantiated_params.get(i)
                        && param.type_id != TypeId::ANY
                        && param.type_id != TypeId::UNKNOWN
                    {
                        if self
                            .ctx
                            .generic_excess_skip
                            .as_ref()
                            .is_some_and(|skip| i < skip.len() && skip[i])
                        {
                            continue;
                        }
                        let evaluated_param = self.evaluate_type_with_env(param.type_id);
                        if !is_type_parameter_type(self.ctx.types, evaluated_param)
                            && !self
                                .contextual_type_is_unresolved_for_argument_refresh(evaluated_param)
                        {
                            // Use the unevaluated parameter type as the contextual
                            // type for object literal refresh so that ThisType<T>
                            // markers inside intersection type aliases (e.g.,
                            // `Props & ThisType<Instance>`) are preserved. Evaluating
                            // the intersection can strip the ThisType marker, causing
                            // false TS2339 when `this` is used in method bodies.
                            let arg_type = self.refreshed_generic_call_arg_type_with_context(
                                arg_idx,
                                arg_types.get(i).copied().unwrap_or(TypeId::UNKNOWN),
                                Some(param.type_id),
                            );
                            self.check_object_literal_excess_properties(
                                arg_type,
                                evaluated_param,
                                arg_idx,
                            );
                        }
                    }
                }
                if recovered_mismatch {
                    if let CallResult::ArgumentTypeMismatch {
                        fallback_return, ..
                    } = &result
                    {
                        CallResult::Success(*fallback_return)
                    } else {
                        result
                    }
                } else {
                    result
                }
            } else {
                result
            }
        } else {
            result
        };

        (result, allow_contextual_mismatch_deferral)
    }

    pub(crate) fn try_emit_ts2339_for_missing_this_property(
        &mut self,
        callee_expr: NodeIndex,
    ) -> bool {
        if self.ctx.enclosing_class.is_none() {
            return false;
        }

        let Some(callee_node) = self.ctx.arena.get(callee_expr) else {
            return false;
        };
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(callee_node) else {
            return false;
        };

        let Some(expr_node) = self.ctx.arena.get(access.expression) else {
            return false;
        };
        if expr_node.kind != tsz_scanner::SyntaxKind::ThisKeyword as u16 {
            return false;
        }

        let Some(property_name) = self.get_property_name(access.name_or_argument) else {
            return false;
        };

        let this_type = self.get_type_of_node(access.expression);

        // When `this` resolves to ANY (common in static methods where the constructor
        // type isn't fully resolved), fall back to checking the class symbol's member
        // table directly via the binder. If the property exists as a class member,
        // suppress TS2347 — the call target is typed, not genuinely untyped.
        if this_type == TypeId::ANY || this_type == TypeId::ERROR {
            if let Some(ref class_info) = self.ctx.enclosing_class.clone()
                && let Some(&class_sym) = self.ctx.binder.node_symbols.get(&class_info.class_idx.0)
                && let Some(class_symbol) = self.ctx.binder.get_symbol(class_sym)
            {
                let found_in_exports = class_symbol
                    .exports
                    .as_ref()
                    .and_then(|e| e.get(&property_name))
                    .is_some();
                let found_in_members = class_symbol
                    .members
                    .as_ref()
                    .and_then(|m| m.get(&property_name))
                    .is_some();
                if found_in_exports || found_in_members {
                    return true; // suppress TS2347
                }
            }
            return false;
        }

        let result = self.resolve_property_access_with_env(this_type, &property_name);
        match result {
            crate::query_boundaries::common::PropertyAccessResult::PropertyNotFound { .. } => {
                self.error_property_not_exist_at(
                    &property_name,
                    this_type,
                    access.name_or_argument,
                );
                true
            }
            crate::query_boundaries::common::PropertyAccessResult::Success { type_id, .. }
                if type_id == TypeId::ANY =>
            {
                // Property exists but is explicitly typed as `any` — the call target
                // is genuinely untyped. Do NOT suppress TS2347.
                // e.g., `private foo: any; this.foo<string>()` should emit TS2347.
                false
            }
            _ => {
                // Property exists on a concrete `this` type — the callee resolved to ANY
                // due to generic instantiation limitations, not because it's genuinely untyped.
                // Suppress TS2347 (e.g., `this.one<T>(...)` in static generic methods).
                true
            }
        }
    }

    /// Suppress TS2347 for `this.property<T>(...)` inside a class.
    /// When an enclosing class exists and the property is a known member that is NOT
    /// explicitly typed as `any`, suppress — the callee is typed, ANY came from
    /// resolution limitations. When the property is explicitly `any`, do NOT suppress
    /// because the call target is genuinely untyped.
    pub(crate) fn is_this_property_access_in_class_context(
        &mut self,
        callee_expr: NodeIndex,
    ) -> bool {
        let Some(callee_node) = self.ctx.arena.get(callee_expr) else {
            return false;
        };
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(callee_node) else {
            return false;
        };
        let Some(expr_node) = self.ctx.arena.get(access.expression) else {
            return false;
        };
        if expr_node.kind != tsz_scanner::SyntaxKind::ThisKeyword as u16 {
            return false;
        }

        if self.nearest_enclosing_class(callee_expr).is_none() {
            return false;
        }

        // Check if the property resolves to `any` — if so, the call target is genuinely
        // untyped and TS2347 should fire.
        let this_type = self.get_type_of_node(access.expression);
        if this_type != TypeId::ANY
            && this_type != TypeId::ERROR
            && let Some(property_name) = self.get_property_name(access.name_or_argument)
        {
            let result = self.resolve_property_access_with_env(this_type, &property_name);
            if let crate::query_boundaries::common::PropertyAccessResult::Success {
                type_id, ..
            } = result
                && type_id == TypeId::ANY
            {
                return false; // genuinely `any` — don't suppress TS2347
            }
        }

        true
    }
}
