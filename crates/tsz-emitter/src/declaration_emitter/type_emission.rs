//! Type syntax emission for declaration files.
//!
//! Handles emission of TypeScript type syntax nodes (type references,
//! unions, intersections, mapped types, conditional types, etc.)
//! and entity names (qualified names, property access expressions).

use super::DeclarationEmitter;
use super::helpers::{escape_string_for_double_quote, escape_string_for_single_quote};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

/// Re-escape a cooked template literal string so it can be placed back
/// between backticks.  The parser stores the *cooked* (processed) value in
/// `LiteralData::text`, so characters like `\n` have already been converted
/// to a real newline.  This function converts them back to escape sequences.
fn escape_template_literal_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '`' => out.push_str("\\`"),
            '$' if chars.peek() == Some(&'{') => out.push_str("\\$"),
            '$' => out.push('$'),
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
                    if self.entity_name_contains_import_call(type_ref.type_name) {
                        self.emit_entity_name(type_ref.type_name);
                    } else {
                        self.emit_node(type_ref.type_name);
                    }
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
                    let multiline_named_tuple_union = self.indent_level > 0
                        && union
                            .types
                            .nodes
                            .iter()
                            .all(|&type_idx| self.is_named_tuple_type_node(type_idx));
                    let mut first = true;
                    for &type_idx in &union.types.nodes {
                        if !first {
                            self.write(" | ");
                        }
                        first = false;
                        if multiline_named_tuple_union {
                            self.emit_named_tuple_type_multiline(type_idx);
                            continue;
                        }
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
                    // Drop intersection arms whose source slice contains an
                    // import-attribute object literal that the parser
                    // recovered with non-property entries (e.g.
                    // `{ with: {1234, "resolution-mode": "import"} }` —
                    // the bare numeric literal as a "key"). tsc treats
                    // those import-types as unrecoverable and elides the
                    // entire arm. Scan the AST source slice for the
                    // recognised broken pattern, since the arm's
                    // ObjectLiteral node has been emptied during parse
                    // recovery and the broken text only survives in the
                    // raw source span.
                    let usable: Vec<NodeIndex> = inter
                        .types
                        .nodes
                        .iter()
                        .copied()
                        .filter(|&type_idx| {
                            !self.intersection_arm_source_has_broken_import_attrs(type_idx)
                        })
                        .collect();
                    let arms: Vec<NodeIndex> = if usable.is_empty() {
                        // Every arm is unrecoverable — tsc emits just the
                        // first arm (which the parser already cleaned up
                        // attribute-side) and drops the rest. Mirror that
                        // by keeping only the first arm.
                        inter
                            .types
                            .nodes
                            .first()
                            .copied()
                            .map(|first_arm| vec![first_arm])
                            .unwrap_or_default()
                    } else {
                        usable
                    };
                    let mut first = true;
                    for type_idx in arms {
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
                    // tsc preserves JSDoc comments inline before tuple
                    // members (commonly seen on named tuple types like
                    // `[/** size */ length: number, /** count */ count:
                    // number]`) by emitting the tuple in multi-line form.
                    // Only switch to multi-line when at least one element
                    // has a leading JSDoc comment — otherwise tsc's
                    // d.ts keeps the compact one-line shape.
                    if self
                        .tuple_type_has_jsdoc_leading_member(type_node.pos, &tuple.elements.nodes)
                    {
                        self.emit_tuple_type_multiline(type_idx);
                    } else {
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
                    // Type operators print with the minimal parentheses needed
                    // for precedence. Source-only parentheses around a stronger
                    // operand such as `A["a"]` are not retained by tsc.
                    let (operand, needs_parens) =
                        self.type_operator_operand_and_parens(type_op.type_node);
                    if needs_parens {
                        self.write("(");
                    }
                    self.emit_type(operand);
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
                self.emit_string_literal_type(type_node);
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
                    let multiline_variadic_tuple = obj_node.is_some_and(|n| {
                        n.kind == syntax_kind_ext::TUPLE_TYPE
                            && self.tuple_type_should_break_multiline(indexed_access.object_type)
                    });
                    let needs_parens = obj_node.is_some_and(|n| {
                        n.kind == syntax_kind_ext::UNION_TYPE
                            || n.kind == syntax_kind_ext::INTERSECTION_TYPE
                            || n.kind == syntax_kind_ext::FUNCTION_TYPE
                            || n.kind == syntax_kind_ext::CONDITIONAL_TYPE
                            || n.kind == syntax_kind_ext::TYPE_QUERY
                    });

                    if needs_parens {
                        self.write("(");
                    }
                    if multiline_variadic_tuple {
                        self.emit_tuple_type_multiline(indexed_access.object_type);
                    } else {
                        self.emit_type(indexed_access.object_type);
                    }
                    if needs_parens {
                        self.write(")");
                    }

                    self.write("[");
                    self.emit_indexed_access_index_type(indexed_access.index_type, false);
                    self.write("]");
                }
            }

            // Mapped type - tsc emits multi-line:
            //   {
            //       [P in keyof T]: Type;
            //   }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                if let Some(expanded) = self.expand_mapped_type_to_portable_properties(type_idx) {
                    self.write(&expanded);
                    return;
                }

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
                            self.emit_mapped_type_constraint(type_param.constraint);
                        }
                    }

                    // Handle the optional 'as' clause (key remapping)
                    if mapped_type.name_type.is_some() {
                        self.emit_mapped_type_as_clause(mapped_type.name_type);
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
                    self.emit_mapped_type_value_type(mapped_type.type_node);

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

                    // extends_type needs parens for nested conditional
                    // types only.  tsc does *not* parenthesise
                    // `FUNCTION_TYPE`/`CONSTRUCTOR_TYPE` here, even when
                    // their body contains a conditional — the outer
                    // conditional's `?`/`:` always terminates the
                    // function body's greedy parse, so adding parens
                    // diverges from tsc's d.ts (e.g. round-tripping
                    // `(<T>() => …) extends <T>() => T extends Y ? 1 : 2 ? A : B`).
                    let extends_needs_parens = self
                        .arena
                        .get(conditional.extends_type)
                        .is_some_and(|node| node.kind == syntax_kind_ext::CONDITIONAL_TYPE);

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
                    // OPTIONAL_TYPE wrapping a REST_TYPE represents the
                    // (invalid) `[...T?]` tuple form. tsc parses this and
                    // displays it as `[...?T]` in declaration emit, so emit
                    // the rest prefix followed by `?` then the inner type.
                    if let Some(inner_node) = self.arena.get(wrapped.type_node)
                        && inner_node.kind == syntax_kind_ext::REST_TYPE
                        && let Some(inner_wrapped) = self.arena.get_wrapped_type(inner_node)
                    {
                        self.write("...?");
                        self.emit_type(inner_wrapped.type_node);
                        return;
                    }
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

    fn is_named_tuple_type_node(&self, type_idx: NodeIndex) -> bool {
        self.arena.get(type_idx).is_some_and(|node| {
            node.kind == syntax_kind_ext::TUPLE_TYPE
                && self.arena.get_tuple_type(node).is_some_and(|tuple| {
                    tuple.elements.nodes.iter().any(|&elem_idx| {
                        self.arena
                            .get(elem_idx)
                            .is_some_and(|n| n.kind == syntax_kind_ext::NAMED_TUPLE_MEMBER)
                    })
                })
        })
    }

    /// Emit any JSDoc comments preceding a tuple element, each on its
    /// own indented line.  Used by `emit_tuple_type_multiline` so the
    /// `[/** … */ length: number, …]` shape survives d.ts emit.
    /// `lower_bound` excludes comments before the previous element's
    /// end so a leading comment on element 0 doesn't get duplicated on
    /// element 1.
    fn emit_tuple_member_jsdoc_comments(&mut self, lower_bound: u32, elem_pos: u32) {
        if self.remove_comments {
            return;
        }
        let Some(text) = self.source_file_text.as_ref().map(|s| s.clone()) else {
            return;
        };
        let bytes = text.as_bytes();
        let mut actual_start = elem_pos as usize;
        while actual_start < bytes.len()
            && matches!(bytes[actual_start], b' ' | b'\t' | b'\r' | b'\n')
        {
            actual_start += 1;
        }
        let actual_start_u32 = actual_start as u32;
        let mut comments: Vec<(u32, u32)> = Vec::new();
        for comment in &self.all_comments {
            if comment.pos < lower_bound
                || comment.end > actual_start_u32
                || comment.pos >= elem_pos
            {
                continue;
            }
            let raw = &text[comment.pos as usize..comment.end as usize];
            if raw.starts_with("/**") && raw != "/**/" {
                comments.push((comment.pos, comment.end));
            }
        }
        for (c_pos, c_end) in comments {
            self.write_indent();
            let raw = &text[c_pos as usize..c_end as usize];
            // Re-indent multi-line JSDoc bodies to align with the current
            // output indentation, mirroring how parameter/property
            // JSDoc blocks are re-emitted elsewhere in the d.ts pipeline.
            // Trim trailing whitespace from each emitted line so source-
            // style `/** ` (with trailing space before the newline)
            // collapses to `/**`, matching tsc's d.ts output.
            let mut first = true;
            for line in raw.split('\n') {
                if !first {
                    self.write_line();
                    self.write_indent();
                    let trimmed = line.trim_start().trim_end();
                    if !trimmed.is_empty() {
                        self.write(" ");
                        self.write(trimmed);
                    }
                } else {
                    self.write(line.trim_start().trim_end());
                    first = false;
                }
            }
            self.write_line();
        }
    }

    /// Whether any element of a tuple type has a JSDoc comment
    /// immediately preceding it.  Used to switch the `TUPLE_TYPE` printer
    /// into the multi-line shape so the comments survive d.ts emit.
    /// `tuple_pos` lower-bounds the comment search so unrelated JSDoc
    /// blocks earlier in the source file aren't mis-attributed.
    fn tuple_type_has_jsdoc_leading_member(&self, tuple_pos: u32, elements: &[NodeIndex]) -> bool {
        let Some(text) = self.source_file_text.as_deref() else {
            return false;
        };
        let bytes = text.as_bytes();
        elements.iter().copied().any(|elem_idx| {
            let Some(elem_node) = self.arena.get(elem_idx) else {
                return false;
            };
            let mut actual_start = elem_node.pos as usize;
            while actual_start < bytes.len()
                && matches!(bytes[actual_start], b' ' | b'\t' | b'\r' | b'\n')
            {
                actual_start += 1;
            }
            let actual_start_u32 = actual_start as u32;
            self.all_comments.iter().any(|comment| {
                if comment.pos < tuple_pos
                    || comment.end > actual_start_u32
                    || comment.pos >= elem_node.pos
                {
                    return false;
                }
                let raw = &text[comment.pos as usize..comment.end as usize];
                raw.starts_with("/**") && raw != "/**/"
            })
        })
    }

    fn tuple_type_should_break_multiline(&self, tuple_idx: NodeIndex) -> bool {
        self.arena
            .get(tuple_idx)
            .filter(|node| node.kind == syntax_kind_ext::TUPLE_TYPE)
            .and_then(|node| self.arena.get_tuple_type(node))
            .is_some_and(|tuple| {
                tuple.elements.nodes.len() > 1
                    && tuple.elements.nodes.iter().any(|&elem_idx| {
                        self.arena
                            .get(elem_idx)
                            .is_some_and(|elem| elem.kind == syntax_kind_ext::REST_TYPE)
                    })
            })
    }

    fn type_argument_tuple_should_preserve_multiline(&self, type_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(type_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::TUPLE_TYPE {
            return false;
        }
        let Some(text) = self.source_file_text.as_deref() else {
            return false;
        };
        let start = node.pos as usize;
        let end = node.end as usize;
        if start >= end || end > text.len() {
            return false;
        }

        let raw = &text[start..end];
        let tuple_text = raw.find('[').map_or(raw, |index| &raw[index..]);
        tuple_text.contains('\n')
    }

    fn emit_tuple_type_multiline(&mut self, tuple_idx: NodeIndex) {
        let Some(tuple_node) = self.arena.get(tuple_idx) else {
            self.emit_type(tuple_idx);
            return;
        };
        let Some(tuple) = self.arena.get_tuple_type(tuple_node) else {
            self.emit_type(tuple_idx);
            return;
        };
        self.write("[");
        self.write_line();
        self.increase_indent();
        let mut previous_elem_end: u32 = tuple_node.pos;
        for (index, &elem_idx) in tuple.elements.nodes.iter().enumerate() {
            // Emit any JSDoc comment that precedes this tuple element on
            // its own indented line(s) before the element itself.  Done
            // up-front (rather than via the parameter inline path) so the
            // comment shape is preserved as tsc renders it: each comment
            // on its own line, followed by the element.  Scope the
            // search to comments that come *after* the previous
            // element's end so a leading comment on element 0 isn't
            // re-attributed to element 1.
            if let Some(elem_node) = self.arena.get(elem_idx) {
                self.emit_tuple_member_jsdoc_comments(previous_elem_end, elem_node.pos);
            }
            self.write_indent();
            self.emit_type(elem_idx);
            if index + 1 < tuple.elements.nodes.len() {
                self.write(",");
            }
            self.write_line();
            if let Some(elem_node) = self.arena.get(elem_idx) {
                previous_elem_end = elem_node.end;
            }
        }
        self.decrease_indent();
        self.write_indent();
        self.write("]");
    }

    fn emit_named_tuple_type_multiline(&mut self, tuple_idx: NodeIndex) {
        self.emit_tuple_type_multiline(tuple_idx);
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
            if self.type_argument_tuple_should_preserve_multiline(arg_idx) {
                self.emit_tuple_type_multiline(arg_idx);
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
                    if let Some(canonical_name) =
                        self.canonical_named_import_name_for_alias(node_idx)
                    {
                        let canonical_name = canonical_name.to_owned();
                        self.write(&canonical_name);
                    } else {
                        self.write(&ident.escaped_text);
                    }
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
                    if self.entity_name_contains_import_call(type_ref.type_name) {
                        self.emit_entity_name(type_ref.type_name);
                    } else {
                        self.emit_node(type_ref.type_name);
                    }
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
                        let only_arg = args.nodes.len() == 1;
                        for &arg_idx in &args.nodes {
                            if !first {
                                self.write(", ");
                            }
                            let is_first = first;
                            first = false;
                            if is_first
                                && only_arg
                                && self.arena.get(arg_idx).is_some_and(|node| {
                                    node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                                })
                            {
                                self.emit_import_type_object_literal_as_type(arg_idx);
                            } else {
                                self.emit_import_type_argument(arg_idx);
                            }
                        }
                    }
                    self.write(")");
                }
            }
            _ => {}
        }
    }

    /// Detect whether an intersection arm's source slice contains an
    /// import-attribute object literal where the parser recovered with
    /// non-property entries (e.g. a bare numeric literal as a "key" like
    /// `{1234, "resolution-mode": "import"}`). The broken object's AST
    /// elements list is emptied during parse recovery, so the only
    /// surviving evidence is in the raw source span. Scan the slice for
    /// `with:` followed by `{` followed by a numeric literal at the
    /// recovered key position — that is the recognised parser-recovery
    /// shape.
    fn intersection_arm_source_has_broken_import_attrs(&self, type_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(type_idx) else {
            return false;
        };
        let Some(slice) = self.get_source_slice(node.pos, node.end) else {
            return false;
        };
        if !slice.contains("import(") || !slice.contains("with") {
            return false;
        }
        let bytes = slice.as_bytes();
        let mut i = 0;
        while i + 5 < bytes.len() {
            if &bytes[i..i + 5] == b"with:" || &bytes[i..i + 5] == b"with " {
                // Skip the `with` keyword and its colon, plus surrounding
                // whitespace.
                let mut j = i + 4;
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b':') {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'{' {
                    // First non-whitespace inside the attribute object.
                    let mut k = j + 1;
                    while k < bytes.len() && (bytes[k] as char).is_ascii_whitespace() {
                        k += 1;
                    }
                    if k < bytes.len() && (bytes[k] as char).is_ascii_digit() {
                        return true;
                    }
                }
            }
            i += 1;
        }
        false
    }

    fn entity_name_contains_import_call(&self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::CALL_EXPRESSION => true,
            k if k == syntax_kind_ext::QUALIFIED_NAME => self
                .arena
                .get_qualified_name(node)
                .is_some_and(|name| self.entity_name_contains_import_call(name.left)),
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                self.arena
                    .get_access_expr(node)
                    .is_some_and(|access| self.entity_name_contains_import_call(access.expression))
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => self
                .arena
                .get_type_ref(node)
                .is_some_and(|type_ref| self.entity_name_contains_import_call(type_ref.type_name)),
            _ => false,
        }
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

    fn emit_import_type_object_literal_as_type(&mut self, arg_idx: NodeIndex) {
        let Some(arg_node) = self.arena.get(arg_idx) else {
            return;
        };

        let Some(obj) = self.arena.get_literal_expr(arg_node) else {
            self.emit_node(arg_idx);
            return;
        };

        if obj.elements.nodes.is_empty() {
            self.write("{}");
            return;
        }

        self.write("{");
        self.write_line();
        self.increase_indent();
        for &elem_idx in &obj.elements.nodes {
            self.write_indent();
            self.emit_import_type_object_literal_member(elem_idx);
            self.write(";");
            self.write_line();
        }
        self.decrease_indent();
        self.write_indent();
        self.write("}");
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
            k if k == SyntaxKind::StringLiteral as u16 => {
                self.emit_string_literal_type(inner_node);
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(inner_node) {
                    self.write(&Self::declaration_numeric_literal_text(
                        &lit.text, lit.value,
                    ));
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

    fn emit_string_literal_type(&mut self, node: &tsz_parser::parser::node::Node) {
        if let Some(lit) = self.arena.get_literal(node) {
            let source_quote = if self.normalize_string_literal_type_quotes {
                None
            } else {
                self.get_source_slice(node.pos, node.end)
                    .and_then(|source| source.chars().next())
                    .filter(|quote| *quote == '\'' || *quote == '"')
            };

            if source_quote == Some('\'') {
                self.write("'");
                self.write(&escape_string_for_single_quote(&lit.text));
                self.write("'");
            } else {
                self.write("\"");
                self.write(&escape_string_for_double_quote(&lit.text));
                self.write("\"");
            }
        }
    }

    fn emit_indexed_access_index_type(
        &mut self,
        index_type_idx: NodeIndex,
        preserve_source_quote: bool,
    ) {
        if preserve_source_quote
            && let Some(node) = self.arena.get(index_type_idx)
            && let Some(text) = self.get_source_slice(node.pos, node.end)
            && (text.starts_with('"') || text.starts_with('\''))
        {
            let text = text.strip_suffix(']').unwrap_or(&text).trim_end();
            self.write(text);
            return;
        }

        self.emit_type(index_type_idx);
    }

    fn type_operator_operand_and_parens(&self, type_idx: NodeIndex) -> (NodeIndex, bool) {
        let source_was_parenthesized = self
            .arena
            .get(type_idx)
            .is_some_and(|n| n.kind == syntax_kind_ext::PARENTHESIZED_TYPE);
        let mut operand = type_idx;
        if let Some(node) = self.arena.get(operand)
            && node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            && let Some(paren) = self.arena.get_wrapped_type(node)
        {
            operand = paren.type_node;
        }

        let needs_parens = self.arena.get(operand).is_some_and(|n| {
            // Type operators (`keyof T`, `readonly T`, `unique T`) bind
            // tighter than `|`, `&`, `extends ? :`, `=>` and `new (...)`.
            // Without parens, e.g. `keyof () => void` would be parsed as
            // a function type whose first parameter list starts with `(`,
            // which is either a syntax error or completely different
            // semantics.
            n.kind == syntax_kind_ext::UNION_TYPE
                || n.kind == syntax_kind_ext::INTERSECTION_TYPE
                || n.kind == syntax_kind_ext::CONDITIONAL_TYPE
                || n.kind == syntax_kind_ext::FUNCTION_TYPE
                || n.kind == syntax_kind_ext::CONSTRUCTOR_TYPE
        });

        // tsc retains user-written parens around generic type references
        // (e.g. `keyof (Record<T, any>)`). Indexed-access operands keep
        // their stripping behavior since tsc renders `keyof A["a"]` (no
        // parens) regardless of source.
        let preserve_source_parens = source_was_parenthesized
            && self.arena.get(operand).is_some_and(|n| {
                n.kind == syntax_kind_ext::TYPE_REFERENCE
                    && self.arena.get_type_ref(n).is_some_and(|tr| {
                        tr.type_arguments
                            .as_ref()
                            .is_some_and(|args| !args.nodes.is_empty())
                    })
            });

        (operand, needs_parens || preserve_source_parens)
    }

    fn emit_mapped_type_value_type(&mut self, type_idx: NodeIndex) {
        let Some(type_node) = self.arena.get(type_idx) else {
            return;
        };

        if type_node.kind == syntax_kind_ext::INDEXED_ACCESS_TYPE
            && let Some(indexed_access) = self.arena.get_indexed_access_type(type_node)
        {
            self.emit_type(indexed_access.object_type);
            self.write("[");
            self.emit_indexed_access_index_type(indexed_access.index_type, true);
            self.write("]");
            return;
        }

        self.emit_type(type_idx);
    }

    fn expand_mapped_type_to_portable_properties(&self, type_idx: NodeIndex) -> Option<String> {
        let node = self.arena.get(type_idx)?;
        let text = self.get_source_slice(node.pos, node.end)?;
        let trimmed = text.trim().trim_end_matches(';').trim();
        let inner = trimmed
            .strip_prefix('{')
            .and_then(|text| text.strip_suffix('}'))
            .map(str::trim)
            .unwrap_or(trimmed);

        self.expand_portable_mapped_object_text(self.arena, inner)
    }

    pub(in crate::declaration_emitter) fn emit_mapped_type_constraint(
        &mut self,
        constraint_idx: NodeIndex,
    ) {
        if let Some(node) = self.arena.get(constraint_idx)
            && let Some(text) = self.get_source_slice(node.pos, node.end)
        {
            let text = Self::mapped_type_constraint_source_text(&text);
            if !text.is_empty() {
                self.write(text);
                return;
            }
        }

        self.emit_type(constraint_idx);
    }

    pub(in crate::declaration_emitter) fn emit_mapped_type_name_type(
        &mut self,
        name_type_idx: NodeIndex,
    ) {
        if let Some(node) = self.arena.get(name_type_idx)
            && let Some(text) = self.get_source_slice(node.pos, node.end)
        {
            let text = Self::mapped_type_name_source_text(&text);
            if !text.is_empty() {
                self.write(text);
                return;
            }
        }

        self.emit_type(name_type_idx);
    }

    pub(in crate::declaration_emitter) fn emit_mapped_type_as_clause(
        &mut self,
        name_type_idx: NodeIndex,
    ) {
        self.write(" as ");

        if let Some(node) = self.arena.get(name_type_idx)
            && let Some(text) = self.get_source_slice(node.pos, node.end)
        {
            let text = Self::mapped_type_name_source_text(&text);
            if !text.is_empty() {
                self.write(text);
                return;
            }
        }

        let start = self.writer.len();
        self.emit_mapped_type_name_type(name_type_idx);
        let emitted = self.writer.get_output()[start..].to_string();
        let normalized = Self::mapped_type_name_source_text(&emitted);
        if normalized != emitted.trim() {
            let normalized = normalized.to_string();
            self.writer.truncate(start);
            self.write(&normalized);
        }
    }

    fn mapped_type_constraint_source_text(text: &str) -> &str {
        let text = text.trim();
        let text = Self::split_mapped_as_clause(text)
            .map(|(before, _)| before.trim_end())
            .unwrap_or_else(|| Self::trim_trailing_mapped_as_keyword(text));
        Self::trim_unbalanced_closing_bracket(text)
    }

    fn mapped_type_name_source_text(text: &str) -> &str {
        let text = text.trim();
        let text = Self::split_mapped_as_clause(text)
            .map(|(_, after)| after.trim_start())
            .unwrap_or_else(|| Self::trim_leading_mapped_as_keyword(text));
        Self::trim_unbalanced_closing_bracket(text)
    }

    fn trim_leading_mapped_as_keyword(text: &str) -> &str {
        let mut trimmed = text.trim_start();
        while let Some(after_as) = trimmed.strip_prefix("as") {
            let has_boundary = after_as
                .chars()
                .next()
                .is_some_and(|ch| ch.is_whitespace() || !Self::is_identifier_part(ch));
            if !has_boundary {
                break;
            }
            trimmed = after_as.trim_start();
        }
        trimmed
    }

    fn split_mapped_as_clause(text: &str) -> Option<(&str, &str)> {
        let mut string_quote: Option<char> = None;
        let mut escaped = false;
        let mut angle_depth = 0u32;
        let mut brace_depth = 0u32;
        let mut bracket_depth = 0u32;
        let mut paren_depth = 0u32;

        for (idx, ch) in text.char_indices() {
            if let Some(quote) = string_quote {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == quote {
                    string_quote = None;
                }
                continue;
            }

            match ch {
                '\'' | '"' | '`' => {
                    string_quote = Some(ch);
                    continue;
                }
                '<' => {
                    angle_depth += 1;
                    continue;
                }
                '>' => {
                    angle_depth = angle_depth.saturating_sub(1);
                    continue;
                }
                '{' => {
                    brace_depth += 1;
                    continue;
                }
                '}' => {
                    brace_depth = brace_depth.saturating_sub(1);
                    continue;
                }
                '[' => {
                    bracket_depth += 1;
                    continue;
                }
                ']' => {
                    bracket_depth = bracket_depth.saturating_sub(1);
                    continue;
                }
                '(' => {
                    paren_depth += 1;
                    continue;
                }
                ')' => {
                    paren_depth = paren_depth.saturating_sub(1);
                    continue;
                }
                _ => {}
            }

            if ch != 'a'
                || !text[idx..].starts_with("as")
                || angle_depth != 0
                || brace_depth != 0
                || bracket_depth != 0
                || paren_depth != 0
            {
                continue;
            }

            let before = &text[..idx];
            let after = &text[idx + 2..];
            let before_boundary = before
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_whitespace() || !Self::is_identifier_part(ch));
            let after_boundary = after
                .chars()
                .next()
                .is_some_and(|ch| ch.is_whitespace() || !Self::is_identifier_part(ch));
            if before_boundary && after_boundary {
                return Some((before, after));
            }
        }
        None
    }

    fn trim_trailing_mapped_as_keyword(text: &str) -> &str {
        let trimmed = text.trim_end();
        let Some(before_as) = trimmed.strip_suffix("as") else {
            return trimmed;
        };
        let had_separator = before_as
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace);
        let before_as = before_as.trim_end();
        let has_boundary = before_as
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_whitespace() || !Self::is_identifier_part(ch));
        if had_separator || has_boundary {
            before_as
        } else {
            trimmed
        }
    }

    fn trim_unbalanced_closing_bracket(text: &str) -> &str {
        let trimmed = text.trim_end();
        if !trimmed.ends_with(']') {
            return trimmed;
        }

        let opens = trimmed.chars().filter(|&ch| ch == '[').count();
        let closes = trimmed.chars().filter(|&ch| ch == ']').count();
        if closes > opens {
            trimmed[..trimmed.len() - 1].trim_end()
        } else {
            trimmed
        }
    }

    const fn is_identifier_part(ch: char) -> bool {
        ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
    }
}

#[cfg(test)]
mod tests {
    use super::DeclarationEmitter;

    #[test]
    fn mapped_type_source_text_splits_compact_as_clause_after_indexed_access() {
        assert_eq!(
            DeclarationEmitter::mapped_type_constraint_source_text("T[number]as Item[Attr]"),
            "T[number]"
        );
        assert_eq!(
            DeclarationEmitter::mapped_type_constraint_source_text("T[number] as"),
            "T[number]"
        );
        assert_eq!(
            DeclarationEmitter::mapped_type_constraint_source_text("keyof T as"),
            "keyof T"
        );
        assert_eq!(
            DeclarationEmitter::mapped_type_name_source_text("T[number]as Item[Attr]"),
            "Item[Attr]"
        );
        assert_eq!(
            DeclarationEmitter::mapped_type_name_source_text("as `get${Capitalize<string & K>}`"),
            "`get${Capitalize<string & K>}`"
        );
        assert_eq!(
            DeclarationEmitter::mapped_type_name_source_text(
                "as as `get${Capitalize<string & K>}`"
            ),
            "`get${Capitalize<string & K>}`"
        );
        assert_eq!(
            DeclarationEmitter::mapped_type_name_source_text("asserts T"),
            "asserts T"
        );
        assert_eq!(
            DeclarationEmitter::mapped_type_constraint_source_text("keyof T]"),
            "keyof T"
        );
    }
}
