use super::super::Printer;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn try_emit_recovered_native_dynamic_import_extra_args(
        &mut self,
        node: &Node,
        callee: NodeIndex,
        args: &Option<NodeList>,
    ) -> bool {
        if !self.should_emit_recovered_root_js_declaration_modifiers() || self.ctx.is_commonjs() {
            return false;
        }
        let Some(callee_node) = self.arena.get(callee) else {
            return false;
        };
        if callee_node.kind != SyntaxKind::ImportKeyword as u16 {
            return false;
        }
        let Some(source_args) = self.recovered_native_dynamic_import_args_source(node) else {
            return false;
        };
        let emitted_arg_count = args.as_ref().map_or(0, |list| {
            list.nodes
                .iter()
                .filter(|&&idx| self.call_argument_should_emit(idx))
                .count()
        });
        if emitted_arg_count >= Self::top_level_comma_count(source_args) + 1 {
            return false;
        }

        self.write("import(");
        self.write(source_args.trim());
        self.write(")");
        true
    }

    fn recovered_native_dynamic_import_args_source<'b>(&self, node: &Node) -> Option<&'b str>
    where
        'a: 'b,
    {
        let text = self.source_text?;
        let bytes = text.as_bytes();
        let start = std::cmp::min(node.pos as usize, bytes.len());
        let end = std::cmp::min(node.end as usize, bytes.len());
        let open = bytes[start..end]
            .iter()
            .position(|&b| b == b'(')
            .map(|offset| start + offset)?;
        let close = bytes[open..end]
            .iter()
            .rposition(|&b| b == b')')
            .map(|offset| open + offset)?;
        if close <= open {
            return None;
        }
        crate::safe_slice::slice(text, open + 1, close).ok()
    }

    fn top_level_comma_count(text: &str) -> usize {
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;
        let mut quote = None;
        let mut escaped = false;
        let mut count = 0usize;

        for ch in text.chars() {
            if let Some(quote_ch) = quote {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == quote_ch {
                    quote = None;
                }
                continue;
            }

            match ch {
                '\'' | '"' | '`' => quote = Some(ch),
                '(' => paren_depth += 1,
                ')' => paren_depth = paren_depth.saturating_sub(1),
                '[' => bracket_depth += 1,
                ']' => bracket_depth = bracket_depth.saturating_sub(1),
                '{' => brace_depth += 1,
                '}' => brace_depth = brace_depth.saturating_sub(1),
                ',' if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => count += 1,
                _ => {}
            }
        }

        count
    }
}
