//! Public package module specifier helpers for inferred type text

#[allow(unused_imports)]
use super::super::{DeclarationEmitter, ImportPlan, PlannedImportModule, PlannedImportSymbol};
#[allow(unused_imports)]
use crate::emitter::type_printer::TypePrinter;
#[allow(unused_imports)]
use crate::output::source_writer::{SourcePosition, SourceWriter, source_position_from_offset};
#[allow(unused_imports)]
use rustc_hash::{FxHashMap, FxHashSet};
#[allow(unused_imports)]
use std::sync::Arc;
#[allow(unused_imports)]
use tracing::debug;
#[allow(unused_imports)]
use tsz_binder::{BinderState, SymbolId, symbol_flags};
#[allow(unused_imports)]
use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};
#[allow(unused_imports)]
use tsz_parser::parser::ParserState;
#[allow(unused_imports)]
use tsz_parser::parser::node::{Node, NodeAccess, NodeArena};
#[allow(unused_imports)]
use tsz_parser::parser::syntax_kind_ext;
#[allow(unused_imports)]
use tsz_parser::parser::{NodeIndex, NodeList};
#[allow(unused_imports)]
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn qualify_public_package_names_in_text(
        &self,
        binder: &BinderState,
        base_module: &str,
        text: &str,
        excluded_names: &[String],
    ) -> String {
        let base_package = Self::bare_package_specifier(base_module);
        let mut replacements = Vec::new();
        for export_name in Self::type_reference_names_in_text(text) {
            if excluded_names.iter().any(|name| name == &export_name)
                || !Self::contains_whole_word_in_text(text, &export_name)
            {
                continue;
            }
            let Some(module_specifier) = self.public_module_specifier_exporting_name(
                binder,
                base_package,
                base_module,
                &export_name,
            ) else {
                continue;
            };
            replacements.push((
                export_name.clone(),
                format!("import(\"{module_specifier}\").{export_name}"),
            ));
        }

        Self::replace_whole_words_in_text(text, &replacements)
    }

    fn public_module_specifier_exporting_name(
        &self,
        binder: &BinderState,
        base_package: &str,
        base_module: &str,
        export_name: &str,
    ) -> Option<String> {
        if self.imported_module_exports_name(binder, base_module, export_name) {
            return Some(base_module.to_string());
        }

        let current_path = self.current_file_path.as_deref()?;
        let mut candidates = binder
            .module_exports
            .iter()
            .filter_map(|(module_path, exports)| {
                exports.get(export_name)?;
                let specifier =
                    self.package_specifier_for_node_modules_path(current_path, module_path)?;
                (Self::bare_package_specifier(&specifier) == base_package).then_some(specifier)
            })
            .collect::<Vec<_>>();
        candidates.sort_by_key(|specifier| (specifier.len(), specifier.clone()));
        candidates.into_iter().next().or_else(|| {
            self.public_module_specifier_from_package_files(base_package, base_module, export_name)
        })
    }

    fn public_module_specifier_from_package_files(
        &self,
        base_package: &str,
        base_module: &str,
        export_name: &str,
    ) -> Option<String> {
        use std::path::Path;

        let current_path = Path::new(self.current_file_path.as_deref()?);
        let mut ancestor = current_path.parent();
        let package_parts = base_package.split('/').collect::<Vec<_>>();
        while let Some(dir) = ancestor {
            let mut package_root = dir.join("node_modules");
            for part in &package_parts {
                package_root.push(part);
            }
            if package_root.exists() {
                if let Some(specifier) = self.explicit_package_dts_export_specifier(
                    &package_root,
                    base_package,
                    export_name,
                ) {
                    return Some(specifier);
                }
                if self.package_root_has_export_star(&package_root) {
                    return Some(base_module.to_string());
                }
                return None;
            }
            ancestor = dir.parent();
        }

        None
    }

    fn explicit_package_dts_export_specifier(
        &self,
        package_root: &std::path::Path,
        base_package: &str,
        export_name: &str,
    ) -> Option<String> {
        let mut stack = vec![package_root.to_path_buf()];
        let mut dts_files = Vec::new();
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|ext| ext == "ts")
                    && path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.ends_with(".d.ts"))
                {
                    dts_files.push(path);
                }
            }
        }
        dts_files.sort();
        for file in dts_files {
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            if !Self::dts_text_explicitly_exports_name(&text, export_name) {
                continue;
            }
            let rel = file.strip_prefix(package_root).ok()?;
            let mut subpath = self.strip_ts_extensions(&rel.to_string_lossy().replace('\\', "/"));
            if subpath.ends_with("/index") {
                subpath.truncate(subpath.len() - "/index".len());
            }
            return if subpath.is_empty() {
                Some(base_package.to_string())
            } else {
                Some(format!("{base_package}/{subpath}"))
            };
        }

        None
    }

    fn package_root_has_export_star(&self, package_root: &std::path::Path) -> bool {
        let package_json = package_root.join("package.json");
        let root_dts = std::fs::read_to_string(&package_json)
            .ok()
            .and_then(|text| {
                let typings = text
                    .lines()
                    .find_map(|line| {
                        line.split_once("\"typings\"")
                            .or_else(|| line.split_once("\"types\""))
                    })?
                    .1;
                let value = typings.split('"').nth(1)?;
                Some(package_root.join(value))
            })
            .unwrap_or_else(|| package_root.join("index.d.ts"));
        std::fs::read_to_string(root_dts)
            .ok()
            .is_some_and(|text| text.contains("export * from"))
    }

    fn dts_text_explicitly_exports_name(text: &str, export_name: &str) -> bool {
        text.lines().any(|line| {
            let trimmed = line.trim();
            if !trimmed.starts_with("export ") || !trimmed.contains('{') {
                return false;
            }
            let Some(named) = trimmed
                .split_once('{')
                .and_then(|(_, rest)| rest.split_once('}'))
            else {
                return false;
            };
            named.0.split(',').any(|part| {
                let name = part
                    .trim()
                    .split_once(" as ")
                    .map_or_else(|| part.trim(), |(name, _)| name.trim());
                name == export_name
            })
        })
    }

    pub(in crate::declaration_emitter) fn export_symbol_from_module_specifier(
        &self,
        binder: &BinderState,
        module_specifier: &str,
        export_name: &str,
    ) -> Option<SymbolId> {
        if let Some(sym_id) = binder
            .module_exports
            .get(module_specifier)
            .and_then(|exports| exports.get(export_name))
        {
            return Some(sym_id);
        }

        if let Some(current_path) = self.current_file_path.as_deref() {
            for module_path in
                self.matching_module_export_paths(binder, current_path, module_specifier)
            {
                if let Some(sym_id) = binder
                    .module_exports
                    .get(module_path)
                    .and_then(|exports| exports.get(export_name))
                {
                    return Some(sym_id);
                }
            }
        }

        if !module_specifier.starts_with('.') && !module_specifier.starts_with('/') {
            let mut matches = binder
                .module_exports
                .iter()
                .filter_map(|(module_path, exports)| {
                    if !(self
                        .node_modules_path_matches_import_specifier(module_path, module_specifier)
                        || self.node_modules_package_path_matches_import_specifier(
                            module_path,
                            module_specifier,
                        ))
                    {
                        return None;
                    }
                    exports
                        .get(export_name)
                        .map(|sym_id| (module_path.len(), sym_id))
                })
                .collect::<Vec<_>>();
            matches.sort_by_key(|(path_len, _)| *path_len);
            return matches.into_iter().map(|(_, sym_id)| sym_id).next();
        }

        None
    }

    fn type_reference_names_in_text(text: &str) -> Vec<String> {
        let mut names = Vec::new();
        let mut chars = text.char_indices().peekable();
        while let Some((start, ch)) = chars.next() {
            if !Self::is_type_reference_identifier_start(ch) {
                continue;
            }
            let mut end = start + ch.len_utf8();
            while let Some(&(next_idx, next_ch)) = chars.peek() {
                if !Self::is_type_reference_identifier_continue(next_ch) {
                    break;
                }
                end = next_idx + next_ch.len_utf8();
                chars.next();
            }
            let name = &text[start..end];
            if !matches!(
                name,
                "import"
                    | "typeof"
                    | "keyof"
                    | "readonly"
                    | "string"
                    | "number"
                    | "boolean"
                    | "bigint"
                    | "symbol"
                    | "undefined"
                    | "null"
                    | "true"
                    | "false"
                    | "any"
                    | "unknown"
                    | "never"
                    | "void"
            ) && !names.iter().any(|existing| existing == name)
            {
                names.push(name.to_string());
            }
        }
        names
    }

    pub(in crate::declaration_emitter) fn is_builtin_conditional_utility_type_name(
        name: &str,
    ) -> bool {
        matches!(name, "Exclude" | "Extract" | "NonNullable")
    }

    pub(in crate::declaration_emitter) fn rewrite_typeof_import_default_return_type(
        &self,
        source_path: &str,
        imported_module: &str,
        type_text: &str,
        binder: &BinderState,
    ) -> Option<String> {
        let import_text = type_text.trim().strip_prefix("typeof ")?;
        let (start, module_specifier, tail) = Self::next_import_type_text(import_text)?;
        if start != 0 || tail.trim() != ".default" {
            return None;
        }

        let target_module_path = self
            .matching_module_export_paths(binder, source_path, &module_specifier)
            .into_iter()
            .next()?;
        let default_sym = binder
            .module_exports
            .get(target_module_path)?
            .get("default")?;
        let default_sym = self.resolve_portability_symbol(default_sym, binder);
        let declared_type = self.declared_type_annotation_text_for_symbol(default_sym)?;
        let public_module =
            Self::combine_public_module_specifier(imported_module, &module_specifier)?;
        let exported_name = Self::leading_type_reference_name(&declared_type)?;
        if binder
            .module_exports
            .get(target_module_path)
            .is_some_and(|exports| exports.get(exported_name).is_some())
        {
            return Some(format!(
                "import(\"{public_module}\").{}{}",
                exported_name,
                &declared_type[exported_name.len()..]
            ));
        }

        None
    }

    pub(in crate::declaration_emitter) fn combine_public_module_specifier(
        base: &str,
        relative: &str,
    ) -> Option<String> {
        if base.starts_with('.') || base.starts_with('/') {
            return None;
        }
        let mut parts = base.split('/').collect::<Vec<_>>();
        if parts.is_empty() {
            return None;
        }
        let package_len = if parts[0].starts_with('@') { 2 } else { 1 };
        if parts.len() < package_len {
            return None;
        }
        if parts.len() > package_len {
            parts.pop();
        }

        for segment in relative.split('/') {
            match segment {
                "" | "." => {}
                ".." if parts.len() > package_len => {
                    parts.pop();
                }
                ".." => return None,
                text => parts.push(text),
            }
        }

        Some(parts.join("/"))
    }
}
