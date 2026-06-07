//! Object display helpers for indexed-access diagnostics.
//!
//! Kept separate from `indexed_access.rs` so the checker-boundary line cap
//! stays stable as TS2536 cases grow.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

/// Normalize an indexed-access object's source text so its display matches the
/// type printer's canonical form instead of leaking the original span.
///
/// `tsc` renders the object type through its printer, which never carries the
/// stray whitespace a source slice can ("`A[ F ]`", or a `keyof   T` written
/// across lines). This collapses runs of insignificant whitespace to a single
/// space and drops the spaces that hug `[`/`]`, so unusual spacing renders the
/// same as `tsc`. Cleanly-written source has no stray spacing, so it is returned
/// unchanged — this only repairs the irregular cases and never rewrites the
/// common one. Source text is retained (rather than a printed type) because it
/// preserves the written alias name, which a resolved/rebuilt type would expand.
pub(super) fn normalize_indexed_access_object_text(text: &str) -> String {
    let mut collapsed = String::with_capacity(text.len());
    let mut prev_was_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !prev_was_space {
                collapsed.push(' ');
            }
            prev_was_space = true;
        } else {
            collapsed.push(ch);
            prev_was_space = false;
        }
    }
    collapsed
        .replace("[ ", "[")
        .replace(" ]", "]")
        .trim()
        .to_string()
}

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
            let text = normalize_indexed_access_object_text(&text);
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
