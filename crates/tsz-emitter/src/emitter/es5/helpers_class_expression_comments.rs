//! Comment-cursor reconciliation for ES5 class-expression lowering.
//!
//! When an ES6 class expression is downleveled to the ES5 IIFE form, the
//! interior emission is performed by a sub-emitter (`ClassES5Emitter`) that
//! reads comments directly from the source text and prints member comments
//! (e.g. a constructor's JSDoc) inside the generated IIFE. The main `Printer`
//! maintains a separate global comment cursor (`comment_emit_idx`) over
//! `all_comments`. Because the sub-emitter does not advance that cursor, any
//! comment that lived *inside* the class body would otherwise remain pending
//! and be re-emitted by the next leading/trailing comment scan after the
//! statement — duplicating it (and dumping it after the IIFE).
//!
//! This mirrors the reconciliation the ES5 class *declaration* and namespace
//! dispatch arms already perform: after the sub-emitter prints the lowered
//! form, advance the cursor past every comment that falls inside the lowered
//! node's source range.

use super::super::*;

impl<'a> Printer<'a> {
    /// Emit an ES5-lowered class expression and reconcile the global comment
    /// cursor so interior member comments handled by the sub-emitter are not
    /// re-emitted after the statement.
    pub(in crate::emitter) fn emit_class_expression_es5(&mut self, class_node: NodeIndex) {
        self.emit_class_expression_es5_inner(class_node);
        self.advance_comment_cursor_past_node_interior(class_node);
    }

    /// Advance `comment_emit_idx` past every pending comment that lies strictly
    /// inside `node`'s source range (i.e. before its closing token). Used after
    /// a sub-emitter has already emitted those interior comments itself.
    ///
    /// The end boundary is narrowed with `find_token_end_before_trivia` so
    /// comments in the node's trailing trivia — which logically belong to the
    /// following construct — are left for the normal trailing/leading scan.
    pub(in crate::emitter) fn advance_comment_cursor_past_node_interior(
        &mut self,
        node_idx: NodeIndex,
    ) {
        if self.ctx.options.remove_comments {
            return;
        }
        let Some(node) = self.arena.get(node_idx) else {
            return;
        };
        let close_pos = self.find_token_end_before_trivia(node.pos, node.end);
        while self.comment_emit_idx < self.all_comments.len()
            && self.all_comments[self.comment_emit_idx].pos < close_pos
        {
            self.comment_emit_idx += 1;
        }
    }
}
