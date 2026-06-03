use super::super::DeclarationEmitter;
use tsz_binder::BinderState;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn expand_imported_indexed_access_type_text(
        &self,
        type_text: &str,
    ) -> Option<String> {
        if !type_text.contains("import(\"") || !type_text.contains("[\"") {
            return None;
        }

        let mut changed = false;
        let mut remaining = type_text;
        let mut output = String::with_capacity(type_text.len());
        while let Some(start) = remaining.find("import(\"") {
            output.push_str(&remaining[..start]);
            let candidate = &remaining[start..];
            let Some((consumed, replacement)) =
                self.expand_imported_indexed_access_type_text_at(candidate)
            else {
                output.push_str("import(\"");
                remaining = &candidate["import(\"".len()..];
                continue;
            };
            output.push_str(&replacement);
            remaining = &candidate[consumed..];
            changed = true;
        }
        output.push_str(remaining);

        changed.then_some(output)
    }

    fn expand_imported_indexed_access_type_text_at(
        &self,
        type_text: &str,
    ) -> Option<(usize, String)> {
        let after_import = type_text.strip_prefix("import(\"")?;
        let module_end = after_import.find("\")")?;
        let module_specifier = &after_import[..module_end];
        let after_module = &after_import[module_end + "\")".len()..];
        let after_dot = after_module.strip_prefix('.')?;
        let export_len = after_dot
            .char_indices()
            .find_map(|(idx, ch)| {
                (!(ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())).then_some(idx)
            })
            .unwrap_or(after_dot.len());
        if export_len == 0 {
            return None;
        }
        let export_name = &after_dot[..export_len];
        let after_export = &after_dot[export_len..];
        let after_open = after_export.strip_prefix("[\"")?;
        let key_end = after_open.find("\"]")?;
        let member_name = &after_open[..key_end];
        let consumed = "import(\"".len()
            + module_end
            + "\")".len()
            + '.'.len_utf8()
            + export_len
            + "[\"".len()
            + key_end
            + "\"]".len();
        let replacement = self.imported_indexed_access_member_type_text(
            module_specifier,
            export_name,
            member_name,
        )?;
        Some((consumed, replacement))
    }

    fn imported_indexed_access_member_type_text(
        &self,
        module_specifier: &str,
        export_name: &str,
        member_name: &str,
    ) -> Option<String> {
        let binder = self.binder?;
        let current_path = self.current_file_path.as_deref().unwrap_or("");
        let mut module_paths =
            self.matching_module_export_paths(binder, current_path, module_specifier);
        if module_paths.is_empty()
            && !module_specifier.starts_with('.')
            && !module_specifier.starts_with('/')
        {
            module_paths = self.package_root_index_module_export_paths(binder, module_specifier);
        }
        if module_paths.is_empty()
            && !module_specifier.starts_with('.')
            && !module_specifier.starts_with('/')
        {
            module_paths = self.package_export_module_paths(binder, module_specifier, export_name);
        }
        for module_path in module_paths {
            let Some(exports) = binder.module_exports.get(module_path) else {
                continue;
            };
            let Some(export_sym_id) = exports.get(export_name) else {
                continue;
            };
            let export_sym_id = self
                .resolve_portability_import_alias(export_sym_id, binder)
                .unwrap_or(export_sym_id);
            let export_sym_id = self.resolve_portability_declaration_symbol(export_sym_id, binder);
            if let Some(type_text) =
                self.type_member_declared_type_annotation_text(export_sym_id, member_name)
            {
                return Some(type_text);
            }
        }
        None
    }

    fn package_export_module_paths<'b>(
        &self,
        binder: &'b BinderState,
        module_specifier: &str,
        export_name: &str,
    ) -> Vec<&'b str> {
        let mut matches: Vec<_> = binder
            .module_exports
            .iter()
            .filter_map(|(module_path, exports)| {
                Self::deepest_node_modules_package_root_path(module_path, module_specifier)?;
                exports.has(export_name).then_some(module_path.as_str())
            })
            .collect();
        matches.sort_by(|left, right| right.len().cmp(&left.len()).then_with(|| left.cmp(right)));
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::type_cache_view::TypeCacheView;
    use std::sync::Arc;
    use tsz_binder::{BinderState, SymbolTable};
    use tsz_parser::ParserState;

    #[test]
    fn package_root_imported_indexed_access_expands_member_annotation() {
        let mut package_parser = ParserState::new(
            "/project/node_modules/create-emotion-styled/index.d.ts".to_string(),
            r#"
export interface StyledOtherComponentList {
    "div": import("react").DetailedHTMLProps<import("react").HTMLAttributes<HTMLDivElement>, HTMLDivElement>;
}
"#
            .to_string(),
        );
        let package_root = package_parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(&package_parser.arena, package_root);
        let list_sym = binder
            .symbols
            .iter()
            .find(|symbol| symbol.escaped_name == "StyledOtherComponentList")
            .map(|symbol| symbol.id)
            .expect("missing list symbol");
        let package_arena = Arc::new(package_parser.arena.clone());
        let mut symbol_arenas = rustc_hash::FxHashMap::default();
        symbol_arenas.insert(list_sym, Arc::clone(&package_arena));
        binder.symbol_arenas = Arc::new(symbol_arenas);
        let mut exports = SymbolTable::new();
        exports.set("StyledOtherComponentList".to_string(), list_sym);
        let package_path = "/project/node_modules/create-emotion-styled/index.d.ts".to_string();
        let mut module_exports = rustc_hash::FxHashMap::default();
        module_exports.insert(package_path.clone(), exports);
        binder.module_exports = Arc::new(module_exports);

        let mut current_parser = ParserState::new("/project/index.ts".to_string(), String::new());
        let _ = current_parser.parse_source_file();
        let interner = tsz_solver::construction::TypeInterner::new();
        let mut emitter = DeclarationEmitter::with_type_info(
            &current_parser.arena,
            TypeCacheView::default(),
            &interner,
            &binder,
        );
        emitter.current_file_path = Some("/project/index.ts".to_string());
        let mut arena_to_path = rustc_hash::FxHashMap::default();
        arena_to_path.insert(Arc::as_ptr(&package_arena) as usize, package_path);
        emitter.set_arena_to_path(arena_to_path);

        let expanded = emitter
            .expand_imported_indexed_access_type_text(
                r#"import("create-emotion-styled").StyledOtherComponentList["div"]"#,
            )
            .expect("expected package-root indexed access expansion");

        assert_eq!(
            expanded,
            r#"import("react").DetailedHTMLProps<import("react").HTMLAttributes<HTMLDivElement>, HTMLDivElement>"#
        );
    }
}
