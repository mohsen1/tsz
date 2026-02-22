use super::declarations_namespace::rewrite_enum_iife_for_namespace_export;
use super::{Printer, ScriptTarget};
use crate::transforms::ClassES5Emitter;
use crate::transforms::enum_es5::EnumES5Transformer;
use crate::transforms::ir_printer::IRPrinter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Entry for a static field initializer that will be emitted after the class body.
/// Fields: (name, initializer node, member pos, leading comments, trailing comments)
type StaticFieldInit = (String, NodeIndex, u32, Vec<String>, Vec<String>);

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
    // Classes
    // =========================================================================

    pub(super) fn collect_class_decorators(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> Vec<NodeIndex> {
        let Some(mods) = modifiers else {
            return Vec::new();
        };
        mods.nodes
            .iter()
            .copied()
            .filter(|&mod_idx| {
                self.arena
                    .get(mod_idx)
                    .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
            })
            .collect()
    }

    pub(super) fn emit_legacy_class_decorator_assignment(
        &mut self,
        class_name: &str,
        decorators: &[NodeIndex],
        commonjs_exported: bool,
        commonjs_default: bool,
        emit_commonjs_pre_assignment: bool,
    ) {
        if class_name.is_empty() || decorators.is_empty() {
            return;
        }

        if commonjs_exported && !commonjs_default && emit_commonjs_pre_assignment {
            self.write("exports.");
            self.write(class_name);
            self.write(" = ");
            self.write(class_name);
            self.write(";");
            self.write_line();
        }

        if commonjs_exported {
            if commonjs_default {
                self.write("exports.default = ");
            } else {
                self.write("exports.");
                self.write(class_name);
                self.write(" = ");
            }
        }

        self.write(class_name);
        self.write(" = __decorate([");
        self.write_line();
        self.increase_indent();
        for (i, &dec_idx) in decorators.iter().enumerate() {
            if let Some(dec_node) = self.arena.get(dec_idx)
                && let Some(dec) = self.arena.get_decorator(dec_node)
            {
                self.emit(dec.expression);
                if i + 1 != decorators.len() {
                    self.write(",");
                }
                self.write_line();
            }
        }
        self.decrease_indent();
        self.write("], ");
        self.write(class_name);
        self.write(");");
    }

    /// Emit a class declaration.
    pub(super) fn emit_class_declaration(&mut self, node: &Node, idx: NodeIndex) {
        let Some(class) = self.arena.get_class(node) else {
            return;
        };

        // Skip ambient declarations (declare class)
        if self
            .arena
            .has_modifier(&class.modifiers, SyntaxKind::DeclareKeyword)
        {
            self.skip_comments_for_erased_node(node);
            return;
        }

        let legacy_class_decorators = if self.ctx.options.legacy_decorators
            && node.kind == syntax_kind_ext::CLASS_DECLARATION
        {
            self.collect_class_decorators(&class.modifiers)
        } else {
            Vec::new()
        };

        if !legacy_class_decorators.is_empty() {
            let class_name = if class.name.is_none() {
                self.anonymous_default_export_name
                    .clone()
                    .unwrap_or_default()
            } else {
                self.get_identifier_text_idx(class.name)
            };

            if self.ctx.target_es5 {
                let mut es5_emitter = ClassES5Emitter::new(self.arena);
                es5_emitter.set_indent_level(self.writer.indent_level());
                es5_emitter.set_transforms(self.transforms.clone());
                if let Some(text) = self.source_text_for_map() {
                    if self.writer.has_source_map() {
                        es5_emitter
                            .set_source_map_context(text, self.writer.current_source_index());
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
                self.write_line();
                let commonjs_exported = self.ctx.is_commonjs()
                    && self
                        .arena
                        .has_modifier(&class.modifiers, SyntaxKind::ExportKeyword)
                    && !self.ctx.module_state.has_export_assignment;
                let commonjs_default = commonjs_exported
                    && self
                        .arena
                        .has_modifier(&class.modifiers, SyntaxKind::DefaultKeyword);
                self.emit_legacy_class_decorator_assignment(
                    &class_name,
                    &legacy_class_decorators,
                    commonjs_exported,
                    commonjs_default,
                    false,
                );
                while self.comment_emit_idx < self.all_comments.len()
                    && self.all_comments[self.comment_emit_idx].end <= node.end
                {
                    self.comment_emit_idx += 1;
                }
                return;
            }

            if class_name.is_empty() {
                self.emit_class_es6_with_options(node, idx, false, None);
                return;
            }

            self.emit_class_es6_with_options(node, idx, true, Some(("let", class_name.clone())));
            // Only write newline if not already at line start (class declarations
            // with lowered static fields already end with write_line()).
            if !self.writer.is_at_line_start() {
                self.write_line();
            }
            let commonjs_exported = self.ctx.is_commonjs()
                && self
                    .arena
                    .has_modifier(&class.modifiers, SyntaxKind::ExportKeyword)
                && !self.ctx.module_state.has_export_assignment;
            let commonjs_default = commonjs_exported
                && self
                    .arena
                    .has_modifier(&class.modifiers, SyntaxKind::DefaultKeyword);
            self.emit_legacy_class_decorator_assignment(
                &class_name,
                &legacy_class_decorators,
                commonjs_exported,
                commonjs_default,
                false,
            );
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
            // Skip comments that were part of this class declaration since the
            // ES5 class emitter handles class comments internally.
            self.skip_comments_for_erased_node(node);
            return;
        }

        self.emit_class_es6_with_options(node, idx, false, None);
    }

    /// Emit a class using ES6 native class syntax (no transforms).
    /// This is the pure emission logic that can be reused by both the old API
    /// and the new transform system.
    pub(super) fn emit_class_es6(&mut self, node: &Node, idx: NodeIndex) {
        self.emit_class_es6_with_options(node, idx, false, None);
    }

    pub(super) fn emit_class_es6_with_options(
        &mut self,
        node: &Node,
        _idx: NodeIndex,
        suppress_modifiers: bool,
        assignment_prefix: Option<(&str, String)>,
    ) {
        let Some(class) = self.arena.get_class(node) else {
            return;
        };
        let class_name = if class.name.is_none() {
            assignment_prefix
                .as_ref()
                .map(|(_, binding_name)| binding_name.clone())
                .unwrap_or_default()
        } else {
            self.get_identifier_text_idx(class.name)
        };

        // Emit modifiers (including decorators) - skip TS-only modifiers for JS output
        if !suppress_modifiers && let Some(ref modifiers) = class.modifiers {
            for &mod_idx in &modifiers.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    // Skip export/default modifiers in CommonJS mode or namespace IIFE
                    if (self.ctx.is_commonjs() || self.in_namespace_iife)
                        && (mod_node.kind == SyntaxKind::ExportKeyword as u16
                            || mod_node.kind == SyntaxKind::DefaultKeyword as u16)
                    {
                        continue;
                    }
                    // Skip TypeScript-only modifiers (abstract, declare, etc.)
                    // Also skip `async` — it's an error on class declarations but
                    // TSC still emits the class without the modifier.
                    if mod_node.kind == SyntaxKind::AbstractKeyword as u16
                        || mod_node.kind == SyntaxKind::DeclareKeyword as u16
                        || mod_node.kind == SyntaxKind::AsyncKeyword as u16
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

        if let Some((keyword, binding_name)) = assignment_prefix.as_ref() {
            self.write(keyword);
            self.write(" ");
            self.write(binding_name);
            self.write(" = ");
        }

        // Collect instance `accessor` fields to lower using WeakMap-backed
        // getter/setter pairs. Only needed when target < ES2022 (ES2022+ uses
        // native private fields / accessor syntax).
        let mut auto_accessor_members: Vec<(NodeIndex, String, Option<NodeIndex>)> = Vec::new();
        let mut auto_accessor_inits: Vec<(String, Option<NodeIndex>)> = Vec::new();
        if !class_name.is_empty() && self.ctx.needs_es2022_lowering {
            for &member_idx in &class.members.nodes {
                let Some(member_node) = self.arena.get(member_idx) else {
                    continue;
                };
                let Some(prop) = self.arena.get_property_decl(member_node).filter(|prop| {
                    self.arena
                        .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
                }) else {
                    continue;
                };
                if self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::StaticKeyword)
                    || self
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                {
                    continue;
                }
                if self
                    .arena
                    .get(prop.name)
                    .is_some_and(|n| n.kind == SyntaxKind::PrivateIdentifier as u16)
                {
                    continue;
                }
                let Some(name_node) = self.arena.get(prop.name) else {
                    continue;
                };
                if name_node.kind != SyntaxKind::Identifier as u16 {
                    continue;
                }
                let name = self.get_identifier_text_idx(prop.name);
                if name.is_empty() {
                    continue;
                }
                let storage_name = format!("_{class_name}_{name}_accessor_storage");
                auto_accessor_members.push((
                    member_idx,
                    storage_name.clone(),
                    Some(prop.initializer),
                ));
                auto_accessor_inits.push((storage_name, Some(prop.initializer)));
            }
        }

        if !auto_accessor_members.is_empty() {
            self.write("var ");
            for (i, (_, storage_name, _)) in auto_accessor_members.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.write(storage_name);
            }
            self.write(";");
            self.write_line();
            self.emit_comments_before_pos(node.pos);
        }

        self.write("class");

        let override_name = self.anonymous_default_export_name.clone();
        let class_name = if class.name.is_none() {
            override_name.unwrap_or_default()
        } else {
            self.get_identifier_text_idx(class.name)
        };
        if class.name.is_none() {
            if !class_name.is_empty() {
                self.write_space();
                self.write(&class_name);
            }
        } else {
            self.write_space();
            self.emit_decl_name(class.name);
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

        // Store auto-accessor inits for constructor emission.
        let prev_auto_accessor_inits = std::mem::take(&mut self.pending_auto_accessor_inits);
        if !auto_accessor_inits.is_empty() {
            self.pending_auto_accessor_inits = auto_accessor_inits.clone();
        }

        // Check if we need to lower class fields to constructor.
        // This is needed when target < ES2022 OR when useDefineForClassFields is false
        // (legacy behavior where fields are assigned in the constructor).
        let needs_class_field_lowering = (self.ctx.options.target as u32)
            < (ScriptTarget::ES2022 as u32)
            || !self.ctx.options.use_define_for_class_fields;

        // Check if we need to lower static blocks to IIFEs (for targets < ES2022)
        let needs_static_block_lowering =
            (self.ctx.options.target as u32) < (ScriptTarget::ES2022 as u32);
        let mut deferred_static_blocks: Vec<NodeIndex> = Vec::new();

        // Collect property initializers that need lowering
        let mut field_inits: Vec<(String, NodeIndex)> = Vec::new();
        let mut static_field_inits: Vec<StaticFieldInit> = Vec::new();
        if needs_class_field_lowering {
            for &member_idx in &class.members.nodes {
                if let Some(member_node) = self.arena.get(member_idx)
                    && member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                    && let Some(prop) = self.arena.get_property_decl(member_node)
                {
                    if prop.initializer.is_none()
                        || self
                            .arena
                            .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                    {
                        continue;
                    }
                    let name = self.get_identifier_text_idx(prop.name);
                    if name.is_empty() {
                        continue;
                    }
                    if self
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::StaticKeyword)
                    {
                        static_field_inits.push((
                            name,
                            prop.initializer,
                            member_node.pos,
                            Vec::new(), // leading_comments filled during class body emission
                            Vec::new(), // trailing_comments filled during class body emission
                        ));
                    } else {
                        field_inits.push((name, prop.initializer));
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
        let synthesize_constructor =
            !has_constructor && (!field_inits.is_empty() || !auto_accessor_inits.is_empty());

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
                if self.ctx.options.use_define_for_class_fields {
                    self.write("Object.defineProperty(this, ");
                    self.emit_string_literal_text(name);
                    self.write(", {");
                    self.write_line();
                    self.increase_indent();
                    self.write("enumerable: true,");
                    self.write_line();
                    self.write("configurable: true,");
                    self.write_line();
                    self.write("writable: true,");
                    self.write_line();
                    self.write("value: ");
                    self.emit_expression(*init_idx);
                    self.write_line();
                    self.decrease_indent();
                    self.write("});");
                } else {
                    self.write("this.");
                    self.write(name);
                    self.write(" = ");
                    self.emit_expression(*init_idx);
                    self.write(";");
                }
                self.write_line();
            }
            for (storage_name, init_idx) in &auto_accessor_inits {
                self.write(storage_name);
                self.write(".set(this, ");
                match init_idx {
                    Some(init) => self.emit_expression(*init),
                    None => self.write("void 0"),
                }
                self.write(");");
                self.write_line();
            }
            self.decrease_indent();
            self.write("}");
            self.write_line();
        }

        // When useDefineForClassFields is true, emit parameter property field
        // declarations (e.g. `foo;`) at the beginning of the class body.
        // TSC emits these before any other class members.
        let mut emitted_any_member = false;
        if self.ctx.options.use_define_for_class_fields {
            // Find the constructor and collect its parameter properties
            for &member_idx in &class.members.nodes {
                if let Some(member_node) = self.arena.get(member_idx)
                    && member_node.kind == syntax_kind_ext::CONSTRUCTOR
                    && let Some(ctor) = self.arena.get_constructor(member_node)
                    && ctor.body.is_some()
                {
                    let param_props = self.collect_parameter_properties(&ctor.parameters.nodes);
                    for name in &param_props {
                        self.write(name);
                        self.write(";");
                        self.write_line();
                        emitted_any_member = true;
                    }
                    break;
                }
            }
        }
        for (member_i, &member_idx) in class.members.nodes.iter().enumerate() {
            // Skip property declarations that were lowered
            if needs_class_field_lowering
                && let Some(member_node) = self.arena.get(member_idx)
                && member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                && let Some(prop) = self.arena.get_property_decl(member_node)
                && !auto_accessor_members
                    .iter()
                    .any(|(accessor_idx, _, _)| *accessor_idx == member_idx)
                && prop.initializer.is_some()
                && !self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
            {
                // For static properties, save leading and trailing comments before
                // skipping so they can be emitted when the initialization is moved
                // after the class body.
                let is_static = self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::StaticKeyword);
                if is_static {
                    let leading = self.collect_leading_comments(member_node.pos);
                    if let Some(entry) = static_field_inits
                        .iter_mut()
                        .find(|e| e.2 == member_node.pos)
                    {
                        entry.3 = leading;
                    }
                }
                if let Some(member_node) = self.arena.get(member_idx) {
                    // Use a tighter bound for property declarations to avoid
                    // consuming comments that belong to the next class member.
                    // Property node.end can extend past newlines into the next
                    // member's territory, so we bound by the next member's pos.
                    let skip_end = class
                        .members
                        .nodes
                        .get(member_i + 1)
                        .and_then(|&next_idx| self.arena.get(next_idx))
                        .map_or(member_node.end, |next| next.pos);
                    // Find the actual end of the property's content
                    let actual_end = self.find_token_end_before_trivia(member_node.pos, skip_end);
                    // Find line end from actual_end
                    let line_end = if let Some(text) = self.source_text {
                        let bytes = text.as_bytes();
                        let mut pos = actual_end as usize;
                        while pos < bytes.len() && bytes[pos] != b'\n' && bytes[pos] != b'\r' {
                            pos += 1;
                        }
                        pos as u32
                    } else {
                        actual_end
                    };
                    // For static fields, collect trailing comments on the same line
                    // (e.g. `static x = 1; // ok`) before advancing past them.
                    if is_static && let Some(text) = self.source_text {
                        let mut trailing = Vec::new();
                        let mut idx = self.comment_emit_idx;
                        while idx < self.all_comments.len() {
                            let c = &self.all_comments[idx];
                            if c.pos >= actual_end && c.end <= line_end {
                                let comment_text = crate::printer::safe_slice::slice(
                                    text,
                                    c.pos as usize,
                                    c.end as usize,
                                );
                                trailing.push(comment_text.to_string());
                            }
                            if c.end > line_end {
                                break;
                            }
                            idx += 1;
                        }
                        if let Some(entry) = static_field_inits
                            .iter_mut()
                            .find(|e| e.2 == member_node.pos)
                        {
                            entry.4 = trailing;
                        }
                    }
                    while self.comment_emit_idx < self.all_comments.len() {
                        let c = &self.all_comments[self.comment_emit_idx];
                        if c.end <= line_end {
                            self.comment_emit_idx += 1;
                        } else {
                            break;
                        }
                    }
                }
                continue;
            }

            // Skip static blocks that need lowering to IIFEs after the class
            if needs_static_block_lowering
                && let Some(member_node) = self.arena.get(member_idx)
                && member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
            {
                deferred_static_blocks.push(member_idx);
                self.skip_comments_for_erased_node(member_node);
                continue;
            }

            // Check if this member is erased (no runtime representation)
            if let Some(member_node) = self.arena.get(member_idx) {
                let is_erased = match member_node.kind {
                    // Abstract methods and bodyless overloads are erased
                    k if k == syntax_kind_ext::METHOD_DECLARATION => {
                        self.arena.get_function(member_node).is_some_and(|f| {
                            self.arena
                                .has_modifier(&f.modifiers, SyntaxKind::AbstractKeyword)
                                || f.body.is_none()
                        })
                    }
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        self.arena.get_accessor(member_node).is_some_and(|a| {
                            self.arena
                                .has_modifier(&a.modifiers, SyntaxKind::AbstractKeyword)
                        })
                    }
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                        if let Some(p) = self.arena.get_property_decl(member_node) {
                            // Abstract properties: erased
                            if self
                                .arena
                                .has_modifier(&p.modifiers, SyntaxKind::AbstractKeyword)
                            {
                                true
                            } else {
                                // Type-only properties (no initializer, not private, not accessor): erased
                                // But when useDefineForClassFields is true (ES2022+),
                                // uninitialised properties are real class field declarations.
                                if self.ctx.options.use_define_for_class_fields {
                                    false
                                } else {
                                    let is_private = self.arena.get(p.name).is_some_and(|n| {
                                        n.kind == SyntaxKind::PrivateIdentifier as u16
                                    });
                                    let has_accessor = self
                                        .arena
                                        .has_modifier(&p.modifiers, SyntaxKind::AccessorKeyword);
                                    p.initializer.is_none() && !is_private && !has_accessor
                                }
                            }
                        } else {
                            false
                        }
                    }
                    // Bodyless constructor overloads are erased
                    k if k == syntax_kind_ext::CONSTRUCTOR => self
                        .arena
                        .get_function(member_node)
                        .is_some_and(|f| f.body.is_none()),
                    // Index signatures are TypeScript-only
                    k if k == syntax_kind_ext::INDEX_SIGNATURE => true,
                    // Semicolon class elements are preserved in JS output (valid JS syntax)
                    k if k == syntax_kind_ext::SEMICOLON_CLASS_ELEMENT => false,
                    _ => false,
                };
                if is_erased {
                    self.skip_comments_for_erased_node(member_node);
                    continue;
                }
            }

            // Emit leading comments before this member
            if let Some(member_node) = self.arena.get(member_idx) {
                self.emit_comments_before_pos(member_node.pos);
            }

            let before_len = self.writer.len();
            let auto_accessor = auto_accessor_members
                .iter()
                .find(|(idx, _, _)| *idx == member_idx)
                .map(|(_, storage_name, _)| storage_name.clone());
            if let Some(member_node) = self.arena.get(member_idx) {
                let property_end = if auto_accessor.is_some() {
                    let upper = class
                        .members
                        .nodes
                        .get(member_i + 1)
                        .and_then(|&next_idx| self.arena.get(next_idx))
                        .map(|n| n.pos)
                        .unwrap_or(member_node.end);
                    Some(self.find_token_end_before_trivia(member_node.pos, upper))
                } else {
                    None
                };

                if let Some(storage_name) = auto_accessor {
                    self.emit_auto_accessor_methods(
                        member_node,
                        &storage_name,
                        property_end.unwrap_or(member_node.end),
                    );
                } else {
                    self.emit(member_idx);
                }
            }
            let mut emit_standalone_class_semicolon = false;
            if let Some(member_node) = self.arena.get(member_idx)
                && (member_node.kind == syntax_kind_ext::GET_ACCESSOR
                    || member_node.kind == syntax_kind_ext::SET_ACCESSOR
                    || member_node.kind == syntax_kind_ext::METHOD_DECLARATION)
            {
                let next_is_semicolon_member = class
                    .members
                    .nodes
                    .get(member_i + 1)
                    .and_then(|&idx| self.arena.get(idx))
                    .is_some_and(|n| n.kind == syntax_kind_ext::SEMICOLON_CLASS_ELEMENT);

                // Check if the member has a body (method/accessor with `{}`).
                let member_has_body_for_semi = match member_node.kind {
                    k if k == syntax_kind_ext::METHOD_DECLARATION => self
                        .arena
                        .get_method_decl(member_node)
                        .is_some_and(|m| m.body.is_some()),
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        self.arena
                            .get_accessor(member_node)
                            .is_some_and(|a| a.body.is_some())
                    }
                    _ => false,
                };
                if !next_is_semicolon_member {
                    let has_source_semicolon = self.source_text.is_some_and(|text| {
                        let member_end = std::cmp::min(member_node.end as usize, text.len());
                        // For members WITHOUT bodies, check the gap after the member.
                        if !member_has_body_for_semi {
                            let gap_end = class
                                .members
                                .nodes
                                .get(member_i + 1)
                                .and_then(|&idx| self.arena.get(idx))
                                .map_or_else(
                                    || {
                                        let search_end =
                                            std::cmp::min(node.end as usize, text.len());
                                        text[member_end..search_end]
                                            .rfind('}')
                                            .map_or(search_end, |pos| member_end + pos)
                                    },
                                    |n| n.pos as usize,
                                );
                            let gap_end = std::cmp::min(gap_end, text.len());
                            if member_end < gap_end && text[member_end..gap_end].contains(';') {
                                return true;
                            }
                        }
                        // For members WITH bodies, the parser may absorb trailing `;`
                        // into the member span (e.g., `get x() { ... };`).
                        // Check if the member source ends with `} ;` pattern.
                        if member_has_body_for_semi && member_end >= 2 {
                            let tail = &text[member_node.pos as usize..member_end];
                            let trimmed = tail.trim_end();
                            if let Some(before_semi) = trimmed.strip_suffix(';')
                                && before_semi.trim_end().ends_with('}')
                            {
                                return true;
                            }
                        }
                        false
                    });
                    emit_standalone_class_semicolon = has_source_semicolon;
                }

                // Some parser recoveries include the semicolon in member.end without
                // creating a separate SEMICOLON_CLASS_ELEMENT; preserve it from source.
                // Only check this for methods/accessors that DON'T have a body (i.e.,
                // abstract methods or overload signatures like `foo(): void;`).
                if !member_has_body_for_semi
                    && self.source_text.is_some_and(|text| {
                        let start = std::cmp::min(member_node.pos as usize, text.len());
                        let end = std::cmp::min(member_node.end as usize, text.len());
                        if start >= end {
                            return false;
                        }
                        let member_text = text[start..end].trim_end();
                        member_text.ends_with(';')
                    })
                {
                    emit_standalone_class_semicolon = true;
                }
            }
            if self.writer.len() == before_len
                && let (Some(member_node), Some(text)) =
                    (self.arena.get(member_idx), self.source_text)
            {
                let start = std::cmp::min(member_node.pos as usize, text.len());
                let end = std::cmp::min(member_node.end as usize, text.len());
                if start < end {
                    let raw = &text[start..end];
                    let compact: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
                    if compact.starts_with("*(){") {
                        self.write("*() { }");
                    }
                }
            }
            // Only add newline if something was actually emitted
            if self.writer.len() > before_len && !self.writer.is_at_line_start() {
                emitted_any_member = true;
                // Emit trailing comments on the same line as the member.
                // For property declarations, member_node.end can include the leading trivia
                // of the next member (because the parser records token_end() = scanner.pos
                // which is after the lookahead token). Use the AST initializer/name end
                // to get the true end of the property's last token.
                if let Some(member_node) = self.arena.get(member_idx) {
                    // Use the next member's pos as upper bound to avoid scanning
                    // past the current member into the next member's trivia.
                    let next_member_pos = class
                        .members
                        .nodes
                        .get(member_i + 1)
                        .and_then(|&next_idx| self.arena.get(next_idx))
                        .map(|n| n.pos);
                    let upper = next_member_pos.unwrap_or(member_node.end);
                    let token_end = self.find_token_end_before_trivia(member_node.pos, upper);
                    self.emit_trailing_comments(token_end);
                }
                self.write_line();
                if emit_standalone_class_semicolon {
                    self.write(";");
                    self.write_line();
                }
            }
        }

        if !emitted_any_member && let Some(text) = self.source_text {
            let start = std::cmp::min(node.pos as usize, text.len());
            let end = std::cmp::min(node.end as usize, text.len());
            if start < end {
                let raw = &text[start..end];
                let compact: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
                if compact.contains("*(){}") {
                    self.write("*() { }");
                    self.write_line();
                }
            }
        }

        // Skip orphaned comments inside the class body.
        // When class members are erased (type-only properties, abstract members, etc.),
        // comments on lines between erased members or between the last erased member
        // and the closing `}` are left unconsumed. Without this, they leak into the
        // output as spurious comments after the class.
        // Find the closing `}` position and skip any remaining comments before it.
        {
            let class_body_end = self.find_token_end_before_trivia(node.pos, node.end);
            while self.comment_emit_idx < self.all_comments.len() {
                let c = &self.all_comments[self.comment_emit_idx];
                if c.end <= class_body_end {
                    self.comment_emit_idx += 1;
                } else {
                    break;
                }
            }
        }

        // Restore field inits
        self.pending_class_field_inits = prev_field_inits;
        self.pending_auto_accessor_inits = prev_auto_accessor_inits;

        self.decrease_indent();
        self.write("}");
        if assignment_prefix.is_some() {
            self.write(";");
        }

        if let Some(class_name) = self.pending_commonjs_class_export_name.take() {
            self.write_line();
            self.write("exports.");
            self.write(&class_name);
            self.write(" = ");
            self.write(&class_name);
            self.write(";");
        }

        if let Some(recovery_name) = self.class_var_function_recovery_name(node) {
            self.write_line();
            self.write("var ");
            self.write(&recovery_name);
            self.write(";");
            self.write_line();
            self.write("() => { };");
        }

        // Emit static field initializers after class body: ClassName.field = value;
        if !static_field_inits.is_empty() {
            let class_name = self.get_identifier_text_idx(class.name);
            if !class_name.is_empty() {
                self.write_line();
                for (name, init_idx, _member_pos, leading_comments, trailing_comments) in
                    &static_field_inits
                {
                    // Emit saved leading comments from the original static property declaration
                    for comment_text in leading_comments {
                        self.write_comment(comment_text);
                        self.write_line();
                    }
                    if self.ctx.options.use_define_for_class_fields {
                        self.write("Object.defineProperty(");
                        self.write(&class_name);
                        self.write(", ");
                        self.emit_string_literal_text(name);
                        self.write(", {");
                        self.write_line();
                        self.increase_indent();
                        self.write("enumerable: true,");
                        self.write_line();
                        self.write("configurable: true,");
                        self.write_line();
                        self.write("writable: true,");
                        self.write_line();
                        self.write("value: ");
                        self.emit_expression(*init_idx);
                        self.write_line();
                        self.decrease_indent();
                        self.write("});");
                    } else {
                        self.write(&class_name);
                        self.write(".");
                        self.write(name);
                        self.write(" = ");
                        self.emit_expression(*init_idx);
                        self.write(";");
                    }
                    // Emit saved trailing comments (e.g. `// ok` from
                    // `static intance = new C3(); // ok`)
                    for comment_text in trailing_comments {
                        self.write_space();
                        self.write(comment_text);
                    }
                    self.write_line();
                }
            }
        }

        // Emit auto-accessor WeakMap initializations after class body:
        // var _Class_prop_accessor_storage;
        // ...
        // _Class_prop_accessor_storage = new WeakMap();
        if !auto_accessor_inits.is_empty() {
            for (storage_name, _init_idx) in &auto_accessor_inits {
                self.write_line();
                self.write(storage_name);
                self.write(" = new WeakMap();");
            }
        }

        // Emit deferred static blocks as IIFEs after the class body
        for static_block_idx in deferred_static_blocks {
            self.write_line();
            self.write("(() => ");
            if let Some(static_node) = self.arena.get(static_block_idx) {
                // Static block uses the same data as a Block node
                self.emit_block(static_node, static_block_idx);
            } else {
                self.write("{ }");
            }
            self.write(")();");
        }

        // Track class name to prevent duplicate var declarations for merged namespaces.
        // When a class and namespace have the same name (declaration merging), the class
        // provides the declaration, so the namespace shouldn't emit `var name;`.
        if class.name.is_some() {
            let class_name = self.get_identifier_text_idx(class.name);
            if !class_name.is_empty() {
                self.declared_namespace_names.insert(class_name);
            }
        }
    }

    pub(super) fn class_has_auto_accessor_members(
        &self,
        class: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let Some(prop_data) = self.arena.get_property_decl(member_node) else {
                continue;
            };
            if self
                .arena
                .has_modifier(&prop_data.modifiers, SyntaxKind::AccessorKeyword)
                && !self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::StaticKeyword)
                && !self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::AbstractKeyword)
            {
                let Some(name_node) = self.arena.get(prop_data.name) else {
                    continue;
                };
                if name_node.kind == SyntaxKind::Identifier as u16 {
                    return true;
                }
            }
        }
        false
    }

    fn emit_auto_accessor_methods(&mut self, node: &Node, storage_name: &str, property_end: u32) {
        let Some(prop) = self.arena.get_property_decl(node) else {
            return;
        };

        self.write("get ");
        self.emit(prop.name);
        self.write("() { return __classPrivateFieldGet(this, ");
        self.write(storage_name);
        self.write(", \"f\"); }");
        self.emit_trailing_comments(property_end);
        self.write_line();
        self.write("set ");
        self.emit(prop.name);
        self.write("(value) { __classPrivateFieldSet(this, ");
        self.write(storage_name);
        self.write(", value, \"f\"); }");
    }

    /// Parser recovery parity for malformed class members like:
    /// `var constructor() { }`
    /// which TypeScript preserves as:
    /// `var constructor;`
    /// `() => { };`
    fn class_var_function_recovery_name(&self, class_node: &Node) -> Option<String> {
        let text = self.source_text?;
        let start = std::cmp::min(class_node.pos as usize, text.len());
        let end = std::cmp::min(class_node.end as usize, text.len());
        if start >= end {
            return None;
        }

        let slice = &text[start..end];
        let mut i = 0usize;
        let bytes = slice.as_bytes();

        while i < bytes.len() {
            if bytes[i].is_ascii_whitespace() {
                i += 1;
                continue;
            }
            if i + 3 > bytes.len() || &bytes[i..i + 3] != b"var" {
                i += 1;
                continue;
            }
            i += 3;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let ident_start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$')
            {
                i += 1;
            }
            if ident_start == i {
                continue;
            }
            let ident = String::from_utf8_lossy(&bytes[ident_start..i]).to_string();
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] != b'(' {
                continue;
            }
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] != b')' {
                continue;
            }
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] != b'{' {
                continue;
            }
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] != b'}' {
                continue;
            }

            return Some(ident);
        }

        None
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

#[cfg(test)]
mod tests {
    use crate::emitter::ScriptTarget;
    use crate::printer::{PrintOptions, Printer};
    use tsz_parser::ParserState;

    /// Regression test: trailing comments on static class fields must be
    /// preserved when the field is lowered to `ClassName.field = value;`
    /// for targets < ES2022.
    #[test]
    fn static_field_lowering_preserves_trailing_comment() {
        let source = "class C3 {\n    static intance = new C3(); // ok\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let opts = PrintOptions {
            target: ScriptTarget::ES2017,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        // The lowered static field should preserve the trailing comment
        assert!(
            output.contains("C3.intance = new C3(); // ok"),
            "Trailing comment '// ok' should be preserved on lowered static field.\nOutput:\n{output}"
        );
    }

    /// Test: multiple static fields with trailing comments are all preserved.
    #[test]
    fn static_field_lowering_preserves_multiple_trailing_comments() {
        let source = "class Foo {\n    static a = 1; // first\n    static b = 2; // second\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let opts = PrintOptions {
            target: ScriptTarget::ES2017,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("Foo.a = 1; // first"),
            "Trailing comment '// first' should be preserved.\nOutput:\n{output}"
        );
        assert!(
            output.contains("Foo.b = 2; // second"),
            "Trailing comment '// second' should be preserved.\nOutput:\n{output}"
        );
    }

    /// Test: static fields without trailing comments still emit correctly.
    #[test]
    fn static_field_lowering_without_trailing_comment() {
        let source = "class Bar {\n    static x = 42;\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let opts = PrintOptions {
            target: ScriptTarget::ES2017,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("Bar.x = 42;"),
            "Static field should be lowered correctly.\nOutput:\n{output}"
        );
        // Should NOT have any trailing comment text
        assert!(
            !output.contains("Bar.x = 42; //"),
            "Should not have spurious trailing comment.\nOutput:\n{output}"
        );
    }

    #[test]
    fn auto_accessor_instance_fields_emit_getter_setter_with_weakmap() {
        let source =
            "class RegularClass {\n    accessor shouldError: string; // Should still error\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let opts = PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("var _RegularClass_shouldError_accessor_storage;"),
            "Auto-accessor storage declaration should be emitted.\nOutput:\n{output}"
        );
        assert!(
            output.contains("constructor() {",),
            "Constructor should be synthesized for auto-accessor initialization.\nOutput:\n{output}"
        );
        assert!(
            output.contains("_RegularClass_shouldError_accessor_storage.set(this, void 0);"),
            "Auto-accessor storage should initialize to void 0 in constructor.\nOutput:\n{output}"
        );
        assert!(
            output.contains("_RegularClass_shouldError_accessor_storage = new WeakMap();"),
            "Auto-accessor storage should be initialized with WeakMap after class body.\nOutput:\n{output}"
        );
        assert!(
            output.contains(
                "get shouldError() { return __classPrivateFieldGet(this, _RegularClass_shouldError_accessor_storage, \"f\"); } // Should still error",
            ),
            "Auto accessor getter should be lowered.\nOutput:\n{output}"
        );
        assert!(
            output.contains(
                "set shouldError(value) { __classPrivateFieldSet(this, _RegularClass_shouldError_accessor_storage, value, \"f\"); }",
            ),
            "Auto accessor setter should be lowered.\nOutput:\n{output}"
        );
        assert!(
            output.contains("__classPrivateFieldGet"),
            "Private field helpers should be emitted.\nOutput:\n{output}"
        );
    }

    /// Regression test: class with lowered static fields followed by another
    /// statement must not produce an extra blank line. The static field
    /// emission ends with `write_line()` after `ClassName.field = value;`,
    /// so the source-file-level loop must not add a second newline.
    #[test]
    fn no_extra_blank_line_after_static_field_lowering() {
        let source = "class Foo {\n    static x = 1;\n}\nconst y = 2;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let opts = PrintOptions {
            target: ScriptTarget::ES2017,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        // Should have `Foo.x = 1;\n` immediately followed by `const y = 2;`
        // with NO blank line in between.
        assert!(
            output.contains("Foo.x = 1;\nconst y = 2;"),
            "Should not have blank line between lowered static field and next statement.\nOutput:\n{output}"
        );
    }

    /// Regression test: class with lowered static field inside a block
    /// (e.g., for-loop body) must not produce an extra blank line before
    /// the next statement in the block.
    #[test]
    fn no_extra_blank_line_after_static_field_in_block() {
        let source = "for (const x of [1]) {\n    class Row {\n        static factory = 1;\n    }\n    use(Row);\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let opts = PrintOptions {
            target: ScriptTarget::ES2017,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        // Should have `Row.factory = 1;\n    use(Row);` with no blank line.
        assert!(
            !output.contains("Row.factory = 1;\n\n"),
            "Should not have blank line after lowered static field in block.\nOutput:\n{output}"
        );
    }

    /// Regression test: `export default class` with static field in CJS mode
    /// must not produce a blank line between the lowered static field init
    /// and the `exports.default = ClassName;` assignment.
    #[test]
    fn no_extra_blank_line_cjs_default_export_with_static_field() {
        use crate::emitter::ModuleKind;

        let source = "export default class MyComponent {\n    static create = 1;\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let opts = PrintOptions {
            target: ScriptTarget::ES2017,
            module: ModuleKind::CommonJS,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        // Should have `MyComponent.create = 1;\n` followed by
        // `exports.default = MyComponent;` with NO blank line.
        assert!(
            !output.contains("MyComponent.create = 1;\n\n"),
            "Should not have blank line between lowered static field and CJS export.\nOutput:\n{output}"
        );
        assert!(
            output.contains("exports.default = MyComponent;"),
            "Should emit CJS default export assignment.\nOutput:\n{output}"
        );
    }
}
