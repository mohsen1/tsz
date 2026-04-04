use super::super::super::{Printer, ScriptTarget};
use super::class_has_self_references;
use super::is_ident_char;
use crate::transforms::{ClassDecoratorInfo, ClassES5Emitter};
use tsz_parser::parser::node::{Node, NodeAccess};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
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

        if let Some(class_name) = self.get_identifier_text_opt(class.name)
            && let Some(output) =
                self.render_simple_tc39_decorated_class_es5(node, idx, &class_name, &class_name)
        {
            self.write(&output);
            while self.comment_emit_idx < self.all_comments.len()
                && self.all_comments[self.comment_emit_idx].end <= node.end
            {
                self.comment_emit_idx += 1;
            }
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
        // Also checks for parameter decorators on methods and constructors.
        let has_legacy_member_decorators = self.ctx.options.legacy_decorators
            && class.members.nodes.iter().any(|&m_idx| {
                let Some(m_node) = self.arena.get(m_idx) else {
                    return false;
                };
                // Check member-level decorators
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
                let has_member_decorator = mods.is_some_and(|m| {
                    m.nodes.iter().any(|&mod_idx| {
                        self.arena
                            .get(mod_idx)
                            .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                    })
                });
                if has_member_decorator {
                    return true;
                }
                // Check parameter decorators on methods and constructors
                let params: Option<&tsz_parser::parser::NodeList> = match m_node.kind {
                    k if k == syntax_kind_ext::METHOD_DECLARATION => {
                        self.arena.get_method_decl(m_node).map(|m| &m.parameters)
                    }
                    k if k == syntax_kind_ext::CONSTRUCTOR => {
                        self.arena.get_constructor(m_node).map(|c| &c.parameters)
                    }
                    _ => None,
                };
                params.is_some_and(|p| {
                    p.nodes.iter().any(|&param_idx| {
                        let Some(param_node) = self.arena.get(param_idx) else {
                            return false;
                        };
                        let Some(param) = self.arena.get_parameter(param_node) else {
                            return false;
                        };
                        !self.collect_class_decorators(&param.modifiers).is_empty()
                    })
                })
            });

        if !legacy_class_decorators.is_empty() || has_legacy_member_decorators {
            let class_name = if class.name.is_none() {
                // For anonymous default exports with decorators, ensure we have a name
                // so __decorate calls can reference it (e.g., `default_1.prototype`)
                self.anonymous_default_export_name
                    .clone()
                    .unwrap_or_else(|| "default_1".to_string())
            } else {
                self.get_identifier_text_idx(class.name)
            };

            if self.ctx.target_es5 {
                let mut es5_emitter = ClassES5Emitter::new(self.arena);
                es5_emitter.set_temp_var_counter(self.ctx.destructuring_state.temp_var_counter);
                es5_emitter.set_indent_level(self.writer.indent_level());
                es5_emitter.set_transforms(self.transforms.clone());
                es5_emitter.set_remove_comments(self.ctx.options.remove_comments);
                if let Some(text) = self.source_text_for_map() {
                    if self.writer.has_source_map() {
                        es5_emitter
                            .set_source_map_context(text, self.writer.current_source_index());
                    } else {
                        es5_emitter.set_source_text(text);
                    }
                }
                if self.ctx.options.import_helpers && self.ctx.is_effectively_commonjs() {
                    es5_emitter.set_tslib_prefix(true);
                }
                es5_emitter
                    .set_use_define_for_class_fields(self.ctx.options.use_define_for_class_fields);
                // Pass decorator info to the ES5 emitter so __decorate calls
                // are emitted INSIDE the IIFE (before `return ClassName;`)
                es5_emitter.set_decorator_info(ClassDecoratorInfo {
                    class_decorators: legacy_class_decorators,
                    has_member_decorators: has_legacy_member_decorators,
                    emit_decorator_metadata: self.ctx.options.emit_decorator_metadata,
                });
                let output = es5_emitter.emit_class_with_name(idx, &class_name);
                self.ctx.destructuring_state.temp_var_counter = es5_emitter.temp_var_counter();
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

            // For anonymous classes that got a generated name (e.g., "default_1"),
            // ensure `anonymous_default_export_name` is set so `emit_class_es6_with_options`
            // can inject the name into the class expression.
            let prev_anon_name =
                if class.name.is_none() && self.anonymous_default_export_name.is_none() {
                    self.anonymous_default_export_name = Some(class_name.clone());
                    true
                } else {
                    false
                };

            // Check if the class needs a class-level __decorate call due to constructor
            // parameter decorators (even without class-level decorators).
            let has_ctor_param_decorators = !self
                .collect_constructor_param_decorators(&class.members.nodes)
                .is_empty();
            // A class-level __decorate is needed for class decorators OR ctor param decorators
            let needs_class_decorate =
                !legacy_class_decorators.is_empty() || has_ctor_param_decorators;

            // Detect if the class body has self-references that need aliasing.
            // When a decorated class references itself (e.g. `static x() { return C.y; }`),
            // tsc emits: `var C_1; let C = C_1 = class C { static x() { return C_1.y; } };`
            let needs_alias = !legacy_class_decorators.is_empty()
                && class_has_self_references(
                    self.arena,
                    self.source_text_for_map(),
                    &class_name,
                    &class.members.nodes,
                );

            let alias_name = if needs_alias {
                let alias = format!("{class_name}_1");
                // Emit `var C_1;\n` before the class declaration
                self.write("var ");
                self.write(&alias);
                self.write(";");
                self.write_line();
                Some(alias)
            } else {
                None
            };

            // When there are class-level decorators or ctor param decorators,
            // emit as `let Name = class { ... };`
            // When only member decorators, emit as normal `class Name { ... }`
            if needs_class_decorate {
                if let Some(ref alias) = alias_name {
                    // Emit: `let Name = Name_1 = class Name { ... };`
                    // First capture the class body, then replace self-refs
                    let before_len = self.writer.len();
                    self.emit_class_es6_with_options(
                        node,
                        idx,
                        true,
                        Some(("let", class_name.clone())),
                    );
                    let after_len = self.writer.len();

                    // Post-process: replace class name with alias in class body
                    let full_output = self.writer.get_output().to_string();
                    let emitted_str = &full_output[before_len..after_len];

                    // The emitted text starts with `let Name = class Name {`
                    // We need to insert `Name_1 = ` after `let Name = `
                    // and replace body references to Name with Name_1
                    let prefix = format!("let {class_name} = class {class_name}");
                    let alias_prefix = format!("let {class_name} = {alias} = class {class_name}");

                    let mut replaced = emitted_str.replacen(&prefix, &alias_prefix, 1);

                    // Replace self-references ONLY inside the class body (between { and };)
                    // Static fields after the class close brace should keep the original name.
                    if let Some(brace_pos) = replaced.find('{') {
                        // Find the matching close of the class expression: `};\n` or `};`
                        // The class body ends at `\n};` (the closing brace of the class expr)
                        let close_pattern = "\n};";
                        let body_end =
                            if let Some(close_pos) = replaced[brace_pos..].find(close_pattern) {
                                brace_pos + close_pos + close_pattern.len()
                            } else {
                                replaced.len()
                            };

                        let header = &replaced[..brace_pos];
                        let class_body = &replaced[brace_pos..body_end];
                        let after_class = &replaced[body_end..];

                        // Only replace identifiers within the class body
                        let mut new_body = String::with_capacity(class_body.len());
                        let name_bytes = class_name.as_bytes();
                        let body_bytes = class_body.as_bytes();
                        let mut i = 0;
                        while i < body_bytes.len() {
                            if i + name_bytes.len() <= body_bytes.len()
                                && &body_bytes[i..i + name_bytes.len()] == name_bytes
                            {
                                let before_ok = i == 0 || !is_ident_char(body_bytes[i - 1]);
                                let after_ok = i + name_bytes.len() == body_bytes.len()
                                    || !is_ident_char(body_bytes[i + name_bytes.len()]);
                                if before_ok && after_ok {
                                    new_body.push_str(alias);
                                    i += name_bytes.len();
                                    continue;
                                }
                            }
                            new_body.push(body_bytes[i] as char);
                            i += 1;
                        }
                        replaced = format!("{header}{new_body}{after_class}");
                    }

                    // Replace the emitted range with the modified text.
                    // Trim trailing newline to avoid double blank line before __decorate.
                    let replaced = replaced.trim_end_matches('\n');
                    self.writer.truncate(before_len);
                    self.write(replaced);
                    self.write_line();
                } else {
                    self.emit_class_es6_with_options(
                        node,
                        idx,
                        true,
                        Some(("let", class_name.clone())),
                    );
                }
            } else {
                self.emit_class_es6_with_options(node, idx, false, None);
            }

            // Restore anonymous_default_export_name if we temporarily set it
            if prev_anon_name {
                self.anonymous_default_export_name = None;
            }
            // Only write newline if not already at line start (class declarations
            // with lowered static fields already end with write_line()).
            if !self.writer.is_at_line_start() {
                self.write_line();
            }

            // Set type parameter names for metadata serialization so that
            // generic type params (T, U, etc.) serialize as "Object" not the param name.
            if self.ctx.options.emit_decorator_metadata
                && let Some(ref tp_list) = class.type_parameters
            {
                let tp_names: Vec<String> = tp_list
                    .nodes
                    .iter()
                    .filter_map(|&tp_idx| {
                        let tp_node = self.arena.get(tp_idx)?;
                        let tp = self.arena.get_type_parameter(tp_node)?;
                        let name = self.get_identifier_text_idx(tp.name);
                        if name.is_empty() { None } else { Some(name) }
                    })
                    .collect();
                if !tp_names.is_empty() {
                    self.metadata_class_type_params = Some(tp_names);
                }
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
            if let Some(ref alias) = alias_name {
                // Emit: `Name = Name_1 = __decorate([...], Name);`
                // We intercept the normal pattern and insert the alias assignment
                let before_len = self.writer.len();
                self.emit_legacy_class_decorator_assignment(
                    &class_name,
                    &legacy_class_decorators,
                    commonjs_exported,
                    commonjs_default,
                    false,
                    &class.members.nodes,
                );
                let after_len = self.writer.len();
                let full_output = self.writer.get_output().to_string();
                let emitted = &full_output[before_len..after_len];

                // Replace `Name = __decorate` with `Name = Name_1 = __decorate`
                let pattern = format!("{class_name} = __decorate");
                let replacement = format!("{class_name} = {alias} = __decorate");
                let modified = emitted.replacen(&pattern, &replacement, 1);
                self.writer.truncate(before_len);
                self.write(&modified);
            } else {
                self.emit_legacy_class_decorator_assignment(
                    &class_name,
                    &legacy_class_decorators,
                    commonjs_exported,
                    commonjs_default,
                    false,
                    &class.members.nodes,
                );
            }

            // Clear type parameter names after decorator emission
            self.metadata_class_type_params = None;

            return;
        }

        if self.ctx.target_es5 {
            let mut es5_emitter = ClassES5Emitter::new(self.arena);
            es5_emitter.set_temp_var_counter(self.ctx.destructuring_state.temp_var_counter);
            es5_emitter.set_indent_level(self.writer.indent_level());
            // Pass transform directives to the ClassES5Emitter
            es5_emitter.set_transforms(self.transforms.clone());
            es5_emitter.set_remove_comments(self.ctx.options.remove_comments);
            if let Some(text) = self.source_text_for_map() {
                if self.writer.has_source_map() {
                    es5_emitter.set_source_map_context(text, self.writer.current_source_index());
                } else {
                    es5_emitter.set_source_text(text);
                }
            }
            if self.ctx.options.import_helpers && self.ctx.is_effectively_commonjs() {
                es5_emitter.set_tslib_prefix(true);
            }
            es5_emitter
                .set_use_define_for_class_fields(self.ctx.options.use_define_for_class_fields);
            let output = es5_emitter.emit_class(idx);
            self.ctx.destructuring_state.temp_var_counter = es5_emitter.temp_var_counter();
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
            // Emit any trailing comment from the class's closing `}` line
            // (e.g., `class Foo { ... } // comment` → `}()); // comment`).
            // This must happen BEFORE `skip_comments_for_erased_node` consumes
            // the trailing comment.
            let class_close_pos = self.find_token_end_before_trivia(node.pos, node.end);
            self.emit_trailing_comments(class_close_pos);
            // Skip comments within the class body range since the ES5 class emitter
            // handles them separately. Without this, they'd appear at end of file.
            // Skip comments that were part of this class declaration since the
            // ES5 class emitter handles class comments internally.
            self.skip_comments_for_erased_node(node);
            return;
        }

        self.emit_class_es6_with_options(node, idx, false, None);
    }

    pub(in crate::emitter) fn can_render_simple_tc39_decorated_class_es5(
        &self,
        node: &Node,
    ) -> bool {
        if self.ctx.options.legacy_decorators || !self.ctx.target_es5 {
            return false;
        }

        let Some(class) = self.arena.get_class(node) else {
            return false;
        };

        !self.collect_class_decorators(&class.modifiers).is_empty()
            && class.members.nodes.is_empty()
            && class.heritage_clauses.is_none()
    }

    pub(in crate::emitter) fn render_simple_tc39_decorated_class_es5(
        &mut self,
        node: &Node,
        idx: NodeIndex,
        binding_name: &str,
        display_name: &str,
    ) -> Option<String> {
        if !self.can_render_simple_tc39_decorated_class_es5(node) {
            return None;
        }

        let class = self.arena.get_class(node)?;
        let decorator_exprs = self
            .collect_class_decorators(&class.modifiers)
            .into_iter()
            .filter_map(|decorator_idx| {
                let decorator_node = self.arena.get(decorator_idx)?;
                let decorator = self.arena.get_decorator(decorator_node)?;
                let before_len = self.writer.len();
                self.emit_expression(decorator.expression);
                let after_len = self.writer.len();
                let full_output = self.writer.get_output().to_string();
                let emitted = full_output[before_len..after_len].trim().to_string();
                self.writer.truncate(before_len);
                Some(emitted)
            })
            .collect::<Vec<_>>();
        if decorator_exprs.is_empty() {
            return None;
        }

        let inner_name = if class.name.is_some() && !binding_name.ends_with("_1") {
            format!("{binding_name}_1")
        } else {
            binding_name.to_string()
        };

        let mut es5_emitter = ClassES5Emitter::new(self.arena);
        es5_emitter.set_temp_var_counter(self.ctx.destructuring_state.temp_var_counter);
        es5_emitter.set_indent_level(self.writer.indent_level() + 1);
        es5_emitter.set_transforms(self.transforms.clone());
        es5_emitter.set_remove_comments(self.ctx.options.remove_comments);
        if let Some(text) = self.source_text_for_map() {
            es5_emitter.set_source_text(text);
        }
        if self.ctx.options.import_helpers && self.ctx.is_effectively_commonjs() {
            es5_emitter.set_tslib_prefix(true);
        }
        es5_emitter.set_use_define_for_class_fields(self.ctx.options.use_define_for_class_fields);
        let mut inner_output = es5_emitter.emit_class_with_name(idx, &inner_name);
        self.ctx.destructuring_state.temp_var_counter = es5_emitter.temp_var_counter();
        inner_output = inner_output.trim_end_matches('\n').to_string();

        let base_indent = "    ".repeat(self.writer.indent_level() as usize);
        let body_indent = "    ".repeat((self.writer.indent_level() + 1) as usize);
        let decorator_indent = "    ".repeat((self.writer.indent_level() + 2) as usize);

        let inner_prefix = format!("var {inner_name} = ");
        let indented_inner_prefix = format!("{body_indent}{inner_prefix}");
        if inner_output.starts_with(&inner_prefix) {
            inner_output = format!(
                "{body_indent}var {binding_name} = _classThis = {}",
                &inner_output[inner_prefix.len()..]
            );
        } else if inner_output.starts_with(&indented_inner_prefix) {
            inner_output = format!(
                "{body_indent}var {binding_name} = _classThis = {}",
                &inner_output[indented_inner_prefix.len()..]
            );
        } else if !inner_output.starts_with(&body_indent) {
            inner_output = format!("{body_indent}{inner_output}");
        }

        Some(format!(
            "{base_indent}var {binding_name} = function () {{\n{body_indent}var _classDecorators = [{}];\n{body_indent}var _classDescriptor;\n{body_indent}var _classExtraInitializers = [];\n{body_indent}var _classThis;\n{inner_output}\n{body_indent}__setFunctionName(_classThis, \"{display_name}\");\n{body_indent}(function () {{\n{decorator_indent}var _metadata = typeof Symbol === \"function\" && Symbol.metadata ? Object.create(null) : void 0;\n{decorator_indent}__esDecorate(null, _classDescriptor = {{ value: _classThis }}, _classDecorators, {{ kind: \"class\", name: _classThis.name, metadata: _metadata }}, null, _classExtraInitializers);\n{decorator_indent}{binding_name} = _classThis = _classDescriptor.value;\n{decorator_indent}if (_metadata) Object.defineProperty(_classThis, Symbol.metadata, {{ enumerable: true, configurable: true, writable: true, value: _metadata }});\n{decorator_indent}__runInitializers(_classThis, _classExtraInitializers);\n{body_indent}}})();\n{body_indent}return {binding_name} = _classThis;\n{base_indent}}}();",
            decorator_exprs.join(", "),
        ))
    }

    pub(in crate::emitter) fn emit_tc39_decorated_class_expression(
        &mut self,
        class_node: NodeIndex,
        display_name: &str,
    ) -> bool {
        if self.ctx.options.legacy_decorators || self.ctx.options.target == ScriptTarget::ESNext {
            return false;
        }

        let Some(node) = self.arena.get(class_node) else {
            return false;
        };
        if node.kind != syntax_kind_ext::CLASS_DECLARATION {
            return false;
        }

        if self.ctx.target_es5 {
            return false;
        }

        use crate::transforms::es_decorators::TC39DecoratorEmitter;

        let mut emitter = TC39DecoratorEmitter::new(self.arena);
        emitter.set_indent_level(self.writer.indent_level() as usize);
        emitter.set_use_static_blocks(!self.ctx.needs_es2022_lowering);
        emitter.set_use_define_for_class_fields(self.ctx.options.use_define_for_class_fields);
        emitter.set_expression_mode(true);
        emitter.set_function_name(display_name.to_string());
        if self.ctx.options.import_helpers && self.ctx.is_effectively_commonjs() {
            emitter.set_tslib_prefix(true);
        }
        if let Some(text) = self.source_text_for_map() {
            emitter.set_source_text(text);
        }

        let output = emitter.emit_class(class_node);
        if output.is_empty() {
            return false;
        }

        let mut output = output.trim_end_matches('\n').to_string();
        if display_name == "default" {
            output = output.replace(
                "var class_1 = _classThis = class",
                "var default_1 = _classThis = class",
            );
            output = output.replace(
                "class_1 = _classThis = _classDescriptor.value",
                "default_1 = _classThis = _classDescriptor.value",
            );
            output = output.replace(
                "return class_1 = _classThis;",
                "return default_1 = _classThis;",
            );
            output = output.replace(
                "__setFunctionName(_classThis, \"class_1\")",
                "__setFunctionName(_classThis, \"default\")",
            );
        }

        self.write(&output);
        self.skip_comments_for_erased_node(node);
        true
    }
}
