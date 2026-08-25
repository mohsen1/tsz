use super::{
    ArrowBody, ClassDeclaration, ClassMember, ClassMemberKind, Expression, ExpressionKind,
    FunctionDeclaration, FunctionLikeExpression, FunctionLikeSyntax, Parameter, Statement,
    StatementKind, SwitchClauseKind,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NestedStatement {
    Handled,
    Descend,
}

pub(crate) enum DescendantContainer<'ast> {
    Statement(&'ast Statement),
    Function(&'ast Statement, &'ast FunctionDeclaration),
    Class(&'ast Statement, &'ast ClassDeclaration),
    ClassMember(&'ast ClassMember),
    FunctionLike(&'ast Expression, &'ast FunctionLikeExpression),
}

/// Adapter over authored statement and expression edges. The syntax walker
/// owns source order; consumers own context transitions and subtree handling.
pub(crate) trait DescendantAdapter<'ast> {
    type Context;

    fn context(
        &mut self,
        context: &Self::Context,
        container: DescendantContainer<'ast>,
    ) -> Self::Context;

    fn nested_statement(
        &mut self,
        context: &Self::Context,
        statement: &'ast Statement,
        next_statement: Option<&'ast Statement>,
    ) -> NestedStatement;

    fn function_like(
        &mut self,
        context: &Self::Context,
        expression: &'ast Expression,
        function: &'ast FunctionLikeExpression,
    );

    fn expression(&mut self, _context: &Self::Context, _expression: &'ast Expression) {}

    fn identifier(&mut self, _context: &Self::Context, _expression: &'ast Expression) {}
}

pub(crate) fn walk_statement_descendants<'ast, A>(
    adapter: &mut A,
    context: &A::Context,
    statement: &'ast Statement,
) where
    A: DescendantAdapter<'ast>,
{
    match &statement.kind {
        StatementKind::Import(_)
        | StatementKind::TypeAlias(_)
        | StatementKind::Interface(_)
        | StatementKind::Break(_)
        | StatementKind::Continue(_)
        | StatementKind::Empty
        | StatementKind::Unknown => {}
        StatementKind::Export(declaration) => {
            if let Some(expression) = &declaration.assignment {
                walk_expression_descendants(adapter, context, expression);
            }
        }
        StatementKind::Variable(declaration) => {
            if let Some(initializer) = &declaration.initializer {
                walk_expression_descendants(adapter, context, initializer);
            }
        }
        StatementKind::Function(declaration) => {
            let context = adapter.context(
                context,
                DescendantContainer::Function(statement, declaration),
            );
            walk_parameter_initializers(adapter, &context, &declaration.parameters);
            walk_statement_list(adapter, &context, &declaration.body);
        }
        StatementKind::Class(declaration) => {
            let class_context =
                adapter.context(context, DescendantContainer::Class(statement, declaration));
            walk_class_descendants(adapter, &class_context, declaration);
        }
        StatementKind::If(control_flow) => {
            walk_expression_descendants(adapter, context, &control_flow.condition);
            walk_nested_statement(adapter, context, &control_flow.then_statement, None);
            if let Some(statement) = &control_flow.else_statement {
                walk_nested_statement(adapter, context, statement, None);
            }
        }
        StatementKind::Switch(control_flow) => {
            let switch_context =
                adapter.context(context, DescendantContainer::Statement(statement));
            walk_expression_descendants(adapter, &switch_context, &control_flow.expression);
            for clause in &control_flow.clauses {
                if let SwitchClauseKind::Case(expression) = &clause.kind {
                    walk_expression_descendants(adapter, &switch_context, expression);
                }
                walk_statement_list(adapter, &switch_context, &clause.statements);
            }
        }
        StatementKind::Return(expression) => {
            if let Some(expression) = expression {
                walk_expression_descendants(adapter, context, expression);
            }
        }
        StatementKind::Block(statements) => walk_statement_list(adapter, context, statements),
        StatementKind::Expression(expression) => {
            walk_expression_descendants(adapter, context, expression);
        }
    }
}

pub(crate) fn walk_class_descendants<'ast, A>(
    adapter: &mut A,
    context: &A::Context,
    declaration: &'ast ClassDeclaration,
) where
    A: DescendantAdapter<'ast>,
{
    for member in &declaration.members {
        match &member.kind {
            ClassMemberKind::Constructor {
                parameters, body, ..
            }
            | ClassMemberKind::Method {
                parameters, body, ..
            } => {
                let context = adapter.context(context, DescendantContainer::ClassMember(member));
                walk_parameter_initializers(adapter, &context, parameters);
                walk_statement_list(adapter, &context, body);
            }
            ClassMemberKind::Property { initializer, .. } => {
                if let Some(initializer) = initializer {
                    walk_expression_descendants(adapter, context, initializer);
                }
            }
        }
    }
}

pub(crate) fn walk_function_like_descendants<'ast, A>(
    adapter: &mut A,
    context: &A::Context,
    expression: &'ast Expression,
    function: &'ast FunctionLikeExpression,
) where
    A: DescendantAdapter<'ast>,
{
    let context = adapter.context(
        context,
        DescendantContainer::FunctionLike(expression, function),
    );
    walk_parameter_initializers(adapter, &context, &function.parameters);
    match &function.syntax {
        FunctionLikeSyntax::Arrow(ArrowBody::Expression(body)) => {
            walk_expression_descendants(adapter, &context, body);
        }
        FunctionLikeSyntax::Arrow(ArrowBody::Block(statements))
        | FunctionLikeSyntax::Function {
            body: statements, ..
        } => walk_statement_list(adapter, &context, statements),
    }
}

pub(crate) fn walk_expression_descendants<'ast, A>(
    adapter: &mut A,
    context: &A::Context,
    expression: &'ast Expression,
) where
    A: DescendantAdapter<'ast>,
{
    adapter.expression(context, expression);
    match &expression.kind {
        ExpressionKind::Identifier { .. } => adapter.identifier(context, expression),
        ExpressionKind::This
        | ExpressionKind::Literal(_)
        | ExpressionKind::RegularExpression(_)
        | ExpressionKind::Missing => {}
        ExpressionKind::Object(properties) => {
            for property in properties {
                walk_expression_descendants(adapter, context, &property.value);
            }
        }
        ExpressionKind::Array(elements) => {
            for element in elements {
                walk_expression_descendants(adapter, context, element);
            }
        }
        ExpressionKind::Call {
            callee, arguments, ..
        }
        | ExpressionKind::New {
            callee, arguments, ..
        } => {
            walk_expression_descendants(adapter, context, callee);
            for argument in arguments {
                walk_expression_descendants(adapter, context, argument);
            }
        }
        ExpressionKind::Member { object, .. }
        | ExpressionKind::Unary {
            operand: object, ..
        }
        | ExpressionKind::Parenthesized(object)
        | ExpressionKind::As {
            expression: object, ..
        } => walk_expression_descendants(adapter, context, object),
        ExpressionKind::ElementAccess { object, index } => {
            walk_expression_descendants(adapter, context, object);
            walk_expression_descendants(adapter, context, index);
        }
        ExpressionKind::FunctionLike(function) => {
            adapter.function_like(context, expression, function);
        }
        ExpressionKind::Binary { left, right, .. }
        | ExpressionKind::Assignment { left, right, .. } => {
            walk_expression_descendants(adapter, context, left);
            walk_expression_descendants(adapter, context, right);
        }
    }
}

fn walk_parameter_initializers<'ast, A>(
    adapter: &mut A,
    context: &A::Context,
    parameters: &'ast [Parameter],
) where
    A: DescendantAdapter<'ast>,
{
    for parameter in parameters {
        if let Some(initializer) = &parameter.initializer {
            walk_expression_descendants(adapter, context, initializer);
        }
    }
}

fn walk_statement_list<'ast, A>(
    adapter: &mut A,
    context: &A::Context,
    statements: &'ast [Statement],
) where
    A: DescendantAdapter<'ast>,
{
    for (index, statement) in statements.iter().enumerate() {
        walk_nested_statement(adapter, context, statement, statements.get(index + 1));
    }
}

fn walk_nested_statement<'ast, A>(
    adapter: &mut A,
    context: &A::Context,
    statement: &'ast Statement,
    next_statement: Option<&'ast Statement>,
) where
    A: DescendantAdapter<'ast>,
{
    let context = adapter.context(context, DescendantContainer::Statement(statement));
    if adapter.nested_statement(&context, statement, next_statement) == NestedStatement::Descend {
        walk_statement_descendants(adapter, &context, statement);
    }
}

struct StatementVisitor<'visit, F, P> {
    visit: &'visit mut F,
    descend: &'visit mut P,
}

impl<'ast, F, P> DescendantAdapter<'ast> for StatementVisitor<'_, F, P>
where
    F: FnMut(&'ast Statement),
    P: FnMut(DescendantContainer<'ast>) -> bool,
{
    type Context = bool;

    fn context(
        &mut self,
        context: &Self::Context,
        container: DescendantContainer<'ast>,
    ) -> Self::Context {
        *context && (self.descend)(container)
    }

    fn nested_statement(
        &mut self,
        context: &Self::Context,
        statement: &'ast Statement,
        _next_statement: Option<&'ast Statement>,
    ) -> NestedStatement {
        if !context {
            return NestedStatement::Handled;
        }
        (self.visit)(statement);
        NestedStatement::Descend
    }

    fn function_like(
        &mut self,
        context: &Self::Context,
        expression: &'ast Expression,
        function: &'ast FunctionLikeExpression,
    ) {
        if *context {
            walk_function_like_descendants(self, context, expression, function);
        }
    }
}

impl Statement {
    pub(crate) fn for_each_statement<'ast>(&'ast self, visit: &mut impl FnMut(&'ast Statement)) {
        self.for_each_statement_where(&mut |_| true, visit);
    }

    pub(crate) fn for_each_statement_where<'ast>(
        &'ast self,
        descend: &mut impl FnMut(DescendantContainer<'ast>) -> bool,
        visit: &mut impl FnMut(&'ast Statement),
    ) {
        visit(self);
        let mut adapter = StatementVisitor { visit, descend };
        walk_statement_descendants(&mut adapter, &true, self);
    }
}

pub(crate) fn for_each_statement_in<'ast>(
    statements: &'ast [Statement],
    visit: &mut impl FnMut(&'ast Statement),
) {
    statements
        .iter()
        .for_each(|statement| statement.for_each_statement(visit));
}

/// Whether an expression query enters declaration and function-like bodies.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExpressionTraversal {
    All,
    Executed,
}

pub(crate) enum ExpressionRoot<'ast> {
    Expression(&'ast Expression),
    Statements(&'ast [Statement]),
    Class(&'ast ClassDeclaration),
}

struct ExpressionFinder<'visit, F> {
    traversal: ExpressionTraversal,
    predicate: &'visit mut F,
    found: bool,
}

impl<'ast, F> DescendantAdapter<'ast> for ExpressionFinder<'_, F>
where
    F: FnMut(&'ast Expression) -> bool,
{
    type Context = bool;

    fn context(
        &mut self,
        context: &Self::Context,
        container: DescendantContainer<'ast>,
    ) -> Self::Context {
        !self.found
            && *context
            && match container {
                DescendantContainer::Function(_, _)
                | DescendantContainer::Class(_, _)
                | DescendantContainer::FunctionLike(_, _) => {
                    self.traversal == ExpressionTraversal::All
                }
                DescendantContainer::Statement(_) | DescendantContainer::ClassMember(_) => true,
            }
    }

    fn nested_statement(
        &mut self,
        context: &Self::Context,
        _statement: &'ast Statement,
        _next_statement: Option<&'ast Statement>,
    ) -> NestedStatement {
        if *context {
            NestedStatement::Descend
        } else {
            NestedStatement::Handled
        }
    }

    fn function_like(
        &mut self,
        context: &Self::Context,
        expression: &'ast Expression,
        function: &'ast FunctionLikeExpression,
    ) {
        if *context {
            walk_function_like_descendants(self, context, expression, function);
        }
    }

    fn expression(&mut self, context: &Self::Context, expression: &'ast Expression) {
        self.found |= *context && (self.predicate)(expression);
    }
}

pub(crate) fn contains_matching_expression<'ast>(
    root: ExpressionRoot<'ast>,
    traversal: ExpressionTraversal,
    mut predicate: impl FnMut(&'ast Expression) -> bool,
) -> bool {
    let mut finder = ExpressionFinder {
        traversal,
        predicate: &mut predicate,
        found: false,
    };
    match root {
        ExpressionRoot::Expression(expression) => {
            walk_expression_descendants(&mut finder, &true, expression)
        }
        ExpressionRoot::Statements(statements) => {
            for statement in statements {
                walk_statement_descendants(&mut finder, &true, statement);
            }
        }
        ExpressionRoot::Class(declaration) => {
            walk_class_descendants(&mut finder, &true, declaration)
        }
    }
    finder.found
}
