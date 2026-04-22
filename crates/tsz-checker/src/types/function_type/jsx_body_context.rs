use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    pub(super) fn is_jsx_body_child_closure(&self, func_idx: NodeIndex) -> bool {
        let mut current = func_idx;
        while let Some(parent) = self.ctx.arena.parent_of(current) {
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                break;
            };
            match parent_node.kind {
                k if k == syntax_kind_ext::JSX_ATTRIBUTE => return false,
                k if k == syntax_kind_ext::JSX_ELEMENT || k == syntax_kind_ext::JSX_FRAGMENT => {
                    return true;
                }
                _ => current = parent,
            }
        }
        false
    }
}
