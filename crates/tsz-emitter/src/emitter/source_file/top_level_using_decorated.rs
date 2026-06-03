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

pub(in crate::emitter::source_file) fn export_decorate_assignment(
    emitted: String,
    export_prefix: &str,
    binding_name: &str,
    wrap_system_call: bool,
) -> (String, bool) {
    let local_decorate = format!("{binding_name} = __decorate(");
    let exported_decorate = format!("{export_prefix}{binding_name} = __decorate(");
    if emitted.contains(&exported_decorate) {
        return (emitted, false);
    }
    let Some(decorate_start) = emitted.find(&local_decorate) else {
        return (emitted, false);
    };

    let mut exported = String::with_capacity(emitted.len() + export_prefix.len());
    exported.push_str(&emitted[..decorate_start]);
    exported.push_str(&exported_decorate);
    exported.push_str(&emitted[decorate_start + local_decorate.len()..]);
    if wrap_system_call {
        exported = wrap_system_decorate_assignment(exported, decorate_start);
    }
    (exported, true)
}

fn wrap_system_decorate_assignment(emitted: String, search_from: usize) -> String {
    let suffix_start = search_from.min(emitted.len());
    let Some(relative_end) = emitted[suffix_start..].rfind(");") else {
        return emitted;
    };
    let end = suffix_start + relative_end;
    let mut wrapped = String::with_capacity(emitted.len() + 1);
    wrapped.push_str(&emitted[..end]);
    wrapped.push_str("));\n");
    wrapped.push_str(&emitted[end + 2..]);
    if wrapped.ends_with("\n\n") {
        wrapped.pop();
    }
    wrapped
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
