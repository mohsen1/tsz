use super::super::Printer;
use crate::emitter::syntax_kind_ext;
use tsz_parser::parser::NodeIndex;

pub(in crate::emitter::source_file) fn strip_decorate_export_prefix(
    emitted: &str,
    export_prefix: &str,
    binding_name: &str,
) -> String {
    let exported_decorate = format!("{export_prefix}{binding_name} = __decorate(");
    let local_decorate = format!("{binding_name} = __decorate(");
    let mut lines = Vec::new();
    for line in emitted.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix(&exported_decorate) {
            let leading_len = line.len() - trimmed.len();
            let (leading, _) = line.split_at(leading_len);
            lines.push(format!("{leading}{local_decorate}{rest}"));
        } else {
            lines.push(line.to_string());
        }
    }
    let mut stripped = lines.join("\n");
    if emitted.ends_with('\n') {
        stripped.push('\n');
    }
    stripped
}

impl<'a> Printer<'a> {
    pub(in crate::emitter::source_file) fn emit_top_level_using_initializer(
        &mut self,
        initializer: NodeIndex,
        binding_name: &str,
    ) {
        if self.top_level_using_initializer_is_tc39_decorated_class_expr(initializer)
            && let Some(expr) =
                self.capture_tc39_decorated_class_expression(initializer, binding_name)
        {
            self.write(&expr);
            return;
        }

        if !self.try_emit_object_literal_es5_inline_computed_expression(initializer) {
            self.emit(initializer);
        }
    }

    fn top_level_using_initializer_is_tc39_decorated_class_expr(
        &self,
        initializer: NodeIndex,
    ) -> bool {
        !self.ctx.target_es5
            && !self.ctx.options.legacy_decorators
            && !self.ctx.options.target.supports_es2025()
            && self.arena.get(initializer).is_some_and(|init_node| {
                init_node.kind == syntax_kind_ext::CLASS_EXPRESSION
                    && self.arena.get_class(init_node).is_some_and(|class| {
                        !self.collect_class_decorators(&class.modifiers).is_empty()
                    })
            })
    }
}
