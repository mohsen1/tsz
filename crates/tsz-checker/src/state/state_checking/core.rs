//! Core declaration checking utilities.
//!
//! Contains `declaration_symbol_flags` and isolated-declaration helpers
//! that don't belong to a more specific responsibility slice.

use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> CheckerState<'a> {
    pub(crate) fn declaration_symbol_flags(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        decl_idx: NodeIndex,
    ) -> Option<u32> {
        use tsz_parser::parser::node_flags;

        let decl_idx = self.resolve_duplicate_decl_node(arena, decl_idx)?;
        let node = arena.get(decl_idx)?;

        match node.kind {
            syntax_kind_ext::VARIABLE_DECLARATION => {
                let decl_flags = arena.get_variable_declaration_flags(decl_idx);
                if (decl_flags & (node_flags::LET | node_flags::CONST)) != 0 {
                    Some(symbol_flags::BLOCK_SCOPED_VARIABLE)
                } else {
                    Some(symbol_flags::FUNCTION_SCOPED_VARIABLE)
                }
            }
            syntax_kind_ext::FUNCTION_DECLARATION => Some(symbol_flags::FUNCTION),
            syntax_kind_ext::CLASS_DECLARATION => Some(symbol_flags::CLASS),
            syntax_kind_ext::INTERFACE_DECLARATION => Some(symbol_flags::INTERFACE),
            syntax_kind_ext::TYPE_ALIAS_DECLARATION => Some(symbol_flags::TYPE_ALIAS),
            syntax_kind_ext::ENUM_DECLARATION => {
                // Check if this is a const enum by looking for const modifier
                let is_const_enum = arena
                    .get_enum(node)
                    .and_then(|enum_decl| enum_decl.modifiers.as_ref())
                    .is_some_and(|modifiers| {
                        modifiers.nodes.iter().any(|&mod_idx| {
                            arena.get(mod_idx).is_some_and(|mod_node| {
                                mod_node.kind == tsz_scanner::SyntaxKind::ConstKeyword as u16
                            })
                        })
                    });
                if is_const_enum {
                    Some(symbol_flags::CONST_ENUM)
                } else {
                    Some(symbol_flags::REGULAR_ENUM)
                }
            }
            syntax_kind_ext::MODULE_DECLARATION => {
                // Namespaces (module declarations) can merge with functions, classes, enums
                Some(symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE)
            }
            syntax_kind_ext::GET_ACCESSOR => {
                let mut flags = symbol_flags::GET_ACCESSOR;
                if let Some(accessor) = arena.get_accessor(node)
                    && arena.has_modifier(&accessor.modifiers, SyntaxKind::StaticKeyword)
                {
                    flags |= symbol_flags::STATIC;
                }
                Some(flags)
            }
            syntax_kind_ext::SET_ACCESSOR => {
                let mut flags = symbol_flags::SET_ACCESSOR;
                if let Some(accessor) = arena.get_accessor(node)
                    && arena.has_modifier(&accessor.modifiers, SyntaxKind::StaticKeyword)
                {
                    flags |= symbol_flags::STATIC;
                }
                Some(flags)
            }
            syntax_kind_ext::METHOD_DECLARATION => {
                let mut flags = symbol_flags::METHOD;
                if let Some(method) = arena.get_method_decl(node)
                    && arena.has_modifier(&method.modifiers, SyntaxKind::StaticKeyword)
                {
                    flags |= symbol_flags::STATIC;
                }
                Some(flags)
            }
            syntax_kind_ext::PROPERTY_DECLARATION => {
                let mut flags = symbol_flags::PROPERTY;
                if let Some(prop) = arena.get_property_decl(node)
                    && arena.has_modifier(&prop.modifiers, SyntaxKind::StaticKeyword)
                {
                    flags |= symbol_flags::STATIC;
                }
                Some(flags)
            }
            syntax_kind_ext::CONSTRUCTOR => Some(symbol_flags::CONSTRUCTOR),
            syntax_kind_ext::TYPE_PARAMETER => Some(symbol_flags::TYPE_PARAMETER),
            syntax_kind_ext::PARAMETER => Some(symbol_flags::FUNCTION_SCOPED_VARIABLE),
            syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            | syntax_kind_ext::IMPORT_CLAUSE
            | syntax_kind_ext::NAMESPACE_IMPORT
            | syntax_kind_ext::IMPORT_SPECIFIER => Some(symbol_flags::ALIAS),
            syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION => {
                // 'export as namespace X' creates a UMD global alias to the module.
                // In tsc, AliasExcludes = Alias — aliases only conflict with other
                // aliases. They merge freely with namespaces, classes, functions, etc.
                // Using ALIAS here (matching the binder) prevents false TS2451 when
                // `export as namespace React` coexists with `declare namespace React`.
                Some(symbol_flags::ALIAS)
            }
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn report_isolated_decl_computed_property_names(
        &mut self,
        decl_idx: NodeIndex,
        init_idx: NodeIndex,
    ) -> bool {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = init_idx;
        loop {
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };

            match node.kind {
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    let Some(paren) = self.ctx.arena.get_parenthesized(node) else {
                        return false;
                    };
                    current = paren.expression;
                }
                k if k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::SATISFIES_EXPRESSION
                    || k == syntax_kind_ext::TYPE_ASSERTION =>
                {
                    let Some(assertion) = self.ctx.arena.get_type_assertion(node) else {
                        return false;
                    };
                    current = assertion.expression;
                }
                k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                    break;
                }
                _ => return false,
            }
        }

        let Some(obj_node) = self.ctx.arena.get(current) else {
            return false;
        };
        let Some(obj) = self.ctx.arena.get_literal_expr(obj_node) else {
            return false;
        };

        let var_name = self
            .ctx
            .arena
            .get(decl_idx)
            .and_then(|node| self.ctx.arena.get_variable_declaration(node))
            .and_then(|decl| self.ctx.arena.get_identifier_at(decl.name))
            .map(|ident| ident.escaped_text.clone())
            .unwrap_or_default();

        let mut reported = false;
        for &elem_idx in &obj.elements.nodes {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };

            let computed_name =
                if let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) {
                    Some(prop.name)
                } else if let Some(method) = self.ctx.arena.get_method_decl(elem_node) {
                    Some(method.name)
                } else {
                    self.ctx
                        .arena
                        .get_accessor(elem_node)
                        .map(|accessor| accessor.name)
                };

            let Some(name_idx) = computed_name else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(name_idx) else {
                continue;
            };
            if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                continue;
            }
            if self.is_isolated_decl_simple_computed_name(name_idx) {
                continue;
            }
            self.report_isolated_decl_computed_name_dependency(name_idx);

            self.error_at_node(
                name_idx,
                diagnostic_messages::COMPUTED_PROPERTY_NAMES_ON_CLASS_OR_OBJECT_LITERALS_CANNOT_BE_INFERRED_WITH_ISOL,
                diagnostic_codes::COMPUTED_PROPERTY_NAMES_ON_CLASS_OR_OBJECT_LITERALS_CANNOT_BE_INFERRED_WITH_ISOL,
            );

            if let Some((start, length)) = self.get_node_span(name_idx) {
                let related = crate::diagnostics::DiagnosticRelatedInformation {
                    category: crate::diagnostics::DiagnosticCategory::Message,
                    code: diagnostic_codes::ADD_A_TYPE_ANNOTATION_TO_THE_VARIABLE,
                    file: self.ctx.file_name.clone(),
                    start,
                    length,
                    message_text: crate::diagnostics::format_message(
                        diagnostic_messages::ADD_A_TYPE_ANNOTATION_TO_THE_VARIABLE,
                        &[&var_name],
                    ),
                };
                if let Some(last) = self.ctx.diagnostics.last_mut() {
                    last.related_information.push(related);
                }
            }

            reported = true;
        }

        reported
    }

    #[allow(dead_code)]
    fn is_isolated_decl_simple_computed_name(&self, name_idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }
        let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
            return false;
        };
        let Some(expr_node) = self.ctx.arena.get(computed.expression) else {
            return false;
        };

        match expr_node.kind {
            k if k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                true
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => self
                .ctx
                .arena
                .get_unary_expr(expr_node)
                .is_some_and(|unary| {
                    (unary.operator == SyntaxKind::MinusToken as u16
                        || unary.operator == SyntaxKind::PlusToken as u16)
                        && self.ctx.arena.get(unary.operand).is_some_and(|operand| {
                            operand.kind == SyntaxKind::NumericLiteral as u16
                                || operand.kind == SyntaxKind::BigIntLiteral as u16
                        })
                }),
            _ => false,
        }
    }

    #[allow(dead_code)]
    fn report_isolated_decl_computed_name_dependency(&mut self, name_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::syntax_kind_ext;

        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return;
        }
        let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
            return;
        };
        let Some(expr_node) = self.ctx.arena.get(computed.expression) else {
            return;
        };
        if expr_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return;
        }

        let Some(sym_id) = self.resolve_identifier_symbol(computed.expression) else {
            return;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return;
        };
        if symbol.flags & symbol_flags::VARIABLE == 0 || !symbol.value_declaration.is_some() {
            return;
        }
        let Some(decl_node) = self.ctx.arena.get(symbol.value_declaration) else {
            return;
        };
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return;
        };
        if var_decl.type_annotation.is_some() || var_decl.initializer.is_none() {
            return;
        }
        if self.is_isolated_decl_type_inferrable(var_decl.initializer) {
            return;
        }

        self.error_at_node(
            var_decl.name,
            diagnostic_messages::VARIABLE_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
            diagnostic_codes::VARIABLE_MUST_HAVE_AN_EXPLICIT_TYPE_ANNOTATION_WITH_ISOLATEDDECLARATIONS,
        );
    }

    /// Check whether an initializer's type is inferrable for `--isolatedDeclarations`.
    ///
    /// tsc suppresses TS9010 for:
    /// - `const` with literal initializers (type is the literal type)
    /// - Object/array literals (tsc emits per-property `TS9xxx` diagnostics instead)
    /// - Arrow/function/class expressions (tsc emits TS9007/TS9011 for signature gaps)
    /// - `as const` / `satisfies` assertions
    pub(crate) fn is_isolated_decl_type_inferrable(&self, init: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let Some(init_node) = self.ctx.arena.get(init) else {
            return false;
        };
        match init_node.kind {
            // Direct literal values
            k if k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::BigIntLiteral as u16
                || k == SyntaxKind::TrueKeyword as u16
                || k == SyntaxKind::FalseKeyword as u16
                || k == SyntaxKind::NullKeyword as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                true
            }
            // Prefix unary +/- on numeric or bigint literal
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.ctx.arena.get_unary_expr(init_node)
                    && (unary.operator == SyntaxKind::MinusToken as u16
                        || unary.operator == SyntaxKind::PlusToken as u16)
                    && let Some(operand) = self.ctx.arena.get(unary.operand)
                {
                    operand.kind == SyntaxKind::NumericLiteral as u16
                        || operand.kind == SyntaxKind::BigIntLiteral as u16
                } else {
                    false
                }
            }
            // `expr as const` or `expr as Type`
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                // For `as const` with template expressions: NOT inferrable
                // (literal type requires evaluation). Everything else is inferrable.
                if let Some(assertion) = self.ctx.arena.get_type_assertion(init_node) {
                    let is_const = self
                        .ctx
                        .arena
                        .get(assertion.type_node)
                        .is_some_and(|tn| tn.kind == SyntaxKind::ConstKeyword as u16);
                    if is_const {
                        // `as const` with template expression → NOT inferrable
                        let inner = self.ctx.arena.get(assertion.expression);
                        inner.is_none_or(|n| n.kind != syntax_kind_ext::TEMPLATE_EXPRESSION)
                    } else {
                        true
                    }
                } else {
                    true
                }
            }
            // Parenthesized expression — check inner
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(init_node) {
                    self.is_isolated_decl_type_inferrable(paren.expression)
                } else {
                    false
                }
            }
            // Object/array literal — tsc can infer types from the literal shape
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION =>
            {
                true
            }
            // Arrow functions and function expressions — types come from the signature,
            // so tsc emits more specific TS9007/TS9011 instead of TS9010.
            k if k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::CLASS_EXPRESSION =>
            {
                true
            }
            _ => false,
        }
    }
}
