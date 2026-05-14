//! Portable mapped-object and imported string-union expansion helpers.
//!
//! These routines recover declaration-safe object text from portable mapped
//! types whose keys come from imported string-literal union aliases.

use super::super::DeclarationEmitter;
use rustc_hash::FxHashMap;
use tsz_binder::BinderState;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, ParserState};
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn contains_portable_mapped_object_text(text: &str) -> bool {
        Self::split_top_level_intersection_parts(text)
            .iter()
            .any(|part| {
                let trimmed = part.trim().trim_end_matches(';').trim();
                trimmed
                    .strip_prefix('{')
                    .and_then(|inner| inner.strip_suffix('}'))
                    .is_some_and(|inner| {
                        inner.trim_start().starts_with('[')
                            && Self::type_text_contains_import_type(inner)
                    })
            })
    }

    pub(in crate::declaration_emitter) fn expand_portable_intersection_type_text(
        &self,
        source_arena: &NodeArena,
        text: &str,
    ) -> Option<String> {
        let parts = Self::split_top_level_intersection_parts(text);
        if parts.len() <= 1 {
            return self.expand_portable_object_type_text(source_arena, text);
        }

        let mut changed = false;
        let expanded_parts: Vec<String> = parts
            .into_iter()
            .map(|part| {
                if let Some(expanded) =
                    self.expand_portable_object_type_text(source_arena, part.trim())
                {
                    changed = true;
                    expanded
                } else {
                    part.trim().to_string()
                }
            })
            .collect();

        changed.then(|| expanded_parts.join(" & "))
    }

    fn split_top_level_intersection_parts(text: &str) -> Vec<String> {
        let bytes = text.as_bytes();
        let mut brace_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut paren_depth = 0usize;
        let mut angle_depth = 0usize;
        let mut part_start = 0usize;
        let mut parts = Vec::new();
        let mut i = 0usize;
        while i < bytes.len() {
            match bytes[i] {
                b'{' => brace_depth += 1,
                b'}' => brace_depth = brace_depth.saturating_sub(1),
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b'(' => paren_depth += 1,
                b')' => paren_depth = paren_depth.saturating_sub(1),
                b'<' => angle_depth += 1,
                b'>' => angle_depth = angle_depth.saturating_sub(1),
                b'&' if brace_depth == 0
                    && bracket_depth == 0
                    && paren_depth == 0
                    && angle_depth == 0 =>
                {
                    let part = text
                        .get(part_start..i)
                        .map(str::trim)
                        .unwrap_or_default()
                        .to_string();
                    if !part.is_empty() {
                        parts.push(part);
                    }
                    part_start = i + 1;
                }
                _ => {}
            }
            i += 1;
        }
        let tail = text
            .get(part_start..)
            .map(str::trim)
            .unwrap_or_default()
            .to_string();
        if !tail.is_empty() {
            parts.push(tail);
        }
        parts
    }

    fn expand_portable_object_type_text(
        &self,
        source_arena: &NodeArena,
        text: &str,
    ) -> Option<String> {
        let trimmed = text.trim().trim_end_matches(';').trim();
        let inner = trimmed.strip_prefix('{')?.strip_suffix('}')?.trim();
        if inner.starts_with('[') {
            return self.expand_portable_mapped_object_text(source_arena, inner);
        }
        (!inner.is_empty()).then(|| format!("{{\n    {};\n}}", inner.trim().trim_end_matches(';')))
    }

    pub(in crate::declaration_emitter) fn expand_portable_mapped_object_text_in_current_context(
        &self,
        text: &str,
    ) -> Option<String> {
        Self::contains_portable_mapped_object_text(text)
            .then(|| self.expand_portable_intersection_type_text(self.arena, text))
            .flatten()
    }

    pub(in crate::declaration_emitter) fn expand_portable_mapped_object_text(
        &self,
        source_arena: &NodeArena,
        inner: &str,
    ) -> Option<String> {
        let in_pos = inner.find(" in ")?;
        let after_in = inner.get(in_pos + 4..)?;
        let close_bracket = after_in.find(']')?;
        let key_ref = after_in.get(..close_bracket)?.trim();
        let after_bracket = after_in.get(close_bracket + 1..)?.trim_start();
        let after_optional = after_bracket.strip_prefix("?:")?.trim_start();
        let value_type = after_optional.trim_end().trim_end_matches(';').trim();
        let (module_specifier, export_name) = Self::parse_import_type_reference(key_ref)?;
        let keys = self.expand_imported_string_union_alias_keys(
            source_arena,
            &module_specifier,
            &export_name,
        )?;
        let members: Vec<String> = keys
            .into_iter()
            .map(|key| format!("    {key}?: {value_type} | undefined;"))
            .collect();
        Some(format!("{{\n{}\n}}", members.join("\n")))
    }

    fn parse_import_type_reference(text: &str) -> Option<(String, String)> {
        let (start, module_specifier, tail) = Self::next_import_type_text(text)?;
        if start != 0 {
            return None;
        }
        let export_name = tail.trim().strip_prefix('.')?.to_string();
        Some((module_specifier, export_name))
    }

    fn expand_imported_string_union_alias_keys(
        &self,
        source_arena: &NodeArena,
        module_specifier: &str,
        export_name: &str,
    ) -> Option<Vec<String>> {
        let binder = self.binder?;
        let source_path = self
            .arena_to_path
            .get(&(source_arena as *const NodeArena as usize))
            .cloned()
            .or_else(|| {
                self.arena_source_file(source_arena)
                    .map(|source_file| source_file.file_name.clone())
            })
            .or_else(|| self.current_file_path.clone())?;

        let mut module_paths =
            self.matching_module_export_paths(binder, &source_path, module_specifier);
        if module_paths.is_empty()
            && !module_specifier.starts_with('.')
            && !module_specifier.starts_with('/')
        {
            module_paths = self.package_root_index_module_export_paths(binder, module_specifier);
        }

        for module_path in module_paths {
            let Some(exports) = binder.module_exports.get(module_path) else {
                continue;
            };
            let Some(export_sym_id) = exports.get(export_name) else {
                continue;
            };
            if let Some((foreign_arena, alias_type_node)) =
                self.exported_type_alias_type_node_in_module_path(module_path, export_name)
                && let Some(keys) = self.expand_string_literals_from_type_node_in_arena(
                    foreign_arena,
                    alias_type_node,
                    &FxHashMap::default(),
                    0,
                )
            {
                return Some(keys);
            }
            if let Some(keys) =
                self.expand_string_literals_from_type_alias_file(module_path, export_name)
            {
                return Some(keys);
            }
            if let Some(keys) =
                self.with_symbol_declarations(export_sym_id, |foreign_arena, decl_idx| {
                    let mut current = decl_idx;
                    for _ in 0..4 {
                        let decl_node = foreign_arena.get(current)?;
                        if let Some(alias) = foreign_arena.get_type_alias(decl_node) {
                            return self.expand_string_literals_from_type_node_in_arena(
                                foreign_arena,
                                alias.type_node,
                                &FxHashMap::default(),
                                0,
                            );
                        }
                        current = foreign_arena.parent_of(current)?;
                        if !current.is_some() {
                            return None;
                        }
                    }
                    None
                })
            {
                return Some(keys);
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn package_root_index_module_export_paths<'b>(
        &self,
        binder: &'b BinderState,
        module_specifier: &str,
    ) -> Vec<&'b str> {
        let mut matches: Vec<_> = binder
            .module_exports
            .keys()
            .filter_map(|module_path| {
                let package_root =
                    Self::deepest_node_modules_package_root_path(module_path, module_specifier)?;
                let suffix = module_path.strip_prefix(&package_root)?;
                matches!(
                    suffix.trim_start_matches(['/', '\\']),
                    "index.d.ts" | "index.ts"
                )
                .then_some(module_path.as_str())
            })
            .collect();
        matches.sort_by(|left, right| right.len().cmp(&left.len()).then_with(|| left.cmp(right)));
        matches
    }

    pub(in crate::declaration_emitter) fn deepest_node_modules_package_root_path(
        module_path: &str,
        module_specifier: &str,
    ) -> Option<String> {
        use std::path::{Component, Path, PathBuf};

        let components: Vec<_> = Path::new(module_path).components().collect();
        let pkg_len = if module_specifier.starts_with('@') {
            2
        } else {
            1
        };
        components
            .iter()
            .enumerate()
            .filter_map(|(idx, component)| {
                matches!(component, Component::Normal(part) if part.to_str() == Some("node_modules"))
                    .then_some(idx)
            })
            .filter_map(|nm_idx| {
                let pkg_start = nm_idx + 1;
                if components.len() < pkg_start + pkg_len {
                    return None;
                }
                let package_name = components[pkg_start..pkg_start + pkg_len]
                    .iter()
                    .filter_map(|component| match component {
                        Component::Normal(part) => part.to_str(),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("/");
                (package_name == module_specifier).then(|| {
                    components[..pkg_start + pkg_len]
                        .iter()
                        .fold(PathBuf::new(), |mut path, component| {
                            path.push(component.as_os_str());
                            path
                        })
                        .to_string_lossy()
                        .into_owned()
                })
            })
            .max_by_key(|path| path.len())
    }

    fn expand_string_literals_from_type_alias_file(
        &self,
        module_path: &str,
        export_name: &str,
    ) -> Option<Vec<String>> {
        let source = std::fs::read_to_string(module_path).ok()?;
        let mut parser = ParserState::new(module_path.to_string(), source);
        let _root = parser.parse_source_file();
        let alias_type_node = self
            .find_type_alias_type_node_in_arena(&parser.arena, export_name)
            .or_else(|| self.type_alias_type_node_by_name_in_arena(&parser.arena, export_name))?;
        self.expand_string_literals_from_type_node_in_arena(
            &parser.arena,
            alias_type_node,
            &FxHashMap::default(),
            0,
        )
    }

    fn exported_type_alias_type_node_in_module_path<'arena>(
        &'arena self,
        module_path: &str,
        export_name: &str,
    ) -> Option<(&'arena NodeArena, NodeIndex)> {
        if self.arena_matches_module_path(self.arena, module_path)
            && let Some(type_node) =
                self.find_type_alias_type_node_in_arena(self.arena, export_name)
        {
            return Some((self.arena, type_node));
        }

        for arena in self.global_symbol_arenas.values() {
            if self.arena_matches_module_path(arena.as_ref(), module_path)
                && let Some(type_node) =
                    self.find_type_alias_type_node_in_arena(arena.as_ref(), export_name)
            {
                return Some((arena.as_ref(), type_node));
            }
        }

        let binder = self.binder?;
        for arenas in binder.declaration_arenas.values() {
            for arena in arenas {
                if self.arena_matches_module_path(arena.as_ref(), module_path)
                    && let Some(type_node) =
                        self.find_type_alias_type_node_in_arena(arena.as_ref(), export_name)
                {
                    return Some((arena.as_ref(), type_node));
                }
            }
        }

        for arena in binder.symbol_arenas.values() {
            if self.arena_matches_module_path(arena.as_ref(), module_path)
                && let Some(type_node) =
                    self.find_type_alias_type_node_in_arena(arena.as_ref(), export_name)
            {
                return Some((arena.as_ref(), type_node));
            }
        }

        None
    }

    fn type_alias_type_node_by_name_in_arena(
        &self,
        arena: &NodeArena,
        export_name: &str,
    ) -> Option<NodeIndex> {
        for node_id in 0..arena.len() {
            let node_idx = NodeIndex(u32::try_from(node_id).ok()?);
            let node = arena.get(node_idx)?;
            let Some(alias) = arena.get_type_alias(node) else {
                continue;
            };
            if self
                .identifier_text_from_arena(arena, alias.name)
                .is_some_and(|name| name == export_name)
            {
                return Some(alias.type_node);
            }
        }

        None
    }

    fn arena_matches_module_path(&self, arena: &NodeArena, module_path: &str) -> bool {
        let arena_ptr = arena as *const NodeArena as usize;
        self.arena_to_path
            .get(&arena_ptr)
            .is_some_and(|path| path == module_path)
            || self
                .arena_source_file(arena)
                .is_some_and(|source_file| source_file.file_name == module_path)
    }

    fn type_reference_name_text_from_arena(
        &self,
        arena: &NodeArena,
        name_idx: NodeIndex,
    ) -> Option<String> {
        let name_node = arena.get(name_idx)?;
        if name_node.kind == SyntaxKind::Identifier as u16 {
            return self.identifier_text_from_arena(arena, name_idx);
        }
        if name_node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qualified = arena.get_qualified_name(name_node)?;
            return self.identifier_text_from_arena(arena, qualified.right);
        }
        None
    }

    pub(in crate::declaration_emitter) fn find_type_alias_type_node_in_arena(
        &self,
        arena: &NodeArena,
        name: &str,
    ) -> Option<NodeIndex> {
        let source_file = self.arena_source_file(arena)?;
        for &stmt_idx in &source_file.statements.nodes {
            let stmt_node = arena.get(stmt_idx)?;
            let Some(alias) = arena.get_type_alias(stmt_node) else {
                continue;
            };
            if self
                .identifier_text_from_arena(arena, alias.name)
                .as_deref()
                == Some(name)
            {
                return Some(alias.type_node);
            }
        }
        None
    }

    fn expand_string_literals_from_type_node_in_arena(
        &self,
        arena: &NodeArena,
        type_node: NodeIndex,
        substitutions: &FxHashMap<String, String>,
        depth: usize,
    ) -> Option<Vec<String>> {
        if depth > 32 {
            return None;
        }

        let node = arena.get(type_node)?;
        match node.kind {
            k if k == syntax_kind_ext::UNION_TYPE => {
                let composite = arena.get_composite_type(node)?;
                let mut result = Vec::new();
                for &child in &composite.types.nodes {
                    result.extend(self.expand_string_literals_from_type_node_in_arena(
                        arena,
                        child,
                        substitutions,
                        depth + 1,
                    )?);
                }
                Some(result)
            }
            k if k == syntax_kind_ext::LITERAL_TYPE => {
                let literal = arena.get_literal_type(node)?;
                let literal_node = arena.get(literal.literal)?;
                if literal_node.kind == SyntaxKind::StringLiteral as u16 {
                    Some(vec![arena.get_literal(literal_node)?.text.clone()])
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let type_ref = arena.get_type_ref(node)?;
                let name = self.type_reference_name_text_from_arena(arena, type_ref.type_name)?;
                if let Some(value) = substitutions.get(&name) {
                    return Some(vec![value.clone()]);
                }
                if let Some(alias_type) = self.find_type_alias_type_node_in_arena(arena, &name) {
                    return self.expand_string_literals_from_type_node_in_arena(
                        arena,
                        alias_type,
                        substitutions,
                        depth + 1,
                    );
                }
                let source_file = self.arena_source_file(arena)?;
                for &stmt_idx in &source_file.statements.nodes {
                    let stmt_node = arena.get(stmt_idx)?;
                    let Some(import) = arena.get_import_decl(stmt_node) else {
                        continue;
                    };
                    let Some(module_node) = arena.get(import.module_specifier) else {
                        continue;
                    };
                    let Some(module_lit) = arena.get_literal(module_node) else {
                        continue;
                    };
                    let Some(clause_node) = arena.get(import.import_clause) else {
                        continue;
                    };
                    let Some(clause) = arena.get_import_clause(clause_node) else {
                        continue;
                    };
                    let Some(bindings_node) = arena.get(clause.named_bindings) else {
                        continue;
                    };
                    let Some(bindings) = arena.get_named_imports(bindings_node) else {
                        continue;
                    };
                    for &spec_idx in &bindings.elements.nodes {
                        let spec_node = arena.get(spec_idx)?;
                        let specifier = arena.get_specifier(spec_node)?;
                        let local_name = self.identifier_text_from_arena(arena, specifier.name)?;
                        if local_name != name {
                            continue;
                        }
                        let imported_name = if specifier.property_name.is_some() {
                            self.identifier_text_from_arena(arena, specifier.property_name)
                                .unwrap_or(local_name)
                        } else {
                            local_name
                        };
                        return self.expand_imported_string_union_alias_keys(
                            arena,
                            module_lit.text.as_str(),
                            &imported_name,
                        );
                    }
                }
                None
            }
            k if k == SyntaxKind::Identifier as u16 => {
                let name = self.identifier_text_from_arena(arena, type_node)?;
                if let Some(value) = substitutions.get(&name) {
                    return Some(vec![value.clone()]);
                }
                let alias_type = self.find_type_alias_type_node_in_arena(arena, &name)?;
                self.expand_string_literals_from_type_node_in_arena(
                    arena,
                    alias_type,
                    substitutions,
                    depth + 1,
                )
            }
            _ => None,
        }
    }
}
