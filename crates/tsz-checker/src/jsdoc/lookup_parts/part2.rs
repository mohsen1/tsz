impl<'a> CheckerState<'a> {
    /// Check if two source positions are in different function scopes.
    /// Used for JSDoc typedef scoping — a typedef defined inside a function
    /// should not be visible outside that function.
    #[allow(dead_code)]
    pub(crate) fn is_in_different_function_scope(&self, comment_pos: u32, anchor_pos: u32) -> bool {
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return false;
        };
        let source_text = sf.text.to_string();
        // Walk from anchor_pos backward to see if we cross a function boundary
        // before reaching comment_pos. If comment_pos is inside a function body
        // and anchor_pos is outside it, they're in different scopes.
        let text = &source_text[..anchor_pos as usize];
        let mut depth: i32 = 0;
        for ch in text[comment_pos as usize..].chars() {
            match ch {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
        }
        // If depth != 0, the comment is inside a nested scope relative to anchor
        depth != 0
    }

    /// Find the end position of a function body by scanning for the matching '}'.
    pub(crate) fn find_function_body_end(node_pos: u32, node_end: u32, source_text: &str) -> u32 {
        let start = node_pos as usize;
        let end = node_end as usize;
        if end > source_text.len() {
            return node_end;
        }
        let slice = &source_text[start..end];
        let mut depth = 0i32;
        let mut last_close = node_end;
        for (i, ch) in slice.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        last_close = (start + i + 1) as u32;
                        break;
                    }
                }
                _ => {}
            }
        }
        last_close
    }
}
