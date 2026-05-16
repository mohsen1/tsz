use super::super::Printer;
use crate::emitter::syntax_kind_ext;
use tsz_parser::parser::NodeIndex;

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

        self.emit(initializer);
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
