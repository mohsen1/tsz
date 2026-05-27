use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::DeclarationEmitter;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn emit_class_member(&mut self, member_idx: NodeIndex) {
        let Some(member_node) = self.arena.get(member_idx) else {
            return;
        };

        // Strip members annotated with @internal when --stripInternal is enabled
        if self.has_internal_annotation(member_node.pos) {
            return;
        }

        // Skip members with private identifier names (#foo) - these are replaced by `#private;`
        if self.member_has_private_identifier_name(member_idx) {
            return;
        }

        // Skip members with computed property names that are not emittable in .d.ts
        // (e.g., ["" + ""], [Symbol()], [variable] — only literals and well-known symbols survive)
        if self.member_has_non_emittable_computed_name(member_idx) {
            return;
        }

        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                self.emit_property_declaration(member_idx);
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                self.emit_method_declaration(member_idx);
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                self.emit_constructor_declaration(member_idx);
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                self.emit_accessor_declaration(member_idx, true);
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                self.emit_accessor_declaration(member_idx, false);
            }
            k if k == syntax_kind_ext::INDEX_SIGNATURE => {
                self.emit_index_signature(member_idx);
            }
            _ => {}
        }
    }

    /// Check if a member has a private identifier (#foo) name.
    pub(in crate::declaration_emitter) fn member_has_private_identifier_name(
        &self,
        member_idx: NodeIndex,
    ) -> bool {
        if let Some(name_idx) = self.get_member_name_idx(member_idx)
            && let Some(name_node) = self.arena.get(name_idx)
        {
            return name_node.kind == SyntaxKind::PrivateIdentifier as u16;
        }
        false
    }

    /// Separator preceding a readonly property's literal value: `" = "` for
    /// class-declaration emit (`readonly x = 0`), `": "` for object-type-literal
    /// emit (`readonly x: 0`). Object-type literals do not permit `=` syntax.
    const fn readonly_literal_value_separator(&self) -> &'static str {
        if self.in_object_type_class_body {
            ": "
        } else {
            " = "
        }
    }

    pub(in crate::declaration_emitter) fn emit_property_declaration(
        &mut self,
        prop_idx: NodeIndex,
    ) {
        let Some(prop_node) = self.arena.get(prop_idx) else {
            return;
        };
        let prop_node_end = prop_node.end;
        let Some(prop) = self.arena.get_property_decl(prop_node) else {
            return;
        };
        let prop_name_span = self
            .arena
            .get(prop.name)
            .map(|name_node| (name_node.pos, name_node.end - name_node.pos));

        self.write_indent();

        // Check if abstract for special handling
        let is_abstract = self
            .arena
            .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword);
        // Check if private for type annotation omission
        let is_private = self
            .arena
            .has_modifier(&prop.modifiers, SyntaxKind::PrivateKeyword);
        let has_explicit_accessibility = self
            .arena
            .has_modifier(&prop.modifiers, SyntaxKind::PrivateKeyword)
            || self
                .arena
                .has_modifier(&prop.modifiers, SyntaxKind::ProtectedKeyword)
            || self
                .arena
                .has_modifier(&prop.modifiers, SyntaxKind::PublicKeyword);

        let has_explicit_readonly = self
            .arena
            .has_modifier(&prop.modifiers, SyntaxKind::ReadonlyKeyword);

        // Modifiers
        if self.source_is_js_file
            && !has_explicit_accessibility
            && self.jsdoc_has_protected_for_node(prop_idx)
        {
            self.write("protected ");
        }
        self.emit_member_modifiers(&prop.modifiers);
        if !has_explicit_readonly
            && self.source_is_js_file
            && self.jsdoc_has_readonly_for_node(prop_idx)
        {
            self.write("readonly ");
        }

        // Name
        if self.source_is_js_file
            && let Some(name_text) = self.resolved_computed_property_name_text(prop.name)
        {
            self.write(&name_text);
        } else {
            self.emit_node(prop.name);
        }

        // Optional/definite-assignment marker: properties can have `?` (optional)
        // or `!` (definite assignment assertion), but not both.  tsc preserves
        // whichever is present in the .d.ts so that consumers see the same
        // optionality contract.
        if prop.question_token {
            self.write("?");
        } else if prop.exclamation_token {
            self.write("!");
        }

        // Check if readonly for literal initializer form
        let is_readonly = has_explicit_readonly
            || (self.source_is_js_file && self.jsdoc_has_readonly_for_node(prop_idx));
        let const_asserted_enum_member = prop
            .initializer
            .is_some()
            .then(|| self.const_asserted_enum_access_member_text(prop.initializer))
            .flatten();
        let widened_enum_type = prop
            .initializer
            .is_some()
            .then(|| self.simple_enum_access_base_name_text(prop.initializer))
            .flatten();

        // Type - use explicit annotation if present, otherwise use inferred type
        // SPECIAL CASE: For private properties, TypeScript omits type annotations in .d.ts
        if prop.type_annotation.is_some() && !is_private {
            self.write(": ");
            self.emit_type(prop.type_annotation);
        } else if !is_private && let Some(type_text) = self.jsdoc_type_text_for_node(prop_idx) {
            self.write(": ");
            self.write(&type_text);
            if prop.question_token && self.strict_null_checks && !type_text.ends_with("| undefined")
            {
                self.write(" | undefined");
            }
        } else if !is_private {
            // For a readonly property whose initializer is a simple enum-member
            // access (e.g. `readonly kind = E.A`), tsc uses the initializer form
            // in class declarations and a literal-typed annotation in object-
            // type-literal bodies. Separator selection is centralised.
            let enum_initializer_text = if is_readonly
                && !is_abstract
                && !prop.question_token
                && prop.initializer.is_some()
            {
                self.simple_enum_access_member_text(prop.initializer)
            } else {
                None
            };

            if let Some(enum_member_text) = enum_initializer_text {
                self.write(self.readonly_literal_value_separator());
                self.write(&enum_member_text);
            } else if let Some(enum_member_text) = const_asserted_enum_member {
                self.write(": ");
                self.write(&enum_member_text);
            } else if !is_readonly
                && !is_abstract
                && !prop.question_token
                && let Some(enum_type_text) = widened_enum_type
            {
                self.write(": ");
                self.write(&enum_type_text);
            } else if let Some(typeof_text) =
                self.shadowed_property_initializer_typeof_text(prop.name, prop.initializer)
            {
                self.write(": ");
                self.write(&typeof_text);
                if prop.question_token
                    && self.strict_null_checks
                    && !typeof_text.ends_with("| undefined")
                {
                    self.write(" | undefined");
                }
            } else if prop.initializer.is_some()
                && let Some(type_text) = self.explicit_asserted_type_text(prop.initializer)
            {
                self.write(": ");
                self.write(&type_text);
            } else if is_readonly
                && !is_abstract
                && !prop.question_token
                && prop.initializer.is_some()
                && self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::StaticKeyword)
                && self.is_symbol_call(prop.initializer)
            {
                self.write(": unique symbol");
            } else if prop.initializer.is_some()
                && let Some(type_text) =
                    self.class_property_function_initializer_type_text(prop_idx, prop.initializer)
            {
                self.write(": ");
                self.write(&type_text);
            } else if prop.initializer.is_some()
                && let Some(type_text) =
                    self.anonymous_module_exports_class_new_expression_type_text(prop.initializer)
            {
                self.write(": ");
                self.write(&type_text);
            } else if let Some(type_id) = self.get_node_type_or_names(&[prop_idx, prop.name]) {
                // Readonly literal preservation (matches tsc `const`-like emit).
                if is_readonly
                    && !is_abstract
                    && !prop.question_token
                    && let Some(interner) = self.type_interner
                    && let Some(lit) = tsz_solver::visitor::literal_value(interner, type_id)
                {
                    self.write(self.readonly_literal_value_separator());
                    self.write(&Self::format_literal_initializer(&lit, interner));
                } else if is_readonly
                    && !is_abstract
                    && !prop.question_token
                    && prop.initializer.is_some()
                    && let Some(lit_text) =
                        self.const_literal_initializer_text_deep(prop.initializer)
                {
                    // Type system widened the literal (e.g. `false` → `boolean`);
                    // recover the original from the initializer source text.
                    self.write(self.readonly_literal_value_separator());
                    self.write(&lit_text);
                } else if let Some(typeof_text) = self.typeof_prefix_for_value_entity(
                    prop.initializer,
                    prop.initializer.is_some(),
                    Some(type_id),
                ) {
                    self.write(": ");
                    self.write(&typeof_text);
                    if prop.question_token
                        && self.strict_null_checks
                        && !typeof_text.ends_with("| undefined")
                    {
                        self.write(" | undefined");
                    }
                } else {
                    // Inferred class-property declaration surfaces widen
                    // unique-symbol values from references to `symbol`; a direct
                    // static readonly `Symbol()` initializer is handled above.
                    let effective_type = if prop.initializer.is_some() {
                        self.type_interner
                            .map(|interner| {
                                tsz_solver::operations::widening::widen_type(interner, type_id)
                            })
                            .unwrap_or(type_id)
                    } else if !is_readonly {
                        self.type_interner
                            .map(|interner| {
                                tsz_solver::operations::widening::widen_literal_type(
                                    interner, type_id,
                                )
                            })
                            .unwrap_or(type_id)
                    } else {
                        type_id
                    };
                    let type_text = self
                        .rewrite_recursive_static_class_expression_type(prop_idx, effective_type);
                    let has_object_literal_initializer =
                        self.arena.get(prop.initializer).is_some_and(|node| {
                            node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        });
                    let type_text = if has_object_literal_initializer
                        && (type_text == "any"
                            || type_text.contains(": any;")
                            || self.object_literal_prefers_syntax_type_text(prop.initializer))
                    {
                        self.allowlisted_initializer_type_text(prop.initializer)
                            .unwrap_or(type_text)
                    } else {
                        type_text
                    };
                    let mut emitted_any_for_truncation = false;
                    if let Some(name_node) = self.arena.get(prop.name)
                        && let Some(file_path) = self.current_file_path.clone()
                    {
                        if self.emit_serialized_type_text_truncation_diagnostic_if_needed(
                            &type_text,
                            &file_path,
                            name_node.pos,
                            name_node.end - name_node.pos,
                        ) {
                            self.write(": any");
                            emitted_any_for_truncation = true;
                        }
                        if !emitted_any_for_truncation {
                            if self.emit_non_serializable_local_alias_diagnostic(
                                &type_text,
                                &file_path,
                                name_node.pos,
                                name_node.end - name_node.pos,
                            ) {
                                self.write(": any");
                                emitted_any_for_truncation = true;
                            }
                        }
                        if !emitted_any_for_truncation {
                            let _ = self.emit_non_serializable_import_type_diagnostic(
                                &type_text,
                                &file_path,
                                name_node.pos,
                                name_node.end - name_node.pos,
                            );
                        }
                    }
                    if emitted_any_for_truncation {
                    } else if self.printed_type_uses_private_import_type_root(&type_text)
                        && !self.isolated_declarations
                    {
                        if let (Some(file_path), Some((pos, length))) =
                            (self.current_file_path.as_deref(), prop_name_span)
                        {
                            self.diagnostics
                                .push(tsz_common::diagnostics::Diagnostic::from_code(
                                    7056,
                                    file_path,
                                    pos,
                                    length,
                                    &[],
                                ));
                        }
                        self.write(": any");
                    } else {
                        self.write(": ");
                        self.write(&type_text);
                    }
                    // For optional class properties without an explicit type annotation,
                    // tsc appends `| undefined` when the inferred type doesn't already
                    // include it (e.g., `c? = 2` → `c?: number | undefined`).
                    if prop.question_token
                        && self.strict_null_checks
                        && !type_text.ends_with("| undefined")
                    {
                        self.write(" | undefined");
                    }
                }
            } else if is_readonly
                && !is_abstract
                && !prop.question_token
                && prop.initializer.is_some()
                && let Some(lit_text) = self.const_literal_initializer_text_deep(prop.initializer)
            {
                // Readonly literal preservation via initializer source text.
                self.write(self.readonly_literal_value_separator());
                self.write(&lit_text);
            } else if prop.initializer.is_some()
                && let Some(type_text) = self.allowlisted_initializer_type_text(prop.initializer)
            {
                let emitted_any_for_truncation = if let (Some(file_path), Some((pos, length))) =
                    (self.current_file_path.clone(), prop_name_span)
                {
                    if self.emit_serialized_type_text_truncation_diagnostic_if_needed(
                        &type_text, &file_path, pos, length,
                    ) {
                        self.write(": any");
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                if emitted_any_for_truncation {
                } else if self.printed_type_uses_private_import_type_root(&type_text)
                    && !self.isolated_declarations
                {
                    if let (Some(file_path), Some((pos, length))) =
                        (self.current_file_path.as_deref(), prop_name_span)
                    {
                        self.diagnostics
                            .push(tsz_common::diagnostics::Diagnostic::from_code(
                                7056,
                                file_path,
                                pos,
                                length,
                                &[],
                            ));
                    }
                    self.write(": any");
                } else {
                    self.write(": ");
                    self.write(&type_text);
                }
                // Same `| undefined` rule for fallback-inferred types on optional
                // class properties.
                if prop.question_token
                    && self.strict_null_checks
                    && !type_text.ends_with("| undefined")
                {
                    self.write(" | undefined");
                }
            }
        }

        self.write(";");
        if !prop.initializer.is_some() {
            self.emit_trailing_comment(prop_node_end);
        }
        self.write_line();
    }
}
