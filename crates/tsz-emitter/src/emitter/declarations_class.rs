use super::{Printer, ScriptTarget};
use crate::transforms::ClassES5Emitter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Entry for a static field initializer that will be emitted after the class body.
/// Fields: (name, initializer node, member pos, leading comments, trailing comments)
type StaticFieldInit = (String, NodeIndex, u32, Vec<String>, Vec<String>);

impl<'a> Printer<'a> {
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
        // Suppress trailing comments on class body opening brace.
        // tsc drops same-line comments on `{` for class bodies, just like function
        // bodies (e.g. `class C { // error` → `class C {`).
        if !self.ctx.options.remove_comments
            && let Some(text) = self.source_text
        {
            let bytes = text.as_bytes();
            let start = node.pos as usize;
            let end = (node.end as usize).min(bytes.len());
            if let Some(offset) = bytes[start..end].iter().position(|&b| b == b'{') {
                let brace_end = (start + offset + 1) as u32;
                // Only suppress if there's a newline between `{` and the first
                // member (or `}` if empty).  Single-line class bodies like
                // `class C { x: T; } // error` have the comment after `}`.
                let scan_end = class
                    .members
                    .nodes
                    .first()
                    .and_then(|&idx| self.arena.get(idx))
                    .map_or(end, |m| m.pos as usize);
                let has_newline = bytes[brace_end as usize..scan_end.min(end)]
                    .iter()
                    .any(|&b| b == b'\n' || b == b'\r');
                if has_newline {
                    self.skip_trailing_same_line_comments(brace_end, node.end);
                }
            }
        }
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
        // Collect computed property name expressions from erased type-only members.
        // tsc emits these as standalone side-effect statements after the class body
        // (e.g., `[Symbol.iterator]: Type` → erased member, but `Symbol.iterator;` emitted).
        let mut computed_property_side_effects: Vec<NodeIndex> = Vec::new();

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
                // Private fields (#name) are emitted verbatim at ES2022+ — they
                // use native private field syntax and are unaffected by
                // useDefineForClassFields.  Only skip them for lowering when the
                // target actually requires WeakMap-based lowering (< ES2022).
                && !(self.arena.get(prop.name).is_some_and(|n| {
                    n.kind == SyntaxKind::PrivateIdentifier as u16
                }) && (self.ctx.options.target as u32) >= (ScriptTarget::ES2022 as u32))
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
                                let comment_text =
                                    crate::safe_slice::slice(text, c.pos as usize, c.end as usize);
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
                    // Bodyless methods are erased (abstract methods without body,
                    // overload signatures). Abstract methods WITH a body (an error
                    // in TS) are still emitted by tsc, so we must not erase them.
                    k if k == syntax_kind_ext::METHOD_DECLARATION => self
                        .arena
                        .get_method_decl(member_node)
                        .is_some_and(|m| m.body.is_none()),
                    // Abstract accessors without body are erased. Bodyless non-abstract
                    // accessors (error case) are kept — tsc emits them with `{ }`.
                    // Abstract accessors WITH a body (error case) are also kept.
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        self.arena.get_accessor(member_node).is_some_and(|a| {
                            self.arena
                                .has_modifier(&a.modifiers, SyntaxKind::AbstractKeyword)
                                && a.body.is_none()
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
                        .get_constructor(member_node)
                        .is_some_and(|c| c.body.is_none()),
                    // Index signatures are TypeScript-only
                    k if k == syntax_kind_ext::INDEX_SIGNATURE => true,
                    // Semicolon class elements are preserved in JS output (valid JS syntax)
                    k if k == syntax_kind_ext::SEMICOLON_CLASS_ELEMENT => false,
                    _ => false,
                };
                if is_erased {
                    // When an erased property has a computed name whose expression
                    // could have runtime side effects, tsc emits the expression as
                    // a standalone statement after the class body.
                    // e.g., `[Symbol.iterator]: Type` → `Symbol.iterator;`
                    // Only expressions that might have observable effects are emitted:
                    // property accesses, element accesses, calls, assignments, etc.
                    // Simple identifiers and literals are NOT emitted (no side effects).
                    if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                        && let Some(p) = self.arena.get_property_decl(member_node)
                        && let Some(name_node) = self.arena.get(p.name)
                        && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                        && let Some(computed) = self.arena.get_computed_property(name_node)
                        && let Some(expr_node) = self.arena.get(computed.expression)
                    {
                        let k = expr_node.kind;
                        let is_side_effect_free = k == SyntaxKind::Identifier as u16
                            || k == SyntaxKind::StringLiteral as u16
                            || k == SyntaxKind::NumericLiteral as u16
                            || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                            || k == SyntaxKind::PrivateIdentifier as u16;
                        if !is_side_effect_free {
                            computed_property_side_effects.push(computed.expression);
                        }
                    }
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

        // Emit computed property name side-effect statements for erased members.
        // e.g., `[Symbol.iterator]: Type` → `Symbol.iterator;`
        for expr_idx in &computed_property_side_effects {
            self.write_line();
            self.emit_expression(*expr_idx);
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
}

#[cfg(test)]
mod tests {
    use crate::emitter::ScriptTarget;
    use crate::output::printer::{PrintOptions, Printer};
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

    /// Regression test: private fields (#name) with initializers must be
    /// emitted verbatim at ES2022+ targets even when useDefineForClassFields
    /// is false.  Private fields use native syntax at ES2022+ and are
    /// unaffected by the useDefineForClassFields flag (which only controls
    /// public field semantics).  Previously, the lowering skip logic dropped
    /// them because `identifier_text()` returned empty for `PrivateIdentifier`
    /// nodes, causing them to be neither collected for lowering NOR emitted
    /// in the class body.
    #[test]
    fn private_field_with_initializer_emitted_at_es2022() {
        let source = "class A {\n    static #field = 10;\n    static #uninitialized;\n    #instance = 1;\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        // PrintOptions defaults to use_define_for_class_fields: false via
        // PrinterOptions::default(), which triggers the class field lowering
        // path.  At ES2022+ the lowering should still preserve private fields
        // verbatim.
        let opts = PrintOptions {
            target: ScriptTarget::ES2022,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("static #field = 10;"),
            "Static private field with initializer should be emitted at ES2022.\nOutput:\n{output}"
        );
        assert!(
            output.contains("static #uninitialized;"),
            "Static private field without initializer should be emitted.\nOutput:\n{output}"
        );
        assert!(
            output.contains("#instance = 1;"),
            "Instance private field with initializer should be emitted at ES2022.\nOutput:\n{output}"
        );
    }

    /// Verify that private fields at targets below ES2022 are still handled
    /// by the lowering path (not emitted verbatim with initializers).
    #[test]
    fn private_field_lowered_at_es2015() {
        let source = "class A {\n    static #field = 10;\n    #instance = 1;\n}\n";

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

        // At ES2015, private fields should NOT appear in the class body
        // (they should be lowered to WeakMap-based patterns, though the
        // lowering transform itself may not fully emit them yet).
        assert!(
            !output.contains("static #field = 10;"),
            "Static private field should NOT be emitted verbatim at ES2015.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("#instance = 1;"),
            "Instance private field should NOT be emitted verbatim at ES2015.\nOutput:\n{output}"
        );
    }

    /// Regression test: bodyless method overload signatures are erased,
    /// so their leading comments (JSDoc blocks) must not appear in the output.
    /// Previously, `get_function()` was used instead of `get_method_decl()`,
    /// so `is_erased` was always false for methods.
    #[test]
    fn overload_method_comments_erased() {
        let source = r#"class C {
    /** overload 1 */
    foo(x: number): number;
    /** overload 2 */
    foo(x: string): string;
    /** implementation */
    foo(x: any): any {
        return x;
    }
}"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let opts = PrintOptions {
            target: ScriptTarget::ESNext,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        // Overload JSDoc comments should NOT appear in the output
        assert!(
            !output.contains("overload 1"),
            "JSDoc for overload signature 1 should be erased.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("overload 2"),
            "JSDoc for overload signature 2 should be erased.\nOutput:\n{output}"
        );
        // Implementation JSDoc SHOULD appear
        assert!(
            output.contains("/** implementation */"),
            "JSDoc for implementation should be preserved.\nOutput:\n{output}"
        );
    }

    /// Regression test: bodyless constructor overload signatures are erased,
    /// so their leading comments must not appear in the output.
    /// Previously, `get_function()` was used instead of `get_constructor()`,
    /// so `is_erased` was always false for constructors.
    #[test]
    fn overload_constructor_comments_erased() {
        let source = r#"class C {
    /** ctor overload 1 */
    constructor(x: number);
    /** ctor overload 2 */
    constructor(x: string);
    /** ctor implementation */
    constructor(x: any) {}
}"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let opts = PrintOptions {
            target: ScriptTarget::ESNext,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        // Overload JSDoc comments should NOT appear
        assert!(
            !output.contains("ctor overload 1"),
            "JSDoc for ctor overload 1 should be erased.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("ctor overload 2"),
            "JSDoc for ctor overload 2 should be erased.\nOutput:\n{output}"
        );
        // Implementation JSDoc SHOULD appear
        assert!(
            output.contains("/** ctor implementation */"),
            "JSDoc for ctor implementation should be preserved.\nOutput:\n{output}"
        );
    }

    /// Regression test: when a class member is erased (e.g., a type-only property
    /// at ES2015+ with useDefineForClassFields=false), trailing comments on the same
    /// line as the class closing `}` must NOT be consumed by the erased member's
    /// comment skip logic. For example:
    ///   `class C extends E { foo: string; } // error`
    /// The `// error` comment belongs to the `}`, not to the erased `foo: string;`.
    #[test]
    fn erased_member_does_not_consume_trailing_comment_after_closing_brace() {
        // Single-line class with an erased property and a trailing comment
        let source = "class C extends E { foo: string; } // error\n";

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
            output.contains("// error"),
            "Trailing comment after closing brace should be preserved.\nOutput:\n{output}"
        );
    }

    /// Regression test: an erased member's OWN trailing comment (on the same
    /// line, with only whitespace between the `;` and the comment) should still
    /// be consumed. This ensures the fix for closing-brace comments doesn't
    /// regress the basic erased-comment-suppression behavior.
    #[test]
    fn erased_interface_trailing_comment_is_suppressed() {
        let source = "interface Foo {} // type-only\nconst x = 1;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let opts = PrintOptions {
            target: ScriptTarget::ESNext,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            !output.contains("// type-only"),
            "Trailing comment on erased interface should be suppressed.\nOutput:\n{output}"
        );
    }

    /// Abstract methods WITH a body (an error in TS, but tsc still emits them)
    /// must NOT be erased — only bodyless methods should be erased.
    #[test]
    fn abstract_method_with_body_is_emitted() {
        let source = "abstract class H {\n    abstract baz(): number { return 1; }\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let opts = PrintOptions {
            target: ScriptTarget::ESNext,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("baz()"),
            "Abstract method with body should be emitted (tsc parity).\nOutput:\n{output}"
        );
    }

    /// Abstract methods WITHOUT a body should be erased (standard behavior).
    #[test]
    fn abstract_method_without_body_is_erased() {
        let source = "abstract class G {\n    abstract qux(): number;\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let opts = PrintOptions {
            target: ScriptTarget::ESNext,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            !output.contains("qux"),
            "Abstract method without body should be erased.\nOutput:\n{output}"
        );
    }

    /// Bodyless non-abstract accessors (error case in TS) must NOT be erased —
    /// tsc emits them with an empty body `{ }`.
    #[test]
    fn bodyless_non_abstract_accessor_is_not_erased() {
        let source = "class C {\n    get foo(): string;\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let opts = PrintOptions {
            target: ScriptTarget::ESNext,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("foo"),
            "Bodyless non-abstract accessor should be emitted (tsc parity).\nOutput:\n{output}"
        );
    }

    /// Erased computed property names with potential side effects (property access)
    /// must be emitted as standalone expression statements after the class body.
    /// e.g., `[Symbol.iterator]: Type` → class body erased, then `Symbol.iterator;`
    #[test]
    fn computed_property_side_effect_property_access() {
        let source = "class C {\n    [Symbol.iterator]: any;\n}\n";

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
            output.contains("}\nSymbol.iterator;"),
            "Computed property access expression should be emitted as side-effect statement.\nOutput:\n{output}"
        );
    }

    /// Simple identifier computed property names should NOT produce side-effect
    /// statements — tsc does not emit them (no observable side effects).
    #[test]
    fn computed_property_no_side_effect_for_identifier() {
        let source = "class C {\n    [x]: string;\n}\n";

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
            !output.contains("x;"),
            "Simple identifier computed property should NOT produce side-effect statement.\nOutput:\n{output}"
        );
    }

    /// String literal computed property names should NOT produce side-effect
    /// statements — string literals have no observable side effects.
    #[test]
    fn computed_property_no_side_effect_for_string_literal() {
        let source = "class C {\n    [\"a\"]: string;\n}\n";

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
            !output.contains("\"a\";"),
            "String literal computed property should NOT produce side-effect statement.\nOutput:\n{output}"
        );
    }

    /// Trailing comment on class body opening `{` should be suppressed.
    /// tsc: `class E extends A {` (comment dropped)
    #[test]
    fn class_body_brace_trailing_comment_suppressed() {
        let source =
            "class E extends A { // error -- doesn't implement bar\n    foo() { return 1; }\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            !output.contains("// error"),
            "Trailing comment on class body `{{` should be suppressed.\nOutput:\n{output}"
        );
    }

    /// Comment inside class body (not on opening brace) should still be preserved.
    #[test]
    fn class_body_inner_comment_preserved() {
        let source = "class C {\n    // this is a method\n    foo() { return 1; }\n}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("// this is a method"),
            "Leading comment of class member should be preserved.\nOutput:\n{output}"
        );
    }
}
