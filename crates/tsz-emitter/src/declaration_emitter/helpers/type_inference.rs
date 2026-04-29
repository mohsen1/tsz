//! Type inference for expressions, object literals, and enums

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
    fn type_annotation_text_from_arena_node(
        &self,
        source_arena: &NodeArena,
        type_annotation: NodeIndex,
    ) -> Option<String> {
        let printed = self
            .get_node_type(type_annotation)
            .map(|type_id| self.print_type_id(type_id));
        let type_text = if std::ptr::eq(source_arena, self.arena) {
            self.preferred_annotation_name_text(type_annotation)
                .or_else(|| self.emit_type_node_text(type_annotation))
        } else {
            self.source_slice_from_arena(source_arena, type_annotation)
                .or_else(|| self.emit_type_node_text_from_arena(source_arena, type_annotation))
        }?;
        let type_text = if std::ptr::eq(source_arena, self.arena) {
            printed.filter(|text| text != "any").unwrap_or(type_text)
        } else {
            let rewritten = self.qualify_foreign_imported_names_in_text(source_arena, &type_text);
            let rewritten = self
                .expand_portable_intersection_type_text(source_arena, &rewritten)
                .unwrap_or(rewritten);
            match printed {
                Some(ref printed)
                    if printed != "any"
                        && !printed.contains("any")
                        && (!rewritten.contains("import(\"") || printed.contains("import(\"")) =>
                {
                    printed.clone()
                }
                _ => rewritten,
            }
        };
        let trimmed = type_text.trim_end();
        let trimmed = trimmed.strip_suffix(';').unwrap_or(trimmed).trim_end();
        let trimmed = trimmed.strip_suffix('=').unwrap_or(trimmed).trim_end();
        Some(trimmed.to_string())
    }

    fn expand_portable_intersection_type_text(
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

    fn expand_portable_mapped_object_text(
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
        let module_start = text.find("import(\"")? + 8;
        let module_end = text.get(module_start..)?.find("\")")? + module_start;
        let module_specifier = text.get(module_start..module_end)?.to_string();
        let export_name = text
            .get(module_end + 2..)?
            .trim()
            .strip_prefix('.')?
            .to_string();
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

        for module_path in self.matching_module_export_paths(binder, &source_path, module_specifier)
        {
            let Some(exports) = binder.module_exports.get(module_path) else {
                continue;
            };
            let Some(export_sym_id) = exports.get(export_name) else {
                continue;
            };
            if let Some(keys) =
                self.with_symbol_declarations(export_sym_id, |foreign_arena, decl_idx| {
                    let decl_node = foreign_arena.get(decl_idx)?;
                    let alias = foreign_arena.get_type_alias(decl_node)?;
                    self.expand_string_literals_from_type_node_in_arena(
                        foreign_arena,
                        alias.type_node,
                        &FxHashMap::default(),
                        0,
                    )
                })
            {
                return Some(keys);
            }
        }

        None
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

    fn find_type_alias_type_node_in_arena(
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

    fn declared_type_annotation_text_for_symbol(&self, sym_id: SymbolId) -> Option<String> {
        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let decl_idx = Self::annotation_bearing_declaration_from_arena(source_arena, decl_idx)
                .unwrap_or(decl_idx);
            let decl_node = source_arena.get(decl_idx)?;
            let type_annotation = source_arena
                .get_variable_declaration(decl_node)
                .map(|decl| decl.type_annotation)
                .or_else(|| {
                    source_arena
                        .get_property_decl(decl_node)
                        .map(|decl| decl.type_annotation)
                })
                .or_else(|| {
                    source_arena
                        .get_parameter(decl_node)
                        .map(|param| param.type_annotation)
                })
                .filter(|type_idx| type_idx.is_some())?;
            self.type_annotation_text_from_arena_node(source_arena, type_annotation)
        })
    }

    fn annotation_bearing_declaration_from_arena(
        arena: &NodeArena,
        decl_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = decl_idx;
        for _ in 0..12 {
            let node = arena.get(current)?;
            if arena.get_variable_declaration(node).is_some()
                || arena.get_property_decl(node).is_some()
                || arena.get_parameter(node).is_some()
                || arena.get_interface(node).is_some()
                || arena.get_class(node).is_some()
                || arena.get_type_alias(node).is_some()
            {
                return Some(current);
            }
            let parent = arena.parent_of(current)?;
            if parent.is_none() {
                break;
            }
            current = parent;
        }
        None
    }

    fn emit_type_node_text_from_arena(
        &self,
        source_arena: &NodeArena,
        type_idx: NodeIndex,
    ) -> Option<String> {
        source_arena.get(type_idx)?;

        let mut scratch = if let (Some(type_cache), Some(type_interner), Some(binder)) =
            (&self.type_cache, self.type_interner, self.binder)
        {
            DeclarationEmitter::with_type_info(
                source_arena,
                type_cache.clone(),
                type_interner,
                binder,
            )
        } else {
            DeclarationEmitter::new(source_arena)
        };

        let source_file = self.arena_source_file(source_arena);
        scratch.source_is_declaration_file = source_file
            .map(|source_file| source_file.is_declaration_file)
            .unwrap_or(self.source_is_declaration_file);
        scratch.source_is_js_file = self.source_is_js_file;
        scratch.current_source_file_idx = source_file
            .and_then(|_| {
                source_arena
                    .nodes
                    .iter()
                    .position(|node| source_arena.get_source_file(node).is_some())
                    .and_then(|idx| u32::try_from(idx).ok())
                    .map(NodeIndex)
            })
            .or(self.current_source_file_idx);
        scratch.source_file_text = source_file.map(|source_file| source_file.text.clone());
        scratch.current_file_path = self
            .arena_to_path
            .get(&(source_arena as *const NodeArena as usize))
            .cloned()
            .or_else(|| source_file.map(|source_file| source_file.file_name.clone()))
            .or_else(|| self.current_file_path.clone());
        scratch.current_arena = self.current_arena.clone();
        scratch.arena_to_path = self.arena_to_path.clone();
        scratch.emit_type(type_idx);
        Some(scratch.writer.take_output())
    }

    fn explicit_asserted_type_node_from_arena(
        arena: &NodeArena,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = expr_idx;

        for _ in 0..100 {
            let node = arena.get(current)?;
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = arena.get_parenthesized(node)
            {
                current = paren.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
                && let Some(unary) = arena.get_unary_expr_ex(node)
            {
                current = unary.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(binary) = arena.get_binary_expr(node)
                && binary.operator_token == SyntaxKind::CommaToken as u16
            {
                current = binary.right;
                continue;
            }

            let assertion = arena.get_type_assertion(node)?;
            let asserted_type = arena.get(assertion.type_node)?;
            if asserted_type.kind == SyntaxKind::ConstKeyword as u16 {
                return None;
            }
            return Some(assertion.type_node);
        }

        None
    }

    fn declaration_type_symbol_from_type_node(
        &self,
        arena: &NodeArena,
        type_idx: NodeIndex,
    ) -> Option<SymbolId> {
        let binder = self.binder?;
        let type_node = arena.get(type_idx)?;
        match type_node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let type_ref = arena.get_type_ref(type_node)?;
                if std::ptr::eq(arena, self.arena)
                    && let Some(name) = self.get_identifier_text(type_ref.type_name)
                    && let Some(sym_id) = self.resolve_identifier_symbol(type_ref.type_name, &name)
                {
                    Some(sym_id)
                } else {
                    binder.get_node_symbol(type_ref.type_name)
                }
            }
            k if k == SyntaxKind::Identifier as u16 || k == syntax_kind_ext::QUALIFIED_NAME => {
                if std::ptr::eq(arena, self.arena)
                    && let Some(name) = self.get_identifier_text(type_idx)
                    && let Some(sym_id) = self.resolve_identifier_symbol(type_idx, &name)
                {
                    Some(sym_id)
                } else {
                    binder.get_node_symbol(type_idx)
                }
            }
            _ => None,
        }
    }

    fn property_access_declared_type_annotation_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let binder = self.binder?;
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }

        let access = self.arena.get_access_expr(expr_node)?;
        let member_name = self.get_identifier_text(access.name_or_argument)?;
        let base_sym_id = self.value_reference_symbol(access.expression)?;

        self.with_symbol_declarations(base_sym_id, |source_arena, decl_idx| {
            let decl_idx = Self::annotation_bearing_declaration_from_arena(source_arena, decl_idx)
                .unwrap_or(decl_idx);
            let decl_node = source_arena.get(decl_idx)?;
            let declared_type = source_arena
                .get_variable_declaration(decl_node)
                .and_then(|decl| {
                    if decl.type_annotation.is_some() {
                        Some(decl.type_annotation)
                    } else if decl.initializer.is_some() {
                        Self::explicit_asserted_type_node_from_arena(source_arena, decl.initializer)
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    source_arena.get_parameter(decl_node).and_then(|param| {
                        if param.type_annotation.is_some() {
                            Some(param.type_annotation)
                        } else {
                            None
                        }
                    })
                })
                .or_else(|| {
                    source_arena.get_property_decl(decl_node).and_then(|decl| {
                        if decl.type_annotation.is_some() {
                            Some(decl.type_annotation)
                        } else if decl.initializer.is_some() {
                            Self::explicit_asserted_type_node_from_arena(
                                source_arena,
                                decl.initializer,
                            )
                        } else {
                            None
                        }
                    })
                })?;

            let declared_type_sym_id =
                self.declaration_type_symbol_from_type_node(source_arena, declared_type)?;
            let declared_type_sym_id = self
                .resolve_portability_import_alias(declared_type_sym_id, binder)
                .unwrap_or(declared_type_sym_id);
            let declared_type_sym_id =
                self.resolve_portability_declaration_symbol(declared_type_sym_id, binder);
            self.type_member_declared_type_annotation_text(declared_type_sym_id, &member_name)
        })
    }

    fn type_member_declared_type_annotation_text(
        &self,
        type_sym_id: SymbolId,
        member_name: &str,
    ) -> Option<String> {
        let binder = self.binder?;
        let member_sym_id = binder
            .symbols
            .get(type_sym_id)
            .and_then(|symbol| symbol.members.as_ref())
            .and_then(|members| members.get(member_name));
        let printed_member_type = member_sym_id.and_then(|member_sym_id| {
            self.type_cache
                .as_ref()
                .and_then(|cache| cache.symbol_types.get(&member_sym_id))
                .copied()
                .map(|type_id| self.print_type_id(type_id))
        });

        self.with_symbol_declarations(type_sym_id, |source_arena, decl_idx| {
            let decl_idx = Self::annotation_bearing_declaration_from_arena(source_arena, decl_idx)
                .unwrap_or(decl_idx);
            let decl_node = source_arena.get(decl_idx)?;
            let mut members: Vec<NodeIndex> = Vec::new();
            if let Some(interface) = source_arena.get_interface(decl_node) {
                members.extend(interface.members.nodes.iter().copied());
            }
            if let Some(class_decl) = source_arena.get_class(decl_node) {
                members.extend(class_decl.members.nodes.iter().copied());
            }
            if let Some(type_alias) = source_arena.get_type_alias(decl_node)
                && let Some(type_node) = source_arena.get(type_alias.type_node)
                && type_node.kind == syntax_kind_ext::TYPE_LITERAL
                && let Some(type_literal) = source_arena.get_type_literal(type_node)
            {
                members.extend(type_literal.members.nodes.iter().copied());
            }

            for member_idx in members {
                let Some(member_node) = source_arena.get(member_idx) else {
                    continue;
                };
                if let Some(signature) = source_arena.get_signature(member_node)
                    && self
                        .property_name_text_from_arena(source_arena, signature.name)
                        .as_deref()
                        == Some(member_name)
                    && signature.type_annotation.is_some()
                {
                    let raw = self.type_annotation_text_from_arena_node(
                        source_arena,
                        signature.type_annotation,
                    );
                    if let Some(printed) = printed_member_type.as_ref() {
                        let printed =
                            self.qualify_foreign_imported_names_in_text(source_arena, printed);
                        if !printed.contains("any")
                            && (raw.as_ref().is_none_or(|raw| raw.contains("[k in"))
                                || !printed.contains("[k in"))
                        {
                            return Some(printed);
                        }
                    }
                    return raw;
                }
                if let Some(prop_decl) = source_arena.get_property_decl(member_node)
                    && self
                        .property_name_text_from_arena(source_arena, prop_decl.name)
                        .as_deref()
                        == Some(member_name)
                    && prop_decl.type_annotation.is_some()
                {
                    let raw = self.type_annotation_text_from_arena_node(
                        source_arena,
                        prop_decl.type_annotation,
                    );
                    if let Some(printed) = printed_member_type.as_ref() {
                        let printed =
                            self.qualify_foreign_imported_names_in_text(source_arena, printed);
                        if !printed.contains("any")
                            && (raw.as_ref().is_none_or(|raw| raw.contains("[k in"))
                                || !printed.contains("[k in"))
                        {
                            return Some(printed);
                        }
                    }
                    return raw;
                }
                if let Some(accessor) = source_arena.get_accessor(member_node)
                    && self
                        .property_name_text_from_arena(source_arena, accessor.name)
                        .as_deref()
                        == Some(member_name)
                    && accessor.type_annotation.is_some()
                {
                    let raw = self.type_annotation_text_from_arena_node(
                        source_arena,
                        accessor.type_annotation,
                    );
                    if let Some(printed) = printed_member_type.as_ref() {
                        let printed =
                            self.qualify_foreign_imported_names_in_text(source_arena, printed);
                        if !printed.contains("any")
                            && (raw.as_ref().is_none_or(|raw| raw.contains("[k in"))
                                || !printed.contains("[k in"))
                        {
                            return Some(printed);
                        }
                    }
                    return raw;
                }
            }

            None
        })
    }

    fn with_symbol_declarations<T>(
        &self,
        sym_id: SymbolId,
        mut f: impl FnMut(&NodeArena, NodeIndex) -> Option<T>,
    ) -> Option<T> {
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;

        for decl_idx in symbol.declarations.iter().copied() {
            if let Some(result) = self
                .arena
                .get(decl_idx)
                .and_then(|_| f(self.arena, decl_idx))
            {
                return Some(result);
            }
            if let Some(arenas) = binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                for arena in arenas {
                    if let Some(result) = arena
                        .get(decl_idx)
                        .and_then(|_| f(arena.as_ref(), decl_idx))
                    {
                        return Some(result);
                    }
                }
            }
            if let Some(arena) = binder.symbol_arenas.get(&sym_id)
                && let Some(result) = arena
                    .get(decl_idx)
                    .and_then(|_| f(arena.as_ref(), decl_idx))
            {
                return Some(result);
            }
            if let Some(arena) = self.global_symbol_arenas.get(&sym_id)
                && let Some(result) = arena
                    .get(decl_idx)
                    .and_then(|_| f(arena.as_ref(), decl_idx))
            {
                return Some(result);
            }
        }

        None
    }

    fn replace_whole_words_in_text(text: &str, replacements: &[(String, String)]) -> String {
        if replacements.is_empty() {
            return text.to_string();
        }

        let mut result = String::with_capacity(text.len() + 16);
        let bytes = text.as_bytes();
        let text_len = bytes.len();
        let mut last_copied = 0usize;
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        let mut i = 0;
        while i < text_len {
            match bytes[i] {
                b'\'' if !in_double_quote => {
                    in_single_quote = !in_single_quote;
                    i += 1;
                    continue;
                }
                b'"' if !in_single_quote => {
                    in_double_quote = !in_double_quote;
                    i += 1;
                    continue;
                }
                _ => {}
            }

            if in_single_quote || in_double_quote {
                i += 1;
                continue;
            }

            let mut best_match: Option<(&str, usize)> = None;
            for (word, replacement) in replacements {
                let word_bytes = word.as_bytes();
                let word_len = word_bytes.len();
                if word_len == 0 || i + word_len > text_len {
                    continue;
                }
                if &bytes[i..i + word_len] != word_bytes {
                    continue;
                }
                let before_ok = i == 0 || !Self::is_ident_char_in_text(bytes[i - 1]);
                let after_ok =
                    i + word_len >= text_len || !Self::is_ident_char_in_text(bytes[i + word_len]);
                let qualified_member = i > 0 && bytes[i - 1] == b'.';
                if !before_ok || !after_ok || qualified_member {
                    continue;
                }
                if best_match.is_none_or(|(_, best_len)| word_len > best_len) {
                    best_match = Some((replacement.as_str(), word_len));
                }
            }

            if let Some((replacement, word_len)) = best_match {
                result.push_str(&text[last_copied..i]);
                result.push_str(replacement);
                i += word_len;
                last_copied = i;
                continue;
            }
            i += 1;
        }
        result.push_str(&text[last_copied..]);
        result
    }

    fn contains_whole_word_in_text(text: &str, word: &str) -> bool {
        let bytes = text.as_bytes();
        let word_bytes = word.as_bytes();
        let word_len = word_bytes.len();
        let text_len = bytes.len();
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        let mut i = 0;
        while i < text_len {
            match bytes[i] {
                b'\'' if !in_double_quote => {
                    in_single_quote = !in_single_quote;
                    i += 1;
                    continue;
                }
                b'"' if !in_single_quote => {
                    in_double_quote = !in_double_quote;
                    i += 1;
                    continue;
                }
                _ => {}
            }

            if in_single_quote || in_double_quote {
                i += 1;
                continue;
            }

            if i + word_len <= text_len && &bytes[i..i + word_len] == word_bytes {
                let before_ok = i == 0 || !Self::is_ident_char_in_text(bytes[i - 1]);
                let after_ok =
                    i + word_len >= text_len || !Self::is_ident_char_in_text(bytes[i + word_len]);
                let qualified_member = i > 0 && bytes[i - 1] == b'.';
                if before_ok && after_ok && !qualified_member {
                    return true;
                }
            }
            i += 1;
        }
        false
    }

    const fn is_ident_char_in_text(b: u8) -> bool {
        b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
    }

    fn identifier_text_from_arena(&self, arena: &NodeArena, idx: NodeIndex) -> Option<String> {
        let node = arena.get(idx)?;
        arena
            .get_identifier(node)
            .map(|ident| ident.escaped_text.clone())
    }

    fn property_name_text_from_arena(&self, arena: &NodeArena, idx: NodeIndex) -> Option<String> {
        let node = arena.get(idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return self.identifier_text_from_arena(arena, idx);
        }
        if node.kind == SyntaxKind::StringLiteral as u16
            || node.kind == SyntaxKind::NumericLiteral as u16
        {
            let literal = arena.get_literal(node)?;
            return Some(literal.text.clone());
        }
        None
    }

    fn qualify_foreign_exported_names_in_text(
        &self,
        source_arena: &NodeArena,
        source_path: &str,
        text: &str,
        excluded_names: &[String],
    ) -> String {
        let Some(current_path) = self.current_file_path.as_deref() else {
            return text.to_string();
        };
        if self.paths_refer_to_same_source_file(current_path, source_path) {
            return text.to_string();
        }

        let rel_path =
            self.strip_ts_extensions(&self.calculate_relative_path(current_path, source_path));
        let Some(source_file) = self.arena_source_file(source_arena) else {
            return text.to_string();
        };

        let mut replacements = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = source_arena.get(stmt_idx) else {
                continue;
            };
            let target_node = source_arena
                .get_export_decl(stmt_node)
                .and_then(|export| source_arena.get(export.export_clause))
                .unwrap_or(stmt_node);
            let export_name = if let Some(decl) = source_arena.get_interface(target_node) {
                (source_arena.has_modifier(&decl.modifiers, SyntaxKind::ExportKeyword)
                    || source_arena.get_export_decl(stmt_node).is_some())
                .then_some(decl.name)
            } else if let Some(decl) = source_arena.get_type_alias(target_node) {
                (source_arena.has_modifier(&decl.modifiers, SyntaxKind::ExportKeyword)
                    || source_arena.get_export_decl(stmt_node).is_some())
                .then_some(decl.name)
            } else if let Some(decl) = source_arena.get_class(target_node) {
                (source_arena.has_modifier(&decl.modifiers, SyntaxKind::ExportKeyword)
                    || source_arena.get_export_decl(stmt_node).is_some())
                .then_some(decl.name)
            } else if let Some(decl) = source_arena.get_enum(target_node) {
                (source_arena.has_modifier(&decl.modifiers, SyntaxKind::ExportKeyword)
                    || source_arena.get_export_decl(stmt_node).is_some())
                .then_some(decl.name)
            } else {
                None
            }
            .and_then(|name_idx| self.identifier_text_from_arena(source_arena, name_idx));

            let Some(export_name) = export_name else {
                continue;
            };
            if excluded_names.iter().any(|name| name == &export_name) {
                continue;
            }
            let qualified = format!("import(\"{rel_path}\").{export_name}");
            replacements.push((export_name, qualified));
        }

        Self::replace_whole_words_in_text(text, &replacements)
    }

    fn type_text_contains_unqualified_foreign_value_export(
        &self,
        source_arena: &NodeArena,
        source_path: &str,
        text: &str,
    ) -> bool {
        let Some(current_path) = self.current_file_path.as_deref() else {
            return false;
        };
        if self.paths_refer_to_same_source_file(current_path, source_path) {
            return false;
        }

        let Some(source_file) = self.arena_source_file(source_arena) else {
            return false;
        };

        source_file
            .statements
            .nodes
            .iter()
            .copied()
            .any(|stmt_idx| {
                let Some(stmt_node) = source_arena.get(stmt_idx) else {
                    return false;
                };
                let export_name = if let Some(decl) = source_arena.get_function(stmt_node) {
                    source_arena
                        .has_modifier(&decl.modifiers, SyntaxKind::ExportKeyword)
                        .then_some(decl.name)
                } else if let Some(var_stmt) = source_arena.get_variable(stmt_node) {
                    if !source_arena.has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword) {
                        None
                    } else {
                        var_stmt.declarations.nodes.first().and_then(|decl_idx| {
                            let decl_node = source_arena.get(*decl_idx)?;
                            let decl = source_arena.get_variable_declaration(decl_node)?;
                            Some(decl.name)
                        })
                    }
                } else {
                    None
                }
                .and_then(|name_idx| self.identifier_text_from_arena(source_arena, name_idx));

                export_name.is_some_and(|name| Self::contains_whole_word_in_text(text, &name))
            })
    }

    fn qualify_foreign_imported_names_in_text(
        &self,
        source_arena: &NodeArena,
        text: &str,
    ) -> String {
        let Some(source_file) = self.arena_source_file(source_arena) else {
            return text.to_string();
        };

        let mut replacements = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = source_arena.get(stmt_idx) else {
                continue;
            };
            let Some(import) = source_arena.get_import_decl(stmt_node) else {
                continue;
            };
            let Some(module_node) = source_arena.get(import.module_specifier) else {
                continue;
            };
            let Some(module_lit) = source_arena.get_literal(module_node) else {
                continue;
            };
            let module_specifier = module_lit.text.as_str();
            let Some(clause_node) = source_arena.get(import.import_clause) else {
                continue;
            };
            let Some(clause) = source_arena.get_import_clause(clause_node) else {
                continue;
            };

            if clause.name.is_some()
                && let Some(local_name) = self.identifier_text_from_arena(source_arena, clause.name)
            {
                let qualified = format!("import(\"{module_specifier}\").default");
                replacements.push((local_name, qualified));
            }

            if clause.named_bindings.is_some()
                && let Some(bindings_node) = source_arena.get(clause.named_bindings)
                && let Some(bindings) = source_arena.get_named_imports(bindings_node)
            {
                if bindings.name.is_some() && bindings.elements.nodes.is_empty() {
                    if let Some(local_name) =
                        self.identifier_text_from_arena(source_arena, bindings.name)
                    {
                        let qualified = format!("typeof import(\"{module_specifier}\")");
                        replacements.push((local_name, qualified));
                    }
                } else {
                    for &spec_idx in &bindings.elements.nodes {
                        let Some(spec_node) = source_arena.get(spec_idx) else {
                            continue;
                        };
                        let Some(specifier) = source_arena.get_specifier(spec_node) else {
                            continue;
                        };
                        let Some(local_name) =
                            self.identifier_text_from_arena(source_arena, specifier.name)
                        else {
                            continue;
                        };
                        let imported_name = if specifier.property_name.is_some() {
                            self.identifier_text_from_arena(source_arena, specifier.property_name)
                                .unwrap_or(local_name.clone())
                        } else {
                            local_name.clone()
                        };
                        let qualified = format!("import(\"{module_specifier}\").{imported_name}");
                        replacements.push((local_name, qualified));
                    }
                }
            }
        }

        Self::replace_whole_words_in_text(text, &replacements)
    }

    /// Get the type of a node from the type cache, if available.
    pub(crate) fn get_node_type(&self, node_id: NodeIndex) -> Option<tsz_solver::types::TypeId> {
        if let (Some(cache), _) = (&self.type_cache, &self.type_interner) {
            cache.node_types.get(&node_id.0).copied()
        } else {
            None
        }
    }

    /// Try to find type for a function by looking up both the declaration node and name node.
    /// The binder may map the function declaration node rather than the name identifier,
    /// so we try both.
    pub(crate) fn get_type_via_symbol_for_func(
        &self,
        func_idx: NodeIndex,
        name_node: NodeIndex,
    ) -> Option<tsz_solver::types::TypeId> {
        let cache = self.type_cache.as_ref()?;
        let binder = self.binder?;
        // Try the name node first, then the function declaration node itself
        let symbol_id = binder
            .get_node_symbol(name_node)
            .or_else(|| binder.get_node_symbol(func_idx))?;
        cache.symbol_types.get(&symbol_id).copied()
    }

    pub(crate) fn get_type_via_symbol(
        &self,
        node_id: NodeIndex,
    ) -> Option<tsz_solver::types::TypeId> {
        let binder = self.binder?;
        let symbol_id = binder.get_node_symbol(node_id)?;
        let symbol = binder.symbols.get(symbol_id)?;
        symbol
            .declarations
            .iter()
            .copied()
            .find_map(|decl_idx| self.get_node_type_or_names(&[decl_idx]))
    }

    /// Look up the cached type for a node via its symbol in `symbol_types`.
    /// Unlike `get_type_via_symbol`, this directly queries `symbol_types` without
    /// recursing through declarations — necessary for parameters whose types are
    /// stored by `cache_parameter_types` in `symbol_types` rather than `node_types`.
    pub(crate) fn get_symbol_cached_type(
        &self,
        node_id: NodeIndex,
    ) -> Option<tsz_solver::types::TypeId> {
        let cache = self.type_cache.as_ref()?;
        let binder = self.binder?;
        let sym_id = binder.get_node_symbol(node_id)?;
        cache.symbol_types.get(&sym_id).copied()
    }

    pub(crate) fn infer_fallback_type_text(&self, node_id: NodeIndex) -> Option<String> {
        self.infer_fallback_type_text_at(node_id, self.indent_level)
    }

    pub(in crate::declaration_emitter) fn infer_fallback_type_text_at(
        &self,
        node_id: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        if !node_id.is_some() {
            return None;
        }

        let node = self.arena.get(node_id)?;
        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => Some("number".to_string()),
            k if k == SyntaxKind::StringLiteral as u16 => Some("string".to_string()),
            k if k == SyntaxKind::RegularExpressionLiteral as u16 => Some("RegExp".to_string()),
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == syntax_kind_ext::TEMPLATE_EXPRESSION =>
            {
                Some("string".to_string())
            }
            k if k == SyntaxKind::TrueKeyword as u16 || k == SyntaxKind::FalseKeyword as u16 => {
                Some("boolean".to_string())
            }
            k if k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::UndefinedKeyword as u16 =>
            {
                Some("any".to_string())
            }
            k if k == syntax_kind_ext::NON_NULL_EXPRESSION => {
                let unary = self.arena.get_unary_expr_ex(node)?;
                self.preferred_expression_type_text(unary.expression)
                    .or_else(|| self.infer_fallback_type_text_at(unary.expression, depth + 1))
            }
            k if k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION =>
            {
                self.function_expression_type_text_from_ast(node_id)
            }
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                self.preferred_expression_type_text(node_id)
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                self.infer_object_literal_type_text_at(node_id, depth)
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => self
                .preferred_expression_type_text(node_id)
                .or_else(|| Some("any[]".to_string())),
            k if k == syntax_kind_ext::BINARY_EXPRESSION => self
                .infer_arithmetic_binary_type_text(node_id, depth)
                .or_else(|| {
                    self.get_node_type(node_id)
                        .map(|type_id| self.print_type_id(type_id))
                }),
            _ => self
                .get_node_type(node_id)
                .map(|type_id| self.print_type_id(type_id)),
        }
    }

    /// Infer the type of an arithmetic binary expression for declaration emit.
    /// For numeric operators (`+`, `-`, `*`, `/`, `%`, `**`, bitwise), if both
    /// operands resolve to `number`, the result is `number`.
    /// For `+` specifically, if either operand is `string`, the result is `string`.
    pub(in crate::declaration_emitter) fn infer_arithmetic_binary_type_text(
        &self,
        node_id: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        if depth > 8 {
            return None;
        }
        let node = self.arena.get(node_id)?;
        let binary = self.arena.get_binary_expr(node)?;
        let op = binary.operator_token;

        let is_numeric_op = op == SyntaxKind::MinusToken as u16
            || op == SyntaxKind::AsteriskToken as u16
            || op == SyntaxKind::AsteriskAsteriskToken as u16
            || op == SyntaxKind::SlashToken as u16
            || op == SyntaxKind::PercentToken as u16
            || op == SyntaxKind::LessThanLessThanToken as u16
            || op == SyntaxKind::GreaterThanGreaterThanToken as u16
            || op == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16
            || op == SyntaxKind::AmpersandToken as u16
            || op == SyntaxKind::BarToken as u16
            || op == SyntaxKind::CaretToken as u16;

        let is_plus = op == SyntaxKind::PlusToken as u16;

        if !is_numeric_op && !is_plus {
            return None;
        }

        // Purely numeric operators always produce number
        if is_numeric_op {
            return Some("number".to_string());
        }

        // For `+`, resolve both operands
        let left_type = self.infer_operand_type_text(binary.left, depth + 1)?;
        let right_type = self.infer_operand_type_text(binary.right, depth + 1)?;

        if left_type == "string" || right_type == "string" {
            Some("string".to_string())
        } else if left_type == "number" && right_type == "number" {
            Some("number".to_string())
        } else {
            None
        }
    }

    /// Resolve the primitive type of an operand for arithmetic type inference.
    pub(in crate::declaration_emitter) fn infer_operand_type_text(
        &self,
        node_id: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        // Try preferred expression first (finds declared types)
        if let Some(text) = self.preferred_expression_type_text(node_id) {
            return Some(text);
        }
        // Then try structural fallback
        self.infer_fallback_type_text_at(node_id, depth)
    }

    pub(crate) fn preferred_expression_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        if let Some(asserted_type_text) = self.explicit_asserted_type_text(expr_idx) {
            return Some(asserted_type_text);
        }

        let expr_idx = self.skip_parenthesized_expression(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;

        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION =>
            {
                if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    && self.get_node_type(expr_idx) == Some(tsz_solver::types::TypeId::ANY)
                {
                    return Some("any".to_string());
                }
                self.reference_declared_type_annotation_text(expr_idx)
                    .or_else(|| self.value_reference_symbol_type_text(expr_idx))
                    .or_else(|| self.undefined_identifier_type_text(expr_idx))
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                self.call_expression_declared_return_type_text(expr_idx)
            }
            k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
                self.tagged_template_declared_return_type_text(expr_idx)
            }
            k if k == syntax_kind_ext::NEW_EXPRESSION => {
                self.nameable_new_expression_type_text(expr_idx)
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                self.array_literal_expression_type_text(expr_idx)
            }
            k if k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION =>
            {
                self.function_expression_type_text_from_ast(expr_idx)
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                self.short_circuit_expression_type_text(expr_idx)
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn declaration_emittable_type_text(
        &self,
        initializer: NodeIndex,
        type_id: tsz_solver::types::TypeId,
        printed_type_text: &str,
    ) -> String {
        let initializer = self.skip_parenthesized_non_null_and_comma(initializer);

        if type_id == tsz_solver::types::TypeId::ANY
            && let Some(type_text) = self.data_view_new_expression_type_text(initializer)
        {
            return type_text;
        }

        if self.object_literal_prefers_syntax_type_text(initializer)
            && let Some(type_text) =
                self.rewrite_object_literal_computed_member_type_text(initializer, type_id)
        {
            return type_text;
        }

        if let Some(typeof_text) =
            self.typeof_prefix_for_value_entity(initializer, true, Some(type_id))
        {
            return typeof_text;
        }

        if type_id != tsz_solver::types::TypeId::ANY
            && type_id != tsz_solver::types::TypeId::ERROR
            && self
                .arena
                .get(initializer)
                .is_some_and(|node| node.kind == syntax_kind_ext::CALL_EXPRESSION)
        {
            return printed_type_text.to_string();
        }

        if (type_id != tsz_solver::types::TypeId::ANY
            || !self.initializer_is_new_expression(initializer))
            && let Some(type_text) = self.preferred_expression_type_text(initializer)
        {
            return type_text;
        }

        printed_type_text.to_string()
    }

    pub(in crate::declaration_emitter) fn explicit_asserted_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let mut current = expr_idx;

        for _ in 0..100 {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.arena.get_parenthesized(node)
            {
                current = paren.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
                && let Some(unary) = self.arena.get_unary_expr_ex(node)
            {
                current = unary.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(binary) = self.arena.get_binary_expr(node)
                && binary.operator_token == SyntaxKind::CommaToken as u16
            {
                current = binary.right;
                continue;
            }

            let assertion = self.arena.get_type_assertion(node)?;
            let asserted_type = self.arena.get(assertion.type_node)?;
            if asserted_type.kind == SyntaxKind::ConstKeyword as u16 {
                return None;
            }
            if let Some(alias_text) =
                self.local_asserted_type_alias_text(current, assertion.type_node)
            {
                return Some(alias_text);
            }
            return self.emit_type_node_text_normalized(assertion.type_node);
        }

        None
    }

    fn local_asserted_type_alias_text(
        &self,
        assertion_expr_idx: NodeIndex,
        type_node_idx: NodeIndex,
    ) -> Option<String> {
        let name = self.simple_type_reference_name_text(type_node_idx)?;
        let alias_type_node =
            self.find_enclosing_block_type_alias_type_node(assertion_expr_idx, &name)?;
        let alias_text = self.emit_type_node_text_normalized(alias_type_node)?;
        alias_text.contains("typeof ").then_some(alias_text)
    }

    fn simple_type_reference_name_text(&self, type_node_idx: NodeIndex) -> Option<String> {
        let type_node = self.arena.get(type_node_idx)?;
        if type_node.kind == SyntaxKind::Identifier as u16 {
            return self.get_identifier_text(type_node_idx);
        }
        if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
            let type_ref = self.arena.get_type_ref(type_node)?;
            return self.type_reference_name_text(type_ref.type_name);
        }
        None
    }

    fn find_enclosing_block_type_alias_type_node(
        &self,
        from_idx: NodeIndex,
        name: &str,
    ) -> Option<NodeIndex> {
        let mut current_idx = from_idx;
        while let Some(ext) = self.arena.get_extended(current_idx) {
            let parent_idx = ext.parent;
            if !parent_idx.is_some() {
                return None;
            }
            let parent_node = self.arena.get(parent_idx)?;
            if parent_node.kind == syntax_kind_ext::BLOCK
                && let Some(block) = self.arena.get_block(parent_node)
                && let Some(type_node) =
                    block.statements.nodes.iter().copied().find_map(|stmt_idx| {
                        let stmt_node = self.arena.get(stmt_idx)?;
                        if stmt_node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
                            return None;
                        }
                        let alias = self.arena.get_type_alias(stmt_node)?;
                        (self.get_identifier_text(alias.name).as_deref() == Some(name))
                            .then_some(alias.type_node)
                    })
            {
                return Some(type_node);
            }
            current_idx = parent_idx;
        }
        None
    }

    pub(in crate::declaration_emitter) fn truncation_candidate_type_node(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = expr_idx;

        for _ in 0..100 {
            let node = self.arena.get(current)?;
            if let Some(assertion) = self.arena.get_type_assertion(node) {
                let asserted_type = self.arena.get(assertion.type_node)?;
                if asserted_type.kind == SyntaxKind::ConstKeyword as u16 {
                    return None;
                }
                return Some(assertion.type_node);
            }

            if node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
                return None;
            }

            let access = self.arena.get_access_expr(node)?;
            let argument = self.arena.get(access.name_or_argument)?;
            let literal = self.arena.get_literal(argument)?;
            if argument.kind != SyntaxKind::NumericLiteral as u16 || literal.text != "0" {
                return None;
            }

            let array_node = self.arena.get(access.expression)?;
            let literal_expr = self.arena.get_literal_expr(array_node)?;
            if array_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || literal_expr.elements.nodes.len() != 1
            {
                return None;
            }

            current = literal_expr.elements.nodes[0];
        }

        None
    }

    pub(in crate::declaration_emitter) fn truncation_candidate_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let type_node = self.truncation_candidate_type_node(expr_idx)?;
        if let Some(type_id) = self.get_node_type_or_names(&[type_node]) {
            let printed = self.print_type_id(type_id);
            if printed != "any" {
                return Some(printed);
            }
        }
        self.emit_type_node_text(type_node)
    }

    pub(in crate::declaration_emitter) fn estimated_truncation_candidate_length(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<usize> {
        let type_node = self.truncation_candidate_type_node(expr_idx)?;
        self.estimate_serialized_type_length(type_node, &FxHashMap::default(), 0)
    }

    pub(in crate::declaration_emitter) fn estimate_serialized_type_length(
        &self,
        type_node: NodeIndex,
        substitutions: &FxHashMap<String, String>,
        depth: usize,
    ) -> Option<usize> {
        if depth > 32 {
            return None;
        }

        let node = self.arena.get(type_node)?;
        match node.kind {
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                let mapped = self.arena.get_mapped_type(node)?;
                let type_param = self.arena.get_type_parameter_at(mapped.type_parameter)?;
                let type_param_name = self.get_identifier_text(type_param.name)?;
                let constraint = if type_param.constraint != NodeIndex::NONE {
                    type_param.constraint
                } else {
                    return None;
                };
                let keys = self.expand_string_literals_from_type_node(
                    constraint,
                    substitutions,
                    depth + 1,
                )?;
                let mut total = 4usize;
                for key in keys {
                    let mut next = substitutions.clone();
                    next.insert(type_param_name.clone(), key.clone());
                    let value_len =
                        self.estimate_serialized_type_length(mapped.type_node, &next, depth + 1)?;
                    total = total
                        .saturating_add(self.serialized_property_name_length(&key))
                        .saturating_add(2)
                        .saturating_add(value_len)
                        .saturating_add(2);
                }
                Some(total)
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                let expansions = self.expand_string_literals_from_type_node(
                    type_node,
                    substitutions,
                    depth + 1,
                )?;
                let mut total = 0usize;
                for (idx, value) in expansions.iter().enumerate() {
                    if idx > 0 {
                        total = total.saturating_add(3);
                    }
                    total = total.saturating_add(value.len() + 2);
                }
                Some(total)
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let type_ref = self.arena.get_type_ref(node)?;
                let name = self.type_reference_name_text(type_ref.type_name)?;
                if let Some(value) = substitutions.get(&name) {
                    return Some(value.len() + 2);
                }
                let alias_type = self.find_local_type_alias_type_node(&name)?;
                self.estimate_serialized_type_length(alias_type, substitutions, depth + 1)
            }
            k if k == syntax_kind_ext::UNION_TYPE => {
                let composite = self.arena.get_composite_type(node)?;
                let mut total = 0usize;
                for (idx, child) in composite.types.nodes.iter().enumerate() {
                    if idx > 0 {
                        total = total.saturating_add(3);
                    }
                    total = total.saturating_add(self.estimate_serialized_type_length(
                        *child,
                        substitutions,
                        depth + 1,
                    )?);
                }
                Some(total)
            }
            k if k == syntax_kind_ext::LITERAL_TYPE => {
                let literal = self.arena.get_literal_type(node)?;
                let literal_node = self.arena.get(literal.literal)?;
                match literal_node.kind {
                    k if k == SyntaxKind::StringLiteral as u16 => {
                        Some(self.arena.get_literal(literal_node)?.text.len() + 2)
                    }
                    _ => None,
                }
            }
            k if k == SyntaxKind::Identifier as u16 => {
                let name = self.get_identifier_text(type_node)?;
                if let Some(value) = substitutions.get(&name) {
                    return Some(value.len() + 2);
                }
                let alias_type = self.find_local_type_alias_type_node(&name)?;
                self.estimate_serialized_type_length(alias_type, substitutions, depth + 1)
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn expand_string_literals_from_type_node(
        &self,
        type_node: NodeIndex,
        substitutions: &FxHashMap<String, String>,
        depth: usize,
    ) -> Option<Vec<String>> {
        if depth > 32 {
            return None;
        }

        let node = self.arena.get(type_node)?;
        match node.kind {
            k if k == syntax_kind_ext::LITERAL_TYPE => {
                let literal = self.arena.get_literal_type(node)?;
                let literal_node = self.arena.get(literal.literal)?;
                if literal_node.kind != SyntaxKind::StringLiteral as u16 {
                    return None;
                }
                Some(vec![self.arena.get_literal(literal_node)?.text.clone()])
            }
            k if k == syntax_kind_ext::UNION_TYPE => {
                let composite = self.arena.get_composite_type(node)?;
                let mut result = Vec::new();
                for child in &composite.types.nodes {
                    result.extend(self.expand_string_literals_from_type_node(
                        *child,
                        substitutions,
                        depth + 1,
                    )?);
                }
                Some(result)
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                let template = self.arena.get_template_literal_type(node)?;
                let head = self.arena.get(template.head)?;
                let head_text = self
                    .arena
                    .get_literal(head)
                    .map(|lit| lit.text.clone())
                    .unwrap_or_default();
                let mut results = vec![head_text];
                for span in &template.template_spans.nodes {
                    let data = self.arena.get_template_span_at(*span)?;
                    let expansions = self.expand_string_literals_from_type_node(
                        data.expression,
                        substitutions,
                        depth + 1,
                    )?;
                    let suffix = self
                        .arena
                        .get(data.literal)
                        .and_then(|literal| self.arena.get_literal(literal))
                        .map(|lit| lit.text.clone())
                        .unwrap_or_default();
                    let mut next =
                        Vec::with_capacity(results.len().saturating_mul(expansions.len()));
                    for prefix in &results {
                        for expansion in &expansions {
                            let mut combined = String::with_capacity(
                                prefix.len() + expansion.len() + suffix.len(),
                            );
                            combined.push_str(prefix);
                            combined.push_str(expansion);
                            combined.push_str(&suffix);
                            next.push(combined);
                        }
                    }
                    results = next;
                }
                Some(results)
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let type_ref = self.arena.get_type_ref(node)?;
                let name = self.type_reference_name_text(type_ref.type_name)?;
                if let Some(value) = substitutions.get(&name) {
                    return Some(vec![value.clone()]);
                }
                let alias_type = self.find_local_type_alias_type_node(&name)?;
                self.expand_string_literals_from_type_node(alias_type, substitutions, depth + 1)
            }
            k if k == SyntaxKind::Identifier as u16 => {
                let name = self.get_identifier_text(type_node)?;
                if let Some(value) = substitutions.get(&name) {
                    return Some(vec![value.clone()]);
                }
                let alias_type = self.find_local_type_alias_type_node(&name)?;
                self.expand_string_literals_from_type_node(alias_type, substitutions, depth + 1)
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn find_local_type_alias_type_node(
        &self,
        name: &str,
    ) -> Option<NodeIndex> {
        let binder = self.binder?;
        let symbol = binder
            .file_locals
            .get(name)
            .or_else(|| binder.current_scope.get(name))?;
        let declaration = binder.symbols.get(symbol)?.declarations.first().copied()?;
        let declaration_node = self.arena.get(declaration)?;
        self.arena
            .get_type_alias(declaration_node)
            .map(|alias| alias.type_node)
    }

    pub(in crate::declaration_emitter) fn type_reference_name_text(
        &self,
        name_idx: NodeIndex,
    ) -> Option<String> {
        let name_node = self.arena.get(name_idx)?;
        if name_node.kind == SyntaxKind::Identifier as u16 {
            return self.get_identifier_text(name_idx);
        }
        if name_node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qualified = self.arena.get_qualified_name(name_node)?;
            return self.get_identifier_text(qualified.right);
        }
        None
    }

    pub(in crate::declaration_emitter) fn serialized_property_name_length(
        &self,
        name: &str,
    ) -> usize {
        let mut chars = name.chars();
        let Some(first) = chars.next() else {
            return 2;
        };
        if !(first == '_' || first == '$' || first.is_ascii_alphabetic()) {
            return name.len() + 2;
        }
        if chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()) {
            name.len()
        } else {
            name.len() + 2
        }
    }

    pub(in crate::declaration_emitter) fn skip_parenthesized_expression(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = expr_idx;
        loop {
            let node = self.arena.get(current)?;
            if node.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                return Some(current);
            }
            current = self.arena.get_unary_expr_ex(node)?.expression;
        }
    }

    pub(in crate::declaration_emitter) fn arena_source_file<'arena>(
        &self,
        arena: &'arena tsz_parser::parser::node::NodeArena,
    ) -> Option<&'arena tsz_parser::parser::node::SourceFileData> {
        arena
            .nodes
            .iter()
            .rev()
            .find_map(|node| arena.get_source_file(node))
    }

    pub(in crate::declaration_emitter) fn source_slice_from_arena(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        node_idx: NodeIndex,
    ) -> Option<String> {
        let node = arena.get(node_idx)?;
        let source_file = self.arena_source_file(arena)?;
        let text = source_file.text.as_ref();
        let start = usize::try_from(node.pos).ok()?;
        let end = usize::try_from(node.end).ok()?;
        text.get(start..end).map(str::to_string)
    }

    pub(crate) fn rescued_asserts_parameter_type_text(
        &self,
        param_idx: NodeIndex,
    ) -> Option<String> {
        let param_node = self.arena.get(param_idx)?;
        let param = self.arena.get_parameter(param_node)?;
        let type_node = self.arena.get(param.type_annotation)?;
        let type_ref = self.arena.get_type_ref(type_node)?;
        if type_ref.type_arguments.is_some() {
            return None;
        }
        let type_name = self.arena.get(type_ref.type_name)?;
        let ident = self.arena.get_identifier(type_name)?;
        if ident.escaped_text != "asserts" {
            return None;
        }

        let rescued = self.scan_asserts_parameter_type_text(type_node.pos)?;
        let normalized = rescued.split_whitespace().collect::<Vec<_>>().join(" ");
        (normalized != "asserts").then_some(normalized)
    }

    pub(in crate::declaration_emitter) fn scan_asserts_parameter_type_text(
        &self,
        start: u32,
    ) -> Option<String> {
        let text = self.source_file_text.as_deref()?;
        let bytes = text.as_bytes();
        let start = usize::try_from(start).ok()?;
        if start >= bytes.len() {
            return None;
        }

        let mut i = start;
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut brace_depth = 0usize;
        let mut angle_depth = 0usize;

        while i < bytes.len() {
            match bytes[i] {
                b'(' => paren_depth += 1,
                b')' => {
                    if paren_depth == 0
                        && bracket_depth == 0
                        && brace_depth == 0
                        && angle_depth == 0
                    {
                        break;
                    }
                    paren_depth = paren_depth.saturating_sub(1);
                }
                b'[' => bracket_depth += 1,
                b']' => bracket_depth = bracket_depth.saturating_sub(1),
                b'{' => brace_depth += 1,
                b'}' => brace_depth = brace_depth.saturating_sub(1),
                b'<' => angle_depth += 1,
                b'>' => angle_depth = angle_depth.saturating_sub(1),
                b',' | b'=' | b';'
                    if paren_depth == 0
                        && bracket_depth == 0
                        && brace_depth == 0
                        && angle_depth == 0 =>
                {
                    break;
                }
                _ => {}
            }
            i += 1;
        }

        let rescued = text.get(start..i)?.trim().to_string();
        (!rescued.is_empty()).then_some(rescued)
    }

    pub(in crate::declaration_emitter) fn undefined_identifier_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        (self.get_identifier_text(expr_idx).as_deref() == Some("undefined"))
            .then(|| "any".to_string())
    }

    pub(in crate::declaration_emitter) fn reference_declared_type_annotation_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let binder = self.binder?;
        let raw_sym_id = self.value_reference_symbol(expr_idx)?;
        let sym_id = self
            .resolve_portability_import_alias(raw_sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_declaration_symbol(raw_sym_id, binder));

        self.declared_type_annotation_text_for_symbol(sym_id)
            .or_else(|| self.property_access_declared_type_annotation_text(expr_idx))
    }

    pub(in crate::declaration_emitter) fn value_reference_symbol_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let sym_id = self.value_reference_symbol(expr_idx)?;
        let binder = self.binder?;
        let cache = self.type_cache.as_ref()?;
        let resolved_sym_id = self
            .resolve_portability_import_alias(sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(sym_id, binder));
        let symbol = binder.symbols.get(resolved_sym_id)?;

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                continue;
            };

            if let Some(prop_decl) = self.arena.get_property_decl(decl_node)
                && let Some(type_id) = self.get_node_type_or_names(&[decl_idx, prop_decl.name])
            {
                let effective_type = if self
                    .arena
                    .has_modifier(&prop_decl.modifiers, SyntaxKind::ReadonlyKeyword)
                {
                    type_id
                } else {
                    self.type_interner
                        .map(|interner| {
                            tsz_solver::operations::widening::widen_literal_type(interner, type_id)
                        })
                        .unwrap_or(type_id)
                };
                return Some(self.print_type_id(effective_type));
            }

            if let Some(accessor) = self.arena.get_accessor(decl_node)
                && let Some(type_id) = self.get_node_type_or_names(&[decl_idx, accessor.name])
            {
                return Some(self.print_type_id(type_id));
            }
        }

        let type_id = cache.symbol_types.get(&resolved_sym_id).copied()?;
        Some(self.print_type_id(type_id))
    }

    pub(in crate::declaration_emitter) fn local_type_annotation_text(
        &self,
        type_idx: NodeIndex,
    ) -> Option<String> {
        let text = self.source_file_text.as_deref()?;
        let node = self.arena.get(type_idx)?;
        let start = usize::try_from(node.pos).ok()?;
        let end = usize::try_from(node.end).ok()?;
        let slice = text.get(start..end)?.trim();
        (!slice.is_empty()).then(|| slice.to_string())
    }

    pub(in crate::declaration_emitter) fn preferred_annotation_name_text(
        &self,
        type_idx: NodeIndex,
    ) -> Option<String> {
        let raw = self.local_type_annotation_text(type_idx)?;
        Self::simple_type_reference_name(&raw).map(|_| raw)
    }

    pub(in crate::declaration_emitter) fn call_expression_declared_return_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.arena.get_call_expr(expr_node)?;
        let binder = self.binder?;
        let raw_sym_id = self.value_reference_symbol(call.expression)?;
        let sym_id = self
            .resolve_portability_import_alias(raw_sym_id, binder)
            .unwrap_or_else(|| self.resolve_portability_symbol(raw_sym_id, binder));
        let type_args = self.type_argument_list_source_text(call.type_arguments.as_ref());
        self.with_symbol_declarations(sym_id, |source_arena, decl_idx| {
            let decl_node = source_arena.get(decl_idx)?;
            let func = source_arena.get_function(decl_node)?;
            let source_file = self.arena_source_file(source_arena)?;
            if !source_file.is_declaration_file || !func.type_annotation.is_some() {
                return None;
            }

            let mut type_text = self
                .source_slice_from_arena(source_arena, func.type_annotation)?
                .trim_end()
                .trim_end_matches(';')
                .trim_end()
                .to_string();

            let mut type_param_names = Vec::new();
            let mut type_param_substitutions = Vec::new();
            if !type_args.is_empty()
                && let Some(type_params) = func.type_parameters.as_ref()
            {
                for (&param_idx, arg_text) in type_params.nodes.iter().zip(type_args.iter()) {
                    if let Some(param_node) = source_arena.get(param_idx)
                        && let Some(param) = source_arena.get_type_parameter(param_node)
                        && let Some(name_text) =
                            self.identifier_text_from_arena(source_arena, param.name)
                    {
                        type_param_names.push(name_text.clone());
                        type_param_substitutions.push((name_text, arg_text.clone()));
                    }
                }
            }
            type_text = Self::replace_whole_words_in_text(&type_text, &type_param_substitutions);

            let source_path = self.get_symbol_source_path(sym_id, binder).or_else(|| {
                self.arena_to_path
                    .get(&(source_arena as *const NodeArena as usize))
                    .cloned()
            });
            type_text = self.qualify_foreign_imported_names_in_text(source_arena, &type_text);
            if let Some(source_path) = source_path.as_deref() {
                type_text = self.qualify_foreign_exported_names_in_text(
                    source_arena,
                    source_path,
                    &type_text,
                    &type_param_names,
                );
                if self
                    .current_file_path
                    .as_deref()
                    .is_some_and(|current_path| {
                        !self.paths_refer_to_same_source_file(current_path, source_path)
                            && type_text.starts_with("typeof ")
                            && !type_text.contains("import(\"")
                    })
                {
                    return None;
                }
                if self.type_text_contains_unqualified_foreign_value_export(
                    source_arena,
                    source_path,
                    &type_text,
                ) {
                    return None;
                }
            }
            Some(type_text)
        })
    }

    pub(in crate::declaration_emitter) fn tagged_template_declared_return_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION {
            return None;
        }

        let tagged = self.arena.get_tagged_template(expr_node)?;
        let sym_id = self.value_reference_symbol(tagged.tag)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        let source_arena = binder.symbol_arenas.get(&sym_id)?;
        let source_file = self.arena_source_file(source_arena.as_ref())?;
        if !source_file.is_declaration_file {
            return None;
        }

        for decl_idx in symbol.declarations.iter().copied() {
            let Some(decl_node) = source_arena.get(decl_idx) else {
                continue;
            };
            let Some(func) = source_arena.get_function(decl_node) else {
                continue;
            };
            if func.type_annotation.is_none() {
                continue;
            }
            if let Some(type_text) =
                self.source_slice_from_arena(source_arena.as_ref(), func.type_annotation)
            {
                return Some(
                    type_text
                        .trim_end()
                        .trim_end_matches(';')
                        .trim_end()
                        .to_string(),
                );
            }
        }

        None
    }

    pub(in crate::declaration_emitter) fn nameable_new_expression_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return None;
        }

        let new_expr = self.arena.get_call_expr(expr_node)?;
        let base_text = self.declaration_constructor_expression_text(new_expr.expression)?;
        let type_args = self.type_argument_list_source_text(new_expr.type_arguments.as_ref());
        if type_args.is_empty() {
            Some(base_text)
        } else {
            Some(format!("{base_text}<{}>", type_args.join(", ")))
        }
    }

    pub(in crate::declaration_emitter) fn type_argument_list_source_text(
        &self,
        type_args: Option<&NodeList>,
    ) -> Vec<String> {
        let Some(list) = type_args else {
            return Vec::new();
        };

        list.nodes
            .iter()
            .enumerate()
            .filter_map(|(index, &arg)| {
                let node = self.arena.get(arg)?;
                let mut text = self.get_source_slice_no_semi(node.pos, node.end)?;
                if self.first_type_argument_needs_parentheses(arg, index == 0) {
                    text = format!("({text})");
                }
                Some(text)
            })
            .collect()
    }

    pub(crate) fn first_type_argument_needs_parentheses(
        &self,
        type_arg_idx: NodeIndex,
        is_first: bool,
    ) -> bool {
        if !is_first {
            return false;
        }

        self.arena
            .get(type_arg_idx)
            .and_then(|node| self.arena.get_function_type(node))
            .is_some_and(|func| {
                !func
                    .type_parameters
                    .as_ref()
                    .is_none_or(|params| params.nodes.is_empty())
            })
    }

    pub(in crate::declaration_emitter) fn declaration_constructor_expression_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self.skip_parenthesized_expression(expr_idx)?;
        if self.is_module_exports_reference(expr_idx) {
            let source_file_idx = self.current_source_file_idx?;
            let source_file_node = self.arena.get(source_file_idx)?;
            let source_file = self.arena.get_source_file(source_file_node)?;
            if source_file
                .statements
                .nodes
                .iter()
                .copied()
                .any(|stmt_idx| {
                    self.js_anonymous_export_equals_class_expression_initializer(stmt_idx)
                        .is_some()
                })
            {
                return Some(r#"import(".")"#.to_string());
            }
        }
        let expr_node = self.arena.get(expr_idx)?;

        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                self.identifier_constructor_reference_text(expr_idx)
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.arena.get_access_expr(expr_node)?;
                let lhs = self.declaration_constructor_expression_text(access.expression)?;
                let rhs = self.get_identifier_text(access.name_or_argument)?;
                Some(format!("{lhs}.{rhs}"))
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn identifier_constructor_reference_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let ident = self.get_identifier_text(expr_idx)?;
        let binder = self.binder?;
        let sym_id = self.resolve_identifier_symbol(expr_idx, &ident)?;
        let symbol = binder.symbols.get(sym_id)?;

        for decl_idx in symbol.declarations.iter().copied() {
            let decl_node = self.arena.get(decl_idx)?;
            if decl_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                continue;
            }
            let import_eq = self.arena.get_import_decl(decl_node)?;
            let target_node = self.arena.get(import_eq.module_specifier)?;
            if target_node.kind == SyntaxKind::StringLiteral as u16 {
                return Some(ident);
            }
            return Some(ident);
        }

        Some(ident)
    }

    pub(in crate::declaration_emitter) fn resolve_identifier_symbol(
        &self,
        expr_idx: NodeIndex,
        ident: &str,
    ) -> Option<SymbolId> {
        let binder = self.binder?;
        let no_libs: &[Arc<BinderState>] = &[];
        binder
            .get_node_symbol(expr_idx)
            .or_else(|| {
                binder.resolve_name_with_filter(ident, self.arena, expr_idx, no_libs, |_| true)
            })
            .or_else(|| binder.file_locals.get(ident))
    }

    pub(in crate::declaration_emitter) fn array_literal_expression_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let array = self.arena.get_literal_expr(expr_node)?;
        if array.elements.nodes.is_empty() {
            return Some("any[]".to_string());
        }

        let mut element_types = Vec::with_capacity(array.elements.nodes.len());
        for elem_idx in array.elements.nodes.iter().copied() {
            // When strictNullChecks is off, skip null/undefined/void elements
            // so they don't pollute the array element type (tsc widens them away).
            if !self.strict_null_checks {
                if let Some(elem_node) = self.arena.get(elem_idx) {
                    let k = elem_node.kind;
                    if k == SyntaxKind::NullKeyword as u16
                        || k == SyntaxKind::UndefinedKeyword as u16
                    {
                        continue;
                    }
                    // Also skip void expressions (e.g., void 0)
                    if self.is_void_expression(elem_node) {
                        continue;
                    }
                }
                // Skip elements whose inferred type is null/undefined
                if let Some(type_id) = self.get_node_type_or_names(&[elem_idx])
                    && matches!(
                        type_id,
                        tsz_solver::types::TypeId::NULL
                            | tsz_solver::types::TypeId::UNDEFINED
                            | tsz_solver::types::TypeId::VOID
                    )
                {
                    continue;
                }
            }
            let elem_type = self.preferred_expression_type_text(elem_idx).or_else(|| {
                self.get_node_type_or_names(&[elem_idx])
                    .map(|type_id| self.print_type_id(type_id))
            })?;
            element_types.push(elem_type);
        }

        // If any element type is `any`, the whole union collapses to `any`
        // (matches tsc: T | any = any for all T).
        if element_types.iter().any(|t| t == "any") {
            return Some("any[]".to_string());
        }

        let mut distinct = Vec::new();
        for ty in element_types {
            if !distinct.iter().any(|existing| existing == &ty) {
                distinct.push(ty);
            }
        }

        // tsc orders union members by `TypeFlags` when printing: for the
        // primitive intrinsics the rank is Any < Unknown < String < Number
        // < Boolean < BigInt < Symbol. Our solver-inferred array-element
        // union was otherwise rendered in construction order, so
        // `var a = [1, "hello"]` printed as `(number | string)[]` instead
        // of tsc's `(string | number)[]`. Apply a stable sort that reorders
        // known primitives while keeping non-primitive members in their
        // original relative order (a comparator that returns Equal for
        // them preserves insertion order under a stable sort).
        fn primitive_rank(name: &str) -> Option<u32> {
            match name {
                "any" => Some(1),
                "unknown" => Some(2),
                "string" => Some(4),
                "number" => Some(8),
                "boolean" => Some(16),
                "bigint" => Some(64),
                "symbol" => Some(4096),
                "object" => Some(33_554_432),
                _ => None,
            }
        }
        distinct.sort_by(|a, b| match (primitive_rank(a), primitive_rank(b)) {
            (Some(ra), Some(rb)) => ra.cmp(&rb),
            _ => std::cmp::Ordering::Equal,
        });

        let elem_text = if distinct.len() == 1 {
            distinct.pop()?
        } else {
            distinct.join(" | ")
        };
        let needs_parens =
            elem_text.contains("=>") || elem_text.contains('|') || elem_text.contains('&');
        if needs_parens {
            Some(format!("({elem_text})[]"))
        } else {
            Some(format!("{elem_text}[]"))
        }
    }

    pub(in crate::declaration_emitter) fn short_circuit_expression_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        let binary = self.arena.get_binary_expr(expr_node)?;
        if binary.operator_token != SyntaxKind::BarBarToken as u16 {
            return None;
        }
        if !self.expression_is_always_truthy_for_decl_emit(binary.left) {
            return None;
        }

        self.preferred_expression_type_text(binary.left)
            .or_else(|| {
                self.get_node_type_or_names(&[binary.left])
                    .map(|type_id| self.print_type_id(type_id))
            })
    }

    pub(in crate::declaration_emitter) fn emit_type_node_text(
        &self,
        type_idx: NodeIndex,
    ) -> Option<String> {
        self.emit_type_node_text_impl(type_idx, true)
    }

    // Like `emit_type_node_text` but omits `source_file_text` from the scratch
    // emitter so that string literals are normalized to double quotes.
    // tsc normalizes quotes in type assertions (e.g. `x as T<'a'>` → `T<"a">`).
    pub(in crate::declaration_emitter) fn emit_type_node_text_normalized(
        &self,
        type_idx: NodeIndex,
    ) -> Option<String> {
        self.emit_type_node_text_impl(type_idx, false)
    }

    fn emit_type_node_text_impl(
        &self,
        type_idx: NodeIndex,
        preserve_source_quotes: bool,
    ) -> Option<String> {
        self.arena.get(type_idx)?;

        let mut scratch = if let (Some(type_cache), Some(type_interner), Some(binder)) =
            (&self.type_cache, self.type_interner, self.binder)
        {
            DeclarationEmitter::with_type_info(
                self.arena,
                type_cache.clone(),
                type_interner,
                binder,
            )
        } else {
            DeclarationEmitter::new(self.arena)
        };

        scratch.source_is_declaration_file = self.source_is_declaration_file;
        scratch.source_is_js_file = self.source_is_js_file;
        scratch.current_source_file_idx = self.current_source_file_idx;
        if preserve_source_quotes {
            scratch.source_file_text = self.source_file_text.clone();
        }
        scratch.current_file_path = self.current_file_path.clone();
        scratch.current_arena = self.current_arena.clone();
        scratch.arena_to_path = self.arena_to_path.clone();
        scratch.emit_type(type_idx);
        Some(scratch.writer.take_output())
    }

    pub(in crate::declaration_emitter) fn expression_is_always_truthy_for_decl_emit(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(expr_idx) = self.skip_parenthesized_expression(expr_idx) else {
            return false;
        };
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };

        match expr_node.kind {
            k if k == syntax_kind_ext::NEW_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                || k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::CLASS_EXPRESSION =>
            {
                true
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                self.arena.get_binary_expr(expr_node).is_some_and(|binary| {
                    binary.operator_token == SyntaxKind::BarBarToken as u16
                        && self.expression_is_always_truthy_for_decl_emit(binary.left)
                })
            }
            _ => false,
        }
    }

    pub(in crate::declaration_emitter) fn function_body_preferred_return_type_text(
        &self,
        body_idx: NodeIndex,
    ) -> Option<String> {
        let body_node = self.arena.get(body_idx)?;
        let block = self.arena.get_block(body_node)?;
        let mut preferred = None;
        if self.collect_unique_return_type_text_from_block(&block.statements, &mut preferred) {
            preferred
        } else {
            None
        }
    }

    pub(in crate::declaration_emitter) fn should_prefer_source_return_type_text(
        &self,
        source_type_text: &str,
        inferred_return_type: tsz_solver::types::TypeId,
    ) -> bool {
        if !source_type_text.contains("typeof ") {
            return false;
        }
        !self.print_type_id(inferred_return_type).contains("typeof ")
    }

    pub(in crate::declaration_emitter) fn collect_unique_return_type_text_from_block(
        &self,
        statements: &NodeList,
        preferred: &mut Option<String>,
    ) -> bool {
        statements.nodes.iter().copied().all(|stmt_idx| {
            self.collect_unique_return_type_text_from_statement(stmt_idx, preferred)
        })
    }

    pub(in crate::declaration_emitter) fn collect_unique_return_type_text_from_statement(
        &self,
        stmt_idx: NodeIndex,
        preferred: &mut Option<String>,
    ) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return true;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                let Some(ret) = self.arena.get_return_statement(stmt_node) else {
                    return false;
                };
                let type_text = if !ret.expression.is_some() {
                    // `return;` with no expression contributes `void` to the
                    // function's return type — tsc's inference for a bare
                    // return is equivalent to `return undefined` with
                    // widening to `void`. Matches declFileTypeAnnotationBuiltInType.
                    "void".to_string()
                } else if let Some(text) = self
                    .preferred_expression_type_text(ret.expression)
                    .filter(|text| !text.is_empty() && text != "any")
                {
                    text
                } else if let Some(text) = self
                    .local_variable_initializer_type_text(ret.expression)
                    .filter(|text| !text.is_empty())
                {
                    text
                } else if let Some(text) = self
                    .infer_fallback_type_text_at(ret.expression, 0)
                    .filter(|text| !text.is_empty())
                {
                    text
                } else {
                    return false;
                };
                if let Some(existing) = preferred.as_ref() {
                    existing == &type_text
                } else {
                    *preferred = Some(type_text);
                    true
                }
            }
            k if k == syntax_kind_ext::BLOCK => {
                self.arena.get_block(stmt_node).is_some_and(|block| {
                    self.collect_unique_return_type_text_from_block(&block.statements, preferred)
                })
            }
            k if k == syntax_kind_ext::IF_STATEMENT => self
                .arena
                .get_if_statement(stmt_node)
                .is_some_and(|if_data| {
                    if if_data.else_statement.is_none() {
                        let mut ignored = preferred.clone();
                        return self.collect_unique_return_type_text_from_statement(
                            if_data.then_statement,
                            &mut ignored,
                        );
                    }
                    self.collect_unique_return_type_text_from_statement(
                        if_data.then_statement,
                        preferred,
                    ) && self.collect_unique_return_type_text_from_statement(
                        if_data.else_statement,
                        preferred,
                    )
                }),
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                self.arena.get_try(stmt_node).is_some_and(|try_data| {
                    self.collect_unique_return_type_text_from_statement(
                        try_data.try_block,
                        preferred,
                    ) && try_data.catch_clause.is_some()
                        && self.collect_unique_return_type_text_from_statement(
                            try_data.catch_clause,
                            preferred,
                        )
                        && try_data.finally_block.is_some()
                        && self.collect_unique_return_type_text_from_statement(
                            try_data.finally_block,
                            preferred,
                        )
                })
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => self
                .arena
                .get_catch_clause(stmt_node)
                .is_some_and(|catch_data| {
                    self.collect_unique_return_type_text_from_statement(catch_data.block, preferred)
                }),
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
                self.arena.get_case_clause(stmt_node).is_some_and(|clause| {
                    self.collect_unique_return_type_text_from_block(&clause.statements, preferred)
                })
            }
            _ => true,
        }
    }

    fn local_variable_initializer_type_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.value_reference_symbol(expr_idx)?;
        let binder = self.binder?;
        let symbol = binder.symbols.get(sym_id)?;
        for decl_idx in symbol.declarations.iter().copied() {
            let decl_node = self.arena.get(decl_idx)?;
            let Some(var_decl) = self.arena.get_variable_declaration(decl_node) else {
                continue;
            };
            if !var_decl.initializer.is_some() {
                continue;
            }
            if let Some(type_text) = self
                .preferred_expression_type_text(var_decl.initializer)
                .or_else(|| self.infer_fallback_type_text_at(var_decl.initializer, 0))
            {
                return Some(type_text);
            }
        }
        None
    }

    fn function_expression_type_text_from_ast(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::ARROW_FUNCTION
            && expr_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION
        {
            return None;
        }
        let func = self.arena.get_function(expr_node)?;

        let mut scratch = if let (Some(type_cache), Some(type_interner), Some(binder)) =
            (&self.type_cache, self.type_interner, self.binder)
        {
            DeclarationEmitter::with_type_info(
                self.arena,
                type_cache.clone(),
                type_interner,
                binder,
            )
        } else {
            DeclarationEmitter::new(self.arena)
        };
        scratch.source_is_declaration_file = self.source_is_declaration_file;
        scratch.source_is_js_file = self.source_is_js_file;
        scratch.current_source_file_idx = self.current_source_file_idx;
        scratch.source_file_text = self.source_file_text.clone();
        scratch.current_file_path = self.current_file_path.clone();
        scratch.current_arena = self.current_arena.clone();
        scratch.arena_to_path = self.arena_to_path.clone();
        scratch.indent_level = self.indent_level;

        if let Some(ref type_params) = func.type_parameters
            && !type_params.nodes.is_empty()
        {
            scratch.emit_type_parameters(type_params);
        }
        scratch.write("(");
        scratch.emit_parameters_with_body(&func.parameters, func.body);
        scratch.write(") => ");
        if func.type_annotation.is_some() {
            scratch.emit_type(func.type_annotation);
        } else if func.body.is_some() && scratch.body_returns_void(func.body) {
            scratch.write("void");
        } else if let Some(return_type) =
            scratch.function_body_preferred_return_type_text(func.body)
        {
            scratch.write(&return_type);
        } else {
            scratch.write("any");
        }
        Some(scratch.writer.take_output())
    }

    pub(in crate::declaration_emitter) fn infer_object_literal_type_text_at(
        &self,
        object_expr_idx: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        let object_node = self.arena.get(object_expr_idx)?;
        let object = self.arena.get_literal_expr(object_node)?;

        // Pre-scan: collect setter and getter names for accessor pair handling
        let mut setter_names = rustc_hash::FxHashSet::<String>::default();
        let mut getter_names = rustc_hash::FxHashSet::<String>::default();
        for &idx in &object.elements.nodes {
            if let Some(n) = self.arena.get(idx) {
                if n.kind == syntax_kind_ext::SET_ACCESSOR {
                    if let Some(acc) = self.arena.get_accessor(n)
                        && let Some(name) = self.object_literal_member_name_text(acc.name)
                    {
                        setter_names.insert(name);
                    }
                } else if n.kind == syntax_kind_ext::GET_ACCESSOR
                    && let Some(acc) = self.arena.get_accessor(n)
                    && let Some(name) = self.object_literal_member_name_text(acc.name)
                {
                    getter_names.insert(name);
                }
            }
        }

        let mut members = Vec::new();
        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let Some(name_idx) = self.object_literal_member_name_idx(member_node) else {
                continue;
            };
            let Some(name) = self.object_literal_member_name_text(name_idx) else {
                continue;
            };

            if let Some(member_text) = self.infer_object_member_type_text_named_at(
                member_idx,
                &name,
                depth + 1,
                getter_names.contains(&name),
                setter_names.contains(&name),
            ) {
                members.push(member_text);
            }
        }

        if members.is_empty() {
            Some("{}".to_string())
        } else {
            // Format as multi-line to match tsc's .d.ts output
            let member_indent = "    ".repeat((depth + 1) as usize);
            let closing_indent = "    ".repeat(depth as usize);
            let formatted_members: Vec<String> = members
                .iter()
                .map(|m| Self::format_object_member_entry(&member_indent, m))
                .collect();
            Some(format!(
                "{{\n{}\n{closing_indent}}}",
                formatted_members.join("\n")
            ))
        }
    }

    pub(in crate::declaration_emitter) fn infer_object_member_type_text_named_at(
        &self,
        member_idx: NodeIndex,
        name: &str,
        depth: u32,
        getter_exists: bool,
        setter_exists: bool,
    ) -> Option<String> {
        let member_node = self.arena.get(member_idx)?;

        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                let data = self.arena.get_property_assignment(member_node)?;
                let type_text = self
                    .preferred_object_member_initializer_type_text(data.initializer, depth)
                    .unwrap_or_else(|| "any".to_string());
                Some(Self::format_object_member_type_text(
                    name, &type_text, depth,
                ))
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                let data = self.arena.get_shorthand_property(member_node)?;
                // For `{ foo }` the value reference is the name identifier itself.
                // For `{ foo = expr }` (CoverInitializedName) the assignment
                // initializer holds the default value.
                let initializer = if data.object_assignment_initializer == NodeIndex::NONE {
                    data.name
                } else {
                    data.object_assignment_initializer
                };
                let type_text = self
                    .preferred_object_member_initializer_type_text(initializer, depth)
                    .unwrap_or_else(|| "any".to_string());
                Some(Self::format_object_member_type_text(
                    name, &type_text, depth,
                ))
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                let data = self.arena.get_accessor(member_node)?;
                // Infer return type: explicit annotation > body inference > any
                let type_text = self
                    .infer_fallback_type_text_at(data.type_annotation, depth)
                    .or_else(|| self.function_body_preferred_return_type_text(data.body))
                    .unwrap_or_else(|| "any".to_string());
                let readonly = if setter_exists { "" } else { "readonly " };
                Some(format!("{readonly}{name}: {type_text}"))
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                if getter_exists {
                    return None;
                }

                let data = self.arena.get_accessor(member_node)?;
                let type_text = data
                    .parameters
                    .nodes
                    .first()
                    .and_then(|&p_idx| self.arena.get(p_idx))
                    .and_then(|p_node| self.arena.get_parameter(p_node))
                    .and_then(|param| {
                        self.infer_fallback_type_text_at(param.type_annotation, depth)
                    })
                    .unwrap_or_else(|| "any".to_string());
                Some(format!("{name}: {type_text}"))
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let data = self.arena.get_method_decl(member_node)?;
                let type_text = if data.parameters.nodes.is_empty() {
                    "readonly ".to_string()
                } else {
                    String::new()
                };
                Some(format!("{type_text}{name}: any"))
            }
            _ => None,
        }
    }

    fn format_object_member_type_text(name: &str, type_text: &str, depth: u32) -> String {
        if !type_text.contains('\n') {
            return format!("{name}: {type_text}");
        }

        let _ = depth;
        format!("{name}: {type_text}")
    }

    fn format_object_member_entry(member_indent: &str, member_text: &str) -> String {
        let mut lines = member_text.lines();
        let first = lines.next().unwrap_or(member_text);
        let mut result = String::new();
        result.push_str(member_indent);
        result.push_str(first);
        for line in lines {
            result.push('\n');
            result.push_str(line);
        }
        if !result.trim_end().ends_with(';') {
            result.push(';');
        }
        result
    }

    pub(in crate::declaration_emitter) fn object_literal_member_name_text(
        &self,
        name_idx: NodeIndex,
    ) -> Option<String> {
        self.resolved_computed_property_name_text(name_idx)
            .or_else(|| self.infer_property_name_text(name_idx))
    }

    pub(in crate::declaration_emitter) fn resolved_computed_property_name_text(
        &self,
        name_idx: NodeIndex,
    ) -> Option<String> {
        let name_node = self.arena.get(name_idx)?;
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }

        let computed = self.arena.get_computed_property(name_node)?;
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(computed.expression);
        let expr_node = self.arena.get(expr_idx)?;

        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION =>
            {
                if let Some(interner) = self.type_interner
                    && let Some(type_id) = self.get_node_type_or_names(&[expr_idx])
                    && let Some(literal) = tsz_solver::visitor::literal_value(interner, type_id)
                {
                    return Some(Self::format_property_name_literal_value(&literal, interner));
                }
                // Fallback: an enum member access (e.g. `[E.A]`) is a valid
                // property-name source even when the type cache hasn't
                // produced a `Literal` form for it. Detecting it via the
                // binder lets the caller keep method/getter syntax instead
                // of degrading to `[E.A]: () => T`.
                self.enum_member_access_name_text(expr_idx)
            }
            _ => None,
        }
    }

    /// If `expr_idx` is a value reference whose symbol is an enum member,
    /// return the member's escaped name. This is used as a fallback to keep
    /// method-like dts syntax for `[E.A]() {}` even when the type system
    /// hasn't produced a literal type for the access expression.
    pub(in crate::declaration_emitter) fn enum_member_access_name_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let binder = self.binder?;
        let sym_id = self.value_reference_symbol(expr_idx)?;
        let symbol = binder.symbols.get(sym_id)?;
        if symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER == 0 {
            return None;
        }
        Some(symbol.escaped_name.clone())
    }

    pub(in crate::declaration_emitter) fn format_property_name_literal_value(
        literal: &tsz_solver::types::LiteralValue,
        interner: &tsz_solver::TypeInterner,
    ) -> String {
        match literal {
            tsz_solver::types::LiteralValue::String(atom) => {
                Self::format_property_name_literal_text(&interner.resolve_atom(*atom))
            }
            tsz_solver::types::LiteralValue::Number(n) => Self::format_js_number(n.0),
            tsz_solver::types::LiteralValue::Boolean(b) => b.to_string(),
            tsz_solver::types::LiteralValue::BigInt(atom) => {
                format!("{}n", interner.resolve_atom(*atom))
            }
        }
    }

    pub(in crate::declaration_emitter) fn format_property_name_literal_text(text: &str) -> String {
        if Self::is_unquoted_property_name(text) {
            text.to_string()
        } else {
            format!("\"{}\"", super::escape_string_for_double_quote(text))
        }
    }

    pub(in crate::declaration_emitter) fn is_unquoted_property_name(text: &str) -> bool {
        let mut chars = text.chars();
        let Some(first) = chars.next() else {
            return false;
        };

        if !(first == '_' || first == '$' || first.is_ascii_alphabetic()) {
            return false;
        }

        chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
    }

    pub(in crate::declaration_emitter) fn preferred_object_member_initializer_type_text(
        &self,
        initializer: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        let type_id = self.get_node_type_or_names(&[initializer]);
        if let Some(typeof_text) = self.typeof_prefix_for_value_entity(initializer, true, type_id) {
            return Some(typeof_text);
        }
        if let Some(enum_type_text) = self.enum_member_widened_type_text(initializer) {
            return Some(enum_type_text);
        }
        self.preferred_expression_type_text(initializer)
            .or_else(|| self.infer_fallback_type_text_at(initializer, depth))
    }

    pub(in crate::declaration_emitter) fn enum_member_widened_type_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        let access = self.arena.get_access_expr(expr_node)?;
        let binder = self.binder?;

        let member_sym_id = self.value_reference_symbol(expr_idx)?;
        let member_symbol = binder.symbols.get(member_sym_id)?;
        if !member_symbol.has_any_flags(symbol_flags::ENUM_MEMBER) {
            return None;
        }

        let enum_expr = self.skip_parenthesized_non_null_and_comma(access.expression);
        let enum_sym_id = self.value_reference_symbol(enum_expr)?;
        let enum_symbol = binder.symbols.get(enum_sym_id)?;
        if !enum_symbol.has_any_flags(symbol_flags::ENUM)
            || enum_symbol.has_any_flags(symbol_flags::ENUM_MEMBER)
        {
            return None;
        }

        self.nameable_constructor_expression_text(enum_expr)
    }

    pub(in crate::declaration_emitter) fn infer_property_name_text(
        &self,
        node_id: NodeIndex,
    ) -> Option<String> {
        let node = self.arena.get(node_id)?;
        if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            let computed = self.arena.get_computed_property(node)?;
            let expr_idx = self
                .arena
                .skip_parenthesized_and_assertions_and_comma(computed.expression);
            let expr_node = self.arena.get(expr_idx)?;
            match expr_node.kind {
                k if k == SyntaxKind::StringLiteral as u16 => {
                    let literal = self.arena.get_literal(expr_node)?;
                    let quote = self.original_quote_char(expr_node);
                    return Some(format!("{}{}{}", quote, literal.text, quote));
                }
                k if k == SyntaxKind::NumericLiteral as u16 => {
                    let literal = self.arena.get_literal(expr_node)?;
                    return Some(Self::normalize_numeric_literal(literal.text.as_ref()));
                }
                k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                    let unary = self.arena.get_unary_expr(expr_node)?;
                    let operand_idx = self
                        .arena
                        .skip_parenthesized_and_assertions_and_comma(unary.operand);
                    let operand_node = self.arena.get(operand_idx)?;
                    if operand_node.kind != SyntaxKind::NumericLiteral as u16 {
                        return None;
                    }
                    let literal = self.arena.get_literal(operand_node)?;
                    let normalized = Self::normalize_numeric_literal(literal.text.as_ref());
                    return match unary.operator {
                        k if k == SyntaxKind::MinusToken as u16 => Some(format!("[-{normalized}]")),
                        k if k == SyntaxKind::PlusToken as u16 => Some(normalized),
                        _ => None,
                    };
                }
                k if k == SyntaxKind::Identifier as u16
                    || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION =>
                {
                    // Use the COMPUTED_PROPERTY_NAME node's source slice.
                    // The node.end may extend past `]` into trailing `:`
                    // (property colon) or `(` (getter/method params), so
                    // trim to the closing `]` to avoid `::` or `(` leaking.
                    if let Some(mut s) = self.get_source_slice(node.pos, node.end) {
                        // Find the last `]` and truncate after it
                        if let Some(bracket_pos) = s.rfind(']') {
                            s.truncate(bracket_pos + 1);
                        } else {
                            // No brackets — trim trailing punctuation
                            while s.ends_with(':') || s.ends_with('(') {
                                s.pop();
                                s = s.trim_end().to_string();
                            }
                        }
                        if !s.is_empty() {
                            return Some(s);
                        }
                    }
                    return None;
                }
                _ => return None,
            }
        }
        if let Some(ident) = self.arena.get_identifier(node) {
            return Some(ident.escaped_text.clone());
        }
        if let Some(literal) = self.arena.get_literal(node) {
            let quote = self.original_quote_char(node);
            return Some(format!("{}{}{}", quote, literal.text, quote));
        }
        self.get_source_slice(node.pos, node.end)
    }

    pub(in crate::declaration_emitter) fn skip_parenthesized_non_null_and_comma(
        &self,
        mut idx: NodeIndex,
    ) -> NodeIndex {
        for _ in 0..100 {
            let Some(node) = self.arena.get(idx) else {
                return idx;
            };
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.arena.get_parenthesized(node)
            {
                idx = paren.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::NON_NULL_EXPRESSION
                && let Some(unary) = self.arena.get_unary_expr_ex(node)
            {
                idx = unary.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(binary) = self.arena.get_binary_expr(node)
                && binary.operator_token == SyntaxKind::CommaToken as u16
            {
                idx = binary.right;
                continue;
            }
            return idx;
        }
        idx
    }

    pub(in crate::declaration_emitter) fn semantic_simple_enum_access(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        if !self.is_simple_enum_access(expr_node) {
            return None;
        }

        let access = self.arena.get_access_expr(expr_node)?;
        let base_name = self.get_identifier_text(access.expression)?;

        if let Some(binder) = self.binder
            && let Some(symbol_id) = binder.get_node_symbol(access.expression)
            && let Some(symbol) = binder.symbols.get(symbol_id)
            && symbol.flags & tsz_binder::symbol_flags::ENUM != 0
            && symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER == 0
        {
            return Some(expr_idx);
        }

        let source_file_idx = self.current_source_file_idx?;
        let source_file_node = self.arena.get(source_file_idx)?;
        let source_file = self.arena.get_source_file(source_file_node)?;
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::ENUM_DECLARATION {
                continue;
            }
            if let Some(enum_data) = self.arena.get_enum(stmt_node)
                && self.get_identifier_text(enum_data.name).as_deref() == Some(base_name.as_str())
            {
                return Some(expr_idx);
            }
        }
        None
    }

    pub(crate) fn simple_enum_access_member_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_idx = self.semantic_simple_enum_access(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;
        let access = self.arena.get_access_expr(expr_node)?;
        let base_name = self.get_identifier_text(access.expression)?;
        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let member_name = self.get_identifier_text(access.name_or_argument)?;
            return Some(format!("{base_name}.{member_name}"));
        }

        if expr_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let member_node = self.arena.get(access.name_or_argument)?;
            let member_text = self.get_source_slice(member_node.pos, member_node.end)?;
            return Some(format!("{base_name}[{member_text}]"));
        }

        None
    }

    pub(crate) fn simple_enum_access_base_name_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_idx = self.semantic_simple_enum_access(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;
        let access = self.arena.get_access_expr(expr_node)?;
        let base_node = self.arena.get(access.expression)?;
        self.get_source_slice(base_node.pos, base_node.end)
    }

    pub(crate) fn const_asserted_enum_access_member_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        let assertion = self.arena.get_type_assertion(expr_node)?;
        let type_node = self.arena.get(assertion.type_node)?;
        let type_text = self.get_source_slice(type_node.pos, type_node.end)?;
        if type_text != "const" {
            return None;
        }

        self.simple_enum_access_member_text(assertion.expression)
    }

    pub(in crate::declaration_emitter) fn invalid_const_enum_object_access(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.arena.get_access_expr(expr_node) else {
            return false;
        };
        let Some(base_name) = self.get_identifier_text(access.expression) else {
            return false;
        };

        let is_const_enum = if let Some(binder) = self.binder
            && let Some(symbol_id) = binder.get_node_symbol(access.expression)
            && let Some(symbol) = binder.symbols.get(symbol_id)
        {
            symbol.flags & tsz_binder::symbol_flags::CONST_ENUM != 0
        } else if let Some(source_file_idx) = self.current_source_file_idx
            && let Some(source_file_node) = self.arena.get(source_file_idx)
            && let Some(source_file) = self.arena.get_source_file(source_file_node)
        {
            source_file
                .statements
                .nodes
                .iter()
                .copied()
                .any(|stmt_idx| {
                    let Some(stmt_node) = self.arena.get(stmt_idx) else {
                        return false;
                    };
                    if stmt_node.kind != syntax_kind_ext::ENUM_DECLARATION {
                        return false;
                    }
                    let Some(enum_data) = self.arena.get_enum(stmt_node) else {
                        return false;
                    };
                    self.get_identifier_text(enum_data.name).as_deref() == Some(base_name.as_str())
                        && self
                            .arena
                            .has_modifier(&enum_data.modifiers, SyntaxKind::ConstKeyword)
                })
        } else {
            false
        };
        if !is_const_enum {
            return false;
        }

        let argument_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(access.name_or_argument);
        self.arena
            .get(argument_idx)
            .is_some_and(|arg| arg.kind != SyntaxKind::StringLiteral as u16)
    }

    pub(in crate::declaration_emitter) fn object_literal_prefers_syntax_type_text(
        &self,
        initializer: NodeIndex,
    ) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(object) = self.arena.get_literal_expr(init_node) else {
            return false;
        };

        object
            .elements
            .nodes
            .iter()
            .copied()
            .any(|member_idx| self.object_literal_member_needs_syntax_override(member_idx))
    }

    pub(in crate::declaration_emitter) fn rewrite_object_literal_computed_member_type_text(
        &self,
        initializer: NodeIndex,
        type_id: tsz_solver::types::TypeId,
    ) -> Option<String> {
        let init_node = self.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let object = self.arena.get_literal_expr(init_node)?;

        let mut setter_names = rustc_hash::FxHashSet::<String>::default();
        let mut getter_names = rustc_hash::FxHashSet::<String>::default();
        for &idx in &object.elements.nodes {
            if let Some(n) = self.arena.get(idx) {
                if n.kind == syntax_kind_ext::SET_ACCESSOR {
                    if let Some(acc) = self.arena.get_accessor(n)
                        && let Some(name) = self.object_literal_member_name_text(acc.name)
                    {
                        setter_names.insert(name);
                    }
                } else if n.kind == syntax_kind_ext::GET_ACCESSOR
                    && let Some(acc) = self.arena.get_accessor(n)
                    && let Some(name) = self.object_literal_member_name_text(acc.name)
                {
                    getter_names.insert(name);
                }
            }
        }

        let mut computed_members = Vec::new();
        let mut overridden_members = Vec::new();
        let mut concrete_member_names = Vec::new();
        let mut only_numeric_like = true;
        let mut has_non_emittable_computed_members = false;
        let mut synthetic_number_index_member = None;

        for &member_idx in &object.elements.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let name_idx = if let Some(data) = self.arena.get_property_assignment(member_node) {
                Some(data.name)
            } else if let Some(data) = self.arena.get_shorthand_property(member_node) {
                Some(data.name)
            } else if let Some(data) = self.arena.get_accessor(member_node) {
                Some(data.name)
            } else {
                self.arena
                    .get_method_decl(member_node)
                    .map(|data| data.name)
            };
            let Some(name_idx) = name_idx else {
                continue;
            };
            let Some(name_node) = self.arena.get(name_idx) else {
                continue;
            };
            if !self.object_literal_member_needs_syntax_override(member_idx) {
                continue;
            }

            let Some(name_text) = self.object_literal_member_name_text(name_idx) else {
                if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    has_non_emittable_computed_members = true;
                    if synthetic_number_index_member.is_none() {
                        synthetic_number_index_member = self
                            .infer_object_member_type_text_named_at(
                                member_idx,
                                "[x: number]",
                                self.indent_level + 1,
                                false,
                                false,
                            );
                    }
                }
                continue;
            };
            concrete_member_names.push(name_text.clone());
            let preserve_computed_syntax = name_node.kind
                == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                && self
                    .resolved_computed_property_name_text(name_idx)
                    .is_none();
            let Some(member_text) = self.infer_object_member_type_text_named_at(
                member_idx,
                &name_text,
                self.indent_level + 1,
                getter_names.contains(&name_text),
                setter_names.contains(&name_text),
            ) else {
                continue;
            };
            if preserve_computed_syntax {
                // Skip methods with computed names — the solver already produces correct
                // method signatures (e.g., `"new"(x: number): number`). Overriding them
                // would emit a wrong property form like `"new": any`.
                if member_node.kind == syntax_kind_ext::METHOD_DECLARATION {
                    continue;
                }
                only_numeric_like &= Self::is_numeric_property_name_text(&name_text);
                computed_members.push((name_text, member_text));
            } else {
                overridden_members.push((name_text, member_text));
            }
        }

        if computed_members.is_empty() && overridden_members.is_empty() {
            return None;
        }

        if overridden_members
            .iter()
            .any(|(_, member_text)| member_text.contains('\n'))
        {
            return self.infer_object_literal_type_text_at(initializer, self.indent_level);
        }

        let printed = self.print_type_id(type_id);
        let mut lines: Vec<String> = printed.lines().map(str::to_string).collect();
        if lines.len() < 2 {
            return Some(printed);
        }

        if only_numeric_like {
            if has_non_emittable_computed_members {
                for line in &mut lines {
                    let trimmed = line.trim_start();
                    if trimmed.starts_with("[x: string]:") {
                        *line = line.replacen("[x: string]:", "[x: number]:", 1);
                    } else if trimmed.starts_with("readonly [x: string]:") {
                        *line = line.replacen("readonly [x: string]:", "readonly [x: number]:", 1);
                    }
                }
                lines.retain(|line| {
                    let trimmed = line.trim_start();
                    if Self::is_numeric_like_object_property_line(trimmed)
                        && !Self::object_literal_line_matches_any_name(
                            trimmed,
                            &concrete_member_names,
                        )
                    {
                        return false;
                    }
                    true
                });
                let has_number_index = lines
                    .iter()
                    .any(|line| line.trim_start().starts_with("[x: number]:"));
                if !has_number_index
                    && let Some(member_text) = synthetic_number_index_member.as_deref()
                {
                    let member_indent = "    ".repeat((self.indent_level + 1) as usize);
                    lines.insert(1, format!("{member_indent}{member_text};"));
                }
            } else {
                lines.retain(|line| !line.trim_start().starts_with("[x: string]:"));
            }
        }

        let indent = "    ".repeat((self.indent_level + 1) as usize);
        for (name_text, member_text) in overridden_members {
            let replacement = format!("{indent}{member_text};");
            if let Some(existing_idx) = lines.iter().position(|line| {
                Self::object_literal_property_line_matches(line, &name_text, &replacement)
            }) {
                lines[existing_idx] = replacement;
            } else {
                let insert_at = lines.len().saturating_sub(1);
                lines.insert(insert_at, replacement);
            }
        }

        let insert_at = lines.len().saturating_sub(1);
        let mut actual_insertions = 0usize;
        for (name_text, member_text) in computed_members {
            let line = format!("{indent}{member_text};");
            if let Some(existing_idx) = lines.iter().position(|existing| {
                Self::object_literal_property_line_matches(existing, &name_text, &line)
            }) {
                lines[existing_idx] = line;
            } else {
                let line_trimmed = line.trim();
                if !lines.iter().any(|existing| existing.trim() == line_trimmed) {
                    lines.insert(insert_at + actual_insertions, line);
                    actual_insertions += 1;
                }
            }
        }

        Some(lines.join("\n"))
    }

    pub(in crate::declaration_emitter) fn object_literal_property_line_matches(
        existing: &str,
        name_text: &str,
        replacement: &str,
    ) -> bool {
        let trimmed = existing.trim();
        if trimmed == replacement.trim() {
            return true;
        }

        for prefix in Self::object_literal_property_name_prefixes(name_text) {
            if trimmed.starts_with(&prefix) || trimmed.starts_with(&format!("readonly {prefix}")) {
                return true;
            }
        }

        false
    }

    pub(in crate::declaration_emitter) fn object_literal_line_matches_any_name(
        existing: &str,
        names: &[String],
    ) -> bool {
        names.iter().any(|name| {
            Self::object_literal_property_name_prefixes(name)
                .into_iter()
                .any(|prefix| {
                    existing.starts_with(&prefix)
                        || existing.starts_with(&format!("readonly {prefix}"))
                })
        })
    }

    pub(in crate::declaration_emitter) fn object_literal_property_name_prefixes(
        name_text: &str,
    ) -> Vec<String> {
        let mut prefixes = vec![format!("{name_text}:")];

        if let Some(unquoted) = name_text
            .strip_prefix('"')
            .and_then(|name| name.strip_suffix('"'))
            .or_else(|| {
                name_text
                    .strip_prefix('\'')
                    .and_then(|name| name.strip_suffix('\''))
            })
        {
            prefixes.push(format!("\"{unquoted}\":"));
            prefixes.push(format!("'{unquoted}':"));
        }

        if let Some(negative_numeric) = name_text
            .strip_prefix("[-")
            .and_then(|name| name.strip_suffix(']'))
        {
            prefixes.push(format!("\"-{negative_numeric}\":"));
            prefixes.push(format!("'-{negative_numeric}':"));
            prefixes.push(format!("-{negative_numeric}:"));
        }

        prefixes
    }

    pub(in crate::declaration_emitter) fn object_literal_member_needs_syntax_override(
        &self,
        member_idx: NodeIndex,
    ) -> bool {
        let Some(member_node) = self.arena.get(member_idx) else {
            return false;
        };
        let Some(name_idx) = self.object_literal_member_name_idx(member_node) else {
            return false;
        };
        if self
            .arena
            .get(name_idx)
            .is_some_and(|name_node| name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
        {
            return true;
        }

        let Some(initializer) = self.object_literal_member_initializer(member_node) else {
            return false;
        };
        if self
            .arena
            .get(initializer)
            .is_some_and(|node| node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
            && self.object_literal_prefers_syntax_type_text(initializer)
        {
            return true;
        }
        if self.explicit_asserted_type_text(initializer).is_some() {
            return true;
        }
        if self
            .preferred_expression_type_text(initializer)
            .is_some_and(|text| {
                !text.is_empty()
                    && text != "any"
                    && (text.contains("import(\"") || text.starts_with("typeof "))
            })
        {
            return true;
        }
        let type_id = self.get_node_type_or_names(&[initializer]);
        self.typeof_prefix_for_value_entity(initializer, true, type_id)
            .is_some()
            || self.enum_member_widened_type_text(initializer).is_some()
    }

    pub(in crate::declaration_emitter) fn object_literal_member_name_idx(
        &self,
        member_node: &Node,
    ) -> Option<NodeIndex> {
        if let Some(data) = self.arena.get_property_assignment(member_node) {
            return Some(data.name);
        }
        if let Some(data) = self.arena.get_shorthand_property(member_node) {
            return Some(data.name);
        }
        if let Some(data) = self.arena.get_accessor(member_node) {
            return Some(data.name);
        }
        self.arena
            .get_method_decl(member_node)
            .map(|data| data.name)
    }

    pub(in crate::declaration_emitter) fn object_literal_member_initializer(
        &self,
        member_node: &Node,
    ) -> Option<NodeIndex> {
        if let Some(data) = self.arena.get_property_assignment(member_node) {
            return Some(data.initializer);
        }
        // Shorthand `{ foo }` has no separate initializer node; the value
        // reference IS the name identifier. `{ foo = expr }` (CoverInitializedName)
        // is the only shape where `object_assignment_initializer` is non-`NONE`.
        self.arena.get_shorthand_property(member_node).map(|data| {
            if data.object_assignment_initializer == NodeIndex::NONE {
                data.name
            } else {
                data.object_assignment_initializer
            }
        })
    }

    pub(in crate::declaration_emitter) fn is_numeric_property_name_text(name: &str) -> bool {
        name.parse::<f64>().is_ok()
            || (name.starts_with("[-")
                && name.ends_with(']')
                && name[2..name.len().saturating_sub(1)].parse::<f64>().is_ok())
    }

    pub(in crate::declaration_emitter) fn is_numeric_like_object_property_line(line: &str) -> bool {
        let Some((name, _)) = line.split_once(':') else {
            return false;
        };
        let trimmed = name.trim().trim_start_matches("readonly ").trim();
        let normalized = trimmed
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| {
                trimmed
                    .strip_prefix('\'')
                    .and_then(|s| s.strip_suffix('\''))
            })
            .unwrap_or(trimmed);
        normalized.parse::<f64>().is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::DeclarationEmitter;

    #[test]
    fn simultaneous_word_replacement_does_not_rewrite_inserted_import_paths() {
        let rewritten = DeclarationEmitter::replace_whole_words_in_text(
            "A | B",
            &[
                ("A".to_string(), "import(\"./B\").A".to_string()),
                ("B".to_string(), "import(\"./C\").B".to_string()),
            ],
        );

        assert_eq!(rewritten, "import(\"./B\").A | import(\"./C\").B");
    }

    #[test]
    fn simultaneous_word_replacement_does_not_chain_type_parameter_substitutions() {
        let rewritten = DeclarationEmitter::replace_whole_words_in_text(
            "T | U",
            &[
                ("T".to_string(), "Promise<U>".to_string()),
                ("U".to_string(), "string".to_string()),
            ],
        );

        assert_eq!(rewritten, "Promise<U> | string");
    }
}
