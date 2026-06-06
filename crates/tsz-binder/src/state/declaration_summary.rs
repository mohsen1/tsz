//! Precomputed declaration-emission facts for a single source file.
//!
//! `DeclarationSummary` is the binder-owned boundary consumed by declaration
//! emit. It groups stable, reusable facts needed while printing `.d.ts` output
//! so the emitter does not rediscover them during the emit walk.

use crate::{BinderState, ExportSurface};
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;

/// Structured declaration-emission facts for one source file.
///
/// The first populated family is the file's exported surface: exported locals,
/// re-exports, public-API scope, and top-level overload grouping. Future DTS
/// summary facts should extend this type instead of adding more ad hoc emitter
/// discovery state.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct DeclarationSummary {
    pub export_surface: ExportSurface,
}

impl DeclarationSummary {
    /// Build declaration facts from binder state and AST structure.
    pub fn from_binder(
        binder: &BinderState,
        arena: &NodeArena,
        file_name: &str,
        root_idx: NodeIndex,
    ) -> Self {
        Self {
            export_surface: ExportSurface::from_binder(binder, arena, file_name, root_idx),
        }
    }

    /// Top-level function overload names that should suppress implementation
    /// signatures during declaration emit.
    pub const fn overloaded_functions(&self) -> &FxHashSet<String> {
        &self.export_surface.overloaded_functions
    }

    /// Whether declaration emit should filter the file to its public API.
    pub const fn has_public_api_scope(&self) -> bool {
        self.export_surface.has_public_api_scope
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_summary_has_no_public_surface_facts() {
        let summary = DeclarationSummary::default();

        assert!(summary.overloaded_functions().is_empty());
        assert!(!summary.has_public_api_scope());
        assert_eq!(summary.export_surface.public_api_size(), 0);
    }

    #[test]
    fn summary_exposes_export_surface_overload_facts() {
        let mut summary = DeclarationSummary::default();
        summary
            .export_surface
            .overloaded_functions
            .insert("parse".to_string());
        summary.export_surface.has_public_api_scope = true;

        assert!(summary.overloaded_functions().contains("parse"));
        assert!(summary.has_public_api_scope());
    }
}
