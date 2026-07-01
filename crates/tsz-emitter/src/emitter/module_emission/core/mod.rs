mod anonymous_default;
mod export_default_parens;
mod module_analysis;

use super::super::{ModuleKind, Printer, ScriptTarget};
use crate::context::transform::IdentifierId;
use crate::transforms::private_fields_es5::is_private_identifier;
use crate::transforms::{ClassDecoratorInfo, ClassES5Emitter};
use tsz_parser::parser::node::Node;
use tsz_parser::parser::node_flags;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

#[derive(Default)]
pub(in crate::emitter) struct CjsExportVariableSchedule {
    pub local_groups: Vec<CjsExportLocalDeclGroup>,
    pub assignments: Vec<CjsExportAssignment>,
}

pub(in crate::emitter) struct CjsExportLocalDeclGroup {
    pub keyword: &'static str,
    pub declarations: Vec<NodeIndex>,
}

pub(in crate::emitter) struct CjsExportAssignment {
    pub decoded_name: String,
    pub emit_name: String,
    pub value: CjsExportAssignmentValue,
}

pub(in crate::emitter) enum CjsExportAssignmentValue {
    Initializer(NodeIndex),
    LocalName(String),
}
const fn cjs_export_decl_list_keyword(node: &Node, target_es5: bool) -> Option<&'static str> {
    let flags = node.flags as u32;
    if flags & node_flags::USING != 0 {
        None
    } else if target_es5 {
        Some("var")
    } else if flags & node_flags::CONST != 0 {
        Some("const")
    } else if flags & node_flags::LET != 0 {
        Some("let")
    } else {
        Some("var")
    }
}

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
    /// Transforms .ts→.js, .tsx→.jsx/.js, .mts→.mjs, .cts→.cjs for relative paths.
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
            let ext = if self.ctx.options.jsx_preserve_explicit {
                ".jsx"
            } else {
                ".js"
            };
            return format!("{base}{ext}");
        }
        if let Some(base) = spec.strip_suffix(".mts") {
            return format!("{base}.mjs");
        }
        if let Some(base) = spec.strip_suffix(".cts") {
            return format!("{base}.cjs");
        }
        spec.to_string()
    }

    /// Rewrite a relative dynamic-import specifier for `--module none --outFile`
    /// require-lowering. tsc names concatenated bundle modules without the
    /// leading `./` in this mode.
    pub(in crate::emitter) fn rewrite_module_none_out_file_spec(&self, spec: &str) -> String {
        if !self.ctx.module_none_out_file {
            return spec.to_string();
        }
        spec.strip_prefix("./").unwrap_or(spec).to_string()
    }

    /// Emit a call expression argument that may be a module specifier string literal.
    /// If bundle/module settings require a string-literal rewrite, apply it
    /// inline. Otherwise, emit as-is.
    pub(in crate::emitter) fn emit_maybe_rewritten_module_specifier_arg(
        &mut self,
        arg_idx: NodeIndex,
    ) {
        use tsz_scanner::SyntaxKind;
        let Some(node) = self.arena.get(arg_idx) else {
            self.emit(arg_idx);
            return;
        };
        if node.kind != SyntaxKind::StringLiteral as u16
            && node.kind != SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            self.emit(arg_idx);
            return;
        }
        let text = if let Some(lit) = self.arena.get_literal(node) {
            &lit.text
        } else {
            self.emit(arg_idx);
            return;
        };
        if self.ctx.options.rewrite_relative_import_extensions {
            let rewritten = self.rewrite_module_spec(text);
            let rewritten = self.rewrite_module_none_out_file_spec(&rewritten);
            if rewritten == *text {
                self.emit(arg_idx);
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
            return;
        }
        let rewritten = self.rewrite_module_none_out_file_spec(text);
        if rewritten == *text {
            self.emit(arg_idx);
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

    /// Emit `__rewriteRelativeImportExtension(expr)` or
    /// `__rewriteRelativeImportExtension(expr, true)` when jsx=preserve.
    pub(in crate::emitter) fn emit_rewrite_helper_call(&mut self, arg_idx: NodeIndex) {
        self.write_helper("__rewriteRelativeImportExtension");
        self.write("(");
        self.emit(arg_idx);
        if self.ctx.options.jsx_preserve_explicit {
            self.write(", true");
        }
        self.write(")");
    }

    pub(in crate::emitter) fn next_commonjs_module_var(&mut self, module_spec: &str) -> String {
        let base = crate::transforms::emit_utils::sanitize_module_name(module_spec);
        loop {
            let next = self
                .ctx
                .module_state
                .module_temp_counters
                .entry(base.clone())
                .and_modify(|n| *n += 1)
                .or_insert(1);
            let candidate = format!("{base}_{next}");
            if !self.file_identifiers.contains(&candidate)
                && !self.generated_temp_names.contains(&candidate)
            {
                self.generated_temp_names.insert(candidate.clone());
                return candidate;
            }
        }
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

        let inner_emitted = self.with_cjs_export_body_mask(|this| {
            let before_len = this.writer.len();
            emit_inner(this);
            this.writer.len() > before_len
        });

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
                if self
                    .ctx
                    .module_state
                    .iife_exported_names
                    .contains(&name_str)
                    || self
                        .ctx
                        .module_state
                        .inline_exported_names
                        .contains(&name_str)
                {
                    continue;
                }
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
        // synthetic name (`default_1`, `default_2`, ...) and hoists
        // `exports.default = default_N;` BEFORE the declaration. Multiple
        // anonymous defaults are an error case (see
        // `exportDefaultInterfaceAndTwoFunctions`) but tsc still emits each
        // with its own counter rather than colliding on a single name.
        let is_function = node.kind == syntax_kind_ext::FUNCTION_DECLARATION;
        let synthetic_name = self.next_anonymous_default_export_name();
        let prev = self.anonymous_default_export_name.take();
        self.anonymous_default_export_name = Some(synthetic_name.clone());
        if is_function {
            // Function: exports.default before declaration (functions hoist)
            if !self.ctx.module_state.default_func_export_hoisted {
                self.write_export_binding_start("default");
                self.write(&synthetic_name);
                self.write_export_binding_end();
                self.write_line();
            }
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
                self.write(&synthetic_name);
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

        let temp_name = self.next_anonymous_default_export_name();
        if let Some(output) =
            self.render_simple_tc39_decorated_class_es5(node, class_node, &temp_name, "default")
        {
            self.write(&output);
            self.write_line();
            self.write_export_binding_start("default");
            self.write(&temp_name);
            self.write_export_binding_end();
            self.write_line();
            return;
        }

        let mut es5_emitter = ClassES5Emitter::new(self.arena);
        es5_emitter.set_temp_var_counter(self.ctx.destructuring_state.temp_var_counter);
        es5_emitter
            .set_async_generator_inner_name_counts(self.async_generator_inner_name_counts.clone());
        self.configure_es5_class_emitter_disposable_context(&mut es5_emitter);
        es5_emitter.set_indent_level(self.writer.indent_level());
        // Pass transform directives to the ClassES5Emitter
        es5_emitter.set_transforms(self.transforms.clone());
        es5_emitter.set_remove_comments(self.ctx.options.remove_comments);
        es5_emitter.set_printer_options(self.ctx.options.clone());
        es5_emitter.set_module_kind(self.ctx.outer_module_kind());
        es5_emitter.set_es_module_interop(self.ctx.options.es_module_interop);
        if let Some(text) = self.source_text_for_map() {
            if self.writer.has_source_map() {
                es5_emitter.set_source_map_context(text, self.writer.current_source_index());
            } else {
                es5_emitter.set_source_text(text);
            }
        }
        if self.ctx.options.import_helpers && self.ctx.is_effectively_commonjs() {
            es5_emitter.set_tslib_prefix(true);
            es5_emitter.set_tslib_import_binding(self.commonjs_tslib_import_binding.clone());
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
        self.sync_es5_class_emitter_state(&mut es5_emitter);
        let mappings = es5_emitter.take_mappings();
        self.write_with_offset_mappings(&es5_output, &mappings);
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

            // When a default-exported class is lowered to ES5, or has static field
            // initializers that will be lowered after the class body, tsc separates
            // the export:
            //   class C { }
            //   C.s = 0;
            //   export default C;
            // This is needed because ES5 lowering emits `var C = ...`, and static
            // initializers must come after the class body but before the export
            // statement.
            let class_needs_separated_export = if !class_has_legacy_class_decorators {
                if let Some(clause_node) = self.arena.get(export.export_clause)
                    && clause_node.kind == syntax_kind_ext::CLASS_DECLARATION
                    && let Some(class) = self.arena.get_class(clause_node)
                {
                    if self.ctx.target_es5 {
                        true
                    } else {
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
                                    && self.arena.is_static(&prop.modifiers)
                                    && !crate::transforms::emit_utils::is_runtime_omitted_member(
                                        self.arena,
                                        &prop.modifiers,
                                    )
                                {
                                    true
                                } else {
                                    false
                                }
                            })
                        } else {
                            false
                        }
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
                if self.ctx.target_es5
                    && class_name == "default_1"
                    && let Some(clause_node) = self.arena.get(export.export_clause)
                    && let Some(class) = self.arena.get_class(clause_node)
                {
                    let class_decorators = self.collect_class_decorators(&class.modifiers);
                    let tc39_class_decorators =
                        !self.ctx.options.legacy_decorators && !class_decorators.is_empty();
                    if !tc39_class_decorators {
                        let mut es5_emitter = ClassES5Emitter::new(self.arena);
                        es5_emitter
                            .set_temp_var_counter(self.ctx.destructuring_state.temp_var_counter);
                        es5_emitter.set_async_generator_inner_name_counts(
                            self.async_generator_inner_name_counts.clone(),
                        );
                        self.configure_es5_class_emitter_disposable_context(&mut es5_emitter);
                        es5_emitter.set_indent_level(self.writer.indent_level());
                        es5_emitter.set_transforms(self.transforms.clone());
                        es5_emitter.set_remove_comments(self.ctx.options.remove_comments);
                        es5_emitter.set_printer_options(self.ctx.options.clone());
                        es5_emitter.set_module_kind(self.ctx.outer_module_kind());
                        es5_emitter.set_es_module_interop(self.ctx.options.es_module_interop);
                        if let Some(text) = self.source_text_for_map() {
                            if self.writer.has_source_map() {
                                es5_emitter.set_source_map_context(
                                    text,
                                    self.writer.current_source_index(),
                                );
                            } else {
                                es5_emitter.set_source_text(text);
                            }
                        }
                        es5_emitter.set_use_define_for_class_fields(
                            self.ctx.options.use_define_for_class_fields,
                        );
                        es5_emitter.set_decorator_info(ClassDecoratorInfo {
                            class_decorators,
                            has_member_decorators: false,
                            emit_decorator_metadata: self.ctx.options.emit_decorator_metadata,
                        });
                        let output =
                            es5_emitter.emit_class_with_name(export.export_clause, &class_name);
                        self.sync_es5_class_emitter_state(&mut es5_emitter);
                        self.write(&output);
                        if !self.writer.is_at_line_start() {
                            self.write_line();
                        }
                        self.write("export default ");
                        self.write(&class_name);
                        self.write(";");
                        return;
                    }
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
                        if let Some(class_name) = self.get_identifier_text_opt(class.name) {
                            if self.ctx.target_es5 {
                                if let Some(output) = self.render_simple_tc39_decorated_class_es5(
                                    cn,
                                    export.export_clause,
                                    &class_name,
                                    &class_name,
                                ) {
                                    self.write(&output);
                                    if !self.writer.is_at_line_start() {
                                        self.write_line();
                                    }
                                    self.write("export default ");
                                    self.write(&class_name);
                                    self.write(";");
                                    return;
                                }
                            } else if let Some(expr) = self.capture_tc39_decorated_class_expression(
                                export.export_clause,
                                &class_name,
                            ) {
                                self.write("let ");
                                self.write(&class_name);
                                self.write(" = ");
                                self.write(&expr);
                                self.write(";");
                                self.write_line();
                                self.write("export default ");
                                self.write(&class_name);
                                self.write(";");
                                return;
                            }
                        }
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
                            } else if let Some(expr) = self.capture_tc39_decorated_class_expression(
                                export.export_clause,
                                "default",
                            ) {
                                self.write("export default ");
                                self.write(&expr);
                                self.write(";");
                                return;
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
                        self.emit_class_es6_with_options(
                            cn,
                            export.export_clause,
                            true,
                            None,
                            None,
                            None,
                            false,
                        );
                    }
                } else {
                    self.write("export default ");
                    // `export default (class X {} as any)` — when the source
                    // wrapped a class/function expression in parens for a
                    // type cast, tsc preserves the parens after erasure.
                    // Stripping them would silently change "default-export
                    // an expression" into "default-export a declaration".
                    let preserve_paren =
                        self.export_default_paren_protects_class_or_function(export.export_clause);
                    let prev = self.ctx.flags.paren_leftmost_function_or_object;
                    if preserve_paren {
                        self.ctx.flags.paren_leftmost_function_or_object = true;
                    }
                    self.emit(export.export_clause);
                    if preserve_paren {
                        self.ctx.flags.paren_leftmost_function_or_object = prev;
                    }
                    if !clause_is_func_or_class {
                        self.write_semicolon();
                    }
                }
            }
            return;
        }

        if export.export_clause.is_none() {
            if export.module_specifier.is_none() {
                return;
            }
            self.write("export *");
            if export.module_specifier.is_some() {
                // Preserve any comments between the `*` and `from` (e.g.
                // `export * /* star */ from "./b"`). Without this, comments
                // attached to the source range between the star token and the
                // module specifier are silently dropped.
                if let Some(mod_spec_node) = self.arena.get(export.module_specifier)
                    && let Some(text) = self.source_text
                    && let Ok(slice) = crate::safe_slice::slice(
                        text,
                        node.pos as usize,
                        mod_spec_node.pos as usize,
                    )
                    && let Some(rel) = slice.find('*')
                {
                    let after_star = node.pos + (rel as u32) + 1;
                    self.emit_comments_in_range(after_star, mod_spec_node.pos, false, false);
                }
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
            if self.ctx.options.resolved_node_module_to_esm
                && self.import_equals_declaration_is_external(clause_node)
            {
                self.emit_import_equals_declaration_inner(clause_node, true);
                self.write_semicolon();
            } else {
                // The `export` keyword sits on the outer EXPORT_DECLARATION, not
                // on the inner import-equals clause, so the clause carries no
                // export modifier. Route it through the shared exported handler
                // (as `emit_export_declaration_commonjs` already does) so the
                // assignment prefix emits the right form for the module kind
                // (`export var X = ...` for ES-module output). Pre-writing a
                // bare `export ` here would strand an `export ;` whenever the
                // inner emit elides (namespace-alias gate) or expands to its own
                // `export var`.
                self.emit_exported_import_equals_declaration(clause_node);
            }
            return;
        }

        if clause_node.kind == syntax_kind_ext::NAMED_EXPORTS
            && let Some(named_exports) = self.arena.get_named_imports(clause_node)
        {
            // For local exports (`export { x }`), use syntactic value-name
            // filtering to skip type-only specifiers (interfaces, type aliases,
            // etc.). For re-exports (`export { x } from "mod"`), only use the
            // checker-based filtering (type_only_nodes).
            let value_specs = if export.module_specifier.is_none()
                && self.recovered_module_syntax_block_depth == 0
            {
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
                self.write("export {");
                // Preserve any comments between the open `{` and the first
                // specifier (e.g. `export { /* before name */ bar }`).
                if let Some(&first_elem_idx) = value_specs.first()
                    && let Some(first_elem) = self.arena.get(first_elem_idx)
                    && let Some(text) = self.source_text
                    && let Ok(slice) = crate::safe_slice::slice(
                        text,
                        clause_node.pos as usize,
                        first_elem.pos as usize,
                    )
                    && let Some(rel) = slice.find('{')
                {
                    let after_open_brace = clause_node.pos + (rel as u32) + 1;
                    self.emit_comments_in_range(after_open_brace, first_elem.pos, false, false);
                }
                self.write(" ");
                self.emit_comma_separated(&value_specs);
                if self.has_trailing_comma_in_source(clause_node, &named_exports.elements.nodes) {
                    self.write(",");
                }
                self.write(" }");
            }
            if export.module_specifier.is_some() {
                // Preserve any comments between the export clause's closing
                // `}` and the `from` keyword (e.g.
                // `export { foo } /* after clause */ from "./b"`). The
                // NamedExports node's `.end` extends past the `from`
                // keyword in our AST, so locate the `}` directly from the
                // source text.
                if let Some(mod_spec_node) = self.arena.get(export.module_specifier)
                    && let Some(text) = self.source_text
                    && let Ok(slice) = crate::safe_slice::slice(
                        text,
                        clause_node.pos as usize,
                        mod_spec_node.pos as usize,
                    )
                    && let Some(rel) = slice.rfind('}')
                {
                    let after_close_brace = clause_node.pos + (rel as u32) + 1;
                    self.emit_comments_in_range(after_close_brace, mod_spec_node.pos, false, false);
                }
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
            // `export * as ns from "mod"` is an ES2020 feature. When the module
            // output target predates it (`module: es2015`), tsc rewrites the
            // namespace re-export into a namespace import plus a re-export:
            //   import * as ns_1 from "mod";
            //   export { ns_1 as ns };
            // For an identifier clause the generated import binding is named
            // after the export name, matching tsc's `getGeneratedNameForNode`
            // (`export * as ns` -> `ns_1`); a string-literal export name has no
            // identifier base, so a fresh temp is used.
            if self.ctx.options.module == ModuleKind::ES2015 {
                let temp_name = if clause_node.kind == SyntaxKind::Identifier as u16 {
                    let base = self.get_identifier_text_idx(export.export_clause);
                    self.make_unique_name_from_base(&base)
                } else {
                    self.make_unique_name()
                };
                self.write("import * as ");
                self.write(&temp_name);
                self.write(" from ");
                self.emit_module_specifier(export.module_specifier);
                self.emit_import_attributes(export.attributes);
                self.write_semicolon();
                self.write_line();
                self.write("export { ");
                self.write(&temp_name);
                self.write(" as ");
                self.emit(export.export_clause);
                self.write(" };");
                return;
            }
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
            if export.is_default_export {
                // `export default C;` is emitted AFTER the deferred storage init
                // (tsc's deliberate exception), matching the trailing placement.
                self.emit(export.export_clause);
                self.write_line();
                self.write("export default ");
                self.write(&name);
                self.write(";");
            } else {
                // A named `export { C };` is emitted at the class IIFE / WeakMap-init
                // boundary — the same slot the CommonJS `exports.C = C;` assignment
                // uses — so it precedes any deferred private/accessor storage init,
                // matching tsc. Stage it and let the ES5 class IIFE emitter place it;
                // fall back to the trailing form if it was not consumed.
                self.pending_esm_class_export_name = Some((export.export_clause, name.clone()));
                self.emit(export.export_clause);
                if let Some((_, pending_name)) = self.pending_esm_class_export_name.take() {
                    if !self.writer.is_at_line_start() {
                        self.write_line();
                    }
                    self.write("export { ");
                    self.write(&pending_name);
                    self.write(" };");
                }
            }
            return;
        }

        // For merged enums/namespaces/classes/functions, the second+ declaration
        // should not be prefixed with `export`. The first declaration gets
        // `export var E;` and subsequent ones are bare IIFEs. We detect this by
        // checking if the name is already in `declared_namespace_names`, which
        // means a prior declaration already emitted the `var`/`export` prefix.
        let is_merged_subsequent = self.is_merged_subsequent_declaration(clause_node);
        let es5_namespace_should_declare_var =
            if clause_node.kind == syntax_kind_ext::MODULE_DECLARATION && self.ctx.target_es5 {
                self.transforms
                    .get(export.export_clause)
                    .and_then(|directive| directive.es5_namespace_should_declare_var())
            } else {
                None
            };

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

        let clause_emits_export_prefix = clause_node.kind == syntax_kind_ext::VARIABLE_STATEMENT
            && (self.is_es5_empty_binding_pattern_export_statement(clause_node)
                || self.is_esm_object_rest_export_statement(clause_node));

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
            self.emit_class_es6_with_options(
                clause_node,
                export.export_clause,
                true,
                None,
                None,
                None,
                false,
            );
        } else {
            let namespace_iife_supplies_no_binding =
                es5_namespace_should_declare_var == Some(false);
            if !is_merged_subsequent
                && !clause_emits_export_prefix
                && !namespace_iife_supplies_no_binding
            {
                self.write("export ");
                self.emit_recovered_root_js_export_clause_modifiers(node);
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

        // When `export =` appears inside a function body or namespace IIFE
        // (syntactically invalid position), tsc emits it verbatim — no
        // module-system transformation.
        if export_assign.is_export_equals
            && (self.function_scope_depth > 0
                || self.in_namespace_iife
                || self.recovered_module_syntax_block_depth > 0)
        {
            self.write("export = ");
            self.emit_expression(export_assign.expression);
            self.write_semicolon();
            return;
        }

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
                // `export default expr` — use `exports.X` for CommonJS-exported
                // variables because no stable local binding is emitted for them.
                // Non-variable values keep their local declaration binding.
                if let Some(expr_node) = self.arena.get(export_assign.expression)
                    && expr_node.kind == SyntaxKind::Identifier as u16
                {
                    let ident = self.get_identifier_text_idx(export_assign.expression);
                    if self.commonjs_exported_var_names.contains(ident.as_str()) {
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
            // `export default (class X {} as any)` — when the source wrapped a
            // class/function expression in parens (because of a type cast),
            // tsc preserves the parens after erasure. Otherwise stripping them
            // would silently change `export default (class X {})` (expression
            // export) into `export default class X {}` (declaration export).
            let preserve_paren =
                self.export_default_paren_protects_class_or_function(export_assign.expression);
            let prev = self.ctx.flags.paren_leftmost_function_or_object;
            if preserve_paren {
                self.ctx.flags.paren_leftmost_function_or_object = true;
            }
            self.emit_expression(export_assign.expression);
            if preserve_paren {
                self.ctx.flags.paren_leftmost_function_or_object = prev;
            }
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

    /// Build a `CommonJS` export schedule for a variable statement whose declarators
    /// can be lowered structurally. Initializer-less declarations are skipped
    /// because the `CJS` preamble already emits their `exports.x = void 0` forward
    /// declaration. Destructuring and unsupported declaration-list kinds return
    /// `None` so callers can use the existing fallback paths.
    pub(in crate::emitter) fn collect_cjs_export_variable_schedule(
        &self,
        _node_idx: NodeIndex,
        node: &Node,
    ) -> Option<CjsExportVariableSchedule> {
        let var_stmt = self.arena.get_variable(node)?;
        let mut schedule = CjsExportVariableSchedule::default();

        for &decl_list_idx in &var_stmt.declarations.nodes {
            let decl_list_node = self.arena.get(decl_list_idx)?;
            let decl_list = self.arena.get_variable(decl_list_node)?;
            let keyword = cjs_export_decl_list_keyword(decl_list_node, self.ctx.target_es5)?;
            let mut local_group = CjsExportLocalDeclGroup {
                keyword,
                declarations: Vec::new(),
            };

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

                if decl.initializer.is_none() {
                    continue;
                }

                let initializer = decl.initializer;
                if self.cjs_export_initializer_needs_local_binding(initializer) {
                    local_group.declarations.push(decl_idx);
                    schedule.assignments.push(CjsExportAssignment {
                        decoded_name,
                        emit_name: emit_name.clone(),
                        value: CjsExportAssignmentValue::LocalName(emit_name),
                    });
                } else {
                    schedule.assignments.push(CjsExportAssignment {
                        decoded_name,
                        emit_name,
                        value: CjsExportAssignmentValue::Initializer(initializer),
                    });
                }
            }

            if !local_group.declarations.is_empty() {
                schedule.local_groups.push(local_group);
            }
        }

        if schedule.assignments.is_empty() {
            return None;
        }
        Some(schedule)
    }

    fn cjs_export_initializer_needs_local_binding(&self, initializer: NodeIndex) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        let k = init_node.kind;
        let is_class_initializer = self.arena.get_class(init_node).is_some();
        k == syntax_kind_ext::ARROW_FUNCTION
            || k == syntax_kind_ext::FUNCTION_EXPRESSION
            || (is_class_initializer
                && !self.class_expression_export_initializer_can_inline(initializer))
    }

    fn class_expression_export_initializer_can_inline(&self, class_idx: NodeIndex) -> bool {
        let Some(class_node) = self.arena.get(class_idx) else {
            return false;
        };
        let Some(class) = self.arena.get_class(class_node) else {
            return false;
        };
        if matches!(
            self.transforms.get(class_idx),
            Some(crate::context::transform::TransformDirective::TC39Decorators { .. })
        ) {
            return true;
        }

        let needs_private_lowering = !self.ctx.options.target.supports_es2022();
        let target_needs_field_lowering = (self.ctx.options.target as u32)
            < (ScriptTarget::ES2022 as u32)
            || !self.ctx.options.use_define_for_class_fields;
        let target_needs_static_block_lowering =
            (self.ctx.options.target as u32) < (ScriptTarget::ES2022 as u32);

        let has_private_lowering = needs_private_lowering
            && class.members.nodes.iter().any(|&member_idx| {
                self.arena
                    .get(member_idx)
                    .is_some_and(|member| match member.kind {
                        k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                            .arena
                            .get_property_decl(member)
                            .is_some_and(|prop| is_private_identifier(self.arena, prop.name)),
                        k if k == syntax_kind_ext::METHOD_DECLARATION => self
                            .arena
                            .get_method_decl(member)
                            .is_some_and(|method| is_private_identifier(self.arena, method.name)),
                        k if k == syntax_kind_ext::GET_ACCESSOR
                            || k == syntax_kind_ext::SET_ACCESSOR =>
                        {
                            self.arena.get_accessor(member).is_some_and(|accessor| {
                                is_private_identifier(self.arena, accessor.name)
                            })
                        }
                        _ => false,
                    })
            });

        let has_static_field_comma_expr = target_needs_field_lowering
            && class.members.nodes.iter().any(|&member_idx| {
                self.arena.get(member_idx).is_some_and(|member| {
                    member.kind == syntax_kind_ext::PROPERTY_DECLARATION
                        && self.arena.get_property_decl(member).is_some_and(|prop| {
                            self.arena.is_static(&prop.modifiers)
                                && !self
                                    .arena
                                    .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                                && !self
                                    .arena
                                    .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
                                && self.class_property_initializer_has_runtime_equals(member, prop)
                                && !(needs_private_lowering
                                    && is_private_identifier(self.arena, prop.name))
                        })
                })
            });

        let has_static_block_comma_expr = target_needs_static_block_lowering
            && class.members.nodes.iter().any(|&member_idx| {
                self.arena.get(member_idx).is_some_and(|member| {
                    member.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
                })
            });

        let has_static_computed_method_or_accessor =
            class.members.nodes.iter().any(|&member_idx| {
                self.arena
                    .get(member_idx)
                    .is_some_and(|member| match member.kind {
                        k if k == syntax_kind_ext::METHOD_DECLARATION => {
                            self.arena.get_method_decl(member).is_some_and(|method| {
                                self.arena.is_static(&method.modifiers)
                                    && self.arena.get(method.name).is_some_and(|name| {
                                        name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                                    })
                            })
                        }
                        k if k == syntax_kind_ext::GET_ACCESSOR
                            || k == syntax_kind_ext::SET_ACCESSOR =>
                        {
                            self.arena.get_accessor(member).is_some_and(|accessor| {
                                self.arena.is_static(&accessor.modifiers)
                                    && self.arena.get(accessor.name).is_some_and(|name| {
                                        name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                                    })
                            })
                        }
                        _ => false,
                    })
            });

        has_private_lowering
            || has_static_field_comma_expr
            || has_static_block_comma_expr
            || has_static_computed_method_or_accessor
    }

    fn class_property_initializer_has_runtime_equals(
        &self,
        member_node: &Node,
        prop: &tsz_parser::parser::node::PropertyDeclData,
    ) -> bool {
        let Some(init_node) = self.arena.get(prop.initializer) else {
            return false;
        };
        let Some(text) = self.source_text else {
            return true;
        };
        let start = member_node.pos as usize;
        let end = (init_node.pos as usize).min(text.len());
        if start >= end {
            return false;
        }
        let segment = &text.as_bytes()[start..end];
        let search_from = segment
            .iter()
            .rposition(|&byte| byte == b':')
            .map_or(0, |idx| idx + 1);
        segment[search_from..].contains(&b'=')
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
            return self.arena.identifier_text_owned(name_idx);
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
                let raw = self
                    .get_raw_string_literal(node)
                    .or_else(|| self.find_raw_string_literal_near(node, text))
                    .unwrap_or_else(|| format!("\"{text}\""));
                Some(PropertyNameEmit::Bracket(raw))
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
                    // The right side of a qualified entity name is a member of
                    // the left, not a free identifier in the enclosing scope.
                    // Suppress namespace-IIFE auto-qualification so e.g.
                    // `x.c` inside `namespace m3` does not become `x.m3.c`
                    // when `c` happens to be exported from `m3`.
                    let prev = self.suppress_ns_qualification;
                    self.suppress_ns_qualification = true;
                    self.emit_entity_name(name.right);
                    self.suppress_ns_qualification = prev;
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

    /// Write `const` for top-level module imports, or `var` for ES3/ES5.
    pub(in crate::emitter) fn write_var_or_const(&mut self) {
        if self.ctx.target_es5 {
            self.write("var ");
        } else {
            self.write("const ");
        }
    }
}

#[cfg(test)]
mod tests;
