use crate::source::{NodeId, Span};

use super::{
    CommentTrivia, RegularExpressionLiteral, SourceCheckDirective, TokenKind,
    descendant_walk::{
        ExpressionRoot, ExpressionTraversal, contains_matching_expression, for_each_statement_in,
    },
};

#[derive(Debug, Clone)]
pub struct SourceUnit {
    pub statements: Vec<Statement>,
    pub span: Span,
    pub(crate) authored_literal_facts: Vec<AuthoredLiteralFact>,
    pub(crate) parser_recovery_facts: Vec<ParserRecoveryFact>,
    pub(crate) unmodeled_declaration_hosts: Vec<UnmodeledDeclarationHostFact>,
    pub(crate) source_check_directive: Option<SourceCheckDirective>,
    pub(crate) source_syntax_facts: Vec<SourceSyntaxFact>,
    pub(crate) comments: Vec<CommentTrivia>,
    pub(crate) has_unicode_line_comment_terminator: bool,
}

/// Positive parser observations that cannot be reconstructed from the current
/// AST. Product capability analysis maps these authored facts to consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SourceSyntaxFact {
    AsyncClassModifier,
    AuthoredExtendedUnicodeString,
    AuthoredFunctionExpressionModifier,
    AuthoredRegularExpression,
    DefaultExportOnUnsupportedHost,
    ExplicitCallTypeArguments,
    ExplicitNewTypeArguments,
    InvalidClassModifierOrder,
    LiteralBoundary(AuthoredLiteralKind, LiteralSyntaxBoundary),
    ModuleExport,
    TemplateExpressionIdentifier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LiteralSyntaxBoundary {
    LexicalRecovery,
    SourceContext,
    UnsupportedHost,
}

/// Scanner-authored literal occurrence retained for downstream ownership
/// analysis. This is syntax provenance, not a product-support decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AuthoredLiteralFact {
    pub(crate) span: Span,
    /// Parser-owned syntax extent whose statements may belong to the same
    /// recovered literal. This stays separate from the authored token span so
    /// capability analysis never has to infer recovery boundaries from text.
    pub(crate) recovery_extent: Span,
    pub(crate) kind: AuthoredLiteralKind,
    /// Stable syntax identity for the smallest represented statement that
    /// owns this scanner-authored literal and its SourceUnit-root wrapper.
    pub(crate) owner: ParserRecoveryOwner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AuthoredLiteralKind {
    Template,
    ExtendedUnicodeString,
    RegularExpression,
    NumericRecovery,
    NumericSeparator,
}

/// Parser-owned extent for syntax whose recovered AST is not a complete
/// semantic producer. The authored token remains separate from the dependent
/// recovery extent so capability analysis does not infer parser boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ParserRecoveryFact {
    pub(crate) authored_span: Span,
    pub(crate) recovery_extent: Span,
    pub(crate) kind: ParserRecoveryKind,
    pub(crate) owner: ParserRecoveryOwner,
}

/// Stable syntax owners attached after the parser has built the recovered AST.
/// `statement` is the smallest represented statement containing the authored
/// recovery token; `root_statement` retains its SourceUnit-root attribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ParserRecoveryOwner {
    pub(crate) root_statement: NodeId,
    pub(crate) statement: NodeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ParserRecoveryKind {
    Declaration,
    GeneratorFunctionLike,
    VariableDeclaratorTail,
    Expression,
    ObjectMember,
    ForStatement,
    ComputedPropertyName,
    ClassExpression,
    AngleAssertion,
    RejectedGenericArrowPrefix,
    Type,
    Template,
}

/// Parser-retained identity for a declaration whose body is not represented
/// yet. The binder publishes only this authored name; every semantic demand
/// on the declaration remains nonclaiming until the host grammar is owned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnmodeledDeclarationHostFact {
    pub(crate) owner_start: u32,
    pub(crate) recovery_extent: Span,
    pub(crate) name: Option<String>,
    pub(crate) name_span: Option<Span>,
    pub(crate) kind: UnmodeledDeclarationHostKind,
    pub(crate) exported: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnmodeledDeclarationHostKind {
    Namespace,
    Module,
    ExternalModule,
    Global,
    Using,
}

impl SourceUnit {
    #[must_use]
    pub(crate) fn authored_literal_facts(&self) -> &[AuthoredLiteralFact] {
        &self.authored_literal_facts
    }

    #[must_use]
    pub(crate) fn parser_recovery_facts(&self) -> &[ParserRecoveryFact] {
        &self.parser_recovery_facts
    }

    #[must_use]
    pub(crate) fn unmodeled_declaration_hosts(&self) -> &[UnmodeledDeclarationHostFact] {
        &self.unmodeled_declaration_hosts
    }

    #[must_use]
    pub(crate) const fn source_check_directive(&self) -> Option<SourceCheckDirective> {
        self.source_check_directive
    }

    #[must_use]
    pub(crate) fn has_source_syntax_fact(&self, fact: SourceSyntaxFact) -> bool {
        self.source_syntax_facts.binary_search(&fact).is_ok()
    }

    #[must_use]
    pub(crate) fn has_authored_literal(&self, kind: AuthoredLiteralKind) -> bool {
        self.authored_literal_facts
            .iter()
            .any(|fact| fact.kind == kind)
    }

    /// Whether this file owns a module-local root scope rather than
    /// contributing declarations to the program's global script scope.
    #[must_use]
    pub fn is_external_module(&self) -> bool {
        self.unmodeled_declaration_hosts
            .iter()
            .any(|host| host.exported)
            || self
                .statements
                .iter()
                .any(|statement| match &statement.kind {
                    StatementKind::Import(_) | StatementKind::Export(_) => true,
                    StatementKind::Variable(declaration) => declaration.exported,
                    StatementKind::Function(declaration) => {
                        declaration.exported || declaration.default_export
                    }
                    StatementKind::Class(declaration) => {
                        declaration.exported || declaration.default_export
                    }
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
                })
    }

    /// Whether parser recovery escaped an authored type-member list anywhere
    /// in this file. Emit blocks both products until the enclosing recovery
    /// syntax is represented explicitly.
    #[must_use]
    pub fn contains_recovered_type_members(&self) -> bool {
        let mut written_type_contains_recovery = false;
        for_each_statement_in(&self.statements, &mut |statement| {
            written_type_contains_recovery |= statement_owns_recovered_type_members(statement);
        });
        written_type_contains_recovery
            || contains_matching_expression(
                ExpressionRoot::Statements(&self.statements),
                ExpressionTraversal::All,
                expression_owns_recovered_type_members,
            )
    }

    /// Whether declaration emit needs an overload summary that the syntax
    /// printer does not own yet. Runtime implementations must be omitted from
    /// `.d.ts` output whenever a sibling signature exists.
    #[must_use]
    pub fn has_local_unmodeled_declaration_overloads(&self) -> bool {
        self.statements
            .iter()
            .any(|statement| match &statement.kind {
                StatementKind::Function(declaration) if !declaration.has_body => {
                    self.statements.iter().any(|candidate| {
                        matches!(
                            &candidate.kind,
                            StatementKind::Function(candidate)
                                if candidate.has_body && candidate.name == declaration.name
                        )
                    })
                }
                StatementKind::Class(declaration) => declaration.members.iter().any(|member| {
                    let bodyless = matches!(
                        &member.kind,
                        ClassMemberKind::Constructor {
                            has_body: false,
                            ..
                        } | ClassMemberKind::Method {
                            has_body: false,
                            ..
                        }
                    );
                    bodyless
                        && declaration.members.iter().any(|candidate| {
                            match (&member.kind, &candidate.kind) {
                                (
                                    ClassMemberKind::Constructor { .. },
                                    ClassMemberKind::Constructor { has_body: true, .. },
                                ) => true,
                                (
                                    ClassMemberKind::Method { .. },
                                    ClassMemberKind::Method { has_body: true, .. },
                                ) => candidate.name == member.name,
                                _ => false,
                            }
                        })
                }),
                StatementKind::Import(_)
                | StatementKind::Export(_)
                | StatementKind::Variable(_)
                | StatementKind::Function(_)
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
            })
    }

    /// Whether emit would need function-modifier product ownership that the
    /// syntax printer does not yet provide for every module target.
    #[must_use]
    pub fn has_unmodeled_function_products(&self) -> bool {
        let mut unmodeled = false;
        for_each_statement_in(&self.statements, &mut |statement| {
            unmodeled |= matches!(&statement.kind, StatementKind::Function(function)
                if function.default_export || function.abstract_declaration
                    || !function.overload_completion_supported);
        });
        unmodeled
    }

    #[must_use]
    pub fn has_authored_extended_unicode_string(&self) -> bool {
        self.has_source_syntax_fact(SourceSyntaxFact::AuthoredExtendedUnicodeString)
    }

    pub(crate) fn comments(&self) -> &[CommentTrivia] {
        &self.comments
    }

    #[must_use]
    pub(crate) const fn has_unicode_line_comment_terminator(&self) -> bool {
        self.has_unicode_line_comment_terminator
    }

    /// Whether template syntax outside the exact no-substitution expression
    /// slice would require an AST, semantic, or emit product TSZ does not own.
    #[must_use]
    pub fn has_unmodeled_template_products(&self) -> bool {
        self.has_literal_syntax_boundary(AuthoredLiteralKind::Template)
    }

    #[must_use]
    pub fn has_unmodeled_extended_unicode_string_products(&self) -> bool {
        self.has_literal_syntax_boundary(AuthoredLiteralKind::ExtendedUnicodeString)
    }

    #[must_use]
    pub(crate) fn has_authored_regular_expression(&self) -> bool {
        self.has_source_syntax_fact(SourceSyntaxFact::AuthoredRegularExpression)
    }

    #[must_use]
    pub(crate) fn has_unmodeled_regular_expression_products(&self) -> bool {
        self.has_literal_syntax_boundary(AuthoredLiteralKind::RegularExpression)
    }

    #[must_use]
    pub(crate) fn has_unmodeled_numeric_recovery_products(&self) -> bool {
        self.has_literal_syntax_boundary(AuthoredLiteralKind::NumericRecovery)
    }

    #[must_use]
    pub(crate) fn has_unmodeled_numeric_separator_products(&self) -> bool {
        self.has_literal_syntax_boundary(AuthoredLiteralKind::NumericSeparator)
    }

    fn has_literal_syntax_boundary(&self, family: AuthoredLiteralKind) -> bool {
        self.source_syntax_facts.iter().any(|fact| {
            matches!(fact, SourceSyntaxFact::LiteralBoundary(candidate, _) if *candidate == family)
        })
    }
}

#[derive(Debug, Clone)]
pub struct Statement {
    pub id: NodeId,
    pub span: Span,
    pub kind: StatementKind,
}

#[derive(Debug, Clone)]
pub enum StatementKind {
    Import(ImportDeclaration),
    Export(ExportDeclaration),
    Variable(VariableDeclaration),
    Function(FunctionDeclaration),
    Class(ClassDeclaration),
    TypeAlias(TypeAliasDeclaration),
    Interface(InterfaceDeclaration),
    If(IfStatement),
    Switch(SwitchStatement),
    Break(JumpStatement),
    Continue(JumpStatement),
    Return(Option<Expression>),
    Block(Vec<Statement>),
    Expression(Expression),
    Empty,
    Unknown,
}

fn statement_owns_recovered_type_members(statement: &Statement) -> bool {
    match &statement.kind {
        StatementKind::Variable(declaration) => declaration
            .annotation
            .as_ref()
            .is_some_and(TypeNode::contains_recovered_type_members),
        StatementKind::Function(declaration) => signature_contains_recovered_type_members(
            &declaration.type_parameters,
            &declaration.parameters,
            declaration.return_type.as_ref(),
        ),
        StatementKind::Class(declaration) => {
            type_parameters_contain_recovery(&declaration.type_parameters)
                || declaration
                    .extends
                    .as_ref()
                    .is_some_and(TypeNode::contains_recovered_type_members)
                || declaration
                    .implements
                    .iter()
                    .any(TypeNode::contains_recovered_type_members)
                || declaration
                    .members
                    .iter()
                    .any(class_member_owns_recovered_type_members)
        }
        StatementKind::TypeAlias(declaration) => {
            type_parameters_contain_recovery(&declaration.type_parameters)
                || declaration.ty.contains_recovered_type_members()
        }
        StatementKind::Interface(declaration) => {
            type_parameters_contain_recovery(&declaration.type_parameters)
                || declaration
                    .extends
                    .iter()
                    .any(TypeNode::contains_recovered_type_members)
                || declaration
                    .members
                    .iter()
                    .any(|member| member.contains(TypeContainment::RecoveredTypeMembers))
        }
        StatementKind::Import(_)
        | StatementKind::Export(_)
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

fn class_member_owns_recovered_type_members(member: &ClassMember) -> bool {
    match &member.kind {
        ClassMemberKind::Constructor { parameters, .. } => parameters_contain_recovery(parameters),
        ClassMemberKind::Property { annotation, .. } => annotation
            .as_ref()
            .is_some_and(TypeNode::contains_recovered_type_members),
        ClassMemberKind::Method {
            type_parameters,
            parameters,
            return_type,
            ..
        } => signature_contains_recovered_type_members(
            type_parameters,
            parameters,
            return_type.as_ref(),
        ),
    }
}

fn signature_contains_recovered_type_members(
    type_parameters: &[TypeParameterDeclaration],
    parameters: &[Parameter],
    return_type: Option<&TypeNode>,
) -> bool {
    type_parameters_contain_recovery(type_parameters)
        || parameters_contain_recovery(parameters)
        || return_type.is_some_and(TypeNode::contains_recovered_type_members)
}

#[derive(Debug, Clone)]
pub struct IfStatement {
    pub condition: Expression,
    pub then_statement: Box<Statement>,
    pub else_statement: Option<Box<Statement>>,
}

#[derive(Debug, Clone)]
pub struct SwitchStatement {
    pub expression: Expression,
    pub clauses: Vec<SwitchClause>,
    pub recovered_discriminant: bool,
}

#[derive(Debug, Clone)]
pub struct SwitchClause {
    pub span: Span,
    pub kind: SwitchClauseKind,
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub enum SwitchClauseKind {
    Case(Expression),
    Default,
}

#[derive(Debug, Clone)]
pub struct JumpStatement {
    pub label: Option<String>,
    pub label_span: Option<Span>,
}

#[derive(Debug, Clone)]
pub struct ImportDeclaration {
    pub bindings: Vec<ImportBinding>,
    pub module_specifier: String,
    pub module_span: Span,
    pub type_only: bool,
    pub side_effect_only: bool,
}

#[derive(Debug, Clone)]
pub struct ImportBinding {
    pub imported: Option<String>,
    pub local: String,
    pub local_span: Span,
    pub type_only: bool,
    pub namespace: bool,
}

#[derive(Debug, Clone)]
pub struct ExportDeclaration {
    pub specifiers: Vec<ExportSpecifier>,
    pub module_specifier: Option<String>,
    pub module_span: Option<Span>,
    pub type_only: bool,
    pub export_all: bool,
    pub default_export: bool,
    pub assignment: Option<Expression>,
}

#[derive(Debug, Clone)]
pub struct ExportSpecifier {
    pub local: String,
    pub local_span: Span,
    pub exported: String,
    pub exported_span: Span,
    pub type_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableKind {
    Let,
    Const,
    Var,
}

#[derive(Debug, Clone)]
pub struct VariableDeclaration {
    pub declaration_kind: VariableKind,
    pub name: String,
    pub name_span: Span,
    pub(crate) recovered_binding_names: Vec<AuthoredBindingName>,
    pub annotation: Option<TypeNode>,
    pub initializer: Option<Expression>,
    pub exported: bool,
}

/// An authored binding identity retained independently from its declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoredBindingName {
    pub name: String,
    pub span: Span,
    pub token_kind: TokenKind,
}

#[derive(Debug, Clone)]
pub struct FunctionDeclaration {
    pub name: String,
    pub name_span: Span,
    pub type_parameters: Vec<TypeParameterDeclaration>,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<TypeNode>,
    pub body: Vec<Statement>,
    pub has_body: bool,
    pub exported: bool,
    pub default_export: bool,
    pub is_async: bool,
    pub declared: bool,
    pub abstract_declaration: bool,
    pub overload_completion_supported: bool,
}

impl FunctionDeclaration {
    pub(crate) const fn overload_context_is_recovery_free(&self) -> bool {
        self.overload_completion_supported
    }

    pub(crate) const fn bodyless_overload_is_recovery_free(&self) -> bool {
        !self.has_body && self.overload_context_is_recovery_free()
    }
}

#[derive(Debug, Clone)]
pub struct ClassDeclaration {
    pub name: String,
    pub name_span: Span,
    pub type_parameters: Vec<TypeParameterDeclaration>,
    pub extends: Option<TypeNode>,
    pub implements: Vec<TypeNode>,
    pub members: Vec<ClassMember>,
    pub exported: bool,
    pub default_export: bool,
    pub declared: bool,
    pub abstract_class: bool,
    pub member_syntax_recovery_free: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ClassMemberModifiers {
    pub public: bool,
    pub protected: bool,
    pub private: bool,
    pub readonly: bool,
    pub static_member: bool,
    pub abstract_member: bool,
    pub declared: bool,
    pub async_member: bool,
    pub unsupported_for_overload_completion: bool,
    pub unsupported_for_emit_products: bool,
}

impl ClassMemberModifiers {
    pub(crate) const fn observe(&mut self, modifier: ParameterModifier) {
        let slot = match modifier {
            ParameterModifier::Public => &mut self.public,
            ParameterModifier::Protected => &mut self.protected,
            ParameterModifier::Private => &mut self.private,
            ParameterModifier::Readonly => &mut self.readonly,
            ParameterModifier::Static => &mut self.static_member,
            ParameterModifier::Abstract => &mut self.abstract_member,
            ParameterModifier::Declare => &mut self.declared,
            ParameterModifier::Async => &mut self.async_member,
            ParameterModifier::Override | ParameterModifier::Accessor => {
                self.unsupported_for_overload_completion = true;
                self.unsupported_for_emit_products = true;
                return;
            }
            ParameterModifier::Const
            | ParameterModifier::Default
            | ParameterModifier::Export
            | ParameterModifier::In
            | ParameterModifier::Out => return,
        };
        if *slot {
            self.unsupported_for_overload_completion = true;
            self.unsupported_for_emit_products = true;
        }
        *slot = true;
    }

    pub(crate) const fn constructor_products_supported(&self) -> bool {
        !self.unsupported_for_emit_products
            && !self.readonly
            && !self.static_member
            && !self.abstract_member
            && !self.declared
            && !self.async_member
    }

    pub(crate) const fn method_products_supported(&self) -> bool {
        !self.unsupported_for_emit_products
            && !self.readonly
            && !self.abstract_member
            && !self.declared
    }

    pub(crate) const fn property_products_supported(&self) -> bool {
        !self.unsupported_for_emit_products
            && !self.abstract_member
            && !self.declared
            && !self.async_member
    }
}

#[derive(Debug, Clone)]
pub struct ClassMember {
    pub id: NodeId,
    pub name: String,
    pub name_span: Span,
    pub name_kind: PropertyNameKind,
    pub span: Span,
    pub modifiers: ClassMemberModifiers,
    pub overload_completion_supported: bool,
    pub emit_products_supported: bool,
    pub kind: ClassMemberKind,
}

impl ClassMember {
    pub(crate) const fn overload_context_is_recovery_free(&self) -> bool {
        self.overload_completion_supported
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyNameKind {
    Identifier,
    PrivateIdentifier,
    StringLiteral,
    NumericLiteral,
    Computed,
    Unsupported,
}

#[derive(Debug, Clone)]
pub enum ClassMemberKind {
    Constructor {
        parameters: Vec<Parameter>,
        body: Vec<Statement>,
        has_body: bool,
    },
    Property {
        annotation: Option<TypeNode>,
        initializer: Option<Expression>,
        optional: bool,
        definite: bool,
    },
    Method {
        type_parameters: Vec<TypeParameterDeclaration>,
        parameters: Vec<Parameter>,
        return_type: Option<TypeNode>,
        body: Vec<Statement>,
        has_body: bool,
        accessor: Option<AccessorKind>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessorKind {
    Get,
    Set,
}

#[derive(Debug, Clone)]
pub struct TypeAliasDeclaration {
    pub name: String,
    pub name_span: Span,
    pub type_parameters: Vec<TypeParameterDeclaration>,
    pub ty: TypeNode,
    pub exported: bool,
}

#[derive(Debug, Clone)]
pub struct InterfaceDeclaration {
    pub name: String,
    pub name_span: Span,
    pub type_parameters: Vec<TypeParameterDeclaration>,
    pub extends: Vec<TypeNode>,
    pub members: Vec<TypeMember>,
    pub exported: bool,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub name_span: Span,
    pub(crate) recovered_binding_names: Vec<AuthoredBindingName>,
    pub name_kind: ParameterNameKind,
    pub annotation: Option<TypeNode>,
    pub initializer: Option<Expression>,
    pub optional: bool,
    pub optional_span: Option<Span>,
    pub rest: bool,
    pub rest_span: Option<Span>,
    pub modifiers: Vec<ParameterModifierNode>,
    pub overload_completion_supported: bool,
    pub function_implementation_completion_supported: bool,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterNameKind {
    Binding,
    BindingPattern,
    This,
}

impl Parameter {
    pub(crate) const fn overload_context_is_recovery_free(&self) -> bool {
        self.overload_completion_supported
    }

    pub(crate) const fn implementation_name_is_recovery_free(&self) -> bool {
        self.function_implementation_completion_supported
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParameterModifierNode {
    pub kind: ParameterModifier,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterModifier {
    Abstract,
    Accessor,
    Async,
    Const,
    Declare,
    Default,
    Export,
    In,
    Out,
    Override,
    Public,
    Protected,
    Private,
    Readonly,
    Static,
}

#[derive(Debug, Clone)]
pub struct TypeParameterDeclaration {
    pub name: String,
    pub name_span: Span,
    pub span: Span,
    pub constraint: Option<TypeNode>,
    pub default: Option<TypeNode>,
    pub const_parameter: bool,
    pub in_variance: bool,
    pub out_variance: bool,
}

#[derive(Debug, Clone)]
pub struct TypeMember {
    pub id: NodeId,
    pub span: Span,
    pub recovered: bool,
    pub recovery_incomplete: bool,
    pub modifiers: TypeMemberModifiers,
    pub kind: TypeMemberKind,
}

#[derive(Debug, Clone)]
pub struct TypeMemberName {
    pub span: Span,
    pub kind: TypeMemberNameKind,
}

#[derive(Debug, Clone)]
pub enum TypeMemberNameKind {
    Identifier(String),
    StringLiteral(String),
    NumericLiteral(String),
    BigIntLiteral(String),
    Computed(Expression),
}

impl TypeMemberName {
    /// Canonical named-member spelling when syntax already supplies one.
    ///
    /// String and numeric literals need scanner-cooked/canonical property-key
    /// values before they can safely participate in semantic identity. Until
    /// that boundary exists, they remain typed syntax and force the object
    /// shape query to defer. Computed expressions likewise never become a key
    /// by rendering or slicing their source.
    #[must_use]
    pub fn semantic_name(&self) -> Option<&str> {
        match &self.kind {
            TypeMemberNameKind::Identifier(name) => Some(name),
            TypeMemberNameKind::StringLiteral(_)
            | TypeMemberNameKind::NumericLiteral(_)
            | TypeMemberNameKind::BigIntLiteral(_)
            | TypeMemberNameKind::Computed(_) => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TypeMemberModifiers {
    pub nodes: Vec<TypeMemberModifierNode>,
    pub public: bool,
    pub protected: bool,
    pub private: bool,
    pub readonly: bool,
    pub static_member: bool,
    pub abstract_member: bool,
    pub declared: bool,
    pub accessor: bool,
    pub async_member: bool,
    pub const_member: bool,
    pub default_member: bool,
    pub exported: bool,
    pub in_variance: bool,
    pub out_variance: bool,
    pub override_member: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeMemberModifierNode {
    pub kind: TypeMemberModifier,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeMemberModifier {
    Public,
    Protected,
    Private,
    Readonly,
    Static,
    Abstract,
    Declare,
    Accessor,
    Async,
    Const,
    Default,
    Export,
    In,
    Out,
    Override,
}

#[derive(Debug, Clone)]
pub enum TypeMemberKind {
    Property {
        name: TypeMemberName,
        ty: Option<TypeNode>,
        optional: bool,
        initializer: Option<Expression>,
    },
    Method {
        name: TypeMemberName,
        optional: bool,
        type_parameters: Vec<TypeParameterDeclaration>,
        parameters: Vec<Parameter>,
        return_type: Option<TypeNode>,
    },
    Accessor {
        name: TypeMemberName,
        accessor: AccessorKind,
        parameters: Vec<Parameter>,
        return_type: Option<TypeNode>,
    },
    Call {
        type_parameters: Vec<TypeParameterDeclaration>,
        parameters: Vec<Parameter>,
        return_type: Option<TypeNode>,
    },
    Construct {
        type_parameters: Vec<TypeParameterDeclaration>,
        parameters: Vec<Parameter>,
        return_type: Option<TypeNode>,
    },
    Index {
        parameters: Vec<Parameter>,
        value_type: Option<TypeNode>,
    },
}

#[derive(Debug, Clone)]
pub struct TypeNode {
    pub span: Span,
    pub kind: TypeNodeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeywordType {
    Any,
    Unknown,
    Never,
    Void,
    Undefined,
    Null,
    Boolean,
    Number,
    String,
    BigInt,
    Object,
    Symbol,
    UniqueSymbol,
}

#[derive(Debug, Clone)]
pub enum TypeNodeKind {
    Keyword(KeywordType),
    Literal(Literal),
    Array(Box<TypeNode>),
    Tuple(Vec<TypeNode>),
    Union(Vec<TypeNode>),
    Intersection(Vec<TypeNode>),
    Object(Vec<TypeMember>),
    Function {
        id: NodeId,
        type_parameters: Vec<TypeParameterDeclaration>,
        parameters: Vec<Parameter>,
        parameter_list_recovered: bool,
        return_type: Box<TypeNode>,
    },
    Constructor {
        id: NodeId,
        type_parameters: Vec<TypeParameterDeclaration>,
        parameters: Vec<Parameter>,
        parameter_list_recovered: bool,
        return_type: Box<TypeNode>,
        abstract_constructor: bool,
    },
    Reference {
        name: String,
        name_span: Span,
        arguments: Vec<TypeNode>,
    },
    TypeQuery {
        name: String,
        name_span: Span,
        segment_spans: Vec<Span>,
    },
    Infer {
        name: String,
        name_span: Span,
        constraint: Option<Box<TypeNode>>,
    },
    Predicate {
        parameter: String,
        parameter_span: Span,
        asserts: bool,
        ty: Option<Box<TypeNode>>,
    },
    KeyOf(Box<TypeNode>),
    Readonly(Box<TypeNode>),
    Conditional {
        check_type: Box<TypeNode>,
        extends_type: Box<TypeNode>,
        true_type: Box<TypeNode>,
        false_type: Box<TypeNode>,
    },
    Mapped {
        parameter: String,
        parameter_span: Span,
        constraint: Box<TypeNode>,
        name_type: Option<Box<TypeNode>>,
        value_type: Box<TypeNode>,
        readonly: Option<bool>,
        optional: Option<bool>,
        members: Vec<TypeMember>,
    },
    IndexedAccess {
        object: Box<TypeNode>,
        index: Box<TypeNode>,
    },
    Parenthesized(Box<TypeNode>),
    Missing,
}

impl TypeNode {
    pub(super) fn blocks_arrow_parse(&self) -> bool {
        match &self.kind {
            TypeNodeKind::Missing => true,
            TypeNodeKind::Function {
                parameter_list_recovered,
                return_type,
                ..
            }
            | TypeNodeKind::Constructor {
                parameter_list_recovered,
                return_type,
                ..
            } => *parameter_list_recovered || return_type.blocks_arrow_parse(),
            TypeNodeKind::Parenthesized(return_type) => return_type.blocks_arrow_parse(),
            _ => false,
        }
    }

    /// Whether this written type contains a value-space `typeof` query.
    /// Function implementations need a symbol-kind-aware lookup filter for
    /// these positions; callers that do not own that filter fail closed.
    #[must_use]
    pub fn contains_type_query(&self) -> bool {
        self.contains(TypeContainment::TypeQuery)
    }

    /// Whether declaration/runtime recovery for this type escaped an authored
    /// `TypeElement` list. Emitters use this to block a host product until the
    /// enclosing declarator/parameter recovery is represented structurally.
    #[must_use]
    pub fn contains_recovered_type_members(&self) -> bool {
        self.contains(TypeContainment::RecoveredTypeMembers)
    }

    fn contains(&self, containment: TypeContainment) -> bool {
        match &self.kind {
            TypeNodeKind::TypeQuery { .. } => containment == TypeContainment::TypeQuery,
            TypeNodeKind::Array(inner)
            | TypeNodeKind::KeyOf(inner)
            | TypeNodeKind::Readonly(inner)
            | TypeNodeKind::Parenthesized(inner) => inner.contains(containment),
            TypeNodeKind::Tuple(types)
            | TypeNodeKind::Union(types)
            | TypeNodeKind::Intersection(types) => {
                types.iter().any(|node| node.contains(containment))
            }
            TypeNodeKind::Object(members) => {
                members.iter().any(|member| member.contains(containment))
            }
            TypeNodeKind::Function {
                type_parameters,
                parameters,
                return_type,
                ..
            }
            | TypeNodeKind::Constructor {
                type_parameters,
                parameters,
                return_type,
                ..
            } => {
                type_parameters
                    .iter()
                    .any(|parameter| containment.contains_type_parameter(parameter))
                    || parameters
                        .iter()
                        .any(|parameter| containment.contains_parameter(parameter))
                    || return_type.contains(containment)
            }
            TypeNodeKind::Reference { arguments, .. } => {
                arguments.iter().any(|node| node.contains(containment))
            }
            TypeNodeKind::Infer { constraint, .. } => constraint
                .as_deref()
                .is_some_and(|node| node.contains(containment)),
            TypeNodeKind::Predicate { ty, .. } => {
                ty.as_deref().is_some_and(|node| node.contains(containment))
            }
            TypeNodeKind::Conditional {
                check_type,
                extends_type,
                true_type,
                false_type,
            } => {
                check_type.contains(containment)
                    || extends_type.contains(containment)
                    || true_type.contains(containment)
                    || false_type.contains(containment)
            }
            TypeNodeKind::Mapped {
                constraint,
                name_type,
                value_type,
                members,
                ..
            } => {
                constraint.contains(containment)
                    || name_type
                        .as_deref()
                        .is_some_and(|node| node.contains(containment))
                    || value_type.contains(containment)
                    || members.iter().any(|member| member.contains(containment))
            }
            TypeNodeKind::IndexedAccess { object, index } => {
                object.contains(containment) || index.contains(containment)
            }
            TypeNodeKind::Keyword(_) | TypeNodeKind::Literal(_) | TypeNodeKind::Missing => false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TypeContainment {
    TypeQuery,
    RecoveredTypeMembers,
}

impl TypeContainment {
    fn contains_type_parameter(self, parameter: &TypeParameterDeclaration) -> bool {
        parameter
            .constraint
            .as_ref()
            .is_some_and(|node| node.contains(self))
            || parameter
                .default
                .as_ref()
                .is_some_and(|node| node.contains(self))
    }

    fn contains_parameter(self, parameter: &Parameter) -> bool {
        parameter
            .annotation
            .as_ref()
            .is_some_and(|node| node.contains(self))
            || self == Self::RecoveredTypeMembers
                && parameter
                    .initializer
                    .as_ref()
                    .is_some_and(expression_contains_recovered_type_members)
    }
}

impl TypeMember {
    fn contains(&self, containment: TypeContainment) -> bool {
        if self.recovered {
            return containment == TypeContainment::RecoveredTypeMembers;
        }
        if containment == TypeContainment::RecoveredTypeMembers
            && matches!(
                &self.kind,
                TypeMemberKind::Property { name, .. }
                    | TypeMemberKind::Method { name, .. }
                    | TypeMemberKind::Accessor { name, .. }
                    if matches!(
                        &name.kind,
                        TypeMemberNameKind::Computed(expression)
                            if expression_contains_recovered_type_members(expression)
                    )
            )
        {
            return true;
        }
        match &self.kind {
            TypeMemberKind::Property {
                ty, initializer, ..
            } => {
                ty.as_ref().is_some_and(|node| node.contains(containment))
                    || containment == TypeContainment::RecoveredTypeMembers
                        && initializer
                            .as_ref()
                            .is_some_and(expression_contains_recovered_type_members)
            }
            TypeMemberKind::Method {
                type_parameters,
                parameters,
                return_type,
                ..
            }
            | TypeMemberKind::Call {
                type_parameters,
                parameters,
                return_type,
            }
            | TypeMemberKind::Construct {
                type_parameters,
                parameters,
                return_type,
            } => {
                type_parameters
                    .iter()
                    .any(|parameter| containment.contains_type_parameter(parameter))
                    || parameters
                        .iter()
                        .any(|parameter| containment.contains_parameter(parameter))
                    || return_type
                        .as_ref()
                        .is_some_and(|node| node.contains(containment))
            }
            TypeMemberKind::Accessor {
                parameters,
                return_type,
                ..
            } => {
                parameters
                    .iter()
                    .any(|parameter| containment.contains_parameter(parameter))
                    || return_type
                        .as_ref()
                        .is_some_and(|node| node.contains(containment))
            }
            TypeMemberKind::Index {
                parameters,
                value_type,
            } => {
                parameters
                    .iter()
                    .any(|parameter| containment.contains_parameter(parameter))
                    || value_type
                        .as_ref()
                        .is_some_and(|node| node.contains(containment))
            }
        }
    }
}

fn parameters_contain_recovery(parameters: &[Parameter]) -> bool {
    parameters
        .iter()
        .any(|parameter| TypeContainment::RecoveredTypeMembers.contains_parameter(parameter))
}

fn type_parameters_contain_recovery(parameters: &[TypeParameterDeclaration]) -> bool {
    parameters
        .iter()
        .any(|parameter| TypeContainment::RecoveredTypeMembers.contains_type_parameter(parameter))
}

#[derive(Debug, Clone)]
pub struct Expression {
    pub id: NodeId,
    pub span: Span,
    pub kind: ExpressionKind,
}

#[derive(Debug, Clone)]
pub enum ExpressionKind {
    Identifier {
        name: String,
        name_span: Span,
        entity_name: bool,
    },
    This,
    Literal(Literal),
    RegularExpression(RegularExpressionLiteral),
    Object(Vec<ObjectProperty>),
    Array(Vec<Expression>),
    Call {
        callee: Box<Expression>,
        type_arguments: Option<Vec<TypeNode>>,
        arguments: Vec<Expression>,
    },
    New {
        callee: Box<Expression>,
        type_arguments: Vec<TypeNode>,
        arguments: Vec<Expression>,
    },
    Member {
        object: Box<Expression>,
        name: String,
        name_span: Span,
    },
    ElementAccess {
        object: Box<Expression>,
        index: Box<Expression>,
    },
    FunctionLike(Box<FunctionLikeExpression>),
    Binary {
        left: Box<Expression>,
        operator: BinaryOperator,
        operator_span: Span,
        right: Box<Expression>,
    },
    Unary {
        operator: UnaryOperator,
        operand: Box<Expression>,
    },
    Assignment {
        left: Box<Expression>,
        right: Box<Expression>,
    },
    As {
        expression: Box<Expression>,
        ty: TypeNode,
    },
    Parenthesized(Box<Expression>),
    Missing,
}

fn expression_contains_recovered_type_members(expression: &Expression) -> bool {
    contains_matching_expression(
        ExpressionRoot::Expression(expression),
        ExpressionTraversal::All,
        expression_owns_recovered_type_members,
    )
}

fn expression_owns_recovered_type_members(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::Call { type_arguments, .. } => type_arguments
            .iter()
            .flatten()
            .any(TypeNode::contains_recovered_type_members),
        ExpressionKind::New { type_arguments, .. } => type_arguments
            .iter()
            .any(TypeNode::contains_recovered_type_members),
        ExpressionKind::FunctionLike(function) => signature_contains_recovered_type_members(
            &function.type_parameters,
            &function.parameters,
            function.return_type.as_ref(),
        ),
        ExpressionKind::As { ty, .. } => ty.contains_recovered_type_members(),
        ExpressionKind::Identifier { .. }
        | ExpressionKind::This
        | ExpressionKind::Literal(_)
        | ExpressionKind::RegularExpression(_)
        | ExpressionKind::Object(_)
        | ExpressionKind::Array(_)
        | ExpressionKind::Member { .. }
        | ExpressionKind::ElementAccess { .. }
        | ExpressionKind::Binary { .. }
        | ExpressionKind::Unary { .. }
        | ExpressionKind::Assignment { .. }
        | ExpressionKind::Parenthesized(_)
        | ExpressionKind::Missing => false,
    }
}

#[derive(Debug, Clone)]
pub struct FunctionLikeExpression {
    pub type_parameters: Vec<TypeParameterDeclaration>,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<TypeNode>,
    pub syntax: FunctionLikeSyntax,
}

#[derive(Debug, Clone)]
pub enum FunctionLikeSyntax {
    Arrow(ArrowBody),
    Function {
        name: Option<AuthoredBindingName>,
        body: Vec<Statement>,
        body_span: Span,
    },
}

#[derive(Debug, Clone)]
pub enum ArrowBody {
    Expression(Box<Expression>),
    Block(Vec<Statement>),
}

#[derive(Debug, Clone)]
pub struct ObjectProperty {
    pub name: String,
    pub name_span: Span,
    pub shorthand: bool,
    pub shorthand_equals_span: Option<Span>,
    pub value: Expression,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    String(super::StringLiteral),
    NoSubstitutionTemplate(super::NoSubstitutionTemplateLiteral),
    Number(super::NumberLiteral),
    BigInt(String),
    Boolean(bool),
    Null,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    LessThan,
    LessThanEquals,
    GreaterThan,
    GreaterThanEquals,
    Equals,
    NotEquals,
    StrictEquals,
    StrictNotEquals,
    LogicalAnd,
    LogicalOr,
    BitwiseAnd,
    BitwiseOr,
    NullishCoalesce,
    In,
    InstanceOf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOperator {
    Plus,
    Minus,
    Not,
    BitwiseNot,
    TypeOf,
    Void,
    Delete,
    Await,
}
