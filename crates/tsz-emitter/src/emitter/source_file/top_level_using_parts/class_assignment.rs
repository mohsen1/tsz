use super::super::super::{Printer, is_valid_identifier_name};
use super::super::top_level_using_decorated::{
    export_decorate_assignment, strip_decorate_export_prefix,
};
use crate::transforms::{ClassDecoratorInfo, ClassES5Emitter};
use tsz_parser::parser::node::{ClassData, Node};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_parser::syntax::transform_utils::is_private_identifier;
use tsz_scanner::SyntaxKind;

fn top_level_using_temp_name_rank(name: &str) -> Option<u32> {
    let rest = name.strip_prefix('_')?;
    if rest.len() == 1 {
        let byte = rest.as_bytes()[0];
        if byte.is_ascii_lowercase() {
            return Some((byte - b'a') as u32);
        }
    }
    rest.parse::<u32>().ok().map(|index| index + 26)
}

impl<'a> Printer<'a> {
    fn top_level_using_export_binding_stmt(&self, export_name: &str, local_name: &str) -> String {
        if self.in_system_execute_body {
            format!("exports_1(\"{export_name}\", {local_name});")
        } else if is_valid_identifier_name(export_name) {
            format!("exports.{export_name} = {local_name};")
        } else {
            format!("exports[\"{export_name}\"] = {local_name};")
        }
    }

    fn top_level_using_export_binding_prefix(&self, export_name: &str) -> String {
        if self.in_system_execute_body {
            format!("exports_1(\"{export_name}\", ")
        } else if is_valid_identifier_name(export_name) {
            format!("exports.{export_name} = ")
        } else {
            format!("exports[\"{export_name}\"] = ")
        }
    }

    const fn top_level_using_export_binding_suffix(&self) -> &'static str {
        if self.in_system_execute_body {
            ");"
        } else {
            ";"
        }
    }

    fn top_level_using_class_assignment_text(
        emitted: &str,
        binding_name: &str,
        class_has_name: bool,
    ) -> String {
        if let Some(rewritten) = Self::splice_top_level_using_assignment_head(
            emitted,
            &format!("let {binding_name} = "),
            &format!("{binding_name} = "),
        ) {
            return rewritten;
        }
        if let Some(rewritten) = Self::splice_top_level_using_assignment_head(
            emitted,
            &format!("var {binding_name} = "),
            &format!("{binding_name} = "),
        ) {
            return rewritten;
        }

        let class_head = format!("class {binding_name}");
        let assignment_head = if class_has_name {
            format!("{binding_name} = class {binding_name}")
        } else {
            format!("{binding_name} = class")
        };
        Self::splice_top_level_using_assignment_head(emitted, &class_head, &assignment_head)
            .unwrap_or_else(|| emitted.to_string())
    }

    fn splice_top_level_using_assignment_head(
        emitted: &str,
        needle: &str,
        replacement: &str,
    ) -> Option<String> {
        let start = emitted.find(needle)?;
        let mut rewritten =
            String::with_capacity(emitted.len() + replacement.len().saturating_sub(needle.len()));
        rewritten.push_str(&emitted[..start]);
        rewritten.push_str(replacement);
        rewritten.push_str(&emitted[start + needle.len()..]);
        Some(rewritten)
    }

    fn top_level_using_assignment_rhs<'b>(emitted: &'b str, binding_name: &str) -> Option<&'b str> {
        Some(
            emitted
                .strip_prefix(binding_name)?
                .trim_start()
                .strip_prefix('=')?
                .trim_start(),
        )
    }

    fn mark_top_level_using_inline_cjs_export(
        &mut self,
        export_name: Option<&String>,
        is_es_module_output: bool,
    ) {
        if let Some(export_name) = export_name
            && !is_es_module_output
        {
            self.ctx
                .module_state
                .inline_exported_names
                .insert(export_name.clone());
        }
    }

    fn rewrite_direct_top_level_using_class_export(
        &self,
        mut emitted: String,
        binding_name: &str,
        export_name: &str,
        is_legacy_decorator_class: bool,
    ) -> String {
        let current_indent = "    ".repeat(self.writer.indent_level() as usize);
        if let Some(stripped) = emitted.strip_prefix(&current_indent) {
            emitted = stripped.to_string();
        }

        if is_legacy_decorator_class && !self.in_top_level_using_scope {
            let export_stmt = self.top_level_using_export_binding_stmt(export_name, binding_name);
            if self.ctx.target_es5 {
                if !emitted.ends_with('\n') {
                    emitted.push('\n');
                }
                emitted.push_str(&export_stmt);
            } else {
                let export_prefix = self.top_level_using_export_binding_prefix(export_name);
                if let Some(first_stmt_end) = emitted.find(';') {
                    emitted.insert_str(first_stmt_end + 1, &format!("\n{export_stmt}"));
                }
                emitted = export_decorate_assignment(
                    emitted,
                    &export_prefix,
                    binding_name,
                    self.in_system_execute_body,
                )
                .0;
            }
            return emitted;
        }

        let export_stmt = self.top_level_using_export_binding_stmt(export_name, binding_name);
        emitted = emitted
            .lines()
            .filter(|line| line.trim() != export_stmt)
            .collect::<Vec<_>>()
            .join("\n");

        let export_prefix = self.top_level_using_export_binding_prefix(export_name);
        let export_suffix = self.top_level_using_export_binding_suffix();

        if is_legacy_decorator_class && self.ctx.target_es5 && self.in_top_level_using_scope {
            emitted = strip_decorate_export_prefix(&emitted, &export_prefix, binding_name);
        }

        if is_legacy_decorator_class
            && !self.ctx.target_es5
            && self.in_top_level_using_scope
            && let Some(first_stmt_end) = emitted.find(';')
        {
            let first_stmt = emitted[..first_stmt_end].trim_start();
            let mut remainder = emitted[first_stmt_end + 1..]
                .trim_start_matches(['\n', '\r'])
                .to_string();
            remainder = export_decorate_assignment(
                remainder,
                &export_prefix,
                binding_name,
                self.in_system_execute_body,
            )
            .0;
            let mut rewritten = format!("{export_prefix}{first_stmt}{export_suffix}");
            if !remainder.trim().is_empty() {
                rewritten.push('\n');
                rewritten.push_str(&remainder);
            }
            return rewritten;
        }

        let trimmed = emitted.trim_end();
        let trimmed = trimmed.strip_suffix(';').unwrap_or(trimmed);
        format!("{export_prefix}{trimmed}{export_suffix}")
    }

    pub(in crate::emitter) fn rewrite_legacy_top_level_using_class_export(
        &self,
        mut emitted: String,
        binding_name: &str,
        export_name: &str,
        is_es_module_output: bool,
    ) -> String {
        let leading_indent = Some("    ".repeat(self.writer.indent_level() as usize));
        if let Some(indent) = leading_indent.as_ref()
            && let Some(stripped) = emitted.strip_prefix(indent)
        {
            emitted = stripped.to_string();
        }
        // Default-exported decorated classes emitted *inside* a System
        // top-level `using` block are threaded through a `_default`
        // tracker variable that lives at the System closure scope. This
        // applies to BOTH named classes (`@dec export default class C
        // { }` → `exports_1("default", _default = C);`) and anonymous
        // classes (`@dec export default class { }` →
        // `exports_1("default", _default = default_1);`). Native
        // `using` (ES2025+) skips the tracker since the export sits at
        // module top level rather than inside a try/catch.
        let local_expr = if export_name == "default"
            && self.in_top_level_using_scope
            && !self.ctx.options.target.supports_es2025()
        {
            format!("_default = {binding_name}")
        } else {
            binding_name.to_string()
        };

        let plain_export_stmt = self.top_level_using_export_binding_stmt(export_name, binding_name);
        let local_export_stmt = self.top_level_using_export_binding_stmt(export_name, &local_expr);
        emitted = emitted
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                trimmed != plain_export_stmt && trimmed != local_export_stmt
            })
            .collect::<Vec<_>>()
            .join("\n");

        let export_stmt = if is_es_module_output {
            format!("{local_expr};")
        } else if let Some(indent) = leading_indent.as_ref() {
            format!(
                "{indent}{}",
                self.top_level_using_export_binding_stmt(export_name, &local_expr)
            )
        } else {
            self.top_level_using_export_binding_stmt(export_name, &local_expr)
        };

        if export_name == "default" {
            if is_es_module_output && !self.in_top_level_using_scope {
                return emitted;
            }
            if !emitted.ends_with('\n') {
                emitted.push('\n');
            }
            emitted.push_str(&export_stmt);
            return emitted;
        }

        let export_prefix = self.top_level_using_export_binding_prefix(export_name);
        let export_suffix = self.top_level_using_export_binding_suffix();
        if self.in_top_level_using_scope && self.ctx.target_es5 {
            emitted = strip_decorate_export_prefix(&emitted, &export_prefix, binding_name);
            let trimmed = emitted.trim_end();
            let trimmed = trimmed.strip_suffix(';').unwrap_or(trimmed);
            return format!("{export_prefix}{trimmed}{export_suffix}");
        }

        if let Some(first_stmt_end) = emitted.find(';')
            && (!self.in_system_execute_body || !self.ctx.target_es5)
        {
            if self.in_top_level_using_scope && !self.ctx.target_es5 {
                let first_stmt = emitted[..first_stmt_end].trim_start();
                let mut remainder = emitted[first_stmt_end + 1..]
                    .trim_start_matches(['\n', '\r'])
                    .to_string();
                remainder = export_decorate_assignment(
                    remainder,
                    &export_prefix,
                    binding_name,
                    self.in_system_execute_body,
                )
                .0;
                let mut rewritten = format!("{export_prefix}{first_stmt}{export_suffix}");
                if !remainder.trim().is_empty() {
                    rewritten.push('\n');
                    rewritten.push_str(&remainder);
                }
                return rewritten;
            }
            emitted.insert_str(first_stmt_end + 1, &format!("\n{export_stmt}"));
        }

        if self.in_system_execute_body && self.ctx.target_es5 {
            if !emitted.ends_with('\n') {
                emitted.push('\n');
            }
            emitted.push_str(&export_stmt);
            return emitted;
        }

        let (emitted_after_decorate_export, replaced_decorate_assignment) =
            export_decorate_assignment(
                emitted,
                &export_prefix,
                binding_name,
                self.in_system_execute_body,
            );
        emitted = emitted_after_decorate_export;

        if self.in_system_execute_body {
            if !replaced_decorate_assignment {
                if !emitted.ends_with('\n') {
                    emitted.push('\n');
                }
                emitted.push_str(&export_stmt);
            }
            return emitted;
        }

        emitted
    }

    fn top_level_using_es5_class_has_deferred_static_blocks(&self, class: &ClassData) -> bool {
        let mut has_static_block = false;
        let mut has_non_block_static_member = false;

        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                has_static_block = true;
                continue;
            }

            if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                if let Some(prop_data) = self.arena.get_property_decl(member_node) {
                    has_non_block_static_member |= self
                        .arena
                        .has_modifier(&prop_data.modifiers, SyntaxKind::StaticKeyword)
                        && !self
                            .arena
                            .has_modifier(&prop_data.modifiers, SyntaxKind::AbstractKeyword)
                        && !self
                            .arena
                            .has_modifier(&prop_data.modifiers, SyntaxKind::DeclareKeyword)
                        && !is_private_identifier(self.arena, prop_data.name)
                        && !self
                            .arena
                            .has_modifier(&prop_data.modifiers, SyntaxKind::AccessorKeyword)
                        && prop_data.initializer.is_some();
                }
            } else if (member_node.kind == syntax_kind_ext::GET_ACCESSOR
                || member_node.kind == syntax_kind_ext::SET_ACCESSOR)
                && let Some(acc_data) = self.arena.get_accessor(member_node)
            {
                has_non_block_static_member |= self
                    .arena
                    .has_modifier(&acc_data.modifiers, SyntaxKind::StaticKeyword)
                    && !(self
                        .arena
                        .has_modifier(&acc_data.modifiers, SyntaxKind::AbstractKeyword)
                        && acc_data.body.is_none())
                    && !is_private_identifier(self.arena, acc_data.name);
            }
        }

        has_static_block && !has_non_block_static_member
    }

    pub(in crate::emitter) fn reserve_top_level_using_deferred_static_class_result_temps(
        &mut self,
        statements: &NodeList,
        start_idx: usize,
    ) -> Vec<String> {
        self.reserved_top_level_using_class_result_temps.clear();
        if !self.ctx.target_es5 {
            return Vec::new();
        }

        let mut temps = Vec::new();
        let mut class_exprs = Vec::new();
        for &stmt_idx in &statements.nodes[start_idx..] {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let class_idx = if stmt_node.kind == syntax_kind_ext::CLASS_DECLARATION {
                Some(stmt_idx)
            } else if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                self.arena.get_export_decl(stmt_node).and_then(|export| {
                    self.arena.get(export.export_clause).and_then(|clause| {
                        (clause.kind == syntax_kind_ext::CLASS_DECLARATION)
                            .then_some(export.export_clause)
                    })
                })
            } else {
                None
            };
            if let Some(class_idx) = class_idx {
                let Some(class_node) = self.arena.get(class_idx) else {
                    continue;
                };
                let Some(class) = self.arena.get_class(class_node) else {
                    continue;
                };
                if !self.top_level_using_es5_class_has_deferred_static_blocks(class) {
                    continue;
                }

                let temp = self.make_unique_name_fresh();
                self.reserved_top_level_using_class_result_temps
                    .insert(class_idx, temp.clone());
                self.hoisted_deferred_static_class_result_temps
                    .push(temp.clone());
                temps.push(temp);
                continue;
            }
            if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                continue;
            }
            let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
                continue;
            };
            for &decl_list_idx in &var_stmt.declarations.nodes {
                let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                    continue;
                };
                let flags = decl_list_node.flags as u32;
                if (flags & tsz_parser::parser::node_flags::USING) == 0
                    && !tsz_parser::parser::node_flags::is_await_using(flags)
                {
                    continue;
                }
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
                    let Some(init_node) = self.arena.get(decl.initializer) else {
                        continue;
                    };
                    if init_node.kind != syntax_kind_ext::CLASS_EXPRESSION {
                        continue;
                    }
                    let Some(class) = self.arena.get_class(init_node) else {
                        continue;
                    };
                    if self.class_has_static_computed_method_or_accessor_comma_expr(
                        class,
                        decl.initializer,
                        true,
                    ) || self.top_level_using_es5_class_has_deferred_static_blocks(class)
                    {
                        class_exprs.push(decl.initializer);
                    }
                }
            }
        }

        let mut class_expr_temps: Vec<String> = class_exprs
            .iter()
            .filter_map(|idx| self.file_level_class_temp_reservations.get(idx))
            .filter_map(|names| names.front().cloned())
            .collect();
        class_expr_temps
            .sort_by_key(|name| top_level_using_temp_name_rank(name).unwrap_or(u32::MAX));
        for (class_idx, temp) in class_exprs
            .into_iter()
            .zip(class_expr_temps.iter().cloned())
        {
            if let Some(names) = self.file_level_class_temp_reservations.get_mut(&class_idx)
                && let Some(slot) = names.front_mut()
            {
                *slot = temp.clone();
            }
        }
        temps.extend(class_expr_temps);
        temps
    }

    fn make_top_level_using_deferred_static_class_result_temp(&mut self) -> String {
        let temp = self.make_unique_name();
        self.hoisted_deferred_static_class_result_temps
            .push(temp.clone());
        temp
    }

    pub(in crate::emitter) fn emit_top_level_using_class_assignment(
        &mut self,
        node: &Node,
        idx: NodeIndex,
        export_name: Option<String>,
        rewrite_as_direct_export: bool,
        is_es_module_output: bool,
    ) -> bool {
        let Some(class) = self.arena.get_class(node) else {
            return false;
        };
        let binding_name = self.get_identifier_text_opt(class.name).or_else(|| {
            if export_name.as_deref() == Some("default") {
                Some(
                    self.anonymous_default_export_name
                        .clone()
                        .unwrap_or_else(|| "default_1".to_string()),
                )
            } else {
                None
            }
        });
        let Some(binding_name) = binding_name else {
            return false;
        };
        let has_explicit_export_modifier = self
            .arena
            .has_modifier(&class.modifiers, SyntaxKind::ExportKeyword)
            || self
                .arena
                .has_modifier(&class.modifiers, SyntaxKind::DefaultKeyword);
        let synth_default_name = class.name.is_none() && export_name.as_deref() == Some("default");
        let prev_anon_default_name = if synth_default_name {
            let prev = self.anonymous_default_export_name.clone();
            self.anonymous_default_export_name = Some(binding_name.clone());
            Some(prev)
        } else {
            None
        };
        let has_decorators = !self.collect_class_decorators(&class.modifiers).is_empty();
        let display_name = if export_name.as_deref() == Some("default") && class.name.is_none() {
            "default".to_string()
        } else {
            binding_name.clone()
        };
        if self.ctx.options.legacy_decorators
            && self.ctx.target_es5
            && has_decorators
            && export_name.as_deref() == Some("default")
            && class.name.is_none()
        {
            let mut es5_emitter = ClassES5Emitter::new(self.arena);
            es5_emitter.set_temp_var_counter(self.ctx.destructuring_state.temp_var_counter);
            es5_emitter.set_async_generator_inner_name_counts(
                self.async_generator_inner_name_counts.clone(),
            );
            self.configure_es5_class_emitter_disposable_context(&mut es5_emitter);
            es5_emitter.set_indent_level(self.writer.indent_level());
            es5_emitter.set_transforms(self.transforms.clone());
            es5_emitter.set_remove_comments(self.ctx.options.remove_comments);
            es5_emitter.set_printer_options(self.ctx.options.clone());
            es5_emitter.set_module_kind(self.ctx.outer_module_kind());
            if let Some(text) = self.source_text_for_map() {
                es5_emitter.set_source_text(text);
            }
            es5_emitter
                .set_use_define_for_class_fields(self.ctx.options.use_define_for_class_fields);
            es5_emitter.set_decorator_info(ClassDecoratorInfo {
                class_decorators: self.collect_class_decorators(&class.modifiers),
                has_member_decorators: false,
                emit_decorator_metadata: self.ctx.options.emit_decorator_metadata,
            });
            let mut output = es5_emitter.emit_class_assignment_with_name(idx, &binding_name);
            self.sync_es5_class_emitter_state(&mut es5_emitter);
            if self.in_system_execute_body {
                let leading_indent = "    ".repeat(self.writer.indent_level() as usize);
                if let Some(stripped) = output.strip_prefix(&leading_indent) {
                    output = stripped.to_string();
                }
            }
            self.write(&output);
            if !self.writer.is_at_line_start() {
                self.write_line();
            }
            if !is_es_module_output {
                self.write_export_binding_start("default");
            }
            // Inside a top-level System using-block, anonymous default
            // classes thread through a `_default` tracker variable so
            // the export call mirrors tsc's
            // `exports_1("default", _default = default_1);` shape. Outside
            // a using-block (or for non-anonymous classes) the binding
            // name is the live binding and no tracker is needed.
            if self.in_top_level_using_scope {
                self.write("_default = ");
            }
            self.write(&binding_name);
            if !is_es_module_output {
                self.write_export_binding_end();
            } else {
                self.write(";");
            }
            return true;
        }
        if !self.ctx.target_es5
            && has_decorators
            && !self.ctx.options.legacy_decorators
            && !self.ctx.options.target.supports_es2025()
        {
            if self.in_system_top_level_using_prelude
                && let Some(export_name) = export_name.as_ref()
                && let Some(expr) = self.capture_tc39_decorated_class_expression(idx, &display_name)
            {
                if export_name == "default" && class.name.is_none() {
                    self.write_export_binding_start(export_name);
                    self.write(&expr);
                    self.write_export_binding_end();
                } else {
                    self.write(&binding_name);
                    self.write(" = ");
                    self.write(&expr);
                    self.write(";");
                    self.write_line();
                    self.write_export_binding_start(export_name);
                    self.write(&binding_name);
                    self.write_export_binding_end();
                }
                self.mark_top_level_using_inline_cjs_export(Some(export_name), is_es_module_output);
                if let Some(prev) = prev_anon_default_name {
                    self.anonymous_default_export_name = prev;
                }
                return true;
            }
            if let Some(expr) = self.capture_tc39_decorated_class_expression(idx, &display_name) {
                if let Some(export_name) = export_name.as_ref() {
                    if !is_es_module_output {
                        self.write_export_binding_start(export_name);
                    }
                    if export_name == "default" {
                        self.write("_default = ");
                        if class.name.is_some() {
                            self.write(&binding_name);
                            self.write(" = ");
                        }
                    } else {
                        self.write(&binding_name);
                        self.write(" = ");
                    }
                    self.write(&expr);
                    if !is_es_module_output {
                        self.write_export_binding_end();
                    } else {
                        self.write(";");
                    }
                } else {
                    self.write(&binding_name);
                    self.write(" = ");
                    self.write(&expr);
                    self.write(";");
                }
                self.mark_top_level_using_inline_cjs_export(
                    export_name.as_ref(),
                    is_es_module_output,
                );
                if let Some(prev) = prev_anon_default_name {
                    self.anonymous_default_export_name = prev;
                }
                return true;
            }
        }
        if self.ctx.options.target.supports_es2025()
            && has_decorators
            && !self.ctx.options.legacy_decorators
            && self.in_system_execute_body
        {
            let before_len = self.writer.len();
            self.emit_class_es6_with_options(
                node,
                idx,
                false,
                Some(("", binding_name.clone())),
                None,
                None,
                false,
            );
            let after_len = self.writer.len();
            let full_output = self.writer.get_output().to_string();
            let emitted = &full_output[before_len..after_len];
            let assign_prefix = format!("{binding_name} = ");
            let rewritten = if let Some(assign_idx) = emitted.find(&assign_prefix) {
                let leading_modifiers = emitted[..assign_idx].trim_end_matches('\n');
                let class_text = &emitted[assign_idx + assign_prefix.len()..];
                let mut rewritten = String::new();
                rewritten.push_str(&assign_prefix);
                if !leading_modifiers.is_empty() {
                    rewritten.push('\n');
                    rewritten.push_str(leading_modifiers);
                    rewritten.push('\n');
                }
                rewritten.push_str(class_text);
                rewritten
            } else {
                emitted.to_string()
            };

            self.writer.truncate(before_len);
            self.write(&rewritten);
            if !rewritten.trim_end().ends_with(';') {
                self.write(";");
            }
            if let Some(export_name) = export_name.as_ref() {
                self.write_line();
                self.write_export_binding_start(export_name);
                self.write(&binding_name);
                self.write_export_binding_end();
            }
            self.mark_top_level_using_inline_cjs_export(export_name.as_ref(), is_es_module_output);
            if let Some(prev) = prev_anon_default_name {
                self.anonymous_default_export_name = prev;
            }
            return true;
        }
        if export_name.is_none() && !self.ctx.target_es5 && !has_decorators {
            self.emit_class_es6_with_options(
                node,
                idx,
                false,
                Some(("", binding_name.clone())),
                None,
                None,
                false,
            );
            if let Some(prev) = prev_anon_default_name {
                self.anonymous_default_export_name = prev;
            }
            return true;
        }
        if let Some(export_name) = export_name.as_ref()
            && rewrite_as_direct_export
            && export_name != "default"
            && !self.ctx.target_es5
            && !has_decorators
            && !self.in_system_top_level_using_prelude
        {
            let assignment_target = format!(
                "{}{binding_name}",
                self.top_level_using_export_binding_prefix(export_name)
            );
            let assignment_suffix = self.top_level_using_export_binding_suffix();
            self.emit_class_es6_assignment_with_suffix(
                node,
                idx,
                assignment_target,
                assignment_suffix,
            );
            if let Some(prev) = prev_anon_default_name {
                self.anonymous_default_export_name = prev;
            }
            self.mark_top_level_using_inline_cjs_export(Some(export_name), is_es_module_output);
            return true;
        }
        if self.in_system_execute_body
            && self.ctx.target_es5
            && !has_decorators
            && let Some(export_name) = export_name.as_ref()
            && export_name != "default"
        {
            let mut es5_emitter = ClassES5Emitter::new(self.arena);
            es5_emitter.set_temp_var_counter(self.ctx.destructuring_state.temp_var_counter);
            es5_emitter.set_async_generator_inner_name_counts(
                self.async_generator_inner_name_counts.clone(),
            );
            self.configure_es5_class_emitter_disposable_context(&mut es5_emitter);
            es5_emitter.set_indent_level(self.writer.indent_level());
            es5_emitter.set_transforms(self.transforms.clone());
            es5_emitter.set_remove_comments(self.ctx.options.remove_comments);
            es5_emitter.set_printer_options(self.ctx.options.clone());
            es5_emitter.set_module_kind(self.ctx.outer_module_kind());
            if let Some(text) = self.source_text_for_map() {
                es5_emitter.set_source_text(text);
            }
            es5_emitter
                .set_use_define_for_class_fields(self.ctx.options.use_define_for_class_fields);

            let (mut assignment, static_blocks) =
                es5_emitter.emit_class_assignment_split_statics(idx, &binding_name);
            self.sync_es5_class_emitter_state(&mut es5_emitter);

            if !assignment.is_empty() {
                let leading_indent = "    ".repeat(self.writer.indent_level() as usize);
                if let Some(stripped) = assignment.strip_prefix(&leading_indent) {
                    assignment = stripped.to_string();
                }
                self.write(&assignment);
                if !assignment.trim_end().ends_with(';') {
                    self.write(";");
                }
                if !self.writer.is_at_line_start() {
                    self.write_line();
                }
                self.write_export_binding_start(export_name);
                self.write(&binding_name);
                self.write_export_binding_end();

                for mut static_block in static_blocks {
                    if let Some(stripped) = static_block.strip_prefix(&leading_indent) {
                        static_block = stripped.to_string();
                    }
                    self.write_line();
                    self.write(&static_block);
                    if !static_block.trim_end().ends_with(';') {
                        self.write(";");
                    }
                }
                self.mark_top_level_using_inline_cjs_export(Some(export_name), is_es_module_output);
                if let Some(prev) = prev_anon_default_name {
                    self.anonymous_default_export_name = prev;
                }
                return true;
            }
        }
        if export_name.is_none()
            && self.ctx.target_es5
            && !has_decorators
            && self.top_level_using_es5_class_has_deferred_static_blocks(class)
        {
            let result_temp = self
                .reserved_top_level_using_class_result_temps
                .get(&idx)
                .cloned()
                .unwrap_or_else(|| self.make_top_level_using_deferred_static_class_result_temp());
            let mut es5_emitter = ClassES5Emitter::new(self.arena);
            es5_emitter.set_temp_var_counter(self.ctx.destructuring_state.temp_var_counter);
            es5_emitter.set_async_generator_inner_name_counts(
                self.async_generator_inner_name_counts.clone(),
            );
            self.configure_es5_class_emitter_disposable_context(&mut es5_emitter);
            es5_emitter.set_indent_level(self.writer.indent_level());
            es5_emitter.set_transforms(self.transforms.clone());
            es5_emitter.set_remove_comments(self.ctx.options.remove_comments);
            es5_emitter.set_printer_options(self.ctx.options.clone());
            es5_emitter.set_module_kind(self.ctx.outer_module_kind());
            if let Some(text) = self.source_text_for_map() {
                es5_emitter.set_source_text(text);
            }
            es5_emitter
                .set_use_define_for_class_fields(self.ctx.options.use_define_for_class_fields);

            if let Some(mut output) = es5_emitter.emit_class_assignment_with_deferred_static_result(
                idx,
                &binding_name,
                &result_temp,
            ) {
                self.sync_es5_class_emitter_state(&mut es5_emitter);
                let leading_indent = "    ".repeat(self.writer.indent_level() as usize);
                if let Some(stripped) = output.strip_prefix(&leading_indent) {
                    output = stripped.to_string();
                }
                self.write(&output);
                if !output.trim_end().ends_with(';') {
                    self.write(";");
                }
                if let Some(prev) = prev_anon_default_name {
                    self.anonymous_default_export_name = prev;
                }
                self.mark_top_level_using_inline_cjs_export(None, is_es_module_output);
                return true;
            }
            self.sync_es5_class_emitter_state(&mut es5_emitter);
        }
        if export_name.is_none() && self.ctx.target_es5 && !has_decorators {
            let mut es5_emitter = ClassES5Emitter::new(self.arena);
            es5_emitter.set_temp_var_counter(self.ctx.destructuring_state.temp_var_counter);
            es5_emitter.set_async_generator_inner_name_counts(
                self.async_generator_inner_name_counts.clone(),
            );
            self.configure_es5_class_emitter_disposable_context(&mut es5_emitter);
            es5_emitter.set_indent_level(self.writer.indent_level());
            es5_emitter.set_transforms(self.transforms.clone());
            es5_emitter.set_remove_comments(self.ctx.options.remove_comments);
            es5_emitter.set_printer_options(self.ctx.options.clone());
            es5_emitter.set_module_kind(self.ctx.outer_module_kind());
            if let Some(text) = self.source_text_for_map() {
                es5_emitter.set_source_text(text);
            }
            es5_emitter
                .set_use_define_for_class_fields(self.ctx.options.use_define_for_class_fields);

            let mut output = es5_emitter.emit_class_assignment_with_name(idx, &binding_name);
            self.sync_es5_class_emitter_state(&mut es5_emitter);
            if !output.is_empty() {
                let leading_indent = "    ".repeat(self.writer.indent_level() as usize);
                if let Some(stripped) = output.strip_prefix(&leading_indent) {
                    output = stripped.to_string();
                }
                self.write(&output);
                if !output.trim_end().ends_with(';') {
                    self.write(";");
                }
                if let Some(prev) = prev_anon_default_name {
                    self.anonymous_default_export_name = prev;
                }
                self.mark_top_level_using_inline_cjs_export(None, is_es_module_output);
                return true;
            }
        }
        let use_default_tc39_display_name = self.in_system_execute_body
            && export_name.as_deref() == Some("default")
            && !self.ctx.options.target.supports_es2025()
            && class.name.is_none()
            && has_decorators
            && !self.ctx.options.legacy_decorators;
        let prev_pending_tc39_name = if use_default_tc39_display_name {
            self.pending_tc39_class_expression_name
                .replace(("default".to_string(), false))
        } else {
            None
        };

        let before_len = self.writer.len();
        self.emit(idx);
        let after_len = self.writer.len();
        if use_default_tc39_display_name {
            self.pending_tc39_class_expression_name = prev_pending_tc39_name;
        }
        if let Some(prev) = prev_anon_default_name {
            self.anonymous_default_export_name = prev;
        }
        let full_output = self.writer.get_output().to_string();
        let emitted = &full_output[before_len..after_len];

        let mut rewritten = Self::top_level_using_class_assignment_text(
            emitted,
            &binding_name,
            class.name.is_some(),
        );

        self.writer.truncate(before_len);
        if let Some(export_name) = export_name.as_ref() {
            if rewrite_as_direct_export
                && export_name != "default"
                && !self.in_system_top_level_using_prelude
                && !(self.in_system_execute_body
                    && self.ctx.options.target.supports_es2025()
                    && self.ctx.options.legacy_decorators
                    && has_decorators)
            {
                self.write(&self.rewrite_direct_top_level_using_class_export(
                    rewritten,
                    &binding_name,
                    export_name,
                    self.ctx.options.legacy_decorators && has_decorators,
                ));
            } else if self.ctx.options.legacy_decorators && has_decorators {
                self.write(&self.rewrite_legacy_top_level_using_class_export(
                    rewritten,
                    &binding_name,
                    export_name,
                    is_es_module_output,
                ));
            } else if let Some(mut rewritten) = self
                .render_simple_tc39_decorated_class_es5_assignment(
                    node,
                    idx,
                    &binding_name,
                    &display_name,
                )
            {
                if self.in_system_execute_body {
                    let leading_indent = "    ".repeat(self.writer.indent_level() as usize);
                    if let Some(stripped) = rewritten.strip_prefix(&leading_indent) {
                        rewritten = stripped.to_string();
                    }
                }
                if self.in_system_top_level_using_prelude {
                    self.write(&rewritten);
                    if !rewritten.trim_end().ends_with(';') {
                        self.write(";");
                    }
                    self.write_line();
                    self.write_export_binding_start(export_name);
                    self.write(&binding_name);
                    self.write_export_binding_end();
                } else if self.in_system_execute_body
                    && self.in_top_level_using_scope
                    && self.ctx.target_es5
                    && export_name != "default"
                    && !has_explicit_export_modifier
                {
                    let trimmed = rewritten.strip_suffix(';').unwrap_or(&rewritten);
                    self.write_export_binding_start(export_name);
                    self.write(trimmed);
                    self.write_export_binding_end();
                } else if self.in_top_level_using_scope
                    && export_name == "default"
                    && !self.ctx.options.target.supports_es2025()
                {
                    self.write(&rewritten);
                    if !rewritten.trim_end().ends_with(';') {
                        self.write(";");
                    }
                    self.write_line();
                    if !is_es_module_output {
                        self.write_export_binding_start(export_name);
                    }
                    if class.name.is_some() || !self.ctx.options.legacy_decorators {
                        self.write("_default = ");
                    }
                    self.write(&binding_name);
                    if !is_es_module_output {
                        self.write_export_binding_end();
                    } else {
                        self.write(";");
                    }
                } else if self.in_system_execute_body
                    && (has_explicit_export_modifier || export_name == "default")
                {
                    let trimmed = rewritten.strip_suffix(';').unwrap_or(&rewritten);
                    self.write_export_binding_start(export_name);
                    if export_name == "default" && class.name.is_some() {
                        self.write("_default = ");
                    }
                    self.write(trimmed);
                    self.write_export_binding_end();
                } else if self.in_system_execute_body {
                    self.write(&rewritten);
                    if !rewritten.trim_end().ends_with(';') {
                        self.write(";");
                    }
                    self.write_line();
                    self.write_export_binding_start(export_name);
                    self.write(&binding_name);
                    self.write_export_binding_end();
                } else {
                    self.write_export_binding_start(export_name);
                    self.write(&rewritten);
                }
            } else if self.in_system_execute_body
                && export_name == "default"
                && !self.ctx.options.target.supports_es2025()
                && class.name.is_none()
            {
                let trimmed = rewritten
                    .strip_suffix(';')
                    .unwrap_or(&rewritten)
                    .trim_start();
                let inline_expr =
                    Self::top_level_using_assignment_rhs(trimmed, &binding_name).unwrap_or(trimmed);
                self.write_export_binding_start(export_name);
                if self.in_top_level_using_scope {
                    self.write("_default = ");
                }
                self.write(inline_expr);
                self.write_export_binding_end();
            } else if self.in_system_execute_body
                && (has_explicit_export_modifier
                    || (!self.ctx.options.target.supports_es2025() && export_name == "default"))
            {
                let trimmed = rewritten.strip_suffix(';').unwrap_or(&rewritten);
                self.write_export_binding_start(export_name);
                if export_name == "default"
                    && !self.ctx.options.target.supports_es2025()
                    && class.name.is_some()
                {
                    self.write("_default = ");
                }
                self.write(trimmed);
                self.write_export_binding_end();
            } else if self.in_system_execute_body {
                self.write(&rewritten);
                if !rewritten.trim_end().ends_with(';') {
                    self.write(";");
                }
                self.write_line();
                self.write_export_binding_start(export_name);
                self.write(&binding_name);
                self.write_export_binding_end();
            } else {
                self.write_export_binding_start(export_name);
                self.write(&rewritten);
            }
        } else {
            let leading_indent = "    ".repeat(self.writer.indent_level() as usize);
            if let Some(stripped) = rewritten.strip_prefix(&leading_indent) {
                rewritten = stripped.to_string();
            }
            self.write(&rewritten);
            if !rewritten.trim_end().ends_with(';') {
                self.write(";");
            }
        }
        self.mark_top_level_using_inline_cjs_export(export_name.as_ref(), is_es_module_output);
        true
    }

    pub(in crate::emitter) fn emit_top_level_using_function_assignment(
        &mut self,
        node: &Node,
        idx: NodeIndex,
        export_name: Option<String>,
    ) -> bool {
        let Some(func) = self.arena.get_function(node) else {
            return false;
        };
        let Some(name) = self.get_identifier_text_opt(func.name) else {
            return false;
        };
        if let Some(export_name) = export_name.as_ref() {
            self.write_export_binding_start(export_name);
        }
        self.write(&name);
        self.write(" = ");
        self.emit_function_expression(node, idx);
        if export_name.is_some() {
            self.write_export_binding_end();
        } else {
            self.write(";");
        }
        true
    }
}
