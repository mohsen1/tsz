//! Gate that decides whether
//! [`delegate_cross_arena_symbol_resolution`](super::cross_file::CheckerState::delegate_cross_arena_symbol_resolution)
//! may share a symbol-type result across file checkers via the
//! `DefinitionStore` cache, skipping a fresh child-checker construction.
//! See [`classify_declaration_file_for_cache`] for the per-file-kind policy.

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

        let cacheable_outcome = match classify_declaration_file_for_cache(
            &source_file.file_name,
            source_file.is_declaration_file,
        ) {
            DeclarationFileCacheClass::DomOrExternalPackage => Outcome::CacheableDeclarationFile,
            DeclarationFileCacheClass::UserSource => Outcome::Cacheable,
            DeclarationFileCacheClass::NonDomBuiltinLib => {
                record(Outcome::TargetDeclarationFile);
                return None;
            }
        };

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

        record(cacheable_outcome);
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
                "{file_name}",
            );
        }
    }

    #[test]
    fn external_package_paths_with_separator_variants_route_through_cache() {
        for file_name in [
            "node_modules/.pnpm/react@18.2.0/node_modules/react/index.d.ts",
            "/repo/node_modules/.pnpm/lodash@4.17.21/node_modules/lodash/index.d.ts",
            r"C:\repo\node_modules\@scope\pkg\sub\types.d.ts",
        ] {
            assert_eq!(
                classify_declaration_file_for_cache(file_name, true),
                DeclarationFileCacheClass::DomOrExternalPackage,
                "{file_name}",
            );
        }
    }

    #[test]
    fn non_dom_builtin_lib_keeps_existing_shared_name_path() {
        for file_name in [
            "lib.es5.d.ts",
            "lib.es2015.d.ts",
            "lib.es2015.core.d.ts",
            "lib.es2020.symbol.wellknown.d.ts",
            "lib.esnext.d.ts",
            "lib.decorators.d.ts",
            "lib.scripthost.d.ts",
            "/repo/node_modules/typescript/lib/lib.es5.d.ts",
            r"C:\repo\node_modules\typescript\lib\lib.es2020.symbol.wellknown.d.ts",
        ] {
            assert_eq!(
                classify_declaration_file_for_cache(file_name, true),
                DeclarationFileCacheClass::NonDomBuiltinLib,
                "{file_name}",
            );
        }
    }

    #[test]
    fn local_declaration_files_outside_node_modules_stay_on_legacy_path() {
        for file_name in [
            "packages/foo/src/types.d.ts",
            "/repo/fixtures/node-modules-like/types.d.ts",
        ] {
            assert_eq!(
                classify_declaration_file_for_cache(file_name, true),
                DeclarationFileCacheClass::NonDomBuiltinLib,
                "{file_name}",
            );
        }
    }

    #[test]
    fn declaration_flag_drives_classification_even_for_lib_named_user_files() {
        // `is_declaration_file` is the source of truth; the file-name check
        // is only a refinement *within* declaration files, so a user `.ts`
        // whose name collides with a lib stem must not be reclassified.
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
