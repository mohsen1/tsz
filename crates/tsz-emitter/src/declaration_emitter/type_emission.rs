//! Type syntax emission for declaration files.
//!
//! Handles emission of TypeScript type syntax nodes (type references,
//! unions, intersections, mapped types, conditional types, etc.)
//! and entity names (qualified names, property access expressions).

use super::DeclarationEmitter;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Re-escape a cooked template literal string so it can be placed back
/// between backticks.  The parser stores the *cooked* (processed) value in
/// `LiteralData::text`, so characters like `\n` have already been converted
/// to a real newline.  This function converts them back to escape sequences.
fn escape_template_literal_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '`' => out.push_str("\\`"),
            '$' => {
                // Only escape $ when followed by { (but we don't have lookahead here,
                // so just push as-is; actual ${...} is handled structurally)
                out.push('$');
            }
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c => out.push(c),
        }
    }
    out
}

impl<'a> DeclarationEmitter<'a> {
    pub(crate) fn emit_type(&mut self, type_idx: NodeIndex) {
        let Some(type_node) = self.arena.get(type_idx) else {
            return;
        };

        // Skip any non-JSDoc comments that precede this type node.
        // This prevents comments between `:` and the type (e.g.
        // `var x: /** comment */ (a: number) => void`) from leaking
        // into parameter positions.
        self.skip_comments_before(type_node.pos);

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
            // THIS_TYPE is a distinct node kind created by the parser for `this` in type position
            k if k == syntax_kind_ext::THIS_TYPE => self.write("this"),

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
                    // Check if type_node is a meaningful type (not an empty/error placeholder).
                    // All keyword types including `never`, `unknown`, `void`, etc. are valid
                    // type predicate targets (e.g., `asserts x is never`, `asserts x is unknown`).
                    let has_meaningful_type = type_node.is_some_and(|n| {
                        n.kind != 1 // Exclude error recovery nodes only
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
                    let needs_parens = self.arena.get(arr.element_type).is_some_and(|n| {
                        n.kind == syntax_kind_ext::FUNCTION_TYPE
                            || n.kind == syntax_kind_ext::CONSTRUCTOR_TYPE
                            || n.kind == syntax_kind_ext::UNION_TYPE
                            || n.kind == syntax_kind_ext::INTERSECTION_TYPE
                            || n.kind == syntax_kind_ext::CONDITIONAL_TYPE
                            || n.kind == syntax_kind_ext::TYPE_OPERATOR
                            || n.kind == syntax_kind_ext::INFER_TYPE
                    });
                    if needs_parens {
                        self.write("(");
                    }
                    self.emit_type(arr.element_type);
                    if needs_parens {
                        self.write(")");
                    }
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
                        let needs_parens = self.arena.get(type_idx).is_some_and(|n| {
                            n.kind == syntax_kind_ext::FUNCTION_TYPE
                                || n.kind == syntax_kind_ext::CONSTRUCTOR_TYPE
                                || n.kind == syntax_kind_ext::CONDITIONAL_TYPE
                        });
                        if needs_parens {
                            self.write("(");
                        }
                        self.emit_type(type_idx);
                        if needs_parens {
                            self.write(")");
                        }
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
                        // Union and conditional types inside an intersection need parentheses
                        // to preserve operator precedence:
                        // `(A | B) & C` is different from `A | B & C`.
                        // `(A extends B ? C : D) & E` is different from `A extends B ? C : D & E`.
                        let needs_parens = self.arena.get(type_idx).is_some_and(|n| {
                            n.kind == syntax_kind_ext::FUNCTION_TYPE
                                || n.kind == syntax_kind_ext::CONSTRUCTOR_TYPE
                                || n.kind == syntax_kind_ext::UNION_TYPE
                                || n.kind == syntax_kind_ext::CONDITIONAL_TYPE
                        });
                        if needs_parens {
                            self.write("(");
                        }
                        self.emit_type(type_idx);
                        if needs_parens {
                            self.write(")");
                        }
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
                            // Re-escape the cooked text — the parser stores processed
                            // values (e.g., `\n` as a real newline) in `lit.text`.
                            self.write("`");
                            self.write(&escape_template_literal_text(&lit.text));
                            self.write("`");
                        } else {
                            // TemplateHead: text before first substitution
                            self.write("`");
                            self.write(&escape_template_literal_text(&lit.text));
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
                                    self.write(&escape_template_literal_text(&lit.text));
                                    self.write("`");
                                } else {
                                    self.write("}");
                                    self.write(&escape_template_literal_text(&lit.text));
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
                    // Emit the type parameter name
                    if let Some(tp_node) = self.arena.get(infer.type_parameter)
                        && let Some(tp) = self.arena.get_type_parameter(tp_node)
                    {
                        self.emit_node(tp.name);
                        // Emit constraint if present (infer U extends string)
                        if tp.constraint.is_some() {
                            self.write(" extends ");
                            self.emit_type(tp.constraint);
                        }
                    } else {
                        self.emit_node(infer.type_parameter);
                    }
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
                    // Our parser doesn't create ParenthesizedType nodes, so we must
                    // add parens when the operand is a union, intersection, or conditional type
                    // (keyof/readonly bind tighter than |, &, and extends).
                    let needs_parens = self.arena.get(type_op.type_node).is_some_and(|n| {
                        n.kind == syntax_kind_ext::UNION_TYPE
                            || n.kind == syntax_kind_ext::INTERSECTION_TYPE
                            || n.kind == syntax_kind_ext::CONDITIONAL_TYPE
                    });
                    if needs_parens {
                        self.write("(");
                    }
                    self.emit_type(type_op.type_node);
                    if needs_parens {
                        self.write(")");
                    }
                }
            }

            // Literal type wrapper (wraps string/number/boolean/bigint literals)
            k if k == syntax_kind_ext::LITERAL_TYPE => {
                if let Some(lit_type) = self.arena.get_literal_type(type_node) {
                    self.emit_literal_type_inner(lit_type.literal);
                }
            }

            // Literal types
            k if k == SyntaxKind::StringLiteral as u16 => {
                // Delegate to emit_node which handles string escape sequences
                self.emit_node(type_idx);
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(type_node) {
                    // tsc strips numeric separators in .d.ts output and converts
                    // non-decimal literals with separators to their decimal value.
                    if lit.text.contains('_') {
                        if let Some(v) = lit.value {
                            // Use the pre-computed numeric value (handles hex/octal/binary with separators)
                            if v.fract() == 0.0 && v.abs() < 1e20 {
                                self.write(&format!("{}", v as i64));
                            } else {
                                self.write(&v.to_string());
                            }
                        } else {
                            // Fallback: just strip underscores
                            self.write(&lit.text.replace('_', ""));
                        }
                    } else {
                        self.write(&lit.text);
                    }
                }
            }
            k if k == SyntaxKind::BigIntLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(type_node) {
                    // Strip numeric separators in .d.ts output (matching tsc)
                    if lit.text.contains('_') {
                        self.write(&lit.text.replace('_', ""));
                    } else {
                        self.write(&lit.text);
                    }
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
                            || n.kind == syntax_kind_ext::CONDITIONAL_TYPE
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

            // Mapped type - tsc emits multi-line:
            //   {
            //       [P in keyof T]: Type;
            //   }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                if let Some(mapped_type) = self.arena.get_mapped_type(type_node) {
                    self.write("{");
                    self.write_line();
                    self.increase_indent();
                    self.write_indent();

                    // Emit readonly modifier if present (inside the braces)
                    // Token kind determines the prefix: +readonly, -readonly, or readonly
                    if let Some(readonly_node) = self.arena.get(mapped_type.readonly_token) {
                        match readonly_node.kind {
                            k if k == SyntaxKind::PlusToken as u16 => {
                                self.write("+readonly ");
                            }
                            k if k == SyntaxKind::MinusToken as u16 => {
                                self.write("-readonly ");
                            }
                            _ => {
                                self.write("readonly ");
                            }
                        }
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
                    // Token kind determines the prefix: +?, -?, or ?
                    if let Some(question_node) = self.arena.get(mapped_type.question_token) {
                        match question_node.kind {
                            k if k == SyntaxKind::PlusToken as u16 => {
                                self.write("+?");
                            }
                            k if k == SyntaxKind::MinusToken as u16 => {
                                self.write("-?");
                            }
                            _ => {
                                self.write("?");
                            }
                        }
                    }

                    self.write(": ");

                    // Emit type annotation
                    self.emit_type(mapped_type.type_node);

                    self.write(";");
                    self.write_line();
                    self.decrease_indent();
                    self.write_indent();
                    self.write("}");
                }
            }

            // Conditional type (T extends U ? X : Y)
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                if let Some(conditional) = self.arena.get_conditional_type(type_node) {
                    // check_type needs parens for conditional/function/constructor/union/intersection.
                    // Constructor types need parens because their return type parsing
                    // greedily consumes the `extends` keyword:
                    // `new () => T extends U ? X : Y` parses as
                    // `new () => (T extends U ? X : Y)` without parens.
                    let check_needs_parens =
                        if let Some(node) = self.arena.get(conditional.check_type) {
                            node.kind == syntax_kind_ext::CONDITIONAL_TYPE
                                || node.kind == syntax_kind_ext::FUNCTION_TYPE
                                || node.kind == syntax_kind_ext::CONSTRUCTOR_TYPE
                                || node.kind == syntax_kind_ext::UNION_TYPE
                                || node.kind == syntax_kind_ext::INTERSECTION_TYPE
                        } else {
                            false
                        };

                    if check_needs_parens {
                        self.write("(");
                    }
                    self.emit_type(conditional.check_type);
                    if check_needs_parens {
                        self.write(")");
                    }

                    self.write(" extends ");

                    // extends_type needs parens for conditional types.
                    // Function/constructor types also need parens when their return
                    // type is a conditional (the inner `extends` would be mis-parsed
                    // as the outer conditional's extends clause). The parser doesn't
                    // create PARENTHESIZED_TYPE nodes, so we must add parens here.
                    let extends_needs_parens =
                        if let Some(node) = self.arena.get(conditional.extends_type) {
                            if node.kind == syntax_kind_ext::CONDITIONAL_TYPE {
                                true
                            } else if node.kind == syntax_kind_ext::FUNCTION_TYPE
                                || node.kind == syntax_kind_ext::CONSTRUCTOR_TYPE
                            {
                                // Only parenthesize function/constructor types whose
                                // return type is itself a conditional (contains `extends`)
                                self.function_type_has_conditional_return(node)
                            } else {
                                false
                            }
                        } else {
                            false
                        };

                    if extends_needs_parens {
                        self.write("(");
                    }
                    self.emit_type(conditional.extends_type);
                    if extends_needs_parens {
                        self.write(")");
                    }

                    self.write(" ? ");

                    // true_type and false_type don't need parens —
                    // conditional types are right-associative in the false branch
                    self.emit_type(conditional.true_type);

                    self.write(" : ");

                    self.emit_type(conditional.false_type);
                }
            }

            // Optional type (T? in tuple elements)
            k if k == syntax_kind_ext::OPTIONAL_TYPE => {
                if let Some(wrapped) = self.arena.get_wrapped_type(type_node) {
                    // Parenthesize complex types before `?` to avoid ambiguity
                    let needs_parens = if let Some(inner) = self.arena.get(wrapped.type_node) {
                        inner.kind == syntax_kind_ext::UNION_TYPE
                            || inner.kind == syntax_kind_ext::INTERSECTION_TYPE
                            || inner.kind == syntax_kind_ext::FUNCTION_TYPE
                            || inner.kind == syntax_kind_ext::CONSTRUCTOR_TYPE
                            || inner.kind == syntax_kind_ext::CONDITIONAL_TYPE
                    } else {
                        false
                    };
                    if needs_parens {
                        self.write("(");
                    }
                    self.emit_type(wrapped.type_node);
                    if needs_parens {
                        self.write(")");
                    }
                    self.write("?");
                }
            }
            // Rest type (...T in tuple elements)
            k if k == syntax_kind_ext::REST_TYPE => {
                if let Some(wrapped) = self.arena.get_wrapped_type(type_node) {
                    self.write("...");
                    self.emit_type(wrapped.type_node);
                }
            }
            // Named tuple member (name: T, name?: T, ...name: T)
            k if k == syntax_kind_ext::NAMED_TUPLE_MEMBER => {
                if let Some(member) = self.arena.get_named_tuple_member(type_node) {
                    if member.dot_dot_dot_token {
                        self.write("...");
                    }
                    self.emit_node(member.name);
                    if member.question_token {
                        self.write("?");
                    }
                    self.write(": ");
                    self.emit_type(member.type_node);
                }
            }
            _ => {
                // Fallback: emit as node
                self.emit_node(type_idx);
            }
        }
    }

    /// Emit a `<T1, T2, ...>` type argument list.
    pub(crate) fn emit_type_arguments(&mut self, type_args: &tsz_parser::parser::NodeList) {
        self.write("<");
        for (index, &arg_idx) in type_args.nodes.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            if self.first_type_argument_needs_parentheses(arg_idx, index == 0) {
                self.write("(");
                self.emit_type(arg_idx);
                self.write(")");
                continue;
            }
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
            // import("module") call in import type position
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(node) {
                    self.write("import(");
                    if let Some(ref args) = call.arguments {
                        let mut first = true;
                        for &arg_idx in &args.nodes {
                            if !first {
                                self.write(", ");
                            }
                            first = false;
                            self.emit_import_type_argument(arg_idx);
                        }
                    }
                    self.write(")");
                }
            }
            _ => {}
        }
    }

    /// Check if a function type has a conditional type as its return type.
    fn function_type_has_conditional_return(
        &self,
        func_node: &tsz_parser::parser::node::Node,
    ) -> bool {
        let Some(func) = self.arena.get_function_type(func_node) else {
            return false;
        };
        self.arena
            .get(func.type_annotation)
            .is_some_and(|n| n.kind == syntax_kind_ext::CONDITIONAL_TYPE)
    }

    fn emit_import_type_argument(&mut self, arg_idx: NodeIndex) {
        let Some(arg_node) = self.arena.get(arg_idx) else {
            return;
        };

        if arg_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            self.emit_node(arg_idx);
            return;
        }

        let Some(obj) = self.arena.get_literal_expr(arg_node) else {
            self.emit_node(arg_idx);
            return;
        };

        if obj.elements.nodes.is_empty() {
            self.write("{}");
            return;
        }

        self.write("{ ");
        for (i, &elem_idx) in obj.elements.nodes.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.emit_import_type_object_literal_member(elem_idx);
        }
        self.write(" }");
    }

    fn emit_import_type_object_literal_member(&mut self, elem_idx: NodeIndex) {
        let Some(elem_node) = self.arena.get(elem_idx) else {
            return;
        };

        if let Some(prop) = self.arena.get_property_assignment(elem_node) {
            self.emit_node(prop.name);
            self.write(": ");
            self.emit_import_type_argument(prop.initializer);
            return;
        }

        if let Some(shorthand) = self.arena.get_shorthand_property(elem_node) {
            self.emit_node(shorthand.name);
            if shorthand.equals_token {
                self.write(" = ");
                self.emit_import_type_argument(shorthand.object_assignment_initializer);
            }
            return;
        }

        self.emit_node(elem_idx);
    }

    /// Emit the inner node of a `LITERAL_TYPE`.
    ///
    /// This handles numeric separator stripping for numeric/bigint literals in
    /// type position (tsc strips `_` separators in `.d.ts` output and converts
    /// non-decimal literals with separators to their decimal value).
    /// Other literal kinds (string, boolean) delegate to the normal `emit_node`.
    fn emit_literal_type_inner(&mut self, inner_idx: NodeIndex) {
        let Some(inner_node) = self.arena.get(inner_idx) else {
            return;
        };

        match inner_node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(inner_node) {
                    if lit.text.contains('_') {
                        if let Some(v) = lit.value {
                            if v.fract() == 0.0 && v.abs() < 1e20 {
                                self.write(&format!("{}", v as i64));
                            } else {
                                self.write(&v.to_string());
                            }
                        } else {
                            self.write(&lit.text.replace('_', ""));
                        }
                    } else {
                        self.write(&lit.text);
                    }
                }
            }
            k if k == SyntaxKind::BigIntLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(inner_node) {
                    if lit.text.contains('_') {
                        self.write(&lit.text.replace('_', ""));
                    } else {
                        self.write(&lit.text);
                    }
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                // Negative number literals: -1_000 → -1000
                if let Some(unary) = self.arena.get_unary_expr(inner_node) {
                    if unary.operator == SyntaxKind::MinusToken as u16 {
                        self.write("-");
                    } else if unary.operator == SyntaxKind::PlusToken as u16 {
                        self.write("+");
                    }
                    self.emit_literal_type_inner(unary.operand);
                }
            }
            _ => {
                self.emit_node(inner_idx);
            }
        }
    }
}
