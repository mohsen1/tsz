//! Deterministic JavaScript and declaration emit for the supported syntax tree.
//!
//! Emit is deliberately a syntax transform. It erases type-only syntax and
//! prints runtime nodes; it does not validate types or recover semantic facts.

use std::path::{Path, PathBuf};

use crate::program::{CompilerOptions, EmittedFile, ProgramFile};
use crate::source::{SourceText, Span};
use crate::syntax::{
    ArrowBody, BinaryOperator, Expression, ExpressionKind, FunctionDeclaration,
    InterfaceDeclaration, KeywordType, Literal, ObjectProperty, Parameter, SourceUnit, Statement,
    StatementKind, TypeAliasDeclaration, TypeNode, TypeNodeKind, TypeProperty, VariableDeclaration,
    VariableKind,
};

/// Emit the JavaScript product and, when requested, its declaration product.
///
/// Output ordering is stable: JavaScript precedes declarations for a source
/// file, and the program layer performs the final path sort across files.
#[must_use]
pub fn emit_file(file: &ProgramFile, options: &CompilerOptions) -> Vec<EmittedFile> {
    if is_declaration_source(&file.source.path) {
        return Vec::new();
    }

    let mut emitted = Vec::with_capacity(usize::from(options.declaration) + 1);
    let mut javascript = Printer::new(&file.source, options);
    javascript.emit_javascript(&file.syntax);
    emitted.push(EmittedFile {
        path: output_path(&file.source.path, options.out_dir.as_deref(), false),
        text: javascript.finish(),
        declaration: false,
    });

    if options.declaration {
        let directory = options
            .declaration_dir
            .as_deref()
            .or(options.out_dir.as_deref());
        let mut declarations = Printer::new(&file.source, options);
        declarations.emit_declarations(&file.syntax);
        emitted.push(EmittedFile {
            path: output_path(&file.source.path, directory, true),
            text: declarations.finish(),
            declaration: true,
        });
    }

    emitted
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModuleFormat {
    CommonJs,
    EsModule,
}

struct Printer<'a> {
    source: &'a SourceText,
    output: String,
    indent: usize,
    module_format: ModuleFormat,
    implicit_external_module: bool,
    preserve_block_scope: bool,
    preserve_arrows: bool,
}

impl<'a> Printer<'a> {
    fn new(source: &'a SourceText, options: &CompilerOptions) -> Self {
        let target = options.target.trim().to_ascii_lowercase();
        let module = options.module.trim().to_ascii_lowercase();
        let extension = source
            .path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let implicit_external_module = matches!(extension.as_str(), "mts" | "cts");
        Self {
            source,
            output: String::new(),
            indent: 0,
            module_format: if matches!(module.as_str(), "commonjs" | "cjs")
                || (matches!(module.as_str(), "node16" | "nodenext") && extension == "cts")
            {
                ModuleFormat::CommonJs
            } else {
                ModuleFormat::EsModule
            },
            implicit_external_module,
            preserve_block_scope: !matches!(target.as_str(), "es3" | "es5"),
            preserve_arrows: !matches!(target.as_str(), "es3" | "es5"),
        }
    }

    fn finish(self) -> String {
        self.output
    }

    fn emit_javascript(&mut self, unit: &SourceUnit) {
        let has_export = unit.statements.iter().any(statement_is_exported);
        let external_module = has_export || self.implicit_external_module;
        let has_runtime_export = unit
            .statements
            .iter()
            .any(|statement| self.statement_is_runtime_export(statement));

        if self.module_format == ModuleFormat::CommonJs || !external_module {
            self.output.push_str("\"use strict\";\n");
        }
        if external_module && self.module_format == ModuleFormat::CommonJs {
            self.output
                .push_str("Object.defineProperty(exports, \"__esModule\", { value: true });\n");
        }

        for statement in &unit.statements {
            self.write_javascript_statement(statement, true);
        }

        if external_module && !has_runtime_export && self.module_format == ModuleFormat::EsModule {
            self.output.push_str("export {};\n");
        }
    }

    fn emit_declarations(&mut self, unit: &SourceUnit) {
        let has_export = unit.statements.iter().any(statement_is_exported);
        for statement in &unit.statements {
            match &statement.kind {
                StatementKind::Variable(declaration) => {
                    self.write_declaration_variable(declaration);
                }
                StatementKind::Function(declaration) => {
                    self.write_declaration_function(declaration);
                }
                StatementKind::TypeAlias(declaration) => {
                    self.write_declaration_type_alias(declaration);
                }
                StatementKind::Interface(declaration) => {
                    self.write_declaration_interface(declaration);
                }
                StatementKind::Return(_)
                | StatementKind::Block(_)
                | StatementKind::Expression(_)
                | StatementKind::Empty
                | StatementKind::Unknown => {}
            }
        }
        if self.implicit_external_module && !has_export {
            self.output.push_str("export {};\n");
        }
    }

    fn write_javascript_statement(&mut self, statement: &Statement, top_level: bool) {
        match &statement.kind {
            StatementKind::Variable(declaration) => {
                self.write_javascript_variable(declaration, top_level);
            }
            StatementKind::Function(declaration) => {
                self.write_javascript_function(statement, declaration, top_level);
            }
            // Type-only and unknown syntax has no safe JavaScript product.
            // Copying unknown source could leak TypeScript-only syntax.
            StatementKind::TypeAlias(_) | StatementKind::Interface(_) | StatementKind::Unknown => {}
            StatementKind::Return(expression) => {
                self.write_indent();
                self.output.push_str("return");
                if let Some(expression) = expression {
                    self.output.push(' ');
                    self.write_expression(expression, PREC_LOWEST);
                }
                self.output.push_str(";\n");
            }
            StatementKind::Block(statements) => {
                self.write_indent();
                self.write_braced_statements(statements);
                self.output.push('\n');
            }
            StatementKind::Expression(expression) => {
                self.write_indent();
                self.write_expression(expression, PREC_LOWEST);
                self.output.push_str(";\n");
            }
            StatementKind::Empty => {
                self.write_indent();
                self.output.push_str(";\n");
            }
        }
    }

    fn write_javascript_variable(&mut self, declaration: &VariableDeclaration, top_level: bool) {
        self.write_indent();
        if top_level && declaration.exported && self.module_format == ModuleFormat::EsModule {
            self.output.push_str("export ");
        }
        self.output
            .push_str(self.runtime_variable_kind(declaration.declaration_kind));
        self.output.push(' ');
        self.output.push_str(&declaration.name);
        if let Some(initializer) = &declaration.initializer {
            self.output.push_str(" = ");
            self.write_expression(initializer, PREC_ASSIGNMENT);
        }
        self.output.push_str(";\n");

        if top_level && declaration.exported && self.module_format == ModuleFormat::CommonJs {
            self.write_commonjs_export(&declaration.name);
        }
    }

    fn write_javascript_function(
        &mut self,
        statement: &Statement,
        declaration: &FunctionDeclaration,
        top_level: bool,
    ) {
        if declaration.declared || !self.function_has_body(statement) {
            return;
        }

        self.write_indent();
        if top_level && declaration.exported && self.module_format == ModuleFormat::EsModule {
            self.output.push_str("export ");
        }
        if declaration.is_async {
            self.output.push_str("async ");
        }
        self.output.push_str("function ");
        self.output.push_str(&declaration.name);
        self.write_runtime_parameters(&declaration.parameters);
        self.output.push(' ');
        self.write_braced_statements(&declaration.body);
        self.output.push('\n');

        if top_level && declaration.exported && self.module_format == ModuleFormat::CommonJs {
            self.write_commonjs_export(&declaration.name);
        }
    }

    fn write_braced_statements(&mut self, statements: &[Statement]) {
        self.output.push_str("{\n");
        self.indent += 1;
        for statement in statements {
            self.write_javascript_statement(statement, false);
        }
        self.indent = self.indent.saturating_sub(1);
        self.write_indent();
        self.output.push('}');
    }

    fn write_runtime_parameters(&mut self, parameters: &[Parameter]) {
        self.output.push('(');
        for (index, parameter) in parameters.iter().enumerate() {
            if index != 0 {
                self.output.push_str(", ");
            }
            self.output.push_str(&parameter.name);
        }
        self.output.push(')');
    }

    fn write_commonjs_export(&mut self, name: &str) {
        self.write_indent();
        self.output.push_str("exports.");
        self.output.push_str(name);
        self.output.push_str(" = ");
        self.output.push_str(name);
        self.output.push_str(";\n");
    }

    fn write_expression(&mut self, expression: &Expression, parent_precedence: u8) {
        // Parentheses used only to contain a TypeScript assertion disappear
        // with the assertion. The recursive call restores any grouping that
        // the underlying JavaScript expression still requires.
        if let ExpressionKind::As { expression, .. } = &expression.kind {
            self.write_expression(expression, parent_precedence);
            return;
        }
        if let ExpressionKind::Parenthesized(inner) = &expression.kind
            && matches!(&inner.kind, ExpressionKind::As { .. })
        {
            self.write_expression(inner, parent_precedence);
            return;
        }

        let precedence = self.expression_precedence(expression);
        let parenthesize = precedence < parent_precedence;
        if parenthesize {
            self.output.push('(');
        }

        match &expression.kind {
            ExpressionKind::Identifier { name, .. } => self.output.push_str(name),
            ExpressionKind::Literal(literal) => {
                let text = self.literal_text(literal, expression.span);
                self.output.push_str(&text);
            }
            ExpressionKind::Object(properties) => self.write_object_literal(properties),
            ExpressionKind::Array(elements) => {
                self.output.push('[');
                for (index, element) in elements.iter().enumerate() {
                    if index != 0 {
                        self.output.push_str(", ");
                    }
                    self.write_expression(element, PREC_LOWEST);
                }
                self.output.push(']');
            }
            ExpressionKind::Call { callee, arguments } => {
                self.write_expression(callee, PREC_POSTFIX);
                self.output.push('(');
                for (index, argument) in arguments.iter().enumerate() {
                    if index != 0 {
                        self.output.push_str(", ");
                    }
                    self.write_expression(argument, PREC_LOWEST);
                }
                self.output.push(')');
            }
            ExpressionKind::Member { object, name, .. } => {
                self.write_expression(object, PREC_POSTFIX);
                self.output.push('.');
                self.output.push_str(name);
            }
            ExpressionKind::Arrow { parameters, body } => {
                self.write_arrow(parameters, body);
            }
            ExpressionKind::Binary {
                left,
                operator,
                right,
            } => {
                self.write_expression(left, precedence);
                self.output.push(' ');
                self.output.push_str(binary_operator_text(*operator));
                self.output.push(' ');
                self.write_expression(right, precedence.saturating_add(1));
            }
            ExpressionKind::Assignment { left, right } => {
                self.write_expression(left, PREC_ASSIGNMENT.saturating_add(1));
                self.output.push_str(" = ");
                self.write_expression(right, PREC_ASSIGNMENT);
            }
            ExpressionKind::As { .. } => unreachable!("assertions are erased before printing"),
            ExpressionKind::Parenthesized(expression) => {
                self.output.push('(');
                self.write_expression(expression, PREC_LOWEST);
                self.output.push(')');
            }
            ExpressionKind::Missing => self.output.push_str("void 0"),
        }

        if parenthesize {
            self.output.push(')');
        }
    }

    fn write_object_literal(&mut self, properties: &[ObjectProperty]) {
        self.output.push('{');
        if !properties.is_empty() {
            self.output.push(' ');
        }
        for (index, property) in properties.iter().enumerate() {
            if index != 0 {
                self.output.push_str(", ");
            }
            self.write_property_name(&property.name, property.name_span);
            if !object_property_is_shorthand(property) {
                self.output.push_str(": ");
                self.write_expression(&property.value, PREC_LOWEST);
            }
        }
        if !properties.is_empty() {
            self.output.push(' ');
        }
        self.output.push('}');
    }

    fn write_arrow(&mut self, parameters: &[Parameter], body: &ArrowBody) {
        if self.preserve_arrows {
            self.write_runtime_parameters(parameters);
            self.output.push_str(" => ");
            match body {
                ArrowBody::Expression(expression) => {
                    self.write_expression(expression, PREC_ASSIGNMENT);
                }
                ArrowBody::Block(statements) => self.write_braced_statements(statements),
            }
            return;
        }

        self.output.push_str("function ");
        self.write_runtime_parameters(parameters);
        self.output.push(' ');
        match body {
            ArrowBody::Expression(expression) => {
                self.output.push_str("{\n");
                self.indent += 1;
                self.write_indent();
                self.output.push_str("return ");
                self.write_expression(expression, PREC_LOWEST);
                self.output.push_str(";\n");
                self.indent = self.indent.saturating_sub(1);
                self.write_indent();
                self.output.push('}');
            }
            ArrowBody::Block(statements) => self.write_braced_statements(statements),
        }
    }

    fn expression_precedence(&self, expression: &Expression) -> u8 {
        match &expression.kind {
            ExpressionKind::Arrow { .. } | ExpressionKind::Assignment { .. } => PREC_ASSIGNMENT,
            ExpressionKind::Binary { operator, .. } => match operator {
                BinaryOperator::Add | BinaryOperator::Subtract => PREC_ADDITIVE,
                BinaryOperator::Multiply | BinaryOperator::Divide => PREC_MULTIPLICATIVE,
            },
            ExpressionKind::Call { .. } | ExpressionKind::Member { .. } => PREC_POSTFIX,
            ExpressionKind::As { expression, .. } => self.expression_precedence(expression),
            ExpressionKind::Identifier { .. }
            | ExpressionKind::Literal(_)
            | ExpressionKind::Object(_)
            | ExpressionKind::Array(_)
            | ExpressionKind::Parenthesized(_)
            | ExpressionKind::Missing => PREC_PRIMARY,
        }
    }

    fn write_declaration_variable(&mut self, declaration: &VariableDeclaration) {
        self.write_indent();
        if declaration.exported {
            self.output.push_str("export ");
        }
        self.output.push_str("declare ");
        self.output
            .push_str(variable_kind_text(declaration.declaration_kind));
        self.output.push(' ');
        self.output.push_str(&declaration.name);
        if let Some(annotation) = &declaration.annotation {
            self.output.push_str(": ");
            self.write_type(annotation, TYPE_PREC_LOWEST);
        } else if declaration.declaration_kind == VariableKind::Const {
            if let Some(Expression {
                kind: ExpressionKind::Literal(literal),
                span,
                ..
            }) = &declaration.initializer
            {
                if matches!(literal, Literal::Null) {
                    self.output.push_str(": null");
                } else {
                    self.output.push_str(" = ");
                    let text = self.literal_text(literal, *span);
                    self.output.push_str(&text);
                }
            } else {
                self.output.push_str(": unknown");
            }
        } else {
            self.output.push_str(": unknown");
        }
        self.output.push_str(";\n");
    }

    fn write_declaration_function(&mut self, declaration: &FunctionDeclaration) {
        self.write_indent();
        if declaration.exported {
            self.output.push_str("export ");
        }
        self.output.push_str("declare function ");
        self.output.push_str(&declaration.name);
        self.write_type_parameters(&declaration.type_parameters);
        self.write_declaration_parameters(&declaration.parameters);
        self.output.push_str(": ");
        if let Some(return_type) = &declaration.return_type {
            self.write_type(return_type, TYPE_PREC_LOWEST);
        } else {
            self.output.push_str("unknown");
        }
        self.output.push_str(";\n");
    }

    fn write_declaration_type_alias(&mut self, declaration: &TypeAliasDeclaration) {
        self.write_indent();
        if declaration.exported {
            self.output.push_str("export ");
        }
        self.output.push_str("type ");
        self.output.push_str(&declaration.name);
        self.write_type_parameters(&declaration.type_parameters);
        self.output.push_str(" = ");
        self.write_type(&declaration.ty, TYPE_PREC_LOWEST);
        self.output.push_str(";\n");
    }

    fn write_declaration_interface(&mut self, declaration: &InterfaceDeclaration) {
        self.write_indent();
        if declaration.exported {
            self.output.push_str("export ");
        }
        self.output.push_str("interface ");
        self.output.push_str(&declaration.name);
        self.write_type_parameters(&declaration.type_parameters);
        self.output.push_str(" {\n");
        self.indent += 1;
        for property in &declaration.properties {
            self.write_indent();
            self.write_type_property(property);
            self.output.push('\n');
        }
        self.indent = self.indent.saturating_sub(1);
        self.write_indent();
        self.output.push_str("}\n");
    }

    fn write_declaration_parameters(&mut self, parameters: &[Parameter]) {
        self.output.push('(');
        for (index, parameter) in parameters.iter().enumerate() {
            if index != 0 {
                self.output.push_str(", ");
            }
            self.output.push_str(&parameter.name);
            if parameter.optional {
                self.output.push('?');
            }
            self.output.push_str(": ");
            if let Some(annotation) = &parameter.annotation {
                self.write_type(annotation, TYPE_PREC_LOWEST);
            } else {
                self.output.push_str("unknown");
            }
        }
        self.output.push(')');
    }

    fn write_type_parameters(&mut self, parameters: &[String]) {
        if parameters.is_empty() {
            return;
        }
        self.output.push('<');
        for (index, parameter) in parameters.iter().enumerate() {
            if index != 0 {
                self.output.push_str(", ");
            }
            self.output.push_str(parameter);
        }
        self.output.push('>');
    }

    fn write_type(&mut self, ty: &TypeNode, parent_precedence: u8) {
        let precedence = type_precedence(ty);
        let parenthesize = precedence < parent_precedence;
        if parenthesize {
            self.output.push('(');
        }

        match &ty.kind {
            TypeNodeKind::Keyword(keyword) => self.output.push_str(keyword_type_text(*keyword)),
            TypeNodeKind::Literal(literal) => {
                let text = self.literal_text(literal, ty.span);
                self.output.push_str(&text);
            }
            TypeNodeKind::Array(element) => {
                self.write_type(element, TYPE_PREC_POSTFIX);
                self.output.push_str("[]");
            }
            TypeNodeKind::Tuple(elements) => {
                self.output.push('[');
                for (index, element) in elements.iter().enumerate() {
                    if index != 0 {
                        self.output.push_str(", ");
                    }
                    self.write_type(element, TYPE_PREC_LOWEST);
                }
                self.output.push(']');
            }
            TypeNodeKind::Union(types) => self.write_type_list(types, " | ", precedence),
            TypeNodeKind::Intersection(types) => self.write_type_list(types, " & ", precedence),
            TypeNodeKind::Object(properties) => {
                self.output.push('{');
                if !properties.is_empty() {
                    self.output.push(' ');
                }
                for property in properties {
                    self.write_type_property(property);
                    self.output.push(' ');
                }
                self.output.push('}');
            }
            TypeNodeKind::Function {
                parameters,
                return_type,
            } => {
                self.write_declaration_parameters(parameters);
                self.output.push_str(" => ");
                self.write_type(return_type, TYPE_PREC_FUNCTION);
            }
            TypeNodeKind::Reference {
                name, arguments, ..
            } => {
                self.output.push_str(name);
                if !arguments.is_empty() {
                    self.output.push('<');
                    for (index, argument) in arguments.iter().enumerate() {
                        if index != 0 {
                            self.output.push_str(", ");
                        }
                        self.write_type(argument, TYPE_PREC_LOWEST);
                    }
                    self.output.push('>');
                }
            }
            TypeNodeKind::KeyOf(operand) => {
                self.output.push_str("keyof ");
                self.write_type(operand, TYPE_PREC_PREFIX);
            }
            TypeNodeKind::IndexedAccess { object, index } => {
                self.write_type(object, TYPE_PREC_POSTFIX);
                self.output.push('[');
                self.write_type(index, TYPE_PREC_LOWEST);
                self.output.push(']');
            }
            TypeNodeKind::Parenthesized(inner) => {
                self.output.push('(');
                self.write_type(inner, TYPE_PREC_LOWEST);
                self.output.push(')');
            }
            TypeNodeKind::Missing => self.output.push_str("unknown"),
        }

        if parenthesize {
            self.output.push(')');
        }
    }

    fn write_type_list(&mut self, types: &[TypeNode], separator: &str, precedence: u8) {
        if types.is_empty() {
            self.output.push_str("never");
            return;
        }
        for (index, ty) in types.iter().enumerate() {
            if index != 0 {
                self.output.push_str(separator);
            }
            self.write_type(ty, precedence);
        }
    }

    fn write_type_property(&mut self, property: &TypeProperty) {
        if property.readonly {
            self.output.push_str("readonly ");
        }
        self.write_property_name(&property.name, property.name_span);
        if property.optional {
            self.output.push('?');
        }
        self.output.push_str(": ");
        self.write_type(&property.ty, TYPE_PREC_LOWEST);
        self.output.push(';');
    }

    fn write_property_name(&mut self, name: &str, span: Span) {
        let raw = self.source.slice(span).trim();
        if raw_is_property_name(raw) {
            self.output.push_str(raw);
        } else if is_identifier_name(name) || is_numeric_property_name(name) {
            self.output.push_str(name);
        } else {
            self.output.push_str(&quote_string(name));
        }
    }

    fn literal_text(&self, literal: &Literal, span: Span) -> String {
        match literal {
            Literal::String(value) => {
                let raw = self.source.slice(span).trim();
                if is_quoted(raw) {
                    raw.to_string()
                } else {
                    quote_string(value)
                }
            }
            Literal::Number(value) => value.clone(),
            Literal::Boolean(value) => value.to_string(),
            Literal::Null => "null".to_string(),
        }
    }

    const fn runtime_variable_kind(&self, kind: VariableKind) -> &'static str {
        if self.preserve_block_scope {
            variable_kind_text(kind)
        } else {
            "var"
        }
    }

    fn statement_is_runtime_export(&self, statement: &Statement) -> bool {
        match &statement.kind {
            StatementKind::Variable(declaration) => declaration.exported,
            StatementKind::Function(declaration) => {
                declaration.exported && !declaration.declared && self.function_has_body(statement)
            }
            StatementKind::TypeAlias(_)
            | StatementKind::Interface(_)
            | StatementKind::Return(_)
            | StatementKind::Block(_)
            | StatementKind::Expression(_)
            | StatementKind::Empty
            | StatementKind::Unknown => false,
        }
    }

    fn function_has_body(&self, statement: &Statement) -> bool {
        self.source.slice(statement.span).trim_end().ends_with('}')
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent {
            self.output.push_str("    ");
        }
    }
}

const PREC_LOWEST: u8 = 0;
const PREC_ASSIGNMENT: u8 = 1;
const PREC_ADDITIVE: u8 = 2;
const PREC_MULTIPLICATIVE: u8 = 3;
const PREC_POSTFIX: u8 = 4;
const PREC_PRIMARY: u8 = 5;

const TYPE_PREC_LOWEST: u8 = 0;
const TYPE_PREC_FUNCTION: u8 = 1;
const TYPE_PREC_UNION: u8 = 2;
const TYPE_PREC_INTERSECTION: u8 = 3;
const TYPE_PREC_PREFIX: u8 = 4;
const TYPE_PREC_POSTFIX: u8 = 5;
const TYPE_PREC_PRIMARY: u8 = 6;

const fn type_precedence(ty: &TypeNode) -> u8 {
    match &ty.kind {
        TypeNodeKind::Function { .. } => TYPE_PREC_FUNCTION,
        TypeNodeKind::Union(_) => TYPE_PREC_UNION,
        TypeNodeKind::Intersection(_) => TYPE_PREC_INTERSECTION,
        TypeNodeKind::KeyOf(_) => TYPE_PREC_PREFIX,
        TypeNodeKind::Array(_) | TypeNodeKind::IndexedAccess { .. } => TYPE_PREC_POSTFIX,
        TypeNodeKind::Keyword(_)
        | TypeNodeKind::Literal(_)
        | TypeNodeKind::Tuple(_)
        | TypeNodeKind::Object(_)
        | TypeNodeKind::Reference { .. }
        | TypeNodeKind::Parenthesized(_)
        | TypeNodeKind::Missing => TYPE_PREC_PRIMARY,
    }
}

const fn statement_is_exported(statement: &Statement) -> bool {
    match &statement.kind {
        StatementKind::Variable(declaration) => declaration.exported,
        StatementKind::Function(declaration) => declaration.exported,
        StatementKind::TypeAlias(declaration) => declaration.exported,
        StatementKind::Interface(declaration) => declaration.exported,
        StatementKind::Return(_)
        | StatementKind::Block(_)
        | StatementKind::Expression(_)
        | StatementKind::Empty
        | StatementKind::Unknown => false,
    }
}

fn object_property_is_shorthand(property: &ObjectProperty) -> bool {
    matches!(
        &property.value.kind,
        ExpressionKind::Identifier { name, .. }
            if name == &property.name && property.value.span == property.name_span
    )
}

const fn binary_operator_text(operator: BinaryOperator) -> &'static str {
    match operator {
        BinaryOperator::Add => "+",
        BinaryOperator::Subtract => "-",
        BinaryOperator::Multiply => "*",
        BinaryOperator::Divide => "/",
    }
}

const fn variable_kind_text(kind: VariableKind) -> &'static str {
    match kind {
        VariableKind::Let => "let",
        VariableKind::Const => "const",
        VariableKind::Var => "var",
    }
}

const fn keyword_type_text(keyword: KeywordType) -> &'static str {
    match keyword {
        KeywordType::Any => "any",
        KeywordType::Unknown => "unknown",
        KeywordType::Never => "never",
        KeywordType::Void => "void",
        KeywordType::Undefined => "undefined",
        KeywordType::Null => "null",
        KeywordType::Boolean => "boolean",
        KeywordType::Number => "number",
        KeywordType::String => "string",
        KeywordType::BigInt => "bigint",
    }
}

fn output_path(source: &Path, directory: Option<&Path>, declaration: bool) -> PathBuf {
    let name = output_file_name(source, declaration);
    match directory {
        Some(root) => root.join(name),
        None => source.with_file_name(name),
    }
}

fn output_file_name(source: &Path, declaration: bool) -> String {
    let stem = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("output");
    let extension = source
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if declaration {
        match extension.as_str() {
            "mts" => format!("{stem}.d.mts"),
            "cts" => format!("{stem}.d.cts"),
            _ => format!("{stem}.d.ts"),
        }
    } else {
        match extension.as_str() {
            "mts" => format!("{stem}.mjs"),
            "cts" => format!("{stem}.cjs"),
            _ => format!("{stem}.js"),
        }
    }
}

fn is_declaration_source(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    name.ends_with(".d.ts") || name.ends_with(".d.mts") || name.ends_with(".d.cts")
}

fn raw_is_property_name(raw: &str) -> bool {
    is_quoted(raw) || is_identifier_name(raw) || is_numeric_property_name(raw)
}

fn is_quoted(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= 2
        && matches!(bytes.first(), Some(b'\'') | Some(b'"'))
        && bytes.first() == bytes.last()
}

fn is_identifier_name(text: &str) -> bool {
    let mut characters = text.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first == '_' || first == '$' || first.is_alphabetic())
        && characters.all(|character| {
            character == '_'
                || character == '$'
                || character == '\u{200c}'
                || character == '\u{200d}'
                || character.is_alphanumeric()
        })
}

fn is_numeric_property_name(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|character| character.is_ascii_digit() || matches!(character, '.' | '-' | '+'))
        && text.parse::<f64>().is_ok()
}

fn quote_string(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for character in value.chars() {
        match character {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            '\u{08}' => quoted.push_str("\\b"),
            '\u{0c}' => quoted.push_str("\\f"),
            '\u{2028}' => quoted.push_str("\\u2028"),
            '\u{2029}' => quoted.push_str("\\u2029"),
            character if character.is_control() => {
                let code = u32::from(character);
                quoted.push_str(&format!("\\u{code:04x}"));
            }
            character => quoted.push(character),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use crate::bind::bind_source;
    use crate::program::{CompilerOptions, ProgramFile};
    use crate::source::{FileId, SourceText};
    use crate::syntax::parse_source;

    use super::emit_file;

    fn program_file(path: &str, text: &str) -> ProgramFile {
        let source = SourceText::new(FileId(0), PathBuf::from(path), Arc::<str>::from(text));
        let parsed = parse_source(&source);
        assert!(
            parsed.diagnostics.is_empty(),
            "test source must parse without diagnostics: {:?}",
            parsed.diagnostics
        );
        let bindings = bind_source(source.id, &parsed.unit);
        ProgramFile {
            source,
            syntax: parsed.unit,
            bindings,
        }
    }

    #[test]
    fn erases_type_only_syntax_and_annotations() {
        let file = program_file(
            "input.ts",
            concat!(
                "interface Point { x: number; }\n",
                "type Scalar = number;\n",
                "const point: Point = { x: 1 };\n",
                "function add(a: number, b: number): number { return a + b; }\n",
            ),
        );
        let options = CompilerOptions {
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        };

        let output = emit_file(&file, &options);
        assert_eq!(output.len(), 1);
        assert_eq!(
            output[0].text,
            concat!(
                "\"use strict\";\n",
                "const point = { x: 1 };\n",
                "function add(a, b) {\n",
                "    return a + b;\n",
                "}\n",
            )
        );
    }

    #[test]
    fn emits_written_declaration_shapes_without_checking() {
        let file = program_file(
            "src/api.ts",
            concat!(
                "export const greeting: string = \"hello\";\n",
                "export interface Box<T> { readonly value?: T; }\n",
                "export function id<T>(value: T): T { return value; }\n",
            ),
        );
        let options = CompilerOptions {
            declaration: true,
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            out_dir: Some(PathBuf::from("dist")),
            declaration_dir: Some(PathBuf::from("types")),
            ..CompilerOptions::default()
        };

        let output = emit_file(&file, &options);
        assert_eq!(output.len(), 2);
        assert_eq!(output[0].path, Path::new("dist/api.js"));
        assert_eq!(output[1].path, Path::new("types/api.d.ts"));
        assert_eq!(
            output[0].text,
            concat!(
                "export const greeting = \"hello\";\n",
                "export function id(value) {\n",
                "    return value;\n",
                "}\n",
            )
        );
        assert_eq!(
            output[1].text,
            concat!(
                "export declare const greeting: string;\n",
                "export interface Box<T> {\n",
                "    readonly value?: T;\n",
                "}\n",
                "export declare function id<T>(value: T): T;\n",
            )
        );
    }

    #[test]
    fn emits_basic_commonjs_and_downlevels_block_scope() {
        let file = program_file("value.ts", "export const value: number = 1;\n");
        let output = emit_file(&file, &CompilerOptions::default());
        assert_eq!(
            output[0].text,
            concat!(
                "\"use strict\";\n",
                "Object.defineProperty(exports, \"__esModule\", { value: true });\n",
                "var value = 1;\n",
                "exports.value = value;\n",
            )
        );
    }

    #[test]
    fn keeps_expression_grouping_while_erasing_assertions() {
        let file = program_file(
            "math.ts",
            "const result: number = (1 + 2) * (3 as number);\n",
        );
        let options = CompilerOptions {
            target: "esnext".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        };
        let output = emit_file(&file, &options);
        assert_eq!(
            output[0].text,
            "\"use strict\";\nconst result = (1 + 2) * 3;\n"
        );
    }

    #[test]
    fn derives_module_extension_outputs_without_losing_module_identity() {
        let file = program_file("src/value.mts", "const value: number = 1;\n");
        let options = CompilerOptions {
            declaration: true,
            target: "esnext".to_string(),
            module: "nodenext".to_string(),
            ..CompilerOptions::default()
        };
        let output = emit_file(&file, &options);
        assert_eq!(output[0].path, Path::new("src/value.mjs"));
        assert_eq!(output[1].path, Path::new("src/value.d.mts"));
        assert_eq!(output[0].text, "const value = 1;\nexport {};\n");
        assert_eq!(output[1].text, "declare const value: number;\nexport {};\n");
    }
}
