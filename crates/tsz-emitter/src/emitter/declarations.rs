use super::{Printer, ScriptTarget};
use crate::transforms::ClassES5Emitter;
use crate::transforms::enum_es5::EnumES5Transformer;
use crate::transforms::ir_printer::IRPrinter;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
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
        if self.has_declare_modifier(&func.modifiers) {
            return;
        }

        // For JavaScript emit: skip declaration-only functions (no body)
        // These are just type information in TypeScript
        if func.body.is_none() {
            return;
        }

        if func.is_async && self.ctx.needs_async_lowering && !func.asterisk_token {
            let func_name = if !func.name.is_none() {
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
        if !func.name.is_none() {
            self.write_space();
            self.emit(func.name);
        }

        // Parameters - only emit names, not types for JavaScript
        self.write("(");
        self.emit_function_parameters_js(&func.parameters.nodes);
        self.write(")");

        // No return type for JavaScript

        self.write_space();

        // Push temp scope for function body - each function gets fresh temp variables
        self.push_temp_scope();
        self.emit(func.body);
        self.pop_temp_scope();

        // Track function name to prevent duplicate var declarations for merged namespaces.
        // Function declarations provide their own declaration, so if a namespace merges
        // with this function, the namespace shouldn't emit `var name;`.
        if !func.name.is_none() {
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
        let is_const = flags & tsz_parser::parser::node_flags::CONST != 0;
        let is_let = flags & tsz_parser::parser::node_flags::LET != 0;
        let keyword = if is_const {
            // For ES6+ targets, preserve const as-is even without initializer
            // (tsc preserves user's code even if it's a syntax error)
            "const"
        } else if is_let {
            "let"
        } else {
            "var"
        };
        self.write(keyword);
        // Only write space if there are declarations to emit
        if !decl_list.declarations.nodes.is_empty() {
            self.write(" ");
        }

        self.emit_comma_separated(&decl_list.declarations.nodes);
    }

    pub(super) fn emit_variable_declaration(&mut self, node: &Node) {
        let Some(decl) = self.arena.get_variable_declaration(node) else {
            return;
        };

        self.emit(decl.name);

        // Skip type annotation for JavaScript emit

        if decl.initializer.is_none() {
            if self.emit_missing_initializer_as_void_0 {
                self.write(" = void 0");
            }
            return;
        }

        self.write(" = ");
        self.emit_expression(decl.initializer);
    }

    // =========================================================================
    // Classes
    // =========================================================================

    /// Emit a class declaration.
    pub(super) fn emit_class_declaration(&mut self, node: &Node, idx: NodeIndex) {
        let Some(class) = self.arena.get_class(node) else {
            return;
        };

        // Skip ambient declarations (declare class)
        if self.has_declare_modifier(&class.modifiers) {
            return;
        }

        if self.ctx.target_es5 {
            let mut es5_emitter = ClassES5Emitter::new(self.arena);
            es5_emitter.set_indent_level(self.writer.indent_level());
            // Pass transform directives to the ClassES5Emitter
            es5_emitter.set_transforms(self.transforms.clone());
            if let Some(text) = self.source_text_for_map() {
                if self.writer.has_source_map() {
                    es5_emitter.set_source_map_context(text, self.writer.current_source_index());
                } else {
                    es5_emitter.set_source_text(text);
                }
            }
            let output = es5_emitter.emit_class(idx);
            let mappings = es5_emitter.take_mappings();
            if !mappings.is_empty() && self.writer.has_source_map() {
                self.writer.write("");
                let base_line = self.writer.current_line();
                let base_column = self.writer.current_column();
                self.writer
                    .add_offset_mappings(base_line, base_column, &mappings);
                self.writer.write(&output);
            } else {
                self.write(&output);
            }
            // Skip comments within the class body range since the ES5 class emitter
            // handles them separately. Without this, they'd appear at end of file.
            let class_end = node.end;
            while self.comment_emit_idx < self.all_comments.len()
                && self.all_comments[self.comment_emit_idx].end <= class_end
            {
                self.comment_emit_idx += 1;
            }
            return;
        }

        self.emit_class_es6(node, idx);
    }

    /// Emit a class using ES6 native class syntax (no transforms).
    /// This is the pure emission logic that can be reused by both the old API
    /// and the new transform system.
    pub(super) fn emit_class_es6(&mut self, node: &Node, _idx: NodeIndex) {
        let Some(class) = self.arena.get_class(node) else {
            return;
        };

        // Emit modifiers (including decorators) - skip export/default for CommonJS
        if let Some(ref modifiers) = class.modifiers {
            for &mod_idx in &modifiers.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    // Skip export/default modifiers in CommonJS mode or namespace IIFE
                    if (self.ctx.is_commonjs() || self.in_namespace_iife)
                        && (mod_node.kind == SyntaxKind::ExportKeyword as u16
                            || mod_node.kind == SyntaxKind::DefaultKeyword as u16)
                    {
                        continue;
                    }
                    self.emit(mod_idx);
                    // Add space or newline after decorator
                    if mod_node.kind == syntax_kind_ext::DECORATOR {
                        self.write_line();
                    } else {
                        self.write_space();
                    }
                }
            }
        }

        self.write("class");

        if !class.name.is_none() {
            self.write_space();
            self.emit(class.name);
        }

        if let Some(ref heritage_clauses) = class.heritage_clauses {
            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = self.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.arena.get_heritage(clause_node) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }

                if let Some(&extends_type) = heritage.types.nodes.first() {
                    self.write(" extends ");
                    self.emit_heritage_expression(extends_type);
                }
                break;
            }
        }

        self.write(" {");
        self.write_line();
        self.increase_indent();

        // Check if we need to lower class fields to constructor (for targets < ES2022)
        let needs_class_field_lowering =
            (self.ctx.options.target as u32) < (ScriptTarget::ES2022 as u32);

        // Collect property initializers that need lowering
        let mut field_inits: Vec<(String, NodeIndex)> = Vec::new();
        let mut static_field_inits: Vec<(String, NodeIndex, u32)> = Vec::new(); // (name, init, member_pos)
        if needs_class_field_lowering {
            for &member_idx in &class.members.nodes {
                if let Some(member_node) = self.arena.get(member_idx) {
                    if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                        if let Some(prop) = self.arena.get_property_decl(member_node) {
                            if prop.initializer.is_none()
                                || self.has_modifier(
                                    &prop.modifiers,
                                    SyntaxKind::AbstractKeyword as u16,
                                )
                            {
                                continue;
                            }
                            let name = self.get_identifier_text_idx(prop.name);
                            if name.is_empty() {
                                continue;
                            }
                            if self.has_modifier(&prop.modifiers, SyntaxKind::StaticKeyword as u16)
                            {
                                static_field_inits.push((name, prop.initializer, member_node.pos));
                            } else {
                                field_inits.push((name, prop.initializer));
                            }
                        }
                    }
                }
            }
        }

        // Check if class has an explicit constructor
        let has_constructor = class.members.nodes.iter().any(|&idx| {
            self.arena
                .get(idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::CONSTRUCTOR)
        });

        // Check if class has extends clause
        let has_extends = class.heritage_clauses.as_ref().is_some_and(|clauses| {
            clauses.nodes.iter().any(|&idx| {
                self.arena
                    .get(idx)
                    .and_then(|n| self.arena.get_heritage(n))
                    .is_some_and(|h| h.token == SyntaxKind::ExtendsKeyword as u16)
            })
        });

        // Store field inits for constructor emission
        let prev_field_inits = std::mem::take(&mut self.pending_class_field_inits);
        if !field_inits.is_empty() {
            self.pending_class_field_inits = field_inits.clone();
        }

        // If no constructor but we have field inits, synthesize one
        let synthesize_constructor = !has_constructor && !field_inits.is_empty();

        if synthesize_constructor {
            if has_extends {
                self.write("constructor() {");
                self.write_line();
                self.increase_indent();
                self.write("super(...arguments);");
                self.write_line();
            } else {
                self.write("constructor() {");
                self.write_line();
                self.increase_indent();
            }
            for (name, init_idx) in &field_inits {
                self.write("this.");
                self.write(name);
                self.write(" = ");
                self.emit_expression(*init_idx);
                self.write(";");
                self.write_line();
            }
            self.decrease_indent();
            self.write("}");
            self.write_line();
        }

        for &member_idx in &class.members.nodes {
            // Skip property declarations that were lowered
            if needs_class_field_lowering {
                if let Some(member_node) = self.arena.get(member_idx) {
                    if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                        if let Some(prop) = self.arena.get_property_decl(member_node) {
                            if !prop.initializer.is_none()
                                && !self.has_modifier(
                                    &prop.modifiers,
                                    SyntaxKind::AbstractKeyword as u16,
                                )
                            {
                                continue; // Skip - lowered to constructor or after class
                            }
                        }
                    }
                }
            }

            // Emit leading comments before this member
            if let Some(member_node) = self.arena.get(member_idx) {
                self.emit_comments_before_pos(member_node.pos);
            }

            let before_len = self.writer.len();
            self.emit(member_idx);
            // Only add newline if something was actually emitted
            if self.writer.len() > before_len && !self.writer.is_at_line_start() {
                // Emit trailing comments on the same line as the member.
                // node.end includes trailing trivia (comments), so we scan backward
                // to find the actual end of the last token, then scan forward for comments.
                if let Some(member_node) = self.arena.get(member_idx) {
                    let token_end =
                        self.find_token_end_before_trivia(member_node.pos, member_node.end);
                    self.emit_trailing_comments(token_end);
                }
                self.write_line();
            }
        }

        // Restore field inits
        self.pending_class_field_inits = prev_field_inits;

        self.decrease_indent();
        self.write("}");

        // Emit static field initializers after class body: ClassName.field = value;
        if !static_field_inits.is_empty() {
            let class_name = self.get_identifier_text_idx(class.name);
            if !class_name.is_empty() {
                self.write_line();
                for (name, init_idx, member_pos) in &static_field_inits {
                    // Emit leading comment from the original static property declaration
                    self.emit_comments_before_pos(*member_pos);
                    self.write(&class_name);
                    self.write(".");
                    self.write(name);
                    self.write(" = ");
                    self.emit_expression(*init_idx);
                    self.write(";");
                }
            }
        }

        // Track class name to prevent duplicate var declarations for merged namespaces.
        // When a class and namespace have the same name (declaration merging), the class
        // provides the declaration, so the namespace shouldn't emit `var name;`.
        if !class.name.is_none() {
            let class_name = self.get_identifier_text_idx(class.name);
            if !class_name.is_empty() {
                self.declared_namespace_names.insert(class_name);
            }
        }
    }

    // =========================================================================
    // Declarations - Enum, Interface, Type Alias
    // =========================================================================

    pub(super) fn emit_enum_declaration(&mut self, node: &Node, idx: NodeIndex) {
        let Some(enum_decl) = self.arena.get_enum(node) else {
            return;
        };

        // Skip ambient and const enums (declare/const enums are erased)
        if self.has_declare_modifier(&enum_decl.modifiers)
            || self.has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword as u16)
        {
            return;
        }

        // Transform enum to IIFE pattern for all targets
        {
            let mut transformer = EnumES5Transformer::new(self.arena);
            if let Some(ir) = transformer.transform_enum(idx) {
                let mut printer = IRPrinter::with_arena(self.arena);
                printer.set_indent_level(self.writer.indent_level());
                if let Some(source_text) = self.source_text_for_map() {
                    printer.set_source_text(source_text);
                }
                self.write(&printer.emit(&ir));

                // Track enum name to prevent duplicate var declarations for merged namespaces.
                // Enums always emit `var name;` so if a namespace merges with this enum,
                // the namespace shouldn't emit another var declaration.
                if !enum_decl.name.is_none() {
                    let enum_name = self.get_identifier_text_idx(enum_decl.name);
                    if !enum_name.is_empty() {
                        self.declared_namespace_names.insert(enum_name);
                    }
                }
                return;
            }
            // If transformer returns None (e.g., const enum), emit nothing
        }
    }

    pub(super) fn emit_enum_member(&mut self, node: &Node) {
        let Some(member) = self.arena.get_enum_member(node) else {
            return;
        };

        self.emit(member.name);

        if !member.initializer.is_none() {
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

    pub(super) fn emit_module_declaration(&mut self, node: &Node, idx: NodeIndex) {
        let Some(module) = self.arena.get_module(node) else {
            return;
        };

        // Skip ambient module declarations (declare namespace/module)
        if self.has_declare_modifier(&module.modifiers) {
            return;
        }

        // ES5 target: Transform namespace to IIFE pattern
        if self.ctx.target_es5 {
            use crate::transforms::NamespaceES5Emitter;
            let mut es5_emitter = NamespaceES5Emitter::new(self.arena);

            // Set IRPrinter indent to 0 because we'll handle base indentation through
            // the writer when writing each line. This prevents double-indentation for
            // nested namespaces where the writer is already indented.
            es5_emitter.set_indent_level(0);

            if let Some(text) = self.source_text_for_map() {
                es5_emitter.set_source_text(text);
            }
            let output = es5_emitter.emit_namespace(idx);

            // Write the namespace output line by line, letting the writer handle indentation.
            // IRPrinter generates relative indentation (nested constructs indented relative
            // to each other), and the writer adds the base indentation for our current scope.
            let trimmed = output.trim_end_matches('\n');
            for (i, line) in trimmed.lines().enumerate() {
                if i > 0 {
                    self.write_line();
                }
                self.write(line);
            }

            // Skip comments within the namespace body range since the ES5 namespace emitter
            // doesn't use the main comment system. Without this, comments would be dumped
            // at end of file.
            self.skip_comments_for_erased_node(node);
            return;
        }

        // ES6+: Emit namespace as IIFE, preserving ES6+ syntax inside
        let module = module.clone();
        let parent_name = self.current_namespace_name.clone();
        self.emit_namespace_iife(&module, parent_name.as_deref());
    }

    /// Emit a namespace/module as an IIFE for ES6+ targets.
    /// `parent_name` is set when this is a nested namespace (e.g., Bar inside Foo).
    fn emit_namespace_iife(
        &mut self,
        module: &tsz_parser::parser::node::ModuleData,
        parent_name: Option<&str>,
    ) {
        let name = self.get_identifier_text_idx(module.name);

        // Only emit var/let declaration if not already declared
        if !self.declared_namespace_names.contains(&name) {
            // Nested namespaces inside a namespace body use `let`
            // Dotted namespaces (Foo.Bar) use `var` for the inner part
            let keyword = if self.in_namespace_iife && parent_name.is_none() {
                "let"
            } else {
                "var"
            };
            self.write(keyword);
            self.write(" ");
            self.write(&name);
            self.write(";");
            self.write_line();
            self.declared_namespace_names.insert(name.clone());
        }

        // Emit: (function (<name>) {
        self.write("(function (");
        self.write(&name);
        self.write(") {");
        self.write_line();
        self.increase_indent();

        // Check if body is another MODULE_DECLARATION (nested: namespace Foo.Bar)
        if let Some(body_node) = self.arena.get(module.body) {
            if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                // Nested namespace
                if let Some(inner_module) = self.arena.get_module(body_node) {
                    let inner_module = inner_module.clone();
                    self.emit_namespace_iife(&inner_module, Some(&name));
                }
            } else {
                // MODULE_BLOCK: emit body statements
                let prev = self.in_namespace_iife;
                let prev_ns_name = self.current_namespace_name.clone();
                self.in_namespace_iife = true;
                self.current_namespace_name = Some(name.clone());
                self.emit_namespace_body_statements(module, &name);
                self.in_namespace_iife = prev;
                self.current_namespace_name = prev_ns_name;
            }
        }

        self.decrease_indent();
        // Closing: })(name || (name = {})); or
        // })(name = parent.name || (parent.name = {}));
        self.write("})(");
        if let Some(parent) = parent_name {
            self.write(&name);
            self.write(" = ");
            self.write(parent);
            self.write(".");
            self.write(&name);
            self.write(" || (");
            self.write(parent);
            self.write(".");
            self.write(&name);
            self.write(" = {}));");
        } else {
            self.write(&name);
            self.write(" || (");
            self.write(&name);
            self.write(" = {}));");
        }
        self.write_line();
    }

    /// Emit body statements of a namespace IIFE, handling exports.
    fn emit_namespace_body_statements(
        &mut self,
        module: &tsz_parser::parser::node::ModuleData,
        ns_name: &str,
    ) {
        let ns_name = ns_name.to_string();
        if let Some(body_node) = self.arena.get(module.body) {
            if let Some(block) = self.arena.get_module_block(body_node) {
                if let Some(ref stmts) = block.statements {
                    for &stmt_idx in &stmts.nodes {
                        let Some(stmt_node) = self.arena.get(stmt_idx) else {
                            continue;
                        };

                        // Skip erased declarations (interface, type alias) and their comments
                        if stmt_node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                            || stmt_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                        {
                            self.skip_comments_for_erased_node(stmt_node);
                            continue;
                        }

                        // Also handle export { interface/type } by checking export clause
                        if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                            if let Some(export) = self.arena.get_export_decl(stmt_node) {
                                let inner_kind = self
                                    .arena
                                    .get(export.export_clause)
                                    .map(|n| n.kind)
                                    .unwrap_or(0);
                                if inner_kind == syntax_kind_ext::INTERFACE_DECLARATION
                                    || inner_kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                                {
                                    self.skip_comments_for_erased_node(stmt_node);
                                    continue;
                                }
                            }
                        }

                        // Emit leading comments before this statement
                        self.emit_comments_before_pos(stmt_node.pos);

                        if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                            // Strip "export" and handle inner clause
                            if let Some(export) = self.arena.get_export_decl(stmt_node) {
                                let inner_idx = export.export_clause;
                                let inner_kind =
                                    self.arena.get(inner_idx).map(|n| n.kind).unwrap_or(0);

                                if inner_kind == syntax_kind_ext::VARIABLE_STATEMENT {
                                    // export var x = 10; → ns.x = 10;
                                    self.emit_namespace_exported_variable(inner_idx, &ns_name);
                                } else {
                                    // class/function/enum: emit without export, then add assignment
                                    let export_names = self.get_export_names_from_clause(inner_idx);
                                    self.emit(inner_idx);

                                    if !export_names.is_empty() {
                                        if !self.writer.is_at_line_start() {
                                            self.write_line();
                                        }
                                        for export_name in &export_names {
                                            self.write(&ns_name);
                                            self.write(".");
                                            self.write(export_name);
                                            self.write(" = ");
                                            self.write(export_name);
                                            self.write(";");
                                            self.write_line();
                                        }
                                    } else if inner_kind != syntax_kind_ext::MODULE_DECLARATION {
                                        // Don't write extra newline for namespaces - they already call write_line()
                                        self.write_line();
                                    }
                                }
                            }
                        } else if stmt_node.kind == syntax_kind_ext::CLASS_DECLARATION {
                            // Non-exported class in namespace: just emit it
                            let prev = self.in_namespace_iife;
                            self.in_namespace_iife = true;
                            self.emit(stmt_idx);
                            self.in_namespace_iife = prev;
                            self.write_line();
                        } else if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                            // Nested namespace: recurse (emit_namespace_iife adds its own newline)
                            self.emit(stmt_idx);
                        } else {
                            // Regular statement
                            self.emit(stmt_idx);
                            self.write_line();
                        }
                    }
                }
            }
        }
    }

    /// Emit exported variable as namespace property assignment.
    /// `export var x = 10;` → `ns.x = 10;`
    fn emit_namespace_exported_variable(&mut self, var_stmt_idx: NodeIndex, ns_name: &str) {
        let Some(var_node) = self.arena.get(var_stmt_idx) else {
            return;
        };
        let Some(var_stmt) = self.arena.get_variable(var_node) else {
            return;
        };

        // Iterate declaration lists → declarations
        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };

            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    continue;
                };

                let mut names = Vec::new();
                self.collect_binding_names(decl.name, &mut names);

                for name in &names {
                    self.write(ns_name);
                    self.write(".");
                    self.write(name);
                    if !decl.initializer.is_none() {
                        self.write(" = ");
                        self.emit_expression(decl.initializer);
                    }
                    self.write(";");
                    self.write_line();
                }
            }
        }
    }

    /// Get export names from a declaration clause (function, class, variable, enum)
    fn get_export_names_from_clause(&self, clause_idx: NodeIndex) -> Vec<String> {
        let Some(node) = self.arena.get(clause_idx) else {
            return Vec::new();
        };
        match node.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = self.arena.get_variable(node) {
                    return self.collect_variable_names(&var_stmt.declarations);
                }
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                if let Some(func) = self.arena.get_function(node) {
                    if let Some(name_node) = self.arena.get(func.name) {
                        if let Some(ident) = self.arena.get_identifier(name_node) {
                            return vec![ident.escaped_text.clone()];
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = self.arena.get_class(node) {
                    if let Some(name_node) = self.arena.get(class.name) {
                        if let Some(ident) = self.arena.get_identifier(name_node) {
                            return vec![ident.escaped_text.clone()];
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = self.arena.get_enum(node) {
                    if let Some(name_node) = self.arena.get(enum_decl.name) {
                        if let Some(ident) = self.arena.get_identifier(name_node) {
                            return vec![ident.escaped_text.clone()];
                        }
                    }
                }
            }
            _ => {}
        }
        Vec::new()
    }

    // =========================================================================
    // Class Members
    // =========================================================================

    /// Emit class member modifiers (static, public, private, etc.)
    pub(super) fn emit_class_member_modifiers(&mut self, modifiers: &Option<NodeList>) {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    // Emit the modifier keyword based on its kind
                    let keyword = match mod_node.kind as u32 {
                        k if k == SyntaxKind::StaticKeyword as u32 => "static",
                        k if k == SyntaxKind::PublicKeyword as u32 => "public",
                        k if k == SyntaxKind::PrivateKeyword as u32 => "private",
                        k if k == SyntaxKind::ProtectedKeyword as u32 => "protected",
                        k if k == SyntaxKind::ReadonlyKeyword as u32 => "readonly",
                        k if k == SyntaxKind::AbstractKeyword as u32 => "abstract",
                        k if k == SyntaxKind::OverrideKeyword as u32 => "override",
                        k if k == SyntaxKind::AsyncKeyword as u32 => "async",
                        k if k == SyntaxKind::DeclareKeyword as u32 => "declare",
                        _ => continue,
                    };
                    self.write(keyword);
                    self.write_space();
                }
            }
        }
    }

    pub(super) fn emit_method_declaration(&mut self, node: &Node) {
        let Some(method) = self.arena.get_method_decl(node) else {
            return;
        };

        // Skip method declarations without bodies (TypeScript-only overloads)
        if method.body.is_none() {
            return;
        }

        // Emit modifiers (static, async only for JavaScript)
        self.emit_method_modifiers_js(&method.modifiers);

        // Emit generator asterisk
        if method.asterisk_token {
            self.write("*");
        }

        // Parser recovery for `*() {}` can produce an identifier name token `"("`.
        // Treat that as an omitted name to match tsc emit.
        let has_recovery_missing_name = self.arena.get(method.name).is_some_and(|name_node| {
            self.arena
                .get_identifier(name_node)
                .is_some_and(|id| id.escaped_text == "(")
        });
        if !method.name.is_none() && !has_recovery_missing_name {
            self.emit(method.name);
        }
        self.write("(");
        self.emit_function_parameters_js(&method.parameters.nodes);
        self.write(")");

        // Skip return type for JavaScript emit

        self.write(" ");
        self.emit(method.body);
    }

    /// Emit method modifiers for JavaScript (static, async only)
    pub(super) fn emit_method_modifiers_js(&mut self, modifiers: &Option<NodeList>) {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    match mod_node.kind {
                        k if k == SyntaxKind::StaticKeyword as u16 => self.write("static "),
                        k if k == SyntaxKind::AsyncKeyword as u16 => self.write("async "),
                        _ => {} // Skip private/protected/public/readonly/abstract
                    }
                }
            }
        }
    }

    pub(super) fn emit_property_declaration(&mut self, node: &Node) {
        let Some(prop) = self.arena.get_property_decl(node) else {
            return;
        };

        // Skip abstract property declarations (they don't exist at runtime)
        if self.has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword as u16) {
            return;
        }

        // For JavaScript: Skip property declarations without initializers
        // (they are TypeScript-only declarations: typed props, bare props)
        // Exception: Private fields (#name) are always emitted — they are runtime declarations.
        let is_private = self
            .arena
            .get(prop.name)
            .is_some_and(|n| n.kind == SyntaxKind::PrivateIdentifier as u16);
        if prop.initializer.is_none() && !is_private {
            return;
        }

        // Emit modifiers (static only for JavaScript)
        self.emit_class_member_modifiers_js(&prop.modifiers);

        self.emit(prop.name);

        // Skip type annotations for JavaScript emit

        if !prop.initializer.is_none() {
            self.write(" = ");
            self.emit(prop.initializer);
        }

        self.write_semicolon();
    }

    /// Emit class member modifiers for JavaScript (only static is valid)
    pub(super) fn emit_class_member_modifiers_js(&mut self, modifiers: &Option<NodeList>) {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    // Only emit 'static' for JavaScript - skip private/readonly/public/protected
                    if mod_node.kind == SyntaxKind::StaticKeyword as u16 {
                        self.write("static ");
                    }
                }
            }
        }
    }

    pub(super) fn emit_constructor_declaration(&mut self, node: &Node) {
        let Some(ctor) = self.arena.get_constructor(node) else {
            return;
        };

        // Skip declaration-only constructors (no body).
        // These are overload signatures or ambient declarations, not emitted in JS.
        if ctor.body.is_none() {
            return;
        }

        // Collect parameter property names (public/private/protected/readonly params)
        let param_props = self.collect_parameter_properties(&ctor.parameters.nodes);
        let field_inits = std::mem::take(&mut self.pending_class_field_inits);

        self.write("constructor(");
        self.emit_function_parameters_js(&ctor.parameters.nodes);
        self.write(")");
        self.write(" ");

        if param_props.is_empty() && field_inits.is_empty() {
            self.emit(ctor.body);
        } else {
            self.emit_constructor_body_with_prologue(ctor.body, &param_props, &field_inits);
        }
    }

    /// Collect parameter property names from constructor parameters.
    /// Returns names of parameters that have accessibility modifiers (public/private/protected/readonly).
    fn collect_parameter_properties(&self, params: &[NodeIndex]) -> Vec<String> {
        let mut names = Vec::new();
        for &param_idx in params {
            if let Some(param_node) = self.arena.get(param_idx)
                && let Some(param) = self.arena.get_parameter(param_node)
            {
                if self.has_parameter_property_modifier(&param.modifiers) {
                    let name = self.get_identifier_text_idx(param.name);
                    if !name.is_empty() {
                        names.push(name);
                    }
                }
            }
        }
        names
    }

    /// Check if parameter modifiers include an accessibility or readonly modifier.
    fn has_parameter_property_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    let kind = mod_node.kind as u32;
                    if kind == SyntaxKind::PublicKeyword as u32
                        || kind == SyntaxKind::PrivateKeyword as u32
                        || kind == SyntaxKind::ProtectedKeyword as u32
                        || kind == SyntaxKind::ReadonlyKeyword as u32
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Emit constructor body block with parameter property and field initializer assignments.
    fn emit_constructor_body_with_prologue(
        &mut self,
        block_idx: NodeIndex,
        param_props: &[String],
        field_inits: &[(String, NodeIndex)],
    ) {
        let Some(block_node) = self.arena.get(block_idx) else {
            return;
        };
        let Some(block) = self.arena.get_block(block_node) else {
            return;
        };

        self.write("{");
        self.write_line();
        self.increase_indent();

        // Emit parameter property assignments: this.<name> = <name>;
        for name in param_props {
            self.write("this.");
            self.write(name);
            self.write(" = ");
            self.write(name);
            self.write(";");
            self.write_line();
        }

        // Emit class field initializer assignments: this.<name> = <init>;
        for (name, init_idx) in field_inits {
            self.write("this.");
            self.write(name);
            self.write(" = ");
            self.emit_expression(*init_idx);
            self.write(";");
            self.write_line();
        }

        // Emit original body statements
        for &stmt_idx in &block.statements.nodes {
            let before_len = self.writer.len();
            self.emit(stmt_idx);
            if self.writer.len() > before_len {
                self.write_line();
            }
        }

        self.decrease_indent();
        self.write("}");
    }

    pub(super) fn emit_get_accessor(&mut self, node: &Node) {
        let Some(accessor) = self.arena.get_accessor(node) else {
            return;
        };

        // Emit modifiers (static only for JavaScript)
        self.emit_class_member_modifiers_js(&accessor.modifiers);

        self.write("get ");
        self.emit(accessor.name);
        self.write("()");

        // Skip type annotation for JS emit

        if !accessor.body.is_none() {
            self.write(" ");
            self.emit(accessor.body);
        } else {
            // For JS emit, add empty body for accessors without body
            self.write(" { }");
        }
    }

    pub(super) fn emit_set_accessor(&mut self, node: &Node) {
        let Some(accessor) = self.arena.get_accessor(node) else {
            return;
        };

        // Emit modifiers (static only for JavaScript)
        self.emit_class_member_modifiers_js(&accessor.modifiers);

        self.write("set ");
        self.emit(accessor.name);
        self.write("(");
        self.emit_function_parameters_js(&accessor.parameters.nodes);
        self.write(")");

        if !accessor.body.is_none() {
            self.write(" ");
            self.emit(accessor.body);
        } else {
            // For JS emit, add empty body for accessors without body
            self.write(" { }");
        }
    }
}
