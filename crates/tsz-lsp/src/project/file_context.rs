//! Borrowed view of a `ProjectFile`'s common provider inputs.
//!
//! LSP feature dispatch (`Project::get_*`, `Project::resolve_*`, etc.) constructs
//! many providers via the same five-argument shape:
//!
//! ```ignore
//! Provider::new(
//!     file.arena(),
//!     file.binder(),
//!     file.line_map(),
//!     file.file_name().to_string(),
//!     file.source_text(),
//! );
//! ```
//!
//! [`LspProviderContext`] bundles those five inputs into a single borrowed view
//! so binder-tier providers can be built via `Provider::from_context(ctx)`
//! instead of repeating the five accessors at every call site. The shape mirrors
//! the `binder` arm of the `define_lsp_provider!` macro exactly.
//!
//! See [`super::ProjectFile::provider_context`] for the construction entry point.
//! Only binder-tier providers consume this context; minimal-tier providers (which
//! omit the binder and file name) and full-tier providers (which add the type
//! interner, strict, sound mode, and lib contexts) will get their own
//! context views in follow-up DRY slices.
//!
//! # Borrow checker note
//!
//! [`super::ProjectFile::provider_context`] takes `&self`, so the resulting
//! context (and any provider built from it) holds a single immutable borrow of
//! the whole `ProjectFile`. Sites that also need `&mut file.scope_cache`
//! alongside the provider (e.g. `Project::get_definition`) must keep using the
//! flat per-field destructuring pattern so the borrow checker can track the
//! disjoint fields independently. Those sites are explicitly out of scope for
//! this DRY slice.

use tsz_binder::BinderState;
use tsz_common::position::LineMap;
use tsz_parser::parser::node::NodeArena;

/// Borrowed view of a `ProjectFile` shaped for binder-tier LSP providers.
///
/// All fields are borrowed from the underlying [`super::ProjectFile`]. The
/// `file_name` is exposed as `&str` so that callers (or the macro-generated
/// `from_context` constructor) can clone into the owned `String` the provider
/// keeps internally.
#[derive(Clone, Copy)]
pub struct LspProviderContext<'a> {
    /// Parser arena shared by all providers for the file.
    pub arena: &'a NodeArena,
    /// Binder output (symbols, scopes, flow graph) for the file.
    pub binder: &'a BinderState,
    /// Line map for offset/position translation.
    pub line_map: &'a LineMap,
    /// File name used for LSP locations and diagnostics.
    pub file_name: &'a str,
    /// Original source text for the file.
    pub source_text: &'a str,
}

/// Borrowed view of a `ProjectFile` shaped for minimal-tier LSP providers.
///
/// Minimal-tier providers (folding ranges, document symbols, document
/// colors, document links, selection ranges) walk the AST without
/// consulting the binder. They take exactly the three fields listed below;
/// this view bundles them so the providers can be built via
/// `Provider::from_context(ctx)` instead of repeating the three accessors
/// at every dispatch site.
///
/// See [`super::ProjectFile::minimal_provider_context`] for the
/// construction entry point.
#[derive(Clone, Copy)]
pub struct LspMinimalProviderContext<'a> {
    /// Parser arena shared by all providers for the file.
    pub arena: &'a NodeArena,
    /// Line map for offset/position translation.
    pub line_map: &'a LineMap,
    /// Original source text for the file.
    pub source_text: &'a str,
}

#[cfg(test)]
mod tests {
    use super::super::{Project, ProjectFile};

    fn fixture(name: &str, source: &str) -> ProjectFile {
        let mut project = Project::new();
        project.set_file(name.to_string(), source.to_string());
        project
            .files
            .remove(name)
            .expect("project::set_file inserts the file under its given name")
    }

    /// Sanity check: the borrowed view returned by
    /// `ProjectFile::provider_context` exposes the same five fields the
    /// individual file accessors expose, so the
    /// `define_lsp_provider!(binder ...)` macro's `from_context` constructor
    /// produces a provider identical to one built from the per-field accessors.
    #[test]
    fn provider_context_matches_individual_accessors() {
        let file = fixture("a.ts", "const x = 1;");

        let ctx = file.provider_context();

        // Pointer-identity for the borrowed members.
        assert!(std::ptr::eq(ctx.arena, file.arena()));
        assert!(std::ptr::eq(ctx.binder, file.binder()));
        assert!(std::ptr::eq(ctx.line_map, file.line_map()));
        assert!(std::ptr::eq(
            ctx.source_text.as_ptr(),
            file.source_text().as_ptr(),
        ));
        assert_eq!(ctx.file_name, file.file_name());
    }

    /// `LspProviderContext` is `Copy`, so feature dispatch can construct
    /// multiple providers from a single `file.provider_context()` call.
    #[test]
    fn provider_context_is_copy() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<super::LspProviderContext<'_>>();

        let file = fixture("b.ts", "const y = 2;");
        let ctx = file.provider_context();
        let ctx2 = ctx; // does not move
        assert_eq!(ctx.file_name, ctx2.file_name);
    }
}
