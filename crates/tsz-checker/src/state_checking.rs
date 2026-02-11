//! Declaration & Statement Checking Module
//!
//! Extracted from state.rs: Methods for checking source files, declarations,
//! statements, and class/interface validation. Also includes StatementCheckCallbacks.

use crate::EnclosingClassInfo;
use crate::error_handler::ErrorHandler;
use crate::flow_analysis::{ComputedKey, PropertyKey};
use crate::state::CheckerState;
use crate::statements::StatementChecker;
use rustc_hash::FxHashSet;
use std::time::Instant;
use tracing::{Level, span};
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

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
            let perf_enabled = std::env::var_os("TSZ_PERF").is_some();
            let perf_log = |phase: &'static str, start: Instant| {
                if perf_enabled {
                    tracing::info!(
                        target: "wasm::perf",
                        phase,
                        ms = start.elapsed().as_secs_f64() * 1000.0
                    );
                }
            };

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
            self.ctx.compiler_options.no_unused_locals =
                self.resolve_no_unused_locals_from_source(&sf.text);
            self.ctx.compiler_options.no_unused_parameters =
                self.resolve_no_unused_parameters_from_source(&sf.text);

            // `type_env` is rebuilt per file, so drop per-file symbol-resolution memoization.
            self.ctx.application_symbols_resolved.clear();
            self.ctx.application_symbols_resolution_set.clear();
            self.ctx.contains_infer_types_true.clear();
            self.ctx.contains_infer_types_false.clear();

            // CRITICAL FIX: Build TypeEnvironment with all symbols (including lib symbols)
            // This ensures Error, Math, JSON, etc. interfaces are registered for property resolution
            // Without this, TypeKey::Ref(Error) returns ERROR, causing TS2339 false positives
            let env_start = Instant::now();
            let populated_env = self.build_type_environment();
            perf_log("build_type_environment", env_start);
            *self.ctx.type_env.borrow_mut() = populated_env.clone();
            // CRITICAL: Also populate type_environment (Rc-wrapped) for FlowAnalyzer
            // This ensures type alias narrowing works during control flow analysis
            *self.ctx.type_environment.borrow_mut() = populated_env;

            // Register boxed types (String, Number, Boolean, etc.) from lib.d.ts
            // This enables primitive property access to use lib definitions instead of hardcoded lists
            // IMPORTANT: Must run AFTER build_type_environment() because it replaces the
            // TypeEnvironment, which would erase the boxed/array type registrations.
            self.register_boxed_types();

            // Type check each top-level statement
            // Mark that we're now in the checking phase. During build_type_environment,
            // closures may be type-checked without contextual types, which would cause
            // premature TS7006 errors. The checking phase ensures contextual types are available.
            self.ctx.is_checking_statements = true;
            let stmt_start = Instant::now();
            for &stmt_idx in &sf.statements.nodes {
                self.check_statement(stmt_idx);
            }
            // Check for unreachable code at the source file level (TS7027)
            // Must run AFTER statement checking so types are resolved (avoids premature TS7006)
            self.check_unreachable_code_in_block(&sf.statements.nodes);
            perf_log("check_statements", stmt_start);

            let post_start = Instant::now();
            // Check for function overload implementations (2389, 2391)
            self.check_function_implementations(&sf.statements.nodes);

            // Check for export assignment with other exports (2309)
            self.check_export_assignment(&sf.statements.nodes);

            // Check for duplicate identifiers (2300)
            self.check_duplicate_identifiers();

            // Check for missing global types (2318)
            // Emits errors at file start for essential types when libs are not loaded
            self.check_missing_global_types();

            // Check triple-slash reference directives (TS6053)
            self.check_triple_slash_references(&sf.file_name, &sf.text);

            // Check for unused declarations (TS6133/TS6196)
            if self.ctx.no_unused_locals() || self.ctx.no_unused_parameters() {
                self.check_unused_declarations();
            }
            perf_log("post_checks", post_start);
        }
    }

    pub(crate) fn declaration_symbol_flags(&self, decl_idx: NodeIndex) -> Option<u32> {
        use tsz_parser::parser::node_flags;

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
                                mod_node.kind == tsz_scanner::SyntaxKind::ConstKeyword as u16
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
    /// `is_for_in` should be true for for-in loops (to emit TS2404 on type annotations).
    pub(crate) fn assign_for_in_of_initializer_types(
        &mut self,
        decl_list_idx: NodeIndex,
        element_type: TypeId,
        is_for_in: bool,
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
                // TS2404: The left-hand side of a 'for...in' statement cannot use a type annotation
                // TSC emits TS2404 and skips the assignability check for for-in loops.
                if is_for_in {
                    use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.error_at_node(
                        var_decl.type_annotation,
                        diagnostic_messages::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_USE_A_TYPE_ANNOTATION,
                        diagnostic_codes::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_USE_A_TYPE_ANNOTATION,
                    );
                }

                let declared = self.get_type_from_type_node(var_decl.type_annotation);

                // TS2322: Check that element type is assignable to declared type
                // Skip for for-in loops — TSC only emits TS2404 (no assignability check).
                if !is_for_in
                    && declared != TypeId::ANY
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
                    // TS2488: For array binding patterns, check if the element type is iterable
                    // Example: for (const [,] of []) where [] has type never[] with element type never
                    if name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
                        use tsz_parser::NodeIndex;
                        self.check_destructuring_iterability(
                            var_decl.name,
                            declared,
                            NodeIndex::NONE,
                        );
                    }
                    self.assign_binding_pattern_symbol_types(var_decl.name, declared);
                }

                if let Some(sym_id) = self.ctx.binder.get_node_symbol(decl_idx) {
                    self.cache_symbol_type(sym_id, declared);
                } else if let Some(sym_id) = self.ctx.binder.get_node_symbol(var_decl.name) {
                    self.cache_symbol_type(sym_id, declared);
                }
            } else {
                // No type annotation - use element type (with freshness stripped)
                let widened_element_type = if !self.ctx.compiler_options.sound_mode {
                    tsz_solver::freshness::widen_freshness(self.ctx.types, element_type)
                } else {
                    element_type
                };

                // Assign types for binding patterns (e.g., `for (const [a] of arr)`).
                if let Some(name_node) = self.ctx.arena.get(var_decl.name)
                    && (name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
                {
                    // TS2488: For array binding patterns, check if the element type is iterable
                    // Example: for (const [,] of []) where [] has type never[] with element type never
                    if name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
                        use tsz_parser::NodeIndex;
                        self.check_destructuring_iterability(
                            var_decl.name,
                            widened_element_type,
                            NodeIndex::NONE,
                        );
                    }
                    self.assign_binding_pattern_symbol_types(var_decl.name, widened_element_type);
                }

                if let Some(sym_id) = self.ctx.binder.get_node_symbol(decl_idx) {
                    self.cache_symbol_type(sym_id, widened_element_type);
                } else if let Some(sym_id) = self.ctx.binder.get_node_symbol(var_decl.name) {
                    self.cache_symbol_type(sym_id, widened_element_type);
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

        // TS1100: Invalid use of 'arguments'/'eval' in strict mode
        // Applies regardless of target when alwaysStrict is enabled
        if self.ctx.compiler_options.always_strict {
            if let Some(ref name) = var_name {
                if name == "arguments" || name == "eval" {
                    use crate::types::diagnostics::diagnostic_codes;
                    self.error_at_node_msg(
                        var_decl.name,
                        diagnostic_codes::INVALID_USE_OF_IN_STRICT_MODE,
                        &[name],
                    );
                }
            }
        }

        let is_catch_variable = self.is_catch_clause_variable_declaration(decl_idx);

        // TS1039: Initializers are not allowed in ambient contexts
        if !var_decl.initializer.is_none() && self.is_ambient_declaration(decl_idx) {
            use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                var_decl.initializer,
                diagnostic_messages::INITIALIZERS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS,
                diagnostic_codes::INITIALIZERS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS,
            );
        }

        let compute_final_type = |checker: &mut CheckerState| -> TypeId {
            let mut has_type_annotation = !var_decl.type_annotation.is_none();
            let mut declared_type = if has_type_annotation {
                let type_id = checker.get_type_from_type_node(var_decl.type_annotation);

                // TS1196: Catch clause variable type annotation must be 'any' or 'unknown'
                if is_catch_variable
                    && type_id != TypeId::ANY
                    && type_id != TypeId::UNKNOWN
                    && !checker.type_contains_error(type_id)
                {
                    use crate::types::diagnostics::diagnostic_codes;
                    checker.error_at_node(
                        var_decl.type_annotation,
                        "Catch clause variable type annotation must be 'any' or 'unknown' if specified.",
                        diagnostic_codes::CATCH_CLAUSE_VARIABLE_TYPE_ANNOTATION_MUST_BE_ANY_OR_UNKNOWN_IF_SPECIFIED,
                    );
                }

                type_id
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
                    // Evaluate the declared type to resolve conditionals before using as context.
                    // This ensures types like `type C = string extends string ? "yes" : "no"`
                    // provide proper contextual typing for literals, preventing them from widening to string.
                    // Only evaluate conditional/mapped/index access types - NOT type aliases or interface
                    // references, as evaluating those can change their representation and break variance checking.
                    let evaluated_type = if declared_type != TypeId::ANY {
                        use tsz_solver::TypeKey;
                        let should_evaluate =
                            checker.ctx.types.lookup(declared_type).is_some_and(|key| {
                                matches!(
                                    key,
                                    TypeKey::Conditional(_)
                                        | TypeKey::Mapped(_)
                                        | TypeKey::IndexAccess(_, _)
                                )
                            });
                        if should_evaluate {
                            checker.judge_evaluate(declared_type)
                        } else {
                            declared_type
                        }
                    } else {
                        declared_type
                    };

                    // Set contextual type for the initializer (but not for 'any')
                    let prev_context = checker.ctx.contextual_type;
                    if evaluated_type != TypeId::ANY {
                        checker.ctx.contextual_type = Some(evaluated_type);
                        // Clear cached type to force recomputation with contextual type
                        // This is necessary because the expression (especially arrow functions)
                        // might have been previously typed without contextual information
                        // (e.g., during symbol binding or early AST traversal)
                        checker.clear_type_cache_recursive(var_decl.initializer);
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
                        } else {
                            // FIX: Only check excess properties when assignability SUCCEEDS.
                            // This follows Solver-First architecture and prevents multiple TS2322 errors
                            // for the same assignment. The Solver already determined compatibility, so we
                            // only need to check for excess properties if types are assignable.
                            //
                            // Previously, excess properties were checked unconditionally, which violated
                            // the separation of concerns between Solver (assignability) and Checker (freshness).
                            // This caused tuples to be treated as objects with numeric index properties, leading
                            // to multiple redundant errors instead of a single "Type not assignable" error.
                            checker.check_object_literal_excess_properties(
                                init_type,
                                declared_type,
                                var_decl.initializer,
                            );
                        }
                    }

                    // Note: Freshness is tracked by the TypeId flags.
                    // Fresh vs non-fresh object types are interned distinctly.
                }
                // Type annotation determines the final type
                return declared_type;
            }

            // No type annotation - infer from initializer
            if !var_decl.initializer.is_none() {
                // Clear cache for closure initializers so TS7006 is properly emitted.
                // During build_type_environment, closures are typed without contextual info
                // and TS7006 is deferred. Now that we're in the checking phase, re-evaluate
                // so TS7006 can fire for closures that truly lack contextual types.
                if let Some(init_node) = checker.ctx.arena.get(var_decl.initializer) {
                    if matches!(
                        init_node.kind,
                        syntax_kind_ext::FUNCTION_EXPRESSION | syntax_kind_ext::ARROW_FUNCTION
                    ) {
                        checker.clear_type_cache_recursive(var_decl.initializer);
                    }
                }
                let init_type = checker.get_type_of_node(var_decl.initializer);

                // When strictNullChecks is off, undefined and null widen to any
                // (TypeScript treats `var x = undefined` as `any` without strict)
                if !checker.ctx.strict_null_checks()
                    && (init_type == TypeId::UNDEFINED || init_type == TypeId::NULL)
                {
                    return TypeId::ANY;
                }

                // Note: Freshness is tracked by the TypeId flags.
                // Fresh vs non-fresh object types are interned distinctly.

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
            if !self.ctx.compiler_options.sound_mode {
                final_type = tsz_solver::freshness::widen_freshness(self.ctx.types, final_type);
            }
            self.pop_symbol_dependency();

            // FIX: Always cache the widened type, overwriting any fresh type that was
            // cached during compute_final_type. This prevents "Zombie Freshness" where
            // get_type_of_symbol returns the stale fresh type instead of the widened type.
            //
            // EXCEPT: For merged interface+variable symbols (e.g., `interface Error` +
            // `declare var Error: ErrorConstructor`), get_type_of_symbol already cached
            // the INTERFACE type (which is the correct type for type-position usage like
            // `var e: Error`). The variable declaration's type annotation resolves to
            // the constructor/value type, so overwriting would corrupt the cached interface
            // type. Value-position resolution (`new Error()`) is handled separately by
            // `get_type_of_identifier` which has its own merged-symbol path.
            {
                let is_merged_interface = self.ctx.binder.get_symbol(sym_id).is_some_and(|s| {
                    s.flags & tsz_binder::symbol_flags::INTERFACE != 0
                        && s.flags
                            & (tsz_binder::symbol_flags::FUNCTION_SCOPED_VARIABLE
                                | tsz_binder::symbol_flags::BLOCK_SCOPED_VARIABLE)
                            != 0
                });
                if !is_merged_interface {
                    self.cache_symbol_type(sym_id, final_type);
                }
            }

            // FIX: Update node_types cache with the widened type
            self.ctx.node_types.insert(decl_idx.0, final_type);
            if !var_decl.name.is_none() {
                self.ctx.node_types.insert(var_decl.name.0, final_type);
            }

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
                    use crate::types::diagnostics::diagnostic_codes;
                    self.error_at_node_msg(
                        var_decl.name,
                        diagnostic_codes::VARIABLE_IMPLICITLY_HAS_AN_TYPE,
                        &[name, "any"],
                    );
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
        element_data: &tsz_parser::parser::node::BindingElementData,
    ) -> TypeId {
        use tsz_solver::type_queries::{
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
        use tsz_solver::{freshness, type_queries};

        // Excess property checks do not apply to type parameters (even with constraints).
        if type_queries::is_type_parameter(self.ctx.types, target) {
            return;
        }

        // Only check excess properties for FRESH object literals
        // This is the key TypeScript behavior:
        // - const p: Point = {x: 1, y: 2, z: 3}  // ERROR: 'z' is excess (fresh)
        // - const obj = {x: 1, y: 2, z: 3}; p = obj;  // OK: obj loses freshness
        //
        // IMPORTANT: Freshness is tracked on the TypeId itself.
        // This fixes the "Zombie Freshness" bug by keeping fresh vs non-fresh
        // object types distinct at the interner level.
        if !freshness::is_fresh_object_type(self.ctx.types, source) {
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
                } else {
                    // =============================================================
                    // NESTED OBJECT LITERAL EXCESS PROPERTY CHECKING
                    // =============================================================
                    // For nested object literals, recursively check for excess properties
                    // Example: { x: { y: 1, z: 2 } } where target is { x: { y: number } }
                    // should error on 'z' in the nested object literal
                    //
                    // CRITICAL FIX: For union targets, we must union all property types
                    // from all members. Using only the first member causes false positives.
                    // Example: type T = { x: { a: number } } | { x: { b: number } }
                    // Assigning { x: { b: 1 } } should NOT error on 'b'.
                    // =============================================================
                    let nested_target = if target_prop_types.len() == 1 {
                        target_prop_types[0]
                    } else {
                        self.ctx.types.union(target_prop_types.clone())
                    };

                    self.check_nested_object_literal_excess_properties(
                        source_prop.name,
                        Some(nested_target),
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
                if target_prop.is_none() {
                    let prop_name = self.ctx.types.resolve_atom(source_prop.name);
                    self.error_excess_property_at(&prop_name, target, idx);
                } else if let Some(target_prop) = target_prop {
                    // =============================================================
                    // NESTED OBJECT LITERAL EXCESS PROPERTY CHECKING
                    // =============================================================
                    // For nested object literals, recursively check for excess properties
                    self.check_nested_object_literal_excess_properties(
                        source_prop.name,
                        Some(target_prop.type_id),
                        idx,
                    );
                }
            }
        }
        // Note: Missing property checks are handled by solver's explain_failure
    }

    /// Check nested object literal properties for excess properties.
    ///
    /// This implements recursive excess property checking for nested object literals.
    /// For example, in `const p: { x: { y: number } } = { x: { y: 1, z: 2 } }`,
    /// the nested object literal `{ y: 1, z: 2 }` should be checked for excess property `z`.
    fn check_nested_object_literal_excess_properties(
        &mut self,
        prop_name: tsz_common::interner::Atom,
        target_prop_type: Option<TypeId>,
        obj_literal_idx: NodeIndex,
    ) {
        // Get the AST node for the object literal
        let Some(obj_node) = self.ctx.arena.get(obj_literal_idx) else {
            return;
        };

        let Some(obj_lit) = self.ctx.arena.get_literal_expr(obj_node) else {
            return;
        };

        // =============================================================
        // CRITICAL FIX: Iterate in reverse to handle duplicate properties
        // =============================================================
        // JavaScript/TypeScript behavior is "last property wins".
        // Example: const o = { x: { a: 1 }, x: { b: 1 } }
        // The runtime value of o.x is { b: 1 }, so we must check the last assignment.
        // =============================================================
        for &elem_idx in obj_lit.elements.nodes.iter().rev() {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            // Get the property name from this element
            let elem_prop_name = match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_property_assignment(elem_node)
                    .and_then(|prop| self.get_property_name(prop.name))
                    .map(|name| self.ctx.types.intern_string(&name)),
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_shorthand_property(elem_node)
                    .and_then(|prop| {
                        self.get_property_name(prop.name)
                            .map(|name| self.ctx.types.intern_string(&name))
                    }),
                _ => None,
            };

            // Skip if this property doesn't match the one we're looking for
            if elem_prop_name != Some(prop_name) {
                continue;
            }

            // Get the value expression for this property
            let value_idx = match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_property_assignment(elem_node)
                    .map(|prop| prop.initializer),
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                    // For shorthand properties, the value expression is the same as the property name expression
                    self.ctx
                        .arena
                        .get_shorthand_property(elem_node)
                        .map(|prop| prop.name)
                }
                _ => None,
            };

            let Some(value_idx) = value_idx else {
                continue;
            };

            // =============================================================
            // CRITICAL FIX: Handle parenthesized expressions
            // =============================================================
            // TypeScript treats parenthesized object literals as fresh.
            // Example: x: ({ a: 1 }) should be checked for excess properties.
            // We need to unwrap parentheses before checking the kind.
            // =============================================================
            let effective_value_idx = self.skip_parentheses(value_idx);
            let Some(value_node) = self.ctx.arena.get(effective_value_idx) else {
                continue;
            };

            if value_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                // Get the type of the nested object literal
                let nested_source_type = self.get_type_of_node(effective_value_idx);

                // Check if we have a target type for this property
                if let Some(nested_target_type) = target_prop_type {
                    // Recursively check the nested object literal for excess properties
                    self.check_object_literal_excess_properties(
                        nested_source_type,
                        nested_target_type,
                        effective_value_idx,
                    );
                }

                return; // Found the property, stop searching
            }
        }
    }

    /// Skip parentheses to get the effective expression node.
    ///
    /// This unwraps parenthesized expressions to get the underlying expression.
    /// Example: `({ a: 1 })` -> `{ a: 1 }` (OBJECT_LITERAL_EXPRESSION)
    fn skip_parentheses(&self, mut node_idx: NodeIndex) -> NodeIndex {
        while let Some(node) = self.ctx.arena.get(node_idx) {
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    node_idx = paren.expression;
                    continue;
                }
            }
            break;
        }
        node_idx
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
    ) -> tsz_solver::operations_property::PropertyAccessResult {
        // Ensure symbols are resolved in the environment
        self.ensure_application_symbols_resolved(object_type);

        // Route through QueryDatabase so repeated property lookups hit QueryCache.
        // This is especially important for hot paths like repeated `string[].push`
        // checks in class-heavy files.
        let result = self.ctx.types.resolve_property_access_with_options(
            object_type,
            prop_name,
            self.ctx.compiler_options.no_unchecked_indexed_access,
        );

        // If property not found and the type is an Application (e.g. Promise<number>),
        // the QueryCache's noop TypeResolver can't expand it. Evaluate the Application
        // to its structural form and retry property access on the expanded type.
        if matches!(
            result,
            tsz_solver::operations_property::PropertyAccessResult::PropertyNotFound { .. }
        ) && tsz_solver::is_generic_application(self.ctx.types, object_type)
        {
            let expanded = self.evaluate_application_type(object_type);
            if expanded != object_type && expanded != TypeId::ANY && expanded != TypeId::ERROR {
                return self.ctx.types.resolve_property_access_with_options(
                    expanded,
                    prop_name,
                    self.ctx.compiler_options.no_unchecked_indexed_access,
                );
            }
        }

        result
    }

    /// Check if an assignment target is a readonly property.
    /// Reports error TS2540 if trying to assign to a readonly property.
    /// Returns `true` if a readonly error was emitted (caller should skip further type checks).
    #[tracing::instrument(skip(self), fields(target_idx = target_idx.0))]
    pub(crate) fn check_readonly_assignment(
        &mut self,
        target_idx: NodeIndex,
        _expr_idx: NodeIndex,
    ) -> bool {
        let Some(target_node) = self.ctx.arena.get(target_idx) else {
            return false;
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
                        return false;
                    }

                    let index_type = self.get_type_of_node(access.name_or_argument);
                    if let Some(name) = self.get_readonly_element_access_name(
                        object_type,
                        access.name_or_argument,
                        index_type,
                    ) {
                        self.error_readonly_property_at(&name, target_idx);
                        return true;
                    }
                    // Check AST-level interface readonly for element access (obj["x"])
                    if let Some(name) = self.get_literal_string_from_node(access.name_or_argument) {
                        if let Some(type_name) =
                            self.get_declared_type_name_from_expression(access.expression)
                            && self.is_interface_property_readonly(&type_name, &name)
                        {
                            self.error_readonly_property_at(&name, target_idx);
                            return true;
                        }
                        // Also check namespace const exports via element access (M["x"])
                        if self.is_namespace_const_property(access.expression, &name) {
                            self.error_readonly_property_at(&name, target_idx);
                            return true;
                        }
                    }
                }
                return false;
            }
            _ => return false,
        }

        let Some(access) = self.ctx.arena.get_access_expr(target_node) else {
            return false;
        };

        // Get the property name
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return false;
        };

        // Check if this is a private identifier (method or field)
        // Private methods are always readonly
        if self.is_private_identifier_name(access.name_or_argument) {
            let prop_name = if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                ident.escaped_text.clone()
            } else {
                return false;
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
                    return true;
                }
            }
        }

        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };

        let prop_name = ident.escaped_text.clone();

        // Get the type of the object being accessed
        let obj_type = self.get_type_of_node(access.expression);

        // P1 fix: First check if the property exists on the type.
        // If the property doesn't exist, skip the readonly check - TS2339 will be
        // reported elsewhere. This matches tsc behavior which checks existence before
        // readonly status.
        use tsz_solver::operations_property::PropertyAccessResult;
        let property_result = self.resolve_property_access_with_env(obj_type, &prop_name);
        let property_exists = matches!(property_result, PropertyAccessResult::Success { .. });

        if !property_exists {
            // Property doesn't exist on this type - skip readonly check
            // The property existence error (TS2339) is reported elsewhere
            return false;
        }

        // Check if the property is readonly in the object type (solver types)
        if self.is_property_readonly(obj_type, &prop_name) {
            // Special case: readonly properties can be assigned in constructors
            // if the property is declared in the current class (not inherited)
            if self.is_readonly_assignment_allowed_in_constructor(&prop_name, access.expression) {
                return false;
            }

            self.error_readonly_property_at(&prop_name, target_idx);
            return true;
        }

        // Also check AST-level readonly on class properties
        // Get the class name from the object expression (for `c.ro`, get the type of `c`)
        if let Some(class_name) = self.get_class_name_from_expression(access.expression)
            && self.is_class_property_readonly(&class_name, &prop_name)
        {
            // Special case: readonly properties can be assigned in constructors
            // if the property is declared in the current class (not inherited)
            if self.is_readonly_assignment_allowed_in_constructor(&prop_name, access.expression) {
                return false;
            }

            self.error_readonly_property_at(&prop_name, target_idx);
            return true;
        }

        // Check AST-level readonly on interface properties
        // For `obj.x = 10` where `obj: I` and `interface I { readonly x: number }`
        if let Some(type_name) = self.get_declared_type_name_from_expression(access.expression)
            && self.is_interface_property_readonly(&type_name, &prop_name)
        {
            self.error_readonly_property_at(&prop_name, target_idx);
            return true;
        }

        // Check if the property is a const export from a namespace/module (TS2540).
        // For `M.x = 1` where `export const x = 0` in namespace M.
        if self.is_namespace_const_property(access.expression, &prop_name) {
            self.error_readonly_property_at(&prop_name, target_idx);
            return true;
        }

        false
    }

    /// Check if a property access refers to a `const` export from a namespace or module.
    ///
    /// For expressions like `M.x` where `namespace M { export const x = 0; }`,
    /// the property `x` should be treated as readonly (TS2540).
    fn is_namespace_const_property(&self, object_expr: NodeIndex, prop_name: &str) -> bool {
        self.is_namespace_const_property_inner(object_expr, prop_name)
            .unwrap_or(false)
    }

    fn is_namespace_const_property_inner(
        &self,
        object_expr: NodeIndex,
        prop_name: &str,
    ) -> Option<bool> {
        use tsz_binder::symbol_flags;

        // Resolve the object expression to a symbol (e.g., M -> namespace symbol)
        let sym_id = self.resolve_identifier_symbol(object_expr)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;

        // Must be a namespace/module symbol
        if symbol.flags & symbol_flags::MODULE == 0 {
            return Some(false);
        }

        // Look up the property in the namespace's exports
        let member_sym_id = symbol.exports.as_ref()?.get(prop_name)?;
        let member_symbol = self.ctx.binder.get_symbol(member_sym_id)?;

        // Check if the member is a block-scoped variable (const/let)
        if member_symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE == 0 {
            return Some(false);
        }

        // Check if its value declaration has the CONST flag
        let value_decl = member_symbol.value_declaration;
        if value_decl.is_none() {
            return Some(false);
        }

        let decl_node = self.ctx.arena.get(value_decl)?;
        let mut decl_flags = decl_node.flags as u32;

        // If CONST flag not directly on node, check parent (VariableDeclarationList)
        use tsz_parser::parser::flags::node_flags;
        if (decl_flags & node_flags::CONST) == 0 {
            if let Some(ext) = self.ctx.arena.get_extended(value_decl)
                && let Some(parent_node) = self.ctx.arena.get(ext.parent)
                && parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            {
                decl_flags |= parent_node.flags as u32;
            }
        }

        Some(decl_flags & node_flags::CONST != 0)
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
        use tsz_scanner::SyntaxKind;

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
        heritage_clauses: &Option<tsz_parser::parser::NodeList>,
        is_class_declaration: bool,
        class_type_param_names: &[String],
    ) {
        use tsz_parser::parser::syntax_kind_ext::HERITAGE_CLAUSE;
        use tsz_scanner::SyntaxKind;

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
                    // TS2314: Check if generic type is used without required type arguments.
                    // Skip for extends clauses — TypeScript allows omitting type arguments
                    // in class extends, defaulting all missing type params to `any`.
                    // E.g., `class C extends Array { }` is valid (Array<any>).
                    let has_type_args = self
                        .ctx
                        .arena
                        .get_expr_type_args(type_node)
                        .and_then(|e| e.type_arguments.as_ref())
                        .is_some_and(|args| !args.nodes.is_empty());
                    if !has_type_args && !is_extends_clause {
                        let required_count = self.count_required_type_params(heritage_sym);
                        if required_count > 0 {
                            if let Some(name) = self.heritage_name_text(expr_idx) {
                                self.error_generic_type_requires_type_arguments_at(
                                    &name,
                                    required_count,
                                    type_idx,
                                );
                            }
                        }
                    }

                    // TS2449/TS2450: Check if class/enum is used before its declaration
                    if is_extends_clause && is_class_declaration {
                        self.check_heritage_class_before_declaration(heritage_sym, expr_idx);
                    }

                    // Symbol was resolved - check if it represents a constructor type for extends clauses
                    if is_extends_clause {
                        use tsz_binder::symbol_flags;

                        // Note: Must resolve type aliases before checking flags and getting type
                        let mut visited_aliases = Vec::new();
                        let resolved_sym =
                            self.resolve_alias_symbol(heritage_sym, &mut visited_aliases);
                        let sym_to_check = resolved_sym.unwrap_or(heritage_sym);

                        let symbol_type = self.get_type_of_symbol(sym_to_check);
                        if let Some(symbol) = self.ctx.binder.get_symbol(sym_to_check) {
                            if symbol.flags & symbol_flags::MODULE != 0 {
                                if let Some(name) = self.heritage_name_text(expr_idx) {
                                    if is_class_declaration && is_extends_clause {
                                        self.error_namespace_used_as_value_at(&name, expr_idx);
                                    } else {
                                        self.error_namespace_used_as_type_at(&name, expr_idx);
                                    }
                                }
                                continue;
                            }
                        }

                        // TS2675: Check if base class has a private constructor (only for class declarations)
                        if is_class_declaration {
                            use crate::state::MemberAccessLevel;
                            if let Some(MemberAccessLevel::Private) =
                                self.class_constructor_access_level(sym_to_check)
                            {
                                // Check if we are inside the class that defines the private constructor
                                // Nested classes can extend a class with private constructor
                                let is_accessible =
                                    if let Some(ref enclosing) = self.ctx.enclosing_class {
                                        // Get the symbol of the enclosing class
                                        self.ctx
                                            .binder
                                            .get_node_symbol(enclosing.class_idx)
                                            .map(|enclosing_sym| enclosing_sym == sym_to_check)
                                            .unwrap_or(false)
                                    } else {
                                        false
                                    };

                                if !is_accessible {
                                    if let Some(name) = self.heritage_name_text(expr_idx) {
                                        use crate::types::diagnostics::{
                                            diagnostic_codes, diagnostic_messages, format_message,
                                        };
                                        let message = format_message(
                                            diagnostic_messages::CANNOT_EXTEND_A_CLASS_CLASS_CONSTRUCTOR_IS_MARKED_AS_PRIVATE,
                                            &[&name],
                                        );
                                        self.error_at_node(
                                            expr_idx,
                                            &message,
                                            diagnostic_codes::CANNOT_EXTEND_A_CLASS_CLASS_CONSTRUCTOR_IS_MARKED_AS_PRIVATE,
                                        );
                                    }
                                    // Continue to next type - no need to check further for this symbol
                                    continue;
                                }
                            }
                        }

                        // Check if this is ONLY an interface (not also a class or variable
                        // from declaration merging) - emit TS2689 instead of TS2507
                        // BUT only for class declarations, not interface declarations
                        // (interfaces can validly extend other interfaces)
                        // When a name is both an interface and a class (merged declaration),
                        // the class part can be validly extended, so don't emit TS2689.
                        // Also skip when the symbol has VARIABLE flag — built-in types
                        // like Array, Object, Promise have both interface and variable
                        // declarations (`interface Array` + `declare var Array: ArrayConstructor`),
                        // and the variable provides the constructor for extends.
                        let is_interface_only = self
                            .ctx
                            .binder
                            .get_symbol(sym_to_check)
                            .map(|s| {
                                (s.flags & symbol_flags::INTERFACE) != 0
                                    && (s.flags & symbol_flags::CLASS) == 0
                                    && (s.flags & symbol_flags::VARIABLE) == 0
                            })
                            .unwrap_or(false);

                        if is_interface_only && is_class_declaration {
                            // Emit TS2689: Cannot extend an interface (only for classes)
                            if let Some(name) = self.heritage_name_text(expr_idx) {
                                use crate::types::diagnostics::{
                                    diagnostic_codes, diagnostic_messages, format_message,
                                };
                                let message = format_message(
                                    diagnostic_messages::CANNOT_EXTEND_AN_INTERFACE_DID_YOU_MEAN_IMPLEMENTS,
                                    &[&name],
                                );
                                self.error_at_node(
                                    expr_idx,
                                    &message,
                                    diagnostic_codes::CANNOT_EXTEND_AN_INTERFACE_DID_YOU_MEAN_IMPLEMENTS,
                                );
                            }
                        } else if !is_interface_only
                            && is_class_declaration
                            && symbol_type != TypeId::ERROR  // Skip error recovery - don't emit TS2507 for unresolved types
                            && !self.is_constructor_type(symbol_type)
                            && !self.is_class_symbol(sym_to_check)
                            // Skip TS2507 for symbols with both INTERFACE and VARIABLE flags
                            // (built-in types like Array, Object, Promise) — the variable
                            // side provides the constructor even though the interface type
                            // doesn't have construct signatures.
                            && self
                                .ctx
                                .binder
                                .get_symbol(sym_to_check)
                                .map(|s| {
                                    !((s.flags & symbol_flags::INTERFACE) != 0
                                        && (s.flags & symbol_flags::VARIABLE) != 0)
                                })
                                .unwrap_or(true)
                        {
                            // For classes extending non-interfaces: emit TS2507 if not a constructor type
                            // For interfaces: don't check constructor types (interfaces can extend any interface)
                            if let Some(name) = self.heritage_name_text(expr_idx) {
                                use crate::types::diagnostics::{
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
                    //
                    // For property access expressions (e.g., `M1.A`, `"".bogus`),
                    // skip TS2304 — normal type checking will emit TS2339 if the property
                    // doesn't exist, matching tsc behavior.
                    if let Some(expr_node) = self.ctx.arena.get(expr_idx)
                        && expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    {
                        continue;
                    }

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
                                    use crate::types::diagnostics::{
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
                            // TS2422: For implements clauses referencing type parameters,
                            // emit "A class may only implement another class or interface"
                            if !is_extends_clause
                                && is_class_declaration
                                && class_type_param_names.contains(&name)
                            {
                                use crate::types::diagnostics::diagnostic_codes;
                                self.error_at_node(
                                    expr_idx,
                                    "A class may only implement another class or interface.",
                                    diagnostic_codes::AN_INTERFACE_CAN_ONLY_EXTEND_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYPES_WITH,
                                );
                                continue;
                            }
                            self.error_cannot_find_name_at(&name, expr_idx);
                        }
                    }
                }
            }
        }
    }

    /// TS2449/TS2450: Check if a class or enum referenced in a heritage clause
    /// is used before its declaration in the source order.
    fn check_heritage_class_before_declaration(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        usage_idx: NodeIndex,
    ) {
        use tsz_binder::symbol_flags;

        let Some(symbol) = self.ctx.binder.symbols.get(sym_id) else {
            return;
        };

        let is_class = symbol.flags & symbol_flags::CLASS != 0;
        let is_enum = symbol.flags & symbol_flags::REGULAR_ENUM != 0;
        if !is_class && !is_enum {
            return;
        }

        // Skip check for cross-file symbols (imported from another file).
        // Position comparison only makes sense within the same file.
        if symbol.decl_file_idx != u32::MAX || symbol.import_module.is_some() {
            return;
        }

        // Get the declaration position
        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else if let Some(&first_decl) = symbol.declarations.first() {
            first_decl
        } else {
            return;
        };

        let Some(usage_node) = self.ctx.arena.get(usage_idx) else {
            return;
        };
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return;
        };

        // Only flag if usage is before declaration in source order
        if usage_node.pos >= decl_node.pos {
            return;
        }

        use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        // Get the name from the usage site
        let name = self.heritage_name_text(usage_idx).unwrap_or_default();

        let (msg_template, code) = if is_class {
            (
                diagnostic_messages::CLASS_USED_BEFORE_ITS_DECLARATION,
                diagnostic_codes::CLASS_USED_BEFORE_ITS_DECLARATION,
            )
        } else {
            (
                diagnostic_messages::ENUM_USED_BEFORE_ITS_DECLARATION,
                diagnostic_codes::ENUM_USED_BEFORE_ITS_DECLARATION,
            )
        };
        let message = format_message(msg_template, &[&name]);
        self.error_at_node(usage_idx, &message, code);
    }

    /// Check a class declaration.
    pub(crate) fn check_class_declaration(&mut self, stmt_idx: NodeIndex) {
        use crate::class_inheritance::ClassInheritanceChecker;
        use crate::types::diagnostics::diagnostic_codes;
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
                diagnostic_codes::CLASS_NAME_CANNOT_BE,
            );
        }

        // Check if this is a declared class (ambient declaration)
        let is_declared = self.has_declare_modifier(&class.modifiers);

        // Check if this class is abstract
        let is_abstract_class = self.has_abstract_modifier(&class.modifiers);

        // Push type parameters BEFORE checking heritage clauses and abstract members
        // This allows heritage clauses and member checks to reference the class's type parameters
        let (_type_params, type_param_updates) = self.push_type_parameters(&class.type_parameters);

        // Collect class type parameter names for TS2302 checking in static members
        let class_type_param_names: Vec<String> = type_param_updates
            .iter()
            .map(|(name, _)| name.clone())
            .collect();

        // Check for unused type parameters (TS6133)
        self.check_unused_type_params(&class.type_parameters, stmt_idx);

        // Check heritage clauses for unresolved names (TS2304)
        // Must be checked AFTER type parameters are pushed so heritage can reference type params
        self.check_heritage_clauses_for_unresolved_names(
            &class.heritage_clauses,
            true,
            &class_type_param_names,
        );

        // Check for abstract members in non-abstract class (error 1253),
        // private identifiers in ambient classes (error 2819),
        // and private identifiers when targeting ES5 or lower (error 18028)
        for &member_idx in &class.members.nodes {
            if let Some(member_node) = self.ctx.arena.get(member_idx) {
                // Get member name for private identifier checks
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

                // Check if member has a private identifier name
                let is_private_identifier = member_name_idx
                    .filter(|idx| !idx.is_none())
                    .and_then(|idx| self.ctx.arena.get(idx))
                    .map(|node| node.kind == tsz_scanner::SyntaxKind::PrivateIdentifier as u16)
                    .unwrap_or(false);

                if is_private_identifier {
                    use crate::context::ScriptTarget;
                    use crate::types::diagnostics::diagnostic_messages;

                    // TS18028: Check for private identifiers when targeting ES5 or lower
                    let is_es5_or_lower = matches!(
                        self.ctx.compiler_options.target,
                        ScriptTarget::ES3 | ScriptTarget::ES5
                    );
                    if is_es5_or_lower {
                        self.error_at_node(
                            member_name_idx.unwrap(),
                            diagnostic_messages::PRIVATE_IDENTIFIERS_ARE_ONLY_AVAILABLE_WHEN_TARGETING_ECMASCRIPT_2015_AND_HIGHER,
                            diagnostic_codes::PRIVATE_IDENTIFIERS_ARE_ONLY_AVAILABLE_WHEN_TARGETING_ECMASCRIPT_2015_AND_HIGHER,
                        );
                    }

                    // TS18019: Check for private identifiers in ambient classes
                    if is_declared {
                        self.error_at_node(
                            member_name_idx.unwrap(),
                            diagnostic_messages::MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER,
                            diagnostic_codes::MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER,
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
                            diagnostic_codes::ABSTRACT_PROPERTIES_CAN_ONLY_APPEAR_WITHIN_AN_ABSTRACT_CLASS,
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
                cached_instance_this_type: None,
                type_param_names: class_type_param_names,
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

        // Check that class properties are compatible with index signatures (TS2411)
        // Get the class instance type (not constructor type) to access instance index signatures
        let class_instance_type = self.get_class_instance_type(stmt_idx, class);
        self.check_index_signature_compatibility(&class.members.nodes, class_instance_type);

        // Check for decorator-related global types (TS2318)
        // When experimentalDecorators is enabled and a method/accessor has decorators,
        // TypedPropertyDescriptor must be available
        self.check_decorator_global_types(&class.members.nodes);

        // Restore previous enclosing class
        self.ctx.enclosing_class = prev_enclosing_class;

        self.pop_type_parameters(type_param_updates);
    }

    pub(crate) fn check_class_expression(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) {
        let (_type_params, type_param_updates) = self.push_type_parameters(&class.type_parameters);

        let class_type_param_names: Vec<String> = type_param_updates
            .iter()
            .map(|(name, _)| name.clone())
            .collect();

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
            cached_instance_this_type: None,
            type_param_names: class_type_param_names,
        });

        for &member_idx in &class.members.nodes {
            self.check_class_member(member_idx);
        }

        // Check strict property initialization (TS2564) for class expressions
        // Class expressions should have the same property initialization checks as class declarations
        self.check_property_initialization(class_idx, class, false, is_abstract_class);

        // Check for decorator-related global types (TS2318)
        self.check_decorator_global_types(&class.members.nodes);

        self.ctx.enclosing_class = prev_enclosing_class;

        self.pop_type_parameters(type_param_updates);
    }

    pub(crate) fn check_property_initialization(
        &mut self,
        _class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
        is_declared: bool,
        _is_abstract: bool,
    ) {
        use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};

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
            use crate::types::diagnostics::format_message;

            // Use TS2524 if there's a constructor (definite assignment analysis)
            // Use TS2564 if no constructor (just missing initializer)
            let (message, code) = if constructor_body.is_some() {
                (
                    diagnostic_messages::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
                    diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
                )
            } else {
                (
                    diagnostic_messages::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
                    diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
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
        prop: &tsz_parser::parser::node::PropertyDeclData,
        is_derived_class: bool,
    ) -> bool {
        use tsz_scanner::SyntaxKind;

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

    /// Check for decorator-related global types (TS2318).
    ///
    /// When experimentalDecorators is enabled and a method or accessor has decorators,
    /// TypeScript requires the `TypedPropertyDescriptor` type to be available.
    /// If it's not available (e.g., with noLib), we emit TS2318.
    pub(crate) fn check_decorator_global_types(&mut self, members: &[NodeIndex]) {
        // Only check if experimentalDecorators is enabled
        if !self.ctx.compiler_options.experimental_decorators {
            return;
        }

        // Check if any method or accessor has decorators
        let mut has_method_or_accessor_decorator = false;
        for &member_idx in members {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            let modifiers = match node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(node)
                    .and_then(|m| m.modifiers.as_ref()),
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    self.ctx
                        .arena
                        .get_accessor(node)
                        .and_then(|a| a.modifiers.as_ref())
                }
                _ => continue,
            };

            if let Some(mods) = modifiers {
                for &mod_idx in &mods.nodes {
                    if let Some(mod_node) = self.ctx.arena.get(mod_idx)
                        && mod_node.kind == syntax_kind_ext::DECORATOR
                    {
                        has_method_or_accessor_decorator = true;
                        break;
                    }
                }
            }
            if has_method_or_accessor_decorator {
                break;
            }
        }

        if !has_method_or_accessor_decorator {
            return;
        }

        // Check if TypedPropertyDescriptor is available
        let type_name = "TypedPropertyDescriptor";
        if self.ctx.has_name_in_lib(type_name) {
            return; // Type is available from lib
        }
        if self.ctx.binder.file_locals.has(type_name) {
            return; // Type is declared locally
        }

        // TypedPropertyDescriptor is not available - emit TS2318
        // TSC emits this error twice for method decorators
        use tsz_binder::lib_loader::emit_error_global_type_missing;
        let diag = emit_error_global_type_missing(type_name, self.ctx.file_name.clone(), 0, 0);
        self.ctx.push_diagnostic(diag.clone());
        self.ctx.push_diagnostic(diag);
    }

    /// Check triple-slash reference directives and emit TS6053 for missing files.
    ///
    /// Validates `/// <reference path="..." />` directives in TypeScript source files.
    /// If a referenced file doesn't exist, emits error 6053.
    fn check_triple_slash_references(&mut self, file_name: &str, source_text: &str) {
        use crate::triple_slash_validator::{extract_reference_paths, validate_reference_path};
        use std::collections::HashSet;
        use std::path::Path;

        let references = extract_reference_paths(source_text);
        if references.is_empty() {
            return;
        }

        let source_path = Path::new(file_name);

        let mut known_files: HashSet<String> = HashSet::new();
        if let Some(arenas) = self.ctx.all_arenas.as_ref() {
            for arena in arenas.iter() {
                for source_file in &arena.source_files {
                    known_files.insert(source_file.file_name.clone());
                }
            }
        } else {
            for source_file in &self.ctx.arena.source_files {
                known_files.insert(source_file.file_name.clone());
            }
        }

        let has_virtual_reference = |reference_path: &str| {
            let base = source_path.parent().unwrap_or_else(|| Path::new(""));
            let mut candidates = Vec::new();
            candidates.push(base.join(reference_path));
            if !reference_path.contains('.') {
                for ext in [".ts", ".tsx", ".d.ts"] {
                    candidates.push(base.join(format!("{}{}", reference_path, ext)));
                }
            }
            let reference_stem = Path::new(reference_path)
                .file_stem()
                .and_then(|stem| stem.to_str());
            candidates.iter().any(|candidate| {
                let candidate_str = candidate.to_string_lossy();
                if known_files.contains(candidate_str.as_ref()) {
                    return true;
                }
                if known_files
                    .iter()
                    .any(|known| known.ends_with(candidate_str.as_ref()))
                {
                    return true;
                }
                let candidate_file = Path::new(candidate_str.as_ref())
                    .file_name()
                    .and_then(|name| name.to_str());
                if let Some(candidate_file) = candidate_file {
                    return known_files.iter().any(|known| {
                        Path::new(known).file_name().and_then(|name| name.to_str())
                            == Some(candidate_file)
                    });
                }
                if let Some(reference_stem) = reference_stem {
                    return known_files.iter().any(|known| {
                        Path::new(known).file_stem().and_then(|stem| stem.to_str())
                            == Some(reference_stem)
                    });
                }
                false
            })
        };

        for (reference_path, line_num) in references {
            if !has_virtual_reference(&reference_path)
                && !validate_reference_path(source_path, &reference_path)
            {
                // Calculate the position of the error (start of the line)
                let mut pos = 0u32;
                for (idx, _) in source_text.lines().enumerate() {
                    if idx == line_num {
                        break;
                    }
                    pos += source_text
                        .lines()
                        .nth(idx)
                        .map(|l| l.len() + 1)
                        .unwrap_or(0) as u32;
                }

                // Find the actual directive on the line to get accurate position
                if let Some(line) = source_text.lines().nth(line_num) {
                    if let Some(directive_start) = line.find("///") {
                        pos += directive_start as u32;
                    }
                }

                let length = source_text
                    .lines()
                    .nth(line_num)
                    .map(|l| l.len() as u32)
                    .unwrap_or(0);

                use crate::types::diagnostics::{diagnostic_codes, format_message};
                let message = format_message("File '{0}' not found.", &[&reference_path]);
                self.emit_error_at(pos, length, &message, diagnostic_codes::FILE_NOT_FOUND);
            }
        }
    }
}
