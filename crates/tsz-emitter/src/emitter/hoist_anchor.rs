//! Captured insertion point for deferred hoist-line writes.
//!
//! Several emit paths buffer `var _a;` / `let _x;` declarations during
//! statement emission and then insert the combined `var …;` line back at
//! a previously-captured offset (the function-body's opening line). The
//! interval between capture and insertion may grow the writer's live
//! `indent_level` — most commonly when a block-level `using` lowering
//! opens a `try {` wrapper before any statement emits. Recomputing the
//! indent from the live level at insertion time would push the hoist
//! line past the surrounding scope's indent; capturing the level at
//! anchor time pins the insertion to the right column.

use super::Printer;

#[derive(Clone, Copy)]
pub(in crate::emitter) struct HoistAnchor {
    pub(in crate::emitter) byte_offset: usize,
    pub(in crate::emitter) line_no: u32,
    pub(in crate::emitter) indent_level: u32,
}

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn capture_hoist_anchor(&self) -> HoistAnchor {
        HoistAnchor {
            byte_offset: self.writer.len(),
            line_no: self.writer.current_line(),
            indent_level: self.writer.indent_level(),
        }
    }
}
