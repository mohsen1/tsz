//! Go to Source Definition implementation for LSP.
//!
//! Provides navigation from `.d.ts` declaration files to the corresponding
//! `.ts` source files. This is useful when working with compiled packages
//! that include both declaration and source files.
//!
//! The LSP spec calls this `textDocument/_typescript/goToSourceDefinition`
//! (TypeScript-specific extension).

use crate::resolver::ScopeWalker;
use crate::utils::{find_node_at_offset, is_symbol_query_node};
use tsz_common::position::{Location, Position, Range};
use tsz_parser::NodeIndex;

define_lsp_provider!(binder GoToSourceDefinitionProvider, "Provider for Go to Source Definition.");

impl<'a> GoToSourceDefinitionProvider<'a> {
    /// Get the source definition location for the symbol at the given position.
    ///
    /// If the current file is a `.d.ts` file, attempts to find the corresponding
    /// `.ts` source file. Returns None if:
    /// - No symbol found at position
    /// - The current file is already a `.ts` file
    /// - No corresponding source file exists
    pub fn get_source_definition(
        &self,
        root: NodeIndex,
        position: Position,
    ) -> Option<Vec<Location>> {
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;

        let node_idx = find_node_at_offset(self.arena, offset);
        if node_idx.is_none() {
            return None;
        }
        if !is_symbol_query_node(self.arena, node_idx) {
            return None;
        }

        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let symbol_id = walker.resolve_node(root, node_idx)?;

        let symbol = self.binder.symbols.get(symbol_id)?;

        if symbol.declarations.is_empty() {
            return None;
        }

        // If we're in a .d.ts file, compute the potential source file path
        let source_path = self.compute_source_path()?;

        // Return locations pointing to the source file
        let locations: Vec<Location> = symbol
            .declarations
            .iter()
            .filter_map(|&decl_idx| {
                let node = self.arena.get(decl_idx)?;
                let start_pos = self.line_map.offset_to_position(node.pos, self.source_text);
                let end_pos = self.line_map.offset_to_position(node.end, self.source_text);

                Some(Location {
                    file_path: source_path.clone(),
                    range: Range::new(start_pos, end_pos),
                })
            })
            .collect();

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    }

    /// Compute the potential `.ts` source file path from a `.d.ts` file.
    ///
    /// Maps:
    /// - `foo.d.ts` → `foo.ts`
    /// - `foo.d.mts` → `foo.mts`
    /// - `foo.d.cts` → `foo.cts`
    fn compute_source_path(&self) -> Option<String> {
        if self.file_name.ends_with(".d.ts") {
            Some(self.file_name.replace(".d.ts", ".ts"))
        } else if self.file_name.ends_with(".d.mts") {
            Some(self.file_name.replace(".d.mts", ".mts"))
        } else if self.file_name.ends_with(".d.cts") {
            Some(self.file_name.replace(".d.cts", ".cts"))
        } else {
            // Not a declaration file — source definition doesn't apply
            None
        }
    }
}
