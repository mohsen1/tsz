//! Macro to reduce boilerplate in LSP provider struct definitions.
//!
//! LSP providers fall into three tiers based on what they need:
//! - `minimal`: arena, `line_map`, `source_text`
//! - `binder`: arena, binder, `line_map`, `file_name`, `source_text`
//! - `full`: arena, binder, `line_map`, interner, `source_text`, `file_name`, strict, `sound_mode`, lib contexts

/// Shared configuration for type-aware LSP providers.
#[derive(Clone, Copy, Default)]
pub struct FullProviderOptions<'a> {
    pub strict: bool,
    pub sound_mode: bool,
    pub lib_contexts: &'a [tsz_checker::context::LibContext],
}

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
/// Fields: `arena`, `binder`, `line_map`, `interner`, `source_text`, `file_name`, `strict`, `sound_mode`, `lib_contexts`
/// Generates `new()` (strict=false, `sound_mode=false`), `with_strict()`, and `with_options()`.
macro_rules! define_lsp_provider {
    // ── Tier 3: minimal ──────────────────────────────────────────────
    (minimal $name:ident, $doc:expr) => {
        #[doc = $doc]
        pub struct $name<'a> {
            arena: &'a tsz_parser::parser::node::NodeArena,
            line_map: &'a tsz_common::position::LineMap,
            source_text: &'a str,
        }

        impl<'a> $name<'a> {
            pub const fn new(
                arena: &'a tsz_parser::parser::node::NodeArena,
                line_map: &'a tsz_common::position::LineMap,
                source_text: &'a str,
            ) -> Self {
                Self {
                    arena,
                    line_map,
                    source_text,
                }
            }

            /// Build the provider from a borrowed
            /// [`crate::project::LspMinimalProviderContext`].
            ///
            /// Convenience constructor for LSP feature dispatch in
            /// `crate::project::features` and `crate::project::operations`,
            /// which already have a `ProjectFile` in scope and can call
            /// `file.minimal_provider_context()`.
            pub const fn from_context(ctx: crate::project::LspMinimalProviderContext<'a>) -> Self {
                Self {
                    arena: ctx.arena,
                    line_map: ctx.line_map,
                    source_text: ctx.source_text,
                }
            }
        }
    };

    // ── Tier 2: binder ───────────────────────────────────────────────
    (binder $name:ident, $doc:expr) => {
        #[doc = $doc]
        pub struct $name<'a> {
            arena: &'a tsz_parser::parser::node::NodeArena,
            binder: &'a tsz_binder::BinderState,
            line_map: &'a tsz_common::position::LineMap,
            file_name: String,
            source_text: &'a str,
        }

        impl<'a> $name<'a> {
            pub const fn new(
                arena: &'a tsz_parser::parser::node::NodeArena,
                binder: &'a tsz_binder::BinderState,
                line_map: &'a tsz_common::position::LineMap,
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

            /// Build the provider from a borrowed [`crate::project::LspProviderContext`].
            ///
            /// Convenience constructor for LSP feature dispatch in
            /// `crate::project::features` and `crate::project::operations`,
            /// which already have a `ProjectFile` in scope and can call
            /// `file.provider_context()`. The `file_name` field is cloned
            /// into the owned `String` the provider stores internally.
            pub fn from_context(ctx: crate::project::LspProviderContext<'a>) -> Self {
                Self {
                    arena: ctx.arena,
                    binder: ctx.binder,
                    line_map: ctx.line_map,
                    file_name: ctx.file_name.to_string(),
                    source_text: ctx.source_text,
                }
            }
        }
    };

    // ── Tier 1: full (with type checking) ────────────────────────────
    (full $name:ident, $doc:expr) => {
        #[doc = $doc]
        pub struct $name<'a> {
            arena: &'a tsz_parser::parser::node::NodeArena,
            binder: &'a tsz_binder::BinderState,
            line_map: &'a tsz_common::position::LineMap,
            interner: &'a tsz_solver::TypeInterner,
            source_text: &'a str,
            file_name: String,
            strict: bool,
            sound_mode: bool,
            lib_contexts: &'a [tsz_checker::context::LibContext],
        }

        impl<'a> $name<'a> {
            pub const fn new(
                arena: &'a tsz_parser::parser::node::NodeArena,
                binder: &'a tsz_binder::BinderState,
                line_map: &'a tsz_common::position::LineMap,
                interner: &'a tsz_solver::TypeInterner,
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
                    sound_mode: false,
                    lib_contexts: &[],
                }
            }

            pub const fn with_strict(
                arena: &'a tsz_parser::parser::node::NodeArena,
                binder: &'a tsz_binder::BinderState,
                line_map: &'a tsz_common::position::LineMap,
                interner: &'a tsz_solver::TypeInterner,
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
                    sound_mode: false,
                    lib_contexts: &[],
                }
            }

            pub const fn with_options(
                arena: &'a tsz_parser::parser::node::NodeArena,
                binder: &'a tsz_binder::BinderState,
                line_map: &'a tsz_common::position::LineMap,
                interner: &'a tsz_solver::TypeInterner,
                source_text: &'a str,
                file_name: String,
                strict: bool,
                sound_mode: bool,
            ) -> Self {
                Self {
                    arena,
                    binder,
                    line_map,
                    interner,
                    source_text,
                    file_name,
                    strict,
                    sound_mode,
                    lib_contexts: &[],
                }
            }

            pub const fn with_options_and_lib_contexts(
                arena: &'a tsz_parser::parser::node::NodeArena,
                binder: &'a tsz_binder::BinderState,
                line_map: &'a tsz_common::position::LineMap,
                interner: &'a tsz_solver::TypeInterner,
                source_text: &'a str,
                file_name: String,
                options: crate::provider_macro::FullProviderOptions<'a>,
            ) -> Self {
                Self {
                    arena,
                    binder,
                    line_map,
                    interner,
                    source_text,
                    file_name,
                    strict: options.strict,
                    sound_mode: options.sound_mode,
                    lib_contexts: options.lib_contexts,
                }
            }

            fn apply_lib_contexts(&self, checker: &mut tsz_checker::CheckerState<'_>) {
                if !self.lib_contexts.is_empty() {
                    checker.ctx.set_lib_contexts(self.lib_contexts.to_vec());
                }
            }
        }
    };
}

// The macro is exported via #[macro_use] on the module in mod.rs.
