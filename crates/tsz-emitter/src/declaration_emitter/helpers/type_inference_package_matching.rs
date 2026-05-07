use super::super::DeclarationEmitter;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn package_json_name_matches_import_specifier(
        &self,
        module_path: &str,
        module_specifier: &str,
    ) -> bool {
        let mut current = std::path::Path::new(module_path).parent();
        while let Some(dir) = current {
            let package_json_path = dir.join("package.json");
            if let Ok(content) = std::fs::read_to_string(&package_json_path)
                && let Ok(package_json) = serde_json::from_str::<serde_json::Value>(&content)
            {
                let name_matches = package_json.get("name").and_then(|name| name.as_str())
                    == Some(module_specifier);
                let no_exports = package_json.get("exports").is_none();
                return name_matches && no_exports;
            }
            current = dir.parent();
        }
        false
    }

    pub(in crate::declaration_emitter) fn rewrite_relative_import_type_specifiers(
        type_text: &str,
        module_specifier: &str,
    ) -> String {
        let mut result = String::new();
        let mut rest = type_text;
        while let Some((start, specifier, tail)) = Self::next_import_type_text(rest) {
            result.push_str(&rest[..start]);
            if specifier.starts_with('.') || specifier.starts_with('/') {
                result.push_str("import(\"");
                result.push_str(module_specifier);
                result.push_str("\")");
            } else {
                let import_len = rest.len() - tail.len() - start;
                result.push_str(&rest[start..start + import_len]);
            }
            rest = tail;
        }
        result.push_str(rest);
        result
    }

    pub(in crate::declaration_emitter) fn node_modules_package_contains_import_specifier(
        &self,
        module_path: &str,
        module_specifier: &str,
    ) -> bool {
        use std::path::{Component, Path, PathBuf};

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
                if components.len() <= pkg_start + pkg_len {
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
                if package_name != module_specifier {
                    return false;
                }

                let package_root = components[..pkg_start + pkg_len].iter().fold(
                    PathBuf::new(),
                    |mut path, component| {
                        path.push(component.as_os_str());
                        path
                    },
                );
                std::fs::read_to_string(package_root.join("package.json"))
                    .ok()
                    .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
                    .is_none_or(|package_json| package_json.get("exports").is_none())
            })
    }
}
