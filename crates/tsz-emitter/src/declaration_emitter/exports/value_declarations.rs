//! Declaration emitter - exported enum and variable declarations.

use super::super::DeclarationEmitter;
use crate::enums::evaluator::{EnumEvaluator, EnumValue};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(crate) fn emit_exported_enum(&mut self, enum_idx: NodeIndex) {
        let Some(enum_node) = self.arena.get(enum_idx) else {
            return;
        };
        let Some(enum_data) = self.arena.get_enum(enum_node) else {
            return;
        };

        let is_const = self
            .arena
            .has_modifier(&enum_data.modifiers, SyntaxKind::ConstKeyword);

        self.write_indent();
        if self.should_emit_export_keyword() {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(true) {
            self.write("declare ");
        }
        if is_const {
            self.write("const ");
        }
        self.write("enum ");
        self.emit_node(enum_data.name);

        self.write(" {");
        self.write_line();
        self.increase_indent();

        // Evaluate enum member values to get correct auto-increment behavior.
        // Seed with accumulated values for cross-enum reference resolution.
        let prior = std::mem::take(&mut self.all_enum_values);
        let mut evaluator = EnumEvaluator::with_prior_values(self.arena, prior);
        let member_values = evaluator.evaluate_enum(enum_idx);
        self.all_enum_values = evaluator.take_all_enum_values();

        for (i, &member_idx) in enum_data.members.nodes.iter().enumerate() {
            self.write_indent();
            if let Some(member_node) = self.arena.get(member_idx)
                && let Some(member) = self.arena.get_enum_member(member_node)
            {
                self.emit_node(member.name);
                let member_name = self.get_enum_member_name(member.name);
                if let Some(value) = member_values.get(&member_name) {
                    match value {
                        crate::enums::evaluator::EnumValue::Computed => {
                            // Computed values: no initializer in .d.ts
                        }
                        _ => {
                            self.write(" = ");
                            self.emit_enum_value(value);
                        }
                    }
                } else {
                    // Fallback to index if evaluation failed
                    self.write(" = ");
                    self.write(&i.to_string());
                }
            }
            if i < enum_data.members.nodes.len() - 1 {
                self.write(",");
            }
            self.write_line();
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    /// Get the name of an enum member from its name node.
    pub(crate) fn get_enum_member_name(&self, name_idx: NodeIndex) -> String {
        if let Some(name_node) = self.arena.get(name_idx) {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return ident.escaped_text.clone();
            }
            if let Some(lit) = self.arena.get_literal(name_node) {
                return lit.text.clone();
            }
        }
        String::new()
    }

    /// Emit an evaluated enum value.
    pub(crate) fn emit_enum_value(&mut self, value: &EnumValue) {
        match value {
            EnumValue::Number(n) => {
                self.write(&n.to_string());
            }
            EnumValue::String(s) => {
                self.write("\"");
                for ch in s.chars() {
                    match ch {
                        '\\' => self.write("\\\\"),
                        '"' => self.write("\\\""),
                        '\n' => self.write("\\n"),
                        '\r' => self.write("\\r"),
                        '\t' => self.write("\\t"),
                        '\0' => self.write("\\0"),
                        _ => {
                            let mut buf = [0u8; 4];
                            self.write(ch.encode_utf8(&mut buf));
                        }
                    }
                }
                self.write("\"");
            }
            EnumValue::Float(f) => {
                self.write(&Self::format_js_number(*f));
            }
            EnumValue::Computed => {
                // For computed values, emit 0 as fallback
                self.write("0 /* computed */");
            }
        }
    }

    pub(crate) fn emit_exported_variable(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
            return;
        };

        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };

            if decl_list_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                && let Some(decl_list) = self.arena.get_variable(decl_list_node)
            {
                // `using` and `await using` declarations emit as `const` in .d.ts
                let flags = decl_list_node.flags as u32;
                let js_var_promoted_to_const;
                let keyword = if flags
                    & (tsz_parser::parser::node_flags::USING
                        | tsz_parser::parser::node_flags::CONST)
                    != 0
                {
                    js_var_promoted_to_const = false;
                    "const"
                } else if flags & tsz_parser::parser::node_flags::LET != 0 {
                    js_var_promoted_to_const = false;
                    "let"
                } else if self.source_is_js_file {
                    js_var_promoted_to_const = true;
                    "const"
                } else {
                    js_var_promoted_to_const = false;
                    "var"
                };

                // Separate destructuring from regular declarations
                let mut regular_decls = Vec::new();
                for &decl_idx in &decl_list.declarations.nodes {
                    if let Some(decl_node) = self.arena.get(decl_idx)
                        && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                    {
                        let name_node = self.arena.get(decl.name);
                        let is_destructuring = name_node.is_some_and(|n| {
                            n.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                                || n.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                        });

                        if is_destructuring {
                            self.emit_flattened_variable_declaration(decl_idx, keyword, true);
                        } else {
                            regular_decls.push((decl_idx, decl));
                        }
                    }
                }

                if regular_decls.len() == 1 {
                    let (decl_idx, decl) = regular_decls[0];
                    if self.emit_jsdoc_enum_variable_declaration_if_possible(
                        decl_idx,
                        decl.name,
                        decl.initializer,
                        true,
                    ) {
                        if let Some(dn) = self.arena.get(decl_idx) {
                            let skip_end =
                                self.arena.get(decl.initializer).map_or(dn.end, |n| n.end);
                            self.skip_comments_in_node(dn.pos, skip_end);
                        }
                        continue;
                    }
                    if self.emit_js_function_variable_declaration_if_possible(
                        decl_idx,
                        decl.name,
                        decl.initializer,
                        true,
                    ) {
                        continue;
                    }
                    if self.source_is_js_file
                        && self.emit_js_object_literal_namespace(
                            decl.name,
                            decl.initializer,
                            true,
                            false,
                        )
                    {
                        if let Some(dn) = self.arena.get(decl_idx) {
                            let skip_end =
                                self.arena.get(decl.initializer).map_or(dn.end, |n| n.end);
                            self.skip_comments_in_node(dn.pos, skip_end);
                        }
                        continue;
                    }
                    // In JS declaration emit, a variable with a class expression initializer
                    // and no explicit annotation is surfaced as an exported class using the
                    // binding name. TS source keeps the variable shape and emits a structural
                    // constructor object type.
                    let is_exported = self.should_emit_export_keyword();
                    if self.source_is_js_file
                        && decl.type_annotation.is_none()
                        && self.emit_js_named_class_expression_declaration(
                            decl.name,
                            decl.initializer,
                            is_exported,
                        )
                    {
                        if let Some(dn) = self.arena.get(decl_idx) {
                            let skip_end =
                                self.arena.get(decl.initializer).map_or(dn.end, |n| n.end);
                            self.skip_comments_in_node(dn.pos, skip_end);
                        }
                        continue;
                    }
                }

                // Emit all regular declarations together on one line
                if !regular_decls.is_empty() {
                    let suppress_namespace_proto_export = self.inside_non_ambient_namespace
                        && regular_decls.iter().all(|(_, decl)| {
                            self.get_identifier_text(decl.name).as_deref() == Some("__proto__")
                        });
                    self.write_indent();
                    if self.should_emit_export_keyword() && !suppress_namespace_proto_export {
                        self.write("export ");
                    }
                    if self.should_emit_declare_keyword(true) {
                        self.write("declare ");
                    }
                    // For JS `var` promoted to `const`, revert to `var` if
                    // any declaration has a JSDoc @type annotation.
                    let effective_keyword = if js_var_promoted_to_const {
                        let is_named_js_export = regular_decls.iter().any(|(_, decl)| {
                            self.get_identifier_text(decl.name)
                                .is_some_and(|name| self.js_named_export_names.contains(&name))
                        });
                        let has_jsdoc = self.jsdoc_preserves_js_var_keyword(
                            stmt_node.pos,
                            regular_decls
                                .iter()
                                .map(|(decl_idx, decl)| (*decl_idx, decl.name)),
                        );
                        if has_jsdoc || is_named_js_export {
                            "var"
                        } else {
                            keyword
                        }
                    } else {
                        keyword
                    };
                    self.write(effective_keyword);
                    self.write(" ");

                    for (i, (decl_idx, decl)) in regular_decls.iter().enumerate() {
                        if i > 0 {
                            self.write(", ");
                        }

                        self.emit_node(decl.name);
                        // When a variable's initializer is a simple reference to an
                        // import-equals alias (e.g. `var bVal2 = b` where `import b = a.foo`),
                        // tsc emits `typeof b` instead of expanding the type.
                        if !decl.type_annotation.is_some()
                            && decl.initializer.is_some()
                            && let Some(alias_text) =
                                self.initializer_import_alias_typeof_text(decl.initializer)
                        {
                            self.write(": typeof ");
                            self.write(&alias_text);
                        } else {
                            self.emit_variable_decl_type_or_initializer(
                                effective_keyword,
                                stmt_node.pos,
                                *decl_idx,
                                decl.name,
                                decl.type_annotation,
                                decl.initializer,
                            );
                        }
                    }

                    self.write(";");
                    self.write_line();
                }
            }
        }
    }
}
