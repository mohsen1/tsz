//! Path mapping resolution from tsconfig `paths` and `baseUrl`.

use super::{ModuleExtension, ModuleResolver, ResolvedModule};
use crate::resolution::helpers::apply_wildcard_substitution;
use std::path::Path;

pub(super) struct PathMappingAttempt {
    pub resolved: Option<ResolvedModule>,
    pub attempted: bool,
}

impl ModuleResolver {
    /// Try resolving through path mappings.
    ///
    /// tsc resolves path mapping targets by probing the substituted path
    /// (relative to `baseUrl`) with its normal file/directory lookup, regardless
    /// of whether the substituted target already has an explicit extension.
    /// Targets with explicit extensions (e.g. `"./foo.d.ts"`, `"./lib/*.ts"`)
    /// are checked as-is; `try_file_or_directory` handles extension substitution
    /// and declaration-sidecar probing the same way it does for any other path.
    ///
    /// `base` is the resolved `baseUrl` directory; callers must only invoke this
    /// method when `base_url` is set (the type system enforces it via `&Path`).
    pub(super) fn try_path_mappings(&self, specifier: &str, base: &Path) -> PathMappingAttempt {
        // `self.path_mappings` is pre-sorted by specificity descending at
        // construction time (`build_path_mappings` in config/mod.rs).
        let mut attempted = false;
        for mapping in &self.path_mappings {
            if let Some(star_match) = mapping.match_specifier(specifier) {
                attempted = true;
                for target in &mapping.targets {
                    let substituted = apply_wildcard_substitution(target, &star_match, false);
                    let candidate = base.join(&substituted);

                    if let Some(resolved) = self.try_file_or_directory(&candidate) {
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
                            attempted,
                        };
                    }
                }
            }
        }

        PathMappingAttempt {
            resolved: None,
            attempted,
        }
    }
}
