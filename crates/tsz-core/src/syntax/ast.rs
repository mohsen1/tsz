use crate::source::{NodeId, Span};

use super::{CommentTrivia, RegularExpressionLiteral};

#[derive(Debug, Clone)]
pub struct SourceUnit {
    pub statements: Vec<Statement>,
    pub span: Span,
    pub(crate) function_products_supported: bool,
    pub(crate) class_products_supported: bool,
    pub(crate) declaration_products_supported: bool,
    pub(crate) commonjs_class_products_supported: bool,
    pub(crate) declaration_hosts_supported: bool,
    pub(crate) default_export_hosts_supported: bool,
    pub(crate) expression_products_supported: bool,
    pub(crate) comments: Vec<CommentTrivia>,
    pub(crate) has_unicode_line_comment_terminator: bool,
    pub(crate) has_authored_no_substitution_template: bool,
    pub(crate) template_products_supported: bool,
    pub(crate) has_authored_extended_unicode_string: bool,
    pub(crate) extended_unicode_string_products_supported: bool,
    pub(crate) has_authored_regular_expression: bool,
    pub(crate) regular_expression_products_supported: bool,
    pub(crate) has_authored_numeric_recovery: bool,
    pub(crate) numeric_recovery_products_supported: bool,
}

impl SourceUnit {
    /// Whether this file owns a module-local root scope rather than
    /// contributing declarations to the program's global script scope.
    #[must_use]
    pub fn is_external_module(&self) -> bool {
        self.statements
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
        self.statements
            .iter()
            .any(Statement::contains_recovered_type_members)
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
    pub const fn has_unmodeled_function_products(&self) -> bool {
        !self.function_products_supported
    }

    /// Whether class emit needs syntax ownership that the printers do not
    /// currently have in every checked and `noCheck` product mode.
    #[must_use]
    pub const fn has_unmodeled_class_products(&self) -> bool {
        !self.class_products_supported
    }

    #[must_use]
    pub const fn has_unmodeled_declaration_products(&self) -> bool {
        !self.declaration_products_supported
    }

    #[must_use]
    pub const fn has_unmodeled_commonjs_class_products(&self) -> bool {
        !self.commonjs_class_products_supported
    }

    /// Whether every authored declaration host has a semantic owner. Module,
    /// namespace, and ambient-global bodies remain opaque until their scopes
    /// and declaration rules are represented in the AST and binder.
    #[must_use]
    pub const fn has_unmodeled_declaration_hosts(&self) -> bool {
        !self.declaration_hosts_supported
    }

    #[must_use]
    pub const fn has_unmodeled_default_export_hosts(&self) -> bool {
        !self.default_export_hosts_supported
    }

    #[must_use]
    pub const fn has_unmodeled_expression_products(&self) -> bool {
        !self.expression_products_supported
    }

    #[must_use]
    pub const fn has_authored_no_substitution_template(&self) -> bool {
        self.has_authored_no_substitution_template
    }

    #[must_use]
    pub const fn has_authored_extended_unicode_string(&self) -> bool {
        self.has_authored_extended_unicode_string
    }

    pub(crate) fn modeled_comments(&self) -> Option<&[CommentTrivia]> {
        (!self.comments.is_empty()
            && (self.has_authored_no_substitution_template && self.template_products_supported
                || self.has_authored_extended_unicode_string
                    && self.extended_unicode_string_products_supported
                || self.has_authored_regular_expression
                    && self.regular_expression_products_supported))
            .then_some(self.comments.as_slice())
    }

    #[must_use]
    pub(crate) const fn has_unicode_line_comment_terminator(&self) -> bool {
        self.has_unicode_line_comment_terminator
    }

    /// Whether template syntax outside the exact no-substitution expression
    /// slice would require an AST, semantic, or emit product TSZ does not own.
    #[must_use]
    pub const fn has_unmodeled_template_products(&self) -> bool {
        !self.template_products_supported
    }

    #[must_use]
    pub const fn has_unmodeled_extended_unicode_string_products(&self) -> bool {
        !self.extended_unicode_string_products_supported
    }

    #[must_use]
    pub(crate) const fn has_authored_regular_expression(&self) -> bool {
        self.has_authored_regular_expression
    }

    #[must_use]
    pub(crate) const fn has_unmodeled_regular_expression_products(&self) -> bool {
        !self.regular_expression_products_supported
    }

    #[must_use]
    pub(crate) const fn has_authored_numeric_recovery(&self) -> bool {
        self.has_authored_numeric_recovery
    }

    #[must_use]
    pub(crate) const fn has_unmodeled_numeric_recovery_products(&self) -> bool {
        !self.numeric_recovery_products_supported
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

impl Statement {
    fn contains_recovered_type_members(&self) -> bool {
        match &self.kind {
            StatementKind::Import(_)
            | StatementKind::Break(_)
            | StatementKind::Continue(_)
            | StatementKind::Empty
            | StatementKind::Unknown => false,
            StatementKind::Export(declaration) => declaration
                .assignment
                .as_ref()
                .is_some_and(Expression::contains_recovered_type_members),
            StatementKind::Variable(declaration) => {
                declaration
                    .annotation
                    .as_ref()
                    .is_some_and(TypeNode::contains_recovered_type_members)
                    || declaration
                        .initializer
                        .as_ref()
                        .is_some_and(Expression::contains_recovered_type_members)
            }
            StatementKind::Function(declaration) => {
                type_parameters_contain_recovery(&declaration.type_parameters)
                    || parameters_contain_recovery(&declaration.parameters)
                    || declaration
                        .return_type
                        .as_ref()
                        .is_some_and(TypeNode::contains_recovered_type_members)
                    || declaration
                        .body
                        .iter()
                        .any(Statement::contains_recovered_type_members)
            }
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
                        .any(class_member_contains_recovery)
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
                        .any(|member| member.recovered || member.contains_recovered_type_members())
            }
            StatementKind::If(statement) => {
                statement.condition.contains_recovered_type_members()
                    || statement.then_statement.contains_recovered_type_members()
                    || statement
                        .else_statement
                        .as_deref()
                        .is_some_and(Statement::contains_recovered_type_members)
            }
            StatementKind::Switch(statement) => {
                statement.expression.contains_recovered_type_members()
                    || statement.clauses.iter().any(|clause| {
                        matches!(
                            &clause.kind,
                            SwitchClauseKind::Case(expression)
                                if expression.contains_recovered_type_members()
                        ) || clause
                            .statements
                            .iter()
                            .any(Statement::contains_recovered_type_members)
                    })
            }
            StatementKind::Return(expression) => expression
                .as_ref()
                .is_some_and(Expression::contains_recovered_type_members),
            StatementKind::Block(statements) => statements
                .iter()
                .any(Statement::contains_recovered_type_members),
            StatementKind::Expression(expression) => expression.contains_recovered_type_members(),
        }
    }
}

fn class_member_contains_recovery(member: &ClassMember) -> bool {
    match &member.kind {
        ClassMemberKind::Constructor {
            parameters, body, ..
        } => {
            parameters_contain_recovery(parameters)
                || body.iter().any(Statement::contains_recovered_type_members)
        }
        ClassMemberKind::Property {
            annotation,
            initializer,
            ..
        } => {
            annotation
                .as_ref()
                .is_some_and(TypeNode::contains_recovered_type_members)
                || initializer
                    .as_ref()
                    .is_some_and(Expression::contains_recovered_type_members)
        }
        ClassMemberKind::Method {
            type_parameters,
            parameters,
            return_type,
            body,
            ..
        } => {
            type_parameters_contain_recovery(type_parameters)
                || parameters_contain_recovery(parameters)
                || return_type
                    .as_ref()
                    .is_some_and(TypeNode::contains_recovered_type_members)
                || body.iter().any(Statement::contains_recovered_type_members)
        }
    }
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
    pub annotation: Option<TypeNode>,
    pub initializer: Option<Expression>,
    pub exported: bool,
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
    pub span: Span,
    pub modifiers: ClassMemberModifiers,
    pub overload_completion_supported: bool,
    pub emit_products_supported: bool,
    pub kind: ClassMemberKind,
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
        return_type: Box<TypeNode>,
    },
    Constructor {
        id: NodeId,
        type_parameters: Vec<TypeParameterDeclaration>,
        parameters: Vec<Parameter>,
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
    /// Whether this written type contains a value-space `typeof` query.
    /// Function implementations need a symbol-kind-aware lookup filter for
    /// these positions; callers that do not own that filter fail closed.
    #[must_use]
    pub fn contains_type_query(&self) -> bool {
        match &self.kind {
            TypeNodeKind::TypeQuery { .. } => true,
            TypeNodeKind::Array(inner)
            | TypeNodeKind::KeyOf(inner)
            | TypeNodeKind::Readonly(inner)
            | TypeNodeKind::Parenthesized(inner) => inner.contains_type_query(),
            TypeNodeKind::Tuple(types)
            | TypeNodeKind::Union(types)
            | TypeNodeKind::Intersection(types) => types.iter().any(TypeNode::contains_type_query),
            TypeNodeKind::Object(members) => members.iter().any(TypeMember::contains_type_query),
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
                type_parameters.iter().any(type_parameter_contains_query)
                    || parameters.iter().any(parameter_contains_query)
                    || return_type.contains_type_query()
            }
            TypeNodeKind::Reference { arguments, .. } => {
                arguments.iter().any(TypeNode::contains_type_query)
            }
            TypeNodeKind::Infer { constraint, .. } => constraint
                .as_deref()
                .is_some_and(TypeNode::contains_type_query),
            TypeNodeKind::Predicate { ty, .. } => {
                ty.as_deref().is_some_and(TypeNode::contains_type_query)
            }
            TypeNodeKind::Conditional {
                check_type,
                extends_type,
                true_type,
                false_type,
            } => {
                check_type.contains_type_query()
                    || extends_type.contains_type_query()
                    || true_type.contains_type_query()
                    || false_type.contains_type_query()
            }
            TypeNodeKind::Mapped {
                constraint,
                name_type,
                value_type,
                members,
                ..
            } => {
                constraint.contains_type_query()
                    || name_type
                        .as_deref()
                        .is_some_and(TypeNode::contains_type_query)
                    || value_type.contains_type_query()
                    || members.iter().any(TypeMember::contains_type_query)
            }
            TypeNodeKind::IndexedAccess { object, index } => {
                object.contains_type_query() || index.contains_type_query()
            }
            TypeNodeKind::Keyword(_) | TypeNodeKind::Literal(_) | TypeNodeKind::Missing => false,
        }
    }

    /// Whether declaration/runtime recovery for this type escaped an authored
    /// `TypeElement` list. Emitters use this to block a host product until the
    /// enclosing declarator/parameter recovery is represented structurally.
    #[must_use]
    pub fn contains_recovered_type_members(&self) -> bool {
        match &self.kind {
            TypeNodeKind::Array(inner)
            | TypeNodeKind::KeyOf(inner)
            | TypeNodeKind::Readonly(inner)
            | TypeNodeKind::Parenthesized(inner) => inner.contains_recovered_type_members(),
            TypeNodeKind::Tuple(types)
            | TypeNodeKind::Union(types)
            | TypeNodeKind::Intersection(types) => {
                types.iter().any(TypeNode::contains_recovered_type_members)
            }
            TypeNodeKind::Object(members) => members
                .iter()
                .any(|member| member.recovered || member.contains_recovered_type_members()),
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
                type_parameters_contain_recovery(type_parameters)
                    || parameters.iter().any(parameter_contains_recovery)
                    || return_type.contains_recovered_type_members()
            }
            TypeNodeKind::Reference { arguments, .. } => arguments
                .iter()
                .any(TypeNode::contains_recovered_type_members),
            TypeNodeKind::Infer { constraint, .. } => constraint
                .as_deref()
                .is_some_and(TypeNode::contains_recovered_type_members),
            TypeNodeKind::Predicate { ty, .. } => ty
                .as_deref()
                .is_some_and(TypeNode::contains_recovered_type_members),
            TypeNodeKind::Conditional {
                check_type,
                extends_type,
                true_type,
                false_type,
            } => {
                check_type.contains_recovered_type_members()
                    || extends_type.contains_recovered_type_members()
                    || true_type.contains_recovered_type_members()
                    || false_type.contains_recovered_type_members()
            }
            TypeNodeKind::Mapped {
                constraint,
                name_type,
                value_type,
                members,
                ..
            } => {
                constraint.contains_recovered_type_members()
                    || name_type
                        .as_deref()
                        .is_some_and(TypeNode::contains_recovered_type_members)
                    || value_type.contains_recovered_type_members()
                    || members
                        .iter()
                        .any(|member| member.recovered || member.contains_recovered_type_members())
            }
            TypeNodeKind::IndexedAccess { object, index } => {
                object.contains_recovered_type_members() || index.contains_recovered_type_members()
            }
            TypeNodeKind::Keyword(_)
            | TypeNodeKind::Literal(_)
            | TypeNodeKind::TypeQuery { .. }
            | TypeNodeKind::Missing => false,
        }
    }
}

impl TypeMember {
    fn contains_type_query(&self) -> bool {
        if self.recovered {
            return false;
        }
        match &self.kind {
            TypeMemberKind::Property { ty, .. } => {
                ty.as_ref().is_some_and(TypeNode::contains_type_query)
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
                type_parameters.iter().any(type_parameter_contains_query)
                    || parameters.iter().any(parameter_contains_query)
                    || return_type
                        .as_ref()
                        .is_some_and(TypeNode::contains_type_query)
            }
            TypeMemberKind::Accessor {
                parameters,
                return_type,
                ..
            } => {
                parameters.iter().any(parameter_contains_query)
                    || return_type
                        .as_ref()
                        .is_some_and(TypeNode::contains_type_query)
            }
            TypeMemberKind::Index {
                parameters,
                value_type,
            } => {
                parameters.iter().any(parameter_contains_query)
                    || value_type
                        .as_ref()
                        .is_some_and(TypeNode::contains_type_query)
            }
        }
    }

    fn contains_recovered_type_members(&self) -> bool {
        if self.recovered {
            return true;
        }
        let name_contains_recovery = match &self.kind {
            TypeMemberKind::Property { name, .. }
            | TypeMemberKind::Method { name, .. }
            | TypeMemberKind::Accessor { name, .. } => matches!(
                &name.kind,
                TypeMemberNameKind::Computed(expression)
                    if expression.contains_recovered_type_members()
            ),
            TypeMemberKind::Call { .. }
            | TypeMemberKind::Construct { .. }
            | TypeMemberKind::Index { .. } => false,
        };
        if name_contains_recovery {
            return true;
        }
        match &self.kind {
            TypeMemberKind::Property {
                ty, initializer, ..
            } => {
                ty.as_ref()
                    .is_some_and(TypeNode::contains_recovered_type_members)
                    || initializer
                        .as_ref()
                        .is_some_and(Expression::contains_recovered_type_members)
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
                ..
            }
            | TypeMemberKind::Construct {
                type_parameters,
                parameters,
                return_type,
                ..
            } => {
                type_parameters_contain_recovery(type_parameters)
                    || parameters.iter().any(parameter_contains_recovery)
                    || return_type
                        .as_ref()
                        .is_some_and(TypeNode::contains_recovered_type_members)
            }
            TypeMemberKind::Accessor {
                parameters,
                return_type,
                ..
            } => {
                parameters.iter().any(parameter_contains_recovery)
                    || return_type
                        .as_ref()
                        .is_some_and(TypeNode::contains_recovered_type_members)
            }
            TypeMemberKind::Index {
                parameters,
                value_type,
            } => {
                parameters.iter().any(parameter_contains_recovery)
                    || value_type
                        .as_ref()
                        .is_some_and(TypeNode::contains_recovered_type_members)
            }
        }
    }
}

fn type_parameter_contains_query(parameter: &TypeParameterDeclaration) -> bool {
    parameter
        .constraint
        .as_ref()
        .is_some_and(TypeNode::contains_type_query)
        || parameter
            .default
            .as_ref()
            .is_some_and(TypeNode::contains_type_query)
}

fn parameter_contains_query(parameter: &Parameter) -> bool {
    parameter
        .annotation
        .as_ref()
        .is_some_and(TypeNode::contains_type_query)
}

fn parameter_contains_recovery(parameter: &Parameter) -> bool {
    parameter
        .annotation
        .as_ref()
        .is_some_and(TypeNode::contains_recovered_type_members)
        || parameter
            .initializer
            .as_ref()
            .is_some_and(Expression::contains_recovered_type_members)
}

fn parameters_contain_recovery(parameters: &[Parameter]) -> bool {
    parameters.iter().any(parameter_contains_recovery)
}

fn type_parameters_contain_recovery(parameters: &[TypeParameterDeclaration]) -> bool {
    parameters.iter().any(|parameter| {
        parameter
            .constraint
            .as_ref()
            .is_some_and(TypeNode::contains_recovered_type_members)
            || parameter
                .default
                .as_ref()
                .is_some_and(TypeNode::contains_recovered_type_members)
    })
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
    Arrow {
        parameters: Vec<Parameter>,
        return_type: Option<TypeNode>,
        body: ArrowBody,
    },
    Binary {
        left: Box<Expression>,
        operator: BinaryOperator,
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

impl Expression {
    #[must_use]
    pub fn contains_recovered_type_members(&self) -> bool {
        match &self.kind {
            ExpressionKind::Identifier { .. }
            | ExpressionKind::Literal(_)
            | ExpressionKind::RegularExpression(_)
            | ExpressionKind::Missing => false,
            ExpressionKind::Object(properties) => properties
                .iter()
                .any(|property| property.value.contains_recovered_type_members()),
            ExpressionKind::Array(elements) => elements
                .iter()
                .any(Expression::contains_recovered_type_members),
            ExpressionKind::Call {
                callee,
                type_arguments,
                arguments,
            } => {
                callee.contains_recovered_type_members()
                    || type_arguments
                        .iter()
                        .flatten()
                        .any(TypeNode::contains_recovered_type_members)
                    || arguments
                        .iter()
                        .any(Expression::contains_recovered_type_members)
            }
            ExpressionKind::New {
                callee,
                type_arguments,
                arguments,
            } => {
                callee.contains_recovered_type_members()
                    || type_arguments
                        .iter()
                        .any(TypeNode::contains_recovered_type_members)
                    || arguments
                        .iter()
                        .any(Expression::contains_recovered_type_members)
            }
            ExpressionKind::Member { object, .. }
            | ExpressionKind::Unary {
                operand: object, ..
            }
            | ExpressionKind::Parenthesized(object) => object.contains_recovered_type_members(),
            ExpressionKind::Arrow {
                parameters,
                return_type,
                body,
            } => {
                parameters_contain_recovery(parameters)
                    || return_type
                        .as_ref()
                        .is_some_and(TypeNode::contains_recovered_type_members)
                    || match body {
                        ArrowBody::Expression(expression) => {
                            expression.contains_recovered_type_members()
                        }
                        ArrowBody::Block(statements) => statements
                            .iter()
                            .any(Statement::contains_recovered_type_members),
                    }
            }
            ExpressionKind::Binary { left, right, .. }
            | ExpressionKind::Assignment { left, right } => {
                left.contains_recovered_type_members() || right.contains_recovered_type_members()
            }
            ExpressionKind::As { expression, ty } => {
                expression.contains_recovered_type_members() || ty.contains_recovered_type_members()
            }
        }
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
