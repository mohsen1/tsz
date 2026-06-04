use super::super::*;
use super::helpers_class_expression_static::Es5StaticClassExpressionElement;
use crate::emitter::core::PropertyNameEmit;
use crate::emitter::declarations::class::replace_identifier;
use crate::transforms::ir::IRNode;
use crate::transforms::ir_printer::IRPrinter;
use std::sync::Arc;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn es5_class_iife_expression_from_var(
        output: &str,
        class_name: &str,
    ) -> Option<String> {
        let prefix = format!("var {class_name} = ");
        let output = output.trim_end();
        let output = output.strip_suffix(';').unwrap_or(output);
        output.strip_prefix(&prefix).map(str::to_string)
    }

    pub(in crate::emitter) fn write_multiline_fragment_preserving_indent(&mut self, text: &str) {
        let mut lines = text.lines();
        if let Some(first) = lines.next() {
            self.write(first);
        }
        for line in lines {
            self.write_line();
            if !line.is_empty() {
                self.write(line);
            }
        }
    }

    fn write_multiline_fragment_with_continuation_indent(
        &mut self,
        text: &str,
        continuation_indent_level: u32,
    ) {
        let indent_unit = self.writer.indent_unit_width() as usize;
        let indent_unit = if indent_unit == 0 { 4 } else { indent_unit };

        let mut lines = text.lines();
        if let Some(first) = lines.next() {
            self.write(first);
        }
        for line in lines {
            self.write_line();
            if !line.is_empty() {
                let leading = line.len() - line.trim_start_matches(' ').len();
                let original_level = (leading / indent_unit) as u32;
                let trimmed = &line[leading..];
                let output_level = continuation_indent_level.saturating_sub(1) + original_level;
                self.write_line_with_absolute_indent(output_level, trimmed);
            }
        }
    }

    fn class_expression_static_comma_needs_parens(&self, class_node: NodeIndex) -> bool {
        let mut current = class_node;
        loop {
            let Some(ext) = self.arena.get_extended(current) else {
                return true;
            };
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return true;
            }
            let Some(parent) = self.arena.get(parent_idx) else {
                return true;
            };

            match parent.kind {
                syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    current = parent_idx;
                }
                syntax_kind_ext::RETURN_STATEMENT => return false,
                _ => return true,
            }
        }
    }

    fn current_statement_continuation_indent_level(&self) -> u32 {
        self.writer
            .indent_level()
            .max(self.writer.current_line_visual_indent_level())
            + 2
    }

    pub(in crate::emitter) fn emit_es5_static_class_expression_comma(
        &mut self,
        class_node: NodeIndex,
        class_name: &str,
        class_iife_expr: &str,
        class_value_temp: Option<&str>,
        computed_init_exprs: &[IRNode],
        static_elements: &[Es5StaticClassExpressionElement],
        set_function_name: Option<&str>,
    ) {
        let needs_parens = self.class_expression_static_comma_needs_parens(class_node);
        let temp = class_value_temp.map_or_else(
            || {
                if self.class_expression_is_in_loop_body(class_node) {
                    let temp = self.make_class_static_temp_name(class_node);
                    self.block_scoped_private_temps.push(temp.clone());
                    temp
                } else {
                    self.make_class_static_temp_name_hoisted(class_node)
                }
            },
            str::to_string,
        );
        let continuation_indent_level = self.current_statement_continuation_indent_level();

        if needs_parens {
            self.write("(");
        }
        self.write(&temp);
        self.write(" = ");
        self.write_multiline_fragment_with_continuation_indent(
            class_iife_expr,
            continuation_indent_level,
        );

        for init_expr in computed_init_exprs {
            self.write(",");
            self.write_line();
            self.increase_indent();
            self.write(&self.render_es5_class_ir_comma_expression(init_expr));
            self.decrease_indent();
        }

        if let Some(name) = set_function_name {
            self.emit_class_expr_set_function_name_comma_item(&temp, name);
        }

        for element in static_elements {
            match element {
                Es5StaticClassExpressionElement::Field(field) => {
                    self.write(",");
                    self.write_line();
                    self.increase_indent();
                    if self.ctx.options.use_define_for_class_fields {
                        self.write("Object.defineProperty(");
                        self.write(&temp);
                        self.write(", ");
                        match &field.name_emit {
                            PropertyNameEmit::Dot(name) => {
                                self.write("\"");
                                self.write(name);
                                self.write("\"");
                            }
                            PropertyNameEmit::Bracket(name)
                            | PropertyNameEmit::BracketNumeric(name) => {
                                self.write(name);
                            }
                        }
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
                    } else {
                        self.write(&temp);
                        match &field.name_emit {
                            PropertyNameEmit::Dot(name) => {
                                self.write(".");
                                self.write(name);
                            }
                            PropertyNameEmit::Bracket(name)
                            | PropertyNameEmit::BracketNumeric(name) => {
                                self.write("[");
                                self.write(name);
                                self.write("]");
                            }
                        }
                        self.write(" = ");
                    }

                    let prev_self_alias = self.scoped_class_expression_self_alias.clone();
                    if !class_name.is_empty() && class_name != temp {
                        self.scoped_class_expression_self_alias = Some((
                            Arc::<str>::from(class_name),
                            Arc::<str>::from(temp.as_str()),
                        ));
                    }
                    let before = self.writer.len();
                    self.with_scoped_static_initializer_context_cleared(|this| {
                        this.emit_expression(field.initializer);
                    });
                    let after = self.writer.len();
                    self.scoped_class_expression_self_alias = prev_self_alias;

                    if !class_name.is_empty() && class_name != temp {
                        let full = self.writer.get_output().to_string();
                        let segment = &full[before..after];
                        let replaced = replace_identifier(segment, class_name, &temp);
                        if replaced != segment {
                            self.writer.truncate(before);
                            self.write(&replaced);
                        }
                    }
                    if self.ctx.options.use_define_for_class_fields {
                        self.write_line();
                        self.decrease_indent();
                        self.write("})");
                    }
                    self.decrease_indent();
                }
                Es5StaticClassExpressionElement::StaticBlock {
                    block,
                    saved_comment_idx,
                    ..
                } => {
                    self.write(",");
                    self.write_line();
                    self.increase_indent();
                    self.emit_static_block_iife_expression(*block, *saved_comment_idx);
                    self.decrease_indent();
                }
            }
        }

        self.write(",");
        self.write_line();
        self.increase_indent();
        self.write(&temp);
        if needs_parens {
            self.write(")");
        }
        self.decrease_indent();
    }

    fn render_es5_class_ir_comma_expression(&self, node: &IRNode) -> String {
        let expr = match node {
            IRNode::ExpressionStatement(inner) => inner.as_ref(),
            other => other,
        };
        let mut printer = IRPrinter::with_arena(self.arena);
        printer.set_transforms(self.transforms.clone());
        printer.set_target_es5(true);
        printer.set_remove_comments(self.ctx.options.remove_comments);
        printer.set_indent_level(self.writer.indent_level());
        if let Some(text) = self.source_text {
            printer.set_source_text(text);
        }
        if self.ctx.options.import_helpers && self.ctx.is_effectively_commonjs() {
            printer.set_tslib_prefix(true);
            printer.set_tslib_import_binding(self.commonjs_tslib_import_binding.clone());
        }
        printer.emit(expr);
        printer.take_output()
    }

    pub(in crate::emitter) fn emit_es5_static_class_expression_statements(
        &mut self,
        class_name: &str,
        static_elements: &[Es5StaticClassExpressionElement],
    ) {
        for element in static_elements {
            match element {
                Es5StaticClassExpressionElement::Field(field) => {
                    self.write(class_name);
                    match &field.name_emit {
                        PropertyNameEmit::Dot(name) => {
                            self.write(".");
                            self.write(name);
                        }
                        PropertyNameEmit::Bracket(name)
                        | PropertyNameEmit::BracketNumeric(name) => {
                            self.write("[");
                            self.write(name);
                            self.write("]");
                        }
                    }
                    self.write(" = ");
                    self.with_scoped_static_initializer_context_cleared(|this| {
                        this.emit_expression(field.initializer);
                    });
                    self.write(";");
                    self.write_line();
                }
                Es5StaticClassExpressionElement::StaticBlock {
                    block,
                    saved_comment_idx,
                    ..
                } => {
                    self.emit_static_block_iife_expression(*block, *saved_comment_idx);
                    self.write(";");
                    self.write_line();
                }
            }
        }
    }
}
