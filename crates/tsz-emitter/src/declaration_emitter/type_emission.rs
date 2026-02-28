//! Type syntax emission for declaration files.
//!
//! Handles emission of TypeScript type syntax nodes (type references,
//! unions, intersections, mapped types, conditional types, etc.)
//! and entity names (qualified names, property access expressions).

use super::DeclarationEmitter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(crate) fn emit_type(&mut self, type_idx: NodeIndex) {
        let Some(type_node) = self.arena.get(type_idx) else {
            return;
        };

        match type_node.kind {
            // Keyword types
            k if k == SyntaxKind::NumberKeyword as u16 => self.write("number"),
            k if k == SyntaxKind::StringKeyword as u16 => self.write("string"),
            k if k == SyntaxKind::BooleanKeyword as u16 => self.write("boolean"),
            k if k == SyntaxKind::VoidKeyword as u16 => self.write("void"),
            k if k == SyntaxKind::AnyKeyword as u16 => self.write("any"),
            k if k == SyntaxKind::UnknownKeyword as u16 => self.write("unknown"),
            k if k == SyntaxKind::NeverKeyword as u16 => self.write("never"),
            k if k == SyntaxKind::NullKeyword as u16 => self.write("null"),
            k if k == SyntaxKind::UndefinedKeyword as u16 => self.write("undefined"),
            k if k == SyntaxKind::ObjectKeyword as u16 => self.write("object"),
            k if k == SyntaxKind::SymbolKeyword as u16 => self.write("symbol"),
            k if k == SyntaxKind::BigIntKeyword as u16 => self.write("bigint"),
            k if k == SyntaxKind::ThisKeyword as u16 => self.write("this"),

            // Type predicate (for type guards and assertion functions)
            k if k == syntax_kind_ext::TYPE_PREDICATE => {
                if let Some(type_pred) = self.arena.get_type_predicate(type_node) {
                    // Emit "asserts" modifier if present
                    if type_pred.asserts_modifier {
                        self.write("asserts ");
                    }
                    // Emit parameter name
                    self.emit_node(type_pred.parameter_name);

                    // For type guards (x is Type) or assertion type guards (asserts x is Type),
                    // emit the "is Type" part. For simple asserts (asserts condition), omit it.
                    let type_node = self.arena.get(type_pred.type_node);
                    // Check if type_node is a meaningful type (not an empty/error placeholder)
                    let has_meaningful_type = type_node.is_some_and(|n| {
                        // Exclude error type and unknown
                        n.kind != SyntaxKind::UnknownKeyword as u16
                            && n.kind != SyntaxKind::NeverKeyword as u16 // Never might be valid
                            && n.kind != 1 // Error type
                    });

                    if has_meaningful_type {
                        self.write(" is ");
                        self.emit_type(type_pred.type_node);
                    }
                }
            }

            // Type reference
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.arena.get_type_ref(type_node) {
                    self.emit_node(type_ref.type_name);
                    if let Some(ref type_args) = type_ref.type_arguments {
                        self.emit_type_arguments(type_args);
                    }
                }
            }

            // Expression with type arguments (heritage clauses)
            k if k == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS => {
                if let Some(expr) = self.arena.get_expr_type_args(type_node) {
                    self.emit_entity_name(expr.expression);
                    if let Some(ref type_args) = expr.type_arguments
                        && !type_args.nodes.is_empty()
                    {
                        self.emit_type_arguments(type_args);
                    }
                }
            }

            // Array type
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(arr) = self.arena.get_array_type(type_node) {
                    self.emit_type(arr.element_type);
                    self.write("[]");
                }
            }

            // Union type
            k if k == syntax_kind_ext::UNION_TYPE => {
                if let Some(union) = self.arena.get_composite_type(type_node) {
                    let mut first = true;
                    for &type_idx in &union.types.nodes {
                        if !first {
                            self.write(" | ");
                        }
                        first = false;
                        self.emit_type(type_idx);
                    }
                }
            }

            // Intersection type
            k if k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(inter) = self.arena.get_composite_type(type_node) {
                    let mut first = true;
                    for &type_idx in &inter.types.nodes {
                        if !first {
                            self.write(" & ");
                        }
                        first = false;
                        self.emit_type(type_idx);
                    }
                }
            }

            // Tuple type
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                if let Some(tuple) = self.arena.get_tuple_type(type_node) {
                    self.write("[");
                    let mut first = true;
                    for &elem_idx in &tuple.elements.nodes {
                        if !first {
                            self.write(", ");
                        }
                        first = false;
                        self.emit_type(elem_idx);
                    }
                    self.write("]");
                }
            }

            // Function type
            k if k == syntax_kind_ext::FUNCTION_TYPE => {
                if let Some(func) = self.arena.get_function_type(type_node) {
                    if let Some(ref type_params) = func.type_parameters {
                        self.emit_type_parameters(type_params);
                    }
                    self.write("(");
                    self.emit_parameters(&func.parameters);
                    self.write(") => ");
                    self.emit_type(func.type_annotation);
                }
            }

            // Constructor type: `new (...) => T`
            k if k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                if let Some(func) = self.arena.get_function_type(type_node) {
                    if func.is_abstract {
                        self.write("abstract ");
                    }
                    self.write("new ");
                    if let Some(ref type_params) = func.type_parameters {
                        self.emit_type_parameters(type_params);
                    }
                    self.write("(");
                    self.emit_parameters(&func.parameters);
                    self.write(") => ");
                    self.emit_type(func.type_annotation);
                }
            }

            // Template literal type: `` `prefix${T}suffix` ``
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                if let Some(tlt) = self.arena.get_template_literal_type(type_node) {
                    // Emit the head text (includes opening backtick + text before first `${`)
                    if let Some(head_node) = self.arena.get(tlt.head)
                        && let Some(lit) = self.arena.get_literal(head_node)
                    {
                        if head_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16 {
                            self.write("`");
                            self.write(&lit.text);
                            self.write("`");
                        } else {
                            // TemplateHead: text before first substitution
                            self.write("`");
                            self.write(&lit.text);
                            self.write("${");
                        }
                    }
                    // Emit each span: type + middle/tail literal
                    for (i, &span_idx) in tlt.template_spans.nodes.iter().enumerate() {
                        if let Some(span_node) = self.arena.get(span_idx)
                            && let Some(span) = self.arena.get_template_span(span_node)
                        {
                            // Emit the type inside ${...}
                            self.emit_type(span.expression);
                            // Emit the literal part (TemplateMiddle or TemplateTail)
                            if let Some(lit_node) = self.arena.get(span.literal)
                                && let Some(lit) = self.arena.get_literal(lit_node)
                            {
                                let is_last = i == tlt.template_spans.nodes.len() - 1;
                                if is_last {
                                    self.write("}");
                                    self.write(&lit.text);
                                    self.write("`");
                                } else {
                                    self.write("}");
                                    self.write(&lit.text);
                                    self.write("${");
                                }
                            }
                        }
                    }
                }
            }

            // Infer type: `infer U`
            k if k == syntax_kind_ext::INFER_TYPE => {
                if let Some(infer) = self.arena.get_infer_type(type_node) {
                    self.write("infer ");
                    self.emit_node(infer.type_parameter);
                }
            }

            // Type literal - multi-line format with proper indentation
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(lit) = self.arena.get_type_literal(type_node) {
                    // Filter out members with non-emittable computed property names
                    let emittable_members: Vec<_> = lit
                        .members
                        .nodes
                        .iter()
                        .copied()
                        .filter(|&idx| !self.member_has_non_emittable_computed_name(idx))
                        .collect();

                    if emittable_members.is_empty() {
                        self.write("{}");
                    } else {
                        self.write("{\n");
                        self.increase_indent();
                        for member_idx in emittable_members {
                            self.write_indent();
                            self.emit_interface_member_inline(member_idx);
                            self.write(";");
                            self.write_line();
                        }
                        self.decrease_indent();
                        self.write_indent();
                        self.write("}");
                    }
                }
            }

            // Parenthesized type
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
                if let Some(paren) = self.arena.get_wrapped_type(type_node) {
                    self.write("(");
                    self.emit_type(paren.type_node);
                    self.write(")");
                }
            }

            // Type query (typeof)
            k if k == syntax_kind_ext::TYPE_QUERY => {
                self.write("typeof ");
                if let Some(type_query) = self.arena.get_type_query(type_node) {
                    self.emit_entity_name(type_query.expr_name);

                    // Handle type arguments (TS 4.7+)
                    if let Some(ref type_args) = type_query.type_arguments
                        && !type_args.nodes.is_empty()
                    {
                        self.emit_type_arguments(type_args);
                    }
                }
            }

            // Type operator (keyof, readonly, unique)
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                if let Some(type_op) = self.arena.get_type_operator(type_node) {
                    // Check the operator kind
                    if type_op.operator == SyntaxKind::KeyOfKeyword as u16 {
                        self.write("keyof ");
                    } else if type_op.operator == SyntaxKind::ReadonlyKeyword as u16 {
                        self.write("readonly ");
                    } else if type_op.operator == SyntaxKind::UniqueKeyword as u16 {
                        self.write("unique ");
                    }
                    self.emit_type(type_op.type_node);
                }
            }

            // Literal type wrapper (wraps string/number/boolean/bigint literals)
            k if k == syntax_kind_ext::LITERAL_TYPE => {
                if let Some(lit_type) = self.arena.get_literal_type(type_node) {
                    self.emit_node(lit_type.literal);
                }
            }

            // Literal types
            k if k == SyntaxKind::StringLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(type_node) {
                    let quote = self.original_quote_char(type_node);
                    self.write(quote);
                    self.write(&lit.text);
                    self.write(quote);
                }
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(type_node) {
                    self.write(&lit.text);
                }
            }
            k if k == SyntaxKind::TrueKeyword as u16 => self.write("true"),
            k if k == SyntaxKind::FalseKeyword as u16 => self.write("false"),

            // Indexed access type (T[K])
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                if let Some(indexed_access) = self.arena.get_indexed_access_type(type_node) {
                    // Check if object type needs parentheses for precedence
                    let obj_node = self.arena.get(indexed_access.object_type);
                    let needs_parens = obj_node.is_some_and(|n| {
                        n.kind == syntax_kind_ext::UNION_TYPE
                            || n.kind == syntax_kind_ext::INTERSECTION_TYPE
                            || n.kind == syntax_kind_ext::FUNCTION_TYPE
                    });

                    if needs_parens {
                        self.write("(");
                    }
                    self.emit_type(indexed_access.object_type);
                    if needs_parens {
                        self.write(")");
                    }

                    self.write("[");
                    self.emit_type(indexed_access.index_type);
                    self.write("]");
                }
            }

            // Mapped type
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                if let Some(mapped_type) = self.arena.get_mapped_type(type_node) {
                    self.write("{ ");

                    // Emit readonly modifier if present (inside the braces)
                    if mapped_type.readonly_token.is_some() {
                        self.write("readonly ");
                    }

                    self.write("[");

                    // Get the TypeParameter data
                    if let Some(type_param_node) = self.arena.get(mapped_type.type_parameter)
                        && let Some(type_param) = self.arena.get_type_parameter(type_param_node)
                    {
                        // Emit the parameter name (e.g., "P")
                        self.emit_node(type_param.name);

                        // Emit " in "
                        self.write(" in ");

                        // Emit the constraint (e.g., "keyof T")
                        if type_param.constraint.is_some() {
                            self.emit_type(type_param.constraint);
                        }
                    }

                    // Handle the optional 'as' clause (key remapping)
                    if mapped_type.name_type.is_some() {
                        self.write(" as ");
                        self.emit_type(mapped_type.name_type);
                    }

                    self.write("]");

                    // Optionally emit question token (after the bracket)
                    if mapped_type.question_token.is_some() {
                        self.write("?");
                    }

                    self.write(": ");

                    // Emit type annotation
                    self.emit_type(mapped_type.type_node);

                    self.write("; }");
                }
            }

            // Conditional type (T extends U ? X : Y)
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                if let Some(conditional) = self.arena.get_conditional_type(type_node) {
                    // Helper function to check if a type needs parentheses
                    let needs_parens = |type_idx: NodeIndex| -> bool {
                        if let Some(node) = self.arena.get(type_idx) {
                            // Types with lower or equal precedence need parentheses
                            node.kind == syntax_kind_ext::CONDITIONAL_TYPE
                                || node.kind == syntax_kind_ext::FUNCTION_TYPE
                                || node.kind == syntax_kind_ext::UNION_TYPE
                                || node.kind == syntax_kind_ext::INTERSECTION_TYPE
                        } else {
                            false
                        }
                    };

                    // Emit check_type (with parens if needed)
                    if needs_parens(conditional.check_type) {
                        self.write("(");
                    }
                    self.emit_type(conditional.check_type);
                    if needs_parens(conditional.check_type) {
                        self.write(")");
                    }

                    self.write(" extends ");

                    // Emit extends_type (with parens if needed)
                    if needs_parens(conditional.extends_type) {
                        self.write("(");
                    }
                    self.emit_type(conditional.extends_type);
                    if needs_parens(conditional.extends_type) {
                        self.write(")");
                    }

                    self.write(" ? ");

                    // Emit true_type (with parens if needed)
                    if needs_parens(conditional.true_type) {
                        self.write("(");
                    }
                    self.emit_type(conditional.true_type);
                    if needs_parens(conditional.true_type) {
                        self.write(")");
                    }

                    self.write(" : ");

                    // Emit false_type (with parens if needed)
                    if needs_parens(conditional.false_type) {
                        self.write("(");
                    }
                    self.emit_type(conditional.false_type);
                    if needs_parens(conditional.false_type) {
                        self.write(")");
                    }
                }
            }

            _ => {
                // Fallback: emit as node
                self.emit_node(type_idx);
            }
        }
    }

    /// Emit a `<T1, T2, ...>` type argument list.
    fn emit_type_arguments(&mut self, type_args: &tsz_parser::parser::NodeList) {
        self.write("<");
        let mut first = true;
        for &arg_idx in &type_args.nodes {
            if !first {
                self.write(", ");
            }
            first = false;
            self.emit_type(arg_idx);
        }
        self.write(">");
    }

    pub(crate) fn emit_entity_name(&mut self, node_idx: NodeIndex) {
        let Some(node) = self.arena.get(node_idx) else {
            return;
        };

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(ident) = self.arena.get_identifier(node) {
                    self.write(&ident.escaped_text);
                }
            }
            k if k == SyntaxKind::ThisKeyword as u16 => self.write("this"),
            k if k == SyntaxKind::SuperKeyword as u16 => self.write("super"),
            k if k == syntax_kind_ext::TYPE_PARAMETER => {
                // Type parameter reference (e.g., T in mapped types)
                if let Some(param) = self.arena.get_type_parameter(node) {
                    self.emit_node(param.name);
                }
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                // Type reference in mapped type name position
                if let Some(type_ref) = self.arena.get_type_ref(node) {
                    self.emit_node(type_ref.type_name);
                }
            }
            k if k == syntax_kind_ext::QUALIFIED_NAME => {
                if let Some(name) = self.arena.get_qualified_name(node) {
                    self.emit_entity_name(name.left);
                    self.write(".");
                    self.emit_entity_name(name.right);
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    self.emit_entity_name(access.expression);
                    self.write(".");
                    self.emit_entity_name(access.name_or_argument);
                }
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    self.emit_entity_name(access.expression);
                    self.write("[");
                    self.emit_node(access.name_or_argument);
                    self.write("]");
                }
            }
            _ => {}
        }
    }
}
