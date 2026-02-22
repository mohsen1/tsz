use super::Printer;
use super::declarations_namespace::rewrite_enum_iife_for_namespace_export;
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

    pub(super) fn emit_function_declaration(&mut self, node: &Node, _idx: NodeIndex) {
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
        {
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
        }
        self.write("(");
        self.emit_function_parameters_js(&func.parameters.nodes);
        // Map closing `)` — scan backward from body start since parser may
        // include `)` in the last parameter node's range.
        {
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
            self.map_closing_paren_backward(search_start, search_end);
        }
        self.write(")");

        // No return type for JavaScript

        self.write_space();
        let prev_emitting_function_body_block = self.emitting_function_body_block;
        self.emitting_function_body_block = true;

        // Push temp scope and block scope for function body.
        // Each function has its own scope for variable renaming/shadowing.
        self.ctx.block_scope_state.enter_scope();
        self.push_temp_scope();
        self.prepare_logical_assignment_value_temps(func.body);
        let prev_in_generator = self.ctx.flags.in_generator;
        self.ctx.flags.in_generator = func.asterisk_token;
        self.emit(func.body);
        self.ctx.flags.in_generator = prev_in_generator;
        self.pop_temp_scope();
        self.ctx.block_scope_state.exit_scope();
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

    pub(super) fn emit_variable_declaration_list(&mut self, node: &Node) {
        // Variable declaration list is stored as VariableData
        let Some(decl_list) = self.arena.get_variable(node) else {
            return;
        };

        if self.ctx.target_es5 {
            self.emit_variable_declaration_list_es5(node);
            return;
        }

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
        if !decl_list.declarations.nodes.is_empty() {
            self.write(" ");
            self.emit_comma_separated(&decl_list.declarations.nodes);
        } else if !is_let {
            // TSC emits `var ;` and `const ;` (with space) for empty declarations,
            // but `let;` (no space) for empty let declarations.
            self.write(" ");
        }
    }

    pub(super) fn emit_variable_declaration(&mut self, node: &Node) {
        let Some(decl) = self.arena.get_variable_declaration(node) else {
            return;
        };

        self.emit_decl_name(decl.name);

        // Skip type annotation for JavaScript emit

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
        // Always use `let` inside namespace IIFEs (existing behavior).
        if self.in_namespace_iife {
            return true;
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
                | syntax_kind_ext::MODULE_DECLARATION => return true,
                _ => {
                    current = parent;
                }
            }
        }
        false
    }

    pub(super) fn emit_enum_declaration(&mut self, node: &Node, idx: NodeIndex) {
        let Some(enum_decl) = self.arena.get_enum(node) else {
            return;
        };

        // Skip ambient and const enums (declare/const enums are erased)
        if self
            .arena
            .has_modifier(&enum_decl.modifiers, SyntaxKind::DeclareKeyword)
            || self
                .arena
                .has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword)
        {
            self.skip_comments_for_erased_node(node);
            return;
        }

        // Transform enum to IIFE pattern for all targets
        {
            let mut transformer = EnumES5Transformer::new(self.arena);
            if let Some(source_text) = self.source_text {
                transformer.set_source_text(source_text);
            }
            if let Some(mut ir) = transformer.transform_enum(idx) {
                let mut printer = IRPrinter::with_arena(self.arena);
                printer.set_indent_level(self.writer.indent_level());
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
                        output = output[var_prefix.len()..].to_string();
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

                // Track enum name for subsequent namespace/enum merges.
                if !enum_name.is_empty() {
                    self.declared_namespace_names.insert(enum_name);
                }
            }
            // If transformer returns None (e.g., const enum), emit nothing
        }
    }

    pub(super) fn emit_enum_member(&mut self, node: &Node) {
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
    pub(super) fn emit_interface_declaration(&mut self, node: &Node) {
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
    pub(super) fn emit_type_alias_declaration(&mut self, node: &Node) {
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
}
