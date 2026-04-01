//! Variable declaration and destructuring checking.
//!
//! For-in / for-of loop variable checking is in `for_loop.rs`.

use crate::computation::complex::is_contextually_sensitive;
use crate::context::{PendingImplicitAnyKind, PendingImplicitAnyVar, TypingRequest};
use crate::query_boundaries::flow as flow_boundary;
use crate::query_boundaries::state::checking as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn declaration_pattern_initializer_request(
        &mut self,
        pattern_idx: NodeIndex,
        initializer_idx: NodeIndex,
        typing_request: &TypingRequest,
    ) -> TypingRequest {
        let contextual_init = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(initializer_idx);
        let supports_pattern_context =
            self.ctx
                .arena
                .get(contextual_init)
                .is_some_and(|init_node| {
                    matches!(
                        init_node.kind,
                        syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                            | syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                    )
                });

        if !supports_pattern_context {
            return TypingRequest::NONE;
        }

        self.build_contextual_type_from_pattern_with_request(
            pattern_idx,
            &typing_request.read().contextual_opt(None),
        )
        .map_or(TypingRequest::NONE, TypingRequest::with_contextual_type)
    }

    fn cached_inferred_variable_type(
        &self,
        decl_idx: NodeIndex,
        name_idx: NodeIndex,
    ) -> Option<TypeId> {
        self.ctx
            .binder
            .get_node_symbol(decl_idx)
            .and_then(|sym_id| self.ctx.symbol_types.get(&sym_id).copied())
            .or_else(|| {
                self.ctx
                    .binder
                    .get_node_symbol(name_idx)
                    .and_then(|sym_id| self.ctx.symbol_types.get(&sym_id).copied())
            })
            .filter(|&type_id| type_id != TypeId::ERROR)
    }

    fn has_prior_value_declaration_for_symbol(&self, decl_idx: NodeIndex) -> bool {
        let Some(sym_id) = self.ctx.binder.get_node_symbol(decl_idx).or_else(|| {
            self.ctx
                .arena
                .get(decl_idx)
                .and_then(|node| self.ctx.arena.get_variable_declaration(node))
                .and_then(|decl| self.ctx.binder.get_node_symbol(decl.name))
        }) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let current_pos = self
            .ctx
            .arena
            .get(decl_idx)
            .map_or(u32::MAX, |node| node.pos);
        let mut saw_current = false;
        for &other in &symbol.declarations {
            if other == decl_idx {
                saw_current = true;
                break;
            }
            if !other.is_some() {
                continue;
            }
            if let Some(other_node) = self.ctx.arena.get(other)
                && other_node.pos < current_pos
            {
                return true;
            }
        }

        if saw_current {
            return false;
        }

        symbol.declarations.iter().any(|&other| {
            other != decl_idx
                && other.is_some()
                && self
                    .ctx
                    .arena
                    .get(other)
                    .is_some_and(|node| node.pos < current_pos)
        })
    }

    fn redeclaration_initializer_request(
        &mut self,
        decl_idx: NodeIndex,
        name_idx: NodeIndex,
        initializer_idx: NodeIndex,
    ) -> TypingRequest {
        if !self.has_prior_value_declaration_for_symbol(decl_idx) {
            return TypingRequest::NONE;
        }

        let Some(init_node) = self.ctx.arena.get(
            self.ctx
                .arena
                .skip_parenthesized_and_assertions(initializer_idx),
        ) else {
            return TypingRequest::NONE;
        };
        let initializer_needs_context = matches!(
            init_node.kind,
            k if k == syntax_kind_ext::CALL_EXPRESSION
                || k == syntax_kind_ext::NEW_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
        ) || is_contextually_sensitive(self, initializer_idx);
        if !initializer_needs_context {
            return TypingRequest::NONE;
        }

        let Some(cached_type) = self.cached_inferred_variable_type(decl_idx, name_idx) else {
            return TypingRequest::NONE;
        };
        if matches!(cached_type, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN) {
            return TypingRequest::NONE;
        }

        TypingRequest::with_contextual_type(self.contextual_type_for_expression(cached_type))
    }

    fn checked_js_remote_class_declared_type_for_variable(
        &mut self,
        decl_idx: NodeIndex,
    ) -> Option<TypeId> {
        if !self.is_js_file()
            || !self.ctx.compiler_options.check_js
            || self.ctx.binder.is_external_module()
        {
            return None;
        }

        let node = self.ctx.arena.get(decl_idx)?;
        let var_decl = self.ctx.arena.get_variable_declaration(node)?;
        if var_decl.initializer.is_none() {
            return None;
        }
        let name = self
            .ctx
            .arena
            .get_identifier_at(var_decl.name)?
            .escaped_text
            .clone();

        let all_arenas = self.ctx.all_arenas.clone()?;
        let all_binders = self.ctx.all_binders.clone()?;

        for (file_idx, binder) in all_binders.iter().enumerate() {
            if file_idx == self.ctx.current_file_idx || binder.is_external_module() {
                continue;
            }
            let arena = all_arenas.get(file_idx)?;
            let source_file = arena.source_files.first()?;

            for &stmt_idx in &source_file.statements.nodes {
                let Some(stmt_node) = arena.get(stmt_idx) else {
                    continue;
                };
                if stmt_node.kind != syntax_kind_ext::CLASS_DECLARATION {
                    continue;
                }
                let Some(class_decl) = arena.get_class(stmt_node) else {
                    continue;
                };
                let Some(ident) = arena.get_identifier_at(class_decl.name) else {
                    continue;
                };
                if ident.escaped_text != name || !arena.is_in_ambient_context(stmt_idx) {
                    continue;
                }
                let Some(sym_id) = binder.get_node_symbol(stmt_idx) else {
                    continue;
                };
                self.ctx.register_symbol_file_index(sym_id, file_idx);
                return Some(self.get_type_of_symbol(sym_id));
            }
        }

        None
    }

    fn maybe_clear_checked_initializer_type_cache(&mut self, initializer_idx: NodeIndex) {
        // Some initializer forms are first visited during build_type_environment, where we only
        // want a stable type shape. The later checked pass must revisit them so body/member
        // diagnostics (for example TS2454 inside class-expression methods or TS2564 on class
        // fields) are emitted from the canonical checked path.
        if let Some(init_node) = self.ctx.arena.get(initializer_idx)
            && matches!(
                init_node.kind,
                syntax_kind_ext::FUNCTION_EXPRESSION
                    | syntax_kind_ext::ARROW_FUNCTION
                    | syntax_kind_ext::NEW_EXPRESSION
                    | syntax_kind_ext::CLASS_EXPRESSION
                    | syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            )
        {
            self.invalidate_initializer_for_context_change(initializer_idx);
        }
    }

    /// Check a single variable declaration.
    #[tracing::instrument(level = "trace", skip(self), fields(decl_idx = ?decl_idx))]
    pub(crate) fn check_variable_declaration(&mut self, decl_idx: NodeIndex) {
        self.check_variable_declaration_with_request(decl_idx, &TypingRequest::NONE);
    }

    #[tracing::instrument(level = "trace", skip(self, typing_request), fields(decl_idx = ?decl_idx))]
    pub(crate) fn check_variable_declaration_with_request(
        &mut self,
        decl_idx: NodeIndex,
        typing_request: &TypingRequest,
    ) {
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
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

            // tsc points TS1255/TS1263/TS1264 at the `!` token itself, which is
            // immediately after the variable name node (name_node.end, length 1).
            let excl_pos = self.ctx.arena.get(var_decl.name).map(|n| n.end);

            // TS1255: ! is not permitted in ambient context (declare let/var/const)
            if self.is_ambient_declaration(decl_idx) {
                if let Some(pos) = excl_pos {
                    self.emit_error_at(
                        pos,
                        1,
                        diagnostic_messages::A_DEFINITE_ASSIGNMENT_ASSERTION_IS_NOT_PERMITTED_IN_THIS_CONTEXT,
                        diagnostic_codes::A_DEFINITE_ASSIGNMENT_ASSERTION_IS_NOT_PERMITTED_IN_THIS_CONTEXT,
                    );
                } else {
                    self.error_at_node(
                        var_decl.name,
                        diagnostic_messages::A_DEFINITE_ASSIGNMENT_ASSERTION_IS_NOT_PERMITTED_IN_THIS_CONTEXT,
                        diagnostic_codes::A_DEFINITE_ASSIGNMENT_ASSERTION_IS_NOT_PERMITTED_IN_THIS_CONTEXT,
                    );
                }
            }

            // TS1263: ! with initializer is contradictory
            if var_decl.initializer.is_some() {
                if let Some(pos) = excl_pos {
                    self.emit_error_at(
                        pos,
                        1,
                        diagnostic_messages::DECLARATIONS_WITH_INITIALIZERS_CANNOT_ALSO_HAVE_DEFINITE_ASSIGNMENT_ASSERTIONS,
                        diagnostic_codes::DECLARATIONS_WITH_INITIALIZERS_CANNOT_ALSO_HAVE_DEFINITE_ASSIGNMENT_ASSERTIONS,
                    );
                } else {
                    self.error_at_node(
                        var_decl.name,
                        diagnostic_messages::DECLARATIONS_WITH_INITIALIZERS_CANNOT_ALSO_HAVE_DEFINITE_ASSIGNMENT_ASSERTIONS,
                        diagnostic_codes::DECLARATIONS_WITH_INITIALIZERS_CANNOT_ALSO_HAVE_DEFINITE_ASSIGNMENT_ASSERTIONS,
                    );
                }
            }

            // TS1264: ! without type annotation is meaningless
            if var_decl.type_annotation.is_none() {
                if let Some(pos) = excl_pos {
                    self.emit_error_at(
                        pos,
                        1,
                        diagnostic_messages::DECLARATIONS_WITH_DEFINITE_ASSIGNMENT_ASSERTIONS_MUST_ALSO_HAVE_TYPE_ANNOTATIONS,
                        diagnostic_codes::DECLARATIONS_WITH_DEFINITE_ASSIGNMENT_ASSERTIONS_MUST_ALSO_HAVE_TYPE_ANNOTATIONS,
                    );
                } else {
                    self.error_at_node(
                        var_decl.name,
                        diagnostic_messages::DECLARATIONS_WITH_DEFINITE_ASSIGNMENT_ASSERTIONS_MUST_ALSO_HAVE_TYPE_ANNOTATIONS,
                        diagnostic_codes::DECLARATIONS_WITH_DEFINITE_ASSIGNMENT_ASSERTIONS_MUST_ALSO_HAVE_TYPE_ANNOTATIONS,
                    );
                }
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
        let in_non_ambient_class = self
            .ctx
            .enclosing_class
            .as_ref()
            .is_some_and(|c| !c.is_declared)
            || self.is_within_non_ambient_class_body(decl_idx);

        // When an identifier is spelled with unicode escapes (e.g., \u0079ield for yield),
        // TSC treats it as a regular identifier and does NOT emit TS1212/TS1213/TS1214.
        let name_has_unicode_escape = self
            .ctx
            .arena
            .get(var_decl.name)
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .is_some_and(|ident| ident.original_text.is_some());
        if !is_ambient
            && !name_has_unicode_escape
            && self.is_strict_mode_for_node(var_decl.name)
            && let Some(ref name) = var_name
            && crate::state_checking::is_strict_mode_reserved_name(name)
            && !(name.as_str() == "arguments" && in_non_ambient_class)
        {
            self.emit_strict_mode_reserved_word_error(var_decl.name, name, true);
        }
        // TS1100: `eval` or `arguments` used as a variable name in strict mode.
        // In class bodies, `arguments` is reported as TS1210 instead, so only
        // emit TS1100 for `eval` there (not `arguments`).
        if !is_ambient
            && !self.has_syntax_parse_errors()
            && self.is_strict_mode_for_node(var_decl.name)
            && let Some(ref name) = var_name
            && name.as_str() == "arguments"
            && in_non_ambient_class
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let message = format_message(
                diagnostic_messages::CODE_CONTAINED_IN_A_CLASS_IS_EVALUATED_IN_JAVASCRIPTS_STRICT_MODE_WHICH_DOES_NOT,
                &[name],
            );
            self.error_at_node(
                var_decl.name,
                &message,
                diagnostic_codes::CODE_CONTAINED_IN_A_CLASS_IS_EVALUATED_IN_JAVASCRIPTS_STRICT_MODE_WHICH_DOES_NOT,
            );
        }
        if !is_ambient
            && self.is_strict_mode_for_node(var_decl.name)
            && let Some(ref name) = var_name
            && crate::state_checking::is_eval_or_arguments(name)
            && !(in_non_ambient_class && name.as_str() == "arguments")
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
        // TS1039/TS1254: Check initializers in ambient contexts.
        // Use is_in_ambient_context (checks for explicit `declare` keyword ancestors)
        // rather than is_ambient_declaration (which also returns true for all .d.ts files).
        // TSC does not emit TS1039 for variable initializers in .d.ts files.
        if var_decl.initializer.is_some() && self.ctx.arena.is_in_ambient_context(decl_idx) {
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
        let mut jsdoc_declared_type = None;
        let mut compute_final_type = |checker: &mut CheckerState| -> TypeId {
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
            } else if is_catch_variable {
                // Route catch variable type resolution through the flow
                // observation boundary for centralized policy.
                flow_boundary::resolve_catch_variable_type(
                    checker.ctx.use_unknown_in_catch_variables(),
                )
            } else {
                TypeId::ANY
            };
            if !has_type_annotation
                && let Some(jsdoc_type) = checker.jsdoc_type_annotation_for_node(decl_idx)
            {
                declared_type = jsdoc_type;
                jsdoc_declared_type = Some(jsdoc_type);
                has_type_annotation = true;
            }
            if !has_type_annotation
                && let Some(merged_type) =
                    checker.checked_js_remote_class_declared_type_for_variable(decl_idx)
            {
                declared_type = merged_type;
                has_type_annotation = true;
            }
            // If there's a type annotation, that determines the type (even for 'any')
            if has_type_annotation {
                if checker.ctx.no_implicit_any()
                    && let Some(sf) = checker.ctx.arena.source_files.first()
                    && let Some(jsdoc) = checker.find_jsdoc_for_function(decl_idx)
                    && CheckerState::jsdoc_type_tag_function_missing_return(&jsdoc)
                    && let Some((_, comment_pos)) = checker.try_jsdoc_with_ancestor_walk_and_pos(
                        decl_idx,
                        &sf.comments,
                        &sf.text,
                    )
                    && let Some(function_pos) =
                        CheckerState::jsdoc_type_tag_function_keyword_pos_in_source(
                            &sf.text,
                            comment_pos,
                        )
                {
                    checker.ctx.error(
                        function_pos,
                        "function".len() as u32,
                        crate::diagnostics::format_message(
                            crate::diagnostics::diagnostic_messages::FUNCTION_TYPE_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN_TYPE,
                            &["any"],
                        ),
                        crate::diagnostics::diagnostic_codes::FUNCTION_TYPE_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN_TYPE,
                    );
                }
                if var_decl.initializer.is_some() {
                    // Evaluate the declared type to resolve conditionals before using as context.
                    // This ensures types like `type C = string extends string ? "yes" : "no"`
                    // provide proper contextual typing for literals, preventing them from widening to string.
                    // Only evaluate conditional/mapped/index access types - NOT type aliases or interface
                    // references, as evaluating those can change their representation and break variance checking.
                    let evaluated_type = if declared_type != TypeId::ANY {
                        checker.contextual_type_for_expression(declared_type)
                    } else {
                        declared_type
                    };
                    // Build a TypingRequest for the initializer (but not for 'any')
                    let initializer_is_function = checker
                        .ctx
                        .arena
                        .get(var_decl.initializer)
                        .is_some_and(|init_node| {
                            matches!(
                                init_node.kind,
                                syntax_kind_ext::FUNCTION_EXPRESSION
                                    | syntax_kind_ext::ARROW_FUNCTION
                            )
                        });
                    let jsdoc_callable_context = initializer_is_function
                        .then(|| {
                            if var_decl.type_annotation.is_none() {
                                checker.jsdoc_callable_type_annotation_for_node(decl_idx)
                            } else {
                                None
                            }
                        })
                        .flatten()
                        .map(|ty| checker.contextual_type_for_expression(ty));
                    let jsdoc_blocks_callable_context = initializer_is_function
                        && var_decl.type_annotation.is_none()
                        && checker.jsdoc_type_annotation_for_node(decl_idx).is_some()
                        && jsdoc_callable_context.is_none();
                    let suppress_initializer_context = evaluated_type != TypeId::ANY
                        && checker.suppress_initializer_contextual_type_for_generic_call(
                            var_decl.initializer,
                        );
                    let request = if let Some(jsdoc_callable_context) = jsdoc_callable_context {
                        TypingRequest::with_contextual_type(jsdoc_callable_context)
                    } else if evaluated_type != TypeId::ANY
                        && !jsdoc_blocks_callable_context
                        && !suppress_initializer_context
                    {
                        TypingRequest::with_contextual_type(evaluated_type)
                    } else {
                        TypingRequest::NONE
                    };
                    if initializer_is_function && jsdoc_blocks_callable_context {
                        checker
                            .ctx
                            .implicit_any_contextual_closures
                            .remove(&var_decl.initializer);
                        checker
                            .ctx
                            .implicit_any_checked_closures
                            .remove(&var_decl.initializer);
                        checker.invalidate_initializer_for_context_change(var_decl.initializer);
                    }
                    let conditional_branch_ranges = checker
                        .ctx
                        .arena
                        .get(var_decl.initializer)
                        .filter(|node| node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION)
                        .and_then(|node| checker.ctx.arena.get_conditional_expr(node))
                        .map(|cond| {
                            let when_true = checker
                                .ctx
                                .arena
                                .get(cond.when_true)
                                .map(|node| (node.pos, node.end));
                            let when_false = checker
                                .ctx
                                .arena
                                .get(cond.when_false)
                                .map(|node| (node.pos, node.end));
                            [when_true, when_false]
                        });
                    if !request.is_empty()
                        && let Some(init_node) = checker.ctx.arena.get(var_decl.initializer)
                    {
                        let init_start = init_node.pos;
                        let init_end = init_node.end;
                        checker.ctx.diagnostics.retain(|diag| {
                            diag.code
                                == crate::diagnostics::diagnostic_codes::STATIC_MEMBERS_CANNOT_REFERENCE_CLASS_TYPE_PARAMETERS
                                // TS2693/TS2585/TS1361/TS1362: type-only keywords and
                                // type-only import/export used as values are structural
                                // errors, not contextual-typing artifacts.
                                // They must survive the pre-contextual diagnostic reset.
                                || diag.code == crate::diagnostics::diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE
                                || diag.code == crate::diagnostics::diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE_DO_YOU_NEED_TO_CHANGE_YO
                                || diag.code == crate::diagnostics::diagnostic_codes::CANNOT_BE_USED_AS_A_VALUE_BECAUSE_IT_WAS_IMPORTED_USING_IMPORT_TYPE
                                || diag.code == crate::diagnostics::diagnostic_codes::CANNOT_BE_USED_AS_A_VALUE_BECAUSE_IT_WAS_EXPORTED_USING_EXPORT_TYPE
                                // Preserve TS2454 (variable used before assignment) — these
                                // are definite-assignment errors for variables referenced
                                // inside the initializer, not stale contextual-typing
                                // diagnostics that need to be re-evaluated.
                                || diag.code
                                    == crate::diagnostics::diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED
                                // TS2339: "Property does not exist on type" is a structural
                                // error (the object type and property name don't depend on
                                // contextual typing). Preserve it so namespace/module
                                // property-access errors survive the pre-contextual reset.
                                || diag.code
                                    == crate::diagnostics::diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE
                                || diag.start < init_start
                                || diag.start >= init_end
                        });
                        checker.ctx.rebuild_emitted_diagnostics_from_current();
                    }
                    let init_snap = checker.ctx.snapshot_diagnostics();
                    checker.maybe_clear_checked_initializer_type_cache(var_decl.initializer);
                    let init_type =
                        checker.get_type_of_node_with_request(var_decl.initializer, &request);
                    // Ensure the contextually-typed init type is stored in node_types
                    // for the initializer expression. Error elaboration may re-check
                    // the initializer without contextual type, which widens literal
                    // types (e.g., "ok" -> string) and overwrites node_types. By
                    // seeding node_types here, subsequent context-free lookups
                    // (including flow analysis for assignment narrowing) reuse the
                    // contextually-inferred result.
                    if !request.is_empty() && init_type != TypeId::ERROR {
                        checker
                            .ctx
                            .node_types
                            .insert(var_decl.initializer.0, init_type);
                    }
                    let init_type_for_relation = checker.resolve_lazy_type(init_type);
                    if let Some(branch_ranges) = conditional_branch_ranges {
                        // Preserve non-assignability diagnostics from the branch expressions
                        // (e.g. TS2352/TS2873), but drop premature TS2322s produced while
                        // contextually typing the individual branches. The outer variable
                        // declaration check should report the canonical whole-expression error.
                        checker
                            .ctx
                            .rollback_diagnostics_filtered(&init_snap, |diag| {
                                let in_branch = branch_ranges
                                    .iter()
                                    .flatten()
                                    .any(|(start, end)| diag.start >= *start && diag.start < *end);
                                !(in_branch && diag.code == 2322)
                            });
                    }
                    let function_initializer_body_has_error = checker
                        .ctx
                        .arena
                        .get(var_decl.initializer)
                        .and_then(|init_node| {
                            if !matches!(
                                init_node.kind,
                                syntax_kind_ext::ARROW_FUNCTION
                                    | syntax_kind_ext::FUNCTION_EXPRESSION
                            ) {
                                return None;
                            }
                            let func = checker.ctx.arena.get_function(init_node)?;
                            let body_node = checker.ctx.arena.get(func.body)?;
                            if body_node.kind == syntax_kind_ext::BLOCK {
                                return Some(false);
                            }
                            Some(
                                checker.ctx.diagnostics[init_snap.diagnostics_len..]
                                    .iter()
                                    .any(|diag| {
                                        diag.start >= body_node.pos
                                            && diag.start < body_node.end
                                            && matches!(diag.code, 2322 | 2339)
                                    }),
                            )
                        })
                        .unwrap_or(false);
                    // Check assignability (skip for 'any' since anything is assignable to any,
                    // and skip for TypeId::ERROR since the type annotation failed to resolve).
                    // Note: we intentionally do NOT use type_contains_error() here because it
                    // recursively traverses all method/property types — interfaces like String
                    // have methods that reference unresolved lib types (e.g. Intl.CollatorOptions),
                    // causing type_contains_error to return true even though the declared type
                    // itself (String interface) is perfectly valid for assignability checking.
                    if declared_type != TypeId::ANY && declared_type != TypeId::ERROR {
                        // Augment function initializer with expando properties (suppresses spurious TS2741).
                        let checked_init_type = if initializer_is_function
                            && let Some(ref name) = var_name
                            && let Some(sym_id) = checker.ctx.binder.get_node_symbol(decl_idx)
                        {
                            checker.augment_callable_type_with_expandos(
                                name,
                                sym_id,
                                init_type_for_relation,
                            )
                        } else {
                            init_type_for_relation
                        };
                        if let Some((source_level, target_level)) =
                            checker.constructor_accessibility_mismatch_for_var_decl(var_decl)
                        {
                            checker.error_constructor_accessibility_not_assignable(
                                checked_init_type,
                                declared_type,
                                source_level,
                                target_level,
                                decl_idx,
                            );
                        } else if is_destructuring {
                            // For destructuring patterns, try element-level elaboration first
                            // (tsc reports TS2322 on each mismatching element), then fall back
                            // to a generic TS2322 error.
                            if !checker.try_elaborate_initializer_elements(
                                checked_init_type,
                                declared_type,
                                var_decl.initializer,
                            ) {
                                let _ = checker.check_assignable_or_report_generic_at(
                                    checked_init_type,
                                    declared_type,
                                    var_decl.initializer,
                                    decl_idx,
                                );
                            }
                        } else {
                            let handled_discriminated = checker
                                .try_discriminated_union_excess_check(
                                    checked_init_type,
                                    declared_type,
                                    var_decl.initializer,
                                );
                            if handled_discriminated {
                                // Discriminated union excess property check handled the error.
                                // tsc reports TS2353 against the narrowed member instead of
                                // a generic TS2322 for these cases.
                            } else {
                                let elaborated_elements = checker
                                    .try_elaborate_initializer_elements(
                                        checked_init_type,
                                        declared_type,
                                        var_decl.initializer,
                                    );
                                if elaborated_elements {
                                    // Elaboration emitted per-element TS2322 errors on the specific
                                    // mismatching array/tuple elements. Skip the generic TS2322.
                                } else if initializer_is_function
                                    && !checker.is_assignable_to(checked_init_type, declared_type)
                                    && checker.try_elaborate_assignment_source_error(
                                        var_decl.initializer,
                                        declared_type,
                                    )
                                {
                                    // Function initializer return elaboration emitted the canonical
                                    // nested TS2322 for a mismatching returned literal/expression.
                                } else if function_initializer_body_has_error {
                                    // The function initializer already produced the canonical body
                                    // diagnostic (for example on an expression-bodied arrow). Skip
                                    // the redundant outer assignment TS2322.
                                } else {
                                    // Run excess property check first for object literal
                                    // initializers. In tsc, TS2353 (excess property) takes
                                    // priority over TS2741/TS2322 (missing property).
                                    let diags_before = checker.ctx.diagnostics.len();
                                    checker.check_object_literal_excess_properties(
                                        checked_init_type,
                                        declared_type,
                                        var_decl.initializer,
                                    );
                                    if checker.ctx.diagnostics.len() == diags_before {
                                        // to an index-signature type) instead of on the outer assignment.
                                        // Only attempt elaboration when overall assignment fails and
                                        // the initializer is an object literal (arrays/tuples are
                                        // handled earlier by try_elaborate_initializer_elements).
                                        let is_object_literal_initializer = checker
                                            .ctx
                                            .arena
                                            .get(var_decl.initializer)
                                            .is_some_and(|init_node| {
                                                init_node.kind
                                                    == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                            });
                                        if is_object_literal_initializer
                                            && !checker.is_assignable_to(
                                                checked_init_type,
                                                declared_type,
                                            )
                                            && checker.try_elaborate_object_literal_properties_for_var_init(
                                                var_decl.initializer,
                                                declared_type,
                                            )
                                        {
                                        } else {
                                            // Disable callable-with-type-params suppression
                                            // for variable declarations. The suppression is
                                            // designed for class member checks (TS2416/TS2720)
                                            // but incorrectly hides real TS2322 errors when
                                            // a callable with outer-scope type params is
                                            // assigned to a concrete callable target.
                                            // (e.g., (cb: (x: string, ...rest: T) => void) => void
                                            //   vs (cb: (...args: never) => void) => void)
                                            checker.ctx.skip_callable_type_param_suppression.set(true);
                                            let _ = checker.check_assignable_or_report_at(
                                                checked_init_type,
                                                declared_type,
                                                var_decl.initializer,
                                                decl_idx,
                                            );
                                            checker.ctx.skip_callable_type_param_suppression.set(false);
                                        }
                                    }
                                }
                            }
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
                    && let Some(sym_id) = checker.ctx.binder.get_node_symbol(decl_idx)
                {
                    return checker
                        .ctx
                        .types
                        .unique_symbol(tsz_solver::SymbolRef(sym_id.0));
                }
                // Type annotation determines the final type
                return declared_type;
            }
            // No type annotation - infer from initializer
            if var_decl.initializer.is_some() {
                checker.report_malformed_jsdoc_satisfies_tags(decl_idx);
                checker.report_duplicate_jsdoc_satisfies_tags(decl_idx);
                // JSDoc @satisfies on variable declarations: provide contextual type
                // for the initializer so that object literal methods and arrow function
                // parameters get contextually typed from the satisfies type.
                // This mirrors the `satisfies Expr` TypeScript syntax behavior.
                let satisfies_info = checker.jsdoc_satisfies_annotation_with_pos(decl_idx);
                if let Some((sat_type, keyword_pos)) = satisfies_info {
                    let request = TypingRequest::with_contextual_type(sat_type);
                    let init_type =
                        checker.get_type_of_node_with_request(var_decl.initializer, &request);
                    // Check satisfies assignability
                    checker.ensure_relation_input_ready(init_type);
                    checker.ensure_relation_input_ready(sat_type);
                    if !checker.type_contains_error(sat_type) {
                        let _ = checker.check_satisfies_assignable_or_report(
                            init_type,
                            sat_type,
                            var_decl.initializer,
                            Some(keyword_pos),
                        );
                    }
                    return init_type;
                }
                checker.maybe_clear_checked_initializer_type_cache(var_decl.initializer);
                // When the binding pattern contains array sub-patterns and the
                // initializer has matching array literals, provide a contextual type
                // so array literals produce positional (tuple) types instead of widened
                // union arrays.  This matches tsc: `var [a, b] = [1, "hello"]` infers
                // a=number, b=string (tuple), not a=string|number (array).
                let request = if is_destructuring {
                    checker.declaration_pattern_initializer_request(
                        var_decl.name,
                        var_decl.initializer,
                        typing_request,
                    )
                } else {
                    checker.redeclaration_initializer_request(
                        decl_idx,
                        var_decl.name,
                        var_decl.initializer,
                    )
                };
                let mut init_type =
                    checker.get_type_of_node_with_request(var_decl.initializer, &request);
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
                    // When the initializer type is `any` or `unknown` (e.g. from
                    // a JSDoc `@type {*}` cast), the assertion determines the type.
                    // `literal_type_from_initializer` looks through parenthesized
                    // expressions and would find the inner literal (`null`), incorrectly
                    // overriding the cast result.
                    if init_type != TypeId::ANY
                        && init_type != TypeId::UNKNOWN
                        && let Some(literal_type) =
                            checker.literal_type_from_initializer(var_decl.initializer)
                    {
                        return literal_type;
                    }
                    // `const k = Symbol()` — infer unique symbol type.
                    // In TypeScript, const declarations initialized with Symbol() get
                    // a unique symbol type (typeof k), not the general `symbol` type.
                    if checker.is_symbol_call_initializer(var_decl.initializer)
                        && let Some(sym_id) = checker.ctx.binder.get_node_symbol(decl_idx)
                    {
                        return checker
                            .ctx
                            .types
                            .unique_symbol(tsz_solver::SymbolRef(sym_id.0));
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
                // Route null/undefined widening through the flow observation boundary.
                flow_boundary::widen_null_undefined_to_any(
                    checker.ctx.types,
                    widened,
                    checker.ctx.strict_null_checks(),
                )
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

        // TS7031: For destructuring patterns without type annotation or initializer,
        // emit TS7031 for each leaf binding element under noImplicitAny.
        // This must be done before the symbol check since destructuring declarations
        // don't get a symbol assigned to the declaration node itself.
        //
        // Skip for:
        // - catch clause variables (type is implicitly `any` or `unknown`)
        // - for-in/for-of loop variables (type comes from the iterable expression)
        if self.ctx.no_implicit_any()
            && !self.ctx.has_real_syntax_errors
            && !is_catch_variable
            && var_decl.type_annotation.is_none()
            && var_decl.initializer.is_none()
            && !self.is_for_in_or_of_variable_declaration(decl_idx)
        {
            let is_destructuring_pattern =
                self.ctx.arena.get(var_decl.name).is_some_and(|name_node| {
                    name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                });
            if is_destructuring_pattern {
                self.emit_implicit_any_for_var_destructuring(var_decl.name);
            }
        }

        if let Some(sym_id) = self.ctx.binder.get_node_symbol(decl_idx) {
            self.push_symbol_dependency(sym_id, true);
            // Snapshot whether symbol was already cached BEFORE compute_final_type.
            // If it was, any ERROR in the cache is from earlier resolution (e.g., use-before-def),
            // not from circular detection during this declaration's initializer processing.
            let sym_already_cached = self.ctx.symbol_types.contains_key(&sym_id);
            let var_decl_snap = self.ctx.snapshot_diagnostics();
            let mut final_type = compute_final_type(self);
            // Check if get_type_of_symbol cached ERROR specifically DURING compute_final_type.
            // This happens when the initializer (directly or indirectly) references the variable,
            // causing the node-level cycle detection to return ERROR.
            let sym_cached_as_error =
                !sym_already_cached && self.ctx.symbol_types.get(&sym_id) == Some(&TypeId::ERROR);
            let circular_return_sites = if self.ctx.no_implicit_any()
                && var_decl.type_annotation.is_none()
                && var_decl.initializer.is_some()
            {
                let consumed = self
                    .consume_circular_return_sites_for_initializer(sym_id, var_decl.initializer);
                self.retain_immediate_initializer_circular_return_sites(
                    var_decl.initializer,
                    consumed,
                )
            } else {
                Vec::new()
            };
            let has_recorded_circular_return = !circular_return_sites.is_empty();

            // TS2502: 'x' is referenced directly or indirectly in its own type annotation.
            // Skip this check when the variable already had a type from a prior value declaration
            // (including merged parameters). In that case, `typeof x` resolves to the
            // previously-established type, not circularly to itself.
            // This matches tsc behavior where `var p: Point; var p: typeof p;` is valid and
            // where `function f(x: A) { var x: typeof x; }` uses the parameter surface.
            let is_redeclaration = self.has_prior_value_declaration_for_symbol(decl_idx);
            if var_decl.type_annotation.is_some() && !is_redeclaration {
                let accessor_circular =
                    self.type_literal_has_circular_accessor_reference(var_decl.type_annotation);
                // Try AST-based check first (catches complex circularities that confuse the solver)
                let ast_circular = !accessor_circular
                    && self
                        .find_circular_reference_in_type_node(
                            var_decl.type_annotation,
                            sym_id,
                            false,
                        )
                        .is_some();
                // Then try semantic check
                let semantic_circular = !accessor_circular
                    && !ast_circular
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
                let transitive_circular = !accessor_circular
                    && !ast_circular
                    && !semantic_circular
                    && self.check_transitive_type_query_circularity(final_type, sym_id);
                if !accessor_circular
                    && (ast_circular || semantic_circular || transitive_circular)
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
                    crate::query_boundaries::common::widen_freshness(self.ctx.types, final_type);
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
                // For var redeclarations, do NOT overwrite the symbol type.
                // The first declaration's type is canonical. Overwriting with a
                // subsequent declaration's inferred type can corrupt recursive
                // type resolution chains (e.g., `typeof k` indexers resolve to
                // `any` after the symbol type is overwritten by a redeclaration).
                if !is_merged_interface && !is_redeclaration {
                    // Augment callable types with expando properties before caching.
                    if let Some(ref name) = var_name {
                        final_type =
                            self.augment_callable_type_with_expandos(name, sym_id, final_type);
                        if self.ctx.is_js_file() {
                            final_type =
                                self.augment_object_type_with_define_properties(name, final_type);
                            if var_decl.initializer.is_some()
                                && self
                                    .direct_commonjs_module_export_assignment_rhs(
                                        self.ctx.arena,
                                        var_decl.initializer,
                                    )
                                    .is_some()
                            {
                                final_type = self.ctx.types.factory().intersection2(
                                    final_type,
                                    self.current_file_commonjs_module_exports_namespace_type(),
                                );
                            }
                        }
                    }
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
            let is_in_for_in_or_for_of = self.is_var_decl_in_for_in_or_for_of(decl_idx);
            let raw_declared_type = if let Some(jsdoc_type) = jsdoc_declared_type {
                jsdoc_type
            } else if var_decl.type_annotation.is_none()
                && var_decl.initializer.is_none()
                && !is_in_for_in_or_for_of
            {
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
            let is_exported = self.is_declaration_exported(self.ctx.arena, decl_idx);
            if is_exported {
                if var_decl.type_annotation.is_some() {
                    self.maybe_report_private_name_in_exported_variable_type_annotation(
                        var_decl.name,
                        var_name.as_deref().unwrap_or(""),
                        var_decl.type_annotation,
                    );
                } else {
                    self.maybe_report_unnameable_exported_variable_type(
                        var_decl.name,
                        var_name.as_deref().unwrap_or(""),
                        var_decl.initializer,
                        final_type,
                    );
                }
            }
            // TS4094: Property of exported anonymous class type may not be private or protected.
            if is_exported && var_decl.initializer.is_some() {
                self.maybe_report_exported_anonymous_class_private_members(
                    var_decl.name,
                    var_decl.initializer,
                );
            }
            if self.ctx.no_implicit_any()
                && !self.ctx.has_real_syntax_errors
                && !sym_already_cached
                && var_decl.type_annotation.is_none()
                && var_decl.initializer.is_none()
                && raw_declared_type == TypeId::ANY
            {
                // Check if the variable name is a destructuring pattern
                let is_destructuring_pattern =
                    self.ctx.arena.get(var_decl.name).is_some_and(|name_node| {
                        name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                            || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                    });
                if !is_destructuring_pattern && let Some(ref name) = var_name {
                    if (is_ambient || is_const || is_exported) && !self.ctx.is_declaration_file() {
                        // TS7005: Ambient, const, and exported declarations emit at the declaration site.
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
                        // Bare declarations start as implicit-any even if later
                        // assignments let flow analysis recover a concrete type.
                        // TS7034 fires only when a nested capture observes the
                        // variable before it becomes definitely assigned.
                        self.ctx.pending_implicit_any_vars.insert(
                            sym_id,
                            PendingImplicitAnyVar {
                                name_node: var_decl.name,
                                kind: PendingImplicitAnyKind::CaptureOnly,
                            },
                        );
                    }
                }
            }
            let direct_empty_array_implicit_any = self.ctx.no_implicit_any()
                && !self.ctx.has_real_syntax_errors
                && !sym_already_cached
                && var_decl.type_annotation.is_none()
                && var_decl.initializer.is_some()
                && self
                    .ctx
                    .arena
                    .get(var_decl.initializer)
                    .is_some_and(|init_node| {
                        init_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                            && self
                                .ctx
                                .arena
                                .get_literal_expr(init_node)
                                .is_some_and(|lit| lit.elements.nodes.is_empty())
                    })
                && query::array_element_type(self.ctx.types, final_type) == Some(TypeId::ANY);
            if direct_empty_array_implicit_any {
                let is_destructuring_pattern =
                    self.ctx.arena.get(var_decl.name).is_some_and(|name_node| {
                        name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                            || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                    });
                if !is_destructuring_pattern {
                    self.ctx.pending_implicit_any_vars.insert(
                        sym_id,
                        PendingImplicitAnyVar {
                            name_node: var_decl.name,
                            kind: PendingImplicitAnyKind::EvolvingArray,
                        },
                    );
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
            let init_kind = self.ctx.arena.get(var_decl.initializer).map(|n| n.kind);
            let is_direct_deferred_initializer = init_kind.is_some_and(|kind| {
                matches!(
                    kind,
                    syntax_kind_ext::FUNCTION_EXPRESSION
                        | syntax_kind_ext::ARROW_FUNCTION
                        | syntax_kind_ext::CLASS_EXPRESSION
                )
            });
            // Check once whether all self-references are inside deferred contexts
            // (getter/setter/function/arrow/method/class bodies). Used by both
            // TS7022 paths to suppress false circularity diagnostics.
            let has_non_deferred_self_reference = self
                .initializer_has_non_deferred_self_reference(var_decl.initializer, sym_id)
                || var_name.as_ref().is_some_and(|name| {
                    self.initializer_has_non_deferred_self_reference_by_name(
                        var_decl.initializer,
                        name,
                    )
                });
            let all_refs_deferred = !has_non_deferred_self_reference;
            let has_type_wrapper = init_kind.is_some_and(|k| {
                matches!(
                    k,
                    syntax_kind_ext::SATISFIES_EXPRESSION | syntax_kind_ext::AS_EXPRESSION
                )
            });
            let has_jsdoc_satisfies_wrapper = {
                self.has_satisfies_jsdoc_comment(decl_idx)
                    || self.has_satisfies_jsdoc_comment(var_decl.initializer)
            };
            // When a var declaration merges with a parameter (e.g.,
            // `constructor(options?) { var options = (options || 0); }`),
            // the initializer reference to the parameter is not circular
            // because the parameter already has a known type.
            let is_merged_with_parameter =
                self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                    symbol.declarations.iter().any(|&d| {
                        self.ctx
                            .arena
                            .get(d)
                            .is_some_and(|n| n.kind == syntax_kind_ext::PARAMETER)
                    })
                });
            let is_skip_circularity = init_kind
                .is_some_and(|k| k == syntax_kind_ext::CLASS_EXPRESSION)
                || has_type_wrapper
                || has_jsdoc_satisfies_wrapper
                || all_refs_deferred
                || is_merged_with_parameter;
            if self.ctx.no_implicit_any()
                && var_decl.type_annotation.is_none()
                && var_decl.initializer.is_some()
                && has_recorded_circular_return
                && !has_jsdoc_satisfies_wrapper
                && !has_type_wrapper
                && !is_merged_with_parameter
                && !is_direct_deferred_initializer
            {
                self.suppress_circular_initializer_relation_diagnostics(
                    &var_decl_snap,
                    var_decl.initializer,
                );
                final_type = TypeId::ANY;
                if let Some(ref name) = var_name {
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node_msg(
                        var_decl.name,
                        diagnostic_codes::IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION_AND_IS_REFERE,
                        &[name],
                    );
                    for &site_idx in &circular_return_sites {
                        self.emit_circular_return_site_diagnostic(
                            site_idx,
                            Some(name.as_str()),
                            var_decl.name,
                            var_decl.initializer,
                        );
                    }
                }
            } else if self.ctx.no_implicit_any()
                && var_decl.type_annotation.is_none()
                && var_decl.initializer.is_some()
                && is_direct_deferred_initializer
            {
                let has_wrapped_self_call = self
                    .function_like_initializer_has_wrapped_self_call_in_return_expression(
                        var_decl.initializer,
                        sym_id,
                    );
                if has_wrapped_self_call {
                    final_type = TypeId::ANY;
                    if let Some(ref name) = var_name {
                        use crate::diagnostics::diagnostic_codes;
                        self.error_at_node_msg(
                            var_decl.name,
                            diagnostic_codes::IMPLICITLY_HAS_RETURN_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_RETURN_TYPE_ANNOTATION,
                            &[name],
                        );
                    }
                }
            } else if self.ctx.no_implicit_any()
                && var_decl.type_annotation.is_none()
                && var_decl.initializer.is_some()
                && !is_skip_circularity
                && sym_cached_as_error
            {
                // TS7022: The initializer has a non-deferred self-reference AND the
                // symbol was actually cached as ERROR during resolution (confirming
                // semantic circularity, not just an AST name match to a different
                // entity like an enum or namespace with the same name).
                final_type = TypeId::ANY;
                if let Some(ref name) = var_name {
                    use crate::diagnostics::diagnostic_codes;
                    self.error_at_node_msg(
                        var_decl.name,
                        diagnostic_codes::IMPLICITLY_HAS_TYPE_ANY_BECAUSE_IT_DOES_NOT_HAVE_A_TYPE_ANNOTATION_AND_IS_REFERE,
                        &[name],
                    );
                }
            } else if self.ctx.no_implicit_any()
                && var_decl.type_annotation.is_none()
                && var_decl.initializer.is_some()
                && sym_cached_as_error
                && self.type_contains_error(final_type)
            {
                // Class expressions resolve through the constructor type system.
                // Self-references like `let C = class { foo() { return new C(); } }`
                // are valid — skip circularity diagnostics for them.
                //
                // Object literals wrapped in `satisfies`/`as` have explicit type
                // context, so getter self-references like
                //   `const a = { get self() { return a; } } satisfies T`
                // are valid and should NOT get TS7022.  Bare object literals
                // like `var a = { f: a }` SHOULD still get TS7022.
                //
                // Self-references inside getter/setter/function/method bodies are
                // deferred (lazily evaluated) and should NOT trigger TS7022.
                // E.g., `const a = { get self() { return a; } }` or
                //        `const C = object({ get parent() { return optional(C); } })`
                if !is_skip_circularity {
                    let is_deferred_initializer =
                        self.ctx.arena.get(var_decl.initializer).is_some_and(|n| {
                            matches!(
                                n.kind,
                                syntax_kind_ext::FUNCTION_EXPRESSION
                                    | syntax_kind_ext::ARROW_FUNCTION
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
            }

            // Check for variable redeclaration in the current scope (TS2403).
            // Note: This applies specifically to 'var' merging where types must match.
            // let/const duplicates are caught earlier by the binder (TS2451).
            // Skip TS2403 for mergeable declarations (namespace, enum, class, interface, function overloads).
            // Bare declarations (`var x;` with no annotation/initializer) don't establish a
            // type constraint and never trigger TS2403 in tsc.
            //
            // Non-checked JS files should not participate in TS2403 at all.
            // tsc doesn't type-check JS files without checkJs, so they don't
            // establish `var_decl_types` entries. Without this guard, a JS file
            // processed before a TS file can set a bogus prev_type that causes
            // false TS2403 on the TS file's declaration.
            let is_non_checked_js = self.ctx.is_js_file() && !self.ctx.should_resolve_jsdoc();
            // Exception: for-in/for-of loop variables (`for (var x in obj)`) ARE typed
            // (string for for-in, element type for for-of) even without explicit annotation.
            let is_bare_declaration = var_decl.type_annotation.is_none()
                && var_decl.initializer.is_none()
                && !is_in_for_in_or_for_of;
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

            // TS2403 only applies to non-block-scoped variables (var).
            // Also skip when the var shares a block scope with a const/let of the same
            // name — that case is TS2481 (handled by check_var_declared_names_not_shadowed).
            let is_ts2481_case =
                !is_block_scoped && self.is_var_shadowing_block_scoped_in_same_scope(decl_idx);
            if !is_block_scoped && !is_ts2481_case {
                // Non-exported variables inside namespace bodies are local to that body.
                // They should not trigger TS2403 against exported variables of the same
                // name from other (merged) namespace bodies, even if the binder merged
                // their symbols.
                let current_ns_export_status = self.var_decl_namespace_export_status(decl_idx);
                let is_non_exported_ns_var = current_ns_export_status == Some(false);
                // Skip TS2403 when declarations in the same namespace have
                // different export visibility (one exported, one not). In tsc,
                // these are separate symbols (locals vs exports table) and
                // never compared for type identity.  TS2395 already covers
                // the visibility conflict.
                let has_ns_export_visibility_mismatch =
                    if let Some(current_exported) = current_ns_export_status {
                        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                            symbol.declarations.iter().any(|&other_decl| {
                                other_decl != decl_idx
                                    && other_decl.is_some()
                                    && self.var_decl_namespace_export_status(other_decl)
                                        == Some(!current_exported)
                            })
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                let local_decl_count = self
                    .ctx
                    .binder
                    .get_symbol(sym_id)
                    .map(|symbol| {
                        symbol
                            .declarations
                            .iter()
                            .filter(|&&decl| decl.is_some())
                            .count()
                    })
                    .unwrap_or(0);
                if let Some(prev_type) = self.ctx.var_decl_types.get(&sym_id).copied() {
                    if local_decl_count <= 1 {
                        let refined = self.refine_var_decl_type(prev_type, final_type);
                        if refined != prev_type && !is_non_checked_js {
                            self.ctx.var_decl_types.insert(sym_id, refined);
                        }
                        return;
                    }
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
                    // Skip TS2403 when the declarations are in different namespace body
                    // blocks (ModuleBlock nodes) of the same merged namespace. TSC treats
                    // each namespace body as a separate declaration context, so
                    // `namespace A { export var x: number; }` and
                    // `namespace A { export var x: string; }` don't conflict via TS2403.
                    let is_cross_namespace_body = if let Some(symbol) =
                        self.ctx.binder.get_symbol(sym_id)
                    {
                        symbol.declarations.iter().any(|&other_decl| {
                            other_decl != decl_idx
                                && other_decl.is_some()
                                && self
                                    .are_decls_in_different_namespace_bodies(decl_idx, other_decl)
                        })
                    } else {
                        false
                    };
                    // Unchecked JS files do not participate in TS2403, but checked
                    // JS (`// @ts-check` / checkJs) still uses redeclaration identity.
                    if !is_mergeable_declaration
                        && !has_ns_export_visibility_mismatch
                        && !is_cross_namespace_body
                        && !is_non_checked_js
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
                        if refined != prev_type && !is_non_checked_js {
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
                    // Only compare against lib declarations when the current variable
                    // is at global file scope. Variables in non-global scopes are
                    // distinct from lib declarations and never trigger TS2403:
                    // - Module files (files with imports/exports are module-scoped)
                    // - Namespace bodies (whether exported or not)
                    // - Function scopes (e.g. `var top` vs global `window.top`)
                    let is_in_namespace = current_ns_export_status.is_some();
                    let is_in_function_scope = self.find_enclosing_function(decl_idx).is_some();
                    let is_in_external_module = self.ctx.binder.is_external_module();
                    if let Some(name) = symbol_name {
                        for (arena, binder) in lib_contexts_data {
                            // Lookup by name in lib binder to ensure we find the matching symbol
                            // even if SymbolIds are not perfectly aligned across contexts.
                            if let Some(lib_sym_id) = binder.file_locals.get(&name)
                                && let Some(lib_sym) = binder.get_symbol(lib_sym_id)
                            {
                                // TS2403 only applies when the lib symbol has a VALUE
                                // declaration (variable, function, etc.). Type-only symbols
                                // (interfaces, type aliases) occupy a different declaration
                                // space and never conflict with var declarations.
                                use tsz_binder::symbols::symbol_flags;
                                if lib_sym.flags & symbol_flags::VALUE == 0 {
                                    continue;
                                }
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
                                        lib_checker.ctx.lib_contexts = lib_contexts.clone();
                                        let lib_type = lib_checker.get_type_of_node(lib_decl);
                                        CheckerState::leave_cross_arena_delegation();
                                        if !is_in_namespace && !is_in_external_module {
                                            // Check compatibility (skip for bare declarations).
                                            // Function-scoped variables shadow globals and
                                            // never trigger TS2403 against lib types.
                                            // Module-scoped variables don't merge with globals.
                                            if !is_in_function_scope
                                                && !is_bare_declaration
                                                && !is_non_checked_js
                                                && !self.are_var_decl_types_compatible(
                                                    lib_type,
                                                    raw_declared_type,
                                                )
                                                && let Some(ref name) = var_name
                                            {
                                                self.error_subsequent_variable_declaration(
                                                    name,
                                                    lib_type,
                                                    raw_declared_type,
                                                    decl_idx,
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
                    }
                    // 2. Check local declarations (in case of intra-file redeclaration)
                    if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                        for &other_decl in &symbol.declarations {
                            if other_decl == decl_idx {
                                break;
                            }
                            if other_decl.is_some() {
                                // For merged global symbols, the declarations list may
                                // contain NodeIndex values from OTHER files' arenas.
                                // Accessing them in the current arena yields wrong nodes
                                // (different declarations at the same index). Guard: verify
                                // the node at other_decl resolves to a declaration with the
                                // same name as our symbol. A name mismatch means the NodeIndex
                                // is from a different file's arena.
                                let name_matches = if let Some(ref expected_name) = var_name {
                                    self.get_declaration_name_text(other_decl)
                                        .is_some_and(|n| n == *expected_name)
                                } else {
                                    true // No name to compare, assume OK
                                };
                                if !name_matches {
                                    continue;
                                }
                                let other_is_bare = self.is_bare_var_declaration_node(other_decl)
                                    && !self.is_var_decl_in_for_in_or_for_of(other_decl);
                                let other_type = if other_is_bare {
                                    // Bare `var x;` declarations have type `any`.
                                    // tsc treats them as establishing type `any` for TS2403.
                                    TypeId::ANY
                                } else {
                                    let raw = self.get_type_of_node(other_decl);
                                    // get_type_of_node may return ERROR for parameter nodes since
                                    // they are not VariableDeclaration nodes. Compute the
                                    // parameter's declared type from its type annotation and
                                    // optional modifier so TS2403 can compare correctly.
                                    if raw == TypeId::ERROR
                                        && let Some(other_node) = self.ctx.arena.get(other_decl)
                                        && other_node.kind == syntax_kind_ext::PARAMETER
                                        && let Some(param) =
                                            self.ctx.arena.get_parameter(other_node)
                                    {
                                        let mut param_type = if param.type_annotation.is_some() {
                                            self.get_type_from_type_node(param.type_annotation)
                                        } else {
                                            TypeId::ANY
                                        };
                                        // Rest parameters (...args) have array type
                                        if param.dot_dot_dot_token {
                                            param_type = self.ctx.types.array(param_type);
                                        }
                                        // Optional parameters (?) include undefined in their type
                                        if param.question_token && param_type != TypeId::ANY {
                                            param_type = self
                                                .ctx
                                                .types
                                                .union2(param_type, TypeId::UNDEFINED);
                                        }
                                        param_type
                                    } else {
                                        raw
                                    }
                                };
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
                                    && !has_ns_export_visibility_mismatch
                                    && !self.are_var_decl_types_compatible(
                                        other_type,
                                        raw_declared_type,
                                    )
                                    && let Some(ref name) = var_name
                                {
                                    self.error_subsequent_variable_declaration(
                                        name,
                                        other_type,
                                        raw_declared_type,
                                        decl_idx,
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
                    // 3. Check cross-file global declarations (TS2403 across file boundaries).
                    // In script files (non-module), global `var` declarations merge across
                    // files. If the same name appears in another file with a different type,
                    // emit TS2403.
                    if prior_type_found.is_none()
                        && !is_bare_declaration
                        && !is_in_namespace
                        && !is_in_function_scope
                        && !is_in_external_module
                        && !is_non_checked_js
                        && let Some(ref name_str) = var_name
                    {
                        // Clone entries to avoid holding borrow on self during mutation.
                        let cross_file_entries: Vec<(usize, tsz_binder::SymbolId)> = self
                            .ctx
                            .global_file_locals_index
                            .as_ref()
                            .and_then(|idx| idx.get(name_str.as_str()))
                            .cloned()
                            .unwrap_or_default();
                        let all_arenas_opt = self.ctx.all_arenas.clone();
                        let all_binders_opt = self.ctx.all_binders.clone();
                        if let Some(all_arenas) = all_arenas_opt
                            && let Some(all_binders) = all_binders_opt
                            && !cross_file_entries.is_empty()
                        {
                            let current_file_idx = self.ctx.current_file_idx;
                            let types = self.ctx.types;
                            let compiler_options = self.ctx.compiler_options.clone();
                            let definition_store = self.ctx.definition_store.clone();
                            let lib_contexts = self.ctx.lib_contexts.clone();
                            let mut found_cross_file_type = false;
                            for &(file_idx, other_sym_id) in &cross_file_entries {
                                if found_cross_file_type {
                                    break;
                                }
                                // Only check against files with lower indices (earlier in
                                // the program). The first file to declare the variable
                                // establishes its type; subsequent files are checked against
                                // that established type. This matches tsc behavior.
                                if file_idx >= current_file_idx {
                                    continue;
                                }
                                let Some(other_binder) = all_binders.get(file_idx) else {
                                    continue;
                                };
                                // Only merge with other script files (non-module).
                                if other_binder.is_external_module {
                                    continue;
                                }
                                let Some(other_arena) = all_arenas.get(file_idx) else {
                                    continue;
                                };
                                let other_file_name = other_arena
                                    .source_files
                                    .first()
                                    .map(|sf| sf.file_name.clone())
                                    .unwrap_or_else(|| format!("cross-file-{file_idx}"));
                                // JavaScript declarations do not act as the source side of
                                // cross-file TS2403 comparisons. They can still influence
                                // later symbol/type resolution, but tsc does not issue
                                // subsequent-variable-declaration errors against them here.
                                if crate::context::is_js_file_name(&other_file_name) {
                                    continue;
                                }
                                let Some(other_sym) = other_binder.get_symbol(other_sym_id) else {
                                    continue;
                                };
                                // Find var declarations in the other file's symbol.
                                // Merged global symbols may contain NodeIndex values from
                                // multiple files. Verify each declaration belongs to this
                                // file's arena by checking the name matches.
                                for &other_decl in &other_sym.declarations {
                                    if !other_decl.is_some() {
                                        continue;
                                    }
                                    let Some(other_node) = other_arena.get(other_decl) else {
                                        continue;
                                    };
                                    // Guard: verify this NodeIndex resolves to a declaration
                                    // with the expected name in this arena.
                                    let decl_name_matches = other_arena
                                        .get(other_decl)
                                        .and_then(|n| {
                                            other_arena.get_variable_declaration(n).and_then(|vd| {
                                                other_arena
                                                    .get(vd.name)
                                                    .and_then(|name_node| {
                                                        other_arena.get_identifier(name_node)
                                                    })
                                                    .map(|id| {
                                                        other_arena.resolve_identifier_text(id)
                                                    })
                                            })
                                        })
                                        .is_some_and(|n| n == name_str.as_str());
                                    if !decl_name_matches {
                                        continue;
                                    }
                                    // Only compare against var declarations (not classes, namespaces, etc.)
                                    if other_node.kind
                                        != tsz_parser::parser::syntax_kind_ext::VARIABLE_DECLARATION
                                        && other_node.kind
                                            != tsz_parser::parser::syntax_kind_ext::PARAMETER
                                    {
                                        continue;
                                    }
                                    // Check if the other declaration is also a `var` (not let/const)
                                    if let Some(other_ext) = other_arena.get_extended(other_decl)
                                        && let Some(other_parent) =
                                            other_arena.get(other_ext.parent)
                                        && other_parent.kind
                                            == tsz_parser::parser::syntax_kind_ext::VARIABLE_DECLARATION_LIST
                                    {
                                        let other_flags = other_parent.flags as u32;
                                        use tsz_parser::parser::node_flags;
                                        if (other_flags
                                            & (node_flags::LET
                                                | node_flags::CONST
                                                | node_flags::USING))
                                            != 0
                                        {
                                            continue; // block-scoped, skip
                                        }
                                    }
                                    // Skip bare declarations in the other file
                                    let other_is_bare = other_arena
                                        .get(other_decl)
                                        .and_then(|n| other_arena.get_variable_declaration(n))
                                        .is_some_and(|d| {
                                            d.type_annotation.is_none() && d.initializer.is_none()
                                        });
                                    if other_is_bare {
                                        continue;
                                    }
                                    // Resolve the type of the cross-file declaration
                                    if !CheckerState::enter_cross_arena_delegation() {
                                        continue;
                                    }
                                    let mut cross_checker = CheckerState::new_with_shared_def_store(
                                        other_arena,
                                        other_binder,
                                        types,
                                        other_file_name.clone(),
                                        compiler_options.clone(),
                                        definition_store.clone(),
                                    );
                                    cross_checker.ctx.lib_contexts = lib_contexts.clone();
                                    let other_type = cross_checker.get_type_of_node(other_decl);
                                    CheckerState::leave_cross_arena_delegation();
                                    if other_type != TypeId::ERROR
                                        && !self.are_var_decl_types_compatible(
                                            other_type,
                                            raw_declared_type,
                                        )
                                    {
                                        self.error_subsequent_variable_declaration(
                                            name_str,
                                            other_type,
                                            raw_declared_type,
                                            decl_idx,
                                        );
                                    }
                                    prior_type_found = Some(if let Some(prev) = prior_type_found {
                                        self.refine_var_decl_type(prev, other_type)
                                    } else {
                                        other_type
                                    });
                                    found_cross_file_type = true;
                                    break; // One declaration per file is enough
                                }
                            }
                        }
                    }
                    let type_to_store = if let Some(prior) = prior_type_found {
                        self.refine_var_decl_type(prior, raw_declared_type)
                    } else {
                        raw_declared_type
                    };
                    // Always store the declared type, including bare declarations
                    // (`var x;` → type `any`). In tsc, bare declarations establish
                    // type `any` for TS2403 purposes, so subsequent declarations
                    // with different types correctly trigger TS2403.
                    // Skip for non-checked JS files to avoid polluting cross-file
                    // TS2403 checks with types from unchecked JavaScript sources.
                    if !is_non_checked_js {
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
            } else if let Some(inferred) =
                self.cached_inferred_variable_type(decl_idx, var_decl.name)
            {
                // Reuse the declaration's already-computed type so destructuring
                // element checks see the same request-aware initializer result.
                inferred
            } else if var_decl.initializer.is_some() {
                let initializer_request = self.declaration_pattern_initializer_request(
                    var_decl.name,
                    var_decl.initializer,
                    typing_request,
                );
                self.get_type_of_node_with_request(var_decl.initializer, &initializer_request)
            } else if is_catch_variable {
                flow_boundary::resolve_catch_variable_type(
                    self.ctx.use_unknown_in_catch_variables(),
                )
            } else if let Some(inferred) = self.compute_for_in_of_variable_type(decl_idx) {
                inferred
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
            let mut effective_pattern_type = pattern_type;
            if name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
                let is_iterable = self.check_destructuring_iterability(
                    var_decl.name,
                    pattern_type,
                    var_decl.initializer,
                );
                // When not iterable (e.g., `unknown` in catch clause), use ERROR
                // to suppress cascading diagnostics in nested patterns.
                if !is_iterable {
                    effective_pattern_type = TypeId::ERROR;
                }
                self.report_empty_array_destructuring_bounds(var_decl.name, var_decl.initializer);
            }

            // Ensure binding element identifiers get the correct inferred types.
            let binding_request = typing_request.read().contextual_opt(None);
            self.assign_binding_pattern_symbol_types_with_request(
                var_decl.name,
                effective_pattern_type,
                &binding_request,
            );
            self.check_binding_pattern_with_request(
                var_decl.name,
                effective_pattern_type,
                var_decl.type_annotation.is_some(),
                &binding_request,
            );

            // Record source expression for flow-based property narrowing.
            // When `const { bar } = aFoo` and `aFoo.bar` was narrowed by a condition,
            // the binding element `bar` should use the narrowed property type.
            if var_decl.initializer.is_some()
                && name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
            {
                self.record_destructured_binding_sources(var_decl.name, var_decl.initializer);
            }

            // Track destructured binding groups for correlated narrowing.
            // Only needed for union source types where narrowing one property affects others.
            let mut resolved_for_union = self.resolve_lazy_type(pattern_type);
            if query::union_members(self.ctx.types, resolved_for_union).is_none()
                && let Some(constraint) =
                    query::type_parameter_constraint(self.ctx.types, resolved_for_union)
            {
                resolved_for_union = self.evaluate_type_for_assignability(constraint);
            }
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
}

#[cfg(test)]
#[path = "core_tests.rs"]
mod core_tests;
