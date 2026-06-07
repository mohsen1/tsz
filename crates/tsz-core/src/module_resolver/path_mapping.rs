//! Path mapping resolution from tsconfig `paths` and `baseUrl`.

use super::{ModuleExtension, ModuleResolver, PackageType, ResolvedModule};
use crate::config::PathMapping;
use std::path::Path;

pub(super) struct PathMappingAttempt {
    pub resolved: Option<ResolvedModule>,
    pub attempted: bool,
}

impl ModuleResolver {
    /// Try resolving through path mappings.
    ///
    /// tsc selects exactly **one** `paths` pattern for a specifier
    /// (`matchPatternOrExact` -> `findBestPatternMatch`, shared with the CLI
    /// driver via [`PathMapping::select_best`]): an exact, wildcard-free key
    /// equal to the specifier wins outright; otherwise the matching wildcard
    /// with the longest prefix is chosen. Only that single pattern's targets are
    /// probed (in declaration order, first on-disk hit wins). tsc never falls
    /// through to a *less specific* pattern when the chosen pattern's targets are
    /// missing on disk — so neither do we. Previously this method iterated every
    /// matching pattern and returned the first that resolved, which let a missing
    /// target under a specific pattern silently fall through to a catch-all
    /// (`"*"`), resolving where tsc reports `TS2307`.
    ///
    /// Path mapping targets are probed regardless of whether the substituted
    /// target already has an explicit extension. Targets with explicit
    /// extensions (e.g. `"./foo.d.ts"`, `"./lib/*.ts"`) are checked as-is;
    /// `try_file_or_directory` handles extension substitution and
    /// declaration-sidecar probing the same way it does for any other path.
    ///
    /// `base` is the resolved `baseUrl` directory; callers must only invoke this
    /// method when `base_url` is set (the type system enforces it via `&Path`).
    pub(super) fn try_path_mappings(
        &self,
        specifier: &str,
        base: &Path,
        importer_package_type: Option<PackageType>,
    ) -> PathMappingAttempt {
        let Some((mapping_idx, star_match)) =
            PathMapping::select_best(&self.path_mappings, specifier)
        else {
            return PathMappingAttempt {
                resolved: None,
                attempted: false,
            };
        };
        let mapping = &self.path_mappings[mapping_idx];

        // Only the chosen pattern's targets are probed; a miss does not fall
        // through to any other pattern.
        for target in &mapping.targets {
            // tsc's `tryLoadModuleUsingPaths` replaces only the first `*` of a
            // wildcard target and uses an exact-key target verbatim; the shared
            // `PathMapping::substitute_target` owns that rule so this resolver
            // and the CLI driver cannot drift.
            let substituted = mapping.substitute_target(target, &star_match);
            let candidate = base.join(&substituted);

            if let Some(resolved) = self.try_file_or_directory(&candidate, importer_package_type) {
                let extension = ModuleExtension::from_path(&resolved);
                return PathMappingAttempt {
                    resolved: Some(ResolvedModule {
                        resolved_path: resolved,
                        resolved_using_ts_extension: false,
                        is_external: false,
                        package_name: None,
                        original_specifier: specifier.to_string(),
                        extension,
                    }),
                    attempted: true,
                };
            }
        }

        PathMappingAttempt {
            resolved: None,
            attempted: true,
        }
    }
}
