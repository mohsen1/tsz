use crate::state::CheckerState;
use crate::symbols_domain::name_text::entity_name_text_in_arena;
use tsz_common::checker_options::JsxMode;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

impl<'a> CheckerState<'a> {
    pub(crate) fn is_jsx_import_source_runtime_bridge_alias(
        &self,
        decl_arena: &tsz_parser::parser::node::NodeArena,
        type_node: NodeIndex,
    ) -> bool {
        let Some(node) = decl_arena.get(type_node) else {
            return false;
        };
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = decl_arena.get_type_ref(node) else {
            return false;
        };
        if !entity_name_text_in_arena(decl_arena, type_ref.type_name)
            .is_some_and(|name| name.starts_with("JSX."))
        {
            return false;
        }

        let normalized_file_name = decl_arena
            .source_files
            .first()
            .map(|source_file| source_file.file_name.replace('\\', "/"));
        let Some(normalized_file_name) = normalized_file_name else {
            return false;
        };

        self.jsx_import_source_runtime_candidates()
            .into_iter()
            .any(|(source, runtime_suffix)| {
                if source.starts_with('/') {
                    return false;
                }

                let mut package_roots = vec![source.clone()];
                if let Some(types_root) = types_package_root_for_jsx_import_source(&source) {
                    package_roots.push(types_root);
                }

                package_roots.iter().any(|package_root| {
                    jsx_runtime_path_matches(&normalized_file_name, package_root, runtime_suffix)
                })
            })
    }

    fn jsx_import_source_runtime_candidates(&self) -> Vec<(String, &'static str)> {
        let mut candidates: Vec<(String, &'static str)> = Vec::new();
        let mut push_candidate = |source: String, runtime_suffix: &'static str| {
            if !candidates.iter().any(|(existing_source, existing_suffix)| {
                existing_source == &source && *existing_suffix == runtime_suffix
            }) {
                candidates.push((source, runtime_suffix));
            }
        };

        let jsx_mode = self.effective_jsx_mode();
        if let Some(source) = self.extract_jsx_import_source_pragma() {
            let runtime_suffix = if jsx_mode == JsxMode::ReactJsxDev {
                "jsx-dev-runtime"
            } else {
                "jsx-runtime"
            };
            push_candidate(source, runtime_suffix);
        }

        let option_source = if self.ctx.compiler_options.jsx_import_source.is_empty() {
            None
        } else {
            Some(self.ctx.compiler_options.jsx_import_source.clone())
        };
        if matches!(jsx_mode, JsxMode::ReactJsx | JsxMode::ReactJsxDev) || option_source.is_some() {
            let source = option_source.unwrap_or_else(|| "react".to_string());
            let runtime_suffix = if jsx_mode == JsxMode::ReactJsxDev {
                "jsx-dev-runtime"
            } else {
                "jsx-runtime"
            };
            push_candidate(source, runtime_suffix);
        }

        if let Some(all_arenas) = self.ctx.all_arenas.as_ref() {
            for arena in &**all_arenas {
                let Some(source_file) = arena.source_files.first() else {
                    continue;
                };
                let text = &source_file.text;
                let pragma_source = parse_jsx_import_source_pragma_for_bridge(text);
                let mode = match crate::jsx::runtime::extract_jsx_runtime_pragma(text) {
                    Some("classic") => JsxMode::React,
                    Some("automatic") => {
                        if self.ctx.compiler_options.jsx_mode == JsxMode::ReactJsxDev {
                            JsxMode::ReactJsxDev
                        } else {
                            JsxMode::ReactJsx
                        }
                    }
                    _ => self.ctx.compiler_options.jsx_mode,
                };
                let option_source = if self.ctx.compiler_options.jsx_import_source.is_empty() {
                    None
                } else {
                    Some(self.ctx.compiler_options.jsx_import_source.clone())
                };
                let uses_import_source = matches!(mode, JsxMode::ReactJsx | JsxMode::ReactJsxDev)
                    || pragma_source.is_some()
                    || option_source.is_some();
                if !uses_import_source {
                    continue;
                }
                let source = pragma_source
                    .or(option_source)
                    .unwrap_or_else(|| "react".to_string());
                let runtime_suffix = if mode == JsxMode::ReactJsxDev {
                    "jsx-dev-runtime"
                } else {
                    "jsx-runtime"
                };
                push_candidate(source, runtime_suffix);
            }
        }

        candidates
    }
}

fn parse_jsx_import_source_pragma_for_bridge(text: &str) -> Option<String> {
    let scan_limit = text.len().min(4096);
    let scan_text = &text[..scan_limit];
    let bytes = scan_text.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() {
        if bytes[pos].is_ascii_whitespace() {
            pos += 1;
            continue;
        }
        if pos + 1 < bytes.len() && bytes[pos] == b'/' && bytes[pos + 1] == b'*' {
            let comment_start = pos + 2;
            if let Some(end_offset) = scan_text[comment_start..].find("*/") {
                let comment_body = &scan_text[comment_start..comment_start + end_offset];
                if let Some(idx) = comment_body.find("@jsxImportSource") {
                    let after = &comment_body[idx + "@jsxImportSource".len()..];
                    let pkg: String = after
                        .trim_start()
                        .chars()
                        .take_while(|c| {
                            c.is_alphanumeric()
                                || *c == '_'
                                || *c == '-'
                                || *c == '/'
                                || *c == '@'
                                || *c == '.'
                        })
                        .collect();
                    if !pkg.is_empty() {
                        return Some(pkg);
                    }
                }
                pos = comment_start + end_offset + 2;
                continue;
            }
        }
        break;
    }
    None
}

fn types_package_root_for_jsx_import_source(source: &str) -> Option<String> {
    let root = if let Some(stripped) = source.strip_prefix('@') {
        let mut parts = stripped.split('/');
        let scope = parts.next()?;
        let package = parts.next()?;
        if scope.is_empty() || package.is_empty() || parts.next().is_some() {
            return None;
        }
        format!("@types/{scope}__{package}")
    } else {
        let root = source.split('/').next()?;
        if root.is_empty() {
            return None;
        }
        format!("@types/{root}")
    };
    Some(root)
}

fn jsx_runtime_path_matches(
    normalized_file_name: &str,
    package_root: &str,
    runtime_suffix: &str,
) -> bool {
    let components: Vec<&str> = normalized_file_name
        .split('/')
        .filter(|component| !component.is_empty())
        .collect();
    let package_components: Vec<&str> = package_root
        .split('/')
        .filter(|component| !component.is_empty())
        .collect();
    if package_components.is_empty() {
        return false;
    }

    components
        .windows(package_components.len() + 2)
        .any(|window| {
            window[0] == "node_modules"
                && window[1..=package_components.len()] == package_components
                && is_jsx_runtime_entry_component(
                    window[package_components.len() + 1],
                    runtime_suffix,
                )
        })
}

fn is_jsx_runtime_entry_component(component: &str, runtime_suffix: &str) -> bool {
    component == runtime_suffix
        || component
            .strip_prefix(runtime_suffix)
            .is_some_and(|suffix| suffix.starts_with(".d."))
}

#[cfg(test)]
mod tests {
    use super::jsx_runtime_path_matches;

    #[test]
    fn matches_jsx_runtime_directory_entry() {
        assert!(jsx_runtime_path_matches(
            "/repo/node_modules/react/jsx-runtime/index.d.ts",
            "react",
            "jsx-runtime",
        ));
        assert!(jsx_runtime_path_matches(
            r"C:\repo\node_modules\react\jsx-dev-runtime\index.d.ts"
                .replace('\\', "/")
                .as_str(),
            "react",
            "jsx-dev-runtime",
        ));
    }

    #[test]
    fn matches_jsx_runtime_declaration_file_entry() {
        assert!(jsx_runtime_path_matches(
            "/repo/node_modules/react/jsx-runtime.d.ts",
            "react",
            "jsx-runtime",
        ));
        assert!(jsx_runtime_path_matches(
            "/repo/node_modules/@types/react/jsx-runtime.d.mts",
            "@types/react",
            "jsx-runtime",
        ));
    }

    #[test]
    fn matches_scoped_package_runtime_entry() {
        assert!(jsx_runtime_path_matches(
            "/repo/node_modules/@scope/pkg/jsx-runtime/index.d.ts",
            "@scope/pkg",
            "jsx-runtime",
        ));
        assert!(jsx_runtime_path_matches(
            "/repo/node_modules/@types/scope__pkg/jsx-runtime.d.ts",
            "@types/scope__pkg",
            "jsx-runtime",
        ));
    }

    #[test]
    fn rejects_adjacent_substring_paths() {
        assert!(!jsx_runtime_path_matches(
            "/repo/node_modules/not-react/jsx-runtime/index.d.ts",
            "react",
            "jsx-runtime",
        ));
        assert!(!jsx_runtime_path_matches(
            "/repo/node_modules/react/jsx-runtime-extra/index.d.ts",
            "react",
            "jsx-runtime",
        ));
        assert!(!jsx_runtime_path_matches(
            "/repo/vendor/node_modules-react/jsx-runtime.d.ts",
            "react",
            "jsx-runtime",
        ));
    }
}
