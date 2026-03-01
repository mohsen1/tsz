//! Variable declaration and destructuring checking.
//!
//! For-in / for-of loop variable checking is in `for_loop.rs`.

use crate::query_boundaries::state::checking as query;
use crate::state::CheckerState;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn find_circular_reference_in_type_node(
        &self,
        type_idx: NodeIndex,
        target_sym: SymbolId,
        in_lazy_context: bool,
    ) -> Option<NodeIndex> {
        self.find_circular_reference_impl(type_idx, target_sym, in_lazy_context, true)
    }

    /// `follow_aliases`: whether to follow type references to type alias
    /// bodies. Only one level of alias following is performed to prevent
    /// false positives from multi-step chains through structural wrapping.
    fn find_circular_reference_impl(
        &self,
        type_idx: NodeIndex,
        target_sym: SymbolId,
        in_lazy_context: bool,
        follow_aliases: bool,
    ) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(type_idx)?;

        // Function types are safe boundaries (recursion always allowed)
        if matches!(
            node.kind,
            syntax_kind_ext::FUNCTION_TYPE | syntax_kind_ext::CONSTRUCTOR_TYPE
        ) {
            return None;
        }

        // Type literals and mapped types introduce a lazy context where "bare" recursion is allowed
        let is_lazy_boundary = matches!(
            node.kind,
            syntax_kind_ext::TYPE_LITERAL | syntax_kind_ext::MAPPED_TYPE
        );
        let current_lazy = in_lazy_context || is_lazy_boundary;

        // Follow type references to type aliases to detect transitive circularity.
        // E.g., `var x: T5[]` where `type T5 = typeof x` — the type reference T5
        // needs to be followed to its body to discover the `typeof x` query.
        // Only follow one level of alias indirection to avoid false positives
        // from multi-step chains through structural wrapping (generic applications).
        if follow_aliases
            && node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.ctx.arena.get_type_ref(node)
        {
            let ref_sym = self
                .ctx
                .binder
                .get_node_symbol(type_ref.type_name)
                .or_else(|| {
                    self.ctx
                        .binder
                        .resolve_identifier(self.ctx.arena, type_ref.type_name)
                });
            if let Some(sym_id) = ref_sym {
                let is_type_alias = self
                    .ctx
                    .binder
                    .get_symbol(sym_id)
                    .is_some_and(|s| s.flags & tsz_binder::symbol_flags::TYPE_ALIAS != 0);
                if is_type_alias
                    && let Some(decls) = self
                        .ctx
                        .binder
                        .get_symbol(sym_id)
                        .map(|s| s.declarations.clone())
                {
                    for &decl_idx in &decls {
                        if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                            && let Some(alias) = self.ctx.arena.get_type_alias(decl_node)
                            && alias.type_node.is_some()
                        {
                            // Don't follow further aliases from within this body
                            if let Some(found) = self.find_circular_reference_impl(
                                alias.type_node,
                                target_sym,
                                current_lazy,
                                false,
                            ) {
                                return Some(found);
                            }
                        }
                    }
                }
            }
        }

        if node.kind == syntax_kind_ext::TYPE_QUERY {
            if let Some(query) = self.ctx.arena.get_type_query(node) {
                // Check if the query references the target symbol
                // We need to know if it's a "bare" reference or a property access
                let expr_node = self.ctx.arena.get(query.expr_name)?;

                let is_bare_identifier =
                    expr_node.kind == tsz_scanner::SyntaxKind::Identifier as u16;

                // Extract the symbol referenced by the query
                let mut referenced_sym = None;
                let mut error_node = query.expr_name;

                if is_bare_identifier {
                    referenced_sym =
                        self.ctx
                            .binder
                            .get_node_symbol(query.expr_name)
                            .or_else(|| {
                                self.ctx
                                    .binder
                                    .resolve_identifier(self.ctx.arena, query.expr_name)
                            });
                } else if expr_node.kind == syntax_kind_ext::QUALIFIED_NAME {
                    if let Some(qn) = self.ctx.arena.get_qualified_name(expr_node) {
                        // Check left side
                        if let Some(node) = self.ctx.arena.get(qn.left)
                            && node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                        {
                            referenced_sym =
                                self.ctx.binder.get_node_symbol(qn.left).or_else(|| {
                                    self.ctx.binder.resolve_identifier(self.ctx.arena, qn.left)
                                });
                            error_node = qn.left;
                        }
                    }
                } else if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    && let Some(access) = self.ctx.arena.get_access_expr(expr_node)
                {
                    // Check expression
                    if let Some(node) = self.ctx.arena.get(access.expression)
                        && node.kind == tsz_scanner::SyntaxKind::Identifier as u16
                    {
                        referenced_sym = self
                            .ctx
                            .binder
                            .get_node_symbol(access.expression)
                            .or_else(|| {
                                self.ctx
                                    .binder
                                    .resolve_identifier(self.ctx.arena, access.expression)
                            });
                        error_node = access.expression;
                    }
                }

                if let Some(sym) = referenced_sym
                    && sym == target_sym
                {
                    // Found a reference to the target symbol!
                    // If we are in a lazy context AND it's a bare identifier, it's safe.
                    if current_lazy && is_bare_identifier {
                        return None;
                    }
                    return Some(error_node);
                }

                // Also check type arguments if any (always recursive)
                if let Some(ref args) = query.type_arguments {
                    for &arg_idx in &args.nodes {
                        if let Some(found) = self.find_circular_reference_impl(
                            arg_idx,
                            target_sym,
                            current_lazy,
                            follow_aliases,
                        ) {
                            return Some(found);
                        }
                    }
                }
            }
            return None;
        }

        // Explicitly recurse into type annotations of members, as generic get_children might miss them
        if matches!(
            node.kind,
            syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR
        ) {
            if let Some(accessor) = self.ctx.arena.get_accessor(node)
                && accessor.type_annotation.is_some()
                && let Some(found) = self.find_circular_reference_impl(
                    accessor.type_annotation,
                    target_sym,
                    current_lazy,
                    follow_aliases,
                )
            {
                return Some(found);
            }
        } else if matches!(
            node.kind,
            syntax_kind_ext::PROPERTY_SIGNATURE | syntax_kind_ext::PROPERTY_DECLARATION
        ) && let Some(prop) = self.ctx.arena.get_property_decl(node)
            && prop.type_annotation.is_some()
            && let Some(found) = self.find_circular_reference_impl(
                prop.type_annotation,
                target_sym,
                current_lazy,
                follow_aliases,
            )
        {
            return Some(found);
        }

        // Recursive descent
        for child in self.ctx.arena.get_children(type_idx) {
            if let Some(found) =
                self.find_circular_reference_impl(child, target_sym, current_lazy, follow_aliases)
            {
                return Some(found);
            }
        }

        None
    }

    /// Check a single variable declaration.
    #[tracing::instrument(level = "trace", skip(self), fields(decl_idx = ?decl_idx))]
    pub(crate) fn check_variable_declaration(&mut self, decl_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return;
        };

        let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) else {
            return;
        };

        // TS1155: Check if const declarations must be initialized
        // Skip check for ambient declarations (e.g., declare const x;)
        // Skip when file has real syntax errors — the parse error is sufficient.
        if !self.is_ambient_declaration(decl_idx) && !self.ctx.has_real_syntax_errors {
            // Get the parent node (VARIABLE_DECLARATION_LIST) to check flags
            if let Some(ext) = self.ctx.arena.get_extended(decl_idx)
                && let Some(parent_node) = self.ctx.arena.get(ext.parent)
            {
                use tsz_parser::parser::node_flags;
                let is_const = (parent_node.flags & node_flags::CONST as u16) != 0;

                if is_const && var_decl.initializer.is_none() {
                    // Skip for destructuring patterns - they get TS1182 from the parser
                    let is_binding_pattern =
                        if let Some(name_node) = self.ctx.arena.get(var_decl.name) {
                            name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                        } else {
                            false
                        };

                    // Check if this is in a for-in or for-of loop (allowed)
                    let is_in_for_loop =
                        if let Some(parent_ext) = self.ctx.arena.get_extended(ext.parent) {
                            if let Some(gp_node) = self.ctx.arena.get(parent_ext.parent) {
                                gp_node.kind == syntax_kind_ext::FOR_IN_STATEMENT
                                    || gp_node.kind == syntax_kind_ext::FOR_OF_STATEMENT
                            } else {
                                false
                            }
                        } else {
                            false
                        };

                    if !is_in_for_loop && !is_binding_pattern {
                        self.ctx.error(
                            node.pos,
                            node.end - node.pos,
                            "'const' declarations must be initialized.".to_string(),
                            1155,
                        );
                    }
                }
            }
        }

        // TS1255/TS1263/TS1264: Definite assignment assertion checks on variables
        if var_decl.exclamation_token {
            // TS1255: ! is not permitted in ambient context (declare let/var/const)
            if self.is_ambient_declaration(decl_idx) {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    var_decl.name,
                    diagnostic_messages::A_DEFINITE_ASSIGNMENT_ASSERTION_IS_NOT_PERMITTED_IN_THIS_CONTEXT,
                    diagnostic_codes::A_DEFINITE_ASSIGNMENT_ASSERTION_IS_NOT_PERMITTED_IN_THIS_CONTEXT,
                );
            }

            // TS1263: ! with initializer is contradictory
            if var_decl.initializer.is_some() {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    var_decl.name,
                    diagnostic_messages::DECLARATIONS_WITH_INITIALIZERS_CANNOT_ALSO_HAVE_DEFINITE_ASSIGNMENT_ASSERTIONS,
                    diagnostic_codes::DECLARATIONS_WITH_INITIALIZERS_CANNOT_ALSO_HAVE_DEFINITE_ASSIGNMENT_ASSERTIONS,
                );
            }

            // TS1264: ! without type annotation is meaningless
            if var_decl.type_annotation.is_none() {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                    var_decl.name,
                    diagnostic_messages::DECLARATIONS_WITH_DEFINITE_ASSIGNMENT_ASSERTIONS_MUST_ALSO_HAVE_TYPE_ANNOTATIONS,
                    diagnostic_codes::DECLARATIONS_WITH_DEFINITE_ASSIGNMENT_ASSERTIONS_MUST_ALSO_HAVE_TYPE_ANNOTATIONS,
                );
            }
        }

        // TS2481: Check var declarations that shadow block-scoped variables.
        // When a `var` declaration appears in a scope where a `let`/`const` with the same
        // name exists in an enclosing block (but not at function/module/source-file level),
        // the var initialization would write to the outer hoisted variable while the
        // block-scoped binding shadows it — this is a runtime SyntaxError.
        self.check_var_declared_names_not_shadowed(decl_idx, var_decl);

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

        // TS1212/1213/1214: Identifier expected. '{0}' is a reserved word in strict mode.
        // Check if variable name is a strict-mode reserved word used in strict context.

        let mut is_ambient = self.ctx.is_declaration_file();
        if !is_ambient {
            let mut current = decl_idx;
            let mut guard = 0;
            while current.is_some() {
                guard += 1;
                if guard > 256 {
                    break;
                }
                if let Some(node) = self.ctx.arena.get(current) {
                    if node.kind == tsz_parser::parser::syntax_kind_ext::MODULE_DECLARATION {
                        if let Some(module) = self.ctx.arena.get_module(node)
                            && self.ctx.arena.has_modifier(
                                &module.modifiers,
                                tsz_scanner::SyntaxKind::DeclareKeyword,
                            )
                        {
                            is_ambient = true;
                            break;
                        }
                    } else if node.kind == tsz_parser::parser::syntax_kind_ext::VARIABLE_STATEMENT {
                        if let Some(var_stmt) = self.ctx.arena.get_variable(node)
                            && self.ctx.arena.has_modifier(
                                &var_stmt.modifiers,
                                tsz_scanner::SyntaxKind::DeclareKeyword,
                            )
                        {
                            is_ambient = true;
                            break;
                        }
                    } else if node.kind == tsz_parser::parser::syntax_kind_ext::SOURCE_FILE {
                        break;
                    }
                }
                if let Some(ext) = self.ctx.arena.get_extended(current) {
                    current = ext.parent;
                } else {
                    break;
                }
            }
        }
        if !is_ambient
            && self.is_strict_mode_for_node(var_decl.name)
            && let Some(ref name) = var_name
            && crate::state_checking::is_strict_mode_reserved_name(name)
        {
            self.emit_strict_mode_reserved_word_error(var_decl.name, name, true);
        }
        // TS1100: `eval` or `arguments` used as a variable name in strict mode.
        // In class bodies, `arguments` is reported as TS1210 instead, so only
        // emit TS1100 for `eval` there (not `arguments`).
        if !is_ambient
            && self.is_strict_mode_for_node(var_decl.name)
            && let Some(ref name) = var_name
            && crate::state_checking::is_eval_or_arguments(name)
            && !(self.ctx.enclosing_class.is_some() && name.as_str() == "arguments")
        {
            self.emit_eval_or_arguments_strict_mode_error(var_decl.name, name);
        }

        // TS2480: 'let' is not allowed to be used as a name in 'let' or 'const' declarations.
        if let Some(ref name) = var_name
            && name == "let"
            && let Some(ext) = self.ctx.arena.get_extended(decl_idx)
            && let Some(parent_node) = self.ctx.arena.get(ext.parent)
        {
            use tsz_parser::parser::node_flags;
            let parent_flags = parent_node.flags as u32;
            if parent_flags & node_flags::LET != 0 || parent_flags & node_flags::CONST != 0 {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                        var_decl.name,
                        diagnostic_messages::LET_IS_NOT_ALLOWED_TO_BE_USED_AS_A_NAME_IN_LET_OR_CONST_DECLARATIONS,
                        diagnostic_codes::LET_IS_NOT_ALLOWED_TO_BE_USED_AS_A_NAME_IN_LET_OR_CONST_DECLARATIONS,
                    );
            }
        }

        // TS2397: Declaration name conflicts with built-in global identifier.
        // tsc emits TS2397 when a variable is declared with the name `undefined` or `globalThis`.
        // `globalThis` only conflicts in script files (non-modules), since module-scoped
        // declarations don't pollute the global scope.
        if let Some(ref name) = var_name {
            let should_emit = if name == "globalThis" {
                !self.ctx.binder.is_external_module()
            } else {
                name == "undefined"
            };
            if should_emit {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
                let message = format_message(
                    diagnostic_messages::DECLARATION_NAME_CONFLICTS_WITH_BUILT_IN_GLOBAL_IDENTIFIER,
                    &[name],
                );
                self.error_at_node(
                    var_decl.name,
                    &message,
                    diagnostic_codes::DECLARATION_NAME_CONFLICTS_WITH_BUILT_IN_GLOBAL_IDENTIFIER,
                );
            }
        }

        let is_catch_variable = self.is_catch_clause_variable_declaration(decl_idx);

        // TS1039/TS1254: Check initializers in ambient contexts
        if var_decl.initializer.is_some() && self.is_ambient_declaration(decl_idx) {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            let is_const = self.is_const_variable_declaration(decl_idx);
            if is_const && var_decl.type_annotation.is_none() {
                // Ambient const without type annotation: only string/numeric literals allowed
                if !self.is_valid_ambient_const_initializer(var_decl.initializer) {
                    self.error_at_node(
                        var_decl.initializer,
                        diagnostic_messages::A_CONST_INITIALIZER_IN_AN_AMBIENT_CONTEXT_MUST_BE_A_STRING_OR_NUMERIC_LITERAL_OR,
                        diagnostic_codes::A_CONST_INITIALIZER_IN_AN_AMBIENT_CONTEXT_MUST_BE_A_STRING_OR_NUMERIC_LITERAL_OR,
                    );
                }
            } else {
                // Non-const or const with type annotation
                self.error_at_node(
                    var_decl.initializer,
                    diagnostic_messages::INITIALIZERS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS,
                    diagnostic_codes::INITIALIZERS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS,
                );
            }
        }

        let compute_final_type = |checker: &mut CheckerState| -> TypeId {
            let mut has_type_annotation = var_decl.type_annotation.is_some();
            let mut declared_type = if has_type_annotation {
                // Check for undefined type names in nested types (e.g., function type parameters)
                // Skip top-level TYPE_REFERENCE to avoid duplicates with get_type_from_type_node
                checker.check_type_for_missing_names_skip_top_level_ref(var_decl.type_annotation);
                checker.check_type_for_parameter_properties(var_decl.type_annotation);
                let type_id = checker.get_type_from_type_node(var_decl.type_annotation);

                // TS1196: Catch clause variable type annotation must be 'any' or 'unknown'
                if is_catch_variable
                    && type_id != TypeId::ANY
                    && type_id != TypeId::UNKNOWN
                    && !checker.type_contains_error(type_id)
                {
                    use crate::diagnostics::diagnostic_codes;
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
                if var_decl.initializer.is_some() {
                    // Evaluate the declared type to resolve conditionals before using as context.
                    // This ensures types like `type C = string extends string ? "yes" : "no"`
                    // provide proper contextual typing for literals, preventing them from widening to string.
                    // Only evaluate conditional/mapped/index access types - NOT type aliases or interface
                    // references, as evaluating those can change their representation and break variance checking.
                    let evaluated_type = if declared_type != TypeId::ANY {
                        let should_evaluate =
                            crate::query_boundaries::state::should_evaluate_contextual_declared_type(
                                checker.ctx.types,
                                declared_type,
                            );
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

                    // Check assignability (skip for 'any' since anything is assignable to any,
                    // and skip for TypeId::ERROR since the type annotation failed to resolve).
                    // Note: we intentionally do NOT use type_contains_error() here because it
                    // recursively traverses all method/property types — interfaces like String
                    // have methods that reference unresolved lib types (e.g. Intl.CollatorOptions),
                    // causing type_contains_error to return true even though the declared type
                    // itself (String interface) is perfectly valid for assignability checking.
                    if declared_type != TypeId::ANY && declared_type != TypeId::ERROR {
                        if let Some((source_level, target_level)) =
                            checker.constructor_accessibility_mismatch_for_var_decl(var_decl)
                        {
                            checker.error_constructor_accessibility_not_assignable(
                                init_type,
                                declared_type,
                                source_level,
                                target_level,
                                decl_idx,
                            );
                        } else if is_destructuring {
                            // For destructuring patterns, keep emitting a generic TS2322 error
                            // instead of detailed property mismatch errors (TS2326-style detail).
                            let _ = checker.check_assignable_or_report_generic_at(
                                init_type,
                                declared_type,
                                var_decl.initializer,
                                decl_idx,
                            );
                        } else if checker.try_discriminated_union_excess_check(
                            init_type,
                            declared_type,
                            var_decl.initializer,
                        ) {
                            // Discriminated union excess property check handled the error.
                            // tsc reports TS2353 against the narrowed member instead of
                            // a generic TS2322 for these cases.
                        } else if checker.check_assignable_or_report_at(
                            init_type,
                            declared_type,
                            var_decl.initializer,
                            decl_idx,
                        ) {
                            // assignable, keep going to excess-property checks
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
                // `const k: unique symbol = Symbol()` — create a proper UniqueSymbol
                // type using the variable's binder symbol as identity.
                if declared_type == TypeId::SYMBOL
                    && checker.is_const_variable_declaration(decl_idx)
                    && checker.is_unique_symbol_type_annotation(var_decl.type_annotation)
                {
                    if let Some(sym_id) = checker.ctx.binder.get_node_symbol(decl_idx) {
                        return checker
                            .ctx
                            .types
                            .unique_symbol(tsz_solver::SymbolRef(sym_id.0));
                    }
                }
                // Type annotation determines the final type
                return declared_type;
            }

            // No type annotation - infer from initializer
            if var_decl.initializer.is_some() {
                // Clear cache for closure initializers so TS7006 is properly emitted.
                // During build_type_environment, closures are typed without contextual info
                // and TS7006 is deferred. Now that we're in the checking phase, re-evaluate
                // so TS7006 can fire for closures that truly lack contextual types.
                if let Some(init_node) = checker.ctx.arena.get(var_decl.initializer)
                    && matches!(
                        init_node.kind,
                        syntax_kind_ext::FUNCTION_EXPRESSION | syntax_kind_ext::ARROW_FUNCTION
                    )
                {
                    checker.clear_type_cache_recursive(var_decl.initializer);
                }
                let mut init_type = checker.get_type_of_node(var_decl.initializer);

                // TypeScript treats unannotated empty-array declaration initializers
                // (`let/var/const x = []`) as evolving-any arrays for subsequent writes.
                // Keep expression-level `[]` behavior unchanged by only applying this to
                // direct declaration initializers.
                let init_is_direct_empty_array = checker
                    .ctx
                    .arena
                    .get(var_decl.initializer)
                    .is_some_and(|init_node| {
                        init_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                            && checker
                                .ctx
                                .arena
                                .get_literal_expr(init_node)
                                .is_some_and(|lit| lit.elements.nodes.is_empty())
                    });
                if init_is_direct_empty_array
                    && query::array_element_type(checker.ctx.types, init_type)
                        == Some(TypeId::NEVER)
                {
                    init_type = checker.ctx.types.factory().array(TypeId::ANY);
                }

                // When strictNullChecks is off, undefined and null widen to any
                // (TypeScript treats `var x = undefined` as `any` without strict)
                if !checker.ctx.strict_null_checks()
                    && (init_type == TypeId::UNDEFINED || init_type == TypeId::NULL)
                {
                    return TypeId::ANY;
                }

                // Under noImplicitAny, mutable unannotated bindings initialized with
                // `undefined`/`null` should behave like evolving-any variables so later
                // assignments don't produce TS2322 (TypeScript reports implicit-any diagnostics).
                if checker.ctx.no_implicit_any()
                    && !checker.is_const_variable_declaration(decl_idx)
                    && var_decl.type_annotation.is_none()
                    && (init_type == TypeId::UNDEFINED || init_type == TypeId::NULL)
                {
                    return TypeId::ANY;
                }

                // Note: Freshness is tracked by the TypeId flags.
                // Fresh vs non-fresh object types are interned distinctly.

                if checker.is_const_variable_declaration(decl_idx) {
                    if let Some(literal_type) =
                        checker.literal_type_from_initializer(var_decl.initializer)
                    {
                        return literal_type;
                    }
                    // `const k = Symbol()` — infer unique symbol type.
                    // In TypeScript, const declarations initialized with Symbol() get
                    // a unique symbol type (typeof k), not the general `symbol` type.
                    if checker.is_symbol_call_initializer(var_decl.initializer) {
                        if let Some(sym_id) = checker.ctx.binder.get_node_symbol(decl_idx) {
                            return checker
                                .ctx
                                .types
                                .unique_symbol(tsz_solver::SymbolRef(sym_id.0));
                        }
                    }
                    return init_type;
                }

                // Only widen when the initializer is a "fresh" literal expression
                // (direct literal in source code). Types from variable references,
                // narrowing, or computed expressions are "non-fresh" and NOT widened.
                // EXCEPTION: Enum member types are always widened for mutable bindings.
                let is_enum_member = checker.is_enum_member_type_for_widening(init_type);
                let widened = if is_enum_member
                    || checker.is_fresh_literal_expression(var_decl.initializer)
                {
                    checker.widen_initializer_type_for_mutable_binding(init_type)
                } else {
                    init_type
                };
                // When strictNullChecks is off, undefined and null widen to any
                // regardless of freshness (this applies to destructured bindings too)
                if !checker.ctx.strict_null_checks()
                    && query::is_only_null_or_undefined(checker.ctx.types, widened)
                {
                    TypeId::ANY
                } else {
                    widened
                }
            } else {
                // For for-in/for-of loop variables, the element type has already been cached
                // by assign_for_in_of_initializer_types. Use that instead of defaulting to any.
                if let Some(sym_id) = checker.ctx.binder.get_node_symbol(decl_idx)
                    && let Some(&cached) = checker.ctx.symbol_types.get(&sym_id)
                    && cached != TypeId::ANY
                    && cached != TypeId::ERROR
                {
                    return cached;
                }
                declared_type
            }
        };

        if let Some(sym_id) = self.ctx.binder.get_node_symbol(decl_idx) {
            self.push_symbol_dependency(sym_id, true);
            // Snapshot whether symbol was already cached BEFORE compute_final_type.
            // If it was, any ERROR in the cache is from earlier resolution (e.g., use-before-def),
            // not from circular detection during this declaration's initializer processing.
            let sym_already_cached = self.ctx.symbol_types.contains_key(&sym_id);
            let mut final_type = compute_final_type(self);
            // Check if get_type_of_symbol cached ERROR specifically DURING compute_final_type.
            // This happens when the initializer (directly or indirectly) references the variable,
            // causing the node-level cycle detection to return ERROR.
            let sym_cached_as_error =
                !sym_already_cached && self.ctx.symbol_types.get(&sym_id) == Some(&TypeId::ERROR);

            // TS2502: 'x' is referenced directly or indirectly in its own type annotation.
            // Skip this check when the variable already had a type from a prior declaration
            // (i.e., this is a `var` redeclaration). In that case, `typeof x` resolves to
            // the previously-established type, not circularly to itself.
            // This matches tsc behavior where `var p: Point; var p: typeof p;` is valid.
            let is_redeclaration = self.ctx.var_decl_types.contains_key(&sym_id);
            if var_decl.type_annotation.is_some() && !is_redeclaration {
                // Try AST-based check first (catches complex circularities that confuse the solver)
                let ast_circular = self
                    .find_circular_reference_in_type_node(var_decl.type_annotation, sym_id, false)
                    .is_some();

                // Then try semantic check
                let semantic_circular = !ast_circular
                    && query::has_type_query_for_symbol(
                        self.ctx.types,
                        final_type,
                        sym_id.0,
                        |ty| self.resolve_lazy_type(ty),
                    );

                // Third check: transitive typeof circularity.
                // E.g., `var d: typeof e; var e: typeof d;` — the AST check only
                // sees `typeof e` doesn't directly reference `d`, but following the
                // chain through `e`'s annotation reveals `typeof d`.
                let transitive_circular = !ast_circular
                    && !semantic_circular
                    && self.check_transitive_type_query_circularity(final_type, sym_id);

                if (ast_circular || semantic_circular || transitive_circular)
                    && let Some(ref name) = var_name
                {
                    let message = format!(
                        "'{name}' is referenced directly or indirectly in its own type annotation."
                    );
                    self.error_at_node(var_decl.name, &message, 2502);
                    final_type = TypeId::ANY;
                }
            }

            if !self.ctx.compiler_options.sound_mode {
                final_type =
                    tsz_solver::relations::freshness::widen_freshness(self.ctx.types, final_type);
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
            if var_decl.name.is_some() {
                self.ctx.node_types.insert(var_decl.name.0, final_type);
            }

            // Capture the raw declared type of THIS specific declaration for TS2403.
            // A bare `var y;` (no annotation, no initializer) always declares `any`,
            // even if the symbol type was previously cached as a concrete type.
            // `compute_final_type` may return a cached type for for-in/for-of loops,
            // so we must override that for bare redeclarations.
            let raw_declared_type =
                if var_decl.type_annotation.is_none() && var_decl.initializer.is_none() {
                    TypeId::ANY
                } else if var_decl.type_annotation.is_none() && var_decl.initializer.is_some() {
                    // For TS2403, when the initializer is a bare enum identifier (e.g., `var x = E`),
                    // tsc treats the declared type as `typeof E` (the enum object type), not `E`.
                    // This ensures `var x = E; var x = E.a;` correctly triggers TS2403 because
                    // `typeof E` and `E` are not type-identical.
                    self.initializer_ts2403_type(var_decl.initializer, final_type)
                } else {
                    // When the type annotation is `typeof EnumSymbol`, resolve to the enum
                    // object type. This matches tsc where `typeof E` is the enum object
                    // shape, ensuring `var e = E; var e: typeof E;` is compatible.
                    self.annotation_ts2403_type(var_decl.type_annotation, final_type)
                };

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
            // and the inferred type is 'any'.
            // Skip destructuring patterns - TypeScript doesn't emit TS7005 for them
            // because binding elements with default values can infer their types.
            //
            // For non-ambient declarations, `symbol_types` guards against emitting
            // TS7005 for control-flow typed variables (e.g., `var x;` later assigned).
            // For ambient declarations (`declare var foo;`), there's no control flow
            // so we always emit when the type is implicitly `any`.
            let is_ambient = self.is_ambient_declaration(decl_idx);
            let is_const = self.is_const_variable_declaration(decl_idx);
            if self.ctx.no_implicit_any()
                && !self.ctx.has_real_syntax_errors
                && !sym_already_cached
                && var_decl.type_annotation.is_none()
                && var_decl.initializer.is_none()
                && final_type == TypeId::ANY
            {
                // Check if the variable name is a destructuring pattern
                let is_destructuring_pattern =
                    self.ctx.arena.get(var_decl.name).is_some_and(|name_node| {
                        name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                            || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                    });

                if !is_destructuring_pattern && let Some(ref name) = var_name {
                    if (is_ambient || is_const) && !self.ctx.is_declaration_file() {
                        // TS7005: Ambient and const declarations always emit at the declaration site.
                        // tsc suppresses noImplicitAny diagnostics for .d.ts files since
                        // declaration files inherently have ambient declarations.
                        use crate::diagnostics::diagnostic_codes;
                        self.error_at_node_msg(
                            var_decl.name,
                            diagnostic_codes::VARIABLE_IMPLICITLY_HAS_AN_TYPE,
                            &[name, "any"],
                        );
                    } else {
                        // Non-ambient: defer decision between TS7034 and no-error.
                        // TS7034 fires when the variable is captured by a nested function.
                        // Detection happens in get_type_of_identifier when a reference
                        // to this variable is found inside a nested function scope.
                        //
                        // tsc only emits TS7034/TS7005 for function-scoped (var) declarations.
                        // Block-scoped (let/const/using) declarations are NOT subject to
                        // these diagnostics — tsc treats their implicit `any` as benign.
                        let is_block_scoped_decl = if let Some(ext) =
                            self.ctx.arena.get_extended(decl_idx)
                            && let Some(parent) = self.ctx.arena.get(ext.parent)
                            && parent.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                        {
                            let flags = parent.flags as u32;
                            use tsz_parser::parser::node_flags;
                            (flags & (node_flags::LET | node_flags::CONST | node_flags::USING)) != 0
                        } else {
                            false
                        };
                        if !is_block_scoped_decl {
                            self.ctx
                                .pending_implicit_any_vars
                                .insert(sym_id, var_decl.name);
                        }
                    }
                }
            }

            // TS7022/TS7023: Circular initializer/return type implicit any diagnostics.
            // Gated by noImplicitAny (like all TS7xxx implicit-any diagnostics).
            //
            // Detection: During compute_final_type, if get_type_of_symbol was called for
            // this variable's symbol and cached ERROR (sym_cached_as_error), it means the
            // initializer references the variable creating a circular dependency.
            //
            // TS7022: Structural circularity — `var a = { f: a }`.
            // TS7023: Return-type circularity — `var f = () => f()` or
            //         `var f = function() { return f(); }`.
            if self.ctx.no_implicit_any()
                && var_decl.type_annotation.is_none()
                && var_decl.initializer.is_some()
                && sym_cached_as_error
                && self.type_contains_error(final_type)
            {
                let is_deferred_initializer =
                    self.ctx.arena.get(var_decl.initializer).is_some_and(|n| {
                        matches!(
                            n.kind,
                            syntax_kind_ext::FUNCTION_EXPRESSION
                                | syntax_kind_ext::ARROW_FUNCTION
                                | syntax_kind_ext::CLASS_EXPRESSION
                        )
                    });
                if let Some(ref name) = var_name {
                    use crate::diagnostics::diagnostic_codes;
                    if is_deferred_initializer {
                        // TS7023: Function/arrow initializer with circular return type.
                        self.error_at_node_msg(
                            var_decl.name,
                            diagnostic_codes::IMPLICITLY_HAS_RETURN_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_RETURN_TYPE_ANNOTATION,
                            &[name],
                        );
                    } else {
                        // TS7022: Structural circularity in initializer.
                        self.error_at_node_msg(
                            var_decl.name,
                            diagnostic_codes::IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION_AND_IS_REFERE,
                            &[name],
                        );
                    }
                }
            }

            // Check for variable redeclaration in the current scope (TS2403).
            // Note: This applies specifically to 'var' merging where types must match.
            // let/const duplicates are caught earlier by the binder (TS2451).
            // Skip TS2403 for mergeable declarations (namespace, enum, class, interface, function overloads).
            // Bare declarations (`var x;` with no annotation/initializer) don't establish a
            // type constraint and never trigger TS2403 in tsc.
            let is_bare_declaration =
                var_decl.type_annotation.is_none() && var_decl.initializer.is_none();
            let is_block_scoped = if let Some(ext) = self.ctx.arena.get_extended(decl_idx)
                && let Some(parent) = self.ctx.arena.get(ext.parent)
                && parent.kind == tsz_parser::parser::syntax_kind_ext::VARIABLE_DECLARATION_LIST
            {
                let flags = parent.flags as u32;
                use tsz_parser::parser::node_flags;
                (flags & (node_flags::LET | node_flags::CONST | node_flags::USING)) != 0
            } else {
                false
            };

            // TS2403 only applies to non-block-scoped variables (var)
            if !is_block_scoped {
                // Non-exported variables inside namespace bodies are local to that body.
                // They should not trigger TS2403 against exported variables of the same
                // name from other (merged) namespace bodies, even if the binder merged
                // their symbols.
                let is_non_exported_ns_var =
                    self.var_decl_namespace_export_status(decl_idx) == Some(false);

                if let Some(prev_type) = self.ctx.var_decl_types.get(&sym_id).copied() {
                    // Check if this is a mergeable declaration by looking at the node kind.
                    // Mergeable declarations: namespace/module, enum, class, interface, function.
                    // When these are declared with the same name, they merge instead of conflicting.
                    let is_mergeable_declaration =
                        if let Some(decl_node) = self.ctx.arena.get(decl_idx) {
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
                        && !is_bare_declaration
                        && !is_non_exported_ns_var
                        && !self.are_var_decl_types_compatible(prev_type, raw_declared_type)
                    {
                        if let Some(ref name) = var_name {
                            self.error_subsequent_variable_declaration(
                                name,
                                prev_type,
                                raw_declared_type,
                                decl_idx,
                            );
                        }
                    } else {
                        let refined = self.refine_var_decl_type(prev_type, final_type);
                        if refined != prev_type {
                            self.ctx.var_decl_types.insert(sym_id, refined);
                        }
                    }
                } else {
                    // If this is the first time we see this variable in the current check run,
                    // check if it has prior declarations (e.g. in lib.d.ts or earlier in the file)
                    // that establish its type.
                    let mut prior_type_found = None;
                    let symbol_name = self
                        .ctx
                        .binder
                        .get_symbol(sym_id)
                        .map(|s| s.escaped_name.clone());

                    // 1. Check lib contexts for prior declarations (e.g. 'var symbol' in lib.d.ts)
                    // Extract data to avoid holding borrow on self during loop
                    let types = self.ctx.types;
                    let compiler_options = self.ctx.compiler_options.clone();
                    let definition_store = self.ctx.definition_store.clone();
                    let lib_contexts = self.ctx.lib_contexts.clone();
                    let lib_contexts_data: Vec<_> = lib_contexts
                        .iter()
                        .map(|ctx| (ctx.arena.clone(), ctx.binder.clone()))
                        .collect();

                    if let Some(name) = symbol_name {
                        for (arena, binder) in lib_contexts_data {
                            // Lookup by name in lib binder to ensure we find the matching symbol
                            // even if SymbolIds are not perfectly aligned across contexts.
                            if let Some(lib_sym_id) = binder.file_locals.get(&name)
                                && let Some(lib_sym) = binder.get_symbol(lib_sym_id)
                            {
                                for &lib_decl in &lib_sym.declarations {
                                    if lib_decl.is_some()
                                        && CheckerState::enter_cross_arena_delegation()
                                    {
                                        let mut lib_checker =
                                            CheckerState::new_with_shared_def_store(
                                                &arena,
                                                &binder,
                                                types,
                                                "lib.d.ts".to_string(),
                                                compiler_options.clone(),
                                                definition_store.clone(),
                                            );
                                        // Ensure lib checker can resolve types from other lib files
                                        lib_checker.ctx.set_lib_contexts(lib_contexts.clone());

                                        let lib_type = lib_checker.get_type_of_node(lib_decl);
                                        CheckerState::leave_cross_arena_delegation();

                                        // Check compatibility (skip for bare declarations)
                                        if !is_bare_declaration
                                            && !self
                                                .are_var_decl_types_compatible(lib_type, final_type)
                                            && let Some(ref name) = var_name
                                        {
                                            self.error_subsequent_variable_declaration(
                                                name, lib_type, final_type, decl_idx,
                                            );
                                        }

                                        prior_type_found =
                                            Some(if let Some(prev) = prior_type_found {
                                                self.refine_var_decl_type(prev, lib_type)
                                            } else {
                                                lib_type
                                            });
                                    }
                                }
                            }
                        }
                    }

                    // 2. Check local declarations (in case of intra-file redeclaration)
                    if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                        for &other_decl in &symbol.declarations {
                            if other_decl == decl_idx {
                                break;
                            }
                            if other_decl.is_some() {
                                let other_type = self.get_type_of_node(other_decl);

                                // Check if other declaration is mergeable (namespace, etc.)
                                let other_node_kind =
                                    self.ctx.arena.get(other_decl).map_or(0, |n| n.kind);
                                let is_other_mergeable = matches!(
                                    other_node_kind,
                                    syntax_kind_ext::MODULE_DECLARATION
                                        | syntax_kind_ext::ENUM_DECLARATION
                                        | syntax_kind_ext::CLASS_DECLARATION
                                        | syntax_kind_ext::INTERFACE_DECLARATION
                                        | syntax_kind_ext::FUNCTION_DECLARATION
                                );

                                // Functions, classes, and enums don't merge with variables,
                                // so they should not establish a "previous variable type" for TS2403.
                                // Only other variables and namespaces (which DO merge with vars) establish this.
                                let establishes_var_type = matches!(
                                    other_node_kind,
                                    syntax_kind_ext::VARIABLE_DECLARATION
                                        | syntax_kind_ext::PARAMETER
                                        | syntax_kind_ext::BINDING_ELEMENT
                                        | syntax_kind_ext::MODULE_DECLARATION
                                );

                                if !establishes_var_type {
                                    continue;
                                }

                                // Skip TS2403 when either declaration is a non-exported
                                // namespace variable — non-exported members are local to
                                // their namespace body and don't merge with other bodies.
                                let is_other_non_exported_ns_var = self
                                    .var_decl_namespace_export_status(other_decl)
                                    == Some(false);

                                if !is_other_mergeable
                                    && !is_bare_declaration
                                    && !is_non_exported_ns_var
                                    && !is_other_non_exported_ns_var
                                    && !self.are_var_decl_types_compatible(other_type, final_type)
                                    && let Some(ref name) = var_name
                                {
                                    self.error_subsequent_variable_declaration(
                                        name, other_type, final_type, decl_idx,
                                    );
                                }

                                prior_type_found = Some(if let Some(prev) = prior_type_found {
                                    self.refine_var_decl_type(prev, other_type)
                                } else {
                                    other_type
                                });
                            }
                        }
                    }

                    let type_to_store = if let Some(prior) = prior_type_found {
                        self.refine_var_decl_type(prior, raw_declared_type)
                    } else {
                        raw_declared_type
                    };
                    // Don't store bare declarations (`var x;`) unless a prior type
                    // was found from lib or earlier local declarations — bare vars
                    // don't establish a type constraint for TS2403.
                    if !is_bare_declaration || prior_type_found.is_some() {
                        self.ctx.var_decl_types.insert(sym_id, type_to_store);
                    }
                }
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
            let pattern_type = if var_decl.type_annotation.is_some() {
                self.get_type_from_type_node(var_decl.type_annotation)
            } else if var_decl.initializer.is_some() {
                self.get_type_of_node(var_decl.initializer)
            } else if is_catch_variable && self.ctx.use_unknown_in_catch_variables() {
                TypeId::UNKNOWN
            } else {
                TypeId::ANY
            };

            if name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN {
                self.check_destructuring_object_literal_computed_excess_properties(
                    var_decl.name,
                    var_decl.initializer,
                    pattern_type,
                    !var_decl.type_annotation.is_some(),
                );
            }

            // TS2488: Check array destructuring for iterability before assigning types
            if name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
                self.check_destructuring_iterability(
                    var_decl.name,
                    pattern_type,
                    var_decl.initializer,
                );
                self.report_empty_array_destructuring_bounds(var_decl.name, var_decl.initializer);
            }

            // Ensure binding element identifiers get the correct inferred types.
            self.assign_binding_pattern_symbol_types(var_decl.name, pattern_type);
            self.check_binding_pattern(
                var_decl.name,
                pattern_type,
                var_decl.type_annotation.is_some(),
            );

            // Track destructured binding groups for correlated narrowing.
            // Only needed for union source types where narrowing one property affects others.
            let resolved_for_union = self.evaluate_type_for_assignability(pattern_type);
            if query::union_members(self.ctx.types, resolved_for_union).is_some() {
                // Check if this is a const declaration
                let is_const = if let Some(ext) = self.ctx.arena.get_extended(decl_idx) {
                    if let Some(parent_node) = self.ctx.arena.get(ext.parent) {
                        use tsz_parser::parser::node_flags;
                        (parent_node.flags & node_flags::CONST as u16) != 0
                    } else {
                        false
                    }
                } else {
                    false
                };

                self.record_destructured_binding_group(
                    var_decl.name,
                    resolved_for_union,
                    is_const,
                    name_node.kind,
                );
            }
        }
    }

    /// TS2481: Check if a `var` declaration shadows a block-scoped declaration (`let`/`const`)
    /// in an enclosing scope that is NOT at function/module/source-file level.
    ///
    /// When `var x` appears in the same block as `let x` (or `const x`) from an enclosing
    /// block that is NOT the function/module top scope, the `var` hoists past the block-scoped
    /// binding. At runtime this is a `SyntaxError` because the `var` initialization would write
    /// to the outer function-scoped variable while the block-scoped binding shadows it.
    fn check_var_declared_names_not_shadowed(
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
        // symbol with the same name in an enclosing scope. We check all scopes including
        // the immediate one, because `const x` and `var x` in the same block creates
        // separate symbols (var hoists to function scope, const stays in block scope).
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

        // If the block-scoped variable is in a function/module/source-file scope,
        // then names share scope (var hoists to the same level). The binder already
        // handles duplicate declarations in that case — no TS2481 needed.
        // TS2481 only fires when the block-scoped variable is in an intermediate
        // Block scope (not at function/module/source level).
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
    /// initializer expression. When the initializer is a bare enum identifier
    /// (e.g., `var x = E`), tsc treats the declared type as `typeof E` (the
    /// enum object type), not the widened enum union `E`. For all other
    /// initializers, returns `fallback_type` unchanged.
    fn initializer_ts2403_type(&mut self, init_idx: NodeIndex, fallback_type: TypeId) -> TypeId {
        let Some(init_node) = self.ctx.arena.get(init_idx) else {
            return fallback_type;
        };

        // Only applies to bare identifier initializers (not property access, etc.)
        if init_node.kind != SyntaxKind::Identifier as u16 {
            return fallback_type;
        }

        // Check if the identifier resolves to an enum symbol
        if let Some(init_sym_id) = self.resolve_identifier_symbol(init_idx)
            && let Some(symbol) = self.ctx.binder.get_symbol(init_sym_id)
            && (symbol.flags & tsz_binder::symbol_flags::ENUM) != 0
            && (symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER) == 0
        {
            // Return the enum object type ("typeof E") instead of the enum union type
            if let Some(enum_obj) = self.enum_object_type(init_sym_id) {
                return enum_obj;
            }
        }

        fallback_type
    }

    /// For TS2403, when the type annotation is `typeof EnumSymbol`, resolve
    /// to the enum object type. This matches tsc where `typeof E` produces
    /// the enum object shape `{ readonly A: E.A; ... }`. For all other
    /// annotations, returns `fallback_type` unchanged.
    fn annotation_ts2403_type(
        &mut self,
        annotation_idx: NodeIndex,
        fallback_type: TypeId,
    ) -> TypeId {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(ann_node) = self.ctx.arena.get(annotation_idx) else {
            return fallback_type;
        };

        // Check if the annotation is a TypeQuery node (typeof X)
        if ann_node.kind != syntax_kind_ext::TYPE_QUERY {
            return fallback_type;
        }

        let Some(type_query) = self.ctx.arena.get_type_query(ann_node) else {
            return fallback_type;
        };

        // The expr_name of the TypeQuery is the identifier being referenced
        let expr_idx = type_query.expr_name;
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return fallback_type;
        };

        // Must be a simple identifier (not qualified name)
        if expr_node.kind != SyntaxKind::Identifier as u16 {
            return fallback_type;
        }

        // Check if it resolves to an enum symbol
        if let Some(sym_id) = self.resolve_identifier_symbol(expr_idx)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && (symbol.flags & tsz_binder::symbol_flags::ENUM) != 0
            && (symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER) == 0
            && let Some(enum_obj) = self.enum_object_type(sym_id)
        {
            return enum_obj;
        }

        fallback_type
    }

    // Destructuring pattern methods (report_empty_array_destructuring_bounds,
    // assign_binding_pattern_symbol_types, record_destructured_binding_group,
    // get_binding_element_type, rest_binding_array_type, is_only_undefined_or_null)
    // are in `destructuring.rs`.

    /// Check if a variable declaration is inside a namespace body and whether
    /// it has an `export` modifier. Returns `Some(is_exported)` if inside a
    /// namespace, `None` otherwise.
    ///
    /// Used to prevent false TS2403 errors for non-exported variables in merged
    /// namespace bodies. In tsc, non-exported members are local to each body.
    fn var_decl_namespace_export_status(&self, decl_idx: NodeIndex) -> Option<bool> {
        // Walk up: VariableDeclaration -> VariableDeclarationList -> VariableStatement
        let ext = self.ctx.arena.get_extended(decl_idx)?;
        let decl_list_idx = ext.parent;
        let decl_list_ext = self.ctx.arena.get_extended(decl_list_idx)?;
        let var_stmt_idx = decl_list_ext.parent;
        let var_stmt = self.ctx.arena.get(var_stmt_idx)?;
        if var_stmt.kind != syntax_kind_ext::VARIABLE_STATEMENT {
            return None;
        }

        // Walk up: VariableStatement -> ModuleBlock -> ModuleDeclaration
        let var_stmt_ext = self.ctx.arena.get_extended(var_stmt_idx)?;
        let container = self.ctx.arena.get(var_stmt_ext.parent)?;
        if container.kind != syntax_kind_ext::MODULE_BLOCK {
            return None;
        }

        // Check export modifier on the VariableStatement
        let has_export = if let Some(var_data) = self.ctx.arena.get_variable(var_stmt) {
            self.ctx
                .arena
                .has_modifier_ref(var_data.modifiers.as_ref(), SyntaxKind::ExportKeyword)
        } else {
            false
        };

        Some(has_export)
    }

    /// Check if a `TypeQuery` type transitively leads back to the target symbol
    /// through a chain of typeof references in variable declarations.
    ///
    /// Handles `var d: typeof e; var e: typeof d;` where the direct AST check
    /// only sees one level but following the chain reveals circularity.
    fn check_transitive_type_query_circularity(
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
                // Check if this declaration's type annotation references the target
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
                // If the annotation is a typeof, follow the chain
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
}

#[cfg(test)]
mod test_utils {
    pub fn check_and_collect(source: &str, error_code: u32) -> Vec<(u32, String)> {
        crate::test_utils::check_source_diagnostics(source)
            .iter()
            .filter(|d| d.code == error_code)
            .map(|d| (d.start, d.message_text.clone()))
            .collect()
    }
}

#[cfg(test)]
mod ts2481_tests {
    use super::test_utils::check_and_collect;

    #[test]
    fn var_in_for_of_with_let() {
        let source = "for (let v of []) {\n    var v = 0;\n}";
        let errors = check_and_collect(source, 2481);
        assert_eq!(errors.len(), 1, "Expected 1 TS2481: {errors:?}");
        assert!(errors[0].1.contains("'v'"));
    }

    #[test]
    fn var_in_for_of_without_initializer() {
        let source = "for (let v of []) {\n    var v;\n}";
        let errors = check_and_collect(source, 2481);
        assert_eq!(errors.len(), 1, "Expected 1 TS2481: {errors:?}");
    }

    #[test]
    fn var_in_nested_block_with_let() {
        let source = "{\n    let x;\n    {\n        var x = 1;\n    }\n}";
        let errors = check_and_collect(source, 2481);
        assert_eq!(errors.len(), 1, "Expected 1 TS2481: {errors:?}");
    }

    #[test]
    fn var_in_for_in_with_let() {
        let source = "function test() {\n    for (let v in {}) { var v; }\n}";
        let errors = check_and_collect(source, 2481);
        assert_eq!(errors.len(), 1, "Expected 1 TS2481: {errors:?}");
    }

    #[test]
    fn var_in_for_with_let() {
        let source = "function test() {\n    for (let v; ; ) { var v; }\n}";
        let errors = check_and_collect(source, 2481);
        assert_eq!(errors.len(), 1, "Expected 1 TS2481: {errors:?}");
    }

    #[test]
    fn no_error_when_names_share_function_scope() {
        // function f() { let x; var x; } — no TS2481 (names share function scope)
        let source = "function f() {\n    let x = 1;\n    var x = 2;\n}";
        let errors = check_and_collect(source, 2481);
        assert_eq!(
            errors.len(),
            0,
            "No TS2481 when names share function scope: {errors:?}"
        );
    }

    #[test]
    fn no_error_for_let_only() {
        let source = "{\n    let x;\n    {\n        let x;\n    }\n}";
        let errors = check_and_collect(source, 2481);
        assert_eq!(errors.len(), 0, "No TS2481 for let-to-let: {errors:?}");
    }

    #[test]
    fn deeply_nested_var() {
        let source = "{\n    let x;\n    {\n        {\n            var x = 1;\n        }\n    }\n}";
        let errors = check_and_collect(source, 2481);
        assert_eq!(
            errors.len(),
            1,
            "Expected 1 TS2481 for deeply nested var: {errors:?}"
        );
    }
}

#[cfg(test)]
mod ts2397_tests {
    use super::test_utils::check_and_collect;

    #[test]
    fn var_undefined_emits_ts2397() {
        let errors = check_and_collect("var undefined = null;", 2397);
        assert_eq!(errors.len(), 1, "Expected 1 TS2397: {errors:?}");
        assert!(errors[0].1.contains("'undefined'"));
    }

    #[test]
    fn var_global_this_emits_ts2397() {
        let errors = check_and_collect("var globalThis;", 2397);
        assert_eq!(errors.len(), 1, "Expected 1 TS2397: {errors:?}");
        assert!(errors[0].1.contains("'globalThis'"));
    }

    #[test]
    fn let_undefined_emits_ts2397() {
        let errors = check_and_collect("let undefined = 1;", 2397);
        assert_eq!(errors.len(), 1, "Expected 1 TS2397: {errors:?}");
    }

    #[test]
    fn namespace_global_this_emits_ts2397() {
        let errors = check_and_collect("namespace globalThis { export function foo() {} }", 2397);
        assert_eq!(errors.len(), 1, "Expected 1 TS2397: {errors:?}");
        assert!(errors[0].1.contains("'globalThis'"));
    }

    #[test]
    fn normal_var_no_ts2397() {
        let errors = check_and_collect("var x = 1;", 2397);
        assert_eq!(errors.len(), 0, "No TS2397 for normal var: {errors:?}");
    }

    #[test]
    fn const_undefined_emits_ts2397() {
        let errors = check_and_collect("const undefined = void 0;", 2397);
        assert_eq!(errors.len(), 1, "Expected 1 TS2397: {errors:?}");
    }
}

#[cfg(test)]
mod ts2403_false_positive_tests {
    use crate::test_utils::check_source_diagnostics;

    #[test]
    fn recursive_types_with_typeof_no_false_ts2403() {
        // From recursiveTypesWithTypeof.ts
        let source = r#"
var c: typeof c;
var c: any;
"#;
        let ts2403 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 expected for circular typeof: {ts2403:?}"
        );
    }

    #[test]
    fn var_redecl_with_interface_no_false_ts2403() {
        // From TwoInternalModulesWithTheSameNameAndSameCommonRoot.ts (part3)
        let source = r#"
interface Point { x: number; y: number; }
var o: { x: number; y: number };
var o: Point;
"#;
        let ts2403 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 expected for structurally identical types: {ts2403:?}"
        );
    }

    #[test]
    fn typeof_module_no_false_ts2403() {
        // From nonInstantiatedModule.ts
        let source = r#"
namespace M {
    export var a = 1;
}
var m: typeof M;
var m = M;
"#;
        let ts2403 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 expected for typeof module: {ts2403:?}"
        );
    }

    #[test]
    fn optional_tuple_elements_no_false_ts2403() {
        // From optionalTupleElementsAndUndefined.ts
        let source = r#"
var v: [1, 2?];
var v: [1, (2 | undefined)?];
"#;
        let ts2403 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 expected for optional tuple elements: {ts2403:?}"
        );
    }

    #[test]
    fn typeof_var_redecl_no_false_ts2403() {
        // From typeofANonExportedType.ts
        let source = r#"
interface I { foo: string; }
var i: I;
var i2: I;
var r5: typeof i;
var r5: typeof i2;
"#;
        let ts2403 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 expected for typeof var redecl: {ts2403:?}"
        );
    }

    #[test]
    fn namespace_merged_var_no_false_ts2403() {
        // From TwoInternalModulesThatMergeEachWithExportedAndNonExportedLocalVarsOfTheSameName
        let source = r#"
namespace A {
    export interface Point { x: number; y: number; }
    export var Origin: Point = { x: 0, y: 0 };
}
namespace A {
    var Origin: string = "0,0";
}
"#;
        let ts2403 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 expected for merged namespace vars: {ts2403:?}"
        );
    }

    #[test]
    fn namespace_merged_var_redecl_no_false_ts2403() {
        // From TwoInternalModulesWithTheSameNameAndSameCommonRoot.ts (part3 vars)
        let source = r#"
namespace A {
    export interface Point { x: number; y: number; }
    export var Origin: Point = { x: 0, y: 0 };
}
var o: { x: number; y: number };
var o: A.Point;
"#;
        let ts2403 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 for interface/object-literal redecl: {ts2403:?}"
        );
    }

    #[test]
    fn non_instantiated_module_redecl_no_false_ts2403() {
        // From nonInstantiatedModule.ts
        let source = r#"
namespace M {
    export var a = 1;
}
var a1: number;
var a1 = M.a;
"#;
        let ts2403 = check_source_diagnostics(source)
            .into_iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(
            ts2403.len(),
            0,
            "No TS2403 for module property access: {ts2403:?}"
        );
    }

    #[test]
    fn enum_var_redecl_emits_ts2403() {
        // From duplicateLocalVariable4.ts: var x = E; var x = E.a;
        // First x is `typeof E`, second x is `E` — types differ, should emit TS2403.
        let source = r#"
enum E { a }
var x = E;
var x = E.a;
"#;
        let all_diags = check_source_diagnostics(source);
        let ts2403 = all_diags
            .iter()
            .filter(|d| d.code == 2403)
            .collect::<Vec<_>>();
        assert_eq!(ts2403.len(), 1, "Expected 1 TS2403 for enum var redecl");
    }
}
