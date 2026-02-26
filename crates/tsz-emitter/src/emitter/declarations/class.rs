use super::super::{Printer, ScriptTarget};
use crate::transforms::ClassES5Emitter;
use crate::transforms::private_fields_es5::get_private_field_name;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{Node, NodeAccess};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Entry for a static field initializer that will be emitted after the class body.
/// Fields: (name, initializer node, member pos, leading comments with source pos, trailing comments)
type StaticFieldInit = (String, NodeIndex, u32, Vec<(String, u32)>, Vec<String>);
type AutoAccessorInfo = (NodeIndex, String, Option<NodeIndex>, bool);

impl<'a> Printer<'a> {
    // =========================================================================
    // Classes
    // =========================================================================

    pub(in crate::emitter) fn collect_class_decorators(
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

    pub(in crate::emitter) fn emit_legacy_class_decorator_assignment(
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

    /// Collect decorated class members and emit `__decorate` calls for them.
    ///
    /// For legacy (experimental) decorators, tsc emits `__decorate` calls after the
    /// class body for each decorated member:
    /// - Methods/accessors: `__decorate([...], ClassName.prototype, "name", null);`
    /// - Properties: `__decorate([...], ClassName.prototype, "name", void 0);`
    /// - Static members: `__decorate([...], ClassName, "name", ...);`
    pub(in crate::emitter) fn emit_legacy_member_decorator_calls(
        &mut self,
        class_name: &str,
        members: &[NodeIndex],
    ) {
        if class_name.is_empty() {
            return;
        }

        for &member_idx in members {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            let (modifiers, name_idx, is_property) = match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    (&method.modifiers, method.name, false)
                }
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    (&prop.modifiers, prop.name, true)
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(accessor) = self.arena.get_accessor(member_node) else {
                        continue;
                    };
                    (&accessor.modifiers, accessor.name, false)
                }
                _ => continue,
            };

            // Collect decorator nodes from modifiers
            let decorators = self.collect_class_decorators(modifiers);
            if decorators.is_empty() {
                continue;
            }

            let is_static = self
                .arena
                .has_modifier(modifiers, SyntaxKind::StaticKeyword);

            let member_name = self.get_identifier_text_idx(name_idx);
            if member_name.is_empty() {
                // Computed property names are not supported for legacy decorator lowering
                continue;
            }

            self.write("__decorate([");
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
            if !is_static {
                self.write(".prototype");
            }
            self.write(", ");
            self.emit_string_literal_text(&member_name);
            if is_property {
                self.write(", void 0);");
            } else {
                self.write(", null);");
            }
            self.write_line();
        }
    }

    /// Emit a class declaration.
    pub(in crate::emitter) fn emit_class_declaration(&mut self, node: &Node, idx: NodeIndex) {
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

        // Check if any members have legacy decorators (method, property, accessor decorators)
        let has_legacy_member_decorators = self.ctx.options.legacy_decorators
            && class.members.nodes.iter().any(|&m_idx| {
                let Some(m_node) = self.arena.get(m_idx) else {
                    return false;
                };
                let mods = match m_node.kind {
                    k if k == syntax_kind_ext::METHOD_DECLARATION => self
                        .arena
                        .get_method_decl(m_node)
                        .and_then(|m| m.modifiers.as_ref()),
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                        .arena
                        .get_property_decl(m_node)
                        .and_then(|p| p.modifiers.as_ref()),
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        self.arena
                            .get_accessor(m_node)
                            .and_then(|a| a.modifiers.as_ref())
                    }
                    _ => None,
                };
                mods.is_some_and(|m| {
                    m.nodes.iter().any(|&mod_idx| {
                        self.arena
                            .get(mod_idx)
                            .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                    })
                })
            });

        if !legacy_class_decorators.is_empty() || has_legacy_member_decorators {
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

            // When there are class-level decorators, emit as `let Name = class { ... };`
            // When only member decorators, emit as normal `class Name { ... }`
            if !legacy_class_decorators.is_empty() {
                self.emit_class_es6_with_options(
                    node,
                    idx,
                    true,
                    Some(("let", class_name.clone())),
                );
            } else {
                self.emit_class_es6_with_options(node, idx, false, None);
            }
            // Only write newline if not already at line start (class declarations
            // with lowered static fields already end with write_line()).
            if !self.writer.is_at_line_start() {
                self.write_line();
            }

            // Emit __decorate calls for member decorators (methods, properties, accessors)
            if has_legacy_member_decorators {
                self.emit_legacy_member_decorator_calls(&class_name, &class.members.nodes);
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
    pub(in crate::emitter) fn emit_class_es6(&mut self, node: &Node, idx: NodeIndex) {
        self.emit_class_es6_with_options(node, idx, false, None);
    }

    pub(in crate::emitter) fn emit_class_es6_with_options(
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

        // Collect `accessor` fields to lower using one of two strategies:
        // - ES2022+ (except ESNext): emit native private storage + getter/setter.
        // - < ES2022: emit WeakMap-backed getter/setter pairs.
        let auto_accessor_target = self.ctx.options.target;
        let lower_auto_accessors_to_private_fields = auto_accessor_target != ScriptTarget::ESNext
            && (auto_accessor_target as u32) >= (ScriptTarget::ES2022 as u32);
        let lower_auto_accessors_to_weakmap = auto_accessor_target != ScriptTarget::ESNext
            && (auto_accessor_target as u32) < (ScriptTarget::ES2022 as u32);

        let mut auto_accessor_members: Vec<AutoAccessorInfo> = Vec::new();
        let mut auto_accessor_instance_inits: Vec<(String, Option<NodeIndex>)> = Vec::new();
        let mut auto_accessor_static_inits: Vec<(String, Option<NodeIndex>)> = Vec::new();
        let mut auto_accessor_class_alias: Option<String> = None;
        let mut next_auto_accessor_name_index = 0u32;
        let mut private_names_for_auto_accessors: Vec<String> = Vec::new();
        if lower_auto_accessors_to_private_fields {
            let mut nodes_to_visit: Vec<NodeIndex> = class.members.nodes.clone();
            while let Some(member_idx) = nodes_to_visit.pop() {
                let Some(member_node) = self.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || member_node.kind == syntax_kind_ext::CLASS_EXPRESSION
                {
                    continue;
                }
                if member_node.kind == SyntaxKind::PrivateIdentifier as u16
                    && let Some(name) = get_private_field_name(self.arena, member_idx)
                {
                    private_names_for_auto_accessors.push(name.trim_start_matches('#').to_string());
                }
                let mut children = self.arena.get_children(member_idx);
                nodes_to_visit.append(&mut children);
            }
        }

        let mut next_auto_accessor_name = || -> String {
            let name = if next_auto_accessor_name_index < 26 {
                let offset = next_auto_accessor_name_index as u8;
                format!("_{}", (b'a' + offset) as char)
            } else {
                format!("_{}", next_auto_accessor_name_index - 26)
            };
            next_auto_accessor_name_index += 1;
            name
        };

        let mut uniquify_private_accessor_name = |base: &str| -> String {
            if !lower_auto_accessors_to_private_fields {
                return base.to_string();
            }

            let mut candidate = base.to_string();
            let mut candidate_with_storage = format!("{candidate}_accessor_storage");
            let mut suffix = 1usize;
            while private_names_for_auto_accessors
                .iter()
                .any(|name| name == &candidate_with_storage)
            {
                candidate = format!("{base}_{suffix}");
                candidate_with_storage = format!("{candidate}_accessor_storage");
                suffix += 1;
            }
            private_names_for_auto_accessors.push(format!("{candidate}_accessor_storage"));
            candidate
        };

        if lower_auto_accessors_to_private_fields || lower_auto_accessors_to_weakmap {
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
                    .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                {
                    continue;
                }
                if lower_auto_accessors_to_weakmap
                    && self
                        .arena
                        .get(prop.name)
                        .is_some_and(|name| name.kind == SyntaxKind::PrivateIdentifier as u16)
                {
                    continue;
                }
                if lower_auto_accessors_to_weakmap && class_name.is_empty() {
                    continue;
                }
                let is_static = self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::StaticKeyword);
                let Some(name_node) = self.arena.get(prop.name) else {
                    continue;
                };
                let mut accessor_name = match name_node.kind {
                    k if k == SyntaxKind::Identifier as u16 => {
                        self.get_identifier_text_idx(prop.name)
                    }
                    k if k == SyntaxKind::PrivateIdentifier as u16 => {
                        if lower_auto_accessors_to_private_fields {
                            get_private_field_name(self.arena, prop.name)
                                .unwrap_or_default()
                                .trim_start_matches('#')
                                .to_string()
                        } else {
                            String::new()
                        }
                    }
                    _ => String::new(),
                };
                if accessor_name.is_empty() {
                    accessor_name = next_auto_accessor_name();
                }
                if accessor_name.is_empty() {
                    continue;
                }
                let accessor_name = uniquify_private_accessor_name(&accessor_name);
                let storage_name = if lower_auto_accessors_to_weakmap {
                    format!("_{class_name}_{accessor_name}_accessor_storage")
                } else {
                    format!("{accessor_name}_accessor_storage")
                };
                let init = if prop.initializer.is_none() {
                    None
                } else {
                    Some(prop.initializer)
                };
                auto_accessor_members.push((member_idx, storage_name.clone(), init, is_static));
                if is_static {
                    if auto_accessor_class_alias.is_none() {
                        auto_accessor_class_alias = Some(self.make_unique_name());
                    }
                    auto_accessor_static_inits.push((storage_name, init));
                } else {
                    auto_accessor_instance_inits.push((storage_name, init));
                }
            }
        }

        if !auto_accessor_members.is_empty() && lower_auto_accessors_to_weakmap {
            let mut has_written = false;
            self.write("var ");
            if let Some(alias) = auto_accessor_class_alias.as_ref() {
                self.write(alias);
                has_written = true;
            }
            for (_, storage_name, _, _) in &auto_accessor_members {
                if has_written {
                    self.write(", ");
                }
                has_written = true;
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
                // member (or the closing `}` if empty).  Single-line class bodies
                // like `class C { x: T; } // error` have the comment after `}`,
                // so we must NOT suppress it.
                // For empty classes like `class C {} // comment`, scan_end must
                // be the closing `}` position, not node.end — otherwise a newline
                // after `}` (before the next statement) causes us to incorrectly
                // suppress the trailing comment that belongs to `}`.
                let scan_end = class
                    .members
                    .nodes
                    .first()
                    .and_then(|&idx| self.arena.get(idx))
                    .map_or_else(
                        || {
                            // Empty class: find the closing `}` to use as scan_end
                            let be = brace_end as usize;
                            if be <= end {
                                bytes[be..end]
                                    .iter()
                                    .position(|&b| b == b'}')
                                    .map_or(end, |p| be + p)
                            } else {
                                end
                            }
                        },
                        |m| m.pos as usize,
                    );
                let brace_end_usize = brace_end as usize;
                let scan_end_clamped = scan_end.min(end);
                let has_newline = if brace_end_usize <= scan_end_clamped {
                    bytes[brace_end_usize..scan_end_clamped]
                        .iter()
                        .any(|&b| b == b'\n' || b == b'\r')
                } else {
                    // Malformed source: first member pos precedes the opening
                    // brace we found — skip the suppression heuristic.
                    false
                };
                if has_newline {
                    self.skip_trailing_same_line_comments(brace_end, node.end);
                }
            }
        }
        self.write_line();
        self.increase_indent();

        // Store auto-accessor inits for constructor emission.
        let prev_auto_accessor_inits = std::mem::take(&mut self.pending_auto_accessor_inits);
        if !auto_accessor_instance_inits.is_empty() && lower_auto_accessors_to_weakmap {
            self.pending_auto_accessor_inits = auto_accessor_instance_inits.clone();
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
        let mut deferred_static_blocks: Vec<(NodeIndex, usize)> = Vec::new();
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
                            .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
                        || self
                            .arena
                            .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                        || self
                            .arena
                            .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
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
        let synthesize_constructor = !has_constructor
            && (!field_inits.is_empty()
                || (lower_auto_accessors_to_weakmap && !auto_accessor_instance_inits.is_empty()));

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
            if lower_auto_accessors_to_weakmap {
                for (storage_name, init_idx) in &auto_accessor_instance_inits {
                    self.write(storage_name);
                    self.write(".set(this, ");
                    match init_idx {
                        Some(init) => self.emit_expression(*init),
                        None => self.write("void 0"),
                    }
                    self.write(");");
                    self.write_line();
                }
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
        // Compute the class body's closing `}` position so the last member's
        // trailing comment scan doesn't overshoot into comments belonging to
        // the closing brace line (same pattern as namespace IIFE emitter).
        let class_body_close_pos = self
            .source_text
            .map(|text| {
                let end = std::cmp::min(node.end as usize, text.len());
                let bytes = text.as_bytes();
                let mut pos = end;
                while pos > 0 {
                    pos -= 1;
                    if bytes[pos] == b'}' {
                        return pos as u32;
                    }
                }
                node.end
            })
            .unwrap_or(node.end);

        for (member_i, &member_idx) in class.members.nodes.iter().enumerate() {
            // Skip property declarations that were lowered
            if needs_class_field_lowering
                && let Some(member_node) = self.arena.get(member_idx)
                && member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                    && let Some(prop) = self.arena.get_property_decl(member_node)
                    && !auto_accessor_members
                        .iter()
                        .any(|(accessor_idx, _, _, _)| *accessor_idx == member_idx)
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
                // Find the opening `{` of the static block to determine where
                // inner (body) comments start. We skip leading comments but save
                // the index of the first inner comment for replay during IIFE emission.
                let brace_pos = if let Some(text) = self.source_text {
                    let bytes = text.as_bytes();
                    let start = member_node.pos as usize;
                    let end = (member_node.end as usize).min(bytes.len());
                    bytes[start..end]
                        .iter()
                        .position(|&b| b == b'{')
                        .map(|off| (start + off + 1) as u32)
                        .unwrap_or(member_node.end)
                } else {
                    member_node.end
                };
                // Skip comments preceding the block opening `{`
                while self.comment_emit_idx < self.all_comments.len()
                    && self.all_comments[self.comment_emit_idx].end <= brace_pos
                {
                    self.comment_emit_idx += 1;
                }
                // Save index pointing at the first inner comment (if any)
                let inner_comment_idx = self.comment_emit_idx;
                // Skip remaining inner comments so they don't leak as leading
                // comments of subsequent class members
                self.skip_comments_for_erased_node(member_node);
                deferred_static_blocks.push((member_idx, inner_comment_idx));
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
                    // accessors (error case) are kept — tsc emits them as `{}`.
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
                .find(|(idx, _, _, _)| *idx == member_idx)
                .map(|(_, storage_name, _, is_static)| (storage_name.clone(), *is_static));
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

                if let Some((storage_name, is_static)) = auto_accessor {
                    self.emit_auto_accessor_methods(
                        member_node,
                        &storage_name,
                        is_static,
                        auto_accessor_class_alias.as_deref(),
                        lower_auto_accessors_to_private_fields,
                        &class_name,
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
                    // For the last member, use the class body's closing `}` position
                    // so we don't steal comments that belong on the closing brace line.
                    let next_member_pos = class
                        .members
                        .nodes
                        .get(member_i + 1)
                        .and_then(|&next_idx| self.arena.get(next_idx))
                        .map(|n| n.pos);
                    let upper = next_member_pos.unwrap_or(member_node.end);
                    let token_end = self.find_token_end_before_trivia(member_node.pos, upper);
                    // For the last member, cap trailing comment scan at the class
                    // body's closing `}` to avoid stealing comments that belong
                    // on the closing brace line.
                    if next_member_pos.is_none() {
                        self.emit_trailing_comments_before(token_end, class_body_close_pos);
                    } else {
                        self.emit_trailing_comments(token_end);
                    }
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
        if !static_field_inits.is_empty() && !class_name.is_empty() {
            self.write_line();
            for (name, init_idx, _member_pos, leading_comments, trailing_comments) in
                &static_field_inits
            {
                // Emit saved leading comments from the original static property declaration
                for (comment_text, source_pos) in leading_comments {
                    self.write_comment_with_reindent(comment_text, Some(*source_pos));
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

        // Emit auto-accessor WeakMap initializations after class body:
        // var _Class_prop_accessor_storage;
        // ...
        // _Class_prop_accessor_storage = new WeakMap();
        if lower_auto_accessors_to_weakmap
            && (!auto_accessor_instance_inits.is_empty()
                || !auto_accessor_static_inits.is_empty()
                || auto_accessor_class_alias.is_some())
        {
            self.write_line();
            let mut wrote_alias_line = false;

            if let Some(alias) = auto_accessor_class_alias.as_ref()
                && !alias.is_empty()
                && !class_name.is_empty()
            {
                self.write(alias);
                self.write(" = ");
                self.write(&class_name);
                wrote_alias_line = true;
            }

            if !auto_accessor_instance_inits.is_empty() {
                if wrote_alias_line {
                    self.write(", ");
                }
                let mut wrote_instance_line = false;
                for (i, (storage_name, _init_idx)) in
                    auto_accessor_instance_inits.iter().enumerate()
                {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.write(storage_name);
                    self.write(" = new WeakMap()");
                    wrote_instance_line = true;
                }
                if wrote_alias_line || wrote_instance_line {
                    self.write(";");
                    self.write_line();
                }
            }

            for (storage_name, init_idx) in &auto_accessor_static_inits {
                self.write(storage_name);
                self.write(" = { value: ");
                if let Some(init) = init_idx {
                    self.emit_expression(*init);
                } else {
                    self.write("void 0");
                }
                self.write(" };");
                self.write_line();
            }
        }

        // Emit deferred static blocks as IIFEs after the class body
        for (static_block_idx, saved_comment_idx) in deferred_static_blocks {
            self.write_line();
            self.write("(() => ");
            // Restore comment_emit_idx so inner comments from the static block
            // body are available for emit_block to emit inside the IIFE.
            self.comment_emit_idx = saved_comment_idx;
            if let Some(static_node) = self.arena.get(static_block_idx) {
                // Static block uses the same data as a Block node.
                // Treated like a function body for single-line formatting.
                let prev = self.emitting_function_body_block;
                self.emitting_function_body_block = true;
                self.emit_block(static_node, static_block_idx);
                self.emitting_function_body_block = prev;
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

    pub(in crate::emitter) fn class_has_auto_accessor_members(
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
                    .has_modifier(&prop_data.modifiers, SyntaxKind::AbstractKeyword)
                && self
                    .arena
                    .get(prop_data.name)
                    .is_none_or(|n| n.kind != SyntaxKind::PrivateIdentifier as u16)
                && !self
                    .arena
                    .has_modifier(&prop_data.modifiers, SyntaxKind::DeclareKeyword)
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

    fn emit_auto_accessor_methods(
        &mut self,
        node: &Node,
        storage_name: &str,
        is_static: bool,
        static_accessor_alias: Option<&str>,
        lower_auto_accessor_to_private_fields: bool,
        class_name: &str,
        property_end: u32,
    ) {
        let Some(prop) = self.arena.get_property_decl(node) else {
            return;
        };

        if lower_auto_accessor_to_private_fields {
            if is_static {
                self.write("static ");
                self.write("#");
                self.write(storage_name);
                if !prop.initializer.is_none() {
                    self.write(" = ");
                    self.emit_expression(prop.initializer);
                }
                self.write(";");
            } else {
                self.write("#");
                self.write(storage_name);
                if !prop.initializer.is_none() {
                    self.write(" = ");
                    self.emit_expression(prop.initializer);
                }
                self.write(";");
            }
            self.write_line();

            if is_static {
                self.write("static ");
            }
            self.write("get ");
            self.emit(prop.name);
            self.write("() { return ");
            if is_static {
                self.write(if class_name.is_empty() {
                    "this"
                } else {
                    class_name
                });
                self.write(".#");
                self.write(storage_name.trim_start_matches('#'));
                self.write("; }");
                self.write_line();
                self.write("static ");
                self.write("set ");
                self.emit(prop.name);
                self.write("(value) { ");
                self.write(if class_name.is_empty() {
                    "this"
                } else {
                    class_name
                });
                self.write(".#");
                self.write(storage_name.trim_start_matches('#'));
                self.write(" = value; }");
            } else {
                self.write("this.");
                self.write("#");
                self.write(storage_name.trim_start_matches('#'));
                self.write("; }");
                self.write_line();
                self.write("set ");
                self.emit(prop.name);
                self.write("(value) { this.");
                self.write("#");
                self.write(storage_name.trim_start_matches('#'));
                self.write(" = value; }");
            }
            self.emit_trailing_comments(property_end);
            self.write_line();
        } else if is_static {
            let Some(alias) = static_accessor_alias else {
                return;
            };
            self.write("static ");
            self.write("get ");
            self.emit(prop.name);
            self.write("() { return __classPrivateFieldGet(");
            self.write(alias);
            self.write(", ");
            self.write(alias);
            self.write(", \"f\", ");
            self.write(storage_name);
            self.write("); }");
            self.emit_trailing_comments(property_end);
            self.write_line();
            self.write("static ");
            self.write("set ");
            self.emit(prop.name);
            self.write("(value) { __classPrivateFieldSet(");
            self.write(alias);
            self.write(", ");
            self.write(alias);
            self.write(", value, \"f\", ");
            self.write(storage_name);
            self.write("); }");
        } else {
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
#[path = "../../../tests/declarations_class.rs"]
mod tests;
