use super::super::Printer;
use crate::emitter::declarations::class::class_has_self_references;
use crate::output::source_writer::SourceWriter;
use tsz_parser::parser::NodeIndex;

impl<'a> Printer<'a> {
    pub(in crate::emitter::module_wrapper) fn emit_system_legacy_class_decorator_export(
        &mut self,
        export_name: &str,
        class_name: &str,
        decorators: &[NodeIndex],
        members: &[NodeIndex],
        alias_name: Option<&str>,
    ) {
        let assignment = self.capture_system_legacy_class_decorator_assignment(
            class_name, decorators, members, alias_name,
        );
        if assignment.is_empty() {
            return;
        }

        self.write("exports_1(\"");
        self.write(export_name);
        self.write("\", ");
        self.write(&assignment);
        self.write(");");
    }

    pub(in crate::emitter::module_wrapper) fn capture_system_legacy_class_decorator_assignment(
        &mut self,
        class_name: &str,
        decorators: &[NodeIndex],
        members: &[NodeIndex],
        alias_name: Option<&str>,
    ) -> String {
        let mut temp_writer = SourceWriter::with_capacity(256);
        temp_writer.set_new_line_kind(self.ctx.options.new_line);
        temp_writer.set_indent_level(self.writer.indent_level());
        std::mem::swap(&mut self.writer, &mut temp_writer);

        self.emit_legacy_class_decorator_assignment(
            class_name, decorators, false, false, false, alias_name, members,
        );

        std::mem::swap(&mut self.writer, &mut temp_writer);
        let emitted = temp_writer.take_output();
        if emitted.is_empty() {
            return String::new();
        }

        emitted
            .trim_start()
            .trim_end()
            .trim_end_matches(';')
            .to_string()
    }

    pub(in crate::emitter::module_wrapper) fn system_legacy_decorated_class_alias(
        &mut self,
        class_idx: NodeIndex,
        class_name: &str,
        members: &[NodeIndex],
    ) -> Option<String> {
        if !class_has_self_references(self.arena, self.source_text_for_map(), class_name, members) {
            return None;
        }
        if let Some(alias) = self
            .preplanned_legacy_decorated_class_aliases
            .get(&class_idx)
        {
            return Some(alias.clone());
        }
        let alias = self.make_unique_name_from_base(class_name);
        self.preplanned_legacy_decorated_class_aliases
            .insert(class_idx, alias.clone());
        Some(alias)
    }

    pub(in crate::emitter::module_wrapper) fn capture_system_class_assignment(
        &mut self,
        class_node: &tsz_parser::parser::node::Node,
        class_idx: NodeIndex,
        class_name: &str,
        alias_name: Option<&str>,
    ) -> String {
        let before_len = self.writer.len();
        self.write(class_name);
        self.write(" = ");
        if let Some(alias) = alias_name {
            self.write(alias);
            self.write(" = ");
        }
        self.anonymous_default_export_name = None;
        self.emit_class_es6_with_options(
            class_node, class_idx, true, None, alias_name, alias_name, false,
        );
        let after_len = self.writer.len();
        let full_output = self.writer.get_output().to_string();
        let emitted = full_output[before_len..after_len].to_string();
        self.writer.truncate(before_len);

        emitted
    }
}

pub(in crate::emitter::module_wrapper) fn split_system_class_static_tail(
    text: &str,
) -> (&str, &str) {
    if let Some(close_idx) = find_matching_class_body_close(text) {
        text.split_at(close_idx)
    } else {
        (text, "")
    }
}

fn find_matching_class_body_close(text: &str) -> Option<usize> {
    let open_idx = text.find('{')?;
    let mut depth = 0usize;
    for (idx, ch) in text[open_idx..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(open_idx + idx + ch.len_utf8());
                }
            }
            _ => {}
        }
    }
    None
}
