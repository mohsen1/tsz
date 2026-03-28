use super::super::{ModuleKind, Printer, ScriptTarget};
use crate::context::transform::IdentifierId;
use crate::transforms::emit_utils;
use crate::transforms::{ClassDecoratorInfo, ClassES5Emitter};
use tsz_parser::parser::node::{Node, NodeAccess};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    /// Emit a module specifier, rewriting extension if rewriteRelativeImportExtensions is set.
    pub(in crate::emitter) fn emit_module_specifier(&mut self, specifier_idx: NodeIndex) {
        if !self.ctx.options.rewrite_relative_import_extensions {
            self.emit(specifier_idx);
            return;
        }
        let Some(node) = self.arena.get(specifier_idx) else {
            self.emit(specifier_idx);
            return;
        };
        let text = if let Some(lit) = self.arena.get_literal(node) {
            &lit.text
        } else {
            self.emit(specifier_idx);
            return;
        };
        if !text.starts_with("./") && !text.starts_with("../") {
            self.emit(specifier_idx);
            return;
        }
        let rewritten = self.rewrite_module_spec(text);
        if rewritten == *text {
            self.emit(specifier_idx);
            return;
        }
        let quote = if let Some(src) = self.source_text_for_map() {
            let pos = node.pos as usize;
            if pos < src.len() && src.as_bytes()[pos] == b'\'' {
                '\''
            } else {
                '"'
            }
        } else {
            '"'
        };
        self.write(&format!("{quote}{rewritten}{quote}"));
    }

    /// Rewrite a module specifier if rewriteRelativeImportExtensions is enabled.
    /// Transforms .ts→.js, .tsx→.jsx, .mts→.mjs, .cts→.cjs for relative paths.
    pub(in crate::emitter) fn rewrite_module_spec(&self, spec: &str) -> String {
        if !self.ctx.options.rewrite_relative_import_extensions {
            return spec.to_string();
        }
        if !spec.starts_with("./") && !spec.starts_with("../") {
            return spec.to_string();
        }
        if let Some(base) = spec.strip_suffix(".ts") {
            return format!("{base}.js");
        }
        if let Some(base) = spec.strip_suffix(".tsx") {
            return format!("{base}.jsx");
        }
        if let Some(base) = spec.strip_suffix(".mts") {
            return format!("{base}.mjs");
        }
        if let Some(base) = spec.strip_suffix(".cts") {
            return format!("{base}.cjs");
        }
        spec.to_string()
    }

    pub(in crate::emitter) fn next_commonjs_module_var(&mut self, module_spec: &str) -> String {
        let base = crate::transforms::emit_utils::sanitize_module_name(module_spec);
        let next = self
            .ctx
            .module_state
            .module_temp_counters
            .entry(base.clone())
            .and_modify(|n| *n += 1)
            .or_insert(1);
        format!("{base}_{next}")
    }

    /// Emit a CommonJS export with optional hoisting of the export assignment.
    ///
    /// When `is_hoisted_declaration` is true (for function declarations), the
    /// `exports.default = name;` assignment is emitted BEFORE the declaration.
    /// tsc does this because JS function declarations are hoisted — the binding
    /// exists at the top of the scope regardless of textual position.
    pub(in crate::emitter) fn emit_commonjs_export_with_hoisting<F>(
        &mut self,
        names: &[IdentifierId],
        is_default: bool,
        is_hoisted_declaration: bool,
        emit_inner: &mut F,
    ) where
        F: FnMut(&mut Self),
    {
        if names.is_empty() {
            emit_inner(self);
            return;
        }

        // For default exports of hoisted declarations (functions), emit
        // the export assignment before the declaration body, matching tsc.
        // Skip if the assignment was already hoisted to the preamble.
        let hoisted_inline = is_default
            && is_hoisted_declaration
            && !self.ctx.module_state.default_func_export_hoisted;
        if hoisted_inline {
            self.write_export_binding_start("default");
            self.write_identifier_by_id(names[0]);
            self.write_export_binding_end();
            self.write_line();
        }

        let prev_module = self.ctx.options.module;
        let prev_original = self.ctx.original_module_kind;
        self.ctx.options.module = ModuleKind::None;
        self.ctx.original_module_kind = Some(prev_module);

        let before_len = self.writer.len();
        emit_inner(self);
        let inner_emitted = self.writer.len() > before_len;

        self.ctx.options.module = prev_module;
        self.ctx.original_module_kind = prev_original;

        // If the inner emit produced nothing (e.g., variable declaration with
        // no initializer where only the type annotation was stripped), skip
        // the export assignment. The preamble `exports.X = void 0;` already
        // handles the forward declaration.
        if !inner_emitted {
            return;
        }

        // For hoisted declarations (functions), the export assignment was already
        // emitted — either above as inline hoisting (default), or in the preamble
        // (named function exports via `exports.foo = foo;`).
        if is_hoisted_declaration {
            if !self.writer.is_at_line_start() {
                self.write_line();
            }
            return;
        }

        // Only write newline if not already at line start (class declarations
        // with lowered static fields already end with write_line()).
        if !self.writer.is_at_line_start() {
            self.write_line();
        }
        if is_default {
            self.write_export_binding_start("default");
            self.write_identifier_by_id(names[0]);
            self.write_export_binding_end();
        } else {
            for (i, name) in names.iter().enumerate() {
                if i > 0 {
                    self.write_line();
                }
                let name_str = self
                    .arena
                    .identifiers
                    .get(*name as usize)
                    .map(|id| id.escaped_text.clone())
                    .unwrap_or_default();
                self.write_export_binding_start(&name_str);
                self.write_identifier_by_id(*name);
                self.write_export_binding_end();
            }
        }
        self.write_line();
    }

    pub(in crate::emitter) fn emit_commonjs_default_export_expr(
        &mut self,
        node: &Node,
        idx: NodeIndex,
    ) {
        self.emit_commonjs_default_export_assignment(|this| {
            this.emit_commonjs_default_export_expr_inner(node, idx);
        });
    }

    pub(in crate::emitter) fn emit_commonjs_default_export_expr_inner(
        &mut self,
        node: &Node,
        idx: NodeIndex,
    ) {
        match node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.emit_function_expression(node, idx);
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.emit_class_es6(node, idx);
            }
            _ => {
                self.emit_node_default(node, idx);
            }
        }
    }

    /// Emit anonymous default export as a named declaration + export assignment.
    /// TSC pattern: `export default class {}` → `class default_1 {}\nexports.default = default_1;`
    pub(in crate::emitter) fn emit_commonjs_anonymous_default_as_named(
        &mut self,
        node: &Node,
        idx: NodeIndex,
    ) {
        // For anonymous default function/class declarations, tsc assigns a
        // synthetic name (`default_1`) and hoists `exports.default = default_1;`
        // BEFORE the declaration. This works because function declarations are
        // hoisted in JS.
        let is_function = node.kind == syntax_kind_ext::FUNCTION_DECLARATION;
        let prev = self.anonymous_default_export_name.take();
        self.anonymous_default_export_name = Some("default_1".to_string());
        if is_function {
            // Function: exports.default before declaration (functions hoist)
            self.write_export_binding_start("default");
            self.write("default_1");
            self.write_export_binding_end();
            self.write_line();
            self.emit_node_default(node, idx);
        } else {
            let before_len = self.writer.len();
            if self.emit_tc39_decorated_class_expression(idx, "default") {
                let after_len = self.writer.len();
                let full_output = self.writer.get_output().to_string();
                let expr = full_output[before_len..after_len]
                    .trim_end_matches('\n')
                    .to_string();
                self.writer.truncate(before_len);
                self.write_export_binding_start("default");
                self.write(&expr);
                self.write_export_binding_end();
                self.write_line();
            } else {
                self.writer.truncate(before_len);
                // Class/other: declaration first, then exports.default
                self.emit_node_default(node, idx);
                self.write_line();
                self.write_export_binding_start("default");
                self.write("default_1");
                self.write_export_binding_end();
            }
        }
        self.anonymous_default_export_name = prev;
    }

    pub(in crate::emitter) fn emit_commonjs_default_export_assignment<F>(
        &mut self,
        mut emit_inner: F,
    ) where
        F: FnMut(&mut Self),
    {
        self.write_export_binding_start("default");
        emit_inner(self);
        if self.in_system_execute_body {
            self.write(");");
        } else {
            self.write_semicolon();
        }
        self.write_line();
    }

    pub(in crate::emitter) fn emit_commonjs_default_export_class_es5(
        &mut self,
        class_node: NodeIndex,
    ) {
        let Some(node) = self.arena.get(class_node) else {
            return;
        };

        if node.kind != syntax_kind_ext::CLASS_DECLARATION {
            self.emit_node_default(node, class_node);
            return;
        }

        if let Some(output) =
            self.render_simple_tc39_decorated_class_es5(node, class_node, "default_1", "default")
        {
            self.write(&output);
            self.write_line();
            self.write_export_binding_start("default");
            self.write("default_1");
            self.write_export_binding_end();
            self.write_line();
            return;
        }

        let temp_name = "default_1".to_string();
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
        es5_emitter.set_use_define_for_class_fields(self.ctx.options.use_define_for_class_fields);
        if self.ctx.options.legacy_decorators
            && let Some(class) = self.arena.get_class(node)
        {
            let class_decorators = self.collect_class_decorators(&class.modifiers);
            if !class_decorators.is_empty() {
                es5_emitter.set_decorator_info(ClassDecoratorInfo {
                    class_decorators,
                    has_member_decorators: false,
                    emit_decorator_metadata: self.ctx.options.emit_decorator_metadata,
                });
            }
        }
        let es5_output = es5_emitter.emit_class_with_name(class_node, &temp_name);
        self.ctx.destructuring_state.temp_var_counter = es5_emitter.temp_var_counter();
        let mappings = es5_emitter.take_mappings();
        if !mappings.is_empty() && self.writer.has_source_map() {
            self.writer.write("");
            let base_line = self.writer.current_line();
            let base_column = self.writer.current_column();
            self.writer
                .add_offset_mappings(base_line, base_column, &mappings);
            self.writer.write(&es5_output);
        } else {
            self.write(&es5_output);
        }
        self.write_line();
        self.write_export_binding_start("default");
        self.write(&temp_name);
        self.write_export_binding_end();
        self.write_line();
    }

    // =========================================================================
    // Exports
    // =========================================================================

    pub(in crate::emitter) fn emit_export_declaration(&mut self, node: &Node) {
        if self.ctx.is_commonjs() {
            self.emit_export_declaration_commonjs(node);
        } else {
            self.emit_export_declaration_es6(node);
        }
    }

    pub(in crate::emitter) fn emit_export_declaration_es6(&mut self, node: &Node) {
        let Some(export) = self.arena.get_export_decl(node) else {
            return;
        };

        if export.is_type_only {
            return;
        }

        if export.is_default_export {
            // `export default m` where `m` is an identifier referring to a type-only entity
            // (e.g., a non-instantiated namespace or interface) should not emit anything.
            // tsc elides these entirely — the only output is `export {};` from the file-level
            // module-marker logic.
            if let Some(clause_node) = self.arena.get(export.export_clause)
                && (clause_node.kind == SyntaxKind::Identifier as u16
                    || clause_node.kind == syntax_kind_ext::QUALIFIED_NAME)
                && !self.export_default_target_has_runtime_value(export.export_clause)
            {
                return;
            }

            // Check if the clause is a declaration (function/class) that doesn't need semicolon
            let clause_is_func_or_class =
                if let Some(clause_node) = self.arena.get(export.export_clause) {
                    clause_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                        || clause_node.kind == syntax_kind_ext::CLASS_DECLARATION
                } else {
                    false
                };

            // When the clause is a class with legacy (experimental) class-level decorators,
            // tsc separates the export: `let C = class C {}; C = __decorate(...); export default C;`
            // The class emitter handles this internally, so skip the `export default` prefix here.
            let class_has_legacy_class_decorators = self.ctx.options.legacy_decorators
                && if let Some(clause_node) = self.arena.get(export.export_clause) {
                    clause_node.kind == syntax_kind_ext::CLASS_DECLARATION
                        && if let Some(class) = self.arena.get_class(clause_node) {
                            !self.collect_class_decorators(&class.modifiers).is_empty()
                        } else {
                            false
                        }
                } else {
                    false
                };

            // When a default-exported class has static field initializers that will be
            // lowered (emitted after the class body), tsc separates the export:
            //   class C { }
            //   C.s = 0;
            //   export default C;
            // This is needed because static initializers must come after the class body
            // but before the export statement.
            let class_needs_separated_export = if !class_has_legacy_class_decorators {
                if let Some(clause_node) = self.arena.get(export.export_clause)
                    && clause_node.kind == syntax_kind_ext::CLASS_DECLARATION
                    && let Some(class) = self.arena.get_class(clause_node)
                {
                    let needs_class_field_lowering = (self.ctx.options.target as u32)
                        < (ScriptTarget::ES2022 as u32)
                        || !self.ctx.options.use_define_for_class_fields;
                    if needs_class_field_lowering {
                        // Check if the class has any static properties with initializers
                        class.members.nodes.iter().any(|&member_idx| {
                            if let Some(member_node) = self.arena.get(member_idx)
                                && member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                                && let Some(prop) = self.arena.get_property_decl(member_node)
                                && prop.initializer.is_some()
                                && self
                                    .arena
                                    .has_modifier(&prop.modifiers, SyntaxKind::StaticKeyword)
                                && !self
                                    .arena
                                    .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
                                && !self
                                    .arena
                                    .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                                && !self
                                    .arena
                                    .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
                            {
                                true
                            } else {
                                false
                            }
                        })
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };

            if class_has_legacy_class_decorators || class_needs_separated_export {
                // Emit the class without `export default` prefix, then emit
                // `export default C;` afterward. For legacy decorators, the class
                // emitter handles `let C = class C {};` + `__decorate` internally.
                let class_name = if let Some(clause_node) = self.arena.get(export.export_clause)
                    && let Some(class) = self.arena.get_class(clause_node)
                {
                    if class.name.is_none() {
                        "default_1".to_string()
                    } else {
                        self.get_identifier_text_idx(class.name)
                    }
                } else {
                    String::new()
                };
                if class_has_legacy_class_decorators
                    && self.ctx.target_es5
                    && class_name == "default_1"
                    && let Some(clause_node) = self.arena.get(export.export_clause)
                    && let Some(class) = self.arena.get_class(clause_node)
                {
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
                    es5_emitter.set_use_define_for_class_fields(
                        self.ctx.options.use_define_for_class_fields,
                    );
                    let class_decorators = self.collect_class_decorators(&class.modifiers);
                    es5_emitter.set_decorator_info(ClassDecoratorInfo {
                        class_decorators,
                        has_member_decorators: false,
                        emit_decorator_metadata: self.ctx.options.emit_decorator_metadata,
                    });
                    let output =
                        es5_emitter.emit_class_with_name(export.export_clause, &class_name);
                    self.ctx.destructuring_state.temp_var_counter = es5_emitter.temp_var_counter();
                    self.write(&output);
                    if !self.writer.is_at_line_start() {
                        self.write_line();
                    }
                    self.write("export default ");
                    self.write(&class_name);
                    self.write(";");
                    return;
                }
                // For anonymous classes, set the override name so the class emitter
                // uses "default_1" as the binding name.
                let prev_name = self.anonymous_default_export_name.take();
                if class_name == "default_1" {
                    self.anonymous_default_export_name = Some("default_1".to_string());
                }
                self.emit(export.export_clause);
                self.anonymous_default_export_name = prev_name;
                if !class_name.is_empty() {
                    // Only add a newline if the class emitter didn't already end on one.
                    // ES2015 classes with static inits end with write_line() after
                    // `ClassName.field = value;`, but ES5 IIFEs end with `}());`.
                    if !self.writer.is_at_line_start() {
                        self.write_line();
                    }
                    self.write("export default ");
                    self.write(&class_name);
                    self.write(";");
                }
            } else {
                // When a default-exported class has ES (non-legacy) decorators,
                // tsc emits decorators BEFORE `export default`:
                //   @dec
                //   export default class C { }
                let default_class_has_es_decorators = !self.ctx.options.legacy_decorators
                    && clause_is_func_or_class
                    && if let Some(cn) = self.arena.get(export.export_clause) {
                        cn.kind == syntax_kind_ext::CLASS_DECLARATION
                            && if let Some(class) = self.arena.get_class(cn) {
                                !self.collect_class_decorators(&class.modifiers).is_empty()
                            } else {
                                false
                            }
                    } else {
                        false
                    };

                if default_class_has_es_decorators {
                    if let Some(cn) = self.arena.get(export.export_clause)
                        && let Some(class) = self.arena.get_class(cn)
                    {
                        if class.name.is_none() {
                            if self.ctx.target_es5 {
                                if let Some(output) = self.render_simple_tc39_decorated_class_es5(
                                    cn,
                                    export.export_clause,
                                    "default_1",
                                    "default",
                                ) {
                                    self.write(&output);
                                    self.write_line();
                                    self.write("export default default_1;");
                                    return;
                                }
                            } else {
                                let before_len = self.writer.len();
                                if self.emit_tc39_decorated_class_expression(
                                    export.export_clause,
                                    "default",
                                ) {
                                    let after_len = self.writer.len();
                                    let full_output = self.writer.get_output().to_string();
                                    let expr = full_output[before_len..after_len]
                                        .trim_end_matches('\n')
                                        .to_string();
                                    self.writer.truncate(before_len);
                                    self.write("export default ");
                                    self.write(&expr);
                                    self.write(";");
                                    return;
                                }
                                self.writer.truncate(before_len);
                            }
                        }

                        let decorators = self.collect_class_decorators(&class.modifiers);
                        for dec_idx in &decorators {
                            self.emit(*dec_idx);
                            self.write_line();
                        }
                    }
                    self.write("export default ");
                    if let Some(cn) = self.arena.get(export.export_clause) {
                        self.emit_class_es6_with_options(cn, export.export_clause, true, None);
                    }
                } else {
                    self.write("export default ");
                    self.emit(export.export_clause);
                    if !clause_is_func_or_class {
                        self.write_semicolon();
                    }
                }
            }
            return;
        }

        if export.export_clause.is_none() {
            self.write("export *");
            if export.module_specifier.is_some() {
                self.write(" from ");
                self.emit_module_specifier(export.module_specifier);
            }
            self.emit_import_attributes(export.attributes);
            self.write_semicolon();
            return;
        }

        let Some(clause_node) = self.arena.get(export.export_clause) else {
            return;
        };

        if clause_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            self.write("export ");
            self.emit_import_equals_declaration_inner(clause_node);
            self.write_semicolon();
            return;
        }

        if clause_node.kind == syntax_kind_ext::NAMED_EXPORTS
            && let Some(named_exports) = self.arena.get_named_imports(clause_node)
        {
            // For local exports (`export { x }`), use syntactic value-name
            // filtering to skip type-only specifiers (interfaces, type aliases,
            // etc.). For re-exports (`export { x } from "mod"`), only use the
            // checker-based filtering (type_only_nodes).
            let value_specs = if export.module_specifier.is_none() {
                self.collect_local_export_value_specifiers(&named_exports.elements)
            } else {
                self.collect_value_specifiers(&named_exports.elements)
            };
            if value_specs.is_empty() && !named_exports.elements.nodes.is_empty() {
                // All specifiers were type-only — track the elision for local exports
                // so we can emit `export {};` at the end of the file if needed (when
                // no other module syntax survived). Skip entirely for re-exports.
                if export.module_specifier.is_none() {
                    self.ctx.module_state.had_elided_export_clause = true;
                }
                return;
            }
            // Emit `export { ... }` or `export {}` (when originally empty)
            if value_specs.is_empty() {
                self.write("export {}");
            } else {
                self.write("export { ");
                self.emit_comma_separated(&value_specs);
                if self.has_trailing_comma_in_source(clause_node, &named_exports.elements.nodes) {
                    self.write(",");
                }
                self.write(" }");
            }
            if export.module_specifier.is_some() {
                self.write(" from ");
                self.emit_module_specifier(export.module_specifier);
            }
            self.emit_import_attributes(export.attributes);
            self.write_semicolon();
            return;
        }

        // export * as <name> from "..." — clause is an Identifier or StringLiteral
        if export.module_specifier.is_some()
            && (clause_node.kind == SyntaxKind::Identifier as u16
                || clause_node.kind == SyntaxKind::StringLiteral as u16)
        {
            self.write("export * as ");
            self.emit(export.export_clause);
            self.write(" from ");
            self.emit_module_specifier(export.module_specifier);
            self.emit_import_attributes(export.attributes);
            self.write_semicolon();
            return;
        }

        if self.export_clause_is_type_only(clause_node) {
            return;
        }

        // Check if the clause is a declaration that handles its own semicolons
        let is_declaration = clause_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
            || clause_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || clause_node.kind == syntax_kind_ext::CLASS_DECLARATION
            || clause_node.kind == syntax_kind_ext::ENUM_DECLARATION
            || clause_node.kind == syntax_kind_ext::MODULE_DECLARATION;

        if clause_node.kind == syntax_kind_ext::CLASS_DECLARATION
            && !export.is_default_export
            && self.ctx.options.legacy_decorators
            && let Some(class) = self.arena.get_class(clause_node)
        {
            let legacy_decorators = self.collect_class_decorators(&class.modifiers);
            if !legacy_decorators.is_empty()
                && let Some(name) = self.get_identifier_text_opt(class.name)
            {
                self.emit_class_declaration(clause_node, export.export_clause);
                self.write_line();
                self.write("export { ");
                self.write(&name);
                self.write(" };");
                return;
            }
        }

        // When an ES5-transformed class is exported in ESM mode, tsc separates the
        // declaration from the export: `var C = (function() { ... }());` then
        // `export { C };` (or `export default C;`). We detect this by checking if
        // the class has an ES5 transform directive.
        if clause_node.kind == syntax_kind_ext::CLASS_DECLARATION
            && !self.ctx.is_commonjs()
            && self.transforms.has_transform(export.export_clause)
            && let Some(class) = self.arena.get_class(clause_node)
            && let Some(name) = self.get_identifier_text_opt(class.name)
        {
            self.emit(export.export_clause);
            self.write_line();
            if export.is_default_export {
                self.write("export default ");
                self.write(&name);
                self.write(";");
            } else {
                self.write("export { ");
                self.write(&name);
                self.write(" };");
            }
            return;
        }

        // For merged enums/namespaces/classes/functions, the second+ declaration
        // should not be prefixed with `export`. The first declaration gets
        // `export var E;` and subsequent ones are bare IIFEs. We detect this by
        // checking if the name is already in `declared_namespace_names`, which
        // means a prior declaration already emitted the `var`/`export` prefix.
        let is_merged_subsequent = self.is_merged_subsequent_declaration(clause_node);

        // When a class has ES (non-legacy) decorators and is exported, tsc emits
        // decorators BEFORE the `export` keyword:
        //   @dec
        //   export class C { }
        // We need to emit decorators first, then `export`, then the class body
        // with modifiers suppressed (since decorators were already emitted).
        let class_has_es_decorators = !self.ctx.options.legacy_decorators
            && clause_node.kind == syntax_kind_ext::CLASS_DECLARATION
            && if let Some(class) = self.arena.get_class(clause_node) {
                !self.collect_class_decorators(&class.modifiers).is_empty()
            } else {
                false
            };

        if class_has_es_decorators {
            // Emit decorators before `export`
            if let Some(class) = self.arena.get_class(clause_node) {
                let decorators = self.collect_class_decorators(&class.modifiers);
                for dec_idx in &decorators {
                    self.emit(*dec_idx);
                    self.write_line();
                }
            }
            if !is_merged_subsequent {
                self.write("export ");
            }
            // Emit the class with modifiers suppressed (decorators already emitted)
            self.emit_class_es6_with_options(clause_node, export.export_clause, true, None);
        } else {
            if !is_merged_subsequent {
                self.write("export ");
            }
            self.emit(export.export_clause);
        }

        if export.module_specifier.is_some() {
            self.write(" from ");
            self.emit_module_specifier(export.module_specifier);
        }

        // Don't add semicolon for declarations - they handle their own
        if !is_declaration {
            self.write_semicolon();
        }
    }

    /// Emit export assignment (export = expr or export default expr)
    pub(in crate::emitter) fn emit_export_assignment(&mut self, node: &Node) {
        let Some(export_assign) = self.arena.get_export_assignment(node) else {
            return;
        };

        // Check if we're inside an AMD/UMD wrapper (original module was AMD/UMD)
        let is_amd_or_umd = matches!(
            self.ctx.original_module_kind,
            Some(ModuleKind::AMD) | Some(ModuleKind::UMD)
        );

        // System modules: `export =` is not valid and tsc suppresses it.
        // Don't emit `module.exports = expr;` inside System.register bodies.
        if self.in_system_execute_body && export_assign.is_export_equals {
            return;
        }

        if is_amd_or_umd && export_assign.is_export_equals {
            // AMD/UMD: export = expr → return expr;
            self.write("return ");
            self.emit_expression(export_assign.expression);
            self.write_semicolon();
        } else if self.ctx.is_commonjs() {
            // CommonJS: export = expr → module.exports = expr;
            //           export default expr → exports.default = expr;
            if export_assign.is_export_equals {
                self.write("module.exports = ");
            } else {
                // `export default expr` — use `exports.X` when the identifier was
                // inlined (`exports.x = val;`), local name otherwise.
                if let Some(expr_node) = self.arena.get(export_assign.expression)
                    && expr_node.kind == SyntaxKind::Identifier as u16
                {
                    let ident = self.get_identifier_text_idx(export_assign.expression);
                    if self.ctx.module_state.inlined_var_exports.contains(&ident) {
                        self.write_export_binding_start("default");
                        if self.in_system_execute_body {
                            self.write(&ident);
                        } else {
                            self.write("exports.");
                            self.write(&ident);
                        }
                        self.write_export_binding_end();
                        self.write_line();
                        return;
                    }
                }
                self.write_export_binding_start("default");
            }
            self.emit_expression(export_assign.expression);
            if !export_assign.is_export_equals && self.in_system_execute_body {
                self.write(");");
            } else {
                self.write_semicolon();
            }
        } else {
            // ES6: export = expr (not valid ES6, but emit as export default)
            //      export default expr → export default expr;
            self.write("export default ");
            self.emit_expression(export_assign.expression);
            self.write_semicolon();
        }
    }

    /// Collect variable names from a `VARIABLE_STATEMENT` node
    pub(in crate::emitter) fn collect_variable_names_from_node(&self, node: &Node) -> Vec<String> {
        let mut names = Vec::new();
        if let Some(var_stmt) = self.arena.get_variable(node) {
            // VARIABLE_STATEMENT has declarations containing VARIABLE_DECLARATION_LIST
            for &decl_list_idx in &var_stmt.declarations.nodes {
                if let Some(decl_list_node) = self.arena.get(decl_list_idx) {
                    // VARIABLE_DECLARATION_LIST has declarations containing VARIABLE_DECLARATION
                    if let Some(decl_list) = self.arena.get_variable(decl_list_node) {
                        for &decl_idx in &decl_list.declarations.nodes {
                            if let Some(decl_node) = self.arena.get(decl_idx)
                                && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                            {
                                self.collect_binding_names(decl.name, &mut names);
                            }
                        }
                    }
                }
            }
        }
        names
    }

    /// Try to collect inline CJS export info for a variable statement.
    /// Returns `Some(vec of (decoded_name, emit_name, initializer_idx))` if ALL
    /// declarators are simple identifier bindings with initializers. Returns None if
    /// any declarator uses destructuring or lacks an initializer (in which case we
    /// fall back to split form).
    ///
    /// `decoded_name` is the semantic name (for set tracking/matching).
    /// `emit_name` preserves unicode escapes from the source to match tsc output.
    pub(in crate::emitter) fn try_collect_inline_cjs_exports(
        &self,
        node: &Node,
    ) -> Option<Vec<(String, String, NodeIndex)>> {
        let var_stmt = self.arena.get_variable(node)?;
        let mut result = Vec::new();

        for &decl_list_idx in &var_stmt.declarations.nodes {
            let decl_list_node = self.arena.get(decl_list_idx)?;
            let decl_list = self.arena.get_variable(decl_list_node)?;

            for &decl_idx in &decl_list.declarations.nodes {
                let decl_node = self.arena.get(decl_idx)?;
                let decl = self.arena.get_variable_declaration(decl_node)?;

                // Must be a simple identifier (not destructuring)
                let name_node = self.arena.get(decl.name)?;
                if name_node.kind != SyntaxKind::Identifier as u16 {
                    return None;
                }
                let ident = self.arena.get_identifier(name_node)?;
                let decoded_name = ident.escaped_text.clone();
                // Use original_text (preserving unicode escapes) when available,
                // falling back to escaped_text (decoded name). TSC preserves
                // unicode escape sequences in emitted CJS inline exports.
                let emit_name = ident
                    .original_text
                    .as_deref()
                    .unwrap_or(&ident.escaped_text)
                    .to_string();

                // Must have an initializer
                if decl.initializer.is_none() {
                    return None;
                }

                // tsc uses split form (const x = val; exports.x = x;) for
                // arrow functions, function expressions, and class expressions.
                // Only primitive/object/call initializers use inline form.
                if let Some(init_node) = self.arena.get(decl.initializer) {
                    let k = init_node.kind;
                    if k == syntax_kind_ext::ARROW_FUNCTION
                        || k == syntax_kind_ext::FUNCTION_EXPRESSION
                        || k == syntax_kind_ext::CLASS_EXPRESSION
                    {
                        return None;
                    }
                }

                result.push((decoded_name, emit_name, decl.initializer));
            }
        }

        if result.is_empty() {
            return None;
        }
        Some(result)
    }

    /// Get identifier text from optional node index
    pub(in crate::emitter) fn get_identifier_text_opt(&self, idx: NodeIndex) -> Option<String> {
        crate::transforms::emit_utils::identifier_text(self.arena, idx)
    }

    pub(in crate::emitter) fn get_module_root_name(&self, name_idx: NodeIndex) -> Option<String> {
        if name_idx.is_none() {
            return None;
        }

        let node = self.arena.get(name_idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .arena
                .get_identifier(node)
                .map(|id| id.escaped_text.clone());
        }

        if node.kind == syntax_kind_ext::QUALIFIED_NAME
            && let Some(qn) = self.arena.qualified_names.get(node.data_index as usize)
        {
            return self.get_module_root_name(qn.left);
        }

        None
    }

    /// Get identifier text from a node index
    pub(in crate::emitter) fn get_identifier_text_idx(&self, idx: NodeIndex) -> String {
        crate::transforms::emit_utils::identifier_text_or_empty(self.arena, idx)
    }

    /// Get text from a specifier name node (either an identifier or string literal).
    pub(in crate::emitter) fn get_specifier_name_text(&self, idx: NodeIndex) -> Option<String> {
        crate::transforms::emit_utils::specifier_name_text(self.arena, idx)
    }

    /// Write a property access on a module variable: `mod.name` for identifiers,
    /// `mod["name"]` for non-identifier names.
    pub(in crate::emitter) fn write_module_property_access(
        &mut self,
        module_var: &str,
        property_name: &str,
    ) {
        if super::super::is_valid_identifier_name(property_name) {
            self.write(module_var);
            self.write(".");
            self.write(property_name);
        } else {
            self.write(module_var);
            self.write("[\"");
            self.write(property_name);
            self.write("\"]");
        }
    }

    /// Get property name emit info: identifier → Dot, string literal → Bracket,
    /// numeric literal → `BracketNumeric`. Returns None for computed names.
    pub(in crate::emitter) fn get_property_name_emit(
        &self,
        idx: NodeIndex,
    ) -> Option<crate::emitter::core::PropertyNameEmit> {
        use crate::emitter::core::PropertyNameEmit;
        use tsz_parser::parser::node::NodeAccess;
        let node = self.arena.get(idx)?;
        match node.kind {
            k if k == SyntaxKind::Identifier as u16
                || k == SyntaxKind::PrivateIdentifier as u16 =>
            {
                let text = crate::transforms::emit_utils::identifier_text_or_empty(self.arena, idx);
                if text.is_empty() {
                    None
                } else {
                    Some(PropertyNameEmit::Dot(text))
                }
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                let text = self.arena.get_literal_text(idx)?;
                Some(PropertyNameEmit::Bracket(format!("\"{text}\"")))
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let text = self.arena.get_literal_text(idx)?;
                Some(PropertyNameEmit::BracketNumeric(text.to_string()))
            }
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => {
                let text = self.arena.get_literal_text(idx)?;
                Some(PropertyNameEmit::Bracket(format!("`{text}`")))
            }
            k if k == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                let computed = self.arena.get_computed_property(node)?;
                // Recursively resolve the inner expression
                let inner_emit = self.get_property_name_emit(computed.expression)?;
                // Wrap in brackets: Dot("foo") -> Bracket("foo"), Bracket(x) -> Bracket(x)
                match inner_emit {
                    PropertyNameEmit::Dot(s) => Some(PropertyNameEmit::Bracket(format!("\"{s}\""))),
                    PropertyNameEmit::Bracket(s) => Some(PropertyNameEmit::Bracket(s)),
                    PropertyNameEmit::BracketNumeric(s) => {
                        Some(PropertyNameEmit::BracketNumeric(s))
                    }
                }
            }
            _ => None,
        }
    }

    pub(in crate::emitter) fn emit_entity_name(&mut self, idx: NodeIndex) {
        if idx.is_none() {
            return;
        }

        let Some(node) = self.arena.get(idx) else {
            return;
        };

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                self.emit_identifier(node);
            }
            k if k == SyntaxKind::ThisKeyword as u16 => self.write("this"),
            k if k == SyntaxKind::SuperKeyword as u16 => self.write("super"),
            k if k == syntax_kind_ext::QUALIFIED_NAME => {
                if let Some(name) = self.arena.get_qualified_name(node) {
                    self.emit_entity_name(name.left);
                    self.write(".");
                    self.emit_entity_name(name.right);
                }
            }
            _ => {}
        }
    }

    pub(in crate::emitter) fn emit_named_exports(&mut self, node: &Node) {
        // Named exports uses the same data structure as named imports
        let Some(exports) = self.arena.get_named_imports(node) else {
            self.write("{ }");
            return;
        };

        self.write("{ ");
        self.emit_comma_separated(&exports.elements.nodes);
        self.write(" }");
    }

    /// Emit a named import/export specifier: `[propertyName as] name`
    pub(in crate::emitter) fn emit_specifier(&mut self, node: &Node) {
        let Some(spec) = self.arena.get_specifier(node) else {
            return;
        };

        if spec.property_name.is_some() {
            self.emit(spec.property_name);
            self.write(" as ");
        }
        self.emit(spec.name);
    }

    pub(in crate::emitter) fn collect_value_specifiers(
        &self,
        elements: &NodeList,
    ) -> Vec<NodeIndex> {
        let mut specs = Vec::new();
        for &spec_idx in &elements.nodes {
            // Check explicit "import type" syntax (parser-set flag)
            if let Some(spec_node) = self.arena.get(spec_idx)
                && let Some(spec) = self.arena.get_specifier(spec_node)
                && spec.is_type_only
            {
                continue;
            }
            // Check implicit type-only imports (type checker side-table)
            // This handles cases like `import { Interface }` where Interface refers to an interface
            if self.ctx.options.type_only_nodes.contains(&spec_idx) {
                continue;
            }
            specs.push(spec_idx);
        }
        specs
    }

    /// Like `collect_value_specifiers` but also filters specifiers that refer
    /// to type-only declarations using the syntactic `value_declaration_names`
    /// set. This is only appropriate for local exports (`export { x }` without
    /// `from`), NOT for re-exports or imports.
    pub(in crate::emitter) fn collect_local_export_value_specifiers(
        &self,
        elements: &NodeList,
    ) -> Vec<NodeIndex> {
        let base = self.collect_value_specifiers(elements);
        if !self.ctx.module_state.value_decl_names_computed {
            return base;
        }
        base.into_iter()
            .filter(|&spec_idx| {
                if let Some(spec_node) = self.arena.get(spec_idx)
                    && let Some(spec) = self.arena.get_specifier(spec_node)
                {
                    let local_name = if spec.property_name.is_some() {
                        self.get_identifier_text_idx(spec.property_name)
                    } else {
                        self.get_identifier_text_idx(spec.name)
                    };
                    if !local_name.is_empty() {
                        return self
                            .ctx
                            .module_state
                            .value_declaration_names
                            .contains(&local_name);
                    }
                }
                true
            })
            .collect()
    }

    pub(in crate::emitter) fn export_clause_is_type_only(&self, clause_node: &Node) -> bool {
        crate::transforms::emit_utils::export_clause_is_type_only(
            self.arena,
            clause_node,
            self.ctx.options.preserve_const_enums,
        )
    }

    /// Check if this declaration is a subsequent (merged) declaration whose name
    /// was already declared by a prior statement. For merged enums/namespaces,
    /// the first declaration emits `export var E;` and subsequent ones should
    /// be bare IIFEs without `export`.
    fn is_merged_subsequent_declaration(&self, clause_node: &Node) -> bool {
        match clause_node.kind {
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = self.arena.get_enum(clause_node)
                    && let Some(name) = self.get_identifier_text_opt(enum_decl.name)
                {
                    return self.declared_namespace_names.contains(&name);
                }
                false
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                if let Some(module_decl) = self.arena.get_module(clause_node)
                    && let Some(name) = self.get_module_root_name(module_decl.name)
                {
                    return self.declared_namespace_names.contains(&name);
                }
                false
            }
            _ => false,
        }
    }

    /// Check if the file contains an export assignment (export =) with a runtime value.
    pub(in crate::emitter) fn has_export_assignment(&self, statements: &NodeList) -> bool {
        for &stmt_idx in &statements.nodes {
            if let Some(node) = self.arena.get(stmt_idx)
                && node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                && !self.export_assignment_identifier_is_type_only(node, statements)
            {
                return true;
            }
        }
        false
    }

    pub(in crate::emitter) fn export_assignment_identifier_is_type_only(
        &self,
        export_assignment_node: &Node,
        statements: &NodeList,
    ) -> bool {
        // With --verbatimModuleSyntax, type-only exports are NOT elided.
        // tsc preserves `export = I` → `module.exports = I;` even for interfaces.
        if self.ctx.options.verbatim_module_syntax {
            return false;
        }

        let Some(export_assign) = self.arena.get_export_assignment(export_assignment_node) else {
            return false;
        };
        let Some(assigned_name) = self.get_module_root_name(export_assign.expression) else {
            return false;
        };

        let mut matched_type = false;
        let mut matched_runtime = false;

        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            match stmt_node.kind {
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                    if self.arena.get_interface(stmt_node).is_some_and(|iface| {
                        self.get_identifier_text_idx(iface.name) == assigned_name
                    }) {
                        matched_type = true;
                    }
                }
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                    if self.arena.get_type_alias(stmt_node).is_some_and(|alias| {
                        self.get_identifier_text_idx(alias.name) == assigned_name
                    }) {
                        matched_type = true;
                    }
                }
                k if k == syntax_kind_ext::CLASS_DECLARATION => {
                    if self.arena.get_class(stmt_node).is_some_and(|class_decl| {
                        self.get_identifier_text_idx(class_decl.name) == assigned_name
                    }) && !self.arena.get_class(stmt_node).is_some_and(|class_decl| {
                        self.arena
                            .has_modifier(&class_decl.modifiers, SyntaxKind::DeclareKeyword)
                    }) {
                        matched_runtime = true;
                    }
                }
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                    if self.arena.get_function(stmt_node).is_some_and(|func| {
                        self.get_identifier_text_idx(func.name) == assigned_name
                    }) && self.arena.get_function(stmt_node).is_some_and(|func| {
                        func.body.is_some()
                            && !self
                                .arena
                                .has_modifier(&func.modifiers, SyntaxKind::DeclareKeyword)
                    }) {
                        matched_runtime = true;
                    }
                }
                k if k == syntax_kind_ext::ENUM_DECLARATION => {
                    if self.arena.get_enum(stmt_node).is_some_and(|enum_decl| {
                        self.get_identifier_text_idx(enum_decl.name) == assigned_name
                    }) && self.arena.get_enum(stmt_node).is_some_and(|enum_decl| {
                        !self
                            .arena
                            .has_modifier(&enum_decl.modifiers, SyntaxKind::DeclareKeyword)
                            && !self
                                .arena
                                .has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword)
                    }) {
                        matched_runtime = true;
                    }
                }
                k if k == syntax_kind_ext::MODULE_DECLARATION => {
                    if self.arena.get_module(stmt_node).is_some_and(|module_decl| {
                        self.get_identifier_text_idx(module_decl.name) == assigned_name
                    }) && self.arena.get_module(stmt_node).is_some_and(|module_decl| {
                        !self
                            .arena
                            .has_modifier(&module_decl.modifiers, SyntaxKind::DeclareKeyword)
                            && self.is_instantiated_module(module_decl.body)
                    }) {
                        matched_runtime = true;
                    }
                }
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                    if self
                        .collect_variable_names_from_node(stmt_node)
                        .iter()
                        .any(|n| n == &assigned_name)
                        && !self.arena.get_variable(stmt_node).is_some_and(|var_decl| {
                            self.arena
                                .has_modifier(&var_decl.modifiers, SyntaxKind::DeclareKeyword)
                        })
                    {
                        matched_runtime = true;
                    }
                }
                k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                    if self
                        .arena
                        .get_import_decl(stmt_node)
                        .is_some_and(|import_decl| {
                            self.get_identifier_text_idx(import_decl.import_clause) == assigned_name
                                && self.import_decl_has_runtime_value(import_decl)
                        })
                    {
                        matched_runtime = true;
                    }
                }
                k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                    if let Some(export_decl) = self.arena.get_export_decl(stmt_node)
                        && let Some(inner) = self.arena.get(export_decl.export_clause)
                    {
                        let matches_exported_type = (inner.kind
                            == syntax_kind_ext::INTERFACE_DECLARATION
                            && self.arena.get_interface(inner).is_some_and(|iface| {
                                self.get_identifier_text_idx(iface.name) == assigned_name
                            }))
                            || (inner.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                                && self.arena.get_type_alias(inner).is_some_and(|alias| {
                                    self.get_identifier_text_idx(alias.name) == assigned_name
                                }));
                        if matches_exported_type {
                            matched_type = true;
                        }
                    }
                }
                _ => {}
            }
        }

        matched_type && !matched_runtime
    }

    /// Check whether a statement node carries an `export` modifier.
    /// Covers all declaration kinds that can be exported: variable, function,
    /// class, enum, module/namespace, interface, and type alias.
    pub(in crate::emitter) fn statement_has_export_modifier(&self, node: &Node) -> bool {
        match node.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.arena.get_variable(node).is_some_and(|v| {
                    self.arena
                        .has_modifier(&v.modifiers, SyntaxKind::ExportKeyword)
                })
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.arena.get_function(node).is_some_and(|f| {
                    self.arena
                        .has_modifier(&f.modifiers, SyntaxKind::ExportKeyword)
                })
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.arena.get_class(node).is_some_and(|c| {
                    self.arena
                        .has_modifier(&c.modifiers, SyntaxKind::ExportKeyword)
                })
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.arena.get_enum(node).is_some_and(|e| {
                    self.arena
                        .has_modifier(&e.modifiers, SyntaxKind::ExportKeyword)
                })
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.arena.get_module(node).is_some_and(|m| {
                    self.arena
                        .has_modifier(&m.modifiers, SyntaxKind::ExportKeyword)
                })
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                self.arena.get_interface(node).is_some_and(|i| {
                    self.arena
                        .has_modifier(&i.modifiers, SyntaxKind::ExportKeyword)
                })
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                self.arena.get_type_alias(node).is_some_and(|t| {
                    self.arena
                        .has_modifier(&t.modifiers, SyntaxKind::ExportKeyword)
                })
            }
            _ => false,
        }
    }

    /// Check if a file is a module (has any import/export syntax).
    /// TypeScript considers a file a module if it has ANY import/export syntax,
    /// including type-only imports/exports, declared exports, and exported
    /// interfaces/type aliases.
    pub(in crate::emitter) fn file_is_module(&self, statements: &NodeList) -> bool {
        // moduleDetection=force: treat all non-declaration files as modules
        if self.ctx.options.module_detection_force {
            return true;
        }
        // Node16/NodeNext resolved to ESM: file is definitively a module based on
        // file extension (.mts) or package.json "type":"module", regardless of content
        if self.ctx.options.resolved_node_module_to_esm {
            return true;
        }
        for &stmt_idx in &statements.nodes {
            if let Some(node) = self.arena.get(stmt_idx) {
                match node.kind {
                    k if k == syntax_kind_ext::IMPORT_DECLARATION
                        || k == syntax_kind_ext::EXPORT_DECLARATION
                        || k == syntax_kind_ext::EXPORT_ASSIGNMENT =>
                    {
                        return true;
                    }
                    // import equals: only `import x = require("mod")` makes this a module,
                    // NOT `import x = M.A` (namespace alias, not a module indicator)
                    k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                        if let Some(import_data) = self.arena.get_import_decl(node)
                            && let Some(spec_node) = self.arena.get(import_data.module_specifier)
                            && spec_node.kind == SyntaxKind::StringLiteral as u16
                        {
                            return true;
                        }
                    }
                    _ => {
                        if self.statement_has_export_modifier(node) {
                            return true;
                        }
                    }
                }
            }
        }
        // `import.meta` usage makes the file a module (ESM-only syntax).
        if self.contains_import_meta(statements) {
            return true;
        }
        false
    }

    pub(in crate::emitter) fn collect_module_dependencies(
        &self,
        statements: &[NodeIndex],
    ) -> Vec<String> {
        let mut deps = Vec::new();
        for &stmt_idx in statements {
            let Some(node) = self.arena.get(stmt_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::IMPORT_DECLARATION
                || node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                if let Some(import_decl) = self.arena.get_import_decl(node) {
                    if !self.import_decl_has_runtime_value(import_decl) {
                        continue;
                    }
                    if let Some(text) =
                        emit_utils::module_specifier_text(self.arena, import_decl.module_specifier)
                        && !deps.contains(&text)
                    {
                        deps.push(text);
                    }
                }
                continue;
            }

            if node.kind == syntax_kind_ext::EXPORT_DECLARATION
                && let Some(export_decl) = self.arena.get_export_decl(node)
            {
                if !self.export_decl_has_runtime_value(export_decl) {
                    continue;
                }
                if let Some(text) =
                    emit_utils::module_specifier_text(self.arena, export_decl.module_specifier)
                    && !deps.contains(&text)
                {
                    deps.push(text);
                }
            }
        }

        deps
    }

    pub(in crate::emitter) fn import_decl_has_runtime_value(
        &self,
        import_decl: &tsz_parser::parser::node::ImportDeclData,
    ) -> bool {
        if import_decl.import_clause.is_none() {
            return true;
        }

        let Some(clause_node) = self.arena.get(import_decl.import_clause) else {
            return true;
        };

        if clause_node.kind != syntax_kind_ext::IMPORT_CLAUSE {
            // For `import X = require("module")`, check if it has an external module.
            // For `import X = Y` (identifier/qualified name), only emit when the
            // target resolves to a runtime value (TypeScript elides type-only aliases).
            if let Some(spec_node) = self.arena.get(import_decl.module_specifier) {
                return match spec_node.kind {
                    k if k == SyntaxKind::StringLiteral as u16 => true,
                    k if k == syntax_kind_ext::EXTERNAL_MODULE_REFERENCE => true,
                    k if k == SyntaxKind::Identifier as u16
                        || k == syntax_kind_ext::QUALIFIED_NAME =>
                    {
                        self.namespace_alias_target_has_runtime_value(
                            import_decl.module_specifier,
                            None,
                        )
                    }
                    _ => false,
                };
            }
            return false;
        }

        let Some(clause) = self.arena.get_import_clause(clause_node) else {
            return true;
        };

        if clause.is_type_only {
            return false;
        }

        if clause.name.is_some() {
            return true;
        }

        if clause.named_bindings.is_none() {
            return false;
        }

        let Some(bindings_node) = self.arena.get(clause.named_bindings) else {
            return false;
        };

        let Some(named) = self.arena.get_named_imports(bindings_node) else {
            return true;
        };

        if named.name.is_some() {
            return true;
        }

        if named.elements.nodes.is_empty() {
            return true;
        }

        for &spec_idx in &named.elements.nodes {
            let Some(spec_node) = self.arena.get(spec_idx) else {
                continue;
            };
            if let Some(spec) = self.arena.get_specifier(spec_node)
                && !spec.is_type_only
            {
                return true;
            }
        }

        false
    }

    pub(in crate::emitter) fn export_decl_has_runtime_value(
        &self,
        export_decl: &tsz_parser::parser::node::ExportDeclData,
    ) -> bool {
        crate::transforms::emit_utils::export_decl_has_runtime_value(
            self.arena,
            export_decl,
            self.ctx.options.preserve_const_enums,
        )
    }

    /// Check if we should emit the __esModule marker.
    /// Returns true if the file contains any ES6 module syntax (import/export),
    /// excluding `export =` which is legacy `CommonJS`.
    /// TypeScript emits __esModule for ANY module syntax, including type-only
    /// imports/exports, declared exports, exported interfaces/type aliases,
    /// and `import.meta` usage (which makes the file a module per spec).
    ///
    /// Mirrors tsc's `shouldEmitUnderscoreUnderscoreESModule`:
    /// - JS files with CJS patterns (module.exports, exports.foo) and no real ESM syntax
    ///   do NOT get __esModule, even when moduleDetection=force.
    /// - Files with `export =` do NOT get __esModule.
    /// - All other module files get __esModule.
    pub(in crate::emitter) fn should_emit_es_module_marker(&self, statements: &NodeList) -> bool {
        // If file has a runtime `export =`, do not emit __esModule.
        // Type-only `export =` aliases (e.g. interface) are filtered out.
        if self.has_export_assignment(statements) {
            return false;
        }

        // Check if the file has real ESM syntax (import/export statements)
        let has_esm_syntax = self.has_esm_module_syntax(statements);

        // tsc's shouldEmitUnderscoreUnderscoreESModule:
        // For JS files (.js/.cjs/.mjs) with CJS patterns (module.exports, exports.foo)
        // and no real ESM import/export syntax, skip __esModule.
        // This matches: `hasJSFileExtension(file) && file.commonJsModuleIndicator &&
        //   (!file.externalModuleIndicator || file.externalModuleIndicator === true)`
        if self.is_current_root_js_source
            && self.has_commonjs_module_indicator(statements)
            && !has_esm_syntax
        {
            return false;
        }

        // If file has real ESM syntax, emit __esModule
        if has_esm_syntax {
            return true;
        }

        // moduleDetection=force: treat all non-declaration files as modules
        if self.ctx.options.module_detection_force {
            return true;
        }

        false
    }

    /// Check if the file has any ESM module syntax (import/export statements,
    /// import.meta, export modifiers).
    fn has_esm_module_syntax(&self, statements: &NodeList) -> bool {
        for &stmt_idx in &statements.nodes {
            if let Some(node) = self.arena.get(stmt_idx) {
                match node.kind {
                    k if k == syntax_kind_ext::IMPORT_DECLARATION => {
                        return true;
                    }
                    // import equals: only `import x = require("mod")` is module syntax
                    k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                        if let Some(import_data) = self.arena.get_import_decl(node)
                            && let Some(spec_node) = self.arena.get(import_data.module_specifier)
                            && spec_node.kind == SyntaxKind::StringLiteral as u16
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                        return true;
                    }
                    // Type-only `export =` still marks the file as a module and
                    // TypeScript emits `__esModule` for it.
                    k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                        if self.export_assignment_identifier_is_type_only(node, statements) {
                            return true;
                        }
                    }
                    // Check for export modifier on any declaration type
                    // (including declare and type-only declarations)
                    _ => {
                        if self.statement_has_export_modifier(node) {
                            return true;
                        }
                    }
                }
            }
        }

        // `import.meta` usage makes the file a module (ESM-only syntax).
        if self.contains_import_meta(statements) {
            return true;
        }

        false
    }

    /// Check if the file has CJS module patterns like `module.exports = ...`,
    /// `exports.foo = ...`, or `require("...")`.
    /// This is a lightweight emitter-level check that approximates tsc's
    /// binder-level `commonJsModuleIndicator`.
    fn has_commonjs_module_indicator(&self, statements: &NodeList) -> bool {
        for &stmt_idx in &statements.nodes {
            if let Some(node) = self.arena.get(stmt_idx)
                && self.statement_has_cjs_pattern(node)
            {
                return true;
            }
        }
        false
    }

    /// Check if a statement contains CJS module patterns.
    /// Looks for:
    /// - `module.exports = ...` (`BinaryExpression` with `PropertyAccessExpression`)
    /// - `exports.foo = ...` (`BinaryExpression` with `PropertyAccessExpression`)
    /// - Top-level `require("...")` calls
    fn statement_has_cjs_pattern(&self, node: &Node) -> bool {
        // Check expression statements: `module.exports = X;` or `exports.foo = X;`
        if node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
            && let Some(expr_stmt) = self.arena.get_expression_statement(node)
            && let Some(expr_node) = self.arena.get(expr_stmt.expression)
        {
            return self.expression_is_cjs_pattern(expr_node);
        }
        false
    }

    /// Get the identifier text from a node, if it is an identifier.
    fn identifier_text_of(&self, node: &Node) -> Option<&str> {
        self.arena
            .get_identifier(node)
            .map(|id| id.escaped_text.as_str())
    }

    /// Check if an expression is a CJS module pattern.
    fn expression_is_cjs_pattern(&self, node: &Node) -> bool {
        // Binary expression: `module.exports = X` or `exports.foo = X`
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.arena.get_binary_expr(node)
            && let Some(left) = self.arena.get(bin.left)
        {
            // Check for `module.exports` or `exports.foo`
            if left.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && let Some(access) = self.arena.get_access_expr(left)
                && let Some(expr) = self.arena.get(access.expression)
            {
                let expr_text = self.identifier_text_of(expr);
                // `module.exports = ...`
                if expr_text == Some("module")
                    && let Some(name) = self.arena.get(access.name_or_argument)
                    && self.identifier_text_of(name) == Some("exports")
                {
                    return true;
                }
                // `exports.foo = ...`
                if expr_text == Some("exports") {
                    return true;
                }
            }
        }
        // Call expression: `require("...")`
        if node.kind == syntax_kind_ext::CALL_EXPRESSION
            && let Some(call) = self.arena.get_call_expr(node)
            && let Some(callee) = self.arena.get(call.expression)
            && self.identifier_text_of(callee) == Some("require")
        {
            return true;
        }
        false
    }

    /// Check if any statement contains an `import.meta` expression.
    /// Walks the AST looking for `PropertyAccessExpression` nodes where the
    /// expression is the `import` keyword (the AST shape for `import.meta`).
    fn contains_import_meta(&self, statements: &NodeList) -> bool {
        let mut stack: Vec<NodeIndex> = statements.nodes.clone();
        while let Some(idx) = stack.pop() {
            if idx.is_none() {
                continue;
            }
            let Some(node) = self.arena.get(idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && let Some(access) = self.arena.get_access_expr(node)
                && let Some(expr_node) = self.arena.get(access.expression)
                && expr_node.kind == SyntaxKind::ImportKeyword as u16
                && self
                    .get_identifier_text_opt(access.name_or_argument)
                    .as_deref()
                    == Some("meta")
            {
                return true;
            }
            for child in self.arena.get_children(idx) {
                stack.push(child);
            }
        }
        false
    }

    /// Write the appropriate variable declaration keyword based on target.
    /// For ES2015+, use `const` for top-level module imports.
    /// For ES3/ES5, use `var`.
    pub(in crate::emitter) fn write_var_or_const(&mut self) {
        if self.ctx.target_es5 {
            self.write("var ");
        } else {
            self.write("const ");
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::emitter::{ModuleKind, Printer, PrinterOptions};
    use tsz_parser::ParserState;

    /// When moduleDetection=force, a file without any import/export syntax
    /// should still be treated as a module and get the CJS __esModule preamble.
    #[test]
    fn module_detection_force_emits_esmodule_marker() {
        let source = r#"console.log("hello");"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let options = PrinterOptions {
            module: ModuleKind::CommonJS,
            module_detection_force: true,
            ..Default::default()
        };
        let mut printer = Printer::with_options(&parser.arena, options);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("Object.defineProperty(exports, \"__esModule\""),
            "moduleDetection=force should emit __esModule marker for non-module file.\nOutput:\n{output}"
        );
    }

    /// Without moduleDetection=force, a file without import/export syntax
    /// should NOT get the CJS __esModule preamble.
    #[test]
    fn no_module_detection_force_skips_esmodule_marker() {
        let source = r#"console.log("hello");"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let options = PrinterOptions {
            module: ModuleKind::CommonJS,
            module_detection_force: false,
            ..Default::default()
        };
        let mut printer = Printer::with_options(&parser.arena, options);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            !output.contains("__esModule"),
            "Without moduleDetection=force, non-module file should NOT get __esModule.\nOutput:\n{output}"
        );
    }

    /// moduleDetection=force should also cause "use strict" to be emitted
    /// for CJS modules (since the file is now treated as a module).
    #[test]
    fn module_detection_force_emits_use_strict_for_cjs() {
        let source = r#"console.log("hello");"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let options = PrinterOptions {
            module: ModuleKind::CommonJS,
            module_detection_force: true,
            ..Default::default()
        };
        let mut printer = Printer::with_options(&parser.arena, options);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("\"use strict\""),
            "moduleDetection=force with CJS should emit \"use strict\".\nOutput:\n{output}"
        );
    }

    /// `export default function f()` in CJS should emit `exports.default = f;`
    /// BEFORE the function declaration, because JS function declarations are
    /// hoisted. This matches tsc's output ordering.
    #[test]
    fn default_export_function_hoists_export_assignment() {
        let source = "export default function f() { return 1; }\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let options = PrinterOptions {
            module: ModuleKind::CommonJS,
            ..Default::default()
        };
        let mut printer = Printer::with_options(&parser.arena, options);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        // exports.default = f; must appear before `function f()`
        let export_pos = output.find("exports.default = f;");
        let func_pos = output.find("function f()");
        assert!(
            export_pos.is_some() && func_pos.is_some(),
            "Should emit both exports.default = f; and function f().\nOutput:\n{output}"
        );
        assert!(
            export_pos.unwrap() < func_pos.unwrap(),
            "exports.default = f; should appear before function f() (hoisting).\nOutput:\n{output}"
        );
    }

    /// `export default function func()` with other statements before the
    /// function should hoist `exports.default = func;` to the preamble,
    /// before all other statements. This matches tsc behavior:
    /// ```js
    /// "use strict";
    /// Object.defineProperty(exports, "__esModule", { value: true });
    /// exports.default = func;        // <-- hoisted to preamble
    /// var before = func();           // <-- source statement
    /// function func() { return func; } // <-- function declaration
    /// ```
    #[test]
    fn default_export_function_hoisted_to_preamble() {
        let source = r#"var before: typeof func = func();
export default function func(): typeof func {
    return func;
}
var after: typeof func = func();
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let options = PrinterOptions {
            module: ModuleKind::CommonJS,
            ..Default::default()
        };
        let mut printer = Printer::with_options(&parser.arena, options);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        // exports.default = func; should be in the preamble (before `var before`)
        let export_pos = output.find("exports.default = func;");
        let before_pos = output.find("var before");
        let func_pos = output.find("function func()");
        assert!(
            export_pos.is_some(),
            "Should emit exports.default = func; in preamble.\nOutput:\n{output}"
        );
        assert!(
            before_pos.is_some(),
            "Should emit var before.\nOutput:\n{output}"
        );
        assert!(
            export_pos.unwrap() < before_pos.unwrap(),
            "exports.default = func; should appear before var before (preamble hoisting).\nOutput:\n{output}"
        );
        assert!(
            export_pos.unwrap() < func_pos.unwrap(),
            "exports.default = func; should appear before function func().\nOutput:\n{output}"
        );
        // Should NOT have a duplicate exports.default = func; at the function's position
        let count = output.matches("exports.default = func;").count();
        assert_eq!(
            count, 1,
            "Should emit exports.default = func; exactly once.\nOutput:\n{output}"
        );
    }

    /// Non-default function exports should NOT have the export hoisted before
    /// the function — they are handled in the preamble instead.
    #[test]
    fn named_export_function_not_hoisted() {
        let source = "export function g() { return 2; }\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let options = PrinterOptions {
            module: ModuleKind::CommonJS,
            ..Default::default()
        };
        let mut printer = Printer::with_options(&parser.arena, options);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        // For named exports, the preamble emits `exports.g = g;` before the
        // function, and there's no second assignment after.
        let preamble_pos = output.find("exports.g = g;");
        let func_pos = output.find("function g()");
        assert!(
            preamble_pos.is_some() && func_pos.is_some(),
            "Should emit both exports.g = g; and function g().\nOutput:\n{output}"
        );
        assert!(
            preamble_pos.unwrap() < func_pos.unwrap(),
            "Preamble exports.g = g; should appear before function g().\nOutput:\n{output}"
        );
    }

    /// `export { f }` where `f` is a function declaration should emit
    /// `exports.f = f;` in the preamble (hoisted) and NOT emit a duplicate
    /// assignment at the `export { f }` statement position.
    #[test]
    fn named_export_specifier_for_function_hoisted() {
        let source = r#"function isValid(x: unknown): x is string {
    return typeof x === "string";
}
export { isValid };
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let options = PrinterOptions {
            module: ModuleKind::CommonJS,
            ..Default::default()
        };
        let mut printer = Printer::with_options(&parser.arena, options);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        // The preamble should contain `exports.isValid = isValid;`
        assert!(
            output.contains("exports.isValid = isValid;"),
            "Should emit hoisted exports.isValid = isValid; in preamble.\nOutput:\n{output}"
        );
        // Should NOT contain `exports.isValid = void 0;`
        assert!(
            !output.contains("exports.isValid = void 0"),
            "Function export should NOT get void 0 initialization.\nOutput:\n{output}"
        );
        // The hoisted assignment should appear before the function body
        let export_pos = output.find("exports.isValid = isValid;").unwrap();
        let func_pos = output.find("function isValid(").unwrap();
        assert!(
            export_pos < func_pos,
            "exports.isValid = isValid; should appear before function isValid().\nOutput:\n{output}"
        );
        // Should only appear once (no duplicate from the inline export { } handler)
        let count = output.matches("exports.isValid = isValid;").count();
        assert_eq!(
            count, 1,
            "exports.isValid = isValid; should appear exactly once.\nOutput:\n{output}"
        );
    }

    /// `export { f as g }` where `f` is a function should still hoist
    /// the export with the exported name `g` in the preamble.
    #[test]
    fn named_export_specifier_aliased_function_hoisted() {
        let source = r#"function impl() { return 42; }
export { impl as myFunc };
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let options = PrinterOptions {
            module: ModuleKind::CommonJS,
            ..Default::default()
        };
        let mut printer = Printer::with_options(&parser.arena, options);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        // The preamble should contain `exports.myFunc = impl;`
        // (using the local name `impl`, not the exported alias `myFunc` — tsc behavior)
        assert!(
            output.contains("exports.myFunc = impl;"),
            "Should emit hoisted exports.myFunc = impl; in preamble.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("exports.myFunc = void 0"),
            "Aliased function export should NOT get void 0.\nOutput:\n{output}"
        );
    }

    /// Merged enum declarations in ESM should only have `export` on the first
    /// declaration's `var` statement. Subsequent IIFEs should be bare.
    #[test]
    fn merged_enum_esm_no_spurious_export() {
        let source = r#"export enum Animals {
    Cat = 1
}
export enum Animals {
    Dog = 2
}
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let options = PrinterOptions {
            module: ModuleKind::ESNext,
            ..Default::default()
        };
        let mut printer = Printer::with_options(&parser.arena, options);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        // First IIFE should be preceded by `export var Animals;`
        assert!(
            output.contains("export var Animals;"),
            "First enum should have `export var Animals;`.\nOutput:\n{output}"
        );

        // Second IIFE should NOT be preceded by `export`
        // Count occurrences of "export" — should be exactly 1 (on the var decl)
        let export_count = output.matches("export ").count();
        assert_eq!(
            export_count, 1,
            "Should have exactly one `export` (on the var declaration), not on subsequent IIFEs.\nOutput:\n{output}"
        );
    }

    /// Merged namespace declarations in ESM should only have `export` on the
    /// first var declaration, not on subsequent IIFEs.
    #[test]
    fn merged_namespace_esm_no_spurious_export() {
        let source = r#"export function F() { }
export namespace F {
    export var x = 1;
}
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let options = PrinterOptions {
            module: ModuleKind::ESNext,
            ..Default::default()
        };
        let mut printer = Printer::with_options(&parser.arena, options);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        // The namespace IIFE `(function (F) {...})(F || (F = {}))` should NOT
        // be preceded by `export`.
        assert!(
            !output.contains("export (function"),
            "Merged namespace IIFE should not be preceded by `export`.\nOutput:\n{output}"
        );
    }

    /// When a class has legacy decorators and is exported in CJS, the
    /// `exports.X = X;` pre-assignment should appear exactly once — from
    /// `emit_legacy_class_decorator_assignment`, NOT also from the
    /// `pending_commonjs_class_export_name` path.
    #[test]
    fn decorated_class_export_no_duplicate_exports() {
        let source = "declare var dec: any;\n@dec export class A {}\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let options = PrinterOptions {
            module: ModuleKind::CommonJS,
            legacy_decorators: true,
            ..Default::default()
        };
        let mut printer = Printer::with_options(&parser.arena, options);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        // Count occurrences of `exports.A = A;`
        let count = output.matches("exports.A = A;").count();
        assert_eq!(
            count, 1,
            "exports.A = A; should appear exactly once (pre-assignment before __decorate), \
             not duplicated.\nOutput:\n{output}"
        );
        // The __decorate assignment should also reference exports.A
        assert!(
            output.contains("exports.A = A = __decorate("),
            "Should contain the decorator assignment.\nOutput:\n{output}"
        );
    }

    /// When `export = f` is present with `export function f()`, the hoisted
    /// `exports.f = f;` preamble should be suppressed because `module.exports = f`
    /// replaces the entire exports object.
    #[test]
    fn export_assignment_suppresses_hoisted_func_export() {
        let source = "export function f() { }\nexport = f;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let options = PrinterOptions {
            module: ModuleKind::CommonJS,
            ..Default::default()
        };
        let mut printer = Printer::with_options(&parser.arena, options);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            !output.contains("exports.f = f;"),
            "Hoisted exports.f = f; should be suppressed when export = is present.\nOutput:\n{output}"
        );
        assert!(
            output.contains("module.exports = f;"),
            "module.exports = f; should be present for export =.\nOutput:\n{output}"
        );
    }

    /// When `export = B` is present alongside `export class C {}`, the
    /// `exports.C = void 0;` initialization should still be emitted (tsc behavior),
    /// but hoisted function exports should be suppressed.
    #[test]
    fn export_assignment_keeps_void_zero_init_for_classes() {
        let source = "export class C {}\nexport = B;\n";

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let options = PrinterOptions {
            module: ModuleKind::CommonJS,
            ..Default::default()
        };
        let mut printer = Printer::with_options(&parser.arena, options);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("exports.C = void 0;"),
            "exports.C = void 0; should be emitted even with export =.\nOutput:\n{output}"
        );
        assert!(
            output.contains("module.exports = B;"),
            "module.exports = B; should be present.\nOutput:\n{output}"
        );
    }

    /// A file using `import.meta` (with no import/export syntax) should be
    /// treated as a module and get the CJS __esModule preamble. `import.meta`
    /// is ESM-only syntax, making the file implicitly a module.
    #[test]
    fn import_meta_triggers_esmodule_marker() {
        let source = r#"const url = import.meta.url;
console.log(url);
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let options = PrinterOptions {
            module: ModuleKind::CommonJS,
            ..Default::default()
        };
        let mut printer = Printer::with_options(&parser.arena, options);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("Object.defineProperty(exports, \"__esModule\""),
            "File with import.meta should emit __esModule marker.\nOutput:\n{output}"
        );
    }

    /// A file without any module syntax or import.meta should NOT get __esModule.
    #[test]
    fn no_import_meta_no_esmodule_marker() {
        let source = r#"const x = 1;
console.log(x);
"#;

        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let options = PrinterOptions {
            module: ModuleKind::CommonJS,
            ..Default::default()
        };
        let mut printer = Printer::with_options(&parser.arena, options);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            !output.contains("__esModule"),
            "File without module syntax should NOT get __esModule marker.\nOutput:\n{output}"
        );
    }
}
