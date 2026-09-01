use super::{
    CommentTrivia, RegularExpressionLiteral, SourceCheckDirective, TokenKind, Utf16String,
    descendant_walk::for_each_statement_in,
};
use crate::source::{NodeId, Span};
#[derive(Debug, Clone)]
pub struct SourceUnit {
    pub statements: Vec<Statement>,
    pub span: Span,
    /// Scanner-authored identifier-shaped token spans.
    pub(crate) identifier_token_spans: Vec<Span>,
    pub(crate) authored_literal_facts: Vec<AuthoredLiteralFact>,
    pub(crate) parser_recovery_facts: Vec<ParserRecoveryFact>,
    pub(crate) unmodeled_declaration_hosts: Vec<UnmodeledDeclarationHostFact>,
    pub(crate) source_check_directive: Option<SourceCheckDirective>,
    pub(crate) source_syntax_facts: Vec<SourceSyntaxFact>,
    pub(crate) contextual_grammar_facts: Vec<ContextualGrammarFact>,
    pub(crate) comments: Vec<CommentTrivia>,
    pub(crate) has_unicode_line_comment_terminator: bool,
}
/// Authored contextual grammar whose diagnostics are produced by semantic
/// checking and therefore suppressed by `--noCheck`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ContextualGrammarFact {
    pub(crate) span: Span,
    pub(crate) kind: ContextualGrammarKind,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ContextualGrammarKind {
    AccessorTypeParameters,
    AccessorThisParameter,
    AwaitBinding,
    StrictYieldBinding,
    ClassStrictYieldBinding,
}
/// Positive parser observations that cannot be reconstructed from the current
/// AST. Product capability analysis maps these authored facts to consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SourceSyntaxFact {
    AsyncClassModifier,
    AuthoredFunctionExpressionModifier,
    AuthoredRegularExpression,
    DecoratorRecovery,
    DefaultExportOnUnsupportedHost,
    ExplicitCallTypeArguments,
    ExplicitNewTypeArguments,
    InvalidClassModifierOrder,
    JavaScriptJSDocCast(NodeId, JavaScriptJSDocCastKind),
    LiteralBoundary(AuthoredLiteralKind, LiteralSyntaxBoundary),
    ModuleExport,
    NumericRecoveryEmit(NodeId),
    TemplateExpression,
    TemplateExpressionIdentifier,
    UnsignedRightShiftAssignmentRecovery,
    UnsignedRightShiftOperandRecovery,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum JavaScriptJSDocCastKind {
    Type,
    Satisfies,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LiteralSyntaxBoundary {
    LexicalRecovery,
    SemanticValidation,
    UnsupportedHost,
}
/// Scanner-authored syntax provenance retained for downstream ownership analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AuthoredLiteralFact {
    pub(crate) span: Span,
    /// Parser-owned extent; separate from the token so policy never infers text boundaries.
    pub(crate) recovery_extent: Span,
    pub(crate) kind: AuthoredLiteralKind,
    /// Stable identity for the smallest represented owner and its `SourceUnit` root.
    pub(crate) owner: ParserRecoveryOwner,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AuthoredLiteralKind {
    RegularExpression,
    NumericRecovery,
    NumericSeparator,
}
/// Parser-owned extent for syntax whose recovered AST is not a complete producer.
/// The authored token remains separate so policy never infers parser boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ParserRecoveryFact {
    pub(crate) authored_span: Span,
    pub(crate) recovery_extent: Span,
    pub(crate) kind: ParserRecoveryKind,
    pub(crate) owner: ParserRecoveryOwner,
}
/// Stable syntax owners attached after recovery: the smallest statement and `SourceUnit` root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ParserRecoveryOwner {
    pub(crate) root_statement: NodeId,
    pub(crate) statement: NodeId,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ParserRecoveryKind {
    Declaration,
    GeneratorFunctionLike,
    Expression,
    MissingExpression,
    ObjectMember,
    ForStatement,
    ComputedPropertyName,
    ClassMember,
    ClassExpression,
    AngleAssertion,
    RejectedGenericArrowPrefix,
    ConditionalExpression,
    Type,
    MissingType,
    Template,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnmodeledDeclarationHostFact {
    pub(crate) owner_start: u32,
    pub(crate) recovery_extent: Span,
    pub(crate) name: Option<String>,
    pub(crate) name_span: Option<Span>,
    pub(crate) kind: UnmodeledDeclarationHostKind,
    pub(crate) body: DeclarationHostBodyRepresentation,
    pub(crate) declared: bool,
    pub(crate) exported: bool,
}
/// Whether declaration-host recovery retained the authored body as ordinary
/// statements. Semantic ownership remains a separate program capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeclarationHostBodyRepresentation {
    Omitted,
    ParsedStatements,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnmodeledDeclarationHostKind {
    Enum,
    Namespace,
    Module,
    ExternalModule,
    Global,
    Using,
}
impl SourceUnit {
    #[must_use]
    pub(crate) fn has_source_syntax_fact(&self, fact: SourceSyntaxFact) -> bool {
        self.source_syntax_facts.binary_search(&fact).is_ok()
    }
    pub(crate) fn contextual_grammar_facts(&self) -> &[ContextualGrammarFact] {
        &self.contextual_grammar_facts
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
    /// Whether emit would need function-modifier product ownership that the
    /// syntax printer does not yet provide for every module target.
    #[must_use]
    pub fn has_unmodeled_function_products(&self) -> bool {
        let mut unmodeled = false;
        for_each_statement_in(&self.statements, &mut |statement| {
            unmodeled |= matches!(&statement.kind, StatementKind::Function(function)
                if function.abstract_declaration || !function.overload_completion_supported);
        });
        unmodeled
    }
    pub(crate) fn comments(&self) -> &[CommentTrivia] {
        &self.comments
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
    Variable(VariableStatement),
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
    pub imported_span: Option<Span>,
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
pub struct VariableStatement {
    pub declaration_kind: VariableKind,
    pub declarators: Vec<VariableDeclarator>,
    /// A JSDoc comment occurs in this statement's leading trivia range.
    pub has_leading_jsdoc: bool,
    pub exported: bool,
    pub declared: bool,
}
#[derive(Debug, Clone)]
pub struct VariableDeclarator {
    pub name: String,
    pub name_span: Span,
    pub(crate) recovered_binding_names: Vec<AuthoredBindingName>,
    pub annotation: Option<TypeNode>,
    pub initializer: Option<Expression>,
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
    /// Parser-authored span from the opening through closing body brace.
    pub body_span: Option<Span>,
    /// A JSDoc comment occurs in this declaration's leading trivia range.
    pub has_leading_jsdoc: bool,
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
    /// Authored standalone semicolon class elements, in source order.
    pub empty_elements: Vec<Span>,
    /// Parser-authored span from the opening through closing class-body brace.
    pub body_span: Option<Span>,
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
    pub(crate) const fn constructor_modifiers_are_modeled(&self) -> bool {
        !self.unsupported_for_emit_products
            && !self.readonly
            && !self.static_member
            && !self.abstract_member
            && !self.declared
            && !self.async_member
    }
    pub(crate) const fn method_modifiers_are_modeled(&self) -> bool {
        !self.unsupported_for_emit_products
            && !self.readonly
            && !self.abstract_member
            && !self.declared
    }
    pub(crate) const fn property_modifiers_are_modeled(&self) -> bool {
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
    /// Scanner-cooked property identity for string-literal names. Kept
    /// separate from `name` so emit can preserve authored spelling and lone
    /// UTF-16 surrogate units remain representable.
    pub string_name_value: Option<Utf16String>,
    pub span: Span,
    /// Whether authored JSDoc is attached to the member's leading token.
    pub has_leading_jsdoc: bool,
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
        type_parameters: Vec<TypeParameterDeclaration>,
        parameters: Vec<Parameter>,
        return_type: Option<TypeNode>,
        body: Vec<Statement>,
        has_body: bool,
        body_span: Option<Span>,
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
        body_span: Option<Span>,
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
    pub(crate) fn is_property(&self) -> bool {
        self.modifiers
            .iter()
            .any(|modifier| modifier.kind.is_property())
    }
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
impl ParameterModifier {
    pub(crate) const fn is_property(self) -> bool {
        matches!(
            self,
            Self::Override | Self::Public | Self::Protected | Self::Private | Self::Readonly
        )
    }
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
    StringLiteral(String, Option<Utf16String>),
    NumericLiteral(String),
    BigIntLiteral(String),
    Computed(Expression),
}
impl TypeMemberName {
    /// Canonical spelling for scalar syntax names. Literal keys use the binder's UTF-16
    /// identity; computed expressions never become keys by rendering or source slicing.
    #[must_use]
    pub fn semantic_name(&self) -> Option<&str> {
        match &self.kind {
            TypeMemberNameKind::Identifier(name) => Some(name),
            TypeMemberNameKind::StringLiteral(..)
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
type TypeMemberSignature<'a> = (
    Option<&'a TypeMemberName>,
    &'a [TypeParameterDeclaration],
    &'a [Parameter],
    Option<&'a TypeNode>,
);
impl TypeMemberKind {
    pub(crate) fn signature(&self) -> Option<TypeMemberSignature<'_>> {
        match self {
            Self::Method {
                name,
                type_parameters,
                parameters,
                return_type,
                ..
            } => Some((
                Some(name),
                type_parameters,
                parameters,
                return_type.as_ref(),
            )),
            Self::Accessor {
                name,
                parameters,
                return_type,
                ..
            } => Some((Some(name), &[], parameters, return_type.as_ref())),
            Self::Call {
                type_parameters,
                parameters,
                return_type,
            }
            | Self::Construct {
                type_parameters,
                parameters,
                return_type,
            } => Some((None, type_parameters, parameters, return_type.as_ref())),
            Self::Index {
                parameters,
                value_type,
            } => Some((None, &[], parameters, value_type.as_ref())),
            Self::Property { .. } => None,
        }
    }
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
    This,
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
        walk_authored_item(
            AuthoredTypeItem::Type(self, AuthoredTypeEdge::Nested),
            &mut |item| match item {
                AuthoredTypeItem::Type(node, _)
                    if matches!(node.kind, TypeNodeKind::TypeQuery { .. }) =>
                {
                    TypeWalkControl::Stop
                }
                AuthoredTypeItem::Member(member) if member.recovered => TypeWalkControl::Prune,
                _ => TypeWalkControl::Continue,
            },
        )
    }
    /// Visit the `infer` declarations introduced by one conditional extends
    /// pattern. Nested conditionals own their declarations, so their subtrees
    /// are deliberately pruned. Children otherwise retain authored order.
    pub(crate) fn for_each_conditional_infer<'a>(&'a self, visit: &mut impl FnMut(&'a str, Span)) {
        walk_authored_item(
            AuthoredTypeItem::Type(self, AuthoredTypeEdge::Nested),
            &mut |item| match item {
                AuthoredTypeItem::Type(_, AuthoredTypeEdge::TypeParameterDeclaration) => {
                    TypeWalkControl::Prune
                }
                AuthoredTypeItem::Type(node, _) => match &node.kind {
                    TypeNodeKind::Infer {
                        name, name_span, ..
                    } => {
                        visit(name, *name_span);
                        TypeWalkControl::Prune
                    }
                    TypeNodeKind::Conditional { .. } => TypeWalkControl::Prune,
                    _ => TypeWalkControl::Continue,
                },
                AuthoredTypeItem::Member(_) => TypeWalkControl::Continue,
            },
        );
    }
    /// Tests authored type members while walking every nested type position.
    /// The predicate owns any product or semantic policy; syntax only owns the
    /// immutable traversal of written type structure.
    pub(crate) fn contains_matching_type_member(
        &self,
        predicate: &mut impl FnMut(&TypeMember) -> bool,
    ) -> bool {
        walk_authored_item(
            AuthoredTypeItem::Type(self, AuthoredTypeEdge::Nested),
            &mut |item| match item {
                AuthoredTypeItem::Member(member) if predicate(member) => TypeWalkControl::Stop,
                _ => TypeWalkControl::Continue,
            },
        )
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthoredTypeEdge {
    Nested,
    TypeParameterDeclaration,
    ConditionalTrue,
    MappedConstraint,
}
#[derive(Clone, Copy)]
enum TypeWalkControl {
    Continue,
    Prune,
    Stop,
}
#[derive(Clone, Copy)]
pub(crate) enum AuthoredTypeItem<'a> {
    Type(&'a TypeNode, AuthoredTypeEdge),
    Member(&'a TypeMember),
}
fn walk_authored_item<'a>(
    root: AuthoredTypeItem<'a>,
    visit: &mut impl FnMut(AuthoredTypeItem<'a>) -> TypeWalkControl,
) -> bool {
    let mut pending = vec![root];
    while let Some(item) = pending.pop() {
        match visit(item) {
            TypeWalkControl::Stop => return true,
            TypeWalkControl::Prune => continue,
            TypeWalkControl::Continue => {}
        }
        match item {
            AuthoredTypeItem::Type(node, _) => node.push_authored_children(&mut pending),
            AuthoredTypeItem::Member(member) => member.push_authored_children(&mut pending),
        }
    }
    false
}
impl TypeNode {
    pub(crate) fn push_authored_children<'a>(&'a self, pending: &mut Vec<AuthoredTypeItem<'a>>) {
        let nested = |node| AuthoredTypeItem::Type(node, AuthoredTypeEdge::Nested);
        match &self.kind {
            TypeNodeKind::Array(child)
            | TypeNodeKind::KeyOf(child)
            | TypeNodeKind::Readonly(child)
            | TypeNodeKind::Parenthesized(child) => pending.push(nested(child)),
            TypeNodeKind::Tuple(children)
            | TypeNodeKind::Union(children)
            | TypeNodeKind::Intersection(children) => {
                pending.extend(children.iter().rev().map(nested));
            }
            TypeNodeKind::Object(members) => {
                pending.extend(members.iter().rev().map(AuthoredTypeItem::Member));
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
            } => push_authored_signature_children(
                type_parameters,
                parameters,
                Some(return_type),
                pending,
            ),
            TypeNodeKind::Reference { arguments, .. } => {
                pending.extend(arguments.iter().rev().map(nested));
            }
            TypeNodeKind::Infer { constraint, .. } => {
                pending.extend(constraint.iter().map(|node| nested(node)));
            }
            TypeNodeKind::Predicate { ty, .. } => {
                pending.extend(ty.iter().map(|node| nested(node)));
            }
            TypeNodeKind::Conditional {
                check_type,
                extends_type,
                true_type,
                false_type,
            } => {
                pending.push(nested(false_type));
                pending.push(AuthoredTypeItem::Type(
                    true_type,
                    AuthoredTypeEdge::ConditionalTrue,
                ));
                pending.push(nested(extends_type));
                pending.push(nested(check_type));
            }
            TypeNodeKind::Mapped {
                constraint,
                name_type,
                value_type,
                members,
                ..
            } => {
                pending.extend(members.iter().rev().map(AuthoredTypeItem::Member));
                pending.push(nested(value_type));
                pending.extend(name_type.iter().map(|node| nested(node)));
                pending.push(AuthoredTypeItem::Type(
                    constraint,
                    AuthoredTypeEdge::MappedConstraint,
                ));
            }
            TypeNodeKind::IndexedAccess { object, index } => {
                pending.push(nested(index));
                pending.push(nested(object));
            }
            TypeNodeKind::Keyword(_)
            | TypeNodeKind::Literal(_)
            | TypeNodeKind::This
            | TypeNodeKind::TypeQuery { .. }
            | TypeNodeKind::Missing => {}
        }
    }
}
impl TypeMember {
    pub(crate) fn push_authored_children<'a>(&'a self, pending: &mut Vec<AuthoredTypeItem<'a>>) {
        if let TypeMemberKind::Property { ty, .. } = &self.kind {
            pending.extend(
                ty.iter()
                    .map(|node| AuthoredTypeItem::Type(node, AuthoredTypeEdge::Nested)),
            );
            return;
        }
        if let Some((_, type_parameters, parameters, return_type)) = self.kind.signature() {
            push_authored_signature_children(type_parameters, parameters, return_type, pending);
        }
    }
}
fn push_authored_signature_children<'a>(
    type_parameters: &'a [TypeParameterDeclaration],
    parameters: &'a [Parameter],
    return_type: Option<&'a TypeNode>,
    pending: &mut Vec<AuthoredTypeItem<'a>>,
) {
    pending.extend(
        return_type
            .into_iter()
            .map(|node| AuthoredTypeItem::Type(node, AuthoredTypeEdge::Nested)),
    );
    pending.extend(parameters.iter().rev().filter_map(|parameter| {
        parameter
            .annotation
            .as_ref()
            .map(|node| AuthoredTypeItem::Type(node, AuthoredTypeEdge::Nested))
    }));
    for parameter in type_parameters.iter().rev() {
        pending.extend(
            parameter.default.iter().map(|node| {
                AuthoredTypeItem::Type(node, AuthoredTypeEdge::TypeParameterDeclaration)
            }),
        );
        pending.extend(
            parameter.constraint.iter().map(|node| {
                AuthoredTypeItem::Type(node, AuthoredTypeEdge::TypeParameterDeclaration)
            }),
        );
    }
}
#[derive(Debug, Clone)]
pub struct Expression {
    pub id: NodeId,
    pub span: Span,
    pub kind: ExpressionKind,
}
#[derive(Debug, Clone)]
pub struct TemplateExpression {
    pub head: String,
    pub spans: Vec<TemplateSpan>,
}
#[derive(Debug, Clone)]
pub struct TemplateSpan {
    pub expression: Expression,
    pub literal: String,
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
    Template(TemplateExpression),
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
        type_argument_list_close: Option<Span>,
        arguments: Vec<Expression>,
        has_argument_list: bool,
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
    Conditional {
        condition: Box<Expression>,
        question_span: Span,
        when_true: Box<Expression>,
        colon_span: Option<Span>,
        when_false: Box<Expression>,
    },
    Unary {
        operator: UnaryOperator,
        operand: Box<Expression>,
    },
    Assignment {
        left: Box<Expression>,
        operator: AssignmentOperator,
        operator_span: Span,
        right: Box<Expression>,
        /// A JSDoc comment occurs before the assignment's left edge.
        has_leading_jsdoc: bool,
    },
    As {
        expression: Box<Expression>,
        ty: TypeNode,
    },
    NonNull(Box<Expression>),
    Parenthesized(Box<Expression>),
    Missing,
}
impl Expression {
    pub(crate) fn peel_parentheses(&self) -> &Self {
        let mut expression = self;
        while let ExpressionKind::Parenthesized(inner) = &expression.kind {
            expression = inner;
        }
        expression
    }
    pub(crate) fn peel_parentheses_and_assertions(&self) -> &Self {
        let mut expression = self;
        while let ExpressionKind::Parenthesized(inner)
        | ExpressionKind::NonNull(inner)
        | ExpressionKind::As {
            expression: inner, ..
        } = &expression.kind
        {
            expression = inner;
        }
        expression
    }
}
#[derive(Debug, Clone)]
pub struct FunctionLikeExpression {
    pub type_parameters: Vec<TypeParameterDeclaration>,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<TypeNode>,
    /// Parser-authored brace span for function expressions and block arrows.
    pub body_span: Option<Span>,
    /// A JSDoc comment occurs in this expression's leading trivia range.
    pub has_leading_jsdoc: bool,
    pub syntax: FunctionLikeSyntax,
}
#[derive(Debug, Clone)]
pub enum FunctionLikeSyntax {
    Arrow(ArrowBody),
    Function {
        kind: FunctionLikeFunctionKind,
        name: Option<AuthoredBindingName>,
        body: Vec<Statement>,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionLikeFunctionKind {
    Expression,
    ObjectMethod,
}
#[derive(Clone, Copy)]
pub(crate) enum FunctionLikeBody<'a> {
    Expression(&'a Expression),
    Statements(&'a [Statement]),
}
impl FunctionLikeSyntax {
    pub(crate) fn body(&self) -> FunctionLikeBody<'_> {
        match self {
            Self::Arrow(ArrowBody::Expression(body)) => FunctionLikeBody::Expression(body),
            Self::Arrow(ArrowBody::Block(body)) | Self::Function { body, .. } => {
                FunctionLikeBody::Statements(body)
            }
        }
    }
    pub(crate) fn function(&self) -> Option<(&Option<AuthoredBindingName>, &[Statement])> {
        match self {
            Self::Function { name, body, .. } => Some((name, body)),
            Self::Arrow(_) => None,
        }
    }
    pub(crate) const fn is_object_method(&self) -> bool {
        matches!(
            self,
            Self::Function {
                kind: FunctionLikeFunctionKind::ObjectMethod,
                ..
            }
        )
    }
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
    pub name_kind: PropertyNameKind,
    pub shorthand: bool,
    pub shorthand_equals_span: Option<Span>,
    pub value: Expression,
    pub span: Span,
    pub starts_on_new_line: bool,
    pub trailing_comma: bool,
    pub closing_brace_on_new_line: bool,
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
    Comma,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    LessThan,
    LessThanEquals,
    GreaterThan,
    GreaterThanEquals,
    LeftShift,
    SignedRightShift,
    UnsignedRightShift,
    Equals,
    NotEquals,
    StrictEquals,
    StrictNotEquals,
    LogicalAnd,
    LogicalOr,
    BitwiseAnd,
    BitwiseXor,
    BitwiseOr,
    NullishCoalesce,
    In,
    InstanceOf,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignmentOperator {
    Assign,
    AddAssign,
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
