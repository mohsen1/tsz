use super::{Printer, ScriptTarget};
use crate::transforms::ClassES5Emitter;
use crate::transforms::enum_es5::EnumES5Transformer;
use crate::transforms::ir::IRNode;
use crate::transforms::ir_printer::IRPrinter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Rewrite enum IIFE IR from `E || (E = {})` to `E = NS.E || (NS.E = {})`
/// for exported enums in namespaces.
fn rewrite_enum_iife_for_namespace_export(ir: &mut IRNode, enum_name: &str, ns_name: &str) {
    // The IR from EnumES5Transformer is:
    //   Sequence([VarDecl { name }, ExpressionStatement(CallExpr { callee, arguments: [iife_arg] })])
    // where iife_arg is: LogicalOr { left: Identifier(E), right: BinaryExpr(E = {}) }
    //
    // We need to transform it to:
    //   iife_arg = BinaryExpr(E = LogicalOr { left: NS.E, right: BinaryExpr(NS.E = {}) })
    let IRNode::Sequence(stmts) = ir else {
        return;
    };

    // Find the ExpressionStatement containing the CallExpr
    let Some(expr_stmt) = stmts.iter_mut().find_map(|s| match s {
        IRNode::ExpressionStatement(inner) => Some(inner),
        _ => None,
    }) else {
        return;
    };

    let IRNode::CallExpr { arguments, .. } = expr_stmt.as_mut() else {
        return;
    };

    if arguments.len() != 1 {
        return;
    }

    // Build the namespace-qualified property access: NS.E
    let ns_prop = || IRNode::PropertyAccess {
        object: Box::new(IRNode::Identifier(ns_name.to_string())),
        property: enum_name.to_string(),
    };

    // Replace the IIFE argument: E || (E = {}) → E = NS.E || (NS.E = {})
    arguments[0] = IRNode::BinaryExpr {
        left: Box::new(IRNode::Identifier(enum_name.to_string())),
        operator: "=".to_string(),
        right: Box::new(IRNode::LogicalOr {
            left: Box::new(ns_prop()),
            right: Box::new(IRNode::BinaryExpr {
                left: Box::new(ns_prop()),
                operator: "=".to_string(),
                right: Box::new(IRNode::empty_object()),
            }),
        }),
    };
}

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
            } else if !func.name.is_none() {
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
            let search_end = if !func.body.is_none() {
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
        if self.has_declare_modifier(&class.modifiers) {
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
                    && self.has_export_modifier(&class.modifiers)
                    && !self.ctx.module_state.has_export_assignment;
                let commonjs_default =
                    commonjs_exported && self.has_default_modifier(&class.modifiers);
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
            self.write_line();
            let commonjs_exported = self.ctx.is_commonjs()
                && self.has_export_modifier(&class.modifiers)
                && !self.ctx.module_state.has_export_assignment;
            let commonjs_default = commonjs_exported && self.has_default_modifier(&class.modifiers);
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
            let class_end = node.end;
            while self.comment_emit_idx < self.all_comments.len()
                && self.all_comments[self.comment_emit_idx].end <= class_end
            {
                self.comment_emit_idx += 1;
            }
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

        self.write("class");

        let override_name = self.anonymous_default_export_name.clone();
        if class.name.is_none() {
            if let Some(name) = override_name {
                self.write_space();
                self.write(&name);
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
        let mut static_field_inits: Vec<(String, NodeIndex, u32, Vec<String>)> = Vec::new(); // (name, init, member_pos, leading_comments)
        if needs_class_field_lowering {
            for &member_idx in &class.members.nodes {
                if let Some(member_node) = self.arena.get(member_idx)
                    && member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                    && let Some(prop) = self.arena.get_property_decl(member_node)
                {
                    if prop.initializer.is_none()
                        || self.has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword as u16)
                    {
                        continue;
                    }
                    let name = self.get_identifier_text_idx(prop.name);
                    if name.is_empty() {
                        continue;
                    }
                    if self.has_modifier(&prop.modifiers, SyntaxKind::StaticKeyword as u16) {
                        static_field_inits.push((
                            name,
                            prop.initializer,
                            member_node.pos,
                            Vec::new(), // leading_comments filled during class body emission
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
                    && !ctor.body.is_none()
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
                && !prop.initializer.is_none()
                && !self.has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword as u16)
            {
                // For static properties, save leading comments before skipping so they
                // can be emitted when the initialization is moved after the class body.
                if self.has_modifier(&prop.modifiers, SyntaxKind::StaticKeyword as u16) {
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
                            self.has_modifier(&f.modifiers, SyntaxKind::AbstractKeyword as u16)
                                || f.body.is_none()
                        })
                    }
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        self.arena.get_accessor(member_node).is_some_and(|a| {
                            self.has_modifier(&a.modifiers, SyntaxKind::AbstractKeyword as u16)
                        })
                    }
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                        if let Some(p) = self.arena.get_property_decl(member_node) {
                            // Abstract properties: erased
                            if self.has_modifier(&p.modifiers, SyntaxKind::AbstractKeyword as u16) {
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
                                    let has_accessor = self.has_modifier(
                                        &p.modifiers,
                                        SyntaxKind::AccessorKeyword as u16,
                                    );
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
                    // Semicolon class elements produce no output
                    k if k == syntax_kind_ext::SEMICOLON_CLASS_ELEMENT => true,
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
            self.emit(member_idx);
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
                        .is_some_and(|m| !m.body.is_none()),
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        self.arena
                            .get_accessor(member_node)
                            .is_some_and(|a| !a.body.is_none())
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

        // Restore field inits
        self.pending_class_field_inits = prev_field_inits;

        self.decrease_indent();
        self.write("}");
        if assignment_prefix.is_some() {
            self.write(";");
        }

        // Emit static field initializers after class body: ClassName.field = value;
        if !static_field_inits.is_empty() {
            let class_name = self.get_identifier_text_idx(class.name);
            if !class_name.is_empty() {
                self.write_line();
                for (name, init_idx, _member_pos, leading_comments) in &static_field_inits {
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
                    self.write_line();
                }
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
                let enum_name = if !enum_decl.name.is_none() {
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
                } else if self.in_namespace_iife && !enum_name.is_empty() {
                    // Inside namespace IIFE, use `let` instead of `var` for enum declarations
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
            self.skip_comments_for_erased_node(node);
            return;
        }

        // Skip non-instantiated modules (type-only: interfaces, type aliases, empty)
        if !self.is_instantiated_module(module.body) {
            self.skip_comments_for_erased_node(node);
            return;
        }

        // ES5 target: Transform namespace to IIFE pattern
        if self.ctx.target_es5 {
            use crate::transforms::NamespaceES5Emitter;
            let mut es5_emitter = NamespaceES5Emitter::new(self.arena);
            let ns_name = self.get_identifier_text_idx(module.name);
            if !ns_name.is_empty() {
                self.declared_namespace_names.insert(ns_name);
            }

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
        // Only pass parent_name when the inner namespace is exported.
        // Non-exported namespaces get a standalone IIFE without parent assignment.
        // The export status is tracked via `namespace_export_inner` flag, set by
        // `emit_namespace_body_statements` when processing EXPORT_DECLARATION wrappers.
        let parent_name = if self.namespace_export_inner {
            self.namespace_export_inner = false;
            self.current_namespace_name.clone()
        } else {
            None
        };
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

        // Determine if we should emit a variable declaration for this namespace.
        // Skip if name already declared by class/function/enum (both at top level and
        // inside namespace IIFEs - e.g., merged class+namespace doesn't need extra let).
        let should_declare = !self.declared_namespace_names.contains(&name);
        if should_declare {
            let keyword = if self.in_namespace_iife { "let" } else { "var" };
            self.write(keyword);
            self.write(" ");
            self.write(&name);
            self.write(";");
            self.write_line();
            self.declared_namespace_names.insert(name.clone());
        }

        // Check if the IIFE parameter name conflicts with any declaration
        // inside the namespace body. TSC renames the parameter (e.g., A → A_1)
        // when there's a class/function/enum/namespace with the same name inside.
        let iife_param = if self.namespace_body_has_name_conflict(module, &name) {
            format!("{name}_1")
        } else {
            name.clone()
        };

        // Emit: (function (<iife_param>) {
        self.write("(function (");
        self.write(&iife_param);
        self.write(") {");
        self.write_line();
        self.increase_indent();

        // Check if body is another MODULE_DECLARATION (nested: namespace Foo.Bar)
        if let Some(body_node) = self.arena.get(module.body) {
            if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                // Nested namespace (e.g., namespace X.Y.Z expands to nested IIFEs).
                // Save/restore declared_namespace_names so names declared in nested
                // IIFEs don't leak to sibling IIFEs at the same level.
                if let Some(inner_module) = self.arena.get_module(body_node) {
                    let inner_module = inner_module.clone();
                    let prev_declared = self.declared_namespace_names.clone();
                    self.emit_namespace_iife(&inner_module, Some(&name));
                    self.declared_namespace_names = prev_declared;
                }
            } else {
                // MODULE_BLOCK: emit body statements
                let prev = self.in_namespace_iife;
                let prev_ns_name = self.current_namespace_name.clone();
                // Save and restore declared_namespace_names for this IIFE scope.
                // Each IIFE creates a new function scope, so `let` declarations
                // inside don't conflict with those in other IIFEs for the same name.
                let prev_declared = self.declared_namespace_names.clone();
                self.in_namespace_iife = true;
                self.current_namespace_name = Some(iife_param.clone());
                self.emit_namespace_body_statements(module, &iife_param);
                self.in_namespace_iife = prev;
                self.current_namespace_name = prev_ns_name;
                self.declared_namespace_names = prev_declared;
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

    /// Check if any declaration in the namespace body has the same name as the namespace.
    /// TSC renames the IIFE parameter when this happens (e.g., `A` → `A_1`).
    fn namespace_body_has_name_conflict(
        &self,
        module: &tsz_parser::parser::node::ModuleData,
        ns_name: &str,
    ) -> bool {
        let Some(body_node) = self.arena.get(module.body) else {
            return false;
        };
        if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            // Nested namespace (A.B) — check the inner module name
            if let Some(inner) = self.arena.get_module(body_node) {
                let inner_name = self.get_identifier_text_idx(inner.name);
                return inner_name == ns_name;
            }
            return false;
        }
        let Some(block) = self.arena.get_module_block(body_node) else {
            return false;
        };
        let Some(ref stmts) = block.statements else {
            return false;
        };
        for &stmt_idx in &stmts.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            // Check through export declarations
            let check_idx = if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                self.arena
                    .get_export_decl(stmt_node)
                    .map_or(stmt_idx, |e| e.export_clause)
            } else {
                stmt_idx
            };
            let Some(check_node) = self.arena.get(check_idx) else {
                continue;
            };
            let decl_name = match check_node.kind {
                k if k == syntax_kind_ext::CLASS_DECLARATION => {
                    self.arena.get_class(check_node).and_then(|c| {
                        if self.has_declare_modifier(&c.modifiers) {
                            None
                        } else {
                            Some(self.get_identifier_text_idx(c.name))
                        }
                    })
                }
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                    self.arena.get_function(check_node).and_then(|f| {
                        if self.has_declare_modifier(&f.modifiers) {
                            None
                        } else {
                            Some(self.get_identifier_text_idx(f.name))
                        }
                    })
                }
                k if k == syntax_kind_ext::ENUM_DECLARATION => {
                    self.arena.get_enum(check_node).and_then(|e| {
                        if self.has_declare_modifier(&e.modifiers) {
                            None
                        } else {
                            Some(self.get_identifier_text_idx(e.name))
                        }
                    })
                }
                k if k == syntax_kind_ext::MODULE_DECLARATION => {
                    self.arena.get_module(check_node).and_then(|m| {
                        // Skip ambient (declare) and non-instantiated modules
                        if self.has_declare_modifier(&m.modifiers)
                            || !self.is_instantiated_module(m.body)
                        {
                            None
                        } else {
                            Some(self.get_identifier_text_idx(m.name))
                        }
                    })
                }
                _ => None,
            };
            if decl_name.as_deref() == Some(ns_name) {
                return true;
            }
        }
        false
    }

    /// Collect exported *variable* names from a namespace body for identifier qualification.
    ///
    /// Only `export var` names need qualification because their local declaration is replaced
    /// by a namespace property assignment (`ns.x = expr;`).
    /// Exported classes/functions/enums keep their local declaration, so their names
    /// remain in scope without qualification.
    fn collect_namespace_exported_names(
        &self,
        module: &tsz_parser::parser::node::ModuleData,
    ) -> rustc_hash::FxHashSet<String> {
        let mut names = rustc_hash::FxHashSet::default();
        let Some(body_node) = self.arena.get(module.body) else {
            return names;
        };
        let Some(block) = self.arena.get_module_block(body_node) else {
            return names;
        };
        let Some(ref stmts) = block.statements else {
            return names;
        };
        for &stmt_idx in &stmts.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export) = self.arena.get_export_decl(stmt_node) else {
                continue;
            };
            let inner_kind = self.arena.get(export.export_clause).map_or(0, |n| n.kind);
            // Only collect variable names - classes/functions/enums keep their local bindings
            if inner_kind == syntax_kind_ext::VARIABLE_STATEMENT {
                let export_names = self.get_export_names_from_clause(export.export_clause);
                for name in export_names {
                    names.insert(name);
                }
            }
        }
        names
    }

    /// Emit body statements of a namespace IIFE, handling exports.
    fn emit_namespace_body_statements(
        &mut self,
        module: &tsz_parser::parser::node::ModuleData,
        ns_name: &str,
    ) {
        let ns_name = ns_name.to_string();
        if let Some(body_node) = self.arena.get(module.body)
            && let Some(block) = self.arena.get_module_block(body_node)
            && let Some(ref stmts) = block.statements
        {
            // Collect exported names for identifier qualification in emit_identifier
            let prev_exported = std::mem::take(&mut self.namespace_exported_names);
            self.namespace_exported_names = self.collect_namespace_exported_names(module);
            for (stmt_i, &stmt_idx) in stmts.nodes.iter().enumerate() {
                let Some(stmt_node) = self.arena.get(stmt_idx) else {
                    continue;
                };

                // Skip erased declarations (type-only, ambient, etc.) and their comments
                if self.is_erased_statement(stmt_node) {
                    self.skip_comments_for_erased_node(stmt_node);
                    continue;
                }

                // Also handle export wrapping an erased declaration
                if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                    && let Some(export) = self.arena.get_export_decl(stmt_node)
                    && let Some(inner_node) = self.arena.get(export.export_clause)
                    && self.is_erased_statement(inner_node)
                {
                    self.skip_comments_for_erased_node(stmt_node);
                    continue;
                }

                // Compute upper bound for trailing comment scan: use the next statement's
                // position to avoid scanning past the current statement into the next line.
                let next_pos = stmts
                    .nodes
                    .get(stmt_i + 1)
                    .and_then(|&next_idx| self.arena.get(next_idx))
                    .map(|n| n.pos);
                let upper_bound = next_pos.unwrap_or(stmt_node.end);

                // Emit leading comments before this statement
                self.emit_comments_before_pos(stmt_node.pos);

                if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                    // Strip "export" and handle inner clause
                    if let Some(export) = self.arena.get_export_decl(stmt_node) {
                        let inner_idx = export.export_clause;
                        let inner_kind = self.arena.get(inner_idx).map_or(0, |n| n.kind);

                        if inner_kind == syntax_kind_ext::VARIABLE_STATEMENT {
                            // export var x = 10; → ns.x = 10;
                            self.emit_namespace_exported_variable(inner_idx, &ns_name, stmt_node);
                        } else if inner_kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                            // export import X = Y; → ns.X = Y;
                            self.emit_namespace_exported_import_alias(inner_idx, &ns_name);
                        } else {
                            // class/function/enum: emit without export, then add assignment
                            let export_names = self.get_export_names_from_clause(inner_idx);

                            // For exported enums in namespace, fold the export into the
                            // IIFE closing pattern instead of emitting a separate assignment.
                            let is_enum = inner_kind == syntax_kind_ext::ENUM_DECLARATION;
                            if is_enum {
                                self.enum_namespace_export = Some(ns_name.clone());
                            }

                            // For exported namespaces, signal that the IIFE should
                            // use parent assignment (e.g., `m3.m4 || (m3.m4 = {})`).
                            let is_ns = inner_kind == syntax_kind_ext::MODULE_DECLARATION;
                            if is_ns {
                                self.namespace_export_inner = true;
                            }

                            let before_len = self.writer.len();
                            self.emit(inner_idx);
                            let emitted = self.writer.len() > before_len;
                            // Emit trailing comments on the same line
                            if emitted && let Some(inner_node) = self.arena.get(inner_idx) {
                                let inner_upper = next_pos.unwrap_or(inner_node.end);
                                let token_end =
                                    self.find_token_end_before_trivia(inner_node.pos, inner_upper);
                                self.emit_trailing_comments(token_end);
                            }

                            // If the enum absorbed the namespace export into its IIFE,
                            // skip the separate assignment statement.
                            let skip_export = is_enum && self.enum_namespace_export.is_none();

                            if !export_names.is_empty() && !skip_export {
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
                            } else if emitted && inner_kind != syntax_kind_ext::MODULE_DECLARATION {
                                // Don't write extra newline for namespaces - they already call write_line()
                                // Also don't write newline if emit produced nothing (e.g., non-instantiated import alias)
                                self.write_line();
                            }
                            // Clean up in case the enum emitter didn't consume it
                            self.enum_namespace_export = None;
                        }
                    }
                } else if stmt_node.kind == syntax_kind_ext::CLASS_DECLARATION {
                    // Non-exported class in namespace: just emit it
                    let prev = self.in_namespace_iife;
                    self.in_namespace_iife = true;
                    self.emit(stmt_idx);
                    self.in_namespace_iife = prev;
                    let token_end = self.find_token_end_before_trivia(stmt_node.pos, upper_bound);
                    self.emit_trailing_comments(token_end);
                    self.write_line();
                } else if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                    // Nested namespace: recurse (emit_namespace_iife adds its own newline)
                    self.emit(stmt_idx);
                } else {
                    // Regular statement - emit trailing comments on same line
                    self.emit(stmt_idx);
                    let token_end = self.find_token_end_before_trivia(stmt_node.pos, upper_bound);
                    self.emit_trailing_comments(token_end);
                    self.write_line();
                }
            }
            // Restore previous exported names
            self.namespace_exported_names = prev_exported;
        }
    }

    /// Emit exported import alias as namespace property assignment.
    /// `export import X = Y;` → `ns.X = Y;`
    fn emit_namespace_exported_import_alias(&mut self, import_idx: NodeIndex, ns_name: &str) {
        let Some(import_node) = self.arena.get(import_idx) else {
            return;
        };
        let Some(import) = self.arena.get_import_decl(import_node) else {
            return;
        };

        // Get the alias name
        let alias_name = self.get_identifier_text_idx(import.import_clause);
        if alias_name.is_empty() {
            return;
        }

        // Check if the referenced value has runtime semantics
        if !self.import_decl_has_runtime_value(import) {
            return;
        }

        // Emit: ns.X = Y;
        self.write(ns_name);
        self.write(".");
        self.write(&alias_name);
        self.write(" = ");
        self.emit_entity_name(import.module_specifier);
        self.write(";");
        self.write_line();
    }

    /// Emit exported variable as namespace property assignment.
    /// `export var x = 10;` → `ns.x = 10;`
    fn emit_namespace_exported_variable(
        &mut self,
        var_stmt_idx: NodeIndex,
        ns_name: &str,
        outer_stmt: &Node,
    ) {
        let Some(var_node) = self.arena.get(var_stmt_idx) else {
            return;
        };
        let Some(var_stmt) = self.arena.get_variable(var_node) else {
            return;
        };

        // Collect all initialized (name, initializer) pairs across declaration lists.
        // TSC emits multiple exports as a comma expression: `ns.a = 1, ns.c = 2;`
        let mut assignments: Vec<(String, NodeIndex)> = Vec::new();

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

                if decl.initializer.is_none() {
                    continue;
                }

                let mut names = Vec::new();
                self.collect_binding_names(decl.name, &mut names);
                for name in names {
                    assignments.push((name, decl.initializer));
                }
            }
        }

        // Emit as comma expression: ns.a = 1, ns.c = 2;
        if !assignments.is_empty() {
            for (i, (name, init)) in assignments.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.write(ns_name);
                self.write(".");
                self.write(name);
                self.write(" = ");
                self.emit_expression(*init);
            }
            self.write(";");
            let token_end = self.find_token_end_before_trivia(outer_stmt.pos, outer_stmt.end);
            self.emit_trailing_comments(token_end);
            self.write_line();
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
                if let Some(func) = self.arena.get_function(node)
                    && let Some(name_node) = self.arena.get(func.name)
                    && let Some(ident) = self.arena.get_identifier(name_node)
                {
                    return vec![ident.escaped_text.clone()];
                }
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = self.arena.get_class(node)
                    && let Some(name_node) = self.arena.get(class.name)
                    && let Some(ident) = self.arena.get_identifier(name_node)
                {
                    return vec![ident.escaped_text.clone()];
                }
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = self.arena.get_enum(node)
                    && let Some(name_node) = self.arena.get(enum_decl.name)
                    && let Some(ident) = self.arena.get_identifier(name_node)
                {
                    return vec![ident.escaped_text.clone()];
                }
            }
            _ => {}
        }
        Vec::new()
    }
}
