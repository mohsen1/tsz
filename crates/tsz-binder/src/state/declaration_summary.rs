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
    export_surface: ExportSurface,
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

    /// Build declaration facts from an already-computed exported surface.
    ///
    /// This is a compatibility bridge for callers that have not moved to
    /// `from_binder()` yet; new facts should still be added to
    /// `DeclarationSummary` query methods rather than exposing `ExportSurface`
    /// directly to emit callers.
    pub const fn from_export_surface(export_surface: ExportSurface) -> Self {
        Self { export_surface }
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

    /// Check whether a name is directly exported from this file.
    pub fn is_exported(&self, name: &str) -> bool {
        self.export_surface.is_exported(name)
    }

    /// Check whether a direct export is type-only.
    pub fn is_type_only_export(&self, name: &str) -> bool {
        self.export_surface.is_type_only_export(name)
    }

    /// Return the total number of unique public API entries.
    pub fn public_api_size(&self) -> usize {
        self.export_surface.public_api_size()
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
        assert_eq!(summary.public_api_size(), 0);
    }

    #[test]
    fn summary_exposes_export_surface_overload_facts() {
        let mut export_surface = ExportSurface::default();
        export_surface
            .overloaded_functions
            .insert("parse".to_string());
        export_surface.has_public_api_scope = true;
        let summary = DeclarationSummary::from_export_surface(export_surface);

        assert!(summary.overloaded_functions().contains("parse"));
        assert!(summary.has_public_api_scope());
    }

    #[test]
    fn summary_wraps_export_surface_queries() {
        let mut export_surface = ExportSurface::default();
        export_surface.module_exports.insert(
            "PublicType".to_string(),
            crate::ExportedSymbol {
                symbol_id: crate::SymbolId(1),
                flags: 0,
                is_type_only: true,
            },
        );
        let summary = DeclarationSummary::from_export_surface(export_surface);

        assert!(summary.is_exported("PublicType"));
        assert!(summary.is_type_only_export("PublicType"));
        assert_eq!(summary.public_api_size(), 1);
    }
}
