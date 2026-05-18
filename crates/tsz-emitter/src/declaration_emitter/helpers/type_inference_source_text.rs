//! Shared source and AST text helpers for declaration type inference.

use super::super::DeclarationEmitter;
use std::sync::Arc;
use tsz_binder::{BinderState, SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::NodeList;
use tsz_parser::parser::node::{Node, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(crate) fn identifier_text_from_arena(
        &self,
        arena: &NodeArena,
        idx: NodeIndex,
    ) -> Option<String> {
        let node = arena.get(idx)?;
        arena
            .get_identifier(node)
            .map(|ident| ident.escaped_text.clone())
    }

    pub(in crate::declaration_emitter) fn property_name_text_from_arena(
        &self,
        arena: &NodeArena,
        idx: NodeIndex,
    ) -> Option<String> {
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

    pub(in crate::declaration_emitter) fn is_simple_identifier_text(text: &str) -> bool {
        let mut chars = text.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        (first == '_' || first == '$' || first.is_ascii_alphabetic())
            && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
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
                // The parser captures `LiteralType`/`UnionType`/
                // `IntersectionType` end positions with `token_end()`, which
                // reflects the *next* scanned token rather than the type
                // itself.  Inside a type-argument list, that next token is
                // typically `>` or `,` — so a slice of the type-arg's node
                // span pulls those trailing characters into the text we
                // splice into d.ts emit.  Strip them here, since this
                // helper is the only call site that observes the overshoot.
                //
                // Be careful: a nested type-argument list like
                // `F5<C.A<C.B>>` produces an outer arg whose slice ends
                // with the inner list's *own* closing `>`.  Trimming
                // unconditionally would eat that `>` and corrupt the
                // emitted text into `C.A<C.B`.  Only trim trailing `>`s
                // that are unbalanced — i.e. when the slice has more
                // `>`s than `<`s.  Trailing `,`/whitespace can always be
                // dropped (they're never part of the type's own syntax).
                Self::strip_type_argument_overshoot(&mut text);
                if self.first_type_argument_needs_parentheses(arg, index == 0) {
                    text = format!("({text})");
                }
                Some(text)
            })
            .collect()
    }

    /// Trim trailing `>` (and `,` / whitespace) that the parser's
    /// `token_end()`-based span captured beyond a type's own syntax,
    /// while preserving balanced `<…>` pairs that belong to a nested
    /// type-argument list.  See call site for the parser quirk this
    /// works around.
    #[cfg(test)]
    pub(crate) fn strip_type_argument_overshoot_for_test(text: &mut String) {
        Self::strip_type_argument_overshoot(text);
    }

    pub(in crate::declaration_emitter) fn strip_type_argument_overshoot(text: &mut String) {
        loop {
            let Some(&last) = text.as_bytes().last() else {
                return;
            };
            if last == b',' || last.is_ascii_whitespace() {
                text.pop();
                continue;
            }
            if last != b'>' {
                return;
            }
            // Count `<` and `>` not inside string/template literals.
            // If `>`s outnumber `<`s, the trailing `>` is overshoot.
            let bytes = text.as_bytes();
            let mut lt = 0i32;
            let mut gt = 0i32;
            let mut i = 0usize;
            while i < bytes.len() {
                let b = bytes[i];
                match b {
                    b'"' | b'\'' | b'`' => {
                        let quote = b;
                        i += 1;
                        while i < bytes.len() && bytes[i] != quote {
                            if bytes[i] == b'\\' && i + 1 < bytes.len() {
                                i += 2;
                            } else {
                                i += 1;
                            }
                        }
                        if i < bytes.len() {
                            i += 1;
                        }
                    }
                    b'<' => {
                        lt += 1;
                        i += 1;
                    }
                    b'>' => {
                        gt += 1;
                        i += 1;
                    }
                    _ => i += 1,
                }
            }
            if gt > lt {
                text.pop();
            } else {
                return;
            }
        }
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
                let reference_text = format!("{lhs}.{rhs}");
                Some(self.current_namespace_relative_type_reference_text(&reference_text))
            }
            _ => None,
        }
    }

    fn current_namespace_relative_type_reference_text(&self, reference_text: &str) -> String {
        let Some(enclosing_ns) = self.enclosing_namespace_symbol else {
            return reference_text.to_string();
        };
        let Some(binder) = self.binder else {
            return reference_text.to_string();
        };

        let mut namespace_parts = Vec::new();
        let mut current = enclosing_ns;
        while current != SymbolId::NONE {
            let Some(symbol) = binder.symbols.get(current) else {
                break;
            };
            if !symbol.escaped_name.starts_with('"')
                && !symbol.escaped_name.starts_with("__")
                && symbol
                    .escaped_name
                    .chars()
                    .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
            {
                namespace_parts.push(symbol.escaped_name.as_str());
            }
            current = symbol.parent;
        }
        namespace_parts.reverse();
        if namespace_parts.is_empty() {
            return reference_text.to_string();
        }

        let reference_parts: Vec<&str> = reference_text.split('.').collect();
        if reference_parts.len() <= namespace_parts.len()
            || !reference_parts
                .iter()
                .zip(namespace_parts.iter())
                .all(|(left, right)| left == right)
        {
            return reference_text.to_string();
        }

        reference_parts[namespace_parts.len()..].join(".")
    }

    pub(in crate::declaration_emitter) fn identifier_constructor_reference_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let ident = self.get_identifier_text(expr_idx)?;
        let binder = self.binder?;
        let no_libs: &[Arc<BinderState>] = &[];
        let sym_id = binder
            .get_node_symbol(expr_idx)
            .filter(|&candidate| self.symbol_is_constructor_value(candidate))
            .or_else(|| {
                binder.resolve_name_with_filter(
                    &ident,
                    self.arena,
                    expr_idx,
                    no_libs,
                    |candidate| self.symbol_is_constructor_value(candidate),
                )
            })
            .or_else(|| self.resolve_identifier_symbol(expr_idx, &ident))?;
        let symbol = binder.symbols.get(sym_id)?;

        if self.constructor_symbol_requires_global_this(sym_id, &ident, expr_idx) {
            return Some(format!("globalThis.{ident}"));
        }

        for decl_idx in symbol.declarations.iter().copied() {
            let decl_node = self.arena.get(decl_idx)?;
            if decl_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                if self.inside_non_ambient_namespace
                    && let Some(import_type) =
                        self.require_property_initializer_import_type(decl_node)
                {
                    return Some(import_type);
                }
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

    fn symbol_is_constructor_value(&self, sym_id: SymbolId) -> bool {
        self.binder
            .and_then(|binder| binder.symbols.get(sym_id))
            .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::VALUE | symbol_flags::ALIAS))
    }

    fn constructor_symbol_requires_global_this(
        &self,
        sym_id: SymbolId,
        name: &str,
        expr_idx: NodeIndex,
    ) -> bool {
        if !Self::is_unquoted_property_name(name)
            || self.resolve_symbol_module_path(sym_id).is_some()
        {
            return false;
        }
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(symbol) = binder.symbols.get(sym_id) else {
            return false;
        };
        if symbol.parent != SymbolId::NONE || !symbol.has_any_flags(symbol_flags::CLASS) {
            return false;
        }
        let Some(func) = self.enclosing_function_for_node(expr_idx) else {
            return false;
        };
        let Some(ref type_params) = func.type_parameters else {
            return false;
        };
        self.collect_type_param_names(type_params)
            .iter()
            .any(|type_param| type_param == name)
    }

    fn require_property_initializer_import_type(&self, decl_node: &Node) -> Option<String> {
        let (module, export_name) = self.require_property_initializer_parts(decl_node)?;
        Some(format!("import(\"{module}\").{export_name}"))
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

    pub(in crate::declaration_emitter) fn resolve_lexical_identifier_symbol(
        &self,
        expr_idx: NodeIndex,
        ident: &str,
    ) -> Option<SymbolId> {
        let binder = self.binder?;
        let no_libs: &[Arc<BinderState>] = &[];
        binder
            .resolve_name_with_filter(ident, self.arena, expr_idx, no_libs, |_| true)
            .or_else(|| binder.file_locals.get(ident))
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
}
