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

use super::portability_check::{PortabilityCollectState, PortabilityVisitState};

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn next_import_type_text(
        text: &str,
    ) -> Option<(usize, String, &str)> {
        let mut search_start = 0usize;
        while search_start < text.len() {
            let start = text[search_start..].find("import(")? + search_start;
            let mut i = start + "import(".len();
            let bytes = text.as_bytes();
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }

            let quote = *bytes.get(i)?;
            if quote != b'"' && quote != b'\'' {
                search_start = start + "import(".len();
                continue;
            }
            i += 1;

            let specifier_start = i;
            let mut escaped = false;
            while i < bytes.len() {
                if escaped {
                    escaped = false;
                    i += 1;
                    continue;
                }
                if bytes[i] == b'\\' {
                    escaped = true;
                    i += 1;
                    continue;
                }
                if bytes[i] == quote {
                    let module_specifier = text[specifier_start..i].to_string();
                    i += 1;
                    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    if bytes.get(i) == Some(&b')') {
                        return Some((start, module_specifier, &text[i + 1..]));
                    }
                    break;
                }
                i += 1;
            }

            search_start = start + "import(".len();
        }

        None
    }

    pub(in crate::declaration_emitter) fn type_text_starts_with_import_type(text: &str) -> bool {
        Self::next_import_type_text(text).is_some_and(|(start, _, _)| start == 0)
    }

    pub(in crate::declaration_emitter) fn type_text_contains_import_type(text: &str) -> bool {
        Self::next_import_type_text(text).is_some()
    }

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
            let after_import_prefix = if let Some((start, _, tail)) =
                Self::next_import_type_text(rest)
                && start == 0
            {
                tail.strip_prefix('.').unwrap_or(rest)
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

        while let Some((_, module_specifier, after_import)) = Self::next_import_type_text(remaining)
        {
            let Some(tail) = after_import.strip_prefix('.') else {
                remaining = after_import;
                continue;
            };
            let Some(first_name) = tail
                .split([
                    '.', '<', '[', ']', ' ', '&', '|', '>', ',', ')', ';', ':', '?', '{', '}',
                    '\n', '\r',
                ])
                .find(|part| !part.is_empty())
            else {
                remaining = tail;
                continue;
            };

            let exports = binder
                .module_exports
                .iter()
                .find_map(|(module_path, exports)| {
                    let candidate =
                        if module_specifier.starts_with('.') || module_specifier.starts_with('/') {
                            Some(self.strip_module_path_extension(
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

            remaining = tail;
        }

        None
    }

    pub(in crate::declaration_emitter) fn printed_type_uses_non_emittable_local_alias_root(
        &self,
        printed_type_text: &str,
    ) -> bool {
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
            if ch == '"' || ch == '\'' || ch == '`' {
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

        // A function-local alias is never module-visible (tsc expands it).
        if self.name_is_function_local_type_alias(ident) {
            return true;
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

    pub(in crate::declaration_emitter) const fn is_type_text_identifier_start(ch: char) -> bool {
        ch.is_ascii_alphabetic() || ch == '_' || ch == '$'
    }

    pub(in crate::declaration_emitter) const fn is_type_text_identifier_continue(ch: char) -> bool {
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

        let mut v_types = rustc_hash::FxHashSet::default();
        let mut v_symbols = rustc_hash::FxHashSet::default();
        let mut v_decl_syms = rustc_hash::FxHashSet::default();
        let mut v_nodes = rustc_hash::FxHashSet::default();
        let mut visit = PortabilityVisitState {
            visited_types: &mut v_types,
            visited_symbols: &mut v_symbols,
            visited_declaration_symbols: &mut v_decl_syms,
            visited_nodes: &mut v_nodes,
        };
        let mut seen = rustc_hash::FxHashSet::default();
        let mut references: Vec<(String, String)> = Vec::new();
        let mut collect = PortabilityCollectState {
            results: &mut references,
            seen: &mut seen,
        };
        self.collect_non_portable_references_in_type_node(
            arena,
            node_idx,
            &source_path,
            &mut collect,
            &mut visit,
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
        let current_path = self.current_file_path.as_deref()?;
        self.find_symbol_for_import_type_text_from_path(printed, current_path)
    }

    pub(in crate::declaration_emitter) fn find_symbol_for_import_type_text_from_path(
        &self,
        printed: &str,
        current_path: &str,
    ) -> Option<SymbolId> {
        let (module_specifier, first_name) = self.parse_import_type_text(printed)?;
        let binder = self.binder?;

        for module_path in
            self.matching_module_export_paths(binder, current_path, &module_specifier)
        {
            let Some(exports) = binder.module_exports.get(module_path) else {
                continue;
            };
            let Some(exported) = exports.get(first_name.as_str()) else {
                continue;
            };
            return Some(self.resolve_portability_symbol(exported, binder));
        }

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
                        self.strip_module_path_extension(
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
        let (module_specifier, first_name) = self.parse_import_type_text_at(printed, 0)?;
        Some((module_specifier, first_name))
    }

    pub(in crate::declaration_emitter) fn parse_import_type_text_at(
        &self,
        printed: &str,
        expected_start: usize,
    ) -> Option<(String, String)> {
        let (start, module_specifier, tail) = Self::next_import_type_text(printed)?;
        if start != expected_start {
            return None;
        }
        let tail = tail.strip_prefix('.')?;
        let first_name = tail
            .split([
                '.', '<', '[', ']', ' ', '&', '|', '>', ',', ')', ';', ':', '?', '{', '}', '\n',
                '\r',
            ])
            .find(|part| !part.is_empty())?;
        Some((module_specifier, first_name.to_string()))
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
        while let Some((_, module_specifier, tail)) = Self::next_import_type_text(remaining) {
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
                        Some(self.strip_module_path_extension(
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
        } else {
            let qn = arena.get_qualified_name(node)?;
            (qn.left, qn.right)
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
            binder
                .symbol_arenas
                .get(&symbol.id)
                .or_else(|| self.global_symbol_arenas.get(&symbol.id))
                .and_then(|sym_arena| {
                    ((Arc::as_ptr(sym_arena) as usize) == arena_addr).then_some(symbol.id)
                })
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
                let mut from_path = self.strip_module_path_extension(
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

    pub(in crate::declaration_emitter) fn named_import_module_from_text(
        &self,
        source_text: &str,
        local_name: &str,
    ) -> Option<String> {
        for line in source_text.lines() {
            let trimmed = line.trim();
            let Some(rest) = trimmed.strip_prefix("import ") else {
                continue;
            };
            let Some(named_start) = rest.find('{') else {
                continue;
            };
            let Some(named_end) = rest[named_start + 1..].find('}') else {
                continue;
            };
            let named = &rest[named_start + 1..named_start + 1 + named_end];
            let after_named = &rest[named_start + 1 + named_end + 1..];
            let Some((_, module_part)) = after_named.split_once(" from ") else {
                continue;
            };
            let module = module_part.trim().trim_end_matches(';');
            for specifier in named.split(',') {
                let specifier = specifier.trim();
                let local = specifier
                    .split_once(" as ")
                    .map_or(specifier, |(_, alias)| alias.trim());
                if local == local_name {
                    return Self::quoted_string_text(module);
                }
            }
        }
        None
    }

    pub(in crate::declaration_emitter) fn rewrite_leading_import_type_with_public_named_import(
        source_text: &str,
        type_text: &str,
    ) -> Option<(String, String, String, Option<String>)> {
        let (start, private_module, tail) = Self::next_import_type_text(type_text)?;
        if start != 0 {
            return None;
        }
        let tail = tail.strip_prefix('.')?;
        let imported_name = Self::leading_import_type_member_name(tail)?;
        let (public_module, public_imported_name, local_alias) =
            Self::public_named_import_for_exported_name(source_text, imported_name)?;
        if public_module == private_module {
            return None;
        }

        let local_name = local_alias
            .as_deref()
            .unwrap_or(public_imported_name.as_str());
        let import_type_end = type_text.len() - (tail.len() + 1);
        let member_start = import_type_end + 1;
        let member_end = member_start + imported_name.len();
        let mut rewritten = String::with_capacity(type_text.len());
        rewritten.push_str(&type_text[..start]);
        rewritten.push_str(local_name);
        rewritten.push_str(&type_text[member_end..]);
        Some((rewritten, public_module, public_imported_name, local_alias))
    }

    pub(in crate::declaration_emitter) fn leading_import_type_member_name(
        tail: &str,
    ) -> Option<&str> {
        tail.split([
            '.', '<', '[', ']', ' ', '&', '|', '>', ',', ')', ';', ':', '?', '{', '}', '\n', '\r',
        ])
        .find(|part| !part.is_empty())
    }

    fn public_named_import_for_exported_name(
        source_text: &str,
        exported_name: &str,
    ) -> Option<(String, String, Option<String>)> {
        for line in source_text.lines() {
            let trimmed = line.trim();
            let Some(rest) = trimmed.strip_prefix("import ") else {
                continue;
            };
            let Some(named_start) = rest.find('{') else {
                continue;
            };
            let Some(named_end) = rest[named_start + 1..].find('}') else {
                continue;
            };
            let named = &rest[named_start + 1..named_start + 1 + named_end];
            let after_named = &rest[named_start + 1 + named_end + 1..];
            let Some((_, module_part)) = after_named.split_once(" from ") else {
                continue;
            };
            let Some(module) = Self::quoted_string_text(module_part.trim().trim_end_matches(';'))
            else {
                continue;
            };
            if module.starts_with('.') || module.starts_with('/') {
                continue;
            }
            for specifier in named.split(',') {
                let specifier = specifier.trim();
                let (imported, alias) = specifier
                    .split_once(" as ")
                    .map_or((specifier, None), |(imported, alias)| {
                        (imported.trim(), Some(alias.trim()))
                    });
                if imported == exported_name {
                    return Some((module, imported.to_string(), alias.map(str::to_string)));
                }
            }
        }
        None
    }

    pub(in crate::declaration_emitter) fn rewrite_current_source_public_import_type_text(
        &self,
        type_text: &str,
    ) -> Option<String> {
        self.rewrite_current_source_public_import_type_text_with_import(type_text)
            .map(|(rewritten, _, _, _)| rewritten)
    }

    pub(in crate::declaration_emitter) fn rewrite_current_source_named_import_type_text(
        &self,
        type_text: &str,
    ) -> Option<String> {
        let (start, module_specifier, tail) = Self::next_import_type_text(type_text)?;
        if start != 0 {
            return None;
        }
        let tail = tail.strip_prefix('.')?;
        let imported_name = Self::leading_import_type_member_name(tail)?;
        let local_name =
            self.current_source_named_import_local_name(&module_specifier, imported_name)?;
        Self::rewrite_leading_import_type_member(type_text, imported_name, &local_name)
    }

    fn rewrite_leading_import_type_member(
        type_text: &str,
        imported_name: &str,
        local_name: &str,
    ) -> Option<String> {
        let (start, _, tail) = Self::next_import_type_text(type_text)?;
        if start != 0 {
            return None;
        }
        let tail = tail.strip_prefix('.')?;
        let import_type_end = type_text.len() - (tail.len() + 1);
        let member_start = import_type_end + 1;
        let member_end = member_start + imported_name.len();
        let mut rewritten = String::with_capacity(type_text.len());
        rewritten.push_str(&type_text[..start]);
        rewritten.push_str(local_name);
        rewritten.push_str(&type_text[member_end..]);
        Some(rewritten)
    }

    fn current_source_named_import_local_name(
        &self,
        module_specifier: &str,
        imported_name: &str,
    ) -> Option<String> {
        let source_file = self
            .current_source_file_idx
            .and_then(|source_file_idx| self.arena.get(source_file_idx))
            .and_then(|node| self.arena.get_source_file(node))
            .or_else(|| self.arena_source_file(self.arena))?;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let Some(import) = self.arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_node) = self.arena.get(import.module_specifier) else {
                continue;
            };
            let Some(module_lit) = self.arena.get_literal(module_node) else {
                continue;
            };
            if module_lit.text != module_specifier {
                continue;
            }
            let Some(clause_node) = self.arena.get(import.import_clause) else {
                continue;
            };
            let Some(clause) = self.arena.get_import_clause(clause_node) else {
                continue;
            };
            if let Some(bindings_node) = self.arena.get(clause.named_bindings)
                && let Some(bindings) = self.arena.get_named_imports(bindings_node)
            {
                for &element_idx in &bindings.elements.nodes {
                    let Some(element_node) = self.arena.get(element_idx) else {
                        continue;
                    };
                    let Some(specifier) = self.arena.get_specifier(element_node) else {
                        continue;
                    };
                    let imported_idx = if specifier.property_name.is_some() {
                        specifier.property_name
                    } else {
                        specifier.name
                    };
                    if self.get_identifier_text(imported_idx).as_deref() != Some(imported_name) {
                        continue;
                    }
                    return self.get_identifier_text(specifier.name);
                }
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn rewrite_current_source_public_import_type_text_with_import(
        &self,
        type_text: &str,
    ) -> Option<(String, String, String, Option<String>)> {
        let source = self.source_file_text.as_deref()?;
        let (rewritten, module, imported, alias) =
            Self::rewrite_leading_import_type_with_public_named_import(source, type_text)?;
        let local_name = alias.as_deref().unwrap_or(imported.as_str());
        let rewritten = self.append_default_type_arguments_to_public_import_rewrite(
            type_text, rewritten, local_name,
        );
        Some((rewritten, module, imported, alias))
    }

    fn append_default_type_arguments_to_public_import_rewrite(
        &self,
        original_type_text: &str,
        rewritten: String,
        local_name: &str,
    ) -> String {
        let Some(rest) = rewritten.strip_prefix(local_name) else {
            return rewritten;
        };
        if rest.starts_with('<') {
            return rewritten;
        }
        let Some(defaults) = self.default_type_arguments_for_import_type_text(original_type_text)
        else {
            return rewritten;
        };
        if defaults.is_empty() {
            return rewritten;
        }
        format!("{local_name}<{}>{rest}", defaults.join(", "))
    }

    fn default_type_arguments_for_import_type_text(&self, type_text: &str) -> Option<Vec<String>> {
        let binder = self.binder?;
        let (_, module_specifier, tail) = Self::next_import_type_text(type_text)?;
        let type_name = Self::leading_import_type_member_name(tail.strip_prefix('.')?)?;

        let mut candidates = Vec::new();
        if let Some(sym_id) = self.find_symbol_for_import_type_text(type_text) {
            candidates.push(sym_id);
            candidates.push(self.resolve_portability_symbol(sym_id, binder));
        }
        candidates.extend(
            binder
                .symbols
                .iter()
                .filter(|symbol| symbol.escaped_name == type_name)
                .flat_map(|symbol| {
                    [
                        symbol.id,
                        self.resolve_portability_symbol(symbol.id, binder),
                    ]
                }),
        );

        let mut seen = FxHashSet::default();
        candidates.into_iter().find_map(|sym_id| {
            if !seen.insert(sym_id) {
                return None;
            }
            self.default_type_arguments_for_symbol(sym_id, &module_specifier)
        })
    }

    fn default_type_arguments_for_symbol(
        &self,
        sym_id: SymbolId,
        module_specifier: &str,
    ) -> Option<Vec<String>> {
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;

        for &decl_idx in &symbol.declarations {
            let source_arena = binder
                .get_arena_for_declaration(sym_id, decl_idx)
                .map(Arc::as_ref)
                .or_else(|| binder.symbol_arenas.get(&sym_id).map(Arc::as_ref))
                .or_else(|| self.global_symbol_arenas.get(&sym_id).map(Arc::as_ref))
                .unwrap_or(self.arena);
            let Some(class_node) = source_arena.get(decl_idx) else {
                continue;
            };
            let Some(class_data) = source_arena.get_class(class_node) else {
                continue;
            };
            let Some(type_parameters) = class_data.type_parameters.as_ref() else {
                continue;
            };
            let type_param_names = type_parameters
                .nodes
                .iter()
                .filter_map(|&param_idx| {
                    source_arena
                        .get(param_idx)
                        .and_then(|param_node| source_arena.get_type_parameter(param_node))
                        .and_then(|param| self.source_slice_from_arena(source_arena, param.name))
                })
                .collect::<FxHashSet<_>>();
            let defaults = type_parameters
                .nodes
                .iter()
                .map(|&param_idx| {
                    source_arena
                        .get(param_idx)
                        .and_then(|param_node| source_arena.get_type_parameter(param_node))
                        .and_then(|param| self.source_slice_from_arena(source_arena, param.default))
                        .map(|default| {
                            self.qualify_foreign_default_type_argument(
                                default,
                                module_specifier,
                                source_arena,
                                &type_param_names,
                            )
                        })
                })
                .collect::<Option<Vec<_>>>()?;
            return Some(defaults);
        }
        None
    }

    fn qualify_foreign_default_type_argument(
        &self,
        default_text: String,
        module_specifier: &str,
        source_arena: &NodeArena,
        type_param_names: &FxHashSet<String>,
    ) -> String {
        let trimmed = default_text.trim();
        if trimmed != default_text
            || !Self::is_plain_identifier_text(trimmed)
            || type_param_names.contains(trimmed)
            || std::ptr::eq(source_arena, self.arena)
        {
            return default_text;
        }
        let Some(binder) = self.binder else {
            return default_text;
        };
        let Some(current_path) = self.current_file_path.as_deref() else {
            return default_text;
        };

        for module_path in self.matching_module_export_paths(binder, current_path, module_specifier)
        {
            if binder
                .module_exports
                .get(module_path)
                .is_some_and(|exports| exports.has(trimmed))
            {
                return format!("import(\"{module_specifier}\").{trimmed}");
            }
        }

        if binder.symbols.iter().any(|symbol| {
            symbol.escaped_name == trimmed
                && symbol.is_exported
                && self.symbol_has_declaration_in_arena(symbol.id, source_arena)
        }) {
            return format!("import(\"{module_specifier}\").{trimmed}");
        }

        default_text
    }

    fn symbol_has_declaration_in_arena(&self, sym_id: SymbolId, source_arena: &NodeArena) -> bool {
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(symbol) = binder.symbols.get(sym_id) else {
            return false;
        };

        symbol.declarations.iter().any(|&decl_idx| {
            binder
                .get_arena_for_declaration(sym_id, decl_idx)
                .map(Arc::as_ref)
                .or_else(|| binder.symbol_arenas.get(&sym_id).map(Arc::as_ref))
                .or_else(|| self.global_symbol_arenas.get(&sym_id).map(Arc::as_ref))
                .is_some_and(|arena| std::ptr::eq(arena, source_arena))
        })
    }

    fn is_plain_identifier_text(text: &str) -> bool {
        let mut chars = text.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        (first == '_' || first == '$' || first.is_ascii_alphabetic())
            && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
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

        let (from_path, type_name) = if !from_path.contains('/')
            && (type_name.contains('/') || type_name.starts_with('.'))
        {
            (type_name, from_path)
        } else {
            (from_path, type_name)
        };

        if self.relative_node_modules_package_has_no_exports(file, from_path) {
            return;
        }

        self.diagnostics.push(Diagnostic::from_code(
            2883,
            file,
            pos,
            length,
            &[decl_name, from_path, type_name],
        ));
    }

    fn relative_node_modules_package_has_no_exports(&self, file: &str, from_path: &str) -> bool {
        use std::path::{Component, Path, PathBuf};

        if !from_path.starts_with("./node_modules/") && !from_path.starts_with("node_modules/") {
            return false;
        }

        let relative = from_path.strip_prefix("./").unwrap_or(from_path);
        let components: Vec<_> = Path::new(relative).components().collect();
        let Some(nm_idx) = components.iter().position(
            |component| matches!(component, Component::Normal(part) if *part == "node_modules"),
        ) else {
            return false;
        };

        let pkg_start = nm_idx + 1;
        let pkg_len = if components.get(pkg_start).is_some_and(
            |component| matches!(component, Component::Normal(part) if part.to_str().is_some_and(|text| text.starts_with('@'))),
        ) {
            2
        } else {
            1
        };
        if components.len() < pkg_start + pkg_len {
            return false;
        }

        let package_rel: PathBuf = components[..pkg_start + pkg_len]
            .iter()
            .map(|component| component.as_os_str())
            .collect();
        let Some(file_dir) = Path::new(file).parent() else {
            return false;
        };

        for ancestor in file_dir.ancestors() {
            let package_json = ancestor.join(&package_rel).join("package.json");
            if let Ok(pkg_content) = std::fs::read_to_string(package_json)
                && let Ok(pkg_json) = serde_json::from_str::<serde_json::Value>(&pkg_content)
            {
                return pkg_json.get("exports").is_none();
            }
        }

        false
    }

    pub(in crate::declaration_emitter) fn type_text_is_directly_nameable_reference(
        &self,
        printed: &str,
    ) -> bool {
        if printed == "any" || printed.is_empty() {
            return false;
        }

        if let Some((start, _, tail)) = Self::next_import_type_text(printed)
            && start == 0
        {
            return tail.starts_with('.')
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
        while let Some((_, specifier, rest)) = Self::next_import_type_text(remaining) {
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
                        && !self.is_bare_specifier_subpath_publicly_accessible(&specifier)
                    {
                        return true;
                    }
                }
            }
            remaining = rest;
        }
        false
    }

    pub(crate) fn import_type_uses_private_package_subpath(&self, printed: &str) -> bool {
        let Some((start, specifier, _)) = Self::next_import_type_text(printed) else {
            return false;
        };
        if start != 0 {
            return false;
        }

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

        has_subpath && !self.is_bare_specifier_subpath_publicly_accessible(&specifier)
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
        fn check_named_type(
            emitter: &DeclarationEmitter<'_>,
            interner: &tsz_solver::construction::TypeInterner,
            binder: &BinderState,
            current_file_path: &str,
            cache: &crate::type_cache_view::TypeCacheView,
            candidate_type_id: tsz_solver::types::TypeId,
            visit: &mut PortabilityVisitState<'_>,
        ) -> Option<(String, String)> {
            if let Some(def_id) = tsz_solver::lazy_def_id(interner, candidate_type_id)
                && let Some(&sym_id) = cache.def_to_symbol.get(&def_id)
            {
                if let Some(result) =
                    emitter.check_symbol_portability(sym_id, binder, current_file_path, visit)
                {
                    if let Some(alias_result) =
                        emitter.alias_reference_covering_helper_symbol(sym_id, binder, &result.0)
                    {
                        return Some(alias_result);
                    }
                    return Some(result);
                }
                if let Some(result) = emitter
                    .collect_non_portable_references_in_symbol_declaration_with_state(sym_id, visit)
                    .into_iter()
                    .next()
                {
                    return Some(result);
                }
            }

            if let Some(shape_id) = tsz_solver::object_shape_id(interner, candidate_type_id) {
                let shape = interner.object_shape(shape_id);
                if let Some(sym_id) = shape.symbol
                    && let Some(result) =
                        emitter.check_symbol_portability(sym_id, binder, current_file_path, visit)
                {
                    return Some(result);
                }
            }

            if let Some(callable_id) =
                tsz_solver::visitor::callable_shape_id(interner, candidate_type_id)
            {
                let shape = interner.callable_shape(callable_id);
                if let Some(sym_id) = shape.symbol
                    && let Some(result) =
                        emitter.check_symbol_portability(sym_id, binder, current_file_path, visit)
                {
                    return Some(result);
                }
            }

            None
        }

        let mut visit = PortabilityVisitState {
            visited_types,
            visited_symbols,
            visited_declaration_symbols,
            visited_nodes,
        };

        let interner = self.type_interner?;
        let binder = self.binder?;
        let current_file_path = self.current_file_path.as_deref()?;
        let cache = self.type_cache.as_ref()?;

        if !visit.visited_types.insert(type_id) {
            return None;
        }

        if let Some(result) = check_named_type(
            self,
            interner,
            binder,
            current_file_path,
            cache,
            type_id,
            &mut visit,
        ) {
            return Some(result);
        }

        if self.type_has_public_surface_reference_with_portable_arguments(type_id, &mut visit) {
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
                &mut visit,
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
                    &mut visit,
                ) {
                    return Some(result);
                }
                if self.type_application_has_public_surface_reference_with_portable_arguments(
                    app.base, &app.args, &mut visit,
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
                &mut visit,
            ) {
                return Some(result);
            }
            if self.type_application_has_public_surface_reference_with_portable_arguments(
                app.base, &app.args, &mut visit,
            ) {
                return None;
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
                        &mut visit,
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
                    &mut visit,
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
                        &mut visit,
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
                    &mut visit,
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
                && let Some(result) =
                    self.check_symbol_portability(sym_id, binder, current_file_path, &mut visit)
            {
                return Some(result);
            }

            if let Some(symbol_ref) =
                tsz_solver::visitor::module_namespace_symbol_ref(interner, ref_type_id)
                && let Some(sym_id) = binder
                    .symbols
                    .get(SymbolId(symbol_ref.0))
                    .map(|_| SymbolId(symbol_ref.0))
                && let Some(result) =
                    self.check_symbol_portability(sym_id, binder, current_file_path, &mut visit)
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
                &mut visit,
            ) {
                return Some(result);
            }
        }

        None
    }

    fn alias_reference_covering_helper_symbol(
        &self,
        helper_sym_id: SymbolId,
        binder: &BinderState,
        from_path: &str,
    ) -> Option<(String, String)> {
        let helper_source_path = self.get_symbol_source_path(helper_sym_id, binder)?;
        let helper_source_path = helper_source_path.replace('\\', "/");

        binder.symbols.iter().find_map(|candidate| {
            if candidate.id == helper_sym_id {
                return None;
            }
            let candidate_source_path = self
                .get_symbol_source_path(candidate.id, binder)?
                .replace('\\', "/");
            if candidate_source_path != helper_source_path {
                return None;
            }
            let source_arena = binder
                .symbol_arenas
                .get(&candidate.id)
                .or_else(|| self.global_symbol_arenas.get(&candidate.id))?;
            candidate.declarations.iter().copied().find_map(|decl_idx| {
                let decl_node = source_arena.get(decl_idx)?;
                let alias = source_arena.get_type_alias(decl_node)?;
                if matches!(source_arena.get(alias.type_node), Some(node) if node.kind == syntax_kind_ext::TYPE_LITERAL) {
                    return None;
                }
                self.type_node_subtree_references_symbol(
                    source_arena.as_ref(),
                    alias.type_node,
                    helper_sym_id,
                )
                .then(|| (from_path.to_string(), candidate.escaped_name.clone()))
            })
        })
    }

    fn type_node_subtree_references_symbol(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
        target_sym_id: SymbolId,
    ) -> bool {
        if !node_idx.is_some() {
            return false;
        }
        if let Some(binder) = self.binder
            && let Some(sym_id) = binder.get_node_symbol(node_idx)
            && self.resolve_portability_declaration_symbol(sym_id, binder)
                == self.resolve_portability_declaration_symbol(target_sym_id, binder)
        {
            return true;
        }
        if arena.get(node_idx).is_none() {
            return false;
        };
        if let Some(node) = arena.get(node_idx)
            && let Some(identifier) = arena.get_identifier(node)
            && let Some(sym_id) = self.find_symbol_in_arena_by_name(arena, &identifier.escaped_text)
            && let Some(binder) = self.binder
            && self.resolve_portability_declaration_symbol(sym_id, binder)
                == self.resolve_portability_declaration_symbol(target_sym_id, binder)
        {
            return true;
        }
        arena.get_children(node_idx).into_iter().any(|child_idx| {
            self.type_node_subtree_references_symbol(arena, child_idx, target_sym_id)
        })
    }
}
