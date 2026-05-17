//! Cross-file symbol-type cache guard helpers.
//!
//! `symbol_arena_symbol_type_cache_file_idx` decides whether
//! [`delegate_cross_arena_symbol_resolution`](super::cross_file::CheckerState::delegate_cross_arena_symbol_resolution)
//! is allowed to thread its symbol-type read/write through the shared
//! `DefinitionStore` cache. The shared cache, in turn, is what lets repeated
//! file-checker passes over the same project skip the expensive
//! child-checker construction once a symbol has been resolved once.
//!
//! Structural rule: when a symbol's only declaration belongs to a single
//! declaring-file arena that is either a user source file, a DOM-shaped
//! builtin lib file (`lib.dom.*.d.ts`, `lib.webworker.*.d.ts`), or an
//! external package declaration file (`node_modules/**/*.d.ts`), and the
//! program contains no module augmentations, the symbol's type is reusable
//! across every other file checker pointed at the same `DefinitionStore`.
//!
//! Non-DOM builtin lib arenas (`lib.es5`, `lib.es2015`, ...) are *not*
//! routed through this cache because they already have a coarser shared
//! name-keyed cache (`shared_actual_lib_delegation_cache`) one layer up;
//! double-caching them here would only churn the bucket without improving
//! hit rate.

use crate::state::CheckerState;
use crate::state_type_analysis::cross_file_direct::{
    DeclarationFileCacheClass, classify_declaration_file_for_cache,
};
use tsz_binder::SymbolId;
use tsz_common::perf_counters::{
    CrossArenaSymbolMissSource, SourceFileSymbolArenaCacheEligibilityOutcome,
    record_source_file_symbol_arena_cache_eligibility_outcome,
};

impl<'a> CheckerState<'a> {
    pub(super) fn symbol_arena_symbol_type_cache_file_idx(
        &self,
        needs_cross_file_delegation: bool,
        cross_file_idx: Option<usize>,
        delegate_arena_source: CrossArenaSymbolMissSource,
        delegate_arena: Option<&tsz_parser::NodeArena>,
        sym_id: SymbolId,
    ) -> Option<usize> {
        use SourceFileSymbolArenaCacheEligibilityOutcome as Outcome;
        let record = |outcome: Outcome| {
            record_source_file_symbol_arena_cache_eligibility_outcome(outcome);
        };

        if needs_cross_file_delegation {
            record(Outcome::CrossFileTarget);
            return cross_file_idx;
        }
        if delegate_arena_source != CrossArenaSymbolMissSource::SymbolArena {
            record(Outcome::NonSymbolArena);
            return None;
        }
        if self.ctx.program_has_module_augmentations() {
            record(Outcome::ModuleAugmentation);
            return None;
        }

        let Some(arena) = delegate_arena else {
            record(Outcome::MissingDelegateArena);
            return None;
        };
        if std::ptr::eq(arena, self.ctx.arena) {
            record(Outcome::CurrentArena);
            return None;
        }
        let Some(source_file) = arena.source_files.first() else {
            record(Outcome::MissingSourceFile);
            return None;
        };

        let declaration_file_class = classify_declaration_file_for_cache(
            &source_file.file_name,
            source_file.is_declaration_file,
        );
        if declaration_file_class == DeclarationFileCacheClass::NonDomBuiltinLib {
            record(Outcome::TargetDeclarationFile);
            return None;
        }

        let outcome = self
            .ctx
            .source_file_symbol_arena_cache_stability_outcome(sym_id, arena);
        if outcome != Outcome::Cacheable {
            record(outcome);
            return None;
        }

        let Some(file_idx) = self.ctx.get_file_idx_for_arena(arena) else {
            record(Outcome::MissingFileIndex);
            return None;
        };

        let recorded = match declaration_file_class {
            DeclarationFileCacheClass::DomOrExternalPackage => Outcome::CacheableDeclarationFile,
            DeclarationFileCacheClass::UserSource => Outcome::Cacheable,
            // Short-circuited at the early guard above.
            DeclarationFileCacheClass::NonDomBuiltinLib => {
                unreachable!("non-DOM builtin lib was rejected by the early guard")
            }
        };
        record(recorded);
        Some(file_idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_ts_files_are_classified_as_user_source() {
        assert_eq!(
            classify_declaration_file_for_cache("src/main.ts", false),
            DeclarationFileCacheClass::UserSource,
        );
        assert_eq!(
            classify_declaration_file_for_cache("packages/foo/index.tsx", false),
            DeclarationFileCacheClass::UserSource,
        );
    }

    #[test]
    fn dom_like_lib_files_are_cacheable_declaration_files() {
        // Cover all of the DOM-family stems so a future name expansion has
        // to update the classifier on purpose, not by accident.
        for file_name in [
            "lib.dom.d.ts",
            "lib.dom.iterable.d.ts",
            "lib.dom.asynciterable.d.ts",
            "lib.webworker.d.ts",
            "lib.webworker.iterable.d.ts",
            "dom.generated.d.ts",
            "webworker.asynciterable.d.ts",
        ] {
            assert_eq!(
                classify_declaration_file_for_cache(file_name, true),
                DeclarationFileCacheClass::DomOrExternalPackage,
                "{file_name} should land in DomOrExternalPackage",
            );
        }
    }

    #[test]
    fn external_package_paths_with_separator_variants_route_through_cache() {
        // Beyond the cases already covered by `detects_external_package_declaration_paths`
        // in `cross_file_direct_tests.rs`, exercise pnpm-style nested layouts and
        // mixed separator scenarios that the cache routing depends on.
        for file_name in [
            "node_modules/.pnpm/react@18.2.0/node_modules/react/index.d.ts",
            "/repo/node_modules/.pnpm/lodash@4.17.21/node_modules/lodash/index.d.ts",
            r"C:\repo\node_modules\@scope\pkg\sub\types.d.ts",
        ] {
            assert_eq!(
                classify_declaration_file_for_cache(file_name, true),
                DeclarationFileCacheClass::DomOrExternalPackage,
                "{file_name} should route through the cross-arena cache",
            );
        }
    }

    #[test]
    fn non_dom_builtin_lib_keeps_existing_shared_name_path() {
        // Each of these lives in the legacy shared name-keyed cache.
        // Reclassifying them here would double-cache the same symbols.
        for file_name in [
            "lib.es5.d.ts",
            "lib.es2015.d.ts",
            "lib.es2015.core.d.ts",
            "lib.es2020.symbol.wellknown.d.ts",
            "lib.esnext.d.ts",
            "lib.decorators.d.ts",
            "lib.scripthost.d.ts",
        ] {
            assert_eq!(
                classify_declaration_file_for_cache(file_name, true),
                DeclarationFileCacheClass::NonDomBuiltinLib,
                "{file_name} must keep using the shared name-keyed cache",
            );
        }
    }

    #[test]
    fn local_declaration_files_outside_node_modules_stay_on_legacy_path() {
        // Local `.d.ts` files that aren't a builtin lib stem and aren't in
        // `node_modules/` still go through the existing child-checker path.
        // This guards against accidentally widening the cache to user
        // `.d.ts` files where the stability invariants haven't been audited.
        for file_name in [
            "packages/foo/src/types.d.ts",
            "/repo/fixtures/node-modules-like/types.d.ts",
        ] {
            assert_eq!(
                classify_declaration_file_for_cache(file_name, true),
                DeclarationFileCacheClass::NonDomBuiltinLib,
                "{file_name} should not be classified as cacheable yet",
            );
        }
    }

    #[test]
    fn declaration_flag_drives_classification_even_for_lib_named_user_files() {
        // The `is_declaration_file` flag is the source of truth; the file-name
        // check is only a refinement *within* declaration files.
        assert_eq!(
            classify_declaration_file_for_cache("lib.dom.d.ts", false),
            DeclarationFileCacheClass::UserSource,
        );
        assert_eq!(
            classify_declaration_file_for_cache("node_modules/react/x.ts", false),
            DeclarationFileCacheClass::UserSource,
        );
    }
}
