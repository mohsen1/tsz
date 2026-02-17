//! Name resolution error reporting (TS2304, TS2552, TS2583, TS2584).

use crate::diagnostics::{
    Diagnostic, DiagnosticCategory, diagnostic_codes, diagnostic_messages, format_message,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Name Resolution Errors
    // =========================================================================

    fn unresolved_name_matches_enclosing_param(&self, name: &str, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = idx;
        let mut guard = 0;
        while !current.is_none() {
            guard += 1;
            if guard > 256 {
                break;
            }

            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            let parent = ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                break;
            };

            let matches_param = match parent_node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION =>
                {
                    self.ctx
                        .arena
                        .get_function(parent_node)
                        .is_some_and(|func| {
                            func.parameters.nodes.iter().any(|&param_idx| {
                                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                                    return false;
                                };
                                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                                    return false;
                                };
                                let Some(name_node) = self.ctx.arena.get(param.name) else {
                                    return false;
                                };
                                self.ctx
                                    .arena
                                    .get_identifier(name_node)
                                    .is_some_and(|id| id.escaped_text == name)
                            })
                        })
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(parent_node)
                    .is_some_and(|method| {
                        method.parameters.nodes.iter().any(|&param_idx| {
                            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                                return false;
                            };
                            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                                return false;
                            };
                            let Some(name_node) = self.ctx.arena.get(param.name) else {
                                return false;
                            };
                            self.ctx
                                .arena
                                .get_identifier(name_node)
                                .is_some_and(|id| id.escaped_text == name)
                        })
                    }),
                k if k == syntax_kind_ext::CONSTRUCTOR => self
                    .ctx
                    .arena
                    .get_constructor(parent_node)
                    .is_some_and(|ctor| {
                        ctor.parameters.nodes.iter().any(|&param_idx| {
                            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                                return false;
                            };
                            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                                return false;
                            };
                            let Some(name_node) = self.ctx.arena.get(param.name) else {
                                return false;
                            };
                            self.ctx
                                .arena
                                .get_identifier(name_node)
                                .is_some_and(|id| id.escaped_text == name)
                        })
                    }),
                _ => false,
            };

            if matches_param {
                return true;
            }
            current = parent;
        }

        false
    }

    /// Check if a node is in a type-annotation context (type reference, implements, extends, etc.).
    /// Used to determine which symbol meaning to use for spelling suggestions.
    fn is_in_type_context(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        // Walk up the AST to find if we're inside a type annotation
        let mut current = idx;
        let mut guard = 0;
        while !current.is_none() {
            guard += 1;
            if guard > 64 {
                break;
            }
            if let Some(node) = self.ctx.arena.get(current) {
                match node.kind {
                    syntax_kind_ext::TYPE_REFERENCE
                    | syntax_kind_ext::HERITAGE_CLAUSE
                    | syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    | syntax_kind_ext::INTERFACE_DECLARATION
                    | syntax_kind_ext::TYPE_PARAMETER
                    | syntax_kind_ext::MAPPED_TYPE
                    | syntax_kind_ext::CONDITIONAL_TYPE
                    | syntax_kind_ext::INDEXED_ACCESS_TYPE
                    | syntax_kind_ext::UNION_TYPE
                    | syntax_kind_ext::INTERSECTION_TYPE
                    | syntax_kind_ext::ARRAY_TYPE
                    | syntax_kind_ext::TUPLE_TYPE
                    | syntax_kind_ext::TYPE_LITERAL
                    | syntax_kind_ext::FUNCTION_TYPE
                    | syntax_kind_ext::CONSTRUCTOR_TYPE
                    | syntax_kind_ext::PARENTHESIZED_TYPE
                    | syntax_kind_ext::TYPE_OPERATOR
                    | syntax_kind_ext::TYPE_QUERY
                    | syntax_kind_ext::INFER_TYPE => return true,
                    // Stop at expression/statement boundaries
                    syntax_kind_ext::FUNCTION_DECLARATION
                    | syntax_kind_ext::FUNCTION_EXPRESSION
                    | syntax_kind_ext::ARROW_FUNCTION
                    | syntax_kind_ext::CLASS_DECLARATION
                    | syntax_kind_ext::CLASS_EXPRESSION
                    | syntax_kind_ext::VARIABLE_STATEMENT
                    | syntax_kind_ext::EXPRESSION_STATEMENT
                    | syntax_kind_ext::BLOCK
                    | syntax_kind_ext::SOURCE_FILE => return false,
                    _ => {}
                }
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }
        false
    }

    /// Report a cannot find name error using solver diagnostics with source tracking.
    /// Enhanced to provide suggestions for similar names, import suggestions, and
    /// library change suggestions for ES2015+ types.
    pub fn error_cannot_find_name_at(&mut self, name: &str, idx: NodeIndex) {
        use tsz_binder::lib_loader;
        use tsz_parser::parser::node_flags;
        use tsz_parser::parser::syntax_kind_ext;

        // Keep TS2304 for ambiguous generic assertions such as `<<T>(x: T) => T>f`.
        // These nodes can carry parse-error flags, but TypeScript still reports
        // unresolved `T` alongside TS1005/TS1109.
        let force_emit_for_ambiguous_generic = self
            .ctx
            .arena
            .get(idx)
            .and_then(|node| {
                let source = self.ctx.arena.source_files.first()?.text.as_ref();
                let pos = node.pos as usize;
                if pos < 2 {
                    return Some(false);
                }
                let bytes = source.as_bytes();
                Some(
                    bytes.get(pos.saturating_sub(2)) == Some(&b'<')
                        && bytes.get(pos.saturating_sub(1)) == Some(&b'<'),
                )
            })
            .unwrap_or(false);

        let is_primitive_type_keyword = matches!(
            name,
            "number"
                | "string"
                | "boolean"
                | "symbol"
                | "void"
                | "undefined"
                | "null"
                | "any"
                | "unknown"
                | "never"
                | "object"
                | "bigint"
        );
        let is_import_equals_module_specifier = self
            .ctx
            .arena
            .get_extended(idx)
            .and_then(|ext| self.ctx.arena.get(ext.parent))
            .is_some_and(|parent_node| {
                if parent_node.kind
                    != tsz_parser::parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                {
                    return false;
                }
                self.ctx
                    .arena
                    .get_import_decl(parent_node)
                    .is_some_and(|imp| imp.module_specifier == idx)
            });

        if is_primitive_type_keyword && !is_import_equals_module_specifier {
            self.error_type_only_value_at(name, idx);
            return;
        }

        if !force_emit_for_ambiguous_generic
            && self.unresolved_name_matches_enclosing_param(name, idx)
        {
            return;
        }

        // In `import x = <expr>` module reference position, unresolved names should
        // report namespace/module diagnostics (TS2503/TS2307), not TS2304.
        let mut cur = idx;
        while let Some(ext) = self.ctx.arena.get_extended(cur) {
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            if let Some(parent_node) = self.ctx.arena.get(parent)
                && parent_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                return;
            }
            cur = parent;
        }

        // Skip TS2304 for identifiers that are clearly not valid names.
        // These are likely parse errors (e.g., ",", ";", "(", or empty names) that were
        // added to the AST for error recovery. The parse error should have
        // already been emitted (e.g., TS1003 "Identifier expected").
        if name.is_empty() {
            return;
        }
        let is_obviously_invalid = name.len() == 1
            && matches!(
                name.chars().next(),
                Some(
                    ',' | ';'
                        | ':'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '+'
                        | '-'
                        | '*'
                        | '/'
                        | '%'
                        | '&'
                        | '|'
                        | '^'
                        | '!'
                        | '~'
                        | '<'
                        | '>'
                        | '='
                        | '.'
                )
            );
        if is_obviously_invalid {
            return;
        }

        // In parse-recovery inside class bodies, contextual modifier keywords
        // can appear as pseudo-identifiers (e.g. `static f = 3` in a ctor).
        // Suppress TS2304 for those to avoid cascades after the primary syntax error.
        // Exception: computed property name expressions like `[public]` — tsc emits
        // TS2304 for these even in class/object-literal contexts with parse errors.
        // But NOT in enum contexts — tsc only emits TS1164 for enum computed names.
        let is_in_computed_property = self
            .ctx
            .arena
            .get_extended(idx)
            .and_then(|ext| {
                let parent = self.ctx.arena.get(ext.parent)?;
                if parent.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    return None;
                }
                // Exclude enum member context — tsc doesn't emit TS2304 for enum computed names
                let gp_ext = self.ctx.arena.get_extended(ext.parent)?;
                let gp = self.ctx.arena.get(gp_ext.parent)?;
                if gp.kind == syntax_kind_ext::ENUM_MEMBER {
                    return None;
                }
                Some(true)
            })
            .is_some();
        if self.has_parse_errors()
            && !is_in_computed_property
            && matches!(
                name,
                "static"
                    | "public"
                    | "private"
                    | "protected"
                    | "readonly"
                    | "abstract"
                    | "declare"
                    | "override"
                    | "accessor"
            )
        {
            let mut current = idx;
            let mut guard = 0;
            while !current.is_none() {
                guard += 1;
                if guard > 256 {
                    break;
                }
                let Some(node) = self.ctx.arena.get(current) else {
                    break;
                };
                if node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || node.kind == syntax_kind_ext::CLASS_EXPRESSION
                {
                    return;
                }
                let Some(ext) = self.ctx.arena.get_extended(current) else {
                    break;
                };
                if ext.parent.is_none() {
                    break;
                }
                current = ext.parent;
            }
        }

        // In parse-error files, identifiers inside class member bodies are often
        // parser-recovery artifacts (e.g. malformed `static` statements in ctors).
        // Suppress TS2304 there to avoid cascades from the primary syntax error.
        // Exception: computed property name expressions — tsc always emits TS2304 for these.
        if self.has_syntax_parse_errors() && !is_in_computed_property {
            let mut current = idx;
            let mut guard = 0;
            let mut in_class = false;
            let mut in_class_member_body = false;
            while !current.is_none() {
                guard += 1;
                if guard > 256 {
                    break;
                }
                let Some(node) = self.ctx.arena.get(current) else {
                    break;
                };
                if node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || node.kind == syntax_kind_ext::CLASS_EXPRESSION
                {
                    in_class = true;
                }
                if node.kind == syntax_kind_ext::CONSTRUCTOR
                    || node.kind == syntax_kind_ext::METHOD_DECLARATION
                    || node.kind == syntax_kind_ext::GET_ACCESSOR
                    || node.kind == syntax_kind_ext::SET_ACCESSOR
                {
                    in_class_member_body = true;
                }
                let Some(ext) = self.ctx.arena.get_extended(current) else {
                    break;
                };
                if ext.parent.is_none() {
                    break;
                }
                current = ext.parent;
            }
            if in_class && in_class_member_body {
                if is_primitive_type_keyword {
                    self.error_type_only_value_at(name, idx);
                }
                return;
            }
        }

        // Skip TS2304 for nodes that directly have parse errors, but only when
        // the file has real syntax parse errors (not just conflict markers TS1185).
        // Conflict markers are treated as trivia in TS and should not suppress
        // semantic "Cannot find name" diagnostics.
        // Exception: computed property name expressions — tsc always emits TS2304 for these.
        //
        // Only check the identifier itself and its direct parent — NOT distant
        // ancestors. Distant ancestor errors (e.g., enum declaration with TS1164)
        // should not suppress TS2304 for unrelated child expressions.
        if self.has_syntax_parse_errors() && !is_in_computed_property {
            if let Some(node) = self.ctx.arena.get(idx) {
                let flags = node.flags as u32;
                if !force_emit_for_ambiguous_generic
                    && (flags & node_flags::THIS_NODE_HAS_ERROR) != 0
                {
                    return;
                }
            }
            // Check immediate parent
            if let Some(ext) = self.ctx.arena.get_extended(idx)
                && !ext.parent.is_none()
                && let Some(parent_node) = self.ctx.arena.get(ext.parent)
            {
                let flags = parent_node.flags as u32;
                if !force_emit_for_ambiguous_generic
                    && (flags & node_flags::THIS_NODE_HAS_ERROR) != 0
                {
                    return;
                }
            }
        }

        // Also suppress TS2304 for identifiers that appear shortly after a parse error.
        // These identifiers are likely artifacts of error recovery.
        // Exception: computed property name expressions — tsc always emits TS2304 for these.
        if !force_emit_for_ambiguous_generic
            && !is_in_computed_property
            && !self.ctx.syntax_parse_error_positions.is_empty()
            && let Some(node) = self.ctx.arena.get(idx)
        {
            let ident_pos = node.pos;
            for &err_pos in &self.ctx.syntax_parse_error_positions {
                if err_pos < ident_pos && (ident_pos - err_pos) <= 4 {
                    return;
                }
            }
        }

        // In files with real syntax errors, unresolved names inside `typeof` type queries
        // are often cascades from malformed declaration syntax; TypeScript commonly keeps
        // the primary parse diagnostic only for these.
        if self.has_syntax_parse_errors() {
            let mut current = idx;
            let mut guard = 0;
            while !current.is_none() {
                guard += 1;
                if guard > 256 {
                    break;
                }
                if let Some(node) = self.ctx.arena.get(current)
                    && node.kind == syntax_kind_ext::TYPE_QUERY
                {
                    return;
                }
                let Some(ext) = self.ctx.arena.get_extended(current) else {
                    break;
                };
                if ext.parent.is_none() {
                    break;
                }
                current = ext.parent;
            }
        }

        if let Some(original_name) =
            self.unresolved_unused_renaming_property_in_type_query(name, idx)
        {
            let message = format!(
                "'{name}' is an unused renaming of '{original_name}'. Did you intend to use it as a type annotation?"
            );
            self.error_at_node(
                idx,
                &message,
                diagnostic_codes::IS_AN_UNUSED_RENAMING_OF_DID_YOU_INTEND_TO_USE_IT_AS_A_TYPE_ANNOTATION,
            );
            return;
        }

        // Check if this is an ES2015+ type that requires a specific lib
        // If so, emit TS2583 with a suggestion to change the lib
        if lib_loader::is_es2015_plus_type(name) {
            self.error_cannot_find_name_change_lib(name, idx);
            return;
        }

        // Check if this is a known DOM/ScriptHost global that requires the 'dom' lib
        // If so, emit TS2584 with a suggestion to include 'dom'
        if super::is_known_dom_global(name) {
            self.error_cannot_find_name_change_target_lib(name, idx);
            return;
        }

        // Check if this is a known Node.js global → TS2591
        if super::is_known_node_global(name) {
            self.error_cannot_find_name_install_node_types(name, idx);
            return;
        }

        // Check if this is a known test runner global → TS2582
        if super::is_known_test_runner_global(name) {
            self.error_cannot_find_name_install_test_types(name, idx);
            return;
        }

        // Keep TS2304 for accessibility modifier keywords recovered as identifiers.
        // tsc does not emit TS2552 suggestions (e.g. "private" -> "print") in these cases.
        let is_accessibility_modifier_name = matches!(name, "public" | "private" | "protected");
        let mut is_in_spread_element = false;
        let mut current = idx;
        let mut guard = 0;
        while !current.is_none() {
            guard += 1;
            if guard > 256 {
                break;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            if node.kind == syntax_kind_ext::SPREAD_ELEMENT {
                is_in_spread_element = true;
                break;
            }
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }
        // Keep TS2304 (no TS2552 suggestion) for `arguments` lookups.
        // TypeScript does not offer spelling suggestions for unresolved `arguments`.
        let is_arguments_name = name == "arguments";
        let suppress_spelling_suggestion =
            is_accessibility_modifier_name || is_in_spread_element || is_arguments_name;

        // Determine spelling suggestion meaning based on context.
        // In type positions (type annotations, implements clauses, type references),
        // only suggest TYPE-meaning symbols. In value positions, suggest VALUE symbols.
        // This matches tsc's getSpellingSuggestionForName behavior.
        let suggestion_meaning = if self.is_in_type_context(idx) {
            tsz_binder::symbol_flags::TYPE
        } else {
            tsz_binder::symbol_flags::VALUE
        };

        // Try to find similar identifiers in scope for better error messages
        if !suppress_spelling_suggestion
            && let Some(suggestions) = self.find_similar_identifiers(name, idx, suggestion_meaning)
            && !suggestions.is_empty()
        {
            // Use the first suggestion for "Did you mean?" error
            self.error_cannot_find_name_with_suggestions(name, &suggestions, idx);
            return;
        }

        // Fall back to standard error without suggestions
        if let Some(loc) = self.get_source_location(idx) {
            let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
                self.ctx.types,
                &self.ctx.binder.symbols,
                self.ctx.file_name.as_str(),
            )
            .with_def_store(&self.ctx.definition_store);
            let diag = builder.cannot_find_name(name, loc.start, loc.length());
            self.ctx
                .push_diagnostic(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report error 2318/2583: Cannot find global type 'X'.
    /// - TS2318: Cannot find global type (for @noLib tests)
    /// - TS2583: Cannot find name - suggests changing target library (for ES2015+ types)
    pub fn error_cannot_find_global_type(&mut self, name: &str, idx: NodeIndex) {
        use tsz_binder::lib_loader;

        // Check if this is an ES2015+ type that would require a specific lib
        let is_es2015_type = lib_loader::is_es2015_plus_type(name);

        if let Some(loc) = self.get_source_location(idx) {
            let (code, message) = if is_es2015_type {
                (
                    lib_loader::MISSING_ES2015_LIB_SUPPORT,
                    format!(
                        "Cannot find name '{name}'. Do you need to change your target library? Try changing the 'lib' compiler option to es2015 or later."
                    ),
                )
            } else {
                (
                    lib_loader::CANNOT_FIND_GLOBAL_TYPE,
                    format!("Cannot find global type '{name}'."),
                )
            };

            self.ctx.push_diagnostic(Diagnostic {
                code,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS2583: Cannot find name 'X' - suggest changing target library.
    ///
    /// This error is emitted when an ES2015+ global (Promise, Map, Set, Symbol, etc.)
    /// is used as a value but is not available in the current lib configuration.
    /// It provides a helpful suggestion to change the lib compiler option.
    pub fn error_cannot_find_name_change_lib(&mut self, name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(
                diagnostic_messages::CANNOT_FIND_NAME_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB,
                &[name],
            );
            self.ctx.push_diagnostic(Diagnostic {
                code: diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS2584: Cannot find name 'X' - suggest including 'dom' lib.
    ///
    /// This error is emitted when a known DOM/ScriptHost global (console, window,
    /// document, `HTMLElement`, etc.) is used but the 'dom' lib is not included.
    pub fn error_cannot_find_name_change_target_lib(&mut self, name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(
                diagnostic_messages::CANNOT_FIND_NAME_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB_2,
                &[name],
            );
            self.ctx.push_diagnostic(Diagnostic {
                code: diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_CHANGE_YOUR_TARGET_LIBRARY_TRY_CHANGING_THE_LIB_2,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS2591: Cannot find name 'X' - suggest installing @types/node and adding to tsconfig.
    /// tsc uses TS2591 (with "add 'node' to types field") when a tsconfig exists, and TS2580
    /// (without that suggestion) when there's no tsconfig. Since tsz is always invoked via
    /// tsconfig, we use TS2591 to match tsc's conformance output.
    pub fn error_cannot_find_name_install_node_types(&mut self, name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(
                diagnostic_messages::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE_2,
                &[name],
            );
            self.ctx.push_diagnostic(Diagnostic {
                code: diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE_2,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report TS2582: Cannot find name 'X' - suggest installing test runner types.
    pub fn error_cannot_find_name_install_test_types(&mut self, name: &str, idx: NodeIndex) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format_message(
                diagnostic_messages::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_A_TEST_RUNNER_TRY_N,
                &[name],
            );
            self.ctx.push_diagnostic(Diagnostic {
                code: diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_A_TEST_RUNNER_TRY_N,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report error 2304/2552: Cannot find name 'X' with suggestions.
    /// Provides a list of similar names that might be what the user intended.
    pub fn error_cannot_find_name_with_suggestions(
        &mut self,
        name: &str,
        suggestions: &[String],
        idx: NodeIndex,
    ) {
        // Skip TS2304 for identifiers that are clearly not valid names.
        // These are likely parse errors that were added to the AST for error recovery.
        let is_obviously_invalid = name.len() == 1
            && matches!(
                name.chars().next(),
                Some(
                    ',' | ';'
                        | ':'
                        | '('
                        | ')'
                        | '['
                        | ']'
                        | '{'
                        | '}'
                        | '+'
                        | '-'
                        | '*'
                        | '/'
                        | '%'
                        | '&'
                        | '|'
                        | '^'
                        | '!'
                        | '~'
                        | '<'
                        | '>'
                        | '='
                        | '.'
                )
            );
        if is_obviously_invalid {
            return;
        }

        if let Some(loc) = self.get_source_location(idx) {
            // Format the suggestions list
            let suggestions_text = if suggestions.len() == 1 {
                format!("'{}'", suggestions[0])
            } else {
                let formatted: Vec<String> = suggestions.iter().map(|s| format!("'{s}")).collect();
                formatted.join(", ")
            };

            let message = if suggestions.len() == 1 {
                format!("Cannot find name '{name}'. Did you mean {suggestions_text}?")
            } else {
                format!("Cannot find name '{name}'. Did you mean one of: {suggestions_text}?")
            };

            self.ctx.push_diagnostic(Diagnostic {
                code: if suggestions.len() == 1 {
                    diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN
                } else {
                    diagnostic_codes::CANNOT_FIND_NAME
                },
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report error 2552: Cannot find name 'X'. Did you mean 'Y'?
    pub fn error_cannot_find_name_did_you_mean_at(
        &mut self,
        name: &str,
        suggestion: &str,
        idx: NodeIndex,
    ) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format!("Cannot find name '{name}'. Did you mean '{suggestion}'?");
            self.ctx.push_diagnostic(Diagnostic {
                code: diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }

    /// Report error 2662: Cannot find name 'X'. Did you mean the static member 'C.X'?
    pub fn error_cannot_find_name_static_member_at(
        &mut self,
        name: &str,
        class_name: &str,
        idx: NodeIndex,
    ) {
        if let Some(loc) = self.get_source_location(idx) {
            let message = format!(
                "Cannot find name '{name}'. Did you mean the static member '{class_name}.{name}'?"
            );
            self.ctx.push_diagnostic(Diagnostic {
                code: diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN_THE_STATIC_MEMBER,
                category: DiagnosticCategory::Error,
                message_text: message,
                file: self.ctx.file_name.clone(),
                start: loc.start,
                length: loc.length(),
                related_information: Vec::new(),
            });
        }
    }
}
