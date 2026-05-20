use super::super::Printer;
use tsz_parser::parser::{NodeIndex, NodeList};

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_static_block_await_arrow_recovery_blocks_after_variable_statement(
        &mut self,
        declarations: &NodeList,
    ) {
        if !self.ctx.flags.in_class_static_block {
            return;
        }

        let bodies: Vec<NodeIndex> = declarations
            .nodes
            .iter()
            .copied()
            .filter_map(|decl_list_idx| self.arena.get(decl_list_idx))
            .filter_map(|decl_list_node| self.arena.get_variable(decl_list_node))
            .flat_map(|decl_list| decl_list.declarations.nodes.iter().copied())
            .filter_map(|decl_idx| self.arena.get(decl_idx))
            .filter_map(|decl_node| self.arena.get_variable_declaration(decl_node))
            .filter_map(|decl| self.static_block_await_arrow_recovery_body(decl.initializer))
            .collect();

        for body in bodies {
            self.write_line();
            let prev_emitting_function_body_block = self.emitting_function_body_block;
            self.emitting_function_body_block = true;
            self.emit(body);
            self.emitting_function_body_block = prev_emitting_function_body_block;
        }
    }

    pub(in crate::emitter) fn line_has_static_block_await_arrow_recovery(line: &str) -> bool {
        let Some(eq) = line.find('=') else {
            return false;
        };
        let mut rest = line[eq + 1..].trim_start();
        if rest.starts_with("(await)") {
            return true;
        }
        if !rest.starts_with("await") {
            return false;
        }
        rest = &rest["await".len()..];
        if rest
            .as_bytes()
            .first()
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'$')
        {
            return false;
        }
        rest.trim_start().starts_with("=>")
    }
}
