use super::super::Printer;
use tsz_parser::parser::node::{LoopData, Node};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::{SyntaxKind, is_ecmascript_identifier_part, is_ecmascript_identifier_start};

struct RecoveredTypedForBodyCall {
    keyword: String,
    binding: String,
    callee: String,
    argument: String,
}

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn try_emit_typed_for_body_call_recovery(
        &mut self,
        node: &Node,
        loop_stmt: &LoopData,
    ) -> bool {
        let Some(recovered) = self.typed_for_body_call_recovery(node, loop_stmt) else {
            return false;
        };

        self.write("for (");
        self.write(&recovered.keyword);
        self.write(" ");
        self.write(&recovered.binding);
        self.write(", { ");
        self.write(&recovered.callee);
        self.write(" }; (");
        self.write(&recovered.argument);
        self.write("); )");
        self.write_line();
        self.increase_indent();
        self.write(";");
        self.decrease_indent();
        true
    }

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

    fn typed_for_body_call_recovery(
        &self,
        node: &Node,
        loop_stmt: &LoopData,
    ) -> Option<RecoveredTypedForBodyCall> {
        if loop_stmt.condition.is_some() || loop_stmt.incrementor.is_some() {
            return None;
        }

        let init_node = self.arena.get(loop_stmt.initializer)?;
        if init_node.kind != syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            return None;
        }
        let variable = self.arena.get_variable(init_node)?;
        if variable.declarations.nodes.len() != 1 {
            return None;
        }
        let declaration_node = self.arena.get(*variable.declarations.nodes.first()?)?;
        let declaration = self.arena.get_variable_declaration(declaration_node)?;
        if declaration.initializer.is_some() || declaration.type_annotation.is_none() {
            return None;
        }
        let name_node = self.arena.get(declaration.name)?;
        if name_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let body_node = self.arena.get(loop_stmt.statement)?;
        if body_node.kind != syntax_kind_ext::BLOCK {
            return None;
        }
        let block = self.arena.get_block(body_node)?;
        if !block.statements.nodes.is_empty() {
            return None;
        }

        let text = self.source_text?;
        let header_start = node.pos as usize;
        let body_start = body_node.pos as usize;
        let body_end = (body_node.end as usize).min(text.len());
        let header_source = text.get(header_start..body_start)?;
        let open_paren = header_source.find('(')?;
        let close_paren = header_source.rfind(')')?;
        let header_inner = header_source.get(open_paren + 1..close_paren)?.trim();
        if header_inner.contains(';') {
            return None;
        }

        let (keyword, after_keyword) = split_variable_keyword(header_inner)?;
        let (binding, after_binding) = parse_source_identifier(after_keyword.trim_start())?;
        if binding != source_slice(text, name_node)? {
            return None;
        }
        if !after_binding.trim_start().starts_with(':') {
            return None;
        }

        let body_source = text.get(body_start..body_end)?;
        let open_brace = body_source.find('{')?;
        let close_brace = body_source.rfind('}')?;
        let body_inner = body_source.get(open_brace + 1..close_brace)?.trim();
        let (callee, argument) = parse_single_identifier_call(body_inner)?;

        Some(RecoveredTypedForBodyCall {
            keyword: keyword.to_string(),
            binding: binding.to_string(),
            callee: callee.to_string(),
            argument: argument.to_string(),
        })
    }

    fn invalid_let_of_array_for_header(
        &self,
        node: &Node,
        loop_stmt: &LoopData,
    ) -> Option<String> {
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

fn source_slice<'a>(text: &'a str, node: &Node) -> Option<&'a str> {
    crate::safe_slice::slice(text, node.pos as usize, node.end as usize).ok()
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

fn split_variable_keyword(text: &str) -> Option<(&str, &str)> {
    for keyword in ["let", "var", "const"] {
        if let Some(tail) = keyword_tail(text, keyword) {
            return Some((keyword, tail));
        }
    }
    None
}

fn parse_source_identifier(text: &str) -> Option<(&str, &str)> {
    let mut chars = text.char_indices();
    let (_, first) = chars.next()?;
    if !is_identifier_start(first) {
        return None;
    }
    let end = chars
        .find_map(|(idx, ch)| (!is_identifier_part(ch)).then_some(idx))
        .unwrap_or(text.len());
    Some(text.split_at(end))
}

fn parse_single_identifier_call(text: &str) -> Option<(&str, &str)> {
    let (callee, tail) = parse_source_identifier(text)?;
    let tail = tail.trim_start();
    let after_open = tail.strip_prefix('(')?.trim_start();
    let (argument, tail) = parse_source_identifier(after_open)?;
    let tail = tail.trim_start();
    let tail = tail.strip_prefix(')')?.trim_start();
    let tail = tail.strip_prefix(';').unwrap_or(tail).trim_start();
    if source_tail_is_trivia(tail) {
        Some((callee, argument))
    } else {
        None
    }
}

fn is_identifier_start(ch: char) -> bool {
    is_ecmascript_identifier_start(ch)
}

fn is_identifier_part(ch: char) -> bool {
    is_ecmascript_identifier_part(ch)
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

    #[test]
    fn typed_for_body_call_recovery_preserves_tsc_header_shape() {
        let output = emit_es5("for (let x: y) { z(x); }");

        assert!(
            output.contains("for (let x, { z }; (x); )\n    ;"),
            "Typed recovered `for` should preserve tsc's recovered call header.\nOutput:\n{output}"
        );
    }

    #[test]
    fn typed_for_body_call_recovery_uses_source_identifiers() {
        let output = emit_es5("for (let item: Type) { consume(item); }");

        assert!(
            output.contains("for (let item, { consume }; (item); )\n    ;"),
            "Typed recovered `for` should be source-backed, not fixture-name-specific.\nOutput:\n{output}"
        );
    }

    #[test]
    fn typed_for_body_call_recovery_leaves_valid_for_loops_alone() {
        let output = emit_es5("for (let x; ; ) { z(x); }");

        assert!(
            output.contains("for (var x;;) {"),
            "Valid `for` loops should continue through the normal printer path.\nOutput:\n{output}"
        );
    }

    #[test]
    fn typed_for_body_call_recovery_accepts_unicode_identifiers() {
        // \u{e9} and \u{65e5} are valid ECMAScript identifier-start chars.
        let output = emit_es5("for (let r\u{e9}sum\u{e9}: Type) { donn\u{e9}es(r\u{e9}sum\u{e9}); }");
        assert!(
            output.contains("r\u{e9}sum\u{e9}"),
            "Unicode binding identifier should be preserved in recovery.\nOutput:\n{output}"
        );

        let output2 =
            emit_es5("for (let \u{65e5}\u{672c}\u{8a9e}: T) { \u{51e6}\u{7406}(\u{65e5}\u{672c}\u{8a9e}); }");
        assert!(
            output2.contains("\u{65e5}\u{672c}\u{8a9e}"),
            "CJK binding identifier should be preserved in recovery.\nOutput:\n{output2}"
        );
    }
}
