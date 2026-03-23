use super::super::Printer;
use super::namespace::rewrite_enum_iife_for_namespace_export;
use crate::transforms::enum_es5::EnumES5Transformer;
use crate::transforms::ir_printer::IRPrinter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    // =========================================================================
    // Declarations
    // =========================================================================

    pub(in crate::emitter) fn emit_function_declaration(&mut self, node: &Node, _idx: NodeIndex) {
        let Some(func) = self.arena.get_function(node) else {
            return;
        };

        // Skip ambient declarations (declare function)
        if self
            .arena
            .has_modifier(&func.modifiers, SyntaxKind::DeclareKeyword)
        {
            self.skip_comments_for_erased_node(node);
            return;
        }

        // For JavaScript emit: skip declaration-only functions (no body)
        // These are just type information in TypeScript (overload signatures)
        if func.body.is_none() {
            self.skip_comments_for_erased_node(node);
            return;
        }

        if func.is_async && self.ctx.needs_async_lowering && !func.asterisk_token {
            let func_name = if func.name.is_some() {
                self.get_identifier_text_idx(func.name)
            } else {
                String::new()
            };
            self.emit_async_function_es5(func, &func_name, "this");
            return;
        }

        if func.is_async {
            self.write("async ");
        }

        self.write("function");

        if func.asterisk_token {
            self.write("*");
        }

        // Name
        if func.name.is_some() {
            self.write_space();
            self.emit_decl_name(func.name);
        } else if let Some(override_name) = self.anonymous_default_export_name.clone() {
            // Anonymous default export: use the override name (e.g. `default_1`)
            self.write_space();
            self.write(&override_name);
        } else {
            // Space before ( for anonymous functions: `function ()` not `function()`
            self.write(" ");
        }

        // Skip comments inside type parameter list (e.g., `<T, U /*extends T*/>`)
        // since type parameters are stripped in JS output
        if let Some(ref type_params) = func.type_parameters {
            for &tp_idx in &type_params.nodes {
                if let Some(tp_node) = self.arena.get(tp_idx) {
                    self.skip_comments_in_range(tp_node.pos, tp_node.end);
                }
            }
        }

        // Parameters - only emit names, not types for JavaScript
        // Map opening `(` to its source position (after name/type params)
        let open_paren_pos = {
            let search_start = if let Some(ref tp) = func.type_parameters {
                tp.nodes
                    .last()
                    .and_then(|&idx| self.arena.get(idx))
                    .map_or(node.pos, |n| n.end)
            } else if func.name.is_some() {
                self.arena.get(func.name).map_or(node.pos, |n| n.end)
            } else {
                node.pos
            };
            self.map_token_after(search_start, node.end, b'(');
            self.pending_source_pos
                .map(|source_pos| source_pos.pos)
                .unwrap_or(search_start)
        };
        self.write("(");
        let search_start = func
            .parameters
            .nodes
            .first()
            .and_then(|&idx| self.arena.get(idx))
            .map_or(node.pos, |n| n.pos);
        let search_end = if func.body.is_some() {
            self.arena.get(func.body).map_or(node.end, |n| n.pos)
        } else {
            node.end
        };
        // Increment function_scope_depth before parameters so async arrows in
        // parameter defaults use `this` in __awaiter instead of `void 0`
        self.function_scope_depth += 1;
        self.emit_function_parameters_with_trailing_comments(
            &func.parameters.nodes,
            open_paren_pos,
            search_start,
            search_end,
        );
        self.write(")");

        // No return type for JavaScript — skip comments inside erased return type
        if !self.ctx.flags.in_declaration_emit
            && func.type_annotation.is_some()
            && let Some(type_node) = self.arena.get(func.type_annotation)
        {
            self.skip_comments_in_range(type_node.pos, type_node.end);
        }

        self.write_space();
        let prev_emitting_function_body_block = self.emitting_function_body_block;
        self.emitting_function_body_block = true;
        // Don't increment again — already incremented before parameter emission

        // Push temp scope and block scope for function body.
        // Each function has its own scope for variable renaming/shadowing.
        self.ctx.block_scope_state.enter_scope();
        self.push_temp_scope();
        // Save/restore declared_namespace_names so enum/namespace names from the
        // outer scope don't suppress declarations inside this function, and names
        // declared inside don't leak to sibling functions at the outer scope.
        let prev_declared = std::mem::take(&mut self.declared_namespace_names);
        self.prepare_logical_assignment_value_temps(func.body);
        let prev_in_generator = self.ctx.flags.in_generator;
        self.ctx.flags.in_generator = func.asterisk_token;
        self.emit(func.body);
        self.ctx.flags.in_generator = prev_in_generator;
        self.declared_namespace_names = prev_declared;
        self.pop_temp_scope();
        self.ctx.block_scope_state.exit_scope();
        self.function_scope_depth -= 1;
        self.emitting_function_body_block = prev_emitting_function_body_block;

        // Track function name to prevent duplicate var declarations for merged namespaces.
        // Function declarations provide their own declaration, so if a namespace merges
        // with this function, the namespace shouldn't emit `var name;`.
        if func.name.is_some() {
            let func_name = self.get_identifier_text_idx(func.name);
            if !func_name.is_empty() {
                self.declared_namespace_names.insert(func_name);
            }
        }
    }

    pub(in crate::emitter) fn emit_variable_declaration_list(&mut self, node: &Node) {
        // Variable declaration list is stored as VariableData
        let Some(decl_list) = self.arena.get_variable(node) else {
            return;
        };

        if self.ctx.target_es5 {
            self.emit_variable_declaration_list_es5(node);
            return;
        }

        // Check if any declaration has object rest that needs ES2018 lowering
        let has_object_rest = self.ctx.needs_es2018_lowering
            && decl_list
                .declarations
                .nodes
                .iter()
                .any(|&idx| self.decl_has_object_rest(idx));

        // Emit keyword based on node flags.
        let flags = node.flags as u32;
        let is_using = flags & tsz_parser::parser::node_flags::USING != 0;
        let is_const = flags & tsz_parser::parser::node_flags::CONST != 0;
        let is_let = flags & tsz_parser::parser::node_flags::LET != 0;
        let keyword = if is_using && self.ctx.options.target.supports_es2025() {
            // await using is encoded as USING | CONST
            if is_const { "await using" } else { "using" }
        } else if is_const {
            // For ES6+ targets, preserve const as-is even without initializer
            // (tsc preserves user's code even if it's a syntax error)
            "const"
        } else if is_let {
            "let"
        } else {
            "var"
        };
        self.write(keyword);

        if has_object_rest {
            // Emit declarations with object rest lowering
            self.write(" ");
            let mut first = true;
            for &decl_idx in &decl_list.declarations.nodes {
                if !first {
                    self.write(", ");
                }
                first = false;
                if self.decl_has_object_rest(decl_idx) {
                    self.emit_var_decl_with_object_rest(decl_idx);
                } else {
                    self.emit(decl_idx);
                }
            }
        } else if !decl_list.declarations.nodes.is_empty() {
            self.write(" ");
            self.emit_comma_separated(&decl_list.declarations.nodes);
        } else if !is_let {
            // TSC emits `var ;` and `const ;` (with space) for empty declarations,
            // but `let;` (no space) for empty let declarations.
            self.write(" ");
        }
    }

    pub(in crate::emitter) fn emit_variable_declaration(&mut self, node: &Node) {
        let Some(decl) = self.arena.get_variable_declaration(node) else {
            return;
        };

        self.emit_decl_name(decl.name);

        // Skip type annotation for JavaScript emit — consume any comments
        // inside the erased type annotation so they don't leak into output.
        if !self.ctx.flags.in_declaration_emit
            && decl.type_annotation.is_some()
            && let Some(type_node) = self.arena.get(decl.type_annotation)
        {
            self.skip_comments_in_range(type_node.pos, type_node.end);
        }

        if decl.initializer.is_none() {
            if self.emit_missing_initializer_as_void_0 {
                self.write(" = void 0");
            }
            return;
        }

        // Map the `=` to the source position after the name (matching tsc)
        if let Some(name_node) = self.arena.get(decl.name) {
            self.map_source_offset(name_node.end);
        }
        self.write(" = ");
        self.emit_expression(decl.initializer);
    }

    // =========================================================================
    // Declarations - Enum, Interface, Type Alias
    // =========================================================================

    /// Determines whether an enum declaration should use `let` instead of `var`.
    ///
    /// tsc uses `var` for top-level enums and `let` for block-scoped enums
    /// (inside functions, methods, namespaces) when targeting ES2015+.
    fn should_use_let_for_enum(&self, enum_idx: NodeIndex) -> bool {
        // Use `let` inside namespace IIFEs, but only at ES2015+ targets.
        // ES5 doesn't support `let`, so must always use `var`.
        if self.in_namespace_iife {
            return !self.ctx.target_es5;
        }
        // Only upgrade to `let` for ES2015+ targets.
        if self.ctx.target_es5 {
            return false;
        }
        // Walk parent chain to check if enum is inside a block scope
        // (function body, method, etc.) rather than at source file top level.
        let mut current = enum_idx;
        for _ in 0..32 {
            let Some(ext) = self.arena.get_extended(current) else {
                return false;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.arena.get(parent) else {
                return false;
            };
            match parent_node.kind {
                syntax_kind_ext::SOURCE_FILE => return false,
                syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::FUNCTION_EXPRESSION
                | syntax_kind_ext::ARROW_FUNCTION
                | syntax_kind_ext::METHOD_DECLARATION
                | syntax_kind_ext::CONSTRUCTOR
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR
                | syntax_kind_ext::MODULE_DECLARATION
                | syntax_kind_ext::BLOCK => return true,
                _ => {
                    current = parent;
                }
            }
        }
        false
    }

    pub(in crate::emitter) fn emit_enum_declaration(&mut self, node: &Node, idx: NodeIndex) {
        let Some(enum_decl) = self.arena.get_enum(node) else {
            return;
        };

        // Skip ambient enums (always erased) and const enums (erased unless preserveConstEnums)
        if self
            .arena
            .has_modifier(&enum_decl.modifiers, SyntaxKind::DeclareKeyword)
        {
            self.skip_comments_for_erased_node(node);
            return;
        }
        if self
            .arena
            .has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword)
            && !self.ctx.options.preserve_const_enums
        {
            self.skip_comments_for_erased_node(node);
            return;
        }

        // Transform enum to IIFE pattern for all targets
        {
            let mut transformer = EnumES5Transformer::new(self.arena);
            transformer.set_preserve_const_enums(self.ctx.options.preserve_const_enums);
            if let Some(source_text) = self.source_text {
                transformer.set_source_text(source_text);
            }
            // Pass previously-evaluated enum member values for cross-enum
            // reference resolution (e.g., `enum Bar { B = Foo.A }`)
            transformer.set_prior_enum_values(&self.prior_enum_member_values);
            transformer.set_prior_string_members(&self.prior_enum_string_members);
            if let Some(mut ir) = transformer.transform_enum(idx) {
                // Accumulate member values and string member names
                let enum_name_for_accum = transformer.current_enum_name_ref().to_string();
                if !enum_name_for_accum.is_empty() {
                    let entry = self
                        .prior_enum_member_values
                        .entry(enum_name_for_accum.clone())
                        .or_default();
                    for (k, &v) in transformer.get_member_values() {
                        entry.insert(k.clone(), v);
                    }
                    if !transformer.get_string_members().is_empty() {
                        let str_entry = self
                            .prior_enum_string_members
                            .entry(enum_name_for_accum)
                            .or_default();
                        for name in transformer.get_string_members() {
                            str_entry.insert(name.clone());
                        }
                    }
                }
                let mut printer = IRPrinter::with_arena(self.arena);
                printer.set_indent_level(self.writer.indent_level());
                printer.set_target_es5(self.ctx.target_es5);
                if let Some(source_text) = self.source_text_for_map() {
                    printer.set_source_text(source_text);
                }
                let enum_name = if enum_decl.name.is_some() {
                    self.get_identifier_text_idx(enum_decl.name)
                } else {
                    String::new()
                };

                // Fold namespace export into IIFE closing when emitting exported enums
                // in a namespace: `(Color = A.Color || (A.Color = {}))` instead of
                // separate `A.Color = Color;` statement.
                if let Some(ns_name) = self.enum_namespace_export.take() {
                    rewrite_enum_iife_for_namespace_export(&mut ir, &enum_name, &ns_name);
                }

                let mut output = printer.emit(&ir).to_string();
                if !enum_name.is_empty() && self.declared_namespace_names.contains(&enum_name) {
                    let var_prefix = format!("var {enum_name};\n");
                    if output.starts_with(&var_prefix) {
                        // Strip the var declaration and any leading indentation
                        // from the remaining IIFE text, since the main writer's
                        // ensure_indent() will re-add indentation.
                        output = output[var_prefix.len()..]
                            .trim_start_matches(' ')
                            .to_string();
                    }
                } else if !enum_name.is_empty() && self.should_use_let_for_enum(idx) {
                    // Inside a block scope (namespace IIFE or function body) at ES2015+,
                    // use `let` instead of `var` to preserve block scoping semantics.
                    let var_prefix = format!("var {enum_name};");
                    let let_prefix = format!("let {enum_name};");
                    if output.starts_with(&var_prefix) {
                        output = format!("{let_prefix}{}", &output[var_prefix.len()..]);
                    }
                }
                self.write(&output);

                // The enum IR emitter handles comments INSIDE the enum body,
                // so we must advance the main comment system past them to prevent
                // orphaned duplicate comments after the IIFE.
                let enum_close_pos = self.find_token_end_before_trivia(node.pos, node.end);
                while self.comment_emit_idx < self.all_comments.len()
                    && self.all_comments[self.comment_emit_idx].pos < enum_close_pos
                {
                    self.comment_emit_idx += 1;
                }
                // Don't emit trailing comments here — the source_file statement
                // loop handles them with proper next-sibling bounds, preventing
                // us from stealing comments that belong to subsequent statements
                // (e.g., `enum E { One }; // error` where `// error` belongs to `;`).

                // Track enum name for subsequent namespace/enum merges.
                if !enum_name.is_empty() {
                    self.declared_namespace_names.insert(enum_name);
                }
            }
            // If transformer returns None (e.g., const enum), emit nothing
        }
    }

    pub(in crate::emitter) fn emit_enum_member(&mut self, node: &Node) {
        let Some(member) = self.arena.get_enum_member(node) else {
            return;
        };

        self.emit(member.name);

        if member.initializer.is_some() {
            self.write(" = ");
            self.emit(member.initializer);
        }
    }

    /// Emit an interface declaration (for .d.ts declaration emit mode)
    pub(in crate::emitter) fn emit_interface_declaration(&mut self, node: &Node) {
        let Some(interface) = self.arena.get_interface(node) else {
            return;
        };

        self.write("interface ");
        self.emit(interface.name);

        // Type parameters
        if let Some(ref type_params) = interface.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.write("<");
            self.emit_comma_separated(&type_params.nodes);
            self.write(">");
        }

        // Heritage clauses - interfaces can extend multiple types
        if let Some(ref heritage_clauses) = interface.heritage_clauses {
            let mut first_extends = true;
            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = self.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.arena.get_heritage(clause_node) else {
                    continue;
                };
                // Interfaces only have extends clauses (no implements)
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }

                for (i, &type_idx) in heritage.types.nodes.iter().enumerate() {
                    if first_extends && i == 0 {
                        self.write(" extends ");
                        first_extends = false;
                    } else {
                        self.write(", ");
                    }
                    self.emit_heritage_expression(type_idx);
                }
            }
        }

        self.write(" {");
        self.write_line();
        self.increase_indent();

        for &member_idx in &interface.members.nodes {
            self.emit(member_idx);
            self.write_semicolon();
            self.write_line();
        }

        self.decrease_indent();
        self.write("}");
    }

    /// Emit a type alias declaration (for .d.ts declaration emit mode)
    pub(in crate::emitter) fn emit_type_alias_declaration(&mut self, node: &Node) {
        let Some(type_alias) = self.arena.get_type_alias(node) else {
            return;
        };

        self.write("type ");
        self.emit(type_alias.name);

        // Type parameters
        if let Some(ref type_params) = type_alias.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.write("<");
            self.emit_comma_separated(&type_params.nodes);
            self.write(">");
        }

        self.write(" = ");
        self.emit(type_alias.type_node);
        self.write_semicolon();
    }

    /// Emit `export as namespace X;` (UMD global namespace declaration).
    /// Only emitted in declaration mode (.d.ts); erased in JS output.
    pub(in crate::emitter) fn emit_namespace_export_declaration(&mut self, node: &Node) {
        let Some(export) = self.arena.get_export_decl(node) else {
            return;
        };

        self.write("export as namespace ");
        self.emit(export.export_clause);
        self.write_semicolon();
    }
}
