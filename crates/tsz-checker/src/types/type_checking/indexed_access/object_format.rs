//! Object display helpers for indexed-access diagnostics.
//!
//! Kept separate from `indexed_access.rs` so the checker-boundary line cap
//! stays stable as TS2536 cases grow.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn format_ts2536_object_type(
        &self,
        object_node_idx: NodeIndex,
        object_type: TypeId,
    ) -> String {
        if let Some(node) = self.ctx.arena.get(object_node_idx)
            && matches!(
                node.kind,
                k if k == syntax_kind_ext::TYPE_REFERENCE
                    || k == syntax_kind_ext::INDEXED_ACCESS_TYPE
            )
            && let Some(text) = self.node_text(object_node_idx)
        {
            let text = text.trim();
            let text = text.strip_prefix('(').unwrap_or(text);
            let text = text.strip_suffix(')').unwrap_or(text).trim();
            if !text.is_empty() {
                return text.to_string();
            }
        }
        self.format_type(object_type)
    }
}
