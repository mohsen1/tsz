//! Path mapping resolution from tsconfig `paths` and `baseUrl`.

use super::{ModuleExtension, ModuleResolver, ResolvedModule};
use crate::module_resolver_helpers::split_path_extension;
use std::path::Path;

pub(super) struct PathMappingAttempt {
    pub resolved: Option<ResolvedModule>,
    pub attempted: bool,
}

impl ModuleResolver {
    /// Try resolving through path mappings
    pub(super) fn try_path_mappings(
        &self,
        specifier: &str,
        _containing_dir: &Path,
    ) -> PathMappingAttempt {
        // Sort path mappings by specificity (most specific first)
        let mut sorted_mappings: Vec<_> = self.path_mappings.iter().collect();
        sorted_mappings.sort_by_key(|b| std::cmp::Reverse(b.specificity()));

        let mut attempted = false;
        for mapping in sorted_mappings {
            if let Some(star_match) = mapping.match_specifier(specifier) {
                attempted = true;
                // Try each target path
                for target in &mapping.targets {
                    let substituted = if target.contains('*') {
                        target.replace('*', &star_match)
                    } else {
                        target.clone()
                    };
                    if Self::has_path_mapping_target_extension(&substituted) {
                        continue;
                    }

                    let base = self
                        .base_url
                        .as_deref()
                        .expect("path mappings require baseUrl for attempted resolution");
                    let candidate = base.join(&substituted);

                    if let Some(resolved) = self.try_file_or_directory(&candidate) {
                        return PathMappingAttempt {
                            resolved: Some(ResolvedModule {
                                resolved_path: resolved,
                                resolved_using_ts_extension: false,
                                is_external: false,
                                package_name: None,
                                original_specifier: specifier.to_string(),
                                extension: ModuleExtension::from_path(&candidate),
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

    pub(super) fn has_path_mapping_target_extension(target: &str) -> bool {
        let base_path = std::path::Path::new(target);
        split_path_extension(base_path).is_some()
    }
}
