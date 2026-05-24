use rustc_hash::FxHashSet;
use tsz_binder::{BinderState, SymbolId};
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

use super::DeclarationEmitter;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn array_binding_element_type(
        &self,
        tuple_elements: Option<&[tsz_solver::types::TupleElement]>,
        tuple_index: usize,
        array_element_type: Option<tsz_solver::types::TypeId>,
    ) -> Option<tsz_solver::types::TypeId> {
        if let Some(tuple_elements) = tuple_elements
            && let Some(tuple_element) = tuple_elements.get(tuple_index)
        {
            let mut type_id = if tuple_element.rest {
                self.type_interner.and_then(|interner| {
                    tsz_solver::type_queries::get_array_element_type(
                        interner,
                        tuple_element.type_id,
                    )
                    .or(Some(tuple_element.type_id))
                })?
            } else {
                tuple_element.type_id
            };
            if tuple_element.optional
                && let Some(interner) = self.type_interner
            {
                type_id = interner.union(vec![type_id, tsz_solver::types::TypeId::UNDEFINED]);
            }
            return Some(type_id);
        }

        if array_element_type == Some(tsz_solver::types::TypeId::NEVER) {
            return Some(tsz_solver::types::TypeId::UNDEFINED);
        }

        array_element_type
    }

    pub(in crate::declaration_emitter) fn array_rest_binding_type(
        &self,
        source_type: Option<tsz_solver::types::TypeId>,
        tuple_elements: Option<&[tsz_solver::types::TupleElement]>,
        tuple_index: usize,
        array_element_type: Option<tsz_solver::types::TypeId>,
    ) -> Option<tsz_solver::types::TypeId> {
        let interner = self.type_interner?;

        if tuple_index == 0
            && let Some(source_type) = source_type
            && let Some(union_type) =
                tsz_solver::type_queries::get_tuple_element_type_union(interner, source_type)
        {
            return Some(interner.array(union_type));
        }

        if let Some(tuple_elements) = tuple_elements {
            let remaining = tuple_elements
                .get(tuple_index..)
                .map_or_else(Vec::new, ToOwned::to_owned);
            return Some(interner.tuple(remaining));
        }

        array_element_type.map(|element_type| interner.array(element_type))
    }

    pub(in crate::declaration_emitter) fn object_binding_element_type(
        &self,
        source_type: Option<tsz_solver::types::TypeId>,
        element: &tsz_parser::parser::node::BindingElementData,
    ) -> Option<tsz_solver::types::TypeId> {
        let interner = self.type_interner?;
        let source_type = tsz_solver::type_queries::unwrap_readonly(interner, source_type?);
        let property_name_idx = if element.property_name.is_some() {
            element.property_name
        } else {
            element.name
        };
        let property_name = self.destructuring_property_lookup_text(property_name_idx)?;
        let property = tsz_solver::type_queries::find_property_in_type_by_str(
            interner,
            source_type,
            &property_name,
        )?;
        if property.optional {
            Some(interner.union(vec![property.type_id, tsz_solver::types::TypeId::UNDEFINED]))
        } else {
            Some(property.type_id)
        }
    }

    pub(in crate::declaration_emitter) fn collect_typed_bindings_recursive(
        &self,
        node_idx: NodeIndex,
        source_type: Option<tsz_solver::types::TypeId>,
        bindings: &mut Vec<(NodeIndex, Option<tsz_solver::types::TypeId>)>,
    ) {
        let Some(node) = self.arena.get(node_idx) else {
            return;
        };

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                let type_id = source_type
                    .filter(|type_id| *type_id != tsz_solver::types::TypeId::ANY)
                    .or_else(|| self.get_symbol_cached_type(node_idx))
                    .or_else(|| self.get_node_type(node_idx))
                    .or_else(|| self.get_type_via_symbol(node_idx));
                bindings.push((node_idx, type_id));
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                if let Some(element) = self.arena.get_binding_element(node) {
                    let effective_type = source_type
                        .filter(|type_id| *type_id != tsz_solver::types::TypeId::ANY)
                        .or_else(|| self.get_symbol_cached_type(node_idx))
                        .or_else(|| self.get_symbol_cached_type(element.name))
                        .or_else(|| {
                            if element.initializer.is_some() {
                                self.get_node_type(element.initializer)
                            } else {
                                None
                            }
                        })
                        .or_else(|| {
                            self.get_node_type_or_names(&[
                                node_idx,
                                element.name,
                                element.initializer,
                            ])
                        });
                    self.collect_typed_bindings_recursive(element.name, effective_type, bindings);
                }
            }
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                let tuple_elements = self.type_interner.and_then(|interner| {
                    source_type.and_then(|type_id| {
                        tsz_solver::type_queries::get_tuple_elements(interner, type_id)
                    })
                });
                let array_element_type = self.type_interner.and_then(|interner| {
                    source_type.and_then(|type_id| {
                        tsz_solver::type_queries::get_array_element_type(interner, type_id).or_else(
                            || {
                                tsz_solver::type_queries::get_tuple_element_type_union(
                                    interner, type_id,
                                )
                            },
                        )
                    })
                });

                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    let mut tuple_index = 0usize;
                    for &element_idx in &pattern.elements.nodes {
                        let Some(element_node) = self.arena.get(element_idx) else {
                            continue;
                        };
                        if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                            tuple_index += 1;
                            continue;
                        }
                        if element_node.kind != syntax_kind_ext::BINDING_ELEMENT {
                            continue;
                        }
                        let Some(element) = self.arena.get_binding_element(element_node) else {
                            continue;
                        };
                        let element_type = if element.dot_dot_dot_token {
                            self.array_rest_binding_type(
                                source_type,
                                tuple_elements.as_deref(),
                                tuple_index,
                                array_element_type,
                            )
                        } else {
                            self.array_binding_element_type(
                                tuple_elements.as_deref(),
                                tuple_index,
                                array_element_type,
                            )
                        };
                        self.collect_typed_bindings_recursive(element_idx, element_type, bindings);
                        if !element.dot_dot_dot_token {
                            tuple_index += 1;
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    for &element_idx in &pattern.elements.nodes {
                        let Some(element_node) = self.arena.get(element_idx) else {
                            continue;
                        };
                        if element_node.kind != syntax_kind_ext::BINDING_ELEMENT {
                            continue;
                        }
                        let Some(element) = self.arena.get_binding_element(element_node) else {
                            continue;
                        };
                        let element_type = if element.dot_dot_dot_token {
                            source_type
                        } else {
                            self.object_binding_element_type(source_type, element)
                        };
                        self.collect_typed_bindings_recursive(element_idx, element_type, bindings);
                    }
                }
            }
            _ => {}
        }
    }

    pub(in crate::declaration_emitter) fn collect_flattened_binding_entries(
        &self,
        pattern_idx: NodeIndex,
        source_type: Option<tsz_solver::types::TypeId>,
    ) -> Vec<(NodeIndex, Option<tsz_solver::types::TypeId>)> {
        let mut bindings = Vec::new();
        self.collect_typed_bindings_recursive(pattern_idx, source_type, &mut bindings);
        bindings
    }

    pub(in crate::declaration_emitter) fn record_js_elided_bare_require_binding_names(
        &mut self,
        pattern_idx: NodeIndex,
    ) {
        let bindings = self.collect_flattened_binding_entries(pattern_idx, None);
        for (ident_idx, _) in bindings {
            if let Some(name) = self.get_identifier_text(ident_idx) {
                self.js_elided_bare_require_binding_names.insert(name);
            }
        }
    }

    pub(in crate::declaration_emitter) fn initializer_references_js_elided_bare_require_binding(
        &self,
        initializer: NodeIndex,
    ) -> bool {
        if self.js_elided_bare_require_binding_names.is_empty() || initializer.is_none() {
            return false;
        }
        let mut seen = FxHashSet::default();
        self.node_references_js_elided_bare_require_binding(initializer, &mut seen)
    }

    pub(in crate::declaration_emitter) fn node_references_js_elided_bare_require_binding(
        &self,
        node_idx: NodeIndex,
        seen: &mut FxHashSet<NodeIndex>,
    ) -> bool {
        if node_idx.is_none() || !seen.insert(node_idx) {
            return false;
        }
        if self
            .get_identifier_text(node_idx)
            .is_some_and(|name| self.js_elided_bare_require_binding_names.contains(&name))
        {
            return true;
        }
        self.arena
            .get_children(node_idx)
            .into_iter()
            .any(|child_idx| self.node_references_js_elided_bare_require_binding(child_idx, seen))
    }

    pub(in crate::declaration_emitter) fn collect_flattened_binding_type_texts_from_annotation(
        &mut self,
        pattern_idx: NodeIndex,
        type_annotation: NodeIndex,
    ) -> Vec<(NodeIndex, String)> {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return Vec::new();
        };
        if pattern_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN {
            return Vec::new();
        }
        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return Vec::new();
        };
        let pattern_elements = pattern.elements.nodes.clone();

        let Some(type_node) = self.arena.get(type_annotation) else {
            return Vec::new();
        };
        let Some(tuple) = self.arena.get_tuple_type(type_node) else {
            return Vec::new();
        };
        let tuple_elements = tuple.elements.nodes.clone();

        let mut type_texts = Vec::new();
        let mut tuple_index = 0usize;
        for element_idx in pattern_elements {
            let Some(element_node) = self.arena.get(element_idx) else {
                continue;
            };
            if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                tuple_index += 1;
                continue;
            }
            if element_node.kind != syntax_kind_ext::BINDING_ELEMENT {
                continue;
            }
            let Some(element) = self.arena.get_binding_element(element_node) else {
                continue;
            };
            let Some(name_node) = self.arena.get(element.name) else {
                continue;
            };
            if name_node.kind == SyntaxKind::Identifier as u16
                && let Some(&tuple_element_idx) = tuple_elements.get(tuple_index)
                && let Some(type_text) = self.type_node_text(tuple_element_idx)
            {
                type_texts.push((element.name, type_text));
            }
            if !element.dot_dot_dot_token {
                tuple_index += 1;
            }
        }
        type_texts
    }

    pub(in crate::declaration_emitter) fn emit_flattened_binding_type_annotation(
        &mut self,
        ident_idx: NodeIndex,
        type_id: Option<tsz_solver::types::TypeId>,
        widened_literal_kind: Option<&'static str>,
    ) {
        let type_id = type_id
            .or_else(|| self.get_symbol_cached_type(ident_idx))
            .or_else(|| self.get_node_type(ident_idx))
            .or_else(|| self.get_type_via_symbol(ident_idx));
        self.write(": ");
        if let Some(type_id) = type_id {
            if let Some(kind) = widened_literal_kind
                && self.literal_type_can_widen_to_primitive_kind(type_id, kind)
            {
                self.write(kind);
            } else {
                self.write(&self.print_type_id(type_id));
            }
        } else {
            self.write("any");
        }
    }

    /// Emits flattened variable declarations for destructuring patterns.
    ///
    /// In .d.ts files, destructuring like `export const { a, b } = obj;`
    /// must be flattened into individual declarations:
    /// `export declare const a: Type;`
    /// `export declare const b: Type;`
    pub(in crate::declaration_emitter) fn emit_flattened_variable_declaration(
        &mut self,
        decl_idx: NodeIndex,
        keyword: &str,
        is_exported: bool,
    ) {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return;
        };
        let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
            return;
        };
        let bindings = self.collect_flattened_binding_entries(
            decl.name,
            self.preferred_binding_source_type(
                decl.type_annotation,
                decl.initializer,
                &[decl_idx, decl.name, decl.initializer],
            ),
        );
        let widened_literal_kinds = self.collect_flattened_array_binding_literal_widening_kinds(
            decl.name,
            decl.type_annotation,
            decl.initializer,
        );
        let annotation_type_texts = self
            .collect_flattened_binding_type_texts_from_annotation(decl.name, decl.type_annotation);
        if bindings.is_empty() {
            return;
        }

        self.write_indent();
        if is_exported && (!self.inside_declare_namespace || self.ambient_module_has_scope_marker) {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write(keyword);
        self.write(" ");

        for (index, (ident_idx, type_id)) in bindings.into_iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            let has_leading_jsdoc = self.arena.get(ident_idx).is_some_and(|node| {
                !self
                    .leading_jsdoc_comment_chain_for_pos(node.pos)
                    .is_empty()
            });
            if has_leading_jsdoc && let Some(node) = self.arena.get(ident_idx) {
                self.write_line();
                self.emit_leading_jsdoc_comments(node.pos);
            }
            self.emit_node(ident_idx);
            if let Some(type_text) = annotation_type_texts
                .iter()
                .find_map(|(idx, text)| (*idx == ident_idx).then(|| text.clone()))
            {
                self.write(": ");
                self.write(&type_text);
            } else {
                self.emit_flattened_binding_type_annotation(
                    ident_idx,
                    type_id,
                    widened_literal_kinds
                        .iter()
                        .find_map(|(idx, kind)| (*idx == ident_idx).then_some(*kind)),
                );
            }
        }
        self.write(";");
        self.write_line();
    }

    pub(in crate::declaration_emitter) fn collect_flattened_array_binding_literal_widening_kinds(
        &self,
        pattern_idx: NodeIndex,
        type_annotation: NodeIndex,
        initializer: NodeIndex,
    ) -> Vec<(NodeIndex, &'static str)> {
        if type_annotation.is_some() {
            return Vec::new();
        }
        if self
            .arena
            .get(pattern_idx)
            .is_none_or(|node| node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN)
        {
            return Vec::new();
        }
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return Vec::new();
        };
        let Some(pattern) = self.arena.get_binding_pattern(pattern_node) else {
            return Vec::new();
        };
        let Some(initializer) = self.skip_parenthesized_expression(initializer) else {
            return Vec::new();
        };
        let Some(initializer_node) = self.arena.get(initializer) else {
            return Vec::new();
        };
        if initializer_node.kind != syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            return Vec::new();
        }
        let Some(literal) = self.arena.get_literal_expr(initializer_node) else {
            return Vec::new();
        };

        let mut kinds = Vec::new();
        let mut initializer_index = 0usize;
        let initializer_elements = literal.elements.nodes.clone();
        for &element_idx in &pattern.elements.nodes {
            let Some(element_node) = self.arena.get(element_idx) else {
                continue;
            };
            if element_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                initializer_index += 1;
                continue;
            }
            if element_node.kind != syntax_kind_ext::BINDING_ELEMENT {
                continue;
            }
            let Some(element) = self.arena.get_binding_element(element_node) else {
                continue;
            };
            if element.dot_dot_dot_token {
                return Vec::new();
            }
            if self
                .arena
                .get(element.name)
                .is_some_and(|node| node.kind == SyntaxKind::Identifier as u16)
                && let Some(&initializer_element) = initializer_elements.get(initializer_index)
                && let Some(kind) = self.literal_initializer_primitive_kind(initializer_element)
            {
                kinds.push((element.name, kind));
            }
            initializer_index += 1;
        }
        kinds
    }

    pub(in crate::declaration_emitter) fn literal_initializer_primitive_kind(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<&'static str> {
        let node = self.arena.get(expr_idx)?;
        match node.kind {
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => self
                .arena
                .get_parenthesized(node)
                .and_then(|paren| self.literal_initializer_primitive_kind(paren.expression)),
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                Some("string")
            }
            k if k == SyntaxKind::NumericLiteral as u16 => Some("number"),
            k if k == SyntaxKind::BigIntLiteral as u16 => Some("bigint"),
            k if k == SyntaxKind::TrueKeyword as u16 || k == SyntaxKind::FalseKeyword as u16 => {
                Some("boolean")
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.arena.get_unary_expr(node)?;
                let operand = self.arena.get(unary.operand)?;
                match (unary.operator, operand.kind) {
                    (op, k)
                        if (op == SyntaxKind::PlusToken as u16
                            || op == SyntaxKind::MinusToken as u16)
                            && k == SyntaxKind::NumericLiteral as u16 =>
                    {
                        Some("number")
                    }
                    (op, k)
                        if op == SyntaxKind::MinusToken as u16
                            && k == SyntaxKind::BigIntLiteral as u16 =>
                    {
                        Some("bigint")
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    pub(in crate::declaration_emitter) fn literal_type_can_widen_to_primitive_kind(
        &self,
        type_id: tsz_solver::types::TypeId,
        primitive_kind: &str,
    ) -> bool {
        let Some(interner) = self.type_interner else {
            return false;
        };
        if let Some(lit) = tsz_solver::visitor::literal_value(interner, type_id) {
            return Self::literal_primitive_kind_text(&lit) == Some(primitive_kind);
        }
        let Some(union_id) = tsz_solver::visitor::union_list_id(interner, type_id) else {
            return false;
        };
        let members = interner.type_list(union_id);
        !members.is_empty()
            && members.iter().all(|&member| {
                tsz_solver::visitor::literal_value(interner, member)
                    .and_then(|lit| Self::literal_primitive_kind_text(&lit))
                    == Some(primitive_kind)
            })
    }

    pub(in crate::declaration_emitter) fn emit_parameter_property_modifiers(
        &mut self,
        modifiers: &Option<NodeList>,
    ) -> bool {
        let mut is_private = false;
        if let Some(modifiers) = modifiers {
            for &mod_idx in &modifiers.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    match mod_node.kind {
                        k if k == SyntaxKind::PrivateKeyword as u16 => {
                            self.write("private ");
                            is_private = true;
                        }
                        k if k == SyntaxKind::ProtectedKeyword as u16 => {
                            self.write("protected ");
                        }
                        k if k == SyntaxKind::ReadonlyKeyword as u16 => {
                            self.write("readonly ");
                        }
                        k if k == SyntaxKind::OverrideKeyword as u16 => {
                            // tsc strips `override` in .d.ts output.
                        }
                        _ => {}
                    }
                }
            }
        }
        is_private
    }

    /// Check if an initializer is a simple reference (identifier or qualified name)
    /// to a local import-equals alias (e.g. `import b = a.foo`).
    /// Returns the text to use after `typeof` if so (e.g. `"b"`).
    ///
    /// tsc emits `typeof <alias>` for variables initialized with an import-equals
    /// alias target rather than expanding the resolved type. This preserves the
    /// declarative reference in the .d.ts output.
    pub(in crate::declaration_emitter) fn initializer_import_alias_typeof_text(
        &self,
        initializer: NodeIndex,
    ) -> Option<String> {
        let binder = self.binder?;
        let init_node = self.arena.get(initializer)?;

        // Only handle simple identifier references.
        if init_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let ident = self.arena.get_identifier(init_node)?;
        let name = &ident.escaped_text;

        // Resolve the identifier by walking the scope chain from the enclosing scope.
        // The binder's node_symbols map only contains declaration-site mappings, not
        // usage-site references. We need to walk scopes to find the symbol for `b`.
        let scope_id = binder.find_enclosing_scope(self.arena, initializer)?;
        let sym_id = self.resolve_name_in_scope_chain(binder, scope_id, name)?;
        let sym = binder.symbols.get(sym_id)?;

        // Check if this symbol is an alias (import-equals creates ALIAS symbols)
        if sym.flags & tsz_binder::symbol_flags::ALIAS == 0 {
            return None;
        }

        // Verify that at least one declaration is an import-equals declaration
        let has_import_equals_decl = sym.declarations.iter().any(|&decl_idx| {
            self.arena
                .get(decl_idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION)
        });

        if !has_import_equals_decl {
            return None;
        }

        // tsc only emits `typeof alias` when the alias target is a function, class,
        // enum, or module — NOT when it targets a plain variable. For plain variables
        // (e.g. `import b = a.x` where `x` is `var x = 10`), tsc resolves and emits
        // the actual type (e.g. `number`).
        if self.import_alias_targets_plain_variable(binder, sym) {
            return None;
        }

        Some(name.clone())
    }

    pub(in crate::declaration_emitter) fn initializer_references_elided_namespace_require_import(
        &self,
        initializer: NodeIndex,
    ) -> bool {
        if !self.inside_non_ambient_namespace {
            return false;
        }
        let Some(root_ident) = self.expression_root_identifier(initializer) else {
            return false;
        };
        let Some(binder) = self.binder else {
            return false;
        };
        let Some(root_name) = self.get_identifier_text(root_ident) else {
            return false;
        };
        let Some(scope_id) = binder.find_enclosing_scope(self.arena, root_ident) else {
            return false;
        };
        let Some(sym_id) = self.resolve_name_in_scope_chain(binder, scope_id, &root_name) else {
            return false;
        };
        let Some(sym) = binder.symbols.get(sym_id) else {
            return false;
        };
        if sym.flags & tsz_binder::symbol_flags::ALIAS == 0 {
            return false;
        }
        sym.declarations.iter().copied().any(|decl_idx| {
            let Some(decl_node) = self.arena.get(decl_idx) else {
                return false;
            };
            if decl_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                return false;
            }
            let Some(import_decl) = self.arena.get_import_decl(decl_node) else {
                return false;
            };
            if self
                .arena
                .has_modifier(&import_decl.modifiers, SyntaxKind::ExportKeyword)
            {
                return false;
            }
            self.arena
                .get(import_decl.module_specifier)
                .is_some_and(|node| node.kind == SyntaxKind::StringLiteral as u16)
        })
    }

    pub(in crate::declaration_emitter) fn expression_root_identifier(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let node = self.arena.get(expr_idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return Some(expr_idx);
        }
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(node)?;
            return self.expression_root_identifier(access.expression);
        }
        if node.kind == syntax_kind_ext::NEW_EXPRESSION
            || node.kind == syntax_kind_ext::CALL_EXPRESSION
        {
            let call = self.arena.get_call_expr(node)?;
            return self.expression_root_identifier(call.expression);
        }
        None
    }

    /// Check whether an import-equals alias resolves to a plain variable.
    /// Returns `true` when the alias target's symbol has only VARIABLE flags
    /// (not FUNCTION, CLASS, ENUM, or MODULE).
    pub(in crate::declaration_emitter) fn import_alias_targets_plain_variable(
        &self,
        binder: &BinderState,
        alias_sym: &tsz_binder::Symbol,
    ) -> bool {
        use tsz_binder::symbol_flags;

        // Find the import-equals declaration to get the entity name reference.
        let import_decl_idx = alias_sym.declarations.iter().copied().find(|&decl_idx| {
            self.arena
                .get(decl_idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION)
        });
        let import_decl_idx = match import_decl_idx {
            Some(idx) => idx,
            None => return false,
        };
        let import_node = match self.arena.get(import_decl_idx) {
            Some(n) => n,
            None => return false,
        };
        let import_data = match self.arena.get_import_decl(import_node) {
            Some(d) => d,
            None => return false,
        };

        // module_specifier is the entity name (e.g. `a.x` or just `a`).
        // Resolve it to find the target symbol.
        let target_sym_id =
            self.resolve_entity_name_to_symbol(binder, import_data.module_specifier);
        let target_sym_id = match target_sym_id {
            Some(id) => id,
            None => return false,
        };
        let target_sym = match binder.symbols.get(target_sym_id) {
            Some(s) => s,
            None => return false,
        };

        // A "plain variable" has VARIABLE flags but not FUNCTION, CLASS, ENUM, or MODULE.
        let non_variable_value = symbol_flags::FUNCTION
            | symbol_flags::CLASS
            | symbol_flags::REGULAR_ENUM
            | symbol_flags::CONST_ENUM
            | symbol_flags::VALUE_MODULE
            | symbol_flags::NAMESPACE_MODULE;
        target_sym.flags & symbol_flags::VARIABLE != 0 && target_sym.flags & non_variable_value == 0
    }

    /// Resolve a qualified entity name (e.g. `a.x`) to its final symbol by walking
    /// through namespace exports. For a simple identifier, resolve via scope chain.
    pub(in crate::declaration_emitter) fn resolve_entity_name_to_symbol(
        &self,
        binder: &BinderState,
        entity_name: NodeIndex,
    ) -> Option<SymbolId> {
        let node = self.arena.get(entity_name)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            // Simple identifier — resolve via scope chain from the entity name's location.
            let ident = self.arena.get_identifier(node)?;
            let scope_id = binder.find_enclosing_scope(self.arena, entity_name)?;
            self.resolve_name_in_scope_chain(binder, scope_id, &ident.escaped_text)
        } else if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            // Qualified name (e.g. `a.x`) — resolve left side first, then look up right
            // in the left symbol's exports.
            let qn = self.arena.get_qualified_name(node)?;
            let left_sym_id = self.resolve_entity_name_to_symbol(binder, qn.left)?;
            let left_sym = binder.symbols.get(left_sym_id)?;
            let right_node = self.arena.get(qn.right)?;
            let right_ident = self.arena.get_identifier(right_node)?;
            let right_name = &right_ident.escaped_text;

            // Look up in exports table of the left symbol.
            if let Some(exports) = &left_sym.exports
                && let Some(sym_id) = exports.get(right_name)
            {
                return Some(sym_id);
            }
            None
        } else {
            None
        }
    }

    /// Walk the scope chain from `scope_id` upward, looking for a symbol with the given name.
    pub(in crate::declaration_emitter) fn resolve_name_in_scope_chain(
        &self,
        binder: &BinderState,
        start_scope: tsz_binder::scopes::ScopeId,
        name: &str,
    ) -> Option<SymbolId> {
        let mut scope_id = start_scope;
        let mut iterations = 0;
        while scope_id.is_some() {
            iterations += 1;
            if iterations > 100 {
                break;
            }
            let scope = binder.scopes.get(scope_id.0 as usize)?;
            if let Some(sym_id) = scope.table.get(name) {
                return Some(sym_id);
            }
            scope_id = scope.parent;
        }
        None
    }

    pub(in crate::declaration_emitter) fn js_function_body_preferred_return_text_for_declaration(
        &self,
        body_idx: NodeIndex,
        name_idx: NodeIndex,
        params: &NodeList,
    ) -> Option<String> {
        if !self.source_is_js_file || !body_idx.is_some() {
            return None;
        }
        let name = self.get_identifier_text(name_idx)?;
        if self.js_function_body_returns_new_named(body_idx, &name) {
            return Some(name);
        }
        let body_node = self.arena.get(
            self.arena
                .skip_parenthesized_and_assertions_and_comma(body_idx),
        )?;
        if body_node.kind == syntax_kind_ext::JSX_ELEMENT
            || body_node.kind == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
            || body_node.kind == syntax_kind_ext::JSX_FRAGMENT
        {
            return Some("JSX.Element".to_string());
        }
        if self.js_function_body_returns_jsx(body_idx) {
            return Some("JSX.Element".to_string());
        }
        if !self
            .js_function_body_this_property_assignments(body_idx)
            .is_empty()
        {
            return Some("void".to_string());
        }
        if let Some(returned_identifier) = self.function_body_unique_return_identifier(body_idx)
            && let Some(type_text) = self.js_parameter_type_text(params, returned_identifier)
        {
            return Some(type_text);
        }

        self.function_body_single_return_expression(body_idx)
            .and_then(|expr_idx| {
                self.js_constructor_assignment_expression_type_text(expr_idx, params, 0)
            })
            .filter(|type_text| !type_text.is_empty() && type_text != "any")
    }

    pub(in crate::declaration_emitter) fn js_function_body_returns_jsx(
        &self,
        body_idx: NodeIndex,
    ) -> bool {
        let Some(body_node) = self.arena.get(body_idx) else {
            return false;
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return false;
        };
        block.statements.nodes.iter().copied().any(|stmt_idx| {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                return false;
            };
            if stmt_node.kind != syntax_kind_ext::RETURN_STATEMENT {
                return false;
            }
            let Some(ret) = self.arena.get_return_statement(stmt_node) else {
                return false;
            };
            let Some(expr_node) = self.arena.get(
                self.arena
                    .skip_parenthesized_and_assertions_and_comma(ret.expression),
            ) else {
                return false;
            };
            expr_node.kind == syntax_kind_ext::JSX_ELEMENT
                || expr_node.kind == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
                || expr_node.kind == syntax_kind_ext::JSX_FRAGMENT
        })
    }

    pub(in crate::declaration_emitter) fn emit_js_function_like_class_if_needed(
        &mut self,
        name_idx: NodeIndex,
        params: &NodeList,
        body_idx: NodeIndex,
        is_exported: bool,
        jsdoc_anchor: NodeIndex,
    ) -> bool {
        if !self.source_is_js_file || !body_idx.is_some() {
            return false;
        }
        let this_assignments = self.js_function_body_this_property_assignments(body_idx);
        let prototype_members = self.js_prototype_object_members_for_name(name_idx);
        if this_assignments.is_empty() && prototype_members.is_empty() {
            return false;
        }

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
            self.write("declare ");
        }
        self.write("class ");
        self.emit_node(name_idx);
        self.write(" {");
        self.write_line();
        self.increase_indent();

        let returns_new = self
            .get_identifier_text(name_idx)
            .is_some_and(|name| self.js_function_body_returns_new_named(body_idx, &name));
        let is_export_equals_root = self.is_js_export_equals_name(name_idx);
        let constructor_jsdoc = self.function_like_jsdoc_for_node(jsdoc_anchor);
        let has_constructor_jsdoc = constructor_jsdoc
            .as_deref()
            .is_some_and(|jsdoc| jsdoc.contains("@constructor"));
        if !params.nodes.is_empty() || returns_new || has_constructor_jsdoc || is_export_equals_root
        {
            if let Some(jsdoc) = constructor_jsdoc {
                self.emit_multiline_jsdoc_comment(&jsdoc);
            }
            self.write_indent();
            self.write("constructor(");
            self.emit_parameters_with_body(params, body_idx);
            self.write(");");
            self.write_line();
        }

        let mut declared_names = FxHashSet::default();
        for &member_idx in &prototype_members {
            if let Some(name_idx) = self.get_member_name_idx(member_idx)
                && let Some(name) = self.get_identifier_text(name_idx)
            {
                declared_names.insert(name);
            }
        }
        for (stmt_idx, prop_name_idx, rhs_idx) in this_assignments {
            let Some(prop_name) = self.get_identifier_text(prop_name_idx) else {
                continue;
            };
            if !declared_names.insert(prop_name) {
                continue;
            }
            if self.emit_js_function_typed_property(prop_name_idx, rhs_idx) {
                if let Some(stmt_node) = self.arena.get(stmt_idx) {
                    self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
                }
                continue;
            }
            if let Some(jsdoc_type) = self.jsdoc_type_text_for_node(stmt_idx) {
                if let Some(jsdoc) = self.function_like_jsdoc_for_node(stmt_idx) {
                    self.emit_multiline_jsdoc_comment(&jsdoc);
                }
                self.write_indent();
                self.emit_node(prop_name_idx);
                self.write(": ");
                self.write(&jsdoc_type);
                self.write(";");
                self.write_line();
                if let Some(stmt_node) = self.arena.get(stmt_idx) {
                    self.skip_comments_in_node(stmt_node.pos, stmt_node.end);
                }
                continue;
            }
            let type_text = self
                .js_constructor_assignment_expression_type_text(rhs_idx, params, 0)
                .or_else(|| {
                    self.resolve_declaration_type_text(&[rhs_idx], Some(rhs_idx))
                        .map(|resolved| resolved.emitted_type_text)
                })
                .or_else(|| self.allowlisted_initializer_type_text(rhs_idx))
                .unwrap_or_else(|| "any".to_string());
            self.write_indent();
            self.emit_node(prop_name_idx);
            self.write(": ");
            self.write(&type_text);
            if returns_new && !type_text.contains("undefined") {
                self.write(" | undefined");
            }
            self.write(";");
            self.write_line();
        }

        if let Some(name) = self.get_identifier_text(name_idx)
            && let Some(methods) = self.js_class_like_prototype_members.get(&name).cloned()
        {
            for (method_name, initializer) in methods {
                let Some(method_name_text) = self.get_identifier_text(method_name) else {
                    continue;
                };
                if !declared_names.insert(method_name_text) {
                    continue;
                }
                self.emit_js_synthetic_class_method(method_name, initializer);
            }
        }

        let mut proto_type = None;
        let mut emitted_getters = FxHashSet::default();
        for &member_idx in &prototype_members {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
                && self
                    .arena
                    .get_property_assignment(member_node)
                    .and_then(|prop| self.get_identifier_text(prop.name))
                    .as_deref()
                    == Some("__proto__")
            {
                if let Some(type_text) = self.js_proto_property_assignment_type_text(member_idx) {
                    proto_type = Some(type_text);
                }
                continue;
            }
            if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                && let Some(name_idx) = self.get_member_name_idx(member_idx)
                && let Some(name) = self.get_identifier_text(name_idx)
                && self.prototype_members_have_setter_named(&prototype_members, &name)
            {
                continue;
            }
            let before_jsdoc_len = self.writer.len();
            let saved_comment_idx = self.comment_emit_idx;
            self.emit_leading_jsdoc_comments(member_node.pos);
            let before_member_len = self.writer.len();
            self.emit_class_member(member_idx);
            if member_node.kind == syntax_kind_ext::SET_ACCESSOR
                && let Some(name_idx) = self.get_member_name_idx(member_idx)
                && let Some(name) = self.get_identifier_text(name_idx)
                && emitted_getters.insert(name.clone())
                && let Some(getter_idx) =
                    self.prototype_members_getter_named(&prototype_members, &name)
            {
                self.emit_class_member(getter_idx);
            }
            if self.writer.len() == before_member_len {
                self.writer.truncate(before_jsdoc_len);
                self.comment_emit_idx = saved_comment_idx;
                self.skip_comments_in_node(member_node.pos, member_node.end);
            }
        }
        if let Some(proto_type) = proto_type {
            self.write_indent();
            self.write("__proto__: ");
            self.write(&proto_type);
            self.write(";");
            self.write_line();
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
        true
    }

    pub(in crate::declaration_emitter) fn js_proto_property_assignment_type_text(
        &self,
        member_idx: NodeIndex,
    ) -> Option<String> {
        let member_node = self.arena.get(member_idx)?;
        let prop = self.arena.get_property_assignment(member_node)?;
        let initializer = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(prop.initializer);
        let init_node = self.arena.get(initializer)?;
        if init_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return self
                .get_identifier_text(initializer)
                .map(|base_name| format!("typeof {base_name}"));
        }
        let access = self.arena.get_access_expr(init_node)?;
        if self.get_identifier_text(access.name_or_argument).as_deref() != Some("prototype") {
            return self
                .get_identifier_text(initializer)
                .map(|base_name| format!("typeof {base_name}"));
        }

        let base_name = self.get_identifier_text(access.expression)?;
        Some(format!("typeof {base_name}"))
    }

    pub(in crate::declaration_emitter) fn prototype_members_have_setter_named(
        &self,
        members: &[NodeIndex],
        name: &str,
    ) -> bool {
        self.prototype_members_getter_or_setter_named(members, name, syntax_kind_ext::SET_ACCESSOR)
            .is_some()
    }

    pub(in crate::declaration_emitter) fn prototype_members_getter_named(
        &self,
        members: &[NodeIndex],
        name: &str,
    ) -> Option<NodeIndex> {
        self.prototype_members_getter_or_setter_named(members, name, syntax_kind_ext::GET_ACCESSOR)
    }

    pub(in crate::declaration_emitter) fn prototype_members_getter_or_setter_named(
        &self,
        members: &[NodeIndex],
        name: &str,
        kind: u16,
    ) -> Option<NodeIndex> {
        members.iter().copied().find(|&member_idx| {
            self.arena.get(member_idx).is_some_and(|node| {
                node.kind == kind
                    && self
                        .get_member_name_idx(member_idx)
                        .and_then(|name_idx| self.get_identifier_text(name_idx))
                        .as_deref()
                        == Some(name)
            })
        })
    }

    pub(in crate::declaration_emitter) fn js_function_body_this_property_assignments(
        &self,
        body_idx: NodeIndex,
    ) -> Vec<(NodeIndex, NodeIndex, NodeIndex)> {
        let Some(body_node) = self.arena.get(body_idx) else {
            return Vec::new();
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return Vec::new();
        };
        block
            .statements
            .nodes
            .iter()
            .copied()
            .filter_map(|stmt_idx| {
                self.js_this_property_assignment(stmt_idx)
                    .map(|(name_idx, rhs_idx)| (stmt_idx, name_idx, rhs_idx))
            })
            .collect()
    }

    pub(in crate::declaration_emitter) fn js_prototype_object_members_for_name(
        &self,
        name_idx: NodeIndex,
    ) -> Vec<NodeIndex> {
        let Some(name) = self.get_identifier_text(name_idx) else {
            return Vec::new();
        };
        self.js_prototype_object_members_for_export_name(&name)
    }

    pub(in crate::declaration_emitter) fn js_function_body_returns_new_named(
        &self,
        body_idx: NodeIndex,
        name: &str,
    ) -> bool {
        let Some(body_node) = self.arena.get(body_idx) else {
            return false;
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return false;
        };
        block
            .statements
            .nodes
            .iter()
            .copied()
            .any(|stmt_idx| self.js_statement_returns_new_named(stmt_idx, name))
    }

    pub(in crate::declaration_emitter) fn js_statement_returns_new_named(
        &self,
        stmt_idx: NodeIndex,
        name: &str,
    ) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        match stmt_node.kind {
            k if k == syntax_kind_ext::RETURN_STATEMENT => self
                .arena
                .get_return_statement(stmt_node)
                .is_some_and(|ret| self.js_expression_is_new_named(ret.expression, name)),
            k if k == syntax_kind_ext::BLOCK => {
                self.arena.get_block(stmt_node).is_some_and(|block| {
                    block
                        .statements
                        .nodes
                        .iter()
                        .copied()
                        .any(|stmt_idx| self.js_statement_returns_new_named(stmt_idx, name))
                })
            }
            k if k == syntax_kind_ext::IF_STATEMENT => self
                .arena
                .get_if_statement(stmt_node)
                .is_some_and(|if_data| {
                    self.js_statement_returns_new_named(if_data.then_statement, name)
                        || (if_data.else_statement.is_some()
                            && self.js_statement_returns_new_named(if_data.else_statement, name))
                }),
            _ => false,
        }
    }

    pub(in crate::declaration_emitter) fn js_expression_is_new_named(
        &self,
        expr_idx: NodeIndex,
        name: &str,
    ) -> bool {
        let expr_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_idx);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return false;
        }
        let Some(new_expr) = self.arena.get_call_expr(expr_node) else {
            return false;
        };
        self.get_identifier_text(new_expr.expression).as_deref() == Some(name)
    }

    // Export/import emission → exports.rs
    // Type emission and utility helpers → helpers.rs
}
