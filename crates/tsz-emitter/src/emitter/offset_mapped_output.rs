//! Writing pre-rendered output fragments into the active source map.
//!
//! Several lowering paths render a fragment with a nested emitter (ES5 class
//! IIFEs, async `__generator` bodies) and splice the text into the outer
//! writer. When a source map is active, the fragment's mappings must be
//! re-anchored at the splice position; otherwise the text is written as-is.

use super::Printer;
use tsz_common::source_map::Mapping;

impl Printer<'_> {
    /// Write a pre-rendered fragment, re-anchoring its source mappings at the
    /// current writer position when a source map is active.
    pub(in crate::emitter) fn write_with_offset_mappings(
        &mut self,
        rendered: &str,
        mappings: &[Mapping],
    ) {
        if !mappings.is_empty() && self.writer.has_source_map() {
            self.writer.write("");
            let base_line = self.writer.current_line();
            let base_column = self.writer.current_column();
            self.writer
                .add_offset_mappings(base_line, base_column, mappings);
            self.writer.write(rendered);
        } else {
            self.write(rendered);
        }
    }
}
