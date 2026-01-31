//! Declaration & Statement Checking Module
//!
//! Extracted from state.rs: Methods for checking source files, declarations,
//! statements, and class/interface validation. Also includes StatementCheckCallbacks.

use crate::binder::symbol_flags;
use crate::checker::EnclosingClassInfo;
use crate::checker::flow_analysis::{ComputedKey, PropertyKey};
use crate::checker::state::CheckerState;
use crate::checker::statements::StatementChecker;
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use crate::solver::TypeId;
use rustc_hash::FxHashSet;
use tracing::{Level, span};

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Source File Checking (Full Traversal)
    // =========================================================================

    /// Check a source file and populate diagnostics (main entry point).
    ///
    /// This is the primary entry point for type checking after parsing and binding.
    /// It traverses the entire AST and performs all type checking operations.
    ///
    /// ## Checking Process:
    /// 1. Initializes the type environment
    /// 2. Traverses all top-level declarations
    /// 3. Checks all statements and expressions
    /// 4. Populates diagnostics with errors and warnings
    ///
    /// ## What Gets Checked:
    /// - Type annotations
    /// - Assignments (variable, property, return)
    /// - Function calls
    /// - Property access
    /// - Type compatibility (extends, implements)
    /// - Flow analysis (definite assignment, type narrowing)
    /// - Generic constraints
    /// - And much more...
    ///
    /// ## Diagnostics:
    /// - Errors are added to `ctx.diagnostics`
    /// - Includes error codes (TSxxxx) and messages
    /// - Spans point to the problematic code
    ///
    /// ## Compilation Flow:
    /// 1. **Parser**: Source code → AST
    /// 2. **Binder**: AST → Symbols (scopes, declarations)
    /// 3. **Checker** (this function): AST + Symbols → Types + Diagnostics
    ///
    /// ## TypeScript Example:
    /// ```typescript
    /// // File: example.ts
    /// let x: string = 42;  // Type error: number not assignable to string
    ///
    /// function foo(a: number): string {
    ///   return a;  // Type error: number not assignable to string
    /// }
    ///
    /// interface User {
    ///   name: string;
    /// }
    /// const user: User = { age: 25 };  // Type error: missing 'name' property
    ///
    /// // check_source_file() would find all three errors above
    /// ```
    pub fn check_source_file(&mut self, root_idx: NodeIndex) {
        let _span = span!(Level::INFO, "check_source_file", idx = ?root_idx).entered();

        let Some(node) = self.ctx.arena.get(root_idx) else {
            return;
        };

        if let Some(sf) = self.ctx.arena.get_source_file(node) {
            self.ctx.compiler_options.no_implicit_any =
                self.resolve_no_implicit_any_from_source(&sf.text);
            self.ctx.compiler_options.no_implicit_returns =
                self.resolve_no_implicit_returns_from_source(&sf.text);
            self.ctx.compiler_options.use_unknown_in_catch_variables =
                self.resolve_use_unknown_in_catch_variables_from_source(&sf.text);
            self.ctx.compiler_options.no_implicit_this =
                self.resolve_no_implicit_this_from_source(&sf.text);
            self.ctx.compiler_options.strict_property_initialization =
                self.resolve_strict_property_initialization_from_source(&sf.text);
            self.ctx.compiler_options.strict_null_checks =
                self.resolve_strict_null_checks_from_source(&sf.text);
            self.ctx.compiler_options.strict_function_types =
                self.resolve_strict_function_types_from_source(&sf.text);
            self.ctx.compiler_options.allow_unreachable_code =
                self.resolve_allow_unreachable_code_from_source(&sf.text);

            // Register boxed types (String, Number, Boolean, etc.) from lib.d.ts
            // This enables primitive property access to use lib definitions instead of hardcoded lists
            self.register_boxed_types();

            // CRITICAL FIX: Build TypeEnvironment with all symbols (including lib symbols)
            // This ensures Error, Math, JSON, etc. interfaces are registered for property resolution
            // Without this, TypeKey::Ref(Error) returns ERROR, causing TS2339 false positives
            let populated_env = self.build_type_environment();
            *self.ctx.type_env.borrow_mut() = populated_env;

            // Type check each top-level statement
            for &stmt_idx in &sf.statements.nodes {
                self.check_statement(stmt_idx);
            }

            // Check for function overload implementations (2389, 2391)
            self.check_function_implementations(&sf.statements.nodes);

            // Check for export assignment with other exports (2309)
            self.check_export_assignment(&sf.statements.nodes);

            // Check for duplicate identifiers (2300)
            self.check_duplicate_identifiers();

            // Check for missing global types (2318)
            // Emits errors at file start for essential types when libs are not loaded
            self.check_missing_global_types();

            // Check for unused declarations (6133)
            // Only check for unused declarations when no_implicit_any is enabled (strict mode)
            // This prevents test files from reporting unused variable errors when they're testing specific behaviors
            if self.ctx.no_implicit_any() {
                self.check_unused_declarations();
            }
        }
    }

    pub(crate) fn declaration_symbol_flags(&self, decl_idx: NodeIndex) -> Option<u32> {
        use crate::parser::node_flags;

        let decl_idx = self.resolve_duplicate_decl_node(decl_idx)?;
        let node = self.ctx.arena.get(decl_idx)?;

        match node.kind {
            syntax_kind_ext::VARIABLE_DECLARATION => {
                let mut decl_flags = node.flags as u32;
                if (decl_flags & (node_flags::LET | node_flags::CONST)) == 0
                    && let Some(parent) =
                        self.ctx.arena.get_extended(decl_idx).map(|ext| ext.parent)
                    && let Some(parent_node) = self.ctx.arena.get(parent)
                    && parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                {
                    decl_flags |= parent_node.flags as u32;
                }
                if (decl_flags & (node_flags::LET | node_flags::CONST)) != 0 {
                    Some(symbol_flags::BLOCK_SCOPED_VARIABLE)
                } else {
                    Some(symbol_flags::FUNCTION_SCOPED_VARIABLE)
                }
            }
            syntax_kind_ext::FUNCTION_DECLARATION => Some(symbol_flags::FUNCTION),
            syntax_kind_ext::CLASS_DECLARATION => Some(symbol_flags::CLASS),
            syntax_kind_ext::INTERFACE_DECLARATION => Some(symbol_flags::INTERFACE),
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => Some(symbol_flags::TYPE_ALIAS),
            syntax_kind_ext::ENUM_DECLARATION => {
                // Check if this is a const enum by looking for const modifier
                let is_const_enum = self
                    .ctx
                    .arena
                    .get_enum(node)
                    .and_then(|enum_decl| enum_decl.modifiers.as_ref())
                    .map(|modifiers| {
                        modifiers.nodes.iter().any(|&mod_idx| {
                            self.ctx.arena.get(mod_idx).is_some_and(|mod_node| {
                                mod_node.kind == crate::scanner::SyntaxKind::ConstKeyword as u16
                            })
                        })
                    })
                    .unwrap_or(false);
                if is_const_enum {
                    Some(symbol_flags::CONST_ENUM)
                } else {
                    Some(symbol_flags::REGULAR_ENUM)
                }
            }
            syntax_kind_ext::MODULE_DECLARATION => {
                // Namespaces (module declarations) can merge with functions, classes, enums
                Some(symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE)
            }
            syntax_kind_ext::GET_ACCESSOR => {
                let mut flags = symbol_flags::GET_ACCESSOR;
                if let Some(accessor) = self.ctx.arena.get_accessor(node)
                    && self.has_static_modifier(&accessor.modifiers)
                {
                    flags |= symbol_flags::STATIC;
                }
                Some(flags)
            }
            syntax_kind_ext::SET_ACCESSOR => {
                let mut flags = symbol_flags::SET_ACCESSOR;
                if let Some(accessor) = self.ctx.arena.get_accessor(node)
                    && self.has_static_modifier(&accessor.modifiers)
                {
                    flags |= symbol_flags::STATIC;
                }
                Some(flags)
            }
            syntax_kind_ext::METHOD_DECLARATION => {
                let mut flags = symbol_flags::METHOD;
                if let Some(method) = self.ctx.arena.get_method_decl(node)
                    && self.has_static_modifier(&method.modifiers)
                {
                    flags |= symbol_flags::STATIC;
                }
                Some(flags)
            }
            syntax_kind_ext::PROPERTY_DECLARATION => {
                let mut flags = symbol_flags::PROPERTY;
                if let Some(prop) = self.ctx.arena.get_property_decl(node)
                    && self.has_static_modifier(&prop.modifiers)
                {
                    flags |= symbol_flags::STATIC;
                }
                Some(flags)
            }
            syntax_kind_ext::CONSTRUCTOR => Some(symbol_flags::CONSTRUCTOR),
            _ => None,
        }
    }

    /// Check for duplicate parameter names in a parameter list (TS2300).
    /// Check a statement and produce type errors.
    ///
    /// This method delegates to StatementChecker for dispatching logic,
    /// while providing actual implementations via the StatementCheckCallbacks trait.
    pub(crate) fn check_statement(&mut self, stmt_idx: NodeIndex) {
        StatementChecker::check(stmt_idx, self);
    }

    /// Check a variable statement (var/let/const declarations).
    // ============================================================================
    // Iterable/Iterator Type Checking Methods
    // ============================================================================
    // The following methods have been extracted to src/checker/iterable_checker.rs:
    // - is_iterable_type
    // - is_async_iterable_type
    // - for_of_element_type
    // - check_for_of_iterability
    // - check_spread_iterability
    //
    // These methods are now provided via a separate impl block in iterable_checker.rs
    // as part of Phase 2 architecture refactoring to break up the state.rs god object.
    // ============================================================================

    /// Assign the inferred loop-variable type for `for-in` / `for-of` initializers.
    ///
    /// The initializer is a `VariableDeclarationList` in the Thin AST.
    pub(crate) fn assign_for_in_of_initializer_types(
        &mut self,
        decl_list_idx: NodeIndex,
        element_type: TypeId,
    ) {
        let Some(list_node) = self.ctx.arena.get(decl_list_idx) else {
            return;
        };
        let Some(list) = self.ctx.arena.get_variable(list_node) else {
            return;
        };

        for &decl_idx in &list.declarations.nodes {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                continue;
            };

            // If there's a type annotation, check that the element type is assignable to it
            if !var_decl.type_annotation.is_none() {
                let declared = self.get_type_from_type_node(var_decl.type_annotation);

                // TS2322: Check that element type is assignable to declared type
                if declared != TypeId::ANY
                    && !self.type_contains_error(declared)
                    && !self.is_assignable_to(element_type, declared)
                    && !self.should_skip_weak_union_error(element_type, declared, var_decl.name)
                {
                    self.error_type_not_assignable_with_reason_at(
                        element_type,
                        declared,
                        var_decl.name,
                    );
                }

                // Assign types for binding patterns (e.g., `for (const [a] of arr)`).
                if let Some(name_node) = self.ctx.arena.get(var_decl.name)
                    && (name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
                {
                    self.assign_binding_pattern_symbol_types(var_decl.name, declared);
                }

                if let Some(sym_id) = self.ctx.binder.get_node_symbol(decl_idx) {
                    self.cache_symbol_type(sym_id, declared);
                } else if let Some(sym_id) = self.ctx.binder.get_node_symbol(var_decl.name) {
                    self.cache_symbol_type(sym_id, declared);
                }
            } else {
                // No type annotation - use element type
                // Assign types for binding patterns (e.g., `for (const [a] of arr)`).
                if let Some(name_node) = self.ctx.arena.get(var_decl.name)
                    && (name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
                {
                    self.assign_binding_pattern_symbol_types(var_decl.name, element_type);
                }

                if let Some(sym_id) = self.ctx.binder.get_node_symbol(decl_idx) {
                    self.cache_symbol_type(sym_id, element_type);
                } else if let Some(sym_id) = self.ctx.binder.get_node_symbol(var_decl.name) {
                    self.cache_symbol_type(sym_id, element_type);
                }
            }
        }
    }

    /// Check a single variable declaration.
    pub(crate) fn check_variable_declaration(&mut self, decl_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return;
        };

        let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) else {
            return;
        };

        // Check if this is a destructuring pattern (object/array binding)
        let is_destructuring = if let Some(name_node) = self.ctx.arena.get(var_decl.name) {
            name_node.kind != SyntaxKind::Identifier as u16
        } else {
            false
        };

        // Get the variable name for adding to local scope
        let var_name = if !is_destructuring {
            if let Some(name_node) = self.ctx.arena.get(var_decl.name) {
                self.ctx
                    .arena
                    .get_identifier(name_node)
                    .map(|ident| ident.escaped_text.clone())
            } else {
                None
            }
        } else {
            None
        };

        let is_catch_variable = self.is_catch_clause_variable_declaration(decl_idx);

        let compute_final_type = |checker: &mut CheckerState| -> TypeId {
            let mut has_type_annotation = !var_decl.type_annotation.is_none();
            let mut declared_type = if has_type_annotation {
                checker.get_type_from_type_node(var_decl.type_annotation)
            } else if is_catch_variable && checker.ctx.use_unknown_in_catch_variables() {
                TypeId::UNKNOWN
            } else {
                TypeId::ANY
            };
            if !has_type_annotation
                && let Some(jsdoc_type) = checker.jsdoc_type_annotation_for_node(decl_idx)
            {
                declared_type = jsdoc_type;
                has_type_annotation = true;
            }

            // If there's a type annotation, that determines the type (even for 'any')
            if has_type_annotation {
                if !var_decl.initializer.is_none() {
                    // Set contextual type for the initializer (but not for 'any')
                    let prev_context = checker.ctx.contextual_type;
                    if declared_type != TypeId::ANY {
                        checker.ctx.contextual_type = Some(declared_type);
                    }
                    let init_type = checker.get_type_of_node(var_decl.initializer);
                    checker.ctx.contextual_type = prev_context;

                    // Check assignability (skip for 'any' since anything is assignable to any)
                    // This includes strict null checks - null/undefined should NOT be assignable to non-nullable types
                    if declared_type != TypeId::ANY && !checker.type_contains_error(declared_type) {
                        if let Some((source_level, target_level)) =
                            checker.constructor_accessibility_mismatch_for_var_decl(var_decl)
                        {
                            checker.error_constructor_accessibility_not_assignable(
                                init_type,
                                declared_type,
                                source_level,
                                target_level,
                                var_decl.initializer,
                            );
                        } else if !checker.is_assignable_to(init_type, declared_type)
                            && !checker.should_skip_weak_union_error(
                                init_type,
                                declared_type,
                                var_decl.initializer,
                            )
                        {
                            // For destructuring patterns, emit a generic TS2322 error
                            // instead of detailed property mismatch errors (TS2326)
                            if is_destructuring {
                                checker.error_type_not_assignable_generic_at(
                                    init_type,
                                    declared_type,
                                    var_decl.initializer,
                                );
                            } else {
                                checker.error_type_not_assignable_with_reason_at(
                                    init_type,
                                    declared_type,
                                    var_decl.initializer,
                                );
                            }
                        }

                        // For object literals, check excess properties BEFORE removing freshness
                        // Object literals are "fresh" when first created and subject to excess property checks
                        if let Some(init_node) = checker.ctx.arena.get(var_decl.initializer)
                            && init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        {
                            checker.check_object_literal_excess_properties(
                                init_type,
                                declared_type,
                                var_decl.initializer,
                            );
                        }
                    }

                    // Remove freshness AFTER excess property check
                    // Object literals lose freshness when assigned, allowing width subtyping thereafter
                    checker.ctx.freshness_tracker.remove_freshness(init_type);
                }
                // Type annotation determines the final type
                return declared_type;
            }

            // No type annotation - infer from initializer
            if !var_decl.initializer.is_none() {
                let init_type = checker.get_type_of_node(var_decl.initializer);

                // Remove freshness from the initializer type since it's being assigned to a variable
                checker.ctx.freshness_tracker.remove_freshness(init_type);

                if let Some(literal_type) =
                    checker.literal_type_from_initializer(var_decl.initializer)
                {
                    if checker.is_const_variable_declaration(decl_idx) {
                        return literal_type;
                    }
                    return checker.widen_literal_type(literal_type);
                }
                init_type
            } else {
                declared_type
            }
        };

        if let Some(sym_id) = self.ctx.binder.get_node_symbol(decl_idx) {
            self.push_symbol_dependency(sym_id, true);
            let mut final_type = compute_final_type(self);
            self.pop_symbol_dependency();

            // Variables without an initializer/annotation can still get a contextual type in some
            // constructs (notably `for-in` / `for-of` initializers). In those cases, the symbol
            // type may already be cached from the contextual typing logic; prefer that over the
            // default `any` so we match tsc and avoid spurious noImplicitAny errors.
            if var_decl.type_annotation.is_none()
                && var_decl.initializer.is_none()
                && final_type == TypeId::ANY
                && let Some(inferred) = self.ctx.symbol_types.get(&sym_id).copied()
                && inferred != TypeId::ERROR
            {
                final_type = inferred;
            }

            // TS7005: Variable implicitly has an 'any' type
            // Report this error when noImplicitAny is enabled and the variable has no type annotation
            // and the inferred type is 'any'
            // Skip destructuring patterns - TypeScript doesn't emit TS7005 for them
            // because binding elements with default values can infer their types
            if self.ctx.no_implicit_any()
                && var_decl.type_annotation.is_none()
                && var_decl.initializer.is_none()
                && final_type == TypeId::ANY
                && !self.ctx.symbol_types.contains_key(&sym_id)
            {
                // Check if the variable name is a destructuring pattern
                let is_destructuring_pattern =
                    self.ctx.arena.get(var_decl.name).is_some_and(|name_node| {
                        name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                            || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                    });

                if !is_destructuring_pattern && let Some(ref name) = var_name {
                    use crate::checker::types::diagnostics::{
                        diagnostic_codes, diagnostic_messages, format_message,
                    };
                    let message =
                        format_message(diagnostic_messages::VARIABLE_IMPLICIT_ANY, &[name, "any"]);
                    self.error_at_node(var_decl.name, &message, diagnostic_codes::IMPLICIT_ANY);
                }
            }

            // Check for variable redeclaration in the current scope (TS2403).
            // Note: This applies specifically to 'var' merging where types must match.
            // let/const duplicates are caught earlier by the binder (TS2451).
            // Skip TS2403 for mergeable declarations (namespace, enum, class, interface, function overloads).
            if let Some(prev_type) = self.ctx.var_decl_types.get(&sym_id).copied() {
                // Check if this is a mergeable declaration by looking at the node kind.
                // Mergeable declarations: namespace/module, enum, class, interface, function.
                // When these are declared with the same name, they merge instead of conflicting.
                let is_mergeable_declaration = if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                {
                    matches!(
                        decl_node.kind,
                        syntax_kind_ext::MODULE_DECLARATION  // namespace/module
                            | syntax_kind_ext::ENUM_DECLARATION // enum
                            | syntax_kind_ext::CLASS_DECLARATION // class
                            | syntax_kind_ext::INTERFACE_DECLARATION // interface
                            | syntax_kind_ext::FUNCTION_DECLARATION // function
                    )
                } else {
                    false
                };

                if !is_mergeable_declaration
                    && !self.are_var_decl_types_compatible(prev_type, final_type)
                {
                    if let Some(ref name) = var_name {
                        self.error_subsequent_variable_declaration(
                            name, prev_type, final_type, decl_idx,
                        );
                    }
                } else {
                    let refined = self.refine_var_decl_type(prev_type, final_type);
                    if refined != prev_type {
                        self.ctx.var_decl_types.insert(sym_id, refined);
                    }
                }
            } else {
                self.ctx.var_decl_types.insert(sym_id, final_type);
            }

            if !self.ctx.symbol_types.contains_key(&sym_id) {
                self.cache_symbol_type(sym_id, final_type);
            }
        } else {
            compute_final_type(self);
        }

        // If the variable name is a binding pattern, check binding element default values
        if let Some(name_node) = self.ctx.arena.get(var_decl.name)
            && (name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
        {
            // Prefer explicit type annotation; otherwise infer from initializer (matching tsc).
            // This type is used for both default-value checking and for assigning types to
            // binding element symbols created by the binder.
            let pattern_type = if !var_decl.type_annotation.is_none() {
                self.get_type_from_type_node(var_decl.type_annotation)
            } else if !var_decl.initializer.is_none() {
                self.get_type_of_node(var_decl.initializer)
            } else if is_catch_variable && self.ctx.use_unknown_in_catch_variables() {
                TypeId::UNKNOWN
            } else {
                TypeId::ANY
            };

            // TS2488: Check array destructuring for iterability before assigning types
            if name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
                self.check_destructuring_iterability(
                    var_decl.name,
                    pattern_type,
                    var_decl.initializer,
                );
            }

            // Ensure binding element identifiers get the correct inferred types.
            self.assign_binding_pattern_symbol_types(var_decl.name, pattern_type);
            self.check_binding_pattern(var_decl.name, pattern_type);
        }
    }

    /// Check binding pattern elements and their default values for type correctness.
    ///
    /// This function traverses a binding pattern (object or array destructuring) and verifies
    /// that any default values provided in binding elements are assignable to their expected types.
    /// Assign inferred types to binding element symbols (destructuring).
    ///
    /// The binder creates symbols for identifiers inside binding patterns (e.g., `const [x] = arr;`),
    /// but their `value_declaration` is the identifier node, not the enclosing variable declaration.
    /// We infer the binding element type from the destructured value type and cache it on the symbol.
    pub(crate) fn assign_binding_pattern_symbol_types(
        &mut self,
        pattern_idx: NodeIndex,
        parent_type: TypeId,
    ) {
        let Some(pattern_node) = self.ctx.arena.get(pattern_idx) else {
            return;
        };
        let Some(pattern_data) = self.ctx.arena.get_binding_pattern(pattern_node) else {
            return;
        };

        let pattern_kind = pattern_node.kind;
        for (i, &element_idx) in pattern_data.elements.nodes.iter().enumerate() {
            if element_idx.is_none() {
                continue;
            }

            let Some(element_node) = self.ctx.arena.get(element_idx) else {
                continue;
            };
            if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                continue;
            }

            let Some(element_data) = self.ctx.arena.get_binding_element(element_node) else {
                continue;
            };

            let element_type = if parent_type == TypeId::ANY {
                TypeId::ANY
            } else {
                self.get_binding_element_type(pattern_kind, i, parent_type, element_data)
            };

            let Some(name_node) = self.ctx.arena.get(element_data.name) else {
                continue;
            };

            // Identifier binding: cache the inferred type on the symbol.
            if name_node.kind == SyntaxKind::Identifier as u16
                && let Some(sym_id) = self.ctx.binder.get_node_symbol(element_data.name)
            {
                self.cache_symbol_type(sym_id, element_type);
            }

            // Nested binding patterns: check iterability for array patterns, then recurse
            if name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
                // Check iterability for nested array destructuring
                self.check_destructuring_iterability(
                    element_data.name,
                    element_type,
                    NodeIndex::NONE,
                );
                self.assign_binding_pattern_symbol_types(element_data.name, element_type);
            } else if name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
                self.assign_binding_pattern_symbol_types(element_data.name, element_type);
            }
        }
    }

    /// Get the expected type for a binding element from its parent type.
    pub(crate) fn get_binding_element_type(
        &mut self,
        pattern_kind: u16,
        element_index: usize,
        parent_type: TypeId,
        element_data: &crate::parser::node::BindingElementData,
    ) -> TypeId {
        use crate::solver::type_queries::{
            get_array_element_type, get_object_shape, get_tuple_elements, unwrap_readonly_deep,
        };

        // Array binding patterns use the element position.
        if pattern_kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
            if parent_type == TypeId::UNKNOWN || parent_type == TypeId::ERROR {
                return parent_type;
            }

            // Unwrap readonly wrappers for destructuring element access
            let array_like = unwrap_readonly_deep(self.ctx.types, parent_type);

            // Rest element: ...rest
            if element_data.dot_dot_dot_token {
                let elem_type =
                    if let Some(elem) = get_array_element_type(self.ctx.types, array_like) {
                        elem
                    } else if let Some(elems) = get_tuple_elements(self.ctx.types, array_like) {
                        // Best-effort: if the tuple has a rest element, use it; otherwise, fall back to last.
                        elems
                            .iter()
                            .find(|e| e.rest)
                            .or_else(|| elems.last())
                            .map(|e| e.type_id)
                            .unwrap_or(TypeId::ANY)
                    } else {
                        TypeId::ANY
                    };
                return self.ctx.types.array(elem_type);
            }

            return if let Some(elem) = get_array_element_type(self.ctx.types, array_like) {
                elem
            } else if let Some(elems) = get_tuple_elements(self.ctx.types, array_like) {
                elems
                    .get(element_index)
                    .map(|e| e.type_id)
                    .unwrap_or(TypeId::ANY)
            } else {
                TypeId::ANY
            };
        }

        // Get the property name or index
        let property_name = if !element_data.property_name.is_none() {
            // { x: a } - property_name is "x"
            if let Some(prop_node) = self.ctx.arena.get(element_data.property_name) {
                self.ctx
                    .arena
                    .get_identifier(prop_node)
                    .map(|ident| ident.escaped_text.clone())
            } else {
                None
            }
        } else {
            // { x } - the name itself is the property name
            if let Some(name_node) = self.ctx.arena.get(element_data.name) {
                self.ctx
                    .arena
                    .get_identifier(name_node)
                    .map(|ident| ident.escaped_text.clone())
            } else {
                None
            }
        };

        if parent_type == TypeId::UNKNOWN {
            if let Some(prop_name_str) = property_name.as_deref() {
                let error_node = if !element_data.property_name.is_none() {
                    element_data.property_name
                } else if !element_data.name.is_none() {
                    element_data.name
                } else {
                    NodeIndex::NONE
                };
                self.error_property_not_exist_at(prop_name_str, parent_type, error_node);
            }
            return TypeId::UNKNOWN;
        }

        if let Some(prop_name_str) = property_name {
            // Look up the property type in the parent type
            if let Some(shape) = get_object_shape(self.ctx.types, parent_type) {
                // Find the property by comparing names
                for prop in shape.properties.as_slice() {
                    if self.ctx.types.resolve_atom_ref(prop.name).as_ref() == prop_name_str {
                        return prop.type_id;
                    }
                }
                TypeId::ANY
            } else {
                TypeId::ANY
            }
        } else {
            TypeId::ANY
        }
    }

    /// Check object literal assignment for excess properties.
    ///
    /// **Note**: This check is specific to object literals and is NOT part of general
    /// structural subtyping. Excess properties in object literals are errors, but
    /// when assigning from a variable with extra properties, it's allowed.
    /// See https://github.com/microsoft/TypeScript/issues/13813,
    /// https://github.com/microsoft/TypeScript/issues/18075,
    /// https://github.com/microsoft/TypeScript/issues/28616.
    ///
    /// Missing property errors are handled by the solver's `explain_failure` API
    /// via `error_type_not_assignable_with_reason_at`, so we only check excess
    /// properties here to avoid duplication.
    pub(crate) fn check_object_literal_excess_properties(
        &mut self,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
    ) {
        use crate::solver::type_queries;

        // Only check excess properties for FRESH object literals
        // This is the key TypeScript behavior:
        // - const p: Point = {x: 1, y: 2, z: 3}  // ERROR: 'z' is excess (fresh)
        // - const obj = {x: 1, y: 2, z: 3}; p = obj;  // OK: obj loses freshness
        if !self
            .ctx
            .freshness_tracker
            .should_check_excess_properties(source)
        {
            return;
        }

        // Get the properties of source type using type_queries
        let Some(source_shape) = type_queries::get_object_shape(self.ctx.types, source) else {
            return;
        };

        let source_props = source_shape.properties.as_slice();
        let resolved_target = self.resolve_type_for_property_access(target);

        // Handle union targets first using type_queries
        if let Some(members) = type_queries::get_union_members(self.ctx.types, resolved_target) {
            let mut target_shapes = Vec::new();

            for &member in members.iter() {
                let resolved_member = self.resolve_type_for_property_access(member);
                let Some(shape) = type_queries::get_object_shape(self.ctx.types, resolved_member)
                else {
                    continue;
                };

                if shape.properties.is_empty()
                    || shape.string_index.is_some()
                    || shape.number_index.is_some()
                {
                    return;
                }

                target_shapes.push(shape);
            }

            if target_shapes.is_empty() {
                return;
            }

            for source_prop in source_props {
                // For unions, check if property exists in ANY member
                let target_prop_types: Vec<TypeId> = target_shapes
                    .iter()
                    .filter_map(|shape| {
                        shape
                            .properties
                            .iter()
                            .find(|prop| prop.name == source_prop.name)
                            .map(|prop| prop.type_id)
                    })
                    .collect();

                if target_prop_types.is_empty() {
                    let prop_name = self.ctx.types.resolve_atom(source_prop.name);
                    self.error_excess_property_at(&prop_name, target, idx);
                } else if self
                    .ctx
                    .freshness_tracker
                    .should_check_excess_properties(source_prop.type_id)
                {
                    // Property exists in target - check nested object literals (Rule #4)
                    // For unions, create a union of the matching property types
                    let target_prop_type = if target_prop_types.len() == 1 {
                        target_prop_types[0]
                    } else {
                        self.ctx.types.union(target_prop_types)
                    };
                    self.check_object_literal_excess_properties(
                        source_prop.type_id,
                        target_prop_type,
                        idx,
                    );
                }
            }
            return;
        }

        // Handle object targets using type_queries
        if let Some(target_shape) = type_queries::get_object_shape(self.ctx.types, resolved_target)
        {
            let target_props = target_shape.properties.as_slice();

            // Empty object {} accepts any properties - no excess property check needed.
            // This is a key TypeScript behavior: {} means "any non-nullish value".
            // See https://github.com/microsoft/TypeScript/issues/60582
            if target_props.is_empty() {
                return;
            }

            // If target has an index signature, it accepts any properties
            if target_shape.string_index.is_some() || target_shape.number_index.is_some() {
                return;
            }

            // Check for excess properties in source that don't exist in target
            // This is the "freshness" or "strict object literal" check
            for source_prop in source_props {
                let target_prop = target_props.iter().find(|p| p.name == source_prop.name);
                if let Some(target_prop) = target_prop {
                    // Property exists in target - check nested object literals (Rule #4)
                    // If the source property is a fresh object literal, recursively check
                    if self
                        .ctx
                        .freshness_tracker
                        .should_check_excess_properties(source_prop.type_id)
                    {
                        self.check_object_literal_excess_properties(
                            source_prop.type_id,
                            target_prop.type_id,
                            idx,
                        );
                    }
                } else {
                    let prop_name = self.ctx.types.resolve_atom(source_prop.name);
                    self.error_excess_property_at(&prop_name, target, idx);
                }
            }
        }
        // Note: Missing property checks are handled by solver's explain_failure
    }

    /// Resolve property access using TypeEnvironment (includes lib.d.ts types).
    ///
    /// This method creates a PropertyAccessEvaluator with the TypeEnvironment as the resolver,
    /// allowing primitive property access to use lib.d.ts definitions instead of just hardcoded lists.
    ///
    /// For example, "foo".length will look up the String interface from lib.d.ts.
    pub(crate) fn resolve_property_access_with_env(
        &mut self,
        object_type: TypeId,
        prop_name: &str,
    ) -> crate::solver::PropertyAccessResult {
        use crate::solver::operations::PropertyAccessEvaluator;

        // Ensure symbols are resolved in the environment
        self.ensure_application_symbols_resolved(object_type);

        // Borrow the environment and create evaluator with resolver
        let env = self.ctx.type_env.borrow();
        let evaluator = PropertyAccessEvaluator::with_resolver(self.ctx.types, &*env);

        evaluator.resolve_property_access(object_type, prop_name)
    }

    /// Check if an assignment target is a readonly property.
    /// Reports error TS2540 if trying to assign to a readonly property.
    pub(crate) fn check_readonly_assignment(
        &mut self,
        target_idx: NodeIndex,
        _expr_idx: NodeIndex,
    ) {
        let Some(target_node) = self.ctx.arena.get(target_idx) else {
            return;
        };

        match target_node.kind {
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {}
            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.ctx.arena.get_access_expr(target_node) {
                    let object_type = self.get_type_of_node(access.expression);
                    if object_type == TypeId::ANY
                        || object_type == TypeId::UNKNOWN
                        || object_type == TypeId::ERROR
                    {
                        return;
                    }

                    let index_type = self.get_type_of_node(access.name_or_argument);
                    if let Some(name) = self.get_readonly_element_access_name(
                        object_type,
                        access.name_or_argument,
                        index_type,
                    ) {
                        self.error_readonly_property_at(&name, target_idx);
                    }
                }
                return;
            }
            _ => return,
        }

        let Some(access) = self.ctx.arena.get_access_expr(target_node) else {
            return;
        };

        // Get the property name
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return;
        };

        // Check if this is a private identifier (method or field)
        // Private methods are always readonly
        if self.is_private_identifier_name(access.name_or_argument) {
            let prop_name = if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                ident.escaped_text.clone()
            } else {
                return;
            };

            // Check if this private identifier is a method (not a field)
            // by resolving the symbol and checking if any declaration is a method
            let (symbols, _) = self.resolve_private_identifier_symbols(access.name_or_argument);
            if !symbols.is_empty() {
                let is_method = symbols.iter().any(|&sym_id| {
                    if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                        symbol.declarations.iter().any(|&decl_idx| {
                            if let Some(node) = self.ctx.arena.get(decl_idx) {
                                return node.kind == syntax_kind_ext::METHOD_DECLARATION;
                            }
                            false
                        })
                    } else {
                        false
                    }
                });

                if is_method {
                    self.error_private_method_not_writable(&prop_name, target_idx);
                    return;
                }
            }
        }

        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return;
        };

        let prop_name = ident.escaped_text.clone();

        // Get the type of the object being accessed
        let obj_type = self.get_type_of_node(access.expression);

        // P1 fix: First check if the property exists on the type.
        // If the property doesn't exist, skip the readonly check - TS2339 will be
        // reported elsewhere. This matches tsc behavior which checks existence before
        // readonly status.
        use crate::solver::PropertyAccessResult;
        let property_result = self.resolve_property_access_with_env(obj_type, &prop_name);
        let property_exists = matches!(
            property_result,
            PropertyAccessResult::Success { .. }
                | PropertyAccessResult::PossiblyNullOrUndefined { .. }
        );

        if !property_exists {
            // Property doesn't exist on this type - skip readonly check
            // The property existence error (TS2339) is reported elsewhere
            return;
        }

        // Check if the property is readonly in the object type (solver types)
        if self.is_property_readonly(obj_type, &prop_name) {
            // Special case: readonly properties can be assigned in constructors
            // if the property is declared in the current class (not inherited)
            if self.is_readonly_assignment_allowed_in_constructor(&prop_name, access.expression) {
                return;
            }

            self.error_readonly_property_at(&prop_name, target_idx);
            return;
        }

        // Also check AST-level readonly on class properties
        // Get the class name from the object expression (for `c.ro`, get the type of `c`)
        if let Some(class_name) = self.get_class_name_from_expression(access.expression)
            && self.is_class_property_readonly(&class_name, &prop_name)
        {
            // Special case: readonly properties can be assigned in constructors
            // if the property is declared in the current class (not inherited)
            if self.is_readonly_assignment_allowed_in_constructor(&prop_name, access.expression) {
                return;
            }

            self.error_readonly_property_at(&prop_name, target_idx);
        }
    }

    /// Check if a readonly property assignment is allowed in the current constructor context.
    ///
    /// Returns true if ALL of the following conditions are met:
    /// 1. We're in a constructor body
    /// 2. The assignment is to `this.property` (not some other object)
    /// 3. The property is declared in the current class (not inherited)
    pub(crate) fn is_readonly_assignment_allowed_in_constructor(
        &mut self,
        prop_name: &str,
        object_expr: NodeIndex,
    ) -> bool {
        // Must be in a constructor
        let class_idx = match &self.ctx.enclosing_class {
            Some(info) if info.in_constructor => info.class_idx,
            _ => return false,
        };

        // Must be assigning to `this.property` (not some other object)
        if !self.is_this_expression_in_constructor(object_expr) {
            return false;
        }

        // The property must be declared in the current class (not inherited)
        self.is_property_declared_in_class(prop_name, class_idx)
    }

    /// Check if an expression is `this` (helper to avoid conflict with existing method).
    pub(crate) fn is_this_expression_in_constructor(&mut self, expr_idx: NodeIndex) -> bool {
        use crate::scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };

        // Check if it's ThisKeyword (node.kind == 110)
        if node.kind == SyntaxKind::ThisKeyword as u16 {
            return true;
        }

        // Check if it's an identifier with text "this"
        if let Some(ident) = self.ctx.arena.get_identifier(node) {
            return ident.escaped_text == "this";
        }

        false
    }

    /// Check if a property is declared in a specific class (not inherited).
    pub(crate) fn is_property_declared_in_class(
        &mut self,
        prop_name: &str,
        class_idx: NodeIndex,
    ) -> bool {
        let Some(class_node) = self.ctx.arena.get(class_idx) else {
            return false;
        };

        let Some(class) = self.ctx.arena.get_class(class_node) else {
            return false;
        };

        // Check all class members for a property declaration
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            // Check property declarations
            if let Some(prop_decl) = self.ctx.arena.get_property_decl(member_node) {
                if let Some(name_node) = self.ctx.arena.get(prop_decl.name) {
                    if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                        if ident.escaped_text == prop_name {
                            return true;
                        }
                    }
                }
            }

            // Check parameter properties (constructor parameters with readonly/private/etc)
            // Find the constructor kind
            if member_node.kind == syntax_kind_ext::CONSTRUCTOR {
                if let Some(ctor) = self.ctx.arena.get_constructor(member_node) {
                    for &param_idx in &ctor.parameters.nodes {
                        let Some(param_node) = self.ctx.arena.get(param_idx) else {
                            continue;
                        };

                        // Check if it's a parameter property
                        if let Some(param_decl) = self.ctx.arena.get_parameter(param_node) {
                            // Parameter properties have modifiers and a name but no type annotation is required
                            // They're identified by having modifiers (readonly, private, public, protected)
                            if param_decl.modifiers.is_some() {
                                if let Some(name_node) = self.ctx.arena.get(param_decl.name) {
                                    if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                                        if ident.escaped_text == prop_name {
                                            return true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        false
    }

    /// Get the class name from an expression, if it's a class instance.
    pub(crate) fn get_class_name_from_expression(&mut self, expr_idx: NodeIndex) -> Option<String> {
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return None;
        };

        // If it's a simple identifier, look up its type from the binder
        if self.ctx.arena.get_identifier(node).is_some()
            && let Some(sym_id) = self.resolve_identifier_symbol(expr_idx)
        {
            let type_id = self.get_type_of_symbol(sym_id);
            if let Some(class_name) = self.get_class_name_from_type(type_id) {
                return Some(class_name);
            }
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                // Get the value declaration and check if it's a variable with new Class()
                if !symbol.value_declaration.is_none() {
                    return self.get_class_name_from_var_decl(symbol.value_declaration);
                }
            }
        }

        None
    }

    pub(crate) fn is_readonly_index_signature(
        &self,
        type_id: TypeId,
        wants_string: bool,
        wants_number: bool,
    ) -> bool {
        use crate::solver::QueryDatabase;

        self.ctx
            .types
            .is_readonly_index_signature(type_id, wants_string, wants_number)
    }

    pub(crate) fn get_readonly_element_access_name(
        &mut self,
        object_type: TypeId,
        index_expr: NodeIndex,
        index_type: TypeId,
    ) -> Option<String> {
        // First check for literal string/number properties that are readonly
        if let Some(name) = self.get_literal_string_from_node(index_expr) {
            if self.is_property_readonly(object_type, &name) {
                return Some(name);
            }
            // Don't return yet - the literal might access a readonly index signature
        }

        if let Some(index) = self.get_literal_index_from_node(index_expr) {
            let name = index.to_string();
            if self.is_property_readonly(object_type, &name) {
                return Some(name);
            }
            // Don't return yet - the literal might access a readonly index signature
        }

        if let Some((string_keys, number_keys)) = self.get_literal_key_union_from_type(index_type) {
            for key in string_keys {
                let name = self.ctx.types.resolve_atom(key);
                if self.is_property_readonly(object_type, &name) {
                    return Some(name);
                }
            }

            for key in number_keys {
                let name = format!("{}", key);
                if self.is_property_readonly(object_type, &name) {
                    return Some(name);
                }
            }
            // Don't return yet - check for readonly index signatures
        }

        // Finally check for readonly index signatures
        if let Some((wants_string, wants_number)) = self.get_index_key_kind(index_type)
            && self.is_readonly_index_signature(object_type, wants_string, wants_number)
        {
            return Some("index signature".to_string());
        }

        None
    }

    /// Check a return statement.
    /// Check an import equals declaration for ESM compatibility and unresolved modules.
    /// Emits TS1202 when `import x = require()` is used in an ES module.
    /// Emits TS2307 when the required module cannot be found.
    /// Does NOT emit TS1202 for namespace imports like `import x = Namespace.Member`.
    /// Check if individual imported members exist in the module's exports.
    /// Emits TS2305 for each missing export.
    /// Check an export declaration's module specifier for unresolved modules.
    /// Emits TS2792 when the module cannot be resolved.
    /// Handles cases like: export * as ns from './nonexistent';
    /// Check heritage clauses (extends/implements) for unresolved names.
    /// Emits TS2304 when a referenced name cannot be resolved.
    /// Emits TS2689 when a class extends an interface.
    ///
    /// Parameters:
    /// - `heritage_clauses`: The heritage clauses to check
    /// - `is_class_declaration`: true if checking a class, false if checking an interface
    ///   (TS2689 should only be emitted for classes extending interfaces, not interfaces extending interfaces)
    pub(crate) fn check_heritage_clauses_for_unresolved_names(
        &mut self,
        heritage_clauses: &Option<crate::parser::NodeList>,
        is_class_declaration: bool,
    ) {
        use crate::parser::syntax_kind_ext::HERITAGE_CLAUSE;
        use crate::scanner::SyntaxKind;

        let Some(clauses) = heritage_clauses else {
            return;
        };

        for &clause_idx in &clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };

            if clause_node.kind != HERITAGE_CLAUSE {
                continue;
            }

            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            // Check if this is an extends clause (for TS2507 errors)
            let is_extends_clause = heritage.token == SyntaxKind::ExtendsKeyword as u16;

            // Check each type in the heritage clause
            for &type_idx in &heritage.types.nodes {
                let Some(type_node) = self.ctx.arena.get(type_idx) else {
                    continue;
                };

                // Get the expression (identifier or property access) from ExpressionWithTypeArguments
                let expr_idx =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        expr_type_args.expression
                    } else {
                        type_idx
                    };

                // Try to resolve the heritage symbol
                if let Some(heritage_sym) = self.resolve_heritage_symbol(expr_idx) {
                    // Symbol was resolved - check if it represents a constructor type for extends clauses
                    if is_extends_clause {
                        use crate::binder::symbol_flags;

                        // Note: Must resolve type aliases before checking flags and getting type
                        let mut visited_aliases = Vec::new();
                        let resolved_sym =
                            self.resolve_alias_symbol(heritage_sym, &mut visited_aliases);
                        let sym_to_check = resolved_sym.unwrap_or(heritage_sym);

                        let symbol_type = self.get_type_of_symbol(sym_to_check);

                        // Check if this is ONLY an interface (not also a class from
                        // declaration merging) - emit TS2689 instead of TS2507
                        // BUT only for class declarations, not interface declarations
                        // (interfaces can validly extend other interfaces)
                        // When a name is both an interface and a class (merged declaration),
                        // the class part can be validly extended, so don't emit TS2689.
                        let is_interface_only = self
                            .ctx
                            .binder
                            .get_symbol(sym_to_check)
                            .map(|s| {
                                (s.flags & symbol_flags::INTERFACE) != 0
                                    && (s.flags & symbol_flags::CLASS) == 0
                            })
                            .unwrap_or(false);

                        if is_interface_only && is_class_declaration {
                            // Emit TS2689: Cannot extend an interface (only for classes)
                            if let Some(name) = self.heritage_name_text(expr_idx) {
                                use crate::checker::types::diagnostics::{
                                    diagnostic_codes, diagnostic_messages, format_message,
                                };
                                let message = format_message(
                                    diagnostic_messages::CANNOT_EXTEND_AN_INTERFACE,
                                    &[&name],
                                );
                                self.error_at_node(
                                    expr_idx,
                                    &message,
                                    diagnostic_codes::CANNOT_EXTEND_AN_INTERFACE,
                                );
                            }
                        } else if !is_interface_only
                            && is_class_declaration
                            && !self.is_constructor_type(symbol_type)
                            && !self.is_class_symbol(sym_to_check)
                        {
                            // For classes extending non-interfaces: emit TS2507 if not a constructor type
                            // For interfaces: don't check constructor types (interfaces can extend any interface)
                            if let Some(name) = self.heritage_name_text(expr_idx) {
                                use crate::checker::types::diagnostics::{
                                    diagnostic_codes, diagnostic_messages, format_message,
                                };
                                let message = format_message(
                                    diagnostic_messages::TYPE_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE,
                                    &[&name],
                                );
                                self.error_at_node(
                                    expr_idx,
                                    &message,
                                    diagnostic_codes::TYPE_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE,
                                );
                            }
                        }
                    }
                } else {
                    // Could not resolve as a heritage symbol - check if it's an identifier
                    // that references a value with a constructor type
                    let is_valid_constructor = if let Some(expr_node) = self.ctx.arena.get(expr_idx)
                        && expr_node.kind == SyntaxKind::Identifier as u16
                    {
                        // Check if this is a primitive type keyword - these should not
                        // trigger type resolution errors in extends clauses.
                        // TypeScript silently fails for `class C extends number {}`.
                        if let Some(ident) = self.ctx.arena.get_identifier(expr_node) {
                            let name = ident.escaped_text.as_str();
                            if matches!(
                                name,
                                "number"
                                    | "string"
                                    | "boolean"
                                    | "symbol"
                                    | "bigint"
                                    | "any"
                                    | "unknown"
                                    | "never"
                                    | "object"
                            ) {
                                // Skip type resolution for primitive type keywords
                                // They can't be extended and shouldn't emit TS2552 suggestions
                                continue;
                            }
                        }
                        // Try to get the type of the expression to check if it's a constructor
                        let expr_type = self.get_type_of_node(expr_idx);
                        self.is_constructor_type(expr_type)
                    } else {
                        false
                    };

                    if !is_valid_constructor {
                        if let Some(expr_node) = self.ctx.arena.get(expr_idx) {
                            // Special case: `extends null` is valid in TypeScript!
                            // It creates a class that doesn't inherit from Object.prototype
                            if expr_node.kind == SyntaxKind::NullKeyword as u16
                                || (expr_node.kind == SyntaxKind::Identifier as u16
                                    && self
                                        .ctx
                                        .arena
                                        .get_identifier(expr_node)
                                        .is_some_and(|id| id.escaped_text == "null"))
                            {
                                continue;
                            }

                            // Check for literals - emit TS2507 for extends clauses
                            // NOTE: TypeScript allows `extends null` as a special case,
                            // so we don't emit TS2507 for null in extends clauses
                            let literal_type_name: Option<&str> = match expr_node.kind {
                                k if k == SyntaxKind::NullKeyword as u16 => {
                                    // Don't error on null - TypeScript allows `extends null`
                                    None
                                }
                                k if k == SyntaxKind::UndefinedKeyword as u16 => Some("undefined"),
                                k if k == SyntaxKind::TrueKeyword as u16 => Some("true"),
                                k if k == SyntaxKind::FalseKeyword as u16 => Some("false"),
                                k if k == SyntaxKind::VoidKeyword as u16 => Some("void"),
                                k if k == SyntaxKind::NumericLiteral as u16 => Some("number"),
                                k if k == SyntaxKind::StringLiteral as u16 => Some("string"),
                                // Also check for identifiers with reserved names (parsed as identifier)
                                k if k == SyntaxKind::Identifier as u16 => {
                                    if let Some(ident) = self.ctx.arena.get_identifier(expr_node) {
                                        match ident.escaped_text.as_str() {
                                            // Don't error on null - TypeScript allows `extends null`
                                            "null" => None,
                                            "undefined" => Some("undefined"),
                                            "void" => Some("void"),
                                            _ => None,
                                        }
                                    } else {
                                        None
                                    }
                                }
                                _ => None,
                            };

                            if let Some(type_name) = literal_type_name {
                                if is_extends_clause {
                                    use crate::checker::types::diagnostics::{
                                        diagnostic_codes, diagnostic_messages, format_message,
                                    };
                                    let message = format_message(
                                    diagnostic_messages::TYPE_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE,
                                    &[type_name],
                                );
                                    self.error_at_node(
                                        expr_idx,
                                        &message,
                                        diagnostic_codes::TYPE_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE,
                                    );
                                }
                                continue;
                            }
                        }
                        // Get the name for the error message
                        if let Some(name) = self.heritage_name_text(expr_idx) {
                            // Skip certain reserved names that are handled elsewhere or shouldn't trigger errors
                            // Note: "null" is not included because `extends null` is valid and handled above
                            // Primitive type keywords (number, string, boolean, etc.) in extends clauses
                            // are parsed as identifiers but shouldn't emit TS2318/TS2304 errors.
                            // TypeScript silently fails to resolve them without emitting these errors.
                            if matches!(
                                name.as_str(),
                                "undefined"
                                    | "true"
                                    | "false"
                                    | "void"
                                    | "0"
                                    | "number"
                                    | "string"
                                    | "boolean"
                                    | "symbol"
                                    | "bigint"
                                    | "any"
                                    | "unknown"
                                    | "never"
                                    | "object"
                            ) {
                                continue;
                            }
                            if self.is_known_global_type_name(&name) {
                                // Check if the global type is actually available in lib contexts
                                if !self.ctx.has_name_in_lib(&name) {
                                    // TS2318/TS2583: Emit error for missing global type
                                    self.error_cannot_find_global_type(&name, expr_idx);
                                }
                                continue;
                            }
                            // Skip TS2304 for property accesses on imports from unresolved modules
                            // TS2307 is already emitted for the unresolved module
                            if self.is_property_access_on_unresolved_import(expr_idx) {
                                continue;
                            }
                            self.error_cannot_find_name_at(&name, expr_idx);
                        }
                    }
                }
            }
        }
    }

    /// Check a class declaration.
    pub(crate) fn check_class_declaration(&mut self, stmt_idx: NodeIndex) {
        use crate::checker::class_inheritance::ClassInheritanceChecker;
        use crate::checker::types::diagnostics::diagnostic_codes;

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        let Some(class) = self.ctx.arena.get_class(node) else {
            return;
        };

        // TS1042: async modifier cannot be used on class declarations
        self.check_async_modifier_on_declaration(&class.modifiers);

        // CRITICAL: Check for circular inheritance using InheritanceGraph
        // This prevents stack overflow from infinite recursion in get_class_instance_type
        // Must be done BEFORE any type checking to catch cycles early
        let mut checker = ClassInheritanceChecker::new(&mut self.ctx);
        if let Err(()) = checker.check_class_inheritance_cycle(stmt_idx, class) {
            return; // Cycle detected - error already emitted, skip all type checking
        }

        // Check for reserved class names (error 2414)
        if !class.name.is_none()
            && let Some(name_node) = self.ctx.arena.get(class.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && ident.escaped_text == "any"
        {
            self.error_at_node(
                class.name,
                "Class name cannot be 'any'.",
                diagnostic_codes::CLASS_NAME_CANNOT_BE_ANY,
            );
        }

        // Check if this is a declared class (ambient declaration)
        let is_declared = self.has_declare_modifier(&class.modifiers);

        // Check if this class is abstract
        let is_abstract_class = self.has_abstract_modifier(&class.modifiers);

        // Push type parameters BEFORE checking heritage clauses and abstract members
        // This allows heritage clauses and member checks to reference the class's type parameters
        let (_type_params, type_param_updates) = self.push_type_parameters(&class.type_parameters);

        // Check heritage clauses for unresolved names (TS2304)
        // Must be checked AFTER type parameters are pushed so heritage can reference type params
        self.check_heritage_clauses_for_unresolved_names(&class.heritage_clauses, true);

        // Check for abstract members in non-abstract class (error 1253)
        // and private identifiers in ambient classes (error 2819)
        for &member_idx in &class.members.nodes {
            if let Some(member_node) = self.ctx.arena.get(member_idx) {
                // TS2819: Check for private identifiers in ambient classes
                if is_declared {
                    let member_name_idx = match member_node.kind {
                        syntax_kind_ext::PROPERTY_DECLARATION => self
                            .ctx
                            .arena
                            .get_property_decl(member_node)
                            .map(|p| p.name),
                        syntax_kind_ext::METHOD_DECLARATION => {
                            self.ctx.arena.get_method_decl(member_node).map(|m| m.name)
                        }
                        syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                            self.ctx.arena.get_accessor(member_node).map(|a| a.name)
                        }
                        _ => None,
                    };

                    if let Some(name_idx) = member_name_idx
                        && !name_idx.is_none()
                        && let Some(name_node) = self.ctx.arena.get(name_idx)
                        && name_node.kind == crate::scanner::SyntaxKind::PrivateIdentifier as u16
                    {
                        use crate::checker::types::diagnostics::diagnostic_messages;
                        self.error_at_node(
                            name_idx,
                            diagnostic_messages::PRIVATE_IDENTIFIER_IN_AMBIENT_CONTEXT,
                            diagnostic_codes::PRIVATE_IDENTIFIER_IN_AMBIENT_CONTEXT,
                        );
                    }
                }

                // Check for abstract members in non-abstract class
                if !is_abstract_class {
                    let member_has_abstract = match member_node.kind {
                        syntax_kind_ext::PROPERTY_DECLARATION => {
                            if let Some(prop) = self.ctx.arena.get_property_decl(member_node) {
                                self.has_abstract_modifier(&prop.modifiers)
                            } else {
                                false
                            }
                        }
                        syntax_kind_ext::METHOD_DECLARATION => {
                            if let Some(method) = self.ctx.arena.get_method_decl(member_node) {
                                self.has_abstract_modifier(&method.modifiers)
                            } else {
                                false
                            }
                        }
                        syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                            if let Some(accessor) = self.ctx.arena.get_accessor(member_node) {
                                self.has_abstract_modifier(&accessor.modifiers)
                            } else {
                                false
                            }
                        }
                        _ => false,
                    };

                    if member_has_abstract {
                        // Report on the 'abstract' keyword
                        self.error_at_node(
                            member_idx,
                            "Abstract properties can only appear within an abstract class.",
                            diagnostic_codes::ABSTRACT_ONLY_IN_ABSTRACT_CLASS,
                        );
                    }
                }
            }
        }

        // Collect class name and static members for error 2662 suggestions
        let class_name = if !class.name.is_none() {
            if let Some(name_node) = self.ctx.arena.get(class.name) {
                self.ctx
                    .arena
                    .get_identifier(name_node)
                    .map(|ident| ident.escaped_text.clone())
            } else {
                None
            }
        } else {
            None
        };

        // Save previous enclosing class and set current
        let prev_enclosing_class = self.ctx.enclosing_class.take();
        if let Some(name) = class_name {
            self.ctx.enclosing_class = Some(EnclosingClassInfo {
                name,
                class_idx: stmt_idx,
                member_nodes: class.members.nodes.clone(),
                in_constructor: false,
                is_declared,
                in_static_property_initializer: false,
                in_static_method: false,
            });
        }

        // Check each class member
        for &member_idx in &class.members.nodes {
            self.check_class_member(member_idx);
        }

        // Check for duplicate member names (TS2300, TS2393)
        self.check_duplicate_class_members(&class.members.nodes);

        // Check for missing method/constructor implementations (2389, 2390, 2391)
        // Skip for declared classes (ambient declarations don't need implementations)
        if !is_declared {
            self.check_class_member_implementations(&class.members.nodes);
        }

        // Check for accessor abstract consistency (error 2676)
        // Getter and setter must both be abstract or both non-abstract
        self.check_accessor_abstract_consistency(&class.members.nodes);

        // Check for getter/setter type compatibility (error 2322)
        // Getter return type must be assignable to setter parameter type
        self.check_accessor_type_compatibility(&class.members.nodes);

        // Check strict property initialization (TS2564)
        self.check_property_initialization(stmt_idx, class, is_declared, is_abstract_class);

        // Check for property type compatibility with base class (error 2416)
        // Property type in derived class must be assignable to same property in base class
        self.check_property_inheritance_compatibility(stmt_idx, class);

        // Check that non-abstract class implements all abstract members from base class (error 2654)
        self.check_abstract_member_implementations(stmt_idx, class);

        // Check that class properly implements all interfaces from implements clauses (error 2420)
        self.check_implements_clauses(stmt_idx, class);

        // Restore previous enclosing class
        self.ctx.enclosing_class = prev_enclosing_class;

        self.pop_type_parameters(type_param_updates);
    }

    pub(crate) fn check_class_expression(
        &mut self,
        class_idx: NodeIndex,
        class: &crate::parser::node::ClassData,
    ) {
        let (_type_params, type_param_updates) = self.push_type_parameters(&class.type_parameters);

        let class_name = self.get_class_name_from_decl(class_idx);
        let is_abstract_class = self.has_abstract_modifier(&class.modifiers);

        let prev_enclosing_class = self.ctx.enclosing_class.take();
        self.ctx.enclosing_class = Some(EnclosingClassInfo {
            name: class_name,
            class_idx,
            member_nodes: class.members.nodes.clone(),
            in_constructor: false,
            is_declared: false,
            in_static_property_initializer: false,
            in_static_method: false,
        });

        for &member_idx in &class.members.nodes {
            self.check_class_member(member_idx);
        }

        // Check strict property initialization (TS2564) for class expressions
        // Class expressions should have the same property initialization checks as class declarations
        self.check_property_initialization(class_idx, class, false, is_abstract_class);

        self.ctx.enclosing_class = prev_enclosing_class;

        self.pop_type_parameters(type_param_updates);
    }

    pub(crate) fn check_property_initialization(
        &mut self,
        _class_idx: NodeIndex,
        class: &crate::parser::node::ClassData,
        is_declared: bool,
        _is_abstract: bool,
    ) {
        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};

        // Skip TS2564 for declared classes (ambient declarations)
        // Note: Abstract classes DO get TS2564 errors - they can have constructors
        // and properties must be initialized either with defaults or in the constructor
        if is_declared {
            return;
        }

        // Only check property initialization when strictPropertyInitialization is enabled
        if !self.ctx.strict_property_initialization() {
            return;
        }

        // Check if this is a derived class (has base class)
        let is_derived_class = self.class_has_base(class);

        let mut properties = Vec::new();
        let mut tracked = FxHashSet::default();
        let mut parameter_properties = FxHashSet::default();

        // First pass: collect parameter properties from constructor
        // Parameter properties are always definitely assigned
        for &member_idx in &class.members.nodes {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let Some(ctor) = self.ctx.arena.get_constructor(node) else {
                continue;
            };

            // Collect parameter properties from constructor parameters
            for &param_idx in &ctor.parameters.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };

                // Parameter properties have modifiers (public/private/protected/readonly)
                if param.modifiers.is_some()
                    && let Some(key) = self.property_key_from_name(param.name)
                {
                    parameter_properties.insert(key.clone());
                }
            }
        }

        // Second pass: collect class properties that need initialization
        for &member_idx in &class.members.nodes {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                continue;
            }

            let Some(prop) = self.ctx.arena.get_property_decl(node) else {
                continue;
            };

            if !self.property_requires_initialization(member_idx, prop, is_derived_class) {
                continue;
            }

            let Some(key) = self.property_key_from_name(prop.name) else {
                continue;
            };

            // Get property name for error message. Use fallback for complex computed properties.
            let name = self.get_property_name(prop.name).unwrap_or_else(|| {
                // For complex computed properties (e.g., [getKey()]), use a descriptive fallback
                match &key {
                    PropertyKey::Computed(ComputedKey::Ident(s)) => format!("[{}]", s),
                    PropertyKey::Computed(ComputedKey::String(s)) => format!("[\"{}\"]", s),
                    PropertyKey::Computed(ComputedKey::Number(n)) => format!("[{}]", n),
                    PropertyKey::Computed(ComputedKey::Qualified(q)) => format!("[{}]", q),
                    PropertyKey::Computed(ComputedKey::Symbol(Some(s))) => {
                        format!("[Symbol({})]", s)
                    }
                    PropertyKey::Computed(ComputedKey::Symbol(None)) => "[Symbol()]".to_string(),
                    PropertyKey::Private(s) => format!("#{}", s),
                    PropertyKey::Ident(s) => s.clone(),
                }
            });

            tracked.insert(key.clone());
            properties.push((key, name, prop.name));
        }

        if properties.is_empty() {
            return;
        }

        let requires_super = self.class_has_base(class);
        let constructor_body = self.find_constructor_body(&class.members);
        let assigned = if let Some(body_idx) = constructor_body {
            self.analyze_constructor_assignments(body_idx, &tracked, requires_super)
        } else {
            FxHashSet::default()
        };

        for (key, name, name_node) in properties {
            // Property is assigned if it's in the assigned set OR it's a parameter property
            if assigned.contains(&key) || parameter_properties.contains(&key) {
                continue;
            }
            use crate::checker::types::diagnostics::format_message;

            // Use TS2524 if there's a constructor (definite assignment analysis)
            // Use TS2564 if no constructor (just missing initializer)
            let (message, code) = if constructor_body.is_some() {
                (
                    diagnostic_messages::PROPERTY_NO_INITIALIZER_NO_DEFINITE_ASSIGNMENT,
                    diagnostic_codes::PROPERTY_NO_INITIALIZER_NO_DEFINITE_ASSIGNMENT,
                )
            } else {
                (
                    diagnostic_messages::PROPERTY_HAS_NO_INITIALIZER,
                    diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER,
                )
            };

            self.error_at_node(name_node, &format_message(message, &[&name]), code);
        }

        // Check for TS2565 (Property used before being assigned in constructor)
        if let Some(body_idx) = constructor_body {
            self.check_properties_used_before_assigned(body_idx, &tracked, requires_super);
        }
    }

    pub(crate) fn property_requires_initialization(
        &mut self,
        member_idx: NodeIndex,
        prop: &crate::parser::node::PropertyDeclData,
        is_derived_class: bool,
    ) -> bool {
        use crate::scanner::SyntaxKind;

        if !prop.initializer.is_none()
            || prop.question_token
            || prop.exclamation_token
            || self.has_static_modifier(&prop.modifiers)
            || self.has_abstract_modifier(&prop.modifiers)
            || self.has_declare_modifier(&prop.modifiers)
        {
            return false;
        }

        // Properties with string or numeric literal names are not checked for strict property initialization
        // Example: class C { "b": number; 0: number; }  // These are not checked
        let Some(name_node) = self.ctx.arena.get(prop.name) else {
            return false;
        };
        if matches!(
            name_node.kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
        ) {
            return false;
        }

        let prop_type = if !prop.type_annotation.is_none() {
            self.get_type_from_type_node(prop.type_annotation)
        } else if let Some(sym_id) = self.ctx.binder.get_node_symbol(member_idx) {
            self.get_type_of_symbol(sym_id)
        } else {
            TypeId::ANY
        };

        // Enhanced property initialization checking:
        // 1. ANY/UNKNOWN types don't need initialization
        // 2. Union types with undefined don't need initialization
        // 3. Optional types don't need initialization
        if prop_type == TypeId::ANY || prop_type == TypeId::UNKNOWN {
            return false;
        }

        // ERROR types also don't need initialization - these indicate parsing/binding errors
        if prop_type == TypeId::ERROR {
            return false;
        }

        // For derived classes, be more strict about definite assignment
        // Properties in derived classes that redeclare base class properties need initialization
        // This catches cases like: class B extends A { property: any; } where A has property
        if is_derived_class {
            // In derived classes, properties without definite assignment assertions
            // need initialization unless they include undefined in their type
            return !self.type_includes_undefined(prop_type);
        }

        !self.type_includes_undefined(prop_type)
    }

    // Note: class_has_base, type_includes_undefined, find_constructor_body are in type_checking.rs

    /// Check for TS2565: Properties used before being assigned in the constructor.
    ///
    /// This function analyzes the constructor body to detect when a property
    /// is accessed (via `this.X`) before it has been assigned a value.
    pub(crate) fn check_properties_used_before_assigned(
        &mut self,
        body_idx: NodeIndex,
        tracked: &FxHashSet<PropertyKey>,
        require_super: bool,
    ) {
        if body_idx.is_none() {
            return;
        }

        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };

        if body_node.kind != syntax_kind_ext::BLOCK {
            return;
        }

        let Some(block) = self.ctx.arena.get_block(body_node) else {
            return;
        };

        let start_idx = if require_super {
            self.find_super_statement_start(&block.statements.nodes)
                .unwrap_or(0)
        } else {
            0
        };

        let mut assigned = FxHashSet::default();

        // Track parameter properties as already assigned
        for _key in tracked.iter() {
            // Parameter properties are assigned in the parameter list
            // We'll collect them separately if needed
        }

        // Analyze statements in order, checking for property accesses before assignment
        for &stmt_idx in block.statements.nodes.iter().skip(start_idx) {
            self.check_statement_for_early_property_access(stmt_idx, &mut assigned, tracked);
        }
    }

    /// Check a single statement for property accesses that occur before assignment.
    /// Returns true if the statement definitely assigns to the tracked property.
    pub(crate) fn check_statement_for_early_property_access(
        &mut self,
        stmt_idx: NodeIndex,
        assigned: &mut FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) -> bool {
        if stmt_idx.is_none() {
            return false;
        }

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &stmt_idx in &block.statements.nodes {
                        self.check_statement_for_early_property_access(stmt_idx, assigned, tracked);
                    }
                }
                false
            }
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) {
                    self.check_expression_for_early_property_access(
                        expr_stmt.expression,
                        assigned,
                        tracked,
                    );
                }
                false
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.ctx.arena.get_if_statement(node) {
                    // Check the condition expression for property accesses
                    self.check_expression_for_early_property_access(
                        if_stmt.expression,
                        assigned,
                        tracked,
                    );
                    // Check both branches
                    let mut then_assigned = assigned.clone();
                    let mut else_assigned = assigned.clone();
                    self.check_statement_for_early_property_access(
                        if_stmt.then_statement,
                        &mut then_assigned,
                        tracked,
                    );
                    if !if_stmt.else_statement.is_none() {
                        self.check_statement_for_early_property_access(
                            if_stmt.else_statement,
                            &mut else_assigned,
                            tracked,
                        );
                    }
                    // Properties assigned in both branches are considered assigned
                    *assigned = then_assigned
                        .intersection(&else_assigned)
                        .cloned()
                        .collect();
                }
                false
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret_stmt) = self.ctx.arena.get_return_statement(node)
                    && !ret_stmt.expression.is_none()
                {
                    self.check_expression_for_early_property_access(
                        ret_stmt.expression,
                        assigned,
                        tracked,
                    );
                }
                false
            }
            k if k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT
                || k == syntax_kind_ext::FOR_STATEMENT
                || k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT =>
            {
                // For loops, we conservatively don't track assignments across iterations
                // This is a simplified approach - the full TypeScript implementation is more complex
                false
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_stmt) = self.ctx.arena.get_try(node) {
                    self.check_statement_for_early_property_access(
                        try_stmt.try_block,
                        assigned,
                        tracked,
                    );
                    // Check catch and finally blocks
                    // ...
                }
                false
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = self.ctx.arena.get_variable(node) {
                    for &decl_idx in &var_stmt.declarations.nodes {
                        if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                            && let Some(decl) = self.ctx.arena.get_variable_declaration(decl_node)
                            && !decl.initializer.is_none()
                        {
                            self.check_expression_for_early_property_access(
                                decl.initializer,
                                assigned,
                                tracked,
                            );
                        }
                    }
                }
                false
            }
            _ => false,
        }
    }

    // Flow analysis functions moved to checker/flow_analysis.rs
}
