use crate::enums::evaluator::EnumEvaluator;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::DeclarationEmitter;
use super::emit_members::{ClassMemberInfo, ClassMemberKind};

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn emit_type_alias_declaration(
        &mut self,
        alias_idx: NodeIndex,
    ) {
        let Some(alias_node) = self.arena.get(alias_idx) else {
            return;
        };
        let Some(alias) = self.arena.get_type_alias(alias_node) else {
            return;
        };

        let is_exported = self
            .arena
            .has_modifier(&alias.modifiers, SyntaxKind::ExportKeyword);
        if !self.should_emit_public_api_member(&alias.modifiers)
            && !self.should_emit_public_api_dependency(alias.name)
        {
            return;
        }
        if self.should_skip_ns_internal_member(&alias.modifiers, Some(alias_idx)) {
            return;
        }

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self
            .arena
            .has_modifier(&alias.modifiers, SyntaxKind::DeclareKeyword)
            && !self.inside_declare_namespace
        {
            self.write("declare ");
        }
        self.write("type ");

        // Name
        self.emit_node(alias.name);

        // Type parameters
        if let Some(ref type_params) = alias.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        self.write(" = ");
        self.emit_type(alias.type_node);
        self.write(";");
        self.write_line();
    }

    pub(in crate::declaration_emitter) fn emit_enum_declaration(&mut self, enum_idx: NodeIndex) {
        let Some(enum_node) = self.arena.get(enum_idx) else {
            return;
        };
        let Some(enum_data) = self.arena.get_enum(enum_node) else {
            return;
        };

        let is_exported = self
            .arena
            .has_modifier(&enum_data.modifiers, SyntaxKind::ExportKeyword);
        if !self.should_emit_public_api_member(&enum_data.modifiers)
            && !self.should_emit_public_api_dependency(enum_data.name)
        {
            return;
        }
        if self.should_skip_ns_internal_member(&enum_data.modifiers, Some(enum_idx)) {
            return;
        }
        let is_const = self
            .arena
            .has_modifier(&enum_data.modifiers, SyntaxKind::ConstKeyword);

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        if self.should_emit_declare_keyword(is_exported) {
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
        // Seed the evaluator with accumulated values from prior enums so that
        // cross-enum references (e.g., `enum B { Y = A.X }`) can be resolved.
        let prior = std::mem::take(&mut self.all_enum_values);
        let mut evaluator = EnumEvaluator::with_prior_values(self.arena, prior);
        let member_values = evaluator.evaluate_enum(enum_idx);
        self.all_enum_values = evaluator.take_all_enum_values();

        for (i, &member_idx) in enum_data.members.nodes.iter().enumerate() {
            if let Some(mn) = self.arena.get(member_idx) {
                self.emit_leading_jsdoc_comments(mn.pos);
            }
            self.write_indent();
            if let Some(member_node) = self.arena.get(member_idx)
                && let Some(member) = self.arena.get_enum_member(member_node)
            {
                self.emit_node(member.name);
                // For ambient enums (inside declare context or with declare keyword), only
                // emit values for members with explicit initializers.
                // For implementation enums, always emit computed values.
                let is_ambient = self.inside_declare_namespace
                    || self
                        .arena
                        .has_modifier(&enum_data.modifiers, SyntaxKind::DeclareKeyword)
                    || self.source_is_declaration_file;
                let has_explicit_init = member.initializer.is_some();
                let should_emit_value = !is_ambient || has_explicit_init || is_const;
                if should_emit_value {
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
                    } else if !is_ambient {
                        // Fallback to index for non-ambient enums if evaluation failed
                        self.write(" = ");
                        self.write(&i.to_string());
                    }
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

    /// Check if an initializer expression is a `Symbol()` call (for unique symbol detection)
    pub(in crate::declaration_emitter) fn is_symbol_call(&self, initializer: NodeIndex) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };

        // Check if it's a call expression
        if init_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }

        let Some(call_expr) = self.arena.get_call_expr(init_node) else {
            return false;
        };

        // Check if the function being called is named "Symbol"
        let Some(expr_node) = self.arena.get(call_expr.expression) else {
            return false;
        };

        // Handle both simple identifiers and property access like global.Symbol
        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(ident) = self.arena.get_identifier(expr_node) {
                    return ident.escaped_text == "Symbol";
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                // Handle things like global.Symbol or Symbol.constructor
                if let Some(prop_access) = self.arena.get_access_expr(expr_node) {
                    // Check if the property name is "Symbol"
                    let Some(name_node) = self.arena.get(prop_access.name_or_argument) else {
                        return false;
                    };
                    if let Some(ident) = self.arena.get_identifier(name_node) {
                        return ident.escaped_text == "Symbol";
                    }
                }
            }
            _ => {}
        }

        false
    }

    /// Check if a `PrefixUnaryExpression` node is a negative numeric/bigint literal (e.g., `-123`, `-12n`)
    pub(in crate::declaration_emitter) fn is_negative_literal(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> bool {
        if let Some(unary) = self.arena.get_unary_expr(node)
            && unary.operator == SyntaxKind::MinusToken as u16
            && let Some(operand_node) = self.arena.get(unary.operand)
        {
            let k = operand_node.kind;
            return k == SyntaxKind::NumericLiteral as u16 || k == SyntaxKind::BigIntLiteral as u16;
        }
        false
    }

    /// Check whether a property/element access is a simple enum member access (E.A or E["key"]).
    /// Returns true only when the left-hand side is a simple identifier (not a chain like a.b.c).
    pub(in crate::declaration_emitter) fn is_simple_enum_access(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> bool {
        if let Some(access) = self.arena.get_access_expr(node)
            && let Some(expr_node) = self.arena.get(access.expression)
        {
            return expr_node.kind == SyntaxKind::Identifier as u16;
        }
        false
    }

    /// Check whether a computed property name expression is suitable for `.d.ts` emission.
    ///
    /// In tsc, computed property names survive into declaration output when they are
    /// "entity name expressions" — late-bindable names that can be statically resolved:
    /// 1. String literals: `["hello"]`
    /// 2. Numeric literals: `[42]`, `[-1]`
    /// 3. Well-known symbol accesses: `[Symbol.iterator]`, `[Symbol.hasInstance]`, etc.
    /// 4. Identifiers referencing unique symbols or const enums: `[key]`, `[O]`
    /// 5. Property accesses on entity names: `[E.A]`, `[TestEnum.Test1]`
    pub(in crate::declaration_emitter) fn should_emit_computed_property(
        &self,
        name_idx: NodeIndex,
    ) -> bool {
        let Some(name_node) = self.arena.get(name_idx) else {
            return true;
        };

        // Not a computed property name — always emit
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return true;
        }

        let Some(computed) = self.arena.get_computed_property(name_node) else {
            return false;
        };

        self.is_entity_name_expression(computed.expression)
    }

    /// Check if an expression is an "entity name expression" — an expression that can
    /// appear as a computed property name in declaration output.
    pub(in crate::declaration_emitter) fn is_entity_name_expression(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };

        match expr_node.kind {
            // String literal: ["hello"]
            k if k == SyntaxKind::StringLiteral as u16 => true,
            // Numeric literal: [42]
            k if k == SyntaxKind::NumericLiteral as u16 => true,
            // Identifier: [key], [O], [symb]
            k if k == SyntaxKind::Identifier as u16 => true,
            // Property access: [Symbol.iterator], [E.A], [TestEnum.Test1]
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(expr_node) {
                    self.is_entity_name_expression(access.expression)
                } else {
                    false
                }
            }
            // Prefix unary: [-1]
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => true,
            _ => false,
        }
    }

    pub(in crate::declaration_emitter) fn class_member_info(
        &self,
        member_idx: NodeIndex,
    ) -> Option<ClassMemberInfo> {
        let member_node = self.arena.get(member_idx)?;

        if let Some(prop) = self.arena.get_property_decl(member_node) {
            return Some(ClassMemberInfo {
                kind: ClassMemberKind::Property,
                name: Some(prop.name),
                is_static: self.arena.is_static(&prop.modifiers),
            });
        }
        if let Some(method) = self.arena.get_method_decl(member_node) {
            return Some(ClassMemberInfo {
                kind: ClassMemberKind::Method,
                name: Some(method.name),
                is_static: self.arena.is_static(&method.modifiers),
            });
        }
        if let Some(accessor) = self.arena.get_accessor(member_node) {
            return Some(ClassMemberInfo {
                kind: ClassMemberKind::Accessor,
                name: Some(accessor.name),
                is_static: self.arena.is_static(&accessor.modifiers),
            });
        }
        if let Some(sig) = self.arena.get_signature(member_node) {
            return Some(ClassMemberInfo {
                kind: ClassMemberKind::Signature,
                name: Some(sig.name),
                is_static: false,
            });
        }
        if let Some(index) = self.arena.get_index_signature(member_node) {
            return Some(ClassMemberInfo {
                kind: ClassMemberKind::IndexSignature,
                name: None,
                is_static: self.arena.is_static(&index.modifiers),
            });
        }
        if member_node.kind == syntax_kind_ext::CONSTRUCTOR {
            return Some(ClassMemberInfo {
                kind: ClassMemberKind::Constructor,
                name: None,
                is_static: false,
            });
        }
        None
    }

    /// Get the name `NodeIndex` of a class or interface member, if it has one.
    pub(in crate::declaration_emitter) fn get_member_name_idx(
        &self,
        member_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        self.class_member_info(member_idx)?.name
    }

    /// Check if a member has a computed property name that should NOT be emitted in `.d.ts`.
    /// Returns `true` if the member should be skipped.
    pub(in crate::declaration_emitter) fn member_has_non_emittable_computed_name(
        &self,
        member_idx: NodeIndex,
    ) -> bool {
        if let Some(name_idx) = self.get_member_name_idx(member_idx) {
            !self.should_emit_computed_property(name_idx)
        } else {
            false
        }
    }

    /// Check if a class has any member with a `#private` identifier name.
    /// TypeScript collapses all private-name members into a single `#private;` field.
    pub(in crate::declaration_emitter) fn class_has_private_identifier_member(
        &self,
        members: &tsz_parser::parser::NodeList,
    ) -> bool {
        members
            .nodes
            .iter()
            .copied()
            .any(|member_idx| self.member_has_private_identifier_name(member_idx))
    }

    /// Check if a function body has any return statements with value expressions.
    /// Returns true if all returns are bare `return;` or there are no return statements,
    /// meaning the function effectively returns void.
    pub(in crate::declaration_emitter) fn body_returns_void(&self, body_idx: NodeIndex) -> bool {
        let Some(body_node) = self.arena.get(body_idx) else {
            return true;
        };
        let Some(block) = self.arena.get_block(body_node) else {
            return false;
        };
        self.block_returns_void(&block.statements)
    }

    pub(in crate::declaration_emitter) fn block_returns_void(
        &self,
        statements: &tsz_parser::parser::NodeList,
    ) -> bool {
        for &stmt_idx in &statements.nodes {
            if !self.stmt_returns_void(stmt_idx) {
                return false;
            }
        }
        true
    }

    pub(in crate::declaration_emitter) fn stmt_returns_void(&self, stmt_idx: NodeIndex) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return true;
        };
        match stmt_node.kind {
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                // Return with expression → non-void
                if let Some(ret) = self.arena.get_return_statement(stmt_node) {
                    return ret.expression.is_none();
                }
                true
            }
            k if k == syntax_kind_ext::THROW_STATEMENT => true,
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = self.arena.get_block(stmt_node) {
                    self.block_returns_void(&block.statements)
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_data) = self.arena.get_if_statement(stmt_node) {
                    // Must check both branches; an if without else can still
                    // contain `return expr;` in the then-branch
                    self.stmt_returns_void(if_data.then_statement)
                        && (if_data.else_statement.is_none()
                            || self.stmt_returns_void(if_data.else_statement))
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_data) = self.arena.get_try(stmt_node) {
                    self.stmt_returns_void(try_data.try_block)
                        && (try_data.catch_clause.is_none()
                            || self.stmt_returns_void(try_data.catch_clause))
                        && (try_data.finally_block.is_none()
                            || self.stmt_returns_void(try_data.finally_block))
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch_data) = self.arena.get_catch_clause(stmt_node) {
                    self.stmt_returns_void(catch_data.block)
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
                if let Some(clause) = self.arena.get_case_clause(stmt_node) {
                    self.block_returns_void(&clause.statements)
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                // Check all case clauses inside the switch's case block
                if let Some(switch_data) = self.arena.get_switch(stmt_node) {
                    if let Some(case_block_node) = self.arena.get(switch_data.case_block)
                        && let Some(block) = self.arena.get_block(case_block_node)
                    {
                        self.block_returns_void(&block.statements)
                    } else {
                        true
                    }
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::FOR_STATEMENT
                || k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT =>
            {
                if let Some(loop_data) = self.arena.get_loop(stmt_node) {
                    self.stmt_returns_void(loop_data.statement)
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT =>
            {
                if let Some(for_data) = self.arena.get_for_in_of(stmt_node) {
                    self.stmt_returns_void(for_data.statement)
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled) = self.arena.get_labeled_statement(stmt_node) {
                    self.stmt_returns_void(labeled.statement)
                } else {
                    true
                }
            }
            k if k == syntax_kind_ext::WITH_STATEMENT => {
                if let Some(with_data) = self.arena.get_with_statement(stmt_node) {
                    // with_statement stores its body in then_statement
                    self.stmt_returns_void(with_data.then_statement)
                } else {
                    true
                }
            }
            // Non-compound statements (expression statements, variable declarations, etc.)
            // cannot contain return statements, so they're void-safe.
            _ => true,
        }
    }

    pub(in crate::declaration_emitter) fn emit_variable_declaration_statement(
        &mut self,
        stmt_idx: NodeIndex,
    ) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
            return;
        };

        let has_export_modifier = self
            .arena
            .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword);
        let has_js_named_export = var_stmt.declarations.nodes.iter().any(|&decl_list_idx| {
            self.arena
                .get(decl_list_idx)
                .and_then(|decl_list_node| self.arena.get_variable(decl_list_node))
                .is_some_and(|decl_list| {
                    decl_list.declarations.nodes.iter().any(|&decl_idx| {
                        self.arena
                            .get(decl_idx)
                            .and_then(|decl_node| self.arena.get_variable_declaration(decl_node))
                            .is_some_and(|decl| self.is_js_named_exported_name(decl.name))
                    })
                })
        });
        if !has_js_named_export && !self.should_emit_public_api_member(&var_stmt.modifiers) {
            // Check if any individual variable is referenced by the public API
            let has_dependency = var_stmt.declarations.nodes.iter().any(|&decl_list_idx| {
                if let Some(decl_list_node) = self.arena.get(decl_list_idx)
                    && decl_list_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                    && let Some(decl_list) = self.arena.get_variable(decl_list_node)
                {
                    decl_list.declarations.nodes.iter().any(|&decl_idx| {
                        if let Some(decl_node) = self.arena.get(decl_idx)
                            && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                        {
                            self.should_emit_public_api_dependency(decl.name)
                        } else {
                            false
                        }
                    })
                } else {
                    false
                }
            });
            if !has_dependency {
                return;
            }
        }
        if !has_js_named_export && self.should_skip_ns_internal_member(&var_stmt.modifiers, None) {
            return;
        }
        if !has_export_modifier
            && !has_js_named_export
            && self.record_js_require_property_import_alias_statement(stmt_idx)
        {
            return;
        }

        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };

            if decl_list_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                && let Some(decl_list) = self.arena.get_variable(decl_list_node)
            {
                // Determine let/const/var
                // `using` and `await using` declarations emit as `const` in .d.ts
                let flags = decl_list_node.flags as u32;
                // USING(4) and AWAIT_USING(6) both have the USING bit set
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
                            if self.js_local_bare_require_alias_without_export_surface(
                                decl.initializer,
                            ) {
                                self.record_js_elided_bare_require_binding_names(decl.name);
                                let skip_end = self
                                    .arena
                                    .get(decl.initializer)
                                    .map_or(decl_node.end, |n| n.end);
                                self.skip_comments_in_node(decl_node.pos, skip_end);
                                continue;
                            }
                            // Emit destructuring as individual declarations
                            let is_exported =
                                has_export_modifier || self.is_js_named_exported_name(decl.name);
                            self.emit_flattened_variable_declaration(
                                decl_idx,
                                keyword,
                                is_exported,
                            );
                            let skip_end = self
                                .arena
                                .get(decl.initializer)
                                .map_or(decl_node.end, |n| n.end);
                            self.skip_comments_in_node(decl_node.pos, skip_end);
                        } else {
                            let is_exported = (has_export_modifier
                                || self.is_js_named_exported_name(decl.name))
                                && !(self.inside_non_ambient_namespace
                                    && self.get_identifier_text(decl.name).as_deref()
                                        == Some("__proto__"));
                            regular_decls.push((is_exported, decl_idx, decl_node, decl));
                        }
                    }
                }

                if regular_decls.len() == 1 {
                    let (is_exported, decl_idx, _decl_node, decl) = regular_decls[0];
                    if self.emit_js_class_like_heuristic_if_needed(decl.name, is_exported) {
                        if let Some(dn) = self.arena.get(decl_idx) {
                            let skip_end =
                                self.arena.get(decl.initializer).map_or(dn.end, |n| n.end);
                            self.skip_comments_in_node(dn.pos, skip_end);
                        }
                        continue;
                    }
                    let has_jsdoc_type = self.jsdoc_type_text_for_node(decl_idx).is_some()
                        || self.jsdoc_type_text_for_node(decl.name).is_some();
                    if !has_jsdoc_type
                        && self.emit_js_object_literal_namespace_if_possible(
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
                    if self.emit_jsdoc_enum_variable_declaration_if_possible(
                        decl_idx,
                        decl.name,
                        decl.initializer,
                        is_exported,
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
                        is_exported,
                    ) {
                        if let Some(dn) = self.arena.get(decl_idx) {
                            let skip_end =
                                self.arena.get(decl.initializer).map_or(dn.end, |n| n.end);
                            self.skip_comments_in_node(dn.pos, skip_end);
                        }
                        continue;
                    }
                    // In JS declaration emit, an exported variable with a class expression
                    // initializer and no explicit annotation is surfaced as an exported class.
                    // TS source keeps the variable shape and emits a structural constructor
                    // object type, including inside namespaces.
                    // (Top-level `export const` goes through emit_exported_variable instead.)
                    if self.source_is_js_file
                        && is_exported
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

                // When emitting a non-exported variable statement purely because of
                // dependency tracking, filter to only the declarations that are actually
                // referenced. E.g. `const key = Symbol(), value = 12` should only emit
                // `key` if only `key` is in used_symbols.
                if !has_export_modifier && !has_js_named_export {
                    regular_decls.retain(|(_is_exported, _decl_idx, _decl_node, decl)| {
                        self.should_emit_public_api_dependency(decl.name)
                            && !self
                                .public_api_dependency_is_type_only_exported_type_side(decl.name)
                            && !self.default_import_alias_dependency_is_type_only(
                                decl.name,
                                decl.initializer,
                            )
                            && !self.declared_ambient_value_dependency_is_initializer_only(
                                decl.name,
                                decl.initializer,
                                decl.type_annotation,
                            )
                            && !self.js_local_bare_require_alias_without_export_surface(
                                decl.initializer,
                            )
                            && !self.initializer_references_js_elided_bare_require_binding(
                                decl.initializer,
                            )
                            && !self.js_variable_dependency_is_synthetic_class_extends_alias_source(
                                decl.name,
                            )
                    });
                }

                // Emit regular declarations in contiguous export/non-export groups.
                let mut group_start = 0;
                while group_start < regular_decls.len() {
                    let is_exported = regular_decls[group_start].0;
                    let mut group_end = group_start;
                    while group_end < regular_decls.len()
                        && regular_decls[group_end].0 == is_exported
                    {
                        group_end += 1;
                    }
                    for (_, _, _, decl) in &regular_decls[group_start..group_end] {
                        self.emit_pending_js_export_equals_for_name(decl.name);
                    }
                    self.write_indent();
                    if is_exported {
                        self.write("export ");
                    }
                    if self.should_emit_declare_keyword(is_exported) {
                        self.write("declare ");
                    }
                    let effective_keyword = if self.source_is_js_file
                        && self.inside_non_ambient_namespace
                        && is_exported
                        && !has_export_modifier
                    {
                        "let"
                    } else if js_var_promoted_to_const {
                        let has_bundled_duplicate_global_var = regular_decls
                            [group_start..group_end]
                            .iter()
                            .any(|(_, _, _, decl)| {
                                self.get_identifier_text(decl.name).is_some_and(|name| {
                                    self.bundled_duplicate_global_var_types
                                        .contains_key(name.as_str())
                                })
                            });
                        let is_named_js_export =
                            regular_decls[group_start..group_end]
                                .iter()
                                .any(|(_, _, _, decl)| {
                                    self.get_identifier_text(decl.name).is_some_and(|name| {
                                        self.js_named_export_names.contains(&name)
                                    })
                                });
                        let has_jsdoc = self.jsdoc_preserves_js_var_keyword(
                            stmt_node.pos,
                            regular_decls[group_start..group_end]
                                .iter()
                                .map(|(_, decl_idx, _, decl)| (*decl_idx, decl.name)),
                        );
                        if has_jsdoc || is_named_js_export || has_bundled_duplicate_global_var {
                            "var"
                        } else {
                            keyword
                        }
                    } else {
                        keyword
                    };
                    self.write(effective_keyword);
                    self.write(" ");

                    let mut i = group_start;
                    while i < group_end {
                        if i > group_start {
                            self.write(", ");
                        }
                        let (_is_exported, decl_idx, _decl_node, decl) = &regular_decls[i];

                        // Emit inline comments between keyword and name
                        // (e.g. `var /*4*/ point = ...` → `declare var /*4*/ point: ...`)
                        if let Some(name_node) = self.arena.get(decl.name) {
                            self.emit_inline_block_comments(name_node.pos);
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
                        } else if !decl.type_annotation.is_some()
                            && self.emit_arrow_fn_type_from_ast(decl.initializer)
                        {
                            // Emitted function type directly from AST
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

                        // Skip comments within the declaration's omitted parts (initializer,
                        // inline type comments) to prevent them from leaking as leading
                        // comments on the next statement.
                        // Use the initializer/type end position as the bound, not the full
                        // declaration's end — the parser may set `end` to include trailing
                        // trivia that extends into the next statement's leading JSDoc comments.
                        {
                            let skip_end = if decl.initializer.is_some() {
                                self.arena.get(decl.initializer).map_or(0, |n| n.end)
                            } else if decl.type_annotation.is_some() {
                                self.arena.get(decl.type_annotation).map_or(0, |n| n.end)
                            } else {
                                self.arena.get(decl.name).map_or(0, |n| n.end)
                            };
                            if skip_end > 0
                                && let Some(dn) = self.arena.get(*decl_idx)
                            {
                                self.skip_comments_in_node(dn.pos, skip_end);
                            }
                            if let Some(init_node) = self.arena.get(decl.initializer)
                                && (init_node.kind == syntax_kind_ext::ARROW_FUNCTION
                                    || init_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION)
                                && let Some(func) = self.arena.get_function(init_node)
                                && func.body.is_some()
                            {
                                let function_decl_end =
                                    self.arena.get(*decl_idx).map_or(init_node.end, |n| n.end);
                                self.skip_comments_before_raw(function_decl_end);
                            }
                        }
                        i += 1;
                    }

                    self.write(";");
                    self.write_line();
                    for (_, decl_idx, decl_node, decl) in &regular_decls[group_start..group_end] {
                        self.emit_js_export_equals_type_alias_namespace_for_name(
                            decl.name,
                            self.arena
                                .get(*decl_idx)
                                .map_or(decl_node.pos, |node| node.pos),
                        );
                    }
                    group_start = group_end;
                }
            }
        }
    }
}
