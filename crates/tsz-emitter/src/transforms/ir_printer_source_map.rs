//! Source-map capture helpers for [`IRPrinter`].
//!
//! Extracted from `ir_printer.rs` to keep file sizes manageable.

use super::IRPrinter;
use crate::output::source_writer::compute_line_col;
use crate::transforms::emit_utils::skip_trivia_forward;
use tsz_common::source_map::Mapping;
use tsz_parser::parser::base::NodeIndex;

impl<'a> IRPrinter<'a> {
    /// Enable source-map mappings for re-emitted `ASTRef` nodes.
    pub const fn enable_mapping_capture(&mut self) {
        self.capture_mappings = true;
    }

    pub const fn set_source_map_source_index(&mut self, index: u32) {
        self.source_index = index;
    }

    /// Take mappings recorded during emission. Generated positions are relative
    /// to the start of this printer's output.
    pub fn take_mappings(&mut self) -> Vec<Mapping> {
        std::mem::take(&mut self.mappings)
    }

    /// Record a mapping from the current generated output position to the token
    /// start of `idx` in the original source.
    pub(super) fn record_ast_ref_mapping(&mut self, idx: NodeIndex) {
        if !self.capture_mappings {
            return;
        }
        let (Some(arena), Some(text)) = (self.arena, self.source_text) else {
            return;
        };
        let Some(node) = arena.get(idx) else {
            return;
        };
        let token_start = skip_trivia_forward(Some(text), node.pos, node.end);
        let (original_line, original_column) = compute_line_col(text, token_start);
        let (generated_line, generated_column) =
            compute_line_col(&self.output, self.output.len() as u32);
        self.mappings.push(Mapping {
            generated_line,
            generated_column,
            source_index: self.source_index,
            original_line,
            original_column,
            name_index: None,
        });
    }
}
