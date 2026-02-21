//! Declaration emitter - type emission and utility helpers.

use super::{DeclarationEmitter, ImportPlan, PlannedImportModule, PlannedImportSymbol};
use crate::emitter::type_printer::TypePrinter;
use crate::source_writer::{SourcePosition, SourceWriter, source_position_from_offset};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tracing::debug;
use tsz_binder::{BinderState, SymbolId};
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
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
                }
            }

            // Expression with type arguments (heritage clauses)
            k if k == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS => {
                if let Some(expr) = self.arena.get_expr_type_args(type_node) {
                    self.emit_entity_name(expr.expression);
                    if let Some(ref type_args) = expr.type_arguments
                        && !type_args.nodes.is_empty()
                    {
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

            // Type literal - inline format without newlines
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(lit) = self.arena.get_type_literal(type_node) {
                    self.write("{\n");
                    self.increase_indent();
                    for &member_idx in &lit.members.nodes {
                        self.write_indent();
                        self.emit_interface_member_inline(member_idx);
                        self.write(";");
                        self.write_line();
                    }
                    self.decrease_indent();
                    self.write("}");
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
                }
            }

            // Type operator (keyof, readonly, etc.)
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                if let Some(type_op) = self.arena.get_type_operator(type_node) {
                    // Check the operator kind
                    if type_op.operator == SyntaxKind::KeyOfKeyword as u16 {
                        self.write("keyof ");
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
                    self.write("\"");
                    self.write(&lit.text);
                    self.write("\"");
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
            _ => {}
        }
    }

    pub(crate) fn emit_expression(&mut self, expr_idx: NodeIndex) {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return;
        };
        let before_len = self.writer.len();
        self.queue_source_mapping(expr_node);

        match expr_node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(expr_node) {
                    self.write(&lit.text);
                }
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(expr_node) {
                    self.write("\"");
                    self.write(&lit.text);
                    self.write("\"");
                }
            }
            k if k == SyntaxKind::NullKeyword as u16 => {
                self.write("null");
            }
            k if k == SyntaxKind::TrueKeyword as u16 => {
                self.write("true");
            }
            k if k == SyntaxKind::FalseKeyword as u16 => {
                self.write("false");
            }
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                // Array literal in default parameter: emit as []
                self.write("[]");
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => {
                // Object literal in default parameter: emit as {}
                self.write("{}");
            }
            _ => self.emit_node(expr_idx),
        }

        if self.writer.len() == before_len {
            self.pending_source_pos = None;
        }
    }

    pub(crate) fn emit_node(&mut self, node_idx: NodeIndex) {
        let Some(node) = self.arena.get(node_idx) else {
            return;
        };
        let before_len = self.writer.len();
        self.queue_source_mapping(node);

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(ident) = self.arena.get_identifier(node) {
                    self.write(&ident.escaped_text);
                }
            }
            k if k == syntax_kind_ext::TYPE_PARAMETER => {
                // Type parameter node - emit its name
                if let Some(param) = self.arena.get_type_parameter(node) {
                    self.emit_node(param.name);
                }
            }
            k if k == syntax_kind_ext::QUALIFIED_NAME
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == SyntaxKind::ThisKeyword as u16
                || k == SyntaxKind::SuperKeyword as u16 =>
            {
                self.emit_entity_name(node_idx);
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    self.write("\"");
                    self.write(&lit.text);
                    self.write("\"");
                }
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    self.write(&lit.text);
                }
            }
            k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                if let Some(computed) = self.arena.get_computed_property(node) {
                    self.write("[");
                    self.emit_node(computed.expression);
                    self.write("]");
                }
            }
            // Fallback for contextual keywords and other unhandled node kinds used as names.
            _ if self.source_file_text.is_some() => {
                if let Some(text) = self.get_source_slice(node.pos, node.end) {
                    self.write(&text);
                }
            }
            _ => {}
        }

        if self.writer.len() == before_len {
            self.pending_source_pos = None;
        }
    }

    pub(crate) fn has_export_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        self.has_modifier(modifiers, SyntaxKind::ExportKeyword as u16)
    }

    pub(crate) fn has_public_api_exports(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> bool {
        if source_file.is_declaration_file {
            return false;
        }

        let mut has_import = false;
        let mut has_export = false;

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };

            if stmt_node.kind == syntax_kind_ext::IMPORT_DECLARATION
                || stmt_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                has_import = true;
            }

            match stmt_node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                    if let Some(func) = self.arena.get_function(stmt_node)
                        && self.has_export_modifier(&func.modifiers)
                    {
                        has_export = true;
                    }
                }
                k if k == syntax_kind_ext::CLASS_DECLARATION => {
                    if let Some(class) = self.arena.get_class(stmt_node)
                        && self.has_export_modifier(&class.modifiers)
                    {
                        has_export = true;
                    }
                }
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                    if let Some(iface) = self.arena.get_interface(stmt_node)
                        && self.has_export_modifier(&iface.modifiers)
                    {
                        has_export = true;
                    }
                }
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                    if let Some(alias) = self.arena.get_type_alias(stmt_node)
                        && self.has_export_modifier(&alias.modifiers)
                    {
                        has_export = true;
                    }
                }
                k if k == syntax_kind_ext::ENUM_DECLARATION => {
                    if let Some(enum_data) = self.arena.get_enum(stmt_node)
                        && self.has_export_modifier(&enum_data.modifiers)
                    {
                        has_export = true;
                    }
                }
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                    if let Some(var_stmt) = self.arena.get_variable(stmt_node)
                        && self.has_export_modifier(&var_stmt.modifiers)
                    {
                        has_export = true;
                    }
                }
                k if k == syntax_kind_ext::MODULE_DECLARATION => {
                    if let Some(module) = self.arena.get_module(stmt_node)
                        && self.has_export_modifier(&module.modifiers)
                    {
                        has_export = true;
                    }
                }
                k if k == syntax_kind_ext::EXPORT_DECLARATION
                    || k == syntax_kind_ext::EXPORT_ASSIGNMENT =>
                {
                    has_export = true;
                }
                _ => {}
            }
        }

        has_import || has_export
    }

    /// Return true when declarations are filtered to public API members.
    pub(crate) const fn public_api_filter_enabled(&self) -> bool {
        self.emit_public_api_only && self.public_api_scope_depth == 0
    }

    /// Return true if a top-level declaration should be emitted when API filtering is enabled.
    pub(crate) fn should_emit_public_api_member(&self, modifiers: &Option<NodeList>) -> bool {
        if !self.public_api_filter_enabled() {
            return true;
        }

        self.has_export_modifier(modifiers)
    }

    /// Return true if a module declaration should be emitted when API filtering is enabled.
    pub(crate) const fn should_emit_public_api_module(&self, is_exported: bool) -> bool {
        if !self.public_api_filter_enabled() {
            return true;
        }

        is_exported
    }

    /// Return true when we need `export {};` to preserve module-ness in `.d.ts`.
    pub(crate) fn needs_empty_export_marker(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> bool {
        // `export {};` is needed when the source is module-like (imports/exports)
        // but declaration output has no runtime-facing exports. This covers
        // type-only export modules and import-only modules.
        let has_module_syntax = source_file.statements.nodes.iter().any(|&stmt_idx| {
            self.arena.get(stmt_idx).is_some_and(|stmt_node| {
                matches!(
                    stmt_node.kind,
                    k if k == syntax_kind_ext::IMPORT_DECLARATION
                        || k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                        || k == syntax_kind_ext::EXPORT_DECLARATION
                        || k == syntax_kind_ext::EXPORT_ASSIGNMENT
                        || k == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
                )
            })
        });

        has_module_syntax && !self.has_public_api_value_exports(source_file)
    }

    /// Return true if exported declarations include any export (value or type-side).
    #[allow(dead_code)]
    pub(crate) fn has_public_api_any_exports(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> bool {
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            match stmt_node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                    if let Some(func) = self.arena.get_function(stmt_node)
                        && self.has_export_modifier(&func.modifiers)
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::CLASS_DECLARATION => {
                    if let Some(class) = self.arena.get_class(stmt_node)
                        && self.has_export_modifier(&class.modifiers)
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                    if let Some(iface) = self.arena.get_interface(stmt_node)
                        && self.has_export_modifier(&iface.modifiers)
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                    if let Some(alias) = self.arena.get_type_alias(stmt_node)
                        && self.has_export_modifier(&alias.modifiers)
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::ENUM_DECLARATION => {
                    if let Some(enum_data) = self.arena.get_enum(stmt_node)
                        && self.has_export_modifier(&enum_data.modifiers)
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                    if let Some(var_stmt) = self.arena.get_variable(stmt_node)
                        && self.has_export_modifier(&var_stmt.modifiers)
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::MODULE_DECLARATION => {
                    if let Some(module) = self.arena.get_module(stmt_node)
                        && self.has_export_modifier(&module.modifiers)
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                    if let Some(export) = self.arena.get_export_decl(stmt_node) {
                        // Type-only export declarations do not contribute value exports.
                        if export.is_type_only {
                            continue;
                        }
                        if let Some(clause_node) = self.arena.get(export.export_clause) {
                            match clause_node.kind {
                                k if k == syntax_kind_ext::INTERFACE_DECLARATION => continue,
                                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => continue,
                                _ => return true,
                            }
                        } else {
                            return true;
                        }
                    }
                }
                k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    /// Return true if exported declarations include runtime/value-side exports.
    pub(crate) fn has_public_api_value_exports(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> bool {
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            match stmt_node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                    if let Some(func) = self.arena.get_function(stmt_node)
                        && self.has_export_modifier(&func.modifiers)
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::CLASS_DECLARATION => {
                    if let Some(class) = self.arena.get_class(stmt_node)
                        && self.has_export_modifier(&class.modifiers)
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::ENUM_DECLARATION => {
                    if let Some(enum_data) = self.arena.get_enum(stmt_node)
                        && self.has_export_modifier(&enum_data.modifiers)
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                    if let Some(var_stmt) = self.arena.get_variable(stmt_node)
                        && self.has_export_modifier(&var_stmt.modifiers)
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::MODULE_DECLARATION => {
                    if let Some(module) = self.arena.get_module(stmt_node)
                        && self.has_export_modifier(&module.modifiers)
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                    if let Some(export) = self.arena.get_export_decl(stmt_node) {
                        if export.is_type_only {
                            continue;
                        }
                        if let Some(clause_node) = self.arena.get(export.export_clause) {
                            match clause_node.kind {
                                k if k == syntax_kind_ext::INTERFACE_DECLARATION => continue,
                                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => continue,
                                _ => return true,
                            }
                        } else {
                            return true;
                        }
                    }
                }
                k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    /// Return true when a declaration symbol is referenced by the exported API surface.
    pub(crate) fn should_emit_public_api_dependency(&self, name_idx: NodeIndex) -> bool {
        if !self.public_api_filter_enabled() {
            return true;
        }

        let Some(used) = &self.used_symbols else {
            // Usage analysis unavailable: preserve dependent declarations
            // rather than over-pruning and producing unresolved names.
            return true;
        };
        let Some(binder) = self.binder else {
            return true;
        };
        let Some(&sym_id) = binder.node_symbols.get(&name_idx.0) else {
            // Some declaration name nodes are not mapped directly; fall back
            // to root-scope lookup by identifier text.
            let Some(name_node) = self.arena.get(name_idx) else {
                return false;
            };
            let Some(name_ident) = self.arena.get_identifier(name_node) else {
                return false;
            };
            let Some(root_scope) = binder.scopes.first() else {
                return false;
            };
            let Some(scope_sym_id) = root_scope.table.get(&name_ident.escaped_text) else {
                return false;
            };
            return used.contains_key(&scope_sym_id);
        };

        used.contains_key(&sym_id)
    }

    /// Get the function/method name as a string for overload tracking
    pub(crate) fn get_function_name(&self, func_idx: NodeIndex) -> Option<String> {
        let func_node = self.arena.get(func_idx)?;

        // Try to get as function first
        let name_node = if let Some(func) = self.arena.get_function(func_node) {
            self.arena.get(func.name)?
        // Try to get as method
        } else if let Some(method) = self.arena.get_method_decl(func_node) {
            self.arena.get(method.name)?
        } else {
            return None;
        };

        // Only extract identifier names (not computed or other name types)
        if name_node.kind == SyntaxKind::Identifier as u16 {
            // Get the identifier text
            let ident = self.arena.get_identifier(name_node)?;
            Some(ident.escaped_text.clone())
        } else {
            None
        }
    }

    pub(crate) fn has_modifier(&self, modifiers: &Option<NodeList>, kind: u16) -> bool {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx)
                    && mod_node.kind == kind
                {
                    return true;
                }
            }
        }
        false
    }

    /// Check if an import specifier should be emitted based on usage analysis.
    ///
    /// Returns true if:
    /// - No usage tracking is enabled (`used_symbols` is None)
    /// - The specifier's symbol is in the `used_symbols` set
    pub(crate) fn should_emit_import_specifier(&self, specifier_idx: NodeIndex) -> bool {
        // If no usage tracking, emit everything
        let Some(used) = &self.used_symbols else {
            return true;
        };

        // If no binder, we can't check symbols - emit conservatively
        let Some(binder) = &self.binder else {
            return true;
        };

        // Get the specifier node to extract its name
        let Some(spec_node) = self.arena.get(specifier_idx) else {
            return true;
        };

        // Only ImportSpecifier/ExportSpecifier nodes have symbols (on their name field)
        // For other node types, emit conservatively
        if spec_node.kind != tsz_parser::parser::syntax_kind_ext::IMPORT_SPECIFIER
            && spec_node.kind != tsz_parser::parser::syntax_kind_ext::EXPORT_SPECIFIER
        {
            return true;
        }

        let Some(specifier) = self.arena.get_specifier(spec_node) else {
            return true;
        };

        // Check if the specifier's NAME symbol is used
        if let Some(&sym_id) = binder.node_symbols.get(&specifier.name.0) {
            used.contains_key(&sym_id)
        } else {
            // No symbol found - emit conservatively
            true
        }
    }

    /// Count how many import specifiers in an `ImportClause` should be emitted.
    ///
    /// Returns (`default_count`, `named_count`) where:
    /// - `default_count`: 1 if default import is used, 0 otherwise
    /// - `named_count`: number of used named import specifiers
    pub(crate) fn count_used_imports(
        &self,
        import: &tsz_parser::parser::node::ImportDeclData,
    ) -> (usize, usize) {
        let mut default_count = 0;
        let mut named_count = 0;

        if let Some(used) = &self.used_symbols
            && let Some(binder) = &self.binder
        {
            // Check default import
            if import.import_clause.is_some()
                && let Some(clause_node) = self.arena.get(import.import_clause)
                && let Some(clause) = self.arena.get_import_clause(clause_node)
            {
                if clause.name.is_some()
                    && let Some(&sym_id) = binder.node_symbols.get(&clause.name.0)
                    && used.contains_key(&sym_id)
                {
                    default_count = 1;
                }

                // Count named imports
                if clause.named_bindings.is_some()
                    && let Some(bindings_node) = self.arena.get(clause.named_bindings)
                    && let Some(bindings) = self.arena.get_named_imports(bindings_node)
                {
                    for &spec_idx in &bindings.elements.nodes {
                        // Get the specifier's name to check its symbol
                        if let Some(spec_node) = self.arena.get(spec_idx)
                            && let Some(specifier) = self.arena.get_specifier(spec_node)
                            && let Some(&sym_id) = binder.node_symbols.get(&specifier.name.0)
                            && used.contains_key(&sym_id)
                        {
                            named_count += 1;
                        }
                    }
                }
            }
        } else {
            // No usage tracking - count everything as used
            default_count = usize::from(import.import_clause.is_some());
            named_count = 1; // At least one if present
        }

        (default_count, named_count)
    }

    /// Phase 4: Prepare import aliases before emitting anything.
    ///
    /// This detects name collisions and generates aliases for conflicting imports.
    pub(crate) fn prepare_import_aliases(&mut self, root_idx: NodeIndex) {
        // 1. Collect all top-level local declarations into reserved_names
        self.collect_local_declarations(root_idx);

        // 2. Process required_imports (String-based)
        // We clone keys to avoid borrow checker issues during iteration
        let modules: Vec<String> = self.required_imports.keys().cloned().collect();
        for module in modules {
            // Collect names into a separate vector to release the borrow
            let names: Vec<String> = self
                .required_imports
                .get(&module)
                .map(|v| v.to_vec())
                .unwrap_or_default();
            for name in names {
                self.resolve_import_name(&module, &name);
            }
        }

        // 3. Process foreign_symbols (SymbolId-based) - skip for now
        // This requires grouping by module which needs arena_to_path mapping
    }

    /// Collect local top-level names into `reserved_names`.
    pub(crate) fn collect_local_declarations(&mut self, root_idx: NodeIndex) {
        let Some(root_node) = self.arena.get(root_idx) else {
            return;
        };
        let Some(source_file) = self.arena.get_source_file(root_node) else {
            return;
        };

        // If we have a binder, use it to get top-level symbols
        if let Some(binder) = self.binder {
            // Get the root scope (scopes is a Vec, not a HashMap)
            if let Some(root_scope) = binder.scopes.first() {
                // Iterate through all symbols in root scope table
                for (name, _sym_id) in root_scope.table.iter() {
                    self.reserved_names.insert(name.clone());
                }
            }
        } else {
            // Fallback: Walk AST statements for top-level declarations
            for &stmt_idx in &source_file.statements.nodes {
                if stmt_idx.is_none() {
                    continue;
                }
                let Some(stmt_node) = self.arena.get(stmt_idx) else {
                    continue;
                };

                let kind = stmt_node.kind;
                // Collect names from various declaration types
                if kind == tsz_parser::parser::syntax_kind_ext::FUNCTION_DECLARATION
                    || kind == tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION
                    || kind == tsz_parser::parser::syntax_kind_ext::INTERFACE_DECLARATION
                    || kind == tsz_parser::parser::syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || kind == tsz_parser::parser::syntax_kind_ext::ENUM_DECLARATION
                {
                    // Try to get the name
                    if let Some(name) = self.extract_declaration_name(stmt_idx) {
                        self.reserved_names.insert(name);
                    }
                }
            }
        }
    }

    /// Extract the name from a declaration node.
    pub(crate) fn extract_declaration_name(&self, decl_idx: NodeIndex) -> Option<String> {
        let decl_node = self.arena.get(decl_idx)?;

        // Try identifier first
        if let Some(ident) = self.arena.get_identifier(decl_node) {
            return Some(ident.escaped_text.clone());
        }

        // For class/function/interface, the name is in a specific field
        if let Some(func) = self.arena.get_function(decl_node)
            && let Some(name_node) = self.arena.get(func.name)
        {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return Some(ident.escaped_text.clone());
            }
            if let Some(lit) = self.arena.get_literal(name_node) {
                return Some(lit.text.clone());
            }
        }
        if let Some(class) = self.arena.get_class(decl_node)
            && let Some(name_node) = self.arena.get(class.name)
        {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return Some(ident.escaped_text.clone());
            }
            if let Some(lit) = self.arena.get_literal(name_node) {
                return Some(lit.text.clone());
            }
        }
        if let Some(iface) = self.arena.get_interface(decl_node)
            && let Some(name_node) = self.arena.get(iface.name)
        {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return Some(ident.escaped_text.clone());
            }
            if let Some(lit) = self.arena.get_literal(name_node) {
                return Some(lit.text.clone());
            }
        }
        if let Some(alias) = self.arena.get_type_alias(decl_node)
            && let Some(name_node) = self.arena.get(alias.name)
        {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return Some(ident.escaped_text.clone());
            }
            if let Some(lit) = self.arena.get_literal(name_node) {
                return Some(lit.text.clone());
            }
        }
        if let Some(enum_data) = self.arena.get_enum(decl_node)
            && let Some(name_node) = self.arena.get(enum_data.name)
        {
            if let Some(ident) = self.arena.get_identifier(name_node) {
                return Some(ident.escaped_text.clone());
            }
            if let Some(lit) = self.arena.get_literal(name_node) {
                return Some(lit.text.clone());
            }
        }

        None
    }

    /// Resolve name for string imports, generating alias if needed.
    pub(crate) fn resolve_import_name(&mut self, module: &str, name: &str) {
        if self.reserved_names.contains(name) {
            // Collision! Generate alias
            let alias = self.generate_unique_name(name);
            self.import_string_aliases
                .insert((module.to_string(), name.to_string()), alias.clone());
            self.reserved_names.insert(alias);
        } else {
            // No collision, reserve the name
            self.reserved_names.insert(name.to_string());
        }
    }

    /// Generate unique name (e.g., "`TypeA_1`").
    pub(crate) fn generate_unique_name(&self, base: &str) -> String {
        let mut i = 1;
        loop {
            let candidate = format!("{base}_{i}");
            if !self.reserved_names.contains(&candidate) {
                return candidate;
            }
            i += 1;
        }
    }

    pub(crate) fn reset_writer(&mut self) {
        self.writer = SourceWriter::with_capacity(4096);
        self.pending_source_pos = None;
        self.public_api_scope_depth = 0;
        if let Some(state) = &self.source_map_state {
            self.writer.enable_source_map(state.output_name.clone());
            let content = self.source_map_text.map(std::string::ToString::to_string);
            self.writer.add_source(state.source_name.clone(), content);
        }
    }

    pub(crate) fn queue_source_mapping(&mut self, node: &Node) {
        if !self.writer.has_source_map() {
            self.pending_source_pos = None;
            return;
        }

        let Some(text) = self.source_map_text else {
            self.pending_source_pos = None;
            return;
        };

        self.pending_source_pos = Some(source_position_from_offset(text, node.pos));
    }

    pub(crate) const fn take_pending_source_pos(&mut self) -> Option<SourcePosition> {
        self.pending_source_pos.take()
    }

    pub(crate) fn get_source_slice(&self, start: u32, end: u32) -> Option<String> {
        let text = self.source_file_text.as_ref()?;
        let start = start as usize;
        let end = end as usize;
        if start > end || end > text.len() {
            return None;
        }

        let slice = text[start..end].trim().to_string();
        if slice.is_empty() { None } else { Some(slice) }
    }

    pub(crate) fn write_raw(&mut self, s: &str) {
        self.writer.write(s);
    }

    pub(crate) fn write(&mut self, s: &str) {
        if let Some(source_pos) = self.take_pending_source_pos() {
            self.writer.write_node(s, source_pos);
        } else {
            self.writer.write(s);
        }
    }

    pub(crate) fn write_line(&mut self) {
        self.writer.write_line();
    }

    pub(crate) fn write_indent(&mut self) {
        for _ in 0..self.indent_level {
            self.write_raw("    ");
        }
    }

    pub(crate) const fn increase_indent(&mut self) {
        self.indent_level += 1;
    }

    pub(crate) const fn decrease_indent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    /// Get the type of a node from the type cache, if available.
    pub(crate) fn get_node_type(&self, node_id: NodeIndex) -> Option<tsz_solver::types::TypeId> {
        if let (Some(cache), _) = (&self.type_cache, &self.type_interner) {
            cache.node_types.get(&node_id.0).copied()
        } else {
            None
        }
    }

    pub(crate) fn get_node_type_or_names(
        &self,
        node_ids: &[NodeIndex],
    ) -> Option<tsz_solver::types::TypeId> {
        for &node_id in node_ids {
            if let Some(type_id) = self.get_node_type(node_id) {
                return Some(type_id);
            }

            let Some(node) = self.arena.get(node_id) else {
                continue;
            };

            for related_id in self.get_node_type_related_nodes(node) {
                if let Some(type_id) = self.get_node_type(related_id) {
                    return Some(type_id);
                }
            }
        }
        None
    }

    pub(crate) fn get_node_type_related_nodes(&self, node: &Node) -> Vec<NodeIndex> {
        match node.kind {
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                if let Some(decl) = self.arena.get_variable_declaration(node) {
                    let mut related = Vec::with_capacity(1);
                    if decl.initializer.is_some() {
                        related.push(decl.initializer);
                    }
                    related.push(decl.type_annotation);
                    related
                } else {
                    Vec::new()
                }
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                if let Some(decl) = self.arena.get_property_decl(node) {
                    let mut related = Vec::with_capacity(2);
                    if decl.initializer.is_some() {
                        related.push(decl.initializer);
                    }
                    related.push(decl.type_annotation);
                    related
                } else {
                    Vec::new()
                }
            }
            k if k == syntax_kind_ext::PARAMETER => {
                if let Some(param) = self.arena.get_parameter(node) {
                    if param.initializer.is_some() {
                        vec![param.initializer]
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                if let Some(access_expr) = self.arena.get_access_expr(node) {
                    vec![access_expr.expression, access_expr.name_or_argument]
                } else {
                    Vec::new()
                }
            }
            k if k == syntax_kind_ext::TYPE_QUERY => {
                if let Some(query) = self.arena.get_type_query(node) {
                    vec![query.expr_name]
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }

    /// Print a `TypeId` as TypeScript syntax using `TypePrinter`.
    pub(crate) fn print_type_id(&self, type_id: tsz_solver::types::TypeId) -> String {
        if let Some(interner) = self.type_interner {
            let mut printer = TypePrinter::new(interner);

            // Add symbol arena if available for visibility checking
            if let Some(binder) = self.binder {
                printer = printer.with_symbols(&binder.symbols);
            }

            // Add type cache if available for resolving Lazy(DefId) types
            if let Some(cache) = &self.type_cache {
                printer = printer.with_type_cache(cache);
            }

            printer.print_type(type_id)
        } else {
            // Fallback if no interner available
            "any".to_string()
        }
    }

    /// Resolve a foreign symbol to its module path.
    ///
    /// Returns the module specifier (e.g., "./utils") for importing the symbol.
    pub(crate) fn resolve_symbol_module_path(&self, sym_id: SymbolId) -> Option<String> {
        let (Some(binder), Some(current_path)) = (&self.binder, &self.current_file_path) else {
            return None;
        };

        // 1. Check for ambient modules (declare module "name")
        if let Some(ambient_path) = self.check_ambient_module(sym_id, binder) {
            return Some(ambient_path);
        }

        // 2. Check import_symbol_map for imported symbols
        // This handles symbols that were imported from other modules
        if let Some(module_specifier) = self.import_symbol_map.get(&sym_id) {
            return Some(module_specifier.clone());
        }

        // 3. Get the source arena for this symbol
        let source_arena = binder.symbol_arenas.get(&sym_id)?;

        // 4. Look up the file path from arena address
        let arena_addr = Arc::as_ptr(source_arena) as usize;
        let source_path = self.arena_to_path.get(&arena_addr)?;

        // 5. Calculate relative path
        let rel_path = self.calculate_relative_path(current_path, source_path);

        // 6. Strip TypeScript extensions
        Some(self.strip_ts_extensions(&rel_path))
    }

    pub(crate) fn resolve_symbol_module_path_cached(&mut self, sym_id: SymbolId) -> Option<String> {
        if let Some(cached) = self.symbol_module_specifier_cache.get(&sym_id) {
            return cached.clone();
        }

        let resolved = self.resolve_symbol_module_path(sym_id);
        self.symbol_module_specifier_cache
            .insert(sym_id, resolved.clone());
        resolved
    }

    /// Check if a symbol is from an ambient module declaration.
    ///
    /// Returns the module name if the symbol is declared inside `declare module "name"`.
    pub(crate) fn check_ambient_module(
        &self,
        sym_id: SymbolId,
        binder: &BinderState,
    ) -> Option<String> {
        let symbol = binder.symbols.get(sym_id)?;

        // Walk up the parent chain
        let mut current_sym = symbol;
        let mut parent_id = current_sym.parent;
        while parent_id.is_some() {
            let parent_sym = binder.symbols.get(parent_id)?;

            // Check if parent is a module declaration
            if parent_sym.flags & tsz_binder::symbol_flags::MODULE != 0 {
                // Check if this module is in declared_modules
                let module_name = &parent_sym.escaped_name;
                if binder.declared_modules.contains(module_name) {
                    return Some(module_name.clone());
                }
            }

            current_sym = parent_sym;
            parent_id = current_sym.parent;
        }

        None
    }

    /// Calculate relative path from current file to source file.
    ///
    /// Returns a path like "../utils" or "./helper"
    pub(crate) fn calculate_relative_path(&self, current: &str, source: &str) -> String {
        use std::path::{Component, Path};

        let current_path = Path::new(current);
        let source_path = Path::new(source);

        // Get parent directories
        let current_dir = current_path.parent().unwrap_or(current_path);

        // Find common prefix and build relative path
        let current_components: Vec<_> = current_dir.components().collect();
        let source_components: Vec<_> = source_path.components().collect();

        // Find common prefix length
        let common_len = current_components
            .iter()
            .zip(source_components.iter())
            .take_while(|(a, b)| a == b)
            .count();

        // Build relative path: go up from current_dir, then down to source
        let ups = current_components.len() - common_len;
        let mut result = String::new();

        if ups == 0 {
            result.push_str("./");
        } else {
            for _ in 0..ups {
                result.push_str("../");
            }
        }

        // Append remaining source path components
        let remaining: Vec<_> = source_components[common_len..]
            .iter()
            .filter_map(|c| match c {
                Component::Normal(s) => s.to_str(),
                _ => None,
            })
            .collect();
        result.push_str(&remaining.join("/"));

        // Normalize separators
        result.replace('\\', "/")
    }

    /// Strip TypeScript file extensions from a path.
    ///
    /// Converts "../utils.ts" -> "../utils"
    pub(crate) fn strip_ts_extensions(&self, path: &str) -> String {
        // Remove .ts, .tsx, .d.ts, .d.tsx extensions
        for ext in [".d.ts", ".d.tsx", ".tsx", ".ts"] {
            if let Some(path) = path.strip_suffix(ext) {
                return path.to_string();
            }
        }
        path.to_string()
    }

    /// Group foreign symbols by their module paths.
    ///
    /// Returns a map of module path -> Vec<SymbolId> for all foreign symbols.
    pub(crate) fn group_foreign_symbols_by_module(&mut self) -> FxHashMap<String, Vec<SymbolId>> {
        let mut module_map: FxHashMap<String, Vec<SymbolId>> = FxHashMap::default();

        debug!(
            "[DEBUG] group_foreign_symbols_by_module: foreign_symbols = {:?}",
            self.foreign_symbols
        );

        let foreign_symbols: Vec<SymbolId> = self
            .foreign_symbols
            .as_ref()
            .map(|symbols| symbols.iter().copied().collect())
            .unwrap_or_default();

        for sym_id in foreign_symbols {
            debug!(
                "[DEBUG] group_foreign_symbols_by_module: resolving symbol {:?}",
                sym_id
            );
            if let Some(module_path) = self.resolve_symbol_module_path_cached(sym_id) {
                debug!(
                    "[DEBUG] group_foreign_symbols_by_module: symbol {:?} -> module '{}'",
                    sym_id, module_path
                );
                module_map.entry(module_path).or_default().push(sym_id);
            } else {
                debug!(
                    "[DEBUG] group_foreign_symbols_by_module: symbol {:?} -> no module path",
                    sym_id
                );
            }
        }

        debug!(
            "[DEBUG] group_foreign_symbols_by_module: returning {} modules",
            module_map.len()
        );
        module_map
    }

    pub(crate) fn prepare_import_plan(&mut self) {
        let mut plan = ImportPlan::default();

        let mut required_modules: Vec<String> = self.required_imports.keys().cloned().collect();
        required_modules.sort();
        for module in required_modules {
            let Some(symbol_names) = self.required_imports.get(&module) else {
                continue;
            };
            if symbol_names.is_empty() {
                continue;
            }

            let mut deduped = symbol_names.clone();
            deduped.sort();
            deduped.dedup();

            let symbols = deduped
                .into_iter()
                .map(|name| {
                    let alias = self
                        .import_string_aliases
                        .get(&(module.clone(), name.clone()))
                        .cloned();
                    PlannedImportSymbol { name, alias }
                })
                .collect();

            plan.required.push(PlannedImportModule { module, symbols });
        }

        if let Some(binder) = self.binder {
            let module_map = self.group_foreign_symbols_by_module();
            let mut auto_modules: Vec<_> = module_map.into_iter().collect();
            auto_modules.sort_by(|a, b| a.0.cmp(&b.0));

            for (module, symbol_ids) in auto_modules {
                let mut symbol_names: Vec<String> = symbol_ids
                    .into_iter()
                    .filter(|sym_id| self.import_symbol_map.contains_key(sym_id))
                    .filter_map(|sym_id| binder.symbols.get(sym_id).map(|s| s.escaped_name.clone()))
                    .collect();
                symbol_names.sort();
                symbol_names.dedup();

                if symbol_names.is_empty() {
                    continue;
                }

                let symbols = symbol_names
                    .into_iter()
                    .map(|name| PlannedImportSymbol { name, alias: None })
                    .collect();
                plan.auto_generated
                    .push(PlannedImportModule { module, symbols });
            }
        }

        self.import_plan = plan;
    }

    fn emit_import_modules(&mut self, modules: &[PlannedImportModule]) {
        for module in modules {
            self.write_indent();
            self.write("import { ");

            let mut first = true;
            for symbol in &module.symbols {
                if !first {
                    self.write(", ");
                }
                first = false;

                self.write(&symbol.name);
                if let Some(alias) = &symbol.alias {
                    self.write(" as ");
                    self.write(alias);
                }
            }

            self.write(" } from \"");
            self.write(&module.module);
            self.write("\";");
            self.write_line();
        }
    }

    /// Emit auto-generated imports for foreign symbols.
    ///
    /// This should be called before emitting other declarations to ensure
    /// imports appear at the top of the .d.ts file.
    pub(crate) fn emit_auto_imports(&mut self) {
        let modules = std::mem::take(&mut self.import_plan.auto_generated);
        self.emit_import_modules(&modules);
    }

    /// Emit required imports at the beginning of the .d.ts file.
    ///
    /// This should be called before emitting other declarations.
    pub(crate) fn emit_required_imports(&mut self) {
        if self.import_plan.required.is_empty() {
            debug!("[DEBUG] emit_required_imports: no required imports");
            return;
        }

        let modules = std::mem::take(&mut self.import_plan.required);
        self.emit_import_modules(&modules);
    }
}
