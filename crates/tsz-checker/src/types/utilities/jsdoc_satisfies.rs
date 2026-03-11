use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    pub(crate) fn report_duplicate_jsdoc_satisfies_tags(&mut self, idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_common::comments::is_jsdoc_comment;

        if !self.ctx.should_resolve_jsdoc() {
            return;
        }

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;

        let mut attached_positions: Vec<u32> = Vec::new();
        let mut attached_comment_pos = None;
        if let Some((_jsdoc, jsdoc_start)) =
            self.try_jsdoc_with_ancestor_walk_and_pos(idx, comments, source_text)
        {
            if let Some(comment) = comments.iter().find(|c| c.pos == jsdoc_start) {
                let raw = &source_text[comment.pos as usize..comment.end as usize];
                attached_positions = Self::jsdoc_satisfies_keyword_positions(raw, jsdoc_start);
            }
            attached_comment_pos = Some(jsdoc_start);
            self.emit_duplicate_jsdoc_satisfies_positions(&attached_positions);
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) else {
            return;
        };
        if var_decl.initializer.is_none() {
            return;
        }

        let Some(init_node) = self.ctx.arena.get(var_decl.initializer) else {
            return;
        };
        let Some(comment) = self
            .try_leading_jsdoc_with_pos(comments, init_node.pos, source_text)
            .and_then(|(_, pos)| comments.iter().find(|c| c.pos == pos))
            .filter(|c| is_jsdoc_comment(c, source_text))
        else {
            return;
        };

        let inline_positions = Self::jsdoc_satisfies_keyword_positions(
            &source_text[comment.pos as usize..comment.end as usize],
            comment.pos,
        );
        self.emit_duplicate_jsdoc_satisfies_positions(&inline_positions);

        if !attached_positions.is_empty()
            && !inline_positions.is_empty()
            && attached_comment_pos != Some(comment.pos)
        {
            let message =
                format_message(diagnostic_messages::TAG_ALREADY_SPECIFIED, &["satisfies"]);
            self.ctx.error(
                attached_positions[0],
                "satisfies".len() as u32,
                message,
                diagnostic_codes::TAG_ALREADY_SPECIFIED,
            );
        }
    }

    fn jsdoc_satisfies_keyword_positions(jsdoc: &str, jsdoc_start: u32) -> Vec<u32> {
        let mut positions = Vec::new();
        let mut search_from = 0usize;
        while let Some(rel) = jsdoc[search_from..].find("@satisfies") {
            let absolute = search_from + rel;
            positions.push(jsdoc_start + absolute as u32 + 1);
            search_from = absolute + "@satisfies".len();
        }
        positions
    }

    fn emit_duplicate_jsdoc_satisfies_positions(&mut self, positions: &[u32]) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        if positions.len() < 2 {
            return;
        }
        let message = format_message(diagnostic_messages::TAG_ALREADY_SPECIFIED, &["satisfies"]);
        for &pos in &positions[1..] {
            self.ctx.error(
                pos,
                "satisfies".len() as u32,
                message.clone(),
                diagnostic_codes::TAG_ALREADY_SPECIFIED,
            );
        }
    }
}
