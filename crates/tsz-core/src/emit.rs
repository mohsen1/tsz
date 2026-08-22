//! Deterministic JavaScript and declaration emit for the supported syntax tree.
//!
//! Emit is deliberately a syntax transform. It erases type-only syntax and
//! prints runtime nodes; it does not validate types or recover semantic facts.

mod literals;
mod reachability;
mod regular_expression;
mod type_members;

use crate::emit_paths::{EmitFilePlan, EmitPlan, is_declaration_source, is_effective_commonjs};
use crate::program::{CompilerOptions, EmittedFile, ProgramFile};
use crate::source::{SourceText, Span};
use crate::syntax::{
    AccessorKind, ArrowBody, BinaryOperator, ClassDeclaration, ClassMemberKind, ExportDeclaration,
    Expression, ExpressionKind, FunctionDeclaration, ImportDeclaration, InterfaceDeclaration,
    KeywordType, ObjectProperty, Parameter, SourceUnit, Statement, StatementKind, SwitchClauseKind,
    TypeAliasDeclaration, TypeNode, TypeNodeKind, UnaryOperator, VariableDeclaration, VariableKind,
    erased_assertion_expression,
};

/// Emit the JavaScript product and, when requested, its declaration product.
///
/// Output ordering is stable: JavaScript precedes declarations for a source
/// file, and the program layer performs the final path sort across files.
#[must_use]
pub fn emit_file(file: &ProgramFile, options: &CompilerOptions) -> Vec<EmittedFile> {
    let plan = EmitPlan::for_program(
        std::slice::from_ref(file),
        options,
        &crate::config::ProjectProvenance::default(),
    );
    emit_file_with_plan(file, options, plan.for_file(file.source.id))
}

pub(crate) fn emit_file_with_plan(
    file: &ProgramFile,
    options: &CompilerOptions,
    plan: &EmitFilePlan,
) -> Vec<EmittedFile> {
    if is_declaration_source(&file.source.path) {
        return Vec::new();
    }
    if file.syntax.contains_recovered_type_members() {
        return Vec::new();
    }

    let mut emitted = Vec::with_capacity(usize::from(plan.declaration.is_some()) + 1);
    if let Some(javascript_path) = &plan.javascript {
        let mut javascript = Printer::new(&file.source, options);
        javascript.emit_javascript(&file.syntax);
        if javascript.javascript_supported {
            emitted.push(EmittedFile {
                path: javascript_path.clone(),
                text: javascript.finish(),
                declaration: false,
            });
        }
    }

    if let Some(declaration_path) = &plan.declaration
        && !reachability::requires_checked_declaration_reachability(file)
    {
        let mut declarations = Printer::new(&file.source, options);
        declarations.emit_declarations(&file.syntax);
        if declarations.declaration_supported {
            emitted.push(EmittedFile {
                path: declaration_path.clone(),
                text: declarations.finish(),
                declaration: true,
            });
        }
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
    preserve_class_fields: bool,
    preserve_numeric_separators: bool,
    preserve_comments: bool,
    emitting_declaration: bool,
    javascript_supported: bool,
    declaration_supported: bool,
    declaration_parameter_property_host: bool,
}

impl<'a> Printer<'a> {
    fn new(source: &'a SourceText, options: &CompilerOptions) -> Self {
        let target = options.target.trim().to_ascii_lowercase();
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
            module_format: if is_effective_commonjs(&source.path, &options.module) {
                ModuleFormat::CommonJs
            } else {
                ModuleFormat::EsModule
            },
            implicit_external_module,
            preserve_block_scope: !matches!(target.as_str(), "es3" | "es5"),
            preserve_arrows: !matches!(target.as_str(), "es3" | "es5"),
            preserve_class_fields: matches!(
                target.as_str(),
                "es2022" | "es2023" | "es2024" | "es2025" | "esnext"
            ),
            preserve_numeric_separators: matches!(
                target.as_str(),
                "es2021" | "es2022" | "es2023" | "es2024" | "es2025" | "esnext"
            ),
            preserve_comments: !options.remove_comments,
            emitting_declaration: false,
            javascript_supported: true,
            declaration_supported: true,
            declaration_parameter_property_host: false,
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

        if self.preserve_comments
            && let Some(comments) = unit.modeled_comments()
        {
            self.write_modeled_comment_statements(&unit.statements, comments);
        } else {
            for statement in &unit.statements {
                self.write_javascript_statement(statement, true);
            }
        }

        if external_module && !has_runtime_export && self.module_format == ModuleFormat::EsModule {
            self.output.push_str("export {};\n");
        }
    }

    fn emit_declarations(&mut self, unit: &SourceUnit) {
        self.emitting_declaration = true;
        let has_export = unit.statements.iter().any(statement_is_exported);
        for statement in &unit.statements {
            match &statement.kind {
                StatementKind::Import(declaration) => {
                    self.write_declaration_import(statement, declaration);
                }
                StatementKind::Export(declaration) => {
                    self.write_declaration_export(statement, declaration);
                }
                StatementKind::Variable(declaration) => {
                    self.write_declaration_variable(declaration);
                }
                StatementKind::Function(declaration) => {
                    self.write_declaration_function(declaration);
                }
                StatementKind::Class(declaration) => {
                    self.write_declaration_class(declaration);
                }
                StatementKind::TypeAlias(declaration) => {
                    self.write_declaration_type_alias(declaration);
                }
                StatementKind::Interface(declaration) => {
                    self.write_declaration_interface(declaration);
                }
                StatementKind::Return(_)
                | StatementKind::If(_)
                | StatementKind::Switch(_)
                | StatementKind::Break(_)
                | StatementKind::Continue(_)
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
            StatementKind::Import(declaration) => {
                self.write_javascript_import(statement, declaration);
            }
            StatementKind::Export(declaration) => {
                self.write_javascript_export(statement, declaration);
            }
            StatementKind::Variable(declaration) => {
                self.write_javascript_variable(declaration, top_level);
            }
            StatementKind::Function(declaration) => {
                self.write_javascript_function(statement, declaration, top_level);
            }
            StatementKind::Class(declaration) => {
                self.write_javascript_class(declaration, top_level);
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
            StatementKind::If(control_flow) => self.write_javascript_if(control_flow),
            StatementKind::Switch(control_flow) => {
                self.write_indent();
                self.output.push_str("switch (");
                self.write_expression(&control_flow.expression, PREC_LOWEST);
                self.output.push_str(") {\n");
                self.indent += 1;
                for clause in &control_flow.clauses {
                    self.write_indent();
                    match &clause.kind {
                        SwitchClauseKind::Case(expression) => {
                            self.output.push_str("case ");
                            self.write_expression(expression, PREC_LOWEST);
                            self.output.push(':');
                        }
                        SwitchClauseKind::Default => self.output.push_str("default:"),
                    }
                    self.output.push('\n');
                    self.indent += 1;
                    for nested in &clause.statements {
                        self.write_javascript_statement(nested, false);
                    }
                    self.indent = self.indent.saturating_sub(1);
                }
                self.indent = self.indent.saturating_sub(1);
                self.write_indent();
                self.output.push_str("}\n");
            }
            StatementKind::Break(jump) => {
                self.write_jump_statement("break", jump.label.as_deref());
            }
            StatementKind::Continue(jump) => {
                self.write_jump_statement("continue", jump.label.as_deref());
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

    fn write_javascript_if(&mut self, control_flow: &crate::syntax::IfStatement) {
        self.write_indent();
        self.output.push_str("if (");
        self.write_expression(&control_flow.condition, PREC_LOWEST);
        self.output.push(')');
        self.write_control_flow_body(&control_flow.then_statement);
        if let Some(else_statement) = &control_flow.else_statement {
            self.write_indent();
            self.output.push_str("else");
            self.write_control_flow_body(else_statement);
        }
    }

    fn write_control_flow_body(&mut self, statement: &Statement) {
        if let StatementKind::Block(statements) = &statement.kind {
            self.output.push(' ');
            self.write_braced_statements(statements);
            self.output.push('\n');
        } else {
            self.output.push('\n');
            self.indent += 1;
            self.write_javascript_statement(statement, false);
            self.indent = self.indent.saturating_sub(1);
        }
    }

    fn write_jump_statement(&mut self, keyword: &str, label: Option<&str>) {
        self.write_indent();
        self.output.push_str(keyword);
        if let Some(label) = label {
            self.output.push(' ');
            self.output.push_str(label);
        }
        self.output.push_str(";\n");
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

    fn write_javascript_import(&mut self, _statement: &Statement, declaration: &ImportDeclaration) {
        let has_runtime_binding = declaration
            .bindings
            .iter()
            .any(|binding| !binding.type_only);
        if declaration.type_only || (!declaration.side_effect_only && !has_runtime_binding) {
            return;
        }
        if self.module_format == ModuleFormat::EsModule {
            self.write_esmodule_import(declaration);
            return;
        }
        let module = quote_string(&declaration.module_specifier);
        if declaration.side_effect_only {
            self.write_indent();
            self.output.push_str("require(");
            self.output.push_str(&module);
            self.output.push_str(");\n");
            return;
        }
        for binding in declaration
            .bindings
            .iter()
            .filter(|binding| !binding.type_only)
        {
            self.write_indent();
            self.output
                .push_str(self.runtime_variable_kind(VariableKind::Const));
            self.output.push(' ');
            self.output.push_str(&binding.local);
            self.output.push_str(" = require(");
            self.output.push_str(&module);
            self.output.push(')');
            if !binding.namespace {
                self.output.push('.');
                self.output
                    .push_str(binding.imported.as_deref().unwrap_or("default"));
            }
            self.output.push_str(";\n");
        }
    }

    fn write_esmodule_import(&mut self, declaration: &ImportDeclaration) {
        self.write_indent();
        self.output.push_str("import ");
        if declaration.side_effect_only {
            self.write_module_specifier(&declaration.module_specifier, declaration.module_span);
            self.output.push_str(";\n");
            return;
        }

        // The parser records the default clause first. A future syntax-model
        // port should distinguish it explicitly from `{ default as local }`.
        let default_binding = declaration.bindings.first().filter(|binding| {
            !binding.type_only
                && !binding.namespace
                && binding.imported.as_deref() == Some("default")
        });
        let namespace_binding = declaration
            .bindings
            .iter()
            .find(|binding| !binding.type_only && binding.namespace);
        let named_bindings = declaration
            .bindings
            .iter()
            .enumerate()
            .filter(|(index, binding)| {
                !binding.type_only
                    && !binding.namespace
                    && !(default_binding.is_some() && *index == 0)
            })
            .map(|(_, binding)| binding)
            .collect::<Vec<_>>();

        let mut wrote_clause = false;
        if let Some(binding) = default_binding {
            self.output.push_str(&binding.local);
            wrote_clause = true;
        }
        if let Some(binding) = namespace_binding {
            if wrote_clause {
                self.output.push_str(", ");
            }
            self.output.push_str("* as ");
            self.output.push_str(&binding.local);
        } else if !named_bindings.is_empty() {
            if wrote_clause {
                self.output.push_str(", ");
            }
            self.output.push_str("{ ");
            for (index, binding) in named_bindings.iter().enumerate() {
                if index != 0 {
                    self.output.push_str(", ");
                }
                let imported = binding.imported.as_deref().unwrap_or(&binding.local);
                self.output.push_str(imported);
                if imported != binding.local {
                    self.output.push_str(" as ");
                    self.output.push_str(&binding.local);
                }
            }
            self.output.push_str(" }");
        }
        self.output.push_str(" from ");
        self.write_module_specifier(&declaration.module_specifier, declaration.module_span);
        self.output.push_str(";\n");
    }

    fn write_javascript_export(&mut self, _statement: &Statement, declaration: &ExportDeclaration) {
        if declaration.type_only
            || (!declaration.export_all
                && declaration.assignment.is_none()
                && declaration
                    .specifiers
                    .iter()
                    .all(|specifier| specifier.type_only))
        {
            return;
        }
        if self.module_format == ModuleFormat::EsModule {
            self.write_esmodule_export(declaration);
            return;
        }
        if let Some(assignment) = &declaration.assignment {
            self.write_indent();
            self.output.push_str(if declaration.default_export {
                "exports.default = "
            } else {
                "module.exports = "
            });
            self.write_expression(assignment, PREC_ASSIGNMENT);
            self.output.push_str(";\n");
            return;
        }
        if declaration.export_all {
            if let Some(module) = &declaration.module_specifier {
                self.write_indent();
                self.output.push_str("Object.assign(exports, require(");
                self.output.push_str(&quote_string(module));
                self.output.push_str("));\n");
            }
            return;
        }
        for specifier in declaration
            .specifiers
            .iter()
            .filter(|specifier| !specifier.type_only)
        {
            self.write_indent();
            self.output.push_str("exports.");
            self.output.push_str(&specifier.exported);
            self.output.push_str(" = ");
            if let Some(module) = &declaration.module_specifier {
                self.output.push_str("require(");
                self.output.push_str(&quote_string(module));
                self.output.push_str(").");
            }
            self.output.push_str(&specifier.local);
            self.output.push_str(";\n");
        }
    }

    fn write_esmodule_export(&mut self, declaration: &ExportDeclaration) {
        if let Some(assignment) = &declaration.assignment {
            if !declaration.default_export {
                return;
            }
            self.write_indent();
            self.output.push_str("export default ");
            self.write_expression(assignment, PREC_ASSIGNMENT);
            self.output.push_str(";\n");
            return;
        }

        self.write_indent();
        if declaration.export_all {
            self.output.push_str("export *");
            if let Some(specifier) = declaration
                .specifiers
                .iter()
                .find(|specifier| !specifier.type_only)
            {
                self.output.push_str(" as ");
                self.output.push_str(&specifier.exported);
            }
        } else {
            self.output.push_str("export { ");
            let mut first = true;
            for specifier in declaration
                .specifiers
                .iter()
                .filter(|specifier| !specifier.type_only)
            {
                if !first {
                    self.output.push_str(", ");
                }
                first = false;
                self.output.push_str(&specifier.local);
                if specifier.local != specifier.exported {
                    self.output.push_str(" as ");
                    self.output.push_str(&specifier.exported);
                }
            }
            self.output.push_str(" }");
        }
        if let (Some(module), Some(span)) = (
            declaration.module_specifier.as_deref(),
            declaration.module_span,
        ) {
            self.output.push_str(" from ");
            self.write_module_specifier(module, span);
        }
        self.output.push_str(";\n");
    }

    fn write_javascript_class(&mut self, declaration: &ClassDeclaration, top_level: bool) {
        if declaration.declared {
            return;
        }
        self.write_indent();
        if top_level && declaration.exported && self.module_format == ModuleFormat::EsModule {
            self.output.push_str("export ");
            if declaration.default_export {
                self.output.push_str("default ");
            }
        }
        self.output.push_str("class ");
        self.output.push_str(&declaration.name);
        if let Some(base) = &declaration.extends {
            self.output.push_str(" extends ");
            self.write_heritage_type(base);
        }
        self.output.push_str(" {\n");
        self.indent += 1;
        for member in &declaration.members {
            if member.modifiers.declared
                || member.modifiers.abstract_member
                || matches!(
                    &member.kind,
                    ClassMemberKind::Constructor {
                        has_body: false,
                        ..
                    } | ClassMemberKind::Method {
                        has_body: false,
                        ..
                    }
                )
            {
                continue;
            }
            if self.preserve_class_fields
                && let ClassMemberKind::Constructor { parameters, .. } = &member.kind
            {
                self.write_parameter_property_fields(parameters);
            }
            self.write_indent();
            if member.modifiers.static_member {
                self.output.push_str("static ");
            }
            match &member.kind {
                ClassMemberKind::Constructor {
                    parameters, body, ..
                } => {
                    self.output.push_str("constructor");
                    self.write_runtime_parameters(parameters);
                    self.output.push(' ');
                    self.write_constructor_body(body, parameters, declaration.extends.is_some());
                    self.output.push('\n');
                }
                ClassMemberKind::Property { initializer, .. } => {
                    self.write_property_name(&member.name, member.name_span);
                    if let Some(initializer) = initializer {
                        self.output.push_str(" = ");
                        self.write_expression(initializer, PREC_ASSIGNMENT);
                    }
                    self.output.push_str(";\n");
                }
                ClassMemberKind::Method {
                    parameters,
                    body,
                    accessor,
                    ..
                } => {
                    if member.modifiers.async_member {
                        self.output.push_str("async ");
                    }
                    if let Some(accessor) = accessor {
                        self.output.push_str(match accessor {
                            AccessorKind::Get => "get ",
                            AccessorKind::Set => "set ",
                        });
                    }
                    self.write_property_name(&member.name, member.name_span);
                    self.write_runtime_parameters(parameters);
                    self.output.push(' ');
                    self.write_braced_statements(body);
                    self.output.push('\n');
                }
            }
        }
        self.indent = self.indent.saturating_sub(1);
        self.write_indent();
        self.output.push_str("}\n");
        if top_level && declaration.exported && self.module_format == ModuleFormat::CommonJs {
            self.write_commonjs_export(&declaration.name);
        }
    }

    fn write_raw_statement(&mut self, statement: &Statement) {
        self.write_indent();
        self.output
            .push_str(self.source.slice(statement.span).trim());
        self.output.push('\n');
    }

    fn write_module_specifier(&mut self, value: &str, span: Span) {
        let raw = self.source.slice(span).trim();
        if is_quoted(raw) {
            self.output.push_str(raw);
        } else {
            self.output.push_str(&quote_string(value));
        }
    }

    fn write_heritage_type(&mut self, ty: &TypeNode) {
        match &ty.kind {
            TypeNodeKind::Reference { name, .. } => self.output.push_str(name),
            TypeNodeKind::Parenthesized(inner) => {
                self.output.push('(');
                self.write_heritage_type(inner);
                self.output.push(')');
            }
            _ => self.output.push_str(self.source.slice(ty.span).trim()),
        }
    }

    fn write_braced_statements(&mut self, statements: &[Statement]) {
        if statements.is_empty() {
            self.output.push_str("{ }");
            return;
        }
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
            if parameter.rest {
                self.output.push_str("...");
            }
            self.output.push_str(&parameter.name);
            if let Some(initializer) = &parameter.initializer {
                self.output.push_str(" = ");
                self.write_expression(initializer, PREC_LOWEST);
            }
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
        if let Some(expression) = erased_assertion_expression(expression) {
            self.write_expression(expression, parent_precedence);
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
            ExpressionKind::RegularExpression(literal) => {
                self.write_regular_expression(literal);
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
            ExpressionKind::Call {
                callee, arguments, ..
            } => {
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
            ExpressionKind::New {
                callee,
                type_arguments,
                arguments,
            } => {
                self.output.push_str("new ");
                self.write_expression(callee, PREC_POSTFIX);
                self.write_type_arguments(type_arguments);
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
                self.write_member_object(object);
                self.output.push('.');
                self.output.push_str(name);
            }
            ExpressionKind::Arrow {
                parameters, body, ..
            } => {
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
            ExpressionKind::Unary { operator, operand } => {
                self.output.push_str(unary_operator_text(*operator));
                if unary_operator_is_keyword(*operator) {
                    self.output.push(' ');
                }
                self.write_expression(operand, PREC_UNARY);
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
                BinaryOperator::LogicalOr | BinaryOperator::NullishCoalesce => PREC_LOGICAL_OR,
                BinaryOperator::LogicalAnd => PREC_LOGICAL_AND,
                BinaryOperator::Equals
                | BinaryOperator::NotEquals
                | BinaryOperator::StrictEquals
                | BinaryOperator::StrictNotEquals => PREC_EQUALITY,
                BinaryOperator::LessThan
                | BinaryOperator::LessThanEquals
                | BinaryOperator::GreaterThan
                | BinaryOperator::GreaterThanEquals
                | BinaryOperator::In
                | BinaryOperator::InstanceOf => PREC_RELATIONAL,
                BinaryOperator::Add | BinaryOperator::Subtract => PREC_ADDITIVE,
                BinaryOperator::Multiply | BinaryOperator::Divide | BinaryOperator::Remainder => {
                    PREC_MULTIPLICATIVE
                }
            },
            ExpressionKind::Unary { .. } => PREC_UNARY,
            ExpressionKind::Call { .. }
            | ExpressionKind::New { .. }
            | ExpressionKind::Member { .. } => PREC_POSTFIX,
            ExpressionKind::As { expression, .. } => self.expression_precedence(expression),
            ExpressionKind::Identifier { .. }
            | ExpressionKind::Literal(_)
            | ExpressionKind::RegularExpression(_)
            | ExpressionKind::Object(_)
            | ExpressionKind::Array(_)
            | ExpressionKind::Parenthesized(_)
            | ExpressionKind::Missing => PREC_PRIMARY,
        }
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
        if !declaration.extends.is_empty() {
            self.output.push_str(" extends ");
            for (index, base) in declaration.extends.iter().enumerate() {
                if index != 0 {
                    self.output.push_str(", ");
                }
                self.write_type(base, TYPE_PREC_LOWEST);
            }
        }
        self.output.push_str(" {\n");
        self.indent += 1;
        for member in declaration
            .members
            .iter()
            .filter(|member| Self::type_member_is_emittable(member))
        {
            self.write_indent();
            self.write_type_member(member);
            self.output.push('\n');
        }
        self.indent = self.indent.saturating_sub(1);
        self.write_indent();
        self.output.push_str("}\n");
    }

    fn write_declaration_import(
        &mut self,
        statement: &Statement,
        _declaration: &ImportDeclaration,
    ) {
        self.write_raw_statement(statement);
    }

    fn write_declaration_export(&mut self, statement: &Statement, declaration: &ExportDeclaration) {
        if declaration
            .assignment
            .as_ref()
            .is_some_and(literals::expression_contains_template)
        {
            self.declaration_supported = false;
        } else if declaration.assignment.is_none() {
            self.write_raw_statement(statement);
        }
    }

    fn write_declaration_class(&mut self, declaration: &ClassDeclaration) {
        self.write_indent();
        if declaration.exported {
            self.output.push_str("export ");
            if declaration.default_export {
                self.output.push_str("default ");
            }
        }
        self.output.push_str("declare class ");
        self.output.push_str(&declaration.name);
        self.write_type_parameters(&declaration.type_parameters);
        if let Some(base) = &declaration.extends {
            self.output.push_str(" extends ");
            self.write_type(base, TYPE_PREC_LOWEST);
        }
        if !declaration.implements.is_empty() {
            self.output.push_str(" implements ");
            for (index, implemented) in declaration.implements.iter().enumerate() {
                if index != 0 {
                    self.output.push_str(", ");
                }
                self.write_type(implemented, TYPE_PREC_LOWEST);
            }
        }
        self.output.push_str(" {\n");
        self.indent += 1;
        for member in &declaration.members {
            if let ClassMemberKind::Constructor {
                parameters,
                has_body: true,
                ..
            } = &member.kind
            {
                self.write_parameter_property_declarations(parameters);
            }
            self.write_indent();
            if member.modifiers.private {
                self.output.push_str("private ");
            } else if member.modifiers.protected {
                self.output.push_str("protected ");
            } else if member.modifiers.public {
                self.output.push_str("public ");
            }
            if member.modifiers.static_member {
                self.output.push_str("static ");
            }
            if member.modifiers.readonly {
                self.output.push_str("readonly ");
            }
            match &member.kind {
                ClassMemberKind::Constructor { parameters, .. } => {
                    self.output.push_str("constructor");
                    let previous = self.declaration_parameter_property_host;
                    self.declaration_parameter_property_host = true;
                    self.write_declaration_parameters(parameters);
                    self.declaration_parameter_property_host = previous;
                    self.output.push_str(";\n");
                }
                ClassMemberKind::Property {
                    annotation,
                    initializer,
                    optional,
                    ..
                } => {
                    self.write_property_name(&member.name, member.name_span);
                    if *optional {
                        self.output.push('?');
                    }
                    self.output.push_str(": ");
                    if let Some(annotation) = annotation {
                        self.write_type(annotation, TYPE_PREC_LOWEST);
                    } else {
                        if initializer
                            .as_ref()
                            .is_some_and(literals::expression_contains_template)
                        {
                            self.declaration_supported = false;
                        }
                        self.output.push_str("unknown");
                    }
                    self.output.push_str(";\n");
                }
                ClassMemberKind::Method {
                    type_parameters,
                    parameters,
                    return_type,
                    body,
                    has_body,
                    accessor,
                    ..
                } => {
                    if let Some(accessor) = accessor {
                        self.output.push_str(match accessor {
                            AccessorKind::Get => "get ",
                            AccessorKind::Set => "set ",
                        });
                    }
                    self.write_property_name(&member.name, member.name_span);
                    self.write_type_parameters(type_parameters);
                    self.write_declaration_parameters(parameters);
                    self.output.push_str(": ");
                    if let Some(return_type) = return_type {
                        self.write_type(return_type, TYPE_PREC_LOWEST);
                    } else if !has_body {
                        self.output.push_str("any");
                    } else if body.is_empty() {
                        self.output.push_str("void");
                    } else {
                        self.declaration_supported = false;
                        self.output.push_str("unknown");
                    }
                    self.output.push_str(";\n");
                }
            }
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
            if parameter.rest {
                self.output.push_str("...");
            }
            self.output.push_str(&parameter.name);
            if parameter.optional || parameter.initializer.is_some() {
                self.output.push('?');
            }
            self.output.push_str(": ");
            self.write_declaration_parameter_type(parameter);
            if self.declaration_parameter_property_host
                && parameter.optional
                && type_members::is_parameter_property(parameter)
                && parameter.annotation.as_ref().is_none_or(|annotation| {
                    !type_members::optional_type_absorbs_undefined(annotation)
                })
            {
                self.output.push_str(" | undefined");
            }
        }
        self.output.push(')');
    }

    fn write_type_parameters(&mut self, parameters: &[crate::syntax::TypeParameterDeclaration]) {
        if parameters.is_empty() {
            return;
        }
        self.output.push('<');
        for (index, parameter) in parameters.iter().enumerate() {
            if index != 0 {
                self.output.push_str(", ");
            }
            if parameter.const_parameter {
                self.output.push_str("const ");
            }
            if parameter.in_variance {
                self.output.push_str("in ");
            }
            if parameter.out_variance {
                self.output.push_str("out ");
            }
            self.output.push_str(&parameter.name);
            if let Some(constraint) = &parameter.constraint {
                self.output.push_str(" extends ");
                self.write_type(constraint, TYPE_PREC_LOWEST);
            }
            if let Some(default) = &parameter.default {
                self.output.push_str(" = ");
                self.write_type(default, TYPE_PREC_LOWEST);
            }
        }
        self.output.push('>');
    }

    fn write_type_arguments(&mut self, arguments: &[TypeNode]) {
        if arguments.is_empty() {
            return;
        }
        self.output.push('<');
        for (index, argument) in arguments.iter().enumerate() {
            if index != 0 {
                self.output.push_str(", ");
            }
            self.write_type(argument, TYPE_PREC_LOWEST);
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
            TypeNodeKind::Object(members) => {
                self.output.push('{');
                if members.iter().any(Self::type_member_is_emittable) {
                    self.output.push('\n');
                    self.indent += 1;
                    for member in members
                        .iter()
                        .filter(|member| Self::type_member_is_emittable(member))
                    {
                        self.write_indent();
                        self.write_type_member(member);
                        self.output.push('\n');
                    }
                    self.indent = self.indent.saturating_sub(1);
                    self.write_indent();
                }
                self.output.push('}');
            }
            TypeNodeKind::Function {
                type_parameters,
                parameters,
                return_type,
                ..
            } => {
                self.write_type_parameters(type_parameters);
                self.write_declaration_parameters(parameters);
                self.output.push_str(" => ");
                self.write_type(return_type, TYPE_PREC_FUNCTION);
            }
            TypeNodeKind::Constructor {
                type_parameters,
                parameters,
                return_type,
                abstract_constructor,
                ..
            } => {
                if *abstract_constructor {
                    self.output.push_str("abstract ");
                }
                self.output.push_str("new ");
                self.write_type_parameters(type_parameters);
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
            TypeNodeKind::TypeQuery { name, .. } => {
                self.output.push_str("typeof ");
                self.output.push_str(name);
            }
            TypeNodeKind::Infer {
                name, constraint, ..
            } => {
                self.output.push_str("infer ");
                self.output.push_str(name);
                if let Some(constraint) = constraint {
                    self.output.push_str(" extends ");
                    self.write_type(constraint, TYPE_PREC_LOWEST);
                }
            }
            TypeNodeKind::Predicate {
                parameter,
                asserts,
                ty,
                ..
            } => {
                if *asserts {
                    self.output.push_str("asserts ");
                }
                self.output.push_str(parameter);
                if let Some(ty) = ty {
                    self.output.push_str(" is ");
                    self.write_type(ty, TYPE_PREC_LOWEST);
                }
            }
            TypeNodeKind::KeyOf(operand) => {
                self.output.push_str("keyof ");
                self.write_type(operand, TYPE_PREC_PREFIX);
            }
            TypeNodeKind::Readonly(operand) => {
                self.output.push_str("readonly ");
                self.write_type(operand, TYPE_PREC_PREFIX);
            }
            TypeNodeKind::Conditional {
                check_type,
                extends_type,
                true_type,
                false_type,
            } => {
                self.write_type(check_type, TYPE_PREC_FUNCTION);
                self.output.push_str(" extends ");
                self.write_type(extends_type, TYPE_PREC_FUNCTION);
                self.output.push_str(" ? ");
                self.write_type(true_type, TYPE_PREC_LOWEST);
                self.output.push_str(" : ");
                self.write_type(false_type, TYPE_PREC_LOWEST);
            }
            TypeNodeKind::Mapped {
                parameter,
                constraint,
                name_type,
                value_type,
                readonly,
                optional,
                members,
                ..
            } => {
                self.output.push_str("{ ");
                if let Some(readonly) = readonly {
                    self.output
                        .push_str(if *readonly { "readonly " } else { "-readonly " });
                }
                self.output.push('[');
                self.output.push_str(parameter);
                self.output.push_str(" in ");
                self.write_type(constraint, TYPE_PREC_LOWEST);
                if let Some(name_type) = name_type {
                    self.output.push_str(" as ");
                    self.write_type(name_type, TYPE_PREC_LOWEST);
                }
                self.output.push(']');
                if let Some(optional) = optional {
                    self.output.push_str(if *optional { "?" } else { "-?" });
                }
                self.output.push_str(": ");
                self.write_type(value_type, TYPE_PREC_LOWEST);
                self.output.push(';');
                for member in members
                    .iter()
                    .filter(|member| Self::type_member_is_emittable(member))
                {
                    self.output.push(' ');
                    self.write_type_member(member);
                }
                self.output.push_str(" }");
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

    fn write_property_name(&mut self, name: &str, span: Span) {
        let raw = self.source.slice(span).trim();
        if raw_is_property_name(raw) || is_private_identifier(raw) {
            self.output.push_str(raw);
        } else if is_identifier_name(name)
            || is_numeric_property_name(name)
            || is_private_identifier(name)
        {
            self.output.push_str(name);
        } else {
            self.output.push_str(&quote_string(name));
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
            StatementKind::Export(declaration) => {
                !declaration.type_only
                    && (declaration.export_all
                        || declaration.assignment.is_some()
                        || declaration
                            .specifiers
                            .iter()
                            .any(|specifier| !specifier.type_only))
            }
            StatementKind::Class(declaration) => declaration.exported && !declaration.declared,
            StatementKind::Variable(declaration) => declaration.exported,
            StatementKind::Function(declaration) => {
                declaration.exported && !declaration.declared && self.function_has_body(statement)
            }
            StatementKind::Import(_)
            | StatementKind::TypeAlias(_)
            | StatementKind::Interface(_)
            | StatementKind::If(_)
            | StatementKind::Switch(_)
            | StatementKind::Break(_)
            | StatementKind::Continue(_)
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
const PREC_LOGICAL_OR: u8 = 2;
const PREC_LOGICAL_AND: u8 = 3;
const PREC_EQUALITY: u8 = 4;
const PREC_RELATIONAL: u8 = 5;
const PREC_ADDITIVE: u8 = 6;
const PREC_MULTIPLICATIVE: u8 = 7;
const PREC_UNARY: u8 = 8;
const PREC_POSTFIX: u8 = 9;
const PREC_PRIMARY: u8 = 10;

const TYPE_PREC_LOWEST: u8 = 0;
const TYPE_PREC_FUNCTION: u8 = 1;
const TYPE_PREC_UNION: u8 = 2;
const TYPE_PREC_INTERSECTION: u8 = 3;
const TYPE_PREC_PREFIX: u8 = 4;
const TYPE_PREC_POSTFIX: u8 = 5;
const TYPE_PREC_PRIMARY: u8 = 6;

const fn type_precedence(ty: &TypeNode) -> u8 {
    match &ty.kind {
        TypeNodeKind::Function { .. }
        | TypeNodeKind::Constructor { .. }
        | TypeNodeKind::Predicate { .. }
        | TypeNodeKind::Conditional { .. } => TYPE_PREC_FUNCTION,
        TypeNodeKind::Union(_) => TYPE_PREC_UNION,
        TypeNodeKind::Intersection(_) => TYPE_PREC_INTERSECTION,
        TypeNodeKind::KeyOf(_) | TypeNodeKind::Readonly(_) => TYPE_PREC_PREFIX,
        TypeNodeKind::Array(_) | TypeNodeKind::IndexedAccess { .. } => TYPE_PREC_POSTFIX,
        TypeNodeKind::Keyword(_)
        | TypeNodeKind::Literal(_)
        | TypeNodeKind::Tuple(_)
        | TypeNodeKind::Object(_)
        | TypeNodeKind::Reference { .. }
        | TypeNodeKind::TypeQuery { .. }
        | TypeNodeKind::Infer { .. }
        | TypeNodeKind::Mapped { .. }
        | TypeNodeKind::Parenthesized(_)
        | TypeNodeKind::Missing => TYPE_PREC_PRIMARY,
    }
}

const fn statement_is_exported(statement: &Statement) -> bool {
    match &statement.kind {
        StatementKind::Import(_) | StatementKind::Export(_) => true,
        StatementKind::Variable(declaration) => declaration.exported,
        StatementKind::Function(declaration) => declaration.exported,
        StatementKind::Class(declaration) => declaration.exported,
        StatementKind::TypeAlias(declaration) => declaration.exported,
        StatementKind::Interface(declaration) => declaration.exported,
        StatementKind::If(_)
        | StatementKind::Switch(_)
        | StatementKind::Break(_)
        | StatementKind::Continue(_)
        | StatementKind::Return(_)
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
        BinaryOperator::Remainder => "%",
        BinaryOperator::LessThan => "<",
        BinaryOperator::LessThanEquals => "<=",
        BinaryOperator::GreaterThan => ">",
        BinaryOperator::GreaterThanEquals => ">=",
        BinaryOperator::Equals => "==",
        BinaryOperator::NotEquals => "!=",
        BinaryOperator::StrictEquals => "===",
        BinaryOperator::StrictNotEquals => "!==",
        BinaryOperator::LogicalAnd => "&&",
        BinaryOperator::LogicalOr => "||",
        BinaryOperator::NullishCoalesce => "??",
        BinaryOperator::In => "in",
        BinaryOperator::InstanceOf => "instanceof",
    }
}

const fn unary_operator_text(operator: UnaryOperator) -> &'static str {
    match operator {
        UnaryOperator::Plus => "+",
        UnaryOperator::Minus => "-",
        UnaryOperator::Not => "!",
        UnaryOperator::BitwiseNot => "~",
        UnaryOperator::TypeOf => "typeof",
        UnaryOperator::Void => "void",
        UnaryOperator::Delete => "delete",
        UnaryOperator::Await => "await",
    }
}

const fn unary_operator_is_keyword(operator: UnaryOperator) -> bool {
    matches!(
        operator,
        UnaryOperator::TypeOf | UnaryOperator::Void | UnaryOperator::Delete | UnaryOperator::Await
    )
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
        KeywordType::Object => "object",
        KeywordType::Symbol => "symbol",
        KeywordType::UniqueSymbol => "unique symbol",
    }
}

fn raw_is_property_name(raw: &str) -> bool {
    is_quoted(raw) || is_identifier_name(raw) || is_numeric_property_name(raw)
}

fn is_private_identifier(text: &str) -> bool {
    text.strip_prefix('#').is_some_and(is_identifier_name)
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
    fn preserves_es_modules_at_the_ts7_defaults() {
        let file = program_file("value.ts", "export const value: number = 1;\n");
        let output = emit_file(&file, &CompilerOptions::default());
        assert_eq!(output[0].text, "export const value = 1;\n");
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
        let file = program_file("src/value.mts", "export const value: number = 1;\n");
        let options = CompilerOptions {
            declaration: true,
            target: "esnext".to_string(),
            module: "nodenext".to_string(),
            ..CompilerOptions::default()
        };
        let output = emit_file(&file, &options);
        assert_eq!(output[0].path, Path::new("src/value.mjs"));
        assert_eq!(output[1].path, Path::new("src/value.d.mts"));
        assert_eq!(output[0].text, "export const value = 1;\n");
        assert_eq!(output[1].text, "export declare const value: number;\n");
    }

    #[test]
    fn emits_modern_module_classes_from_structured_nodes() {
        let file = program_file(
            "src/service.ts",
            concat!(
                "import { token } from \"./token\";\n",
                "export class Service extends Base {\n",
                "  value: number = token;\n",
                "  constructor(value: number) { this.value = value; }\n",
                "  static create(value: number): Service { return new Service(value); }\n",
                "}\n",
            ),
        );
        let options = CompilerOptions {
            target: "es2022".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        };
        let output = emit_file(&file, &options);
        assert_eq!(
            output[0].text,
            concat!(
                "import { token } from \"./token\";\n",
                "export class Service extends Base {\n",
                "    value = token;\n",
                "    constructor(value) {\n",
                "        this.value = value;\n",
                "    }\n",
                "    static create(value) {\n",
                "        return new Service(value);\n",
                "    }\n",
                "}\n",
            )
        );
    }

    #[test]
    fn erases_class_overload_signatures_but_keeps_empty_implementations() {
        let file = program_file(
            "service.ts",
            concat!(
                "class Service {\n",
                "  constructor(value: string);\n",
                "  constructor() {}\n",
                "  method(value: string): void;\n",
                "  method() {}\n",
                "}\n",
            ),
        );
        let options = CompilerOptions {
            target: "es2022".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        };
        let output = emit_file(&file, &options);
        assert_eq!(
            output[0].text,
            concat!(
                "\"use strict\";\n",
                "class Service {\n",
                "    constructor() { }\n",
                "    method() { }\n",
                "}\n",
            )
        );
    }

    #[test]
    fn rewrites_mixed_esm_clauses_and_assertions_from_structured_nodes() {
        let file = program_file(
            "module.ts",
            concat!(
                "import Default, { type Shape, live as renamed } from \"./dep\";\n",
                "export { type Shape, renamed as exposed } from \"./dep\";\n",
                "const value = [Default, renamed] as unknown;\n",
                "export default (value as unknown);\n",
            ),
        );
        let options = CompilerOptions {
            target: "es2022".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        };
        let output = emit_file(&file, &options);
        assert_eq!(
            output[0].text,
            concat!(
                "import Default, { live as renamed } from \"./dep\";\n",
                "export { renamed as exposed } from \"./dep\";\n",
                "const value = [Default, renamed];\n",
                "export default value;\n",
            )
        );
    }

    #[test]
    fn emits_supported_class_fields_and_preserves_private_names() {
        let file = program_file(
            "model.ts",
            concat!(
                "class Model {\n",
                "  #secret = 1;\n",
                "  visible: number;\n",
                "}\n",
            ),
        );
        let options = CompilerOptions {
            target: "es2022".to_string(),
            module: "esnext".to_string(),
            ..CompilerOptions::default()
        };
        let output = emit_file(&file, &options);
        assert_eq!(
            output[0].text,
            concat!(
                "\"use strict\";\n",
                "class Model {\n",
                "    #secret = 1;\n",
                "    visible;\n",
                "}\n",
            )
        );
    }
}
