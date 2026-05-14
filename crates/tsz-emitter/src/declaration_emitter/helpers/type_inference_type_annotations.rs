//! Source type-annotation lookup helpers for declaration inference.
//!
//! These routines recover source-backed type annotation text from local and
//! foreign arenas, walk symbol declaration arenas, and resolve declared
//! property/member annotations used by declaration emit heuristics.

use super::super::DeclarationEmitter;
use tsz_binder::SymbolId;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn type_annotation_text_from_arena_node(
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
            let raw_string_intrinsic_type_text = self
                .local_type_annotation_text(type_annotation)
                .filter(|raw_type_text| {
                    Self::type_text_starts_with_string_intrinsic(raw_type_text)
                });
            match printed {
                Some(printed) if printed != "any" && raw_string_intrinsic_type_text.is_some() => {
                    raw_string_intrinsic_type_text
                        .expect("string intrinsic type text was checked above")
                }
                Some(printed)
                    if printed != "any"
                        && (!printed.contains("any") || type_text.contains("any"))
                        && printed.contains("typeof ")
                        && !type_text.contains("typeof ") =>
                {
                    printed.replace("typeof ", "")
                }
                Some(printed)
                    if printed != "any"
                        && (!printed.contains("any") || type_text.contains("any")) =>
                {
                    printed
                }
                _ => type_text,
            }
        } else {
            let rewritten = self.qualify_foreign_imported_names_in_text(source_arena, &type_text);
            let expands_mapped_object =
                Self::contains_portable_mapped_object_text(rewritten.as_str());
            let rewritten = self
                .expand_portable_intersection_type_text(source_arena, &rewritten)
                .unwrap_or(rewritten);
            match printed {
                Some(ref printed)
                    if printed != "any"
                        && !printed.contains("any")
                        && !expands_mapped_object
                        && (!Self::type_text_contains_import_type(&rewritten)
                            || Self::type_text_contains_import_type(printed)) =>
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

    pub(in crate::declaration_emitter) fn declared_type_annotation_text_for_symbol(
        &self,
        sym_id: SymbolId,
    ) -> Option<String> {
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

    pub(in crate::declaration_emitter) fn annotation_bearing_declaration_from_arena(
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

    pub(in crate::declaration_emitter) fn emit_type_node_text_from_arena(
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

    pub(in crate::declaration_emitter) fn explicit_asserted_type_node_from_arena(
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

    pub(in crate::declaration_emitter) fn declaration_type_symbol_from_type_node(
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
                    binder.get_node_symbol(type_idx).or_else(|| {
                        self.identifier_text_from_arena(arena, type_idx)
                            .and_then(|name| binder.symbols.find_by_name(&name))
                    })
                }
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn property_access_declared_type_annotation_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
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

    pub(in crate::declaration_emitter) fn type_member_declared_type_annotation_text(
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

            if let Some(interface) = source_arena.get_interface(decl_node)
                && let Some(heritage_clauses) = interface.heritage_clauses.as_ref()
            {
                for &heritage_idx in &heritage_clauses.nodes {
                    let Some(heritage_node) = source_arena.get(heritage_idx) else {
                        continue;
                    };
                    let Some(heritage) = source_arena.get_heritage(heritage_node) else {
                        continue;
                    };
                    for &base_idx in &heritage.types.nodes {
                        let Some(base_node) = source_arena.get(base_idx) else {
                            continue;
                        };
                        let base_expr = source_arena
                            .get_expr_type_args(base_node)
                            .map_or(base_idx, |expr| expr.expression);
                        let Some(base_sym_id) =
                            self.declaration_type_symbol_from_type_node(source_arena, base_expr)
                        else {
                            continue;
                        };
                        if let Some(type_text) =
                            self.type_member_declared_type_annotation_text(base_sym_id, member_name)
                        {
                            return Some(type_text);
                        }
                    }
                }
            }

            None
        })
    }

    pub(crate) fn with_symbol_declarations<T>(
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
}
