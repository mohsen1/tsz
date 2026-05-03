//! Portability resolution and symbol accessibility

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
    pub(in crate::declaration_emitter) fn find_non_serializable_property_name_in_printed_type(
        &self,
        printed_type_text: &str,
    ) -> Option<String> {
        let binder = self.binder?;
        let current_path = self.current_file_path.as_deref()?;
        let mut search = printed_type_text;
        let needle = " in typeof ";

        while let Some(index) = search.find(needle) {
            let rest = &search[index + needle.len()..];
            // After `in typeof `, the printed form may be either
            //   `Symbol`              -> extract `Symbol` directly, or
            //   `import("…").Symbol`  -> skip past the import prefix and
            //                            extract `Symbol` after it.
            // Without the import-prefix skip, we extract the keyword
            // `import` and emit `[import]` instead of the real symbol
            // name (regressed by PR #2425, see
            // declarationEmitMappedTypeTemplateTypeofSymbol.ts).
            let after_import_prefix = if let Some(after_open) = rest.strip_prefix("import(\"") {
                after_open
                    .find("\")")
                    .and_then(|close| after_open[close + 2..].strip_prefix('.'))
                    .unwrap_or(rest)
            } else {
                rest
            };
            let symbol_expr: String = after_import_prefix
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
                .collect();
            if symbol_expr.is_empty() {
                search = rest;
                continue;
            }

            let accessible_symbol = binder
                .file_locals
                .get(&symbol_expr)
                .or_else(|| binder.current_scope.get(&symbol_expr));

            let Some(accessible_symbol) = accessible_symbol else {
                return Some(format!("[{symbol_expr}]"));
            };

            let accessible_source_path = self.get_symbol_source_path(accessible_symbol, binder);
            if accessible_source_path
                .as_deref()
                .is_some_and(|source_path| {
                    self.paths_refer_to_same_source_file(current_path, source_path)
                })
            {
                search = rest;
                continue;
            }

            let original_sym_id = binder
                .resolve_import_symbol(accessible_symbol)
                .filter(|resolved| *resolved != accessible_symbol)
                .unwrap_or(accessible_symbol);

            let original_source_path = self.get_symbol_source_path(original_sym_id, binder);
            if original_source_path.as_deref().is_some_and(|source_path| {
                !self.paths_refer_to_same_source_file(current_path, source_path)
                    && binder.module_exports.contains_key(source_path)
            }) {
                return Some(format!("[{symbol_expr}]"));
            }

            search = rest;
        }

        None
    }

    pub(in crate::declaration_emitter) fn find_unexported_import_type_reference_in_printed_type(
        &self,
        printed_type_text: &str,
    ) -> Option<(String, String)> {
        let binder = self.binder?;
        let current_path = self.current_file_path.as_deref()?;
        let mut remaining = printed_type_text;

        while let Some(start) = remaining.find("import(\"") {
            let after_prefix = &remaining[start + "import(\"".len()..];
            let Some((module_specifier, tail)) = after_prefix.split_once("\")") else {
                break;
            };
            let Some(tail) = tail.strip_prefix('.') else {
                remaining = after_prefix;
                continue;
            };
            let Some(first_name) = tail
                .split(['.', '<', '[', ' ', '&', '|', '>', ',', ')', ';', '\n', '\r'])
                .find(|part| !part.is_empty())
            else {
                remaining = after_prefix;
                continue;
            };

            let exports = binder
                .module_exports
                .iter()
                .find_map(|(module_path, exports)| {
                    let candidate =
                        if module_specifier.starts_with('.') || module_specifier.starts_with('/') {
                            Some(self.strip_ts_extensions(
                                &self.calculate_relative_path(current_path, module_path),
                            ))
                        } else {
                            self.package_specifier_for_node_modules_path(current_path, module_path)
                        }?;
                    (candidate == module_specifier).then_some(exports)
                });

            if let Some(exports) = exports
                && !exports.has(first_name)
            {
                return Some((module_specifier.to_string(), first_name.to_string()));
            }

            remaining = after_prefix;
        }

        None
    }

    pub(in crate::declaration_emitter) fn printed_type_uses_non_emittable_local_alias_root(
        &self,
        printed_type_text: &str,
    ) -> bool {
        if self.current_source_file_idx.is_none() {
            return false;
        }

        let mut visited_names = rustc_hash::FxHashSet::default();
        self.type_text_uses_non_emittable_local_alias_root(printed_type_text, &mut visited_names)
    }

    pub(in crate::declaration_emitter) fn type_text_uses_non_emittable_local_alias_root(
        &self,
        type_text: &str,
        visited_names: &mut rustc_hash::FxHashSet<String>,
    ) -> bool {
        let bytes = type_text.as_bytes();
        let mut i = 0;

        while i < bytes.len() {
            let ch = bytes[i] as char;
            if ch == '"' || ch == '\'' {
                i += 1;
                while i < bytes.len() {
                    let current = bytes[i] as char;
                    if current == '\\' {
                        i = (i + 2).min(bytes.len());
                        continue;
                    }
                    i += 1;
                    if current == ch {
                        break;
                    }
                }
                continue;
            }
            if !Self::is_type_text_identifier_start(ch) {
                i += 1;
                continue;
            }

            let start = i;
            i += 1;
            while i < bytes.len() && Self::is_type_text_identifier_continue(bytes[i] as char) {
                i += 1;
            }

            let ident = &type_text[start..i];
            let prev_non_ws = type_text[..start]
                .chars()
                .rev()
                .find(|c| !c.is_ascii_whitespace());
            if prev_non_ws == Some('.')
                || Self::is_non_type_text_identifier_candidate(ident)
                || Self::type_text_identifier_is_member_name(type_text, i)
            {
                continue;
            }

            if self.local_identifier_requires_serialization_guard(ident, visited_names) {
                return true;
            }
        }

        false
    }

    pub(in crate::declaration_emitter) fn type_text_identifier_is_member_name(
        type_text: &str,
        end: usize,
    ) -> bool {
        let mut iter = type_text[end..]
            .char_indices()
            .skip_while(|(_, ch)| ch.is_ascii_whitespace());
        let Some((offset, ch)) = iter.next() else {
            return false;
        };

        if ch == ':' {
            return true;
        }

        if ch != '?' {
            return false;
        }

        type_text[end + offset + ch.len_utf8()..]
            .chars()
            .find(|next| !next.is_ascii_whitespace())
            == Some(':')
    }

    pub(in crate::declaration_emitter) fn local_identifier_requires_serialization_guard(
        &self,
        ident: &str,
        visited_names: &mut rustc_hash::FxHashSet<String>,
    ) -> bool {
        if !visited_names.insert(ident.to_string()) {
            return false;
        }

        self.current_file_declaration_requires_serialization_guard(ident, visited_names)
    }

    pub(in crate::declaration_emitter) fn current_file_declaration_requires_serialization_guard(
        &self,
        ident: &str,
        visited_names: &mut rustc_hash::FxHashSet<String>,
    ) -> bool {
        let Some(decl_idx) = self.current_file_top_level_declaration_named(ident) else {
            // Declaration not found in current file - no guard needed from this file's perspective
            return false;
        };
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return false;
        };

        // For declarations in the current file, we don't need a serialization guard
        // when referencing them from the same file. The declaration will be emitted
        // in the .d.ts output (even if not exported), so it's visible to other
        // declarations in the same file.
        // Only type aliases that reference non-emittable types need guards.
        if let Some(alias) = self.arena.get_type_alias(decl_node)
            && let Some(alias_type_text) = self.emit_type_node_text(alias.type_node)
            && self.type_text_uses_non_emittable_local_alias_root(&alias_type_text, visited_names)
        {
            return true;
        }

        false
    }

    pub(in crate::declaration_emitter) fn current_file_top_level_declaration_named(
        &self,
        ident: &str,
    ) -> Option<NodeIndex> {
        let source_idx = self.current_source_file_idx?;
        let source_node = self.arena.get(source_idx)?;
        let source_file = self.arena.get_source_file(source_node)?;

        for &stmt_idx in &source_file.statements.nodes {
            let stmt_node = self.arena.get(stmt_idx)?;

            if self.extract_declaration_name(stmt_idx).as_deref() == Some(ident) {
                return Some(stmt_idx);
            }

            if let Some(var_stmt) = self.arena.get_variable(stmt_node) {
                for &decl_list_idx in &var_stmt.declarations.nodes {
                    let decl_list_node = self.arena.get(decl_list_idx)?;
                    let decl_list = self.arena.get_variable(decl_list_node)?;
                    for &decl_idx in &decl_list.declarations.nodes {
                        let decl_node = self.arena.get(decl_idx)?;
                        let decl = self.arena.get_variable_declaration(decl_node)?;
                        if self.get_identifier_text(decl.name).as_deref() == Some(ident) {
                            return Some(decl_idx);
                        }
                    }
                }
            }
        }

        None
    }

    #[allow(dead_code)]
    pub(in crate::declaration_emitter) fn declaration_name_idx_from_source_arena(
        &self,
        source_arena: &NodeArena,
        decl_node: &tsz_parser::parser::node::Node,
    ) -> Option<NodeIndex> {
        source_arena
            .get_function(decl_node)
            .map(|func| func.name)
            .or_else(|| source_arena.get_class(decl_node).map(|class| class.name))
            .or_else(|| {
                source_arena
                    .get_interface(decl_node)
                    .map(|iface| iface.name)
            })
            .or_else(|| {
                source_arena
                    .get_type_alias(decl_node)
                    .map(|alias| alias.name)
            })
            .or_else(|| {
                source_arena
                    .get_enum(decl_node)
                    .map(|enum_data| enum_data.name)
            })
            .or_else(|| {
                source_arena
                    .get_variable_declaration(decl_node)
                    .map(|decl| decl.name)
            })
            .filter(|name_idx| name_idx.is_some())
    }

    #[allow(dead_code)]
    pub(in crate::declaration_emitter) fn declaration_is_publicly_emittable(
        &self,
        decl_node: &tsz_parser::parser::node::Node,
    ) -> bool {
        if let Some(name_idx) = self.declaration_name_idx_from_source_arena(self.arena, decl_node)
            && self.should_emit_public_api_dependency(name_idx)
        {
            return true;
        }

        self.stmt_has_export_modifier(decl_node)
    }

    const fn is_type_text_identifier_start(ch: char) -> bool {
        ch.is_ascii_alphabetic() || ch == '_' || ch == '$'
    }

    const fn is_type_text_identifier_continue(ch: char) -> bool {
        ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
    }

    pub(in crate::declaration_emitter) fn is_non_type_text_identifier_candidate(
        ident: &str,
    ) -> bool {
        matches!(
            ident,
            "any"
                | "as"
                | "asserts"
                | "bigint"
                | "boolean"
                | "false"
                | "get"
                | "import"
                | "in"
                | "infer"
                | "is"
                | "keyof"
                | "never"
                | "new"
                | "null"
                | "number"
                | "object"
                | "readonly"
                | "set"
                | "static"
                | "string"
                | "symbol"
                | "this"
                | "true"
                | "typeof"
                | "undefined"
                | "unique"
                | "unknown"
                | "void"
        )
    }

    pub(in crate::declaration_emitter) fn emit_non_portable_type_node_diagnostic_from_arena(
        &mut self,
        arena: &NodeArena,
        node_idx: NodeIndex,
        decl_name: &str,
        file: &str,
        pos: u32,
        length: u32,
    ) -> bool {
        if !node_idx.is_some() {
            return false;
        }

        let arena_addr = arena as *const NodeArena as usize;
        let Some(source_path) = self.arena_to_path.get(&arena_addr).cloned() else {
            return false;
        };

        let mut visited_symbols = rustc_hash::FxHashSet::default();
        let mut visited_declaration_symbols = rustc_hash::FxHashSet::default();
        let mut visited_nodes = rustc_hash::FxHashSet::default();
        let mut visited_types = rustc_hash::FxHashSet::default();
        let mut seen = rustc_hash::FxHashSet::default();
        let mut references = Vec::new();
        self.collect_non_portable_references_in_type_node(
            arena,
            node_idx,
            &source_path,
            &mut references,
            &mut seen,
            &mut visited_types,
            &mut visited_symbols,
            &mut visited_declaration_symbols,
            &mut visited_nodes,
        );
        let mut indexed_access_object_names = rustc_hash::FxHashSet::default();
        let mut visited_indexed_access_nodes = rustc_hash::FxHashSet::default();
        self.collect_indexed_access_object_type_names(
            arena,
            node_idx,
            &mut indexed_access_object_names,
            &mut visited_indexed_access_nodes,
        );
        let drop_names: rustc_hash::FxHashSet<_> = indexed_access_object_names
            .into_iter()
            .filter(|name| references.iter().any(|(_, other_name)| other_name != name))
            .collect();
        if !drop_names.is_empty() {
            references.retain(|(_, type_name)| !drop_names.contains(type_name));
        }
        if references.is_empty() {
            return false;
        }
        for (from_path, type_name) in references {
            self.emit_non_portable_named_reference_diagnostic(
                decl_name, file, pos, length, &from_path, &type_name,
            );
        }
        true
    }

    pub(in crate::declaration_emitter) fn find_symbol_for_import_type_text(
        &self,
        printed: &str,
    ) -> Option<SymbolId> {
        let (module_specifier, first_name) = self.parse_import_type_text(printed)?;
        let binder = self.binder?;
        let current_path = self.current_file_path.as_deref()?;

        binder
            .symbols
            .iter()
            .filter_map(|symbol| {
                if symbol.escaped_name != first_name {
                    return None;
                }
                let source_arena = binder.symbol_arenas.get(&symbol.id)?;
                let arena_addr = Arc::as_ptr(source_arena) as usize;
                let source_path = self.arena_to_path.get(&arena_addr)?;
                let candidate =
                    if module_specifier.starts_with('.') || module_specifier.starts_with('/') {
                        self.strip_ts_extensions(
                            &self.calculate_relative_path(current_path, source_path),
                        )
                    } else {
                        self.package_specifier_for_node_modules_path(current_path, source_path)?
                    };
                (candidate == module_specifier
                    || (!module_specifier.starts_with('.')
                        && !module_specifier.starts_with('/')
                        && candidate.starts_with(&format!("{module_specifier}/"))))
                .then_some((symbol.id, source_path.clone()))
            })
            // Prefer the deepest matching source path so symlinked package
            // trees win over a flattened top-level node_modules copy.
            .max_by_key(|(_, source_path)| source_path.len())
            .map(|(sym_id, _)| sym_id)
    }

    pub(in crate::declaration_emitter) fn parse_import_type_text(
        &self,
        printed: &str,
    ) -> Option<(String, String)> {
        let rest = printed.strip_prefix("import(\"")?;
        let (module_specifier, tail) = rest.split_once("\")")?;
        let tail = tail.strip_prefix('.')?;
        let first_name = tail
            .split(['.', '<', '[', ' ', '&', '|'])
            .find(|part| !part.is_empty())?;
        Some((module_specifier.to_string(), first_name.to_string()))
    }

    pub(in crate::declaration_emitter) fn private_import_type_package_root_reference(
        &self,
        printed: &str,
    ) -> Option<(String, String)> {
        let (module_specifier, type_name) = self.parse_import_type_text(printed)?;
        if module_specifier.starts_with('.') || module_specifier.starts_with('/') {
            return None;
        }

        let mut parts = module_specifier.split('/');
        let first = parts.next()?;
        if first.is_empty() {
            return None;
        }

        let package_name = if first.starts_with('@') {
            format!("{}/{}", first, parts.next()?)
        } else {
            first.to_string()
        };

        if package_name == module_specifier {
            return None;
        }

        Some((format!("./node_modules/{package_name}"), type_name))
    }

    pub(crate) fn printed_type_uses_private_import_type_root(&self, printed: &str) -> bool {
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(current_file_path) = self.current_file_path.as_deref() else {
            return false;
        };

        let mut remaining = printed;
        while let Some(start) = remaining.find("import(\"") {
            let after_prefix = &remaining[start + "import(\"".len()..];
            let Some((module_specifier, tail)) = after_prefix.split_once("\")") else {
                break;
            };
            remaining = tail;

            let Some(root_name) = tail.strip_prefix('.').and_then(|rest| {
                rest.split(['.', '<', '[', ' ', '&', '|', '(', ')', ',', '?', '{', '}'])
                    .find(|part| !part.is_empty())
            }) else {
                continue;
            };

            let exported = binder
                .module_exports
                .iter()
                .find_map(|(module_path, exports)| {
                    let candidate = if module_specifier.starts_with('.')
                        || module_specifier.starts_with('/')
                    {
                        Some(self.strip_ts_extensions(
                            &self.calculate_relative_path(current_file_path, module_path),
                        ))
                    } else {
                        self.package_specifier_for_node_modules_path(current_file_path, module_path)
                    }?;
                    (candidate == module_specifier).then(|| exports.has(root_name))
                });

            if exported == Some(false) {
                return true;
            }
        }

        false
    }

    pub(in crate::declaration_emitter) fn non_portable_namespace_member_reference(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
        source_path: &str,
    ) -> Option<(String, String)> {
        let node = arena.get(node_idx)?;
        let (left_idx, right_idx) = if let Some(access) = arena.get_access_expr(node) {
            (access.expression, access.name_or_argument)
        } else if let Some(qn) = arena.get_qualified_name(node) {
            (qn.left, qn.right)
        } else {
            return None;
        };

        let left_name = Self::rightmost_name_text_in_arena(arena, left_idx)?;
        let type_name = Self::rightmost_name_text_in_arena(arena, right_idx)?;
        if let Some(sym_id) = self.find_symbol_in_arena_by_name(arena, &left_name) {
            let binder = self.binder?;
            let symbol = binder.symbols.get(sym_id)?;
            if let Some(import_module) = symbol.import_module.as_deref() {
                if import_module.starts_with('.') || import_module.starts_with('/') {
                    return None;
                }
                let from_path =
                    self.transitive_dependency_from_import(source_path, import_module)?;
                return Some((from_path, type_name));
            }
        }

        let source_text = std::fs::read_to_string(source_path).ok()?;
        if let Some(import_module) =
            self.namespace_import_module_from_text(&source_text, &left_name)
            && !import_module.starts_with('.')
            && !import_module.starts_with('/')
        {
            let from_path = self.transitive_dependency_from_import(source_path, &import_module)?;
            return Some((from_path, type_name));
        }

        self.reference_types_namespace_member_reference_from_text(
            &source_text,
            &left_name,
            &type_name,
        )
    }

    pub(in crate::declaration_emitter) fn rightmost_name_text_in_arena(
        arena: &NodeArena,
        idx: NodeIndex,
    ) -> Option<String> {
        let node = arena.get(idx)?;
        if let Some(ident) = arena.get_identifier(node) {
            return Some(ident.escaped_text.clone());
        }
        if let Some(qn) = arena.get_qualified_name(node) {
            return Self::rightmost_name_text_in_arena(arena, qn.right);
        }
        if let Some(access) = arena.get_access_expr(node) {
            return Self::rightmost_name_text_in_arena(arena, access.name_or_argument);
        }
        None
    }

    pub(in crate::declaration_emitter) fn find_symbol_in_arena_by_name(
        &self,
        arena: &NodeArena,
        name: &str,
    ) -> Option<SymbolId> {
        let binder = self.binder?;
        let arena_addr = arena as *const NodeArena as usize;

        binder.symbols.iter().find_map(|symbol| {
            if symbol.escaped_name != name {
                return None;
            }
            let sym_arena = binder.symbol_arenas.get(&symbol.id)?;
            ((Arc::as_ptr(sym_arena) as usize) == arena_addr).then_some(symbol.id)
        })
    }

    pub(in crate::declaration_emitter) fn transitive_dependency_from_import(
        &self,
        source_path: &str,
        import_module: &str,
    ) -> Option<String> {
        use std::path::{Component, Path};

        let components: Vec<_> = Path::new(source_path).components().collect();
        let nm_positions: Vec<usize> = components
            .iter()
            .enumerate()
            .filter_map(|(i, c)| match c {
                Component::Normal(part) if part.to_str() == Some("node_modules") => Some(i),
                _ => None,
            })
            .collect();
        let last_nm = *nm_positions.last()?;
        let pkg_start = last_nm + 1;
        let pkg_len = if components.get(pkg_start).is_some_and(
            |c| matches!(c, Component::Normal(p) if p.to_str().is_some_and(|s| s.starts_with('@'))),
        ) {
            2
        } else {
            1
        };
        let parent_package: Vec<String> = components[pkg_start..pkg_start + pkg_len]
            .iter()
            .filter_map(|c| match c {
                Component::Normal(part) => part.to_str().map(str::to_string),
                _ => None,
            })
            .collect();
        (!parent_package.is_empty()).then(|| {
            format!(
                "{}/node_modules/{}",
                parent_package.join("/"),
                import_module
            )
        })
    }

    pub(in crate::declaration_emitter) fn reference_types_namespace_member_reference_from_text(
        &self,
        source_text: &str,
        left_name: &str,
        type_name: &str,
    ) -> Option<(String, String)> {
        let current_file_path = self.current_file_path.as_deref()?;
        let binder = self.binder?;

        for types_ref in self.extract_reference_types_from_text(source_text) {
            if !types_ref.eq_ignore_ascii_case(left_name) {
                continue;
            }

            if let Some(module_path) = self
                .matching_module_export_paths(binder, current_file_path, &types_ref)
                .into_iter()
                .next()
            {
                let mut from_path = self.strip_ts_extensions(
                    &self.calculate_relative_path(current_file_path, module_path),
                );
                if from_path.ends_with("/index") {
                    from_path.truncate(from_path.len() - "/index".len());
                }
                from_path = Self::ts2883_relative_node_modules_path(from_path);
                return Some((from_path, type_name.to_string()));
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn namespace_import_module_from_text(
        &self,
        source_text: &str,
        alias_name: &str,
    ) -> Option<String> {
        for line in source_text.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("import * as ") {
                let (alias, rest) = rest.split_once(" from ")?;
                if alias.trim() != alias_name {
                    continue;
                }
                let module = rest.trim().trim_end_matches(';');
                return Self::quoted_string_text(module);
            }
            if let Some(rest) = trimmed.strip_prefix("import ")
                && let Some((alias, rhs)) = rest.split_once(" = require(")
            {
                if alias.trim() != alias_name {
                    continue;
                }
                let module = rhs.trim().trim_end_matches(");").trim_end_matches(')');
                return Self::quoted_string_text(module);
            }
        }
        None
    }

    pub(in crate::declaration_emitter) fn quoted_string_text(text: &str) -> Option<String> {
        let trimmed = text.trim();
        let quote = trimmed.chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let rest = &trimmed[quote.len_utf8()..];
        let end = rest.find(quote)?;
        Some(rest[..end].to_string())
    }

    pub(in crate::declaration_emitter) fn extract_reference_types_from_text(
        &self,
        source_text: &str,
    ) -> Vec<String> {
        source_text
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if !trimmed.starts_with("///")
                    || !trimmed.contains("<reference")
                    || !trimmed.contains("types=")
                {
                    return None;
                }

                let attr_start = trimmed.find("types=")?;
                let after = &trimmed[attr_start + "types=".len()..];
                let quote = after.chars().next()?;
                if quote != '"' && quote != '\'' {
                    return None;
                }
                let rest = &after[quote.len_utf8()..];
                let end = rest.find(quote)?;
                Some(rest[..end].to_string())
            })
            .collect()
    }

    pub(in crate::declaration_emitter) fn emit_non_portable_named_reference_diagnostic(
        &mut self,
        decl_name: &str,
        file: &str,
        pos: u32,
        length: u32,
        from_path: &str,
        type_name: &str,
    ) {
        use tsz_common::diagnostics::Diagnostic;

        self.diagnostics.push(Diagnostic::from_code(
            2883,
            file,
            pos,
            length,
            &[decl_name, from_path, type_name],
        ));
    }

    pub(in crate::declaration_emitter) fn type_text_is_directly_nameable_reference(
        &self,
        printed: &str,
    ) -> bool {
        if printed == "any" || printed.is_empty() {
            return false;
        }

        if printed.starts_with("import(\"") {
            return printed.contains("\").")
                && !self.import_type_uses_private_package_subpath(printed)
                && !printed.contains(" & ")
                && !printed.contains(" | ")
                && !printed.contains("{ ")
                && !printed.contains('[')
                && !printed.contains('\n');
        }

        let bytes = printed.as_bytes();
        let Some(&first) = bytes.first() else {
            return false;
        };
        if !matches!(first, b'A'..=b'Z' | b'a'..=b'z' | b'_') {
            return false;
        }

        !printed.contains(" & ")
            && !printed.contains(" | ")
            && !printed.contains("{ ")
            && !printed.contains('[')
            && !printed.contains('(')
            && !printed.contains('\n')
    }

    /// Check whether the printed type text contains any `import("...")` reference
    /// whose module specifier is a private package subpath (has a `/` after the
    /// bare package name).  This scans all `import("...")` occurrences in the
    /// text, not just the leading one.
    ///
    /// When the printed type text has NO such non-portable import references,
    /// the type is already nameable from the consumer's perspective and the
    /// deeper type-graph portability walk can be skipped.
    #[allow(dead_code)]
    pub(in crate::declaration_emitter) fn printed_type_contains_non_portable_import(
        &self,
        printed: &str,
    ) -> bool {
        let mut remaining = printed;
        while let Some(start) = remaining.find("import(\"") {
            let after_prefix = &remaining[start + 8..]; // skip `import("`
            if let Some((specifier, rest)) = after_prefix.split_once("\")") {
                if !specifier.starts_with('.') && !specifier.starts_with('/') {
                    let mut parts = specifier.split('/');
                    if let Some(first) = parts.next()
                        && !first.is_empty()
                    {
                        let has_subpath = if first.starts_with('@') {
                            let _scope_pkg = parts.next();
                            parts.next().is_some()
                        } else {
                            parts.next().is_some()
                        };
                        if has_subpath
                            && !self.is_bare_specifier_subpath_publicly_accessible(specifier)
                        {
                            return true;
                        }
                    }
                }
                remaining = rest;
            } else {
                break;
            }
        }
        false
    }

    pub(crate) fn import_type_uses_private_package_subpath(&self, printed: &str) -> bool {
        let Some(rest) = printed.strip_prefix("import(\"") else {
            return false;
        };
        let Some((specifier, _)) = rest.split_once("\")") else {
            return false;
        };

        if specifier.starts_with('.') || specifier.starts_with('/') {
            return false;
        }

        let mut parts = specifier.split('/');
        let Some(first) = parts.next() else {
            return false;
        };
        if first.is_empty() {
            return false;
        }

        let has_subpath = if first.starts_with('@') {
            let _package = parts.next();
            parts.next().is_some()
        } else {
            parts.next().is_some()
        };

        has_subpath && !self.is_bare_specifier_subpath_publicly_accessible(specifier)
    }

    /// Check whether a bare package specifier with a subpath is publicly accessible.
    /// Returns `true` when the package has no `exports` field (all subpaths accessible)
    /// or the exports map explicitly maps the subpath.
    pub(in crate::declaration_emitter) fn is_bare_specifier_subpath_publicly_accessible(
        &self,
        specifier: &str,
    ) -> bool {
        use std::path::Path;

        let mut parts = specifier.split('/');
        let Some(first) = parts.next() else {
            return false;
        };
        let (package_name, subpath) = if first.starts_with('@') {
            let scope_pkg = parts.next().unwrap_or("");
            let pkg_name = format!("{first}/{scope_pkg}");
            let rest: Vec<&str> = parts.collect();
            if rest.is_empty() {
                return false;
            }
            (pkg_name, rest.join("/"))
        } else {
            let rest: Vec<&str> = parts.collect();
            if rest.is_empty() {
                return false;
            }
            (first.to_string(), rest.join("/"))
        };

        let package_root = self.find_package_root_for_name(&package_name);
        let Some(package_root) = package_root else {
            return false;
        };

        let pkg_json_path = Path::new(&package_root).join("package.json");
        let Ok(pkg_content) = std::fs::read_to_string(&pkg_json_path) else {
            return false;
        };
        let Ok(pkg_json) = serde_json::from_str::<serde_json::Value>(&pkg_content) else {
            return false;
        };

        let Some(exports) = pkg_json.get("exports") else {
            // No exports field: all subpaths are accessible (CommonJS-style package).
            return true;
        };

        let export_subpath = format!("./{subpath}");
        self.exports_map_allows_subpath(exports, &export_subpath)
    }

    /// Find the filesystem path of a package root directory.
    pub(in crate::declaration_emitter) fn find_package_root_for_name(
        &self,
        package_name: &str,
    ) -> Option<String> {
        let needle = format!("node_modules/{package_name}/");
        for source_path in self.arena_to_path.values() {
            if let Some(idx) = source_path.find(&needle) {
                return Some(source_path[..idx + needle.len() - 1].to_string());
            }
        }
        if let Some(binder) = self.binder {
            for module_path in binder.module_exports.keys() {
                if let Some(idx) = module_path.find(&needle) {
                    return Some(module_path[..idx + needle.len() - 1].to_string());
                }
            }
        }
        None
    }

    /// Check whether a package's exports map allows a given subpath.
    pub(in crate::declaration_emitter) fn exports_map_allows_subpath(
        &self,
        exports: &serde_json::Value,
        subpath: &str,
    ) -> bool {
        match exports {
            serde_json::Value::String(target) => {
                subpath == "." || self.match_export_target(".", target, subpath).is_some()
            }
            serde_json::Value::Array(entries) => entries
                .iter()
                .any(|entry| self.exports_map_allows_subpath(entry, subpath)),
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    if key == "." || key.starts_with("./") {
                        if self.export_entry_matches_subpath(key, value, subpath) {
                            return true;
                        }
                    } else if self.exports_map_allows_subpath(value, subpath) {
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    pub(in crate::declaration_emitter) fn export_entry_matches_subpath(
        &self,
        key: &str,
        value: &serde_json::Value,
        subpath: &str,
    ) -> bool {
        if key == subpath {
            return true;
        }
        if key.contains('*') && self.match_exports_wildcard(key, subpath).is_some() {
            return true;
        }
        if key.ends_with('/') && subpath.starts_with(key) {
            return true;
        }
        match value {
            serde_json::Value::Object(map) => {
                for (k, v) in map {
                    if !k.starts_with("./") && k != "." {
                        // Condition key: recurse to check if any branch has a target
                        if self.export_entry_matches_subpath(key, v, subpath) {
                            return true;
                        }
                    }
                }
                false
            }
            serde_json::Value::Array(entries) => entries
                .iter()
                .any(|entry| self.export_entry_matches_subpath(key, entry, subpath)),
            _ => false,
        }
    }

    /// Scan a type for non-portable symbol references by checking all
    /// referenced types for symbols from nested `node_modules`.
    ///
    /// Returns `Some((from_path, type_name))` for the first non-portable reference found.
    pub(in crate::declaration_emitter) fn find_non_portable_type_reference(
        &self,
        type_id: tsz_solver::types::TypeId,
    ) -> Option<(String, String)> {
        let mut visited_types = rustc_hash::FxHashSet::default();
        let mut visited_symbols = rustc_hash::FxHashSet::default();
        let mut visited_declaration_symbols = rustc_hash::FxHashSet::default();
        let mut visited_nodes = rustc_hash::FxHashSet::default();
        self.find_non_portable_type_reference_inner(
            type_id,
            &mut visited_types,
            &mut visited_symbols,
            &mut visited_declaration_symbols,
            &mut visited_nodes,
        )
    }

    pub(in crate::declaration_emitter) fn find_non_portable_type_reference_inner(
        &self,
        type_id: tsz_solver::types::TypeId,
        visited_types: &mut rustc_hash::FxHashSet<tsz_solver::types::TypeId>,
        visited_symbols: &mut rustc_hash::FxHashSet<SymbolId>,
        visited_declaration_symbols: &mut rustc_hash::FxHashSet<SymbolId>,
        visited_nodes: &mut rustc_hash::FxHashSet<(usize, u32)>,
    ) -> Option<(String, String)> {
        #[allow(clippy::too_many_arguments)]
        fn check_named_type(
            emitter: &DeclarationEmitter<'_>,
            interner: &tsz_solver::TypeInterner,
            binder: &BinderState,
            current_file_path: &str,
            cache: &crate::type_cache_view::TypeCacheView,
            candidate_type_id: tsz_solver::types::TypeId,
            visited_types: &mut rustc_hash::FxHashSet<tsz_solver::types::TypeId>,
            visited_symbols: &mut rustc_hash::FxHashSet<SymbolId>,
            visited_declaration_symbols: &mut rustc_hash::FxHashSet<SymbolId>,
            visited_nodes: &mut rustc_hash::FxHashSet<(usize, u32)>,
        ) -> Option<(String, String)> {
            if let Some(def_id) = tsz_solver::lazy_def_id(interner, candidate_type_id)
                && let Some(&sym_id) = cache.def_to_symbol.get(&def_id)
            {
                if let Some(result) = emitter.check_symbol_portability(
                    sym_id,
                    binder,
                    current_file_path,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                ) {
                    return Some(result);
                }
                if let Some(result) = emitter
                    .collect_non_portable_references_in_symbol_declaration_with_state(
                        sym_id,
                        visited_types,
                        visited_symbols,
                        visited_declaration_symbols,
                        visited_nodes,
                    )
                    .into_iter()
                    .next()
                {
                    return Some(result);
                }
            }

            if let Some(shape_id) = tsz_solver::object_shape_id(interner, candidate_type_id) {
                let shape = interner.object_shape(shape_id);
                if let Some(sym_id) = shape.symbol
                    && let Some(result) = emitter.check_symbol_portability(
                        sym_id,
                        binder,
                        current_file_path,
                        visited_types,
                        visited_symbols,
                        visited_declaration_symbols,
                        visited_nodes,
                    )
                {
                    return Some(result);
                }
            }

            if let Some(callable_id) =
                tsz_solver::visitor::callable_shape_id(interner, candidate_type_id)
            {
                let shape = interner.callable_shape(callable_id);
                if let Some(sym_id) = shape.symbol
                    && let Some(result) = emitter.check_symbol_portability(
                        sym_id,
                        binder,
                        current_file_path,
                        visited_types,
                        visited_symbols,
                        visited_declaration_symbols,
                        visited_nodes,
                    )
                {
                    return Some(result);
                }
            }

            None
        }

        let interner = self.type_interner?;
        let binder = self.binder?;
        let current_file_path = self.current_file_path.as_deref()?;
        let cache = self.type_cache.as_ref()?;

        if !visited_types.insert(type_id) {
            return None;
        }

        if let Some(result) = check_named_type(
            self,
            interner,
            binder,
            current_file_path,
            cache,
            type_id,
            visited_types,
            visited_symbols,
            visited_declaration_symbols,
            visited_nodes,
        ) {
            return Some(result);
        }

        if self.type_has_public_surface_reference_with_portable_arguments(
            type_id,
            visited_types,
            visited_symbols,
            visited_declaration_symbols,
            visited_nodes,
        ) {
            return None;
        }

        if let Some(alias_type_id) = interner.get_display_alias(type_id)
            && alias_type_id != type_id
        {
            if let Some(result) = check_named_type(
                self,
                interner,
                binder,
                current_file_path,
                cache,
                alias_type_id,
                visited_types,
                visited_symbols,
                visited_declaration_symbols,
                visited_nodes,
            ) {
                return Some(result);
            }
            if let Some(app_id) = tsz_solver::visitor::application_id(interner, alias_type_id) {
                let app = interner.type_application(app_id);
                if let Some(result) = check_named_type(
                    self,
                    interner,
                    binder,
                    current_file_path,
                    cache,
                    app.base,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                ) {
                    return Some(result);
                }
                if self.type_application_has_public_surface_reference_with_portable_arguments(
                    app.base,
                    &app.args,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                ) {
                    return None;
                }
            }
        }

        if let Some(app_id) = tsz_solver::visitor::application_id(interner, type_id) {
            let app = interner.type_application(app_id);
            if let Some(result) = check_named_type(
                self,
                interner,
                binder,
                current_file_path,
                cache,
                app.base,
                visited_types,
                visited_symbols,
                visited_declaration_symbols,
                visited_nodes,
            ) {
                return Some(result);
            }
        }

        if let Some(callable_id) = tsz_solver::visitor::callable_shape_id(interner, type_id) {
            let shape = interner.callable_shape(callable_id);
            for sig in &shape.call_signatures {
                for param in &sig.params {
                    if let Some(result) = check_named_type(
                        self,
                        interner,
                        binder,
                        current_file_path,
                        cache,
                        param.type_id,
                        visited_types,
                        visited_symbols,
                        visited_declaration_symbols,
                        visited_nodes,
                    ) {
                        return Some(result);
                    }
                }
                if let Some(result) = check_named_type(
                    self,
                    interner,
                    binder,
                    current_file_path,
                    cache,
                    sig.return_type,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                ) {
                    return Some(result);
                }
            }
            for sig in &shape.construct_signatures {
                for param in &sig.params {
                    if let Some(result) = check_named_type(
                        self,
                        interner,
                        binder,
                        current_file_path,
                        cache,
                        param.type_id,
                        visited_types,
                        visited_symbols,
                        visited_declaration_symbols,
                        visited_nodes,
                    ) {
                        return Some(result);
                    }
                }
                if let Some(result) = check_named_type(
                    self,
                    interner,
                    binder,
                    current_file_path,
                    cache,
                    sig.return_type,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                ) {
                    return Some(result);
                }
            }
        }

        // Collect all types referenced by this type (deeply walks into
        // objects, tuples, unions, intersections, etc.)
        let referenced_types = tsz_solver::visitor::collect_referenced_types(interner, type_id);
        for &ref_type_id in &referenced_types {
            if let Some(symbol_ref) = tsz_solver::visitor::type_query_symbol(interner, ref_type_id)
                && let Some(sym_id) = binder
                    .symbols
                    .get(SymbolId(symbol_ref.0))
                    .map(|_| SymbolId(symbol_ref.0))
                && let Some(result) = self.check_symbol_portability(
                    sym_id,
                    binder,
                    current_file_path,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                )
            {
                return Some(result);
            }

            if let Some(symbol_ref) =
                tsz_solver::visitor::module_namespace_symbol_ref(interner, ref_type_id)
                && let Some(sym_id) = binder
                    .symbols
                    .get(SymbolId(symbol_ref.0))
                    .map(|_| SymbolId(symbol_ref.0))
                && let Some(result) = self.check_symbol_portability(
                    sym_id,
                    binder,
                    current_file_path,
                    visited_types,
                    visited_symbols,
                    visited_declaration_symbols,
                    visited_nodes,
                )
            {
                return Some(result);
            }

            if let Some(result) = check_named_type(
                self,
                interner,
                binder,
                current_file_path,
                cache,
                ref_type_id,
                visited_types,
                visited_symbols,
                visited_declaration_symbols,
                visited_nodes,
            ) {
                return Some(result);
            }
        }

        None
    }

    fn type_has_public_surface_reference_with_portable_arguments(
        &self,
        type_id: tsz_solver::types::TypeId,
        visited_types: &mut rustc_hash::FxHashSet<tsz_solver::types::TypeId>,
        visited_symbols: &mut rustc_hash::FxHashSet<SymbolId>,
        visited_declaration_symbols: &mut rustc_hash::FxHashSet<SymbolId>,
        visited_nodes: &mut rustc_hash::FxHashSet<(usize, u32)>,
    ) -> bool {
        let Some(interner) = self.type_interner else {
            return false;
        };
        if let Some(app_id) = tsz_solver::visitor::application_id(interner, type_id) {
            let app = interner.type_application(app_id);
            return self.type_application_has_public_surface_reference_with_portable_arguments(
                app.base,
                &app.args,
                visited_types,
                visited_symbols,
                visited_declaration_symbols,
                visited_nodes,
            );
        }
        false
    }

    fn type_application_has_public_surface_reference_with_portable_arguments(
        &self,
        base: tsz_solver::types::TypeId,
        args: &[tsz_solver::types::TypeId],
        visited_types: &mut rustc_hash::FxHashSet<tsz_solver::types::TypeId>,
        visited_symbols: &mut rustc_hash::FxHashSet<SymbolId>,
        visited_declaration_symbols: &mut rustc_hash::FxHashSet<SymbolId>,
        visited_nodes: &mut rustc_hash::FxHashSet<(usize, u32)>,
    ) -> bool {
        if !self.type_id_is_public_package_export(base) {
            return false;
        }
        args.iter().copied().all(|arg| {
            self.find_non_portable_type_reference_inner(
                arg,
                visited_types,
                visited_symbols,
                visited_declaration_symbols,
                visited_nodes,
            )
            .is_none()
        })
    }

    fn type_id_is_public_package_export(&self, type_id: tsz_solver::types::TypeId) -> bool {
        let Some(interner) = self.type_interner else {
            return false;
        };
        let Some(cache) = self.type_cache.as_ref() else {
            return false;
        };
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(current_file_path) = self.current_file_path.as_deref() else {
            return false;
        };
        let Some(def_id) = tsz_solver::lazy_def_id(interner, type_id) else {
            return false;
        };
        let Some(&sym_id) = cache.def_to_symbol.get(&def_id) else {
            return false;
        };
        let resolved = self.resolve_portability_symbol(sym_id, binder);
        let Some(symbol) = binder.symbols.get(resolved) else {
            return false;
        };
        self.package_root_export_reference_path(
            resolved,
            symbol.escaped_name.as_str(),
            binder,
            current_file_path,
        )
        .is_some()
    }

    /// Check if a symbol comes from a non-portable module path.
    ///
    /// Returns `Some((from_path, type_name))` if the symbol is non-portable, where:
    /// - `from_path` is the problematic module path for the diagnostic message
    /// - `type_name` is the symbol name that can't be referenced
    #[allow(clippy::too_many_arguments)]
    pub(in crate::declaration_emitter) fn check_symbol_portability(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
        current_file_path: &str,
        visited_types: &mut rustc_hash::FxHashSet<tsz_solver::types::TypeId>,
        visited_symbols: &mut rustc_hash::FxHashSet<SymbolId>,
        visited_declaration_symbols: &mut rustc_hash::FxHashSet<SymbolId>,
        visited_nodes: &mut rustc_hash::FxHashSet<(usize, u32)>,
    ) -> Option<(String, String)> {
        use std::path::{Component, Path};

        let original_sym_id = sym_id;
        let original_symbol = binder.symbols.get(original_sym_id)?;
        let original_type_name = original_symbol.escaped_name.clone();
        let original_source_path = self.get_symbol_source_path(original_sym_id, binder)?;
        // Only check for transitive imports when the source path has nested
        // node_modules (i.e., 2+ occurrences).  A single node_modules means
        // the package is a direct dependency and its exports are portable.
        let nm_count_in_original = Path::new(&original_source_path)
            .components()
            .filter(|c| matches!(c, Component::Normal(part) if *part == "node_modules"))
            .count();
        if nm_count_in_original >= 2
            && original_symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS)
            && let Some(import_module) = original_symbol.import_module.as_deref()
            && !import_module.starts_with('.')
            && !import_module.starts_with('/')
            && self
                .package_root_export_reference_path(
                    original_sym_id,
                    &original_type_name,
                    binder,
                    current_file_path,
                )
                .is_some()
        {
            let from_path = self.transitive_import_module_reference_path(
                import_module,
                binder,
                current_file_path,
            );
            if let Some(from_path) = from_path {
                return Some((from_path, original_type_name));
            }
        }

        let sym_id = self.resolve_portability_symbol(sym_id, binder);
        if !visited_symbols.insert(sym_id) {
            return None;
        }
        let symbol = binder.symbols.get(sym_id)?;
        let type_name = symbol.escaped_name.clone();
        let source_path = self.get_symbol_source_path(sym_id, binder)?;

        // Symbols declared inside `declare module "..."` are portable through the
        // ambient module specifier even when the backing `.d.ts` file lives in a
        // package subpath such as `@types/node/fs.d.ts` or `ext/ts3.1/index.d.ts`.
        // TS2883 should only fire when the symbol truly lacks a public module path.
        if self.check_ambient_module(sym_id, binder).is_some() {
            return None;
        }

        // If the symbol is re-exported from a module accessible via a bare
        // package specifier (no subpath), the type IS portable -- consumers
        // can reference it through the package root.  tsc does not emit
        // TS2883 in this situation, even if the type's internal definition
        // references non-exported helper types (those are internal details
        // of the library, not the consumer's concern).
        if self
            .package_root_export_reference_path(sym_id, &type_name, binder, current_file_path)
            .is_some()
        {
            return None;
        }

        // Symlinked-monorepo / nested-package case: when the source path is
        // `<X>/node_modules/<P>/<sub>` but `<X>` is not an ancestor of the
        // consumer's directory, the package was reached only by traversing
        // a symlinked / nested `node_modules` outside the consumer's normal
        // Node.js resolution scope. Writing `<P>` as a bare specifier from
        // the consumer would not resolve to the same file. tsc emits TS2883
        // with the resolved relative path; tsz must do the same.
        if let Some(reference) =
            self.symlinked_nested_package_reference(&source_path, &type_name, current_file_path)
        {
            return Some(reference);
        }

        // Parse node_modules segments from the source path
        let components: Vec<_> = Path::new(&source_path).components().collect();
        let nm_positions: Vec<usize> = components
            .iter()
            .enumerate()
            .filter_map(|(i, c)| match c {
                Component::Normal(part) if part.to_str() == Some("node_modules") => Some(i),
                _ => None,
            })
            .collect();

        // Case 1: Symbol is an import alias from a package in node_modules,
        // and the import specifier is a bare package name (not relative).
        // This means it's importing from a transitive dependency.
        //
        // Example: foo/index.d.ts has `import { NestedProps } from "nested"`
        // where foo is in node_modules and nested is in foo/node_modules/nested.
        // The "from" path is "foo/node_modules/nested".
        if nm_positions.len() >= 2
            && symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS)
            && let Some(import_module) = &symbol.import_module
            && !import_module.starts_with('.')
            && !import_module.starts_with('/')
        {
            // The symbol is an import alias that imports from a bare module specifier.
            // Its source file is in a node_modules package. This means it's importing
            // from a transitive dependency.

            // Get the parent package name from the source path
            let last_nm = *nm_positions.last().unwrap();
            let pkg_start = last_nm + 1;
            let pkg_len = if components.get(pkg_start).is_some_and(|c| {
                matches!(c, Component::Normal(p) if p.to_str().is_some_and(|s| s.starts_with('@')))
            }) {
                2
            } else {
                1
            };

            // Before reporting as non-portable, check the parent package's
            // package.json. If it has no "exports" field, all subpaths are
            // accessible and the reference is portable (common for symlinked
            // workspace dependencies).
            let parent_pkg_root: std::path::PathBuf =
                components[..pkg_start + pkg_len].iter().collect();
            let parent_pkg_json = parent_pkg_root.join("package.json");
            if let Ok(pkg_content) = std::fs::read_to_string(&parent_pkg_json)
                && let Ok(pkg_json) = serde_json::from_str::<serde_json::Value>(&pkg_content)
                && pkg_json.get("exports").is_none()
            {
                return None;
            }

            let parent_package: Vec<String> = components[pkg_start..pkg_start + pkg_len]
                .iter()
                .filter_map(|c| match c {
                    Component::Normal(part) => part.to_str().map(str::to_string),
                    _ => None,
                })
                .collect();

            if !parent_package.is_empty() {
                let from_path = format!(
                    "{}/node_modules/{}",
                    parent_package.join("/"),
                    import_module
                );
                return Some((from_path, type_name));
            }
        }

        // Case 2: Source path has nested node_modules
        // (the resolved original symbol lives in a deeply nested path)
        if nm_positions.len() >= 2 {
            let first_nm = nm_positions[0];
            let second_nm = nm_positions[1];

            // Before flagging as non-portable, check whether the nested
            // package has no "exports" field.  Without an "exports" map
            // every subpath is accessible via standard Node.js resolution,
            // even when the package root is a symlink (workspace deps
            // hoisted by a package manager).  This matches tsc behaviour
            // which does not emit TS2883 for workspace symlinks that lack
            // an exports restriction.
            let nested_start = second_nm + 1;
            let nested_len = if components.get(nested_start).is_some_and(|c| {
                matches!(c, Component::Normal(p) if p.to_str().is_some_and(|s| s.starts_with('@')))
            }) {
                2
            } else {
                1
            };

            let nested_pkg_root: std::path::PathBuf =
                components[..nested_start + nested_len].iter().collect();
            let nested_pkg_json = nested_pkg_root.join("package.json");
            if let Ok(pkg_content) = std::fs::read_to_string(&nested_pkg_json)
                && let Ok(pkg_json) = serde_json::from_str::<serde_json::Value>(&pkg_content)
                && pkg_json.get("exports").is_none()
            {
                return None;
            }

            let parent_parts: Vec<String> = components[first_nm + 1..second_nm]
                .iter()
                .filter_map(|c| match c {
                    Component::Normal(part) => part.to_str().map(str::to_string),
                    _ => None,
                })
                .collect();

            let nested_parts: Vec<String> = components[nested_start..nested_start + nested_len]
                .iter()
                .filter_map(|c| match c {
                    Component::Normal(part) => part.to_str().map(str::to_string),
                    _ => None,
                })
                .collect();

            if !parent_parts.is_empty() && !nested_parts.is_empty() {
                let from_path = format!(
                    "{}/node_modules/{}",
                    parent_parts.join("/"),
                    nested_parts.join("/")
                );
                return Some((from_path, type_name));
            }
        }

        // Case 3: Source is in node_modules and the subpath isn't in the
        // package's exports map (private module)
        if nm_positions.len() == 1 {
            let nm_idx = nm_positions[0];
            let pkg_start = nm_idx + 1;
            let pkg_len = if components.get(pkg_start).is_some_and(|c| {
                matches!(c, Component::Normal(p) if p.to_str().is_some_and(|s| s.starts_with('@')))
            }) {
                2
            } else {
                1
            };

            let subpath_start = pkg_start + pkg_len;
            if subpath_start < components.len() {
                let package_root = Path::new(&source_path)
                    .components()
                    .take(nm_idx + 1 + pkg_len)
                    .collect::<std::path::PathBuf>();

                let subpath_parts: Vec<String> = components[subpath_start..]
                    .iter()
                    .filter_map(|c| match c {
                        Component::Normal(part) => part.to_str().map(str::to_string),
                        _ => None,
                    })
                    .collect();

                let relative_path = subpath_parts.join("/");
                if let Some(runtime_path) = self.declaration_runtime_relative_path(&relative_path)
                    && self
                        .reverse_export_specifier_for_runtime_path(&package_root, &runtime_path)
                        .is_none()
                {
                    let pkg_json_path = package_root.join("package.json");
                    if let Ok(pkg_content) = std::fs::read_to_string(&pkg_json_path)
                        && let Ok(pkg_json) =
                            serde_json::from_str::<serde_json::Value>(&pkg_content)
                    {
                        // No `exports` field: all subpaths are directly importable
                        // (pre-exports Node.js behaviour). No portability concern.
                        pkg_json.get("exports")?;

                        // Before flagging as non-portable, check whether the
                        // symbol is re-exported from a module that IS accessible
                        // through the package's exports map.  If so, the type
                        // can be referenced via the public API and TS2883
                        // should not fire.
                        if self.symbol_is_reexported_from_public_module(
                            sym_id,
                            &type_name,
                            binder,
                            &package_root,
                        ) {
                            return None;
                        }

                        // Also check whether ANY accessible module in this
                        // package re-exports from the same source file.
                        if self.source_file_is_reexported_from_public_module(
                            &source_path,
                            binder,
                            &package_root,
                        ) {
                            return None;
                        }

                        // Check if the subpath falls inside a directory that is
                        // mapped by a `typesVersions` entry in package.json.
                        // e.g. `"typesVersions": {">=3.1.0-0": {"*": ["ts3.1/*"]}}`
                        // means `ts3.1/index.d.ts` is accessible as the package root.
                        if self.subpath_is_in_types_versions_dir(&package_root, &relative_path) {
                            return None;
                        }

                        let package_specifier = self
                            .package_specifier_for_node_modules_path(
                                current_file_path,
                                &source_path,
                            )
                            .unwrap_or_else(|| source_path.clone());
                        if let Some(module_path) = self
                            .matching_module_export_paths(
                                binder,
                                current_file_path,
                                &package_specifier,
                            )
                            .into_iter()
                            .max_by_key(|path| path.len())
                        {
                            let mut from_path = self.strip_ts_extensions(
                                &self.calculate_relative_path(current_file_path, module_path),
                            );
                            if from_path.ends_with("/index") {
                                from_path.truncate(from_path.len() - "/index".len());
                            }
                            return Some((from_path, type_name));
                        }

                        let source_path_for_diag = source_path.clone();
                        let mut from_path = self.strip_ts_extensions(
                            &self.calculate_relative_path(current_file_path, &source_path_for_diag),
                        );
                        if from_path.ends_with("/index") {
                            from_path.truncate(from_path.len() - "/index".len());
                        }
                        return Some((from_path, type_name));
                    }
                }
            }
        }

        if let Some(cache) = &self.type_cache
            && let Some(&symbol_type_id) = cache.symbol_types.get(&sym_id)
            && let Some(result) = self.find_non_portable_type_reference_inner(
                symbol_type_id,
                visited_types,
                visited_symbols,
                visited_declaration_symbols,
                visited_nodes,
            )
        {
            return Some(result);
        }

        None
    }

    /// Detect the "symlinked monorepo / nested-package" portability case.
    ///
    /// When a type's source path is `<X>/node_modules/<P>/<sub>` and `<X>` is
    /// NOT an ancestor of the consumer file's directory, the type was reached
    /// through a symlinked or otherwise nested `node_modules` chain that is
    /// outside the consumer's normal resolution scope (the standard "walk up
    /// looking for `node_modules`" Node.js algorithm starting at the consumer
    /// would not land on this file). Writing `<P>` as a bare specifier from
    /// the consumer would therefore fail at runtime, so tsc emits TS2883 with
    /// the resolved relative path. This helper returns that diagnostic data.
    ///
    /// Restricted to source paths with exactly one `node_modules` segment so
    /// it does not double-fire alongside the existing nested-`node_modules`
    /// rules in `check_symbol_portability` (Cases 1 and 2 there cover the
    /// `>= 2` segment case).
    pub(in crate::declaration_emitter) fn symlinked_nested_package_reference(
        &self,
        source_path: &str,
        type_name: &str,
        current_file_path: &str,
    ) -> Option<(String, String)> {
        use std::path::{Component, Path};

        let path = Path::new(source_path);
        let nm_indices: Vec<usize> = path
            .components()
            .enumerate()
            .filter_map(|(idx, component)| match component {
                Component::Normal(part) if part.to_str() == Some("node_modules") => Some(idx),
                _ => None,
            })
            .collect();

        if nm_indices.len() != 1 {
            return None;
        }

        let nm_idx = nm_indices[0];
        let nm_parent: std::path::PathBuf = path.components().take(nm_idx).collect();
        let consumer_dir = Path::new(current_file_path).parent()?;

        if consumer_dir.starts_with(&nm_parent) {
            return None;
        }

        let mut from_path =
            self.strip_ts_extensions(&self.calculate_relative_path(current_file_path, source_path));
        if from_path.ends_with("/index") {
            from_path.truncate(from_path.len() - "/index".len());
        }

        Some((from_path, type_name.to_string()))
    }

    pub(in crate::declaration_emitter) fn transitive_import_module_reference_path(
        &self,
        import_module: &str,
        binder: &BinderState,
        current_file_path: &str,
    ) -> Option<String> {
        if let Some(module_path) = self
            .matching_module_export_paths(binder, current_file_path, import_module)
            .into_iter()
            .max_by_key(|path| path.len())
        {
            let mut from_path = self
                .strip_ts_extensions(&self.calculate_relative_path(current_file_path, module_path));
            if from_path.ends_with("/index") {
                from_path.truncate(from_path.len() - "/index".len());
            }
            from_path = Self::ts2883_relative_node_modules_path(from_path);
            return Some(from_path);
        }

        let mut package_roots: Vec<_> = binder
            .module_exports
            .keys()
            .filter_map(|module_path| {
                self.node_modules_package_root_path(module_path, import_module)
            })
            .collect();
        package_roots.sort();
        package_roots.dedup();

        // Prefer the deepest matching package root so symlinked package trees
        // keep their full path instead of collapsing to the top-level package.
        let package_root = package_roots.into_iter().max_by_key(|root| root.len())?;
        let mut from_path = self
            .strip_ts_extensions(&self.calculate_relative_path(current_file_path, &package_root));
        if from_path.ends_with("/index") {
            from_path.truncate(from_path.len() - "/index".len());
        }
        Some(Self::ts2883_relative_node_modules_path(from_path))
    }

    pub(in crate::declaration_emitter) fn node_modules_package_root_path(
        &self,
        module_path: &str,
        import_module: &str,
    ) -> Option<String> {
        use std::path::{Component, Path, PathBuf};

        let components: Vec<_> = Path::new(module_path).components().collect();
        let nm_idx = components
            .iter()
            .position(|component| {
                matches!(component, Component::Normal(part) if part.to_str() == Some("node_modules"))
            })?;
        let pkg_start = nm_idx + 1;
        let pkg_len = if import_module.starts_with('@') { 2 } else { 1 };
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
        if package_name != import_module {
            return None;
        }

        Some(
            components[..pkg_start + pkg_len]
                .iter()
                .fold(PathBuf::new(), |mut path, component| {
                    path.push(component.as_os_str());
                    path
                })
                .to_string_lossy()
                .into_owned(),
        )
    }

    pub(in crate::declaration_emitter) fn ts2883_relative_node_modules_path(
        path: String,
    ) -> String {
        if path.starts_with("../") {
            if let Some(path) = path.strip_suffix("../node_modules") {
                return format!("{path}..node_modules");
            }
            if path.contains("../node_modules/") {
                return path.replacen("../node_modules/", "..node_modules/", 1);
            }
        }
        path
    }

    /// Check whether the symbol is re-exported from a module within the same
    /// package whose runtime path IS accessible through the package's exports
    /// map.  Returns `true` when the type can be reached through the public
    /// API, meaning TS2883 should be suppressed.
    pub(in crate::declaration_emitter) fn symbol_is_reexported_from_public_module(
        &self,
        sym_id: SymbolId,
        type_name: &str,
        binder: &BinderState,
        package_root: &std::path::Path,
    ) -> bool {
        let package_root_str = package_root.to_string_lossy();

        for (module_path, exports) in binder.module_exports.iter() {
            // Only consider modules inside the same package.
            if !module_path.starts_with(package_root_str.as_ref()) {
                continue;
            }
            // Check if this module exports the symbol under the same name.
            let Some(exported_sym_id) = exports.get(type_name) else {
                continue;
            };
            let resolved = self
                .resolve_portability_import_alias(exported_sym_id, binder)
                .unwrap_or_else(|| self.resolve_portability_symbol(exported_sym_id, binder));
            if resolved != sym_id {
                continue;
            }
            // The module re-exports the same symbol.  Check whether that
            // module's own path is accessible through the exports map.
            let module_relative = module_path.strip_prefix(package_root_str.as_ref());
            let module_relative = module_relative.map(|p| p.trim_start_matches('/'));
            if let Some(rel) = module_relative
                && !rel.is_empty()
            {
                if let Some(runtime) = self.declaration_runtime_relative_path(rel)
                    && self
                        .reverse_export_specifier_for_runtime_path(package_root, &runtime)
                        .is_some()
                {
                    return true;
                }
            } else {
                // Module IS the package root (index file).
                return true;
            }
        }

        false
    }

    /// Check whether ANY accessible module in the package re-exports from
    /// the source file.  When a public entry point does
    /// `export { x } from "./other.js"`, types from `other.d.ts` are
    /// indirectly reachable and TS2883 should be suppressed.
    pub(in crate::declaration_emitter) fn source_file_is_reexported_from_public_module(
        &self,
        source_path: &str,
        binder: &BinderState,
        package_root: &std::path::Path,
    ) -> bool {
        use std::path::Path;

        let package_root_str = package_root.to_string_lossy();

        let source_relative = source_path
            .strip_prefix(package_root_str.as_ref())
            .map(|p| p.trim_start_matches('/'));
        let Some(source_relative) = source_relative else {
            return false;
        };
        let source_relative_stripped = self.strip_ts_extensions(source_relative);

        for (module_path, exports) in binder.module_exports.iter() {
            if module_path == source_path || !module_path.starts_with(package_root_str.as_ref()) {
                continue;
            }
            let module_relative = module_path.strip_prefix(package_root_str.as_ref());
            let module_relative = module_relative.map(|p| p.trim_start_matches('/'));
            let is_accessible = if let Some(rel) = module_relative
                && !rel.is_empty()
            {
                self.declaration_runtime_relative_path(rel)
                    .and_then(|runtime| {
                        self.reverse_export_specifier_for_runtime_path(package_root, &runtime)
                    })
                    .is_some()
                    || self
                        .reverse_export_specifier_for_runtime_path(package_root, rel)
                        .is_some()
            } else {
                true
            };
            if !is_accessible {
                continue;
            }

            let module_rel_dir = module_relative
                .and_then(|r| Path::new(r).parent())
                .unwrap_or_else(|| Path::new(""));

            for (_, &exported_sym_id) in exports.iter() {
                if let Some(symbol) = binder.symbols.get(exported_sym_id)
                    && symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS)
                    && let Some(import_module) = &symbol.import_module
                    && import_module.starts_with('.')
                {
                    // Normalize the joined path to remove `.` components
                    // introduced by joining a dir with a `./foo.js` specifier.
                    let resolved: std::path::PathBuf = module_rel_dir
                        .join(import_module)
                        .components()
                        .filter(|c| !matches!(c, std::path::Component::CurDir))
                        .collect();
                    let resolved_str = resolved.to_string_lossy();
                    let resolved_stripped = self.strip_ts_extensions(&resolved_str);
                    let resolved_stripped = resolved_stripped
                        .strip_prefix("./")
                        .unwrap_or(&resolved_stripped);
                    let source_cmp = source_relative_stripped
                        .strip_prefix("./")
                        .unwrap_or(&source_relative_stripped);
                    if resolved_stripped == source_cmp {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Returns `true` when `relative_path` (relative to the package root) falls
    /// inside a directory that is targeted by a `typesVersions` mapping in the
    /// package's `package.json`.  Types inside such directories are accessible
    /// via the package specifier for compatible TypeScript versions, so TS2883
    /// must not fire for them.
    fn subpath_is_in_types_versions_dir(
        &self,
        package_root: &std::path::Path,
        relative_path: &str,
    ) -> bool {
        let pkg_json_path = package_root.join("package.json");
        let Ok(pkg_content) = std::fs::read_to_string(&pkg_json_path) else {
            return false;
        };
        let Ok(pkg_json) = serde_json::from_str::<serde_json::Value>(&pkg_content) else {
            return false;
        };
        let Some(types_versions) = pkg_json.get("typesVersions") else {
            return false;
        };
        let Some(version_map) = types_versions.as_object() else {
            return false;
        };
        for (_version, mappings) in version_map {
            let Some(mappings) = mappings.as_object() else {
                continue;
            };
            for (_pattern, targets) in mappings {
                let Some(targets) = targets.as_array() else {
                    continue;
                };
                for target in targets {
                    let Some(target_str) = target.as_str() else {
                        continue;
                    };
                    // Strip trailing "/*" or "*" to get the directory prefix.
                    let dir_prefix = target_str.trim_end_matches('*').trim_end_matches('/');
                    if dir_prefix.is_empty() {
                        continue;
                    }
                    if relative_path == dir_prefix
                        || relative_path.starts_with(&format!("{dir_prefix}/"))
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub(in crate::declaration_emitter) fn package_root_export_reference_path(
        &self,
        sym_id: SymbolId,
        type_name: &str,
        binder: &BinderState,
        current_file_path: &str,
    ) -> Option<String> {
        let source_path = self.get_symbol_source_path(sym_id, binder)?;

        binder
            .module_exports
            .iter()
            .find_map(|(module_path, exports)| {
                let exported_raw = exports.get(type_name)?;
                // Resolve alias using the alias's OWN source file as the base,
                // so relative imports like `./useQuery-CPqkvEsh.js` in
                // `index.d.ts` resolve correctly rather than relative to the
                // user's current file.
                let exported = self
                    .resolve_alias_in_source_context(exported_raw, binder)
                    .unwrap_or_else(|| self.resolve_portability_symbol(exported_raw, binder));
                if module_path == &source_path || exported != sym_id {
                    return None;
                }

                let specifier =
                    self.package_specifier_for_node_modules_path(current_file_path, module_path)?;
                // Allow bare package root specifiers only (no subpath segments).
                // Unscoped packages have no slash ("react", "lodash").
                // Scoped packages have exactly one slash ("@tanstack/vue-query", "@types/node").
                // Subpath imports have extra slashes ("lodash/fp", "@scope/pkg/sub").
                let slash_count = specifier.chars().filter(|&c| c == '/').count();
                let is_root_specifier = if specifier.starts_with('@') {
                    slash_count == 1
                } else {
                    slash_count == 0
                };
                if !is_root_specifier {
                    return None;
                }

                let mut from_path = self.strip_ts_extensions(
                    &self.calculate_relative_path(current_file_path, module_path),
                );
                if from_path.ends_with("/index") {
                    from_path.truncate(from_path.len() - "/index".len());
                }
                Some(from_path)
            })
    }

    pub(in crate::declaration_emitter) fn resolve_portability_symbol(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
    ) -> SymbolId {
        let mut current = sym_id;
        let mut seen = rustc_hash::FxHashSet::default();

        while seen.insert(current) {
            let Some(symbol) = binder.symbols.get(current) else {
                break;
            };
            if !symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS) {
                break;
            }

            let Some(next) = binder
                .resolve_import_symbol(current)
                .filter(|resolved| *resolved != current)
                .or_else(|| self.resolve_import_symbol_from_module_exports(current, binder))
            else {
                break;
            };
            current = next;
        }

        current
    }

    pub(in crate::declaration_emitter) fn resolve_portability_declaration_symbol(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
    ) -> SymbolId {
        let mut resolved = self.resolve_portability_symbol(sym_id, binder);
        if let Some(import_resolved) = self
            .resolve_portability_import_alias(resolved, binder)
            .or_else(|| self.resolve_portability_import_alias(sym_id, binder))
        {
            resolved = import_resolved;
        }
        let Some(symbol) = binder.symbols.get(resolved) else {
            return resolved;
        };
        let Some(current_file_path) = self.current_file_path.as_deref() else {
            return resolved;
        };
        let Some(source_path) = self.get_symbol_source_path(resolved, binder) else {
            return resolved;
        };
        let Some(package_specifier) =
            self.package_specifier_for_node_modules_path(current_file_path, &source_path)
        else {
            return resolved;
        };
        let package_root_specifier = Self::bare_package_specifier(&package_specifier);
        let is_explicitly_exported = self
            .package_root_export_reference_path(
                resolved,
                symbol.escaped_name.as_str(),
                binder,
                current_file_path,
            )
            .is_some();

        if self.symbol_has_portability_declaration(resolved, binder)
            && is_explicitly_exported
            && self
                .collect_non_portable_references_in_symbol_declaration(resolved)
                .is_empty()
        {
            return resolved;
        }

        let mut candidates: Vec<_> = binder
            .module_exports
            .iter()
            .filter_map(|(module_path, exports)| {
                let candidate_specifier =
                    self.package_specifier_for_node_modules_path(current_file_path, module_path)?;
                if Self::bare_package_specifier(&candidate_specifier) != package_root_specifier {
                    return None;
                }
                let export = exports.get(symbol.escaped_name.as_str())?;
                let candidate = self.resolve_portability_symbol(export, binder);
                (candidate != resolved
                    && self.symbol_has_portability_declaration(candidate, binder)
                    && self
                        .package_root_export_reference_path(
                            candidate,
                            symbol.escaped_name.as_str(),
                            binder,
                            current_file_path,
                        )
                        .is_some())
                .then_some(candidate)
            })
            .collect();

        candidates.sort_by(|left, right| {
            let left_path = self.get_symbol_source_path(*left, binder);
            let right_path = self.get_symbol_source_path(*right, binder);
            right_path
                .as_deref()
                .cmp(&left_path.as_deref())
                .then_with(|| right.0.cmp(&left.0))
        });
        candidates.dedup();
        candidates.into_iter().next().unwrap_or(resolved)
    }

    pub(in crate::declaration_emitter) fn bare_package_specifier(specifier: &str) -> &str {
        if let Some(rest) = specifier.strip_prefix('@') {
            let Some((scope_and_name, _)) = rest.split_once('/') else {
                return specifier;
            };
            let consumed = 1 + scope_and_name.len();
            let remaining = &specifier[consumed..];
            if let Some((package_name, _)) = remaining[1..].split_once('/') {
                return &specifier[..consumed + 1 + package_name.len()];
            }
            return specifier;
        }

        specifier
            .split_once('/')
            .map_or(specifier, |(root, _)| root)
    }

    pub(in crate::declaration_emitter) fn resolve_import_symbol_from_module_exports(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
    ) -> Option<SymbolId> {
        let symbol = binder.symbols.get(sym_id)?;
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
            if let Some(resolved) = exports.get(export_name) {
                return Some(resolved);
            }
        }

        None
    }

    /// Resolve an alias symbol to its target, using the alias's OWN source file
    /// as the base for relative `import_module` resolution.
    ///
    /// The standard `resolve_import_symbol_from_module_exports` uses
    /// `self.current_file_path`, which is wrong when the alias lives in a
    /// different file (e.g., `index.d.ts` re-exporting from
    /// `./useQuery-CPqkvEsh.js`).
    pub(in crate::declaration_emitter) fn resolve_alias_in_source_context(
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

        // Use the alias symbol's own source file as the resolution base.
        let source_path = self.get_symbol_source_path(sym_id, binder)?;

        // `matching_module_export_paths` compares the stripped module path
        // against the raw specifier.  ESM `.d.ts` files use `.js` extensions
        // in re-exports (`from './foo.js'`), so we normalise both sides by
        // stripping the specifier's extension too.
        let specifier_normalized = self.strip_ts_extensions(module_specifier);
        let effective_specifier = if specifier_normalized != module_specifier {
            specifier_normalized.as_str()
        } else {
            module_specifier
        };

        for module_path in
            self.matching_module_export_paths(binder, &source_path, effective_specifier)
        {
            let Some(exports) = binder.module_exports.get(module_path) else {
                continue;
            };
            if let Some(resolved) = exports.get(export_name) {
                if resolved != sym_id {
                    return Some(resolved);
                }
            }
        }
        None
    }

    pub(in crate::declaration_emitter) fn symbol_has_portability_declaration(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
    ) -> bool {
        let Some(symbol) = binder.symbols.get(sym_id) else {
            return false;
        };
        let Some(source_arena) = binder.symbol_arenas.get(&sym_id) else {
            return false;
        };

        symbol.declarations.iter().copied().any(|decl_idx| {
            let Some(decl_node) = source_arena.get(decl_idx) else {
                return false;
            };
            source_arena.get_type_alias(decl_node).is_some()
                || source_arena.get_function(decl_node).is_some()
                || source_arena.get_interface(decl_node).is_some()
                || source_arena.get_signature(decl_node).is_some()
                || source_arena.get_function_type(decl_node).is_some()
                || source_arena.get_variable_declaration(decl_node).is_some()
                || source_arena.get_property_decl(decl_node).is_some()
                || source_arena.get_parameter(decl_node).is_some()
        })
    }

    /// Check if an ALIAS symbol's `import_module` resolves to a NESTED
    /// (transitive) sub-node_modules package relative to the alias's own source
    /// package.  Returns `Some(from_path)` if nested, `None` otherwise.
    ///
    /// This handles the case where the standard portability check fails because
    /// the consumer file cannot see the bare specifier (e.g., `"nested"` from
    /// `entry.ts` is invisible, but from `foo/index.d.ts` it resolves to
    /// `foo/node_modules/nested`).
    pub(in crate::declaration_emitter) fn check_nested_transitive_import(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
    ) -> Option<String> {
        use std::path::{Component, Path};

        let symbol = binder.symbols.get(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS) {
            return None;
        }
        let import_module = symbol.import_module.as_deref()?;
        // Only bare specifiers can point to nested node_modules.
        if import_module.is_empty()
            || import_module.starts_with('.')
            || import_module.starts_with('/')
        {
            return None;
        }

        // Get the source file of the alias (e.g., `r/node_modules/foo/index.d.ts`).
        let source_path = self.get_symbol_source_path(sym_id, binder)?;

        // Find the innermost `node_modules` segment in the alias's source path.
        let components: Vec<_> = Path::new(&source_path).components().collect();
        let last_nm = components.iter().rposition(
            |c| matches!(c, Component::Normal(p) if p.to_str() == Some("node_modules")),
        )?;
        let pkg_start = last_nm + 1;
        let pkg_len = if components.get(pkg_start).is_some_and(
            |c| matches!(c, Component::Normal(p) if p.to_str().is_some_and(|s| s.starts_with('@'))),
        ) {
            2
        } else {
            1
        };
        if pkg_start + pkg_len > components.len() {
            return None;
        }

        // Build the sub-node_modules prefix: e.g., `r/node_modules/foo/node_modules/`.
        let pkg_root: std::path::PathBuf = components[..pkg_start + pkg_len].iter().collect();
        let sub_nm = pkg_root.join("node_modules");
        let sub_nm_str = format!("{}/", sub_nm.to_string_lossy());

        // Check whether any entry in module_exports lives under this sub-node_modules
        // and matches the bare specifier `import_module`.
        let found = binder.module_exports.keys().any(|path| {
            let Some(rest) = path.strip_prefix(sub_nm_str.as_str()) else {
                return false;
            };
            if import_module.starts_with('@') {
                let mut parts = rest.splitn(3, '/');
                let scope = parts.next().unwrap_or("");
                let name = parts.next().unwrap_or("");
                format!("{scope}/{name}") == import_module
            } else {
                rest.split('/').next().unwrap_or("") == import_module
            }
        });

        if !found {
            return None;
        }

        // Build the `from_path` for the diagnostic: `{pkg_name}/node_modules/{import_module}`.
        let pkg_parts: Vec<&str> = components[last_nm + 1..pkg_start + pkg_len]
            .iter()
            .filter_map(|c| match c {
                Component::Normal(p) => p.to_str(),
                _ => None,
            })
            .collect();
        Some(format!(
            "{}/node_modules/{import_module}",
            pkg_parts.join("/")
        ))
    }

    /// Get the source file path for a symbol via the binder's `symbol_arenas` and `arena_to_path`.
    ///
    /// Falls back to `global_symbol_arenas` for cross-file symbols whose arenas
    /// are not in the current file's binder (e.g., imported types from `node_modules`).
    pub(in crate::declaration_emitter) fn get_symbol_source_path(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
    ) -> Option<String> {
        // Primary path: look up the symbol's arena in the pre-built mapping.
        if let Some(source_arena) = binder.symbol_arenas.get(&sym_id) {
            let arena_addr = Arc::as_ptr(source_arena) as usize;
            if let Some(path) = self.arena_to_path.get(&arena_addr) {
                return Some(path.clone());
            }
        }

        // Fall back to global symbol arenas for cross-file symbols
        if let Some(source_arena) = self.global_symbol_arenas.get(&sym_id) {
            let arena_addr = Arc::as_ptr(source_arena) as usize;
            if let Some(path) = self.arena_to_path.get(&arena_addr) {
                return Some(path.clone());
            }
        }

        // Fallback: the checker may create symbols (e.g., for resolved imports)
        // whose IDs are not in the merge-phase symbol_arenas map.  Walk the
        // symbol's declarations and check each declaration's arena against the
        // program's arena-to-path mapping.
        let symbol = binder.symbols.get(sym_id)?;
        for &decl_idx in &symbol.declarations {
            if let Some(decl_arenas) = binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                for arena in decl_arenas {
                    let arena_addr = Arc::as_ptr(arena) as usize;
                    if let Some(path) = self.arena_to_path.get(&arena_addr) {
                        return Some(path.clone());
                    }
                }
            }
        }

        // Last resort: use the symbol's decl_file_idx which was set during
        // the multi-file merge phase.  This covers interface/type symbols from
        // foreign files that lack both symbol_arenas and declaration_arenas entries.
        if symbol.decl_file_idx != u32::MAX
            && let Some(path) = self.file_idx_to_path.get(&symbol.decl_file_idx)
        {
            return Some(path.clone());
        }

        None
    }
}
