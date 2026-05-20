//! Portability import alias and module export path matching helpers

use super::super::DeclarationEmitter;
use tsz_binder::{BinderState, SymbolId};

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn resolve_portability_import_alias(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
    ) -> Option<SymbolId> {
        let symbol = binder.symbols.get(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS) {
            return None;
        }

        let module_specifier = symbol.import_module.as_deref()?;
        let export_name = symbol
            .import_name
            .as_deref()
            .unwrap_or(symbol.escaped_name.as_str());
        let current_path = self.current_file_path.as_deref()?;

        for module_path in self.matching_module_export_paths(binder, current_path, module_specifier)
        {
            let Some(exports) = binder.module_exports.get(module_path) else {
                continue;
            };
            if let Some(resolved) = exports.get(export_name)
                && resolved != sym_id
            {
                return Some(resolved);
            }
        }

        if !module_specifier.starts_with('.') && !module_specifier.starts_with('/') {
            return binder.symbols.iter().find_map(|candidate| {
                if candidate.id == sym_id || candidate.escaped_name != export_name {
                    return None;
                }
                let source_path = self.get_symbol_source_path(candidate.id, binder)?;
                let package_specifier =
                    self.package_specifier_for_node_modules_path(current_path, &source_path)?;
                if package_specifier == module_specifier
                    || package_specifier.starts_with(&format!("{module_specifier}/"))
                {
                    self.package_root_export_reference_path(
                        candidate.id,
                        &candidate.escaped_name,
                        binder,
                        current_path,
                    )
                    .is_some()
                    .then_some(candidate.id)
                } else {
                    None
                }
            });
        }

        None
    }

    pub(in crate::declaration_emitter) fn canonical_import_alias_reference_name<'b>(
        binder: &'b BinderState,
        sym_id: SymbolId,
        fallback: &'b str,
    ) -> &'b str {
        binder
            .symbols
            .get(sym_id)
            .and_then(|symbol| symbol.import_name.as_deref())
            .unwrap_or(fallback)
    }

    pub(in crate::declaration_emitter) fn matching_module_export_paths<'b>(
        &self,
        binder: &'b BinderState,
        current_path: &str,
        module_specifier: &str,
    ) -> Vec<&'b str> {
        let mut matches: Vec<_> = binder
            .module_exports
            .keys()
            .filter_map(|module_path| {
                let matches = if module_specifier.starts_with('.')
                    || module_specifier.starts_with('/')
                {
                    let relative = self.strip_ts_extensions(
                        &self.calculate_relative_path(current_path, module_path),
                    );
                    relative == module_specifier
                        || relative
                            .strip_suffix("/index")
                            .is_some_and(|without_index| without_index == module_specifier)
                } else {
                    self.node_modules_path_matches_import_specifier(module_path, module_specifier)
                        || self.path_mapping_module_path_matches_import_specifier(
                            module_path,
                            module_specifier,
                        )
                };
                matches.then_some(module_path.as_str())
            })
            .collect();

        matches.sort_by(|left, right| {
            self.module_export_path_rank(left, module_specifier)
                .cmp(&self.module_export_path_rank(right, module_specifier))
                .then_with(|| left.cmp(right))
        });
        matches
    }

    fn path_mapping_module_path_matches_import_specifier(
        &self,
        module_path: &str,
        module_specifier: &str,
    ) -> bool {
        if module_specifier.starts_with('.') || module_specifier.starts_with('/') {
            return false;
        }

        let mut parts = module_specifier.split('/').collect::<Vec<_>>();
        if parts.is_empty() {
            return false;
        }

        let package_name = if parts[0].starts_with('@') {
            if parts.len() < 2 {
                return false;
            }
            let package = parts[1].to_string();
            parts.drain(0..2);
            package
        } else {
            parts.remove(0).to_string()
        };
        let subpath = parts.join("/");

        let normalized = self.strip_ts_extensions(module_path).replace('\\', "/");
        if subpath.is_empty() {
            return normalized.ends_with(&format!("/{package_name}/src/index"))
                || normalized.ends_with(&format!("/{package_name}/index"));
        }

        normalized.ends_with(&format!("/{package_name}/src/{subpath}"))
            || normalized.ends_with(&format!("/{package_name}/{subpath}"))
    }

    pub(in crate::declaration_emitter) fn node_modules_path_matches_import_specifier(
        &self,
        module_path: &str,
        module_specifier: &str,
    ) -> bool {
        use std::path::{Component, Path};

        let components: Vec<_> = Path::new(module_path).components().collect();
        components
            .iter()
            .enumerate()
            .filter_map(|(idx, component)| {
                matches!(component, Component::Normal(part) if part.to_str() == Some("node_modules"))
                    .then_some(idx)
            })
            .any(|nm_idx| {
                let pkg_start = nm_idx + 1;
                let pkg_len = if components.get(pkg_start).is_some_and(|component| {
                    matches!(component, Component::Normal(part) if part.to_str().is_some_and(|text| text.starts_with('@')))
                }) {
                    2
                } else {
                    1
                };
                if components.len() < pkg_start + pkg_len {
                    return false;
                }

                let package_name = components[pkg_start..pkg_start + pkg_len]
                    .iter()
                    .filter_map(|component| match component {
                        Component::Normal(part) => part.to_str(),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("/");

                let subpath_start = pkg_start + pkg_len;
                if subpath_start >= components.len() {
                    return false;
                }

                let relative_path = components[subpath_start..]
                    .iter()
                    .filter_map(|component| match component {
                        Component::Normal(part) => part.to_str(),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("/");
                let Some(runtime_subpath) =
                    self.declaration_runtime_relative_path(&relative_path)
                else {
                    return false;
                };
                let mut runtime_subpath = runtime_subpath.trim_start_matches("./").to_string();
                if runtime_subpath.ends_with("/index.js") {
                    runtime_subpath.truncate(runtime_subpath.len() - "/index.js".len());
                } else if runtime_subpath == "index.js" {
                    runtime_subpath.clear();
                }
                if module_specifier == package_name {
                    if !runtime_subpath.is_empty() {
                        return false;
                    }

                    let package_root = components[..subpath_start].iter().fold(
                        std::path::PathBuf::new(),
                        |mut path, component| {
                            path.push(component.as_os_str());
                            path
                        },
                    );
                    return self
                        .reverse_export_specifier_for_runtime_path(&package_root, "./index.ts")
                        .is_some()
                        || self
                            .reverse_export_specifier_for_runtime_path(&package_root, "./index.d.ts")
                            .is_some()
                        || std::fs::read_to_string(package_root.join("package.json"))
                            .ok()
                            .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
                            .is_none_or(|package_json| package_json.get("exports").is_none());
                }
                let candidate = if runtime_subpath.is_empty() {
                    package_name
                } else {
                    format!("{package_name}/{runtime_subpath}")
                };
                module_specifier == candidate
            })
    }

    pub(in crate::declaration_emitter) fn module_export_path_rank(
        &self,
        module_path: &str,
        module_specifier: &str,
    ) -> (usize, usize) {
        use std::path::{Component, Path};

        if module_specifier.starts_with('.') || module_specifier.starts_with('/') {
            return (0, module_path.len());
        }

        let components: Vec<_> = Path::new(module_path).components().collect();
        let Some(nm_idx) = components.iter().position(|component| {
            matches!(component, Component::Normal(part) if part.to_str() == Some("node_modules"))
        }) else {
            return (usize::MAX, module_path.len());
        };

        let pkg_start = nm_idx + 1;
        let pkg_len = if module_specifier.starts_with('@') {
            2
        } else {
            1
        };
        let depth_after_package = components.len().saturating_sub(pkg_start + pkg_len);
        (depth_after_package, module_path.len())
    }
}
