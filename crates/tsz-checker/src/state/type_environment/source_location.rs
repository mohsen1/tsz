use crate::query_boundaries::common::SourceLocation;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Create a union type from multiple types.
    ///
    /// Handles empty (-> NEVER), single (-> that type), and multi-member cases.
    /// Automatically normalizes: flattens nested unions, deduplicates, sorts.
    pub fn get_union_type(&self, types: Vec<TypeId>) -> TypeId {
        tsz_solver::utils::union_or_single(self.ctx.types, types)
    }

    pub fn get_source_location(&self, idx: NodeIndex) -> Option<SourceLocation> {
        let node = self.ctx.arena.get(idx)?;
        Some(SourceLocation::new(
            self.ctx.file_name.clone(),
            node.pos,
            node.end,
        ))
    }
}
