use super::super::Printer;
use tsz_parser::parser::node::{LoopData, Node};
use tsz_parser::parser::syntax_kind_ext;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn try_emit_invalid_let_of_array_for_recovery(
        &mut self,
        node: &Node,
        loop_stmt: &LoopData,
    ) -> bool {
        let Some(header) = self.invalid_let_of_array_for_header(node, loop_stmt) else {
            return false;
        };

        self.write("for (");
        self.write(&header);
        self.write(")");
        self.write_line();
        self.increase_indent();
        self.write(";");
        self.decrease_indent();
        true
    }

    pub(in crate::emitter) fn for_in_invalid_let_header_needs_recovery_space(
        &self,
        node: &Node,
    ) -> bool {
        let Some(text) = self.source_text else {
            return false;
        };
        let start = node.pos as usize;
        if start >= text.len() {
            return false;
        }
        let Some(header) = text[start..].split(')').next() else {
            return false;
        };
        let Some(open_paren) = header.find('(') else {
            return false;
        };
        let inner = header[open_paren + 1..].trim_start();
        is_keyword_followed_by(inner, "let", "in")
    }

    fn invalid_let_of_array_for_header(&self, node: &Node, loop_stmt: &LoopData) -> Option<String> {
        if loop_stmt.initializer.is_some()
            || loop_stmt.condition.is_some()
            || loop_stmt.incrementor.is_some()
            || self
                .arena
                .get(loop_stmt.statement)
                .is_none_or(|stmt| stmt.kind != syntax_kind_ext::EMPTY_STATEMENT)
        {
            return None;
        }

        let text = self.source_text?;
        let start = node.pos as usize;
        let header_end = text.get(start..)?.find(')').map(|offset| start + offset)?;
        let header = text.get(start..header_end)?;
        let open_paren = header.find('(')?;
        let inner = header[open_paren + 1..].trim_start();
        let after_let = keyword_tail(inner, "let")?.trim_start();
        let after_of = keyword_tail(after_let, "of")?.trim_start();
        let array = recovered_array_elements_source(after_of)?;
        let elements = array
            .split(',')
            .map(str::trim)
            .filter(|element| !element.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!("let of, []; {elements}; "))
    }
}

fn keyword_tail<'a>(text: &'a str, keyword: &str) -> Option<&'a str> {
    let tail = text.strip_prefix(keyword)?;
    if tail
        .chars()
        .next()
        .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '$')
    {
        Some(tail)
    } else {
        None
    }
}

fn is_keyword_followed_by(text: &str, first: &str, second: &str) -> bool {
    let Some(tail) = keyword_tail(text, first) else {
        return false;
    };
    keyword_tail(tail.trim_start(), second).is_some()
}

fn recovered_array_elements_source(text: &str) -> Option<&str> {
    let after_open = text.strip_prefix('[')?;
    let close_offset = after_open.rfind(']')?;
    let trailing = &after_open[close_offset + 1..];
    if source_tail_is_trivia(trailing) {
        Some(&after_open[..close_offset])
    } else {
        None
    }
}

fn source_tail_is_trivia(mut text: &str) -> bool {
    loop {
        let trimmed = text.trim_start();
        if trimmed.is_empty() {
            return true;
        }
        if let Some(rest) = trimmed.strip_prefix("//") {
            let Some(line_end) = rest.find('\n') else {
                return true;
            };
            text = &rest[line_end + 1..];
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("/*") {
            let Some(end) = rest.find("*/") else {
                return false;
            };
            text = &rest[end + 2..];
            continue;
        }
        return false;
    }
}

#[cfg(test)]
mod tests {
    use crate::context::emit::EmitContext;
    use crate::emitter::{Printer, PrinterOptions};
    use crate::lowering::LoweringPass;
    use tsz_common::ScriptTarget;

    fn emit_es5(source: &str) -> String {
        let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let options = PrinterOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        };
        let ctx = EmitContext::with_options(options.clone());
        let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
        let mut printer = Printer::with_transforms_and_options(&parser.arena, transforms, options);
        printer.set_target_es5(ctx.target_es5);
        printer.set_source_text(source);
        printer.emit(root);
        printer.get_output().to_string()
    }

    #[test]
    fn invalid_let_of_array_for_recovery_accepts_trailing_header_trivia() {
        for source in [
            "for (let of [1, 2, 3] ) ;",
            "for (let of [1, 2, 3] /* keep */) ;",
            "for (let of [1, 2, 3] // keep\n) ;",
        ] {
            let output = emit_es5(source);

            assert!(
                output.contains("for (let of, []; 1, 2, 3; )"),
                "Invalid `let of` recovery should ignore trailing header trivia.\nSource:\n{source}\nOutput:\n{output}"
            );
        }
    }
}
