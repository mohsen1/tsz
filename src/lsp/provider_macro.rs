//! Macro to reduce boilerplate in LSP provider struct definitions.
//!
//! LSP providers fall into three tiers based on what they need:
//! - `minimal`: arena, line_map, source_text
//! - `binder`: arena, binder, line_map, file_name, source_text
//! - `full`: arena, binder, line_map, interner, source_text, file_name, strict

/// Define an LSP provider struct with standard fields and constructors.
///
/// # Tiers
///
/// **`minimal`** — AST-only providers (folding, symbols, selection, etc.)
/// ```ignore
/// define_lsp_provider!(minimal FoldingRangeProvider, "Provider for folding ranges.");
/// ```
/// Fields: `arena`, `line_map`, `source_text`
///
/// **`binder`** — providers that need binder but not type checking
/// ```ignore
/// define_lsp_provider!(binder RenameProvider, "Rename provider.");
/// ```
/// Fields: `arena`, `binder`, `line_map`, `file_name`, `source_text`
///
/// **`full`** — providers that need type checking
/// ```ignore
/// define_lsp_provider!(full HoverProvider, "Hover provider.");
/// ```
/// Fields: `arena`, `binder`, `line_map`, `interner`, `source_text`, `file_name`, `strict`
/// Generates both `new()` (strict=false) and `with_strict()`.
macro_rules! define_lsp_provider {
    // ── Tier 3: minimal ──────────────────────────────────────────────
    (minimal $name:ident, $doc:expr) => {
        #[doc = $doc]
        pub struct $name<'a> {
            arena: &'a crate::parser::node::NodeArena,
            line_map: &'a crate::lsp::position::LineMap,
            source_text: &'a str,
        }

        impl<'a> $name<'a> {
            pub fn new(
                arena: &'a crate::parser::node::NodeArena,
                line_map: &'a crate::lsp::position::LineMap,
                source_text: &'a str,
            ) -> Self {
                Self {
                    arena,
                    line_map,
                    source_text,
                }
            }
        }
    };

    // ── Tier 2: binder ───────────────────────────────────────────────
    (binder $name:ident, $doc:expr) => {
        #[doc = $doc]
        pub struct $name<'a> {
            arena: &'a crate::parser::node::NodeArena,
            binder: &'a crate::binder::BinderState,
            line_map: &'a crate::lsp::position::LineMap,
            file_name: String,
            source_text: &'a str,
        }

        impl<'a> $name<'a> {
            pub fn new(
                arena: &'a crate::parser::node::NodeArena,
                binder: &'a crate::binder::BinderState,
                line_map: &'a crate::lsp::position::LineMap,
                file_name: String,
                source_text: &'a str,
            ) -> Self {
                Self {
                    arena,
                    binder,
                    line_map,
                    file_name,
                    source_text,
                }
            }
        }
    };

    // ── Tier 1: full (with type checking) ────────────────────────────
    (full $name:ident, $doc:expr) => {
        #[doc = $doc]
        pub struct $name<'a> {
            arena: &'a crate::parser::node::NodeArena,
            binder: &'a crate::binder::BinderState,
            line_map: &'a crate::lsp::position::LineMap,
            interner: &'a crate::solver::TypeInterner,
            source_text: &'a str,
            file_name: String,
            strict: bool,
        }

        impl<'a> $name<'a> {
            pub fn new(
                arena: &'a crate::parser::node::NodeArena,
                binder: &'a crate::binder::BinderState,
                line_map: &'a crate::lsp::position::LineMap,
                interner: &'a crate::solver::TypeInterner,
                source_text: &'a str,
                file_name: String,
            ) -> Self {
                Self {
                    arena,
                    binder,
                    line_map,
                    interner,
                    source_text,
                    file_name,
                    strict: false,
                }
            }

            pub fn with_strict(
                arena: &'a crate::parser::node::NodeArena,
                binder: &'a crate::binder::BinderState,
                line_map: &'a crate::lsp::position::LineMap,
                interner: &'a crate::solver::TypeInterner,
                source_text: &'a str,
                file_name: String,
                strict: bool,
            ) -> Self {
                Self {
                    arena,
                    binder,
                    line_map,
                    interner,
                    source_text,
                    file_name,
                    strict,
                }
            }
        }
    };
}

// The macro is exported via #[macro_use] on the module in mod.rs.
