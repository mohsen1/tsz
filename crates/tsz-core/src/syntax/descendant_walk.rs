use super::{
    ClassDeclaration, ClassMember, ClassMemberKind, Expression, ExpressionKind,
    FunctionDeclaration, FunctionLikeBody, FunctionLikeExpression, Parameter, Statement,
    StatementKind, SwitchClauseKind, TypeNode,
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

pub(crate) enum ExpressionEdge<'ast> {
    AssignmentRight(&'ast Expression),
    PropertyInitializer(&'ast ClassMember),
}

/// Adapter over authored statement and expression edges. The syntax walker
/// owns source order; consumers own context transitions and subtree handling.
pub(crate) trait DescendantAdapter<'ast> {
    type Context: Clone;

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

    fn expression_edge(
        &mut self,
        context: &Self::Context,
        _edge: ExpressionEdge<'ast>,
    ) -> Self::Context {
        context.clone()
    }

    fn type_node(&mut self, _context: &Self::Context, _node: &'ast TypeNode) {}

    fn class_member(
        &mut self,
        context: &Self::Context,
        member: &'ast ClassMember,
    ) -> Self::Context {
        match member.kind {
            ClassMemberKind::Constructor { .. } | ClassMemberKind::Method { .. } => {
                self.context(context, DescendantContainer::ClassMember(member))
            }
            ClassMemberKind::Property { .. } => context.clone(),
        }
    }

    fn fold_context(&mut self, context: &Self::Context, _nested: &Self::Context) -> Self::Context {
        context.clone()
    }

    /// Structural adapters recurse through function-like children by default.
    fn function_like(
        &mut self,
        context: &Self::Context,
        expression: &'ast Expression,
        function: &'ast FunctionLikeExpression,
    ) {
        walk_function_like_descendants(self, context, expression, function);
    }

    fn expression(&mut self, _context: &Self::Context, _expression: &'ast Expression) {}

    fn identifier(&mut self, _context: &Self::Context, _expression: &'ast Expression) {}
}

pub(crate) fn walk_statement_descendants<'ast, A>(
    adapter: &mut A,
    context: &A::Context,
    statement: &'ast Statement,
) -> A::Context
where
    A: DescendantAdapter<'ast> + ?Sized,
{
    match &statement.kind {
        StatementKind::Import(_)
        | StatementKind::TypeAlias(_)
        | StatementKind::Interface(_)
        | StatementKind::Break(_)
        | StatementKind::Continue(_)
        | StatementKind::Empty
        | StatementKind::Unknown => context.clone(),
        StatementKind::Export(declaration) => {
            if let Some(expression) = &declaration.assignment {
                walk_expression_descendants(adapter, context, expression);
            }
            context.clone()
        }
        StatementKind::Variable(statement) => {
            for declarator in &statement.declarators {
                if let Some(annotation) = &declarator.annotation {
                    adapter.type_node(context, annotation);
                }
                if let Some(initializer) = &declarator.initializer {
                    walk_expression_descendants(adapter, context, initializer);
                }
            }
            context.clone()
        }
        StatementKind::Function(declaration) => {
            let function_context = adapter.context(
                context,
                DescendantContainer::Function(statement, declaration),
            );
            walk_parameter_initializers(adapter, &function_context, &declaration.parameters);
            walk_statement_list(adapter, &function_context, &declaration.body);
            context.clone()
        }
        StatementKind::Class(declaration) => {
            let class_context =
                adapter.context(context, DescendantContainer::Class(statement, declaration));
            walk_class_descendants(adapter, &class_context, declaration);
            context.clone()
        }
        StatementKind::If(control_flow) => {
            walk_expression_descendants(adapter, context, &control_flow.condition);
            let then_context =
                walk_nested_statement(adapter, context, &control_flow.then_statement, None);
            let else_context = control_flow.else_statement.as_deref().map_or_else(
                || context.clone(),
                |statement| walk_nested_statement(adapter, context, statement, None),
            );
            let context = adapter.fold_context(context, &then_context);
            adapter.fold_context(&context, &else_context)
        }
        StatementKind::Switch(control_flow) => {
            let mut switch_context =
                adapter.context(context, DescendantContainer::Statement(statement));
            walk_expression_descendants(adapter, &switch_context, &control_flow.expression);
            for clause in &control_flow.clauses {
                if let SwitchClauseKind::Case(expression) = &clause.kind {
                    walk_expression_descendants(adapter, &switch_context, expression);
                }
                switch_context = walk_statement_list(adapter, &switch_context, &clause.statements);
            }
            switch_context
        }
        StatementKind::Return(expression) => {
            if let Some(expression) = expression {
                walk_expression_descendants(adapter, context, expression);
            }
            context.clone()
        }
        StatementKind::Block(statements) => walk_statement_list(adapter, context, statements),
        StatementKind::Expression(expression) => {
            walk_expression_descendants(adapter, context, expression);
            context.clone()
        }
    }
}

pub(crate) fn walk_class_descendants<'ast, A>(
    adapter: &mut A,
    context: &A::Context,
    declaration: &'ast ClassDeclaration,
) where
    A: DescendantAdapter<'ast> + ?Sized,
{
    for member in &declaration.members {
        let context = adapter.class_member(context, member);
        match &member.kind {
            ClassMemberKind::Constructor {
                parameters, body, ..
            }
            | ClassMemberKind::Method {
                parameters, body, ..
            } => {
                walk_parameter_initializers(adapter, &context, parameters);
                walk_statement_list(adapter, &context, body);
            }
            ClassMemberKind::Property { initializer, .. } => {
                if let Some(initializer) = initializer {
                    let context = adapter
                        .expression_edge(&context, ExpressionEdge::PropertyInitializer(member));
                    walk_expression_descendants(adapter, &context, initializer);
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
    A: DescendantAdapter<'ast> + ?Sized,
{
    let context = adapter.context(
        context,
        DescendantContainer::FunctionLike(expression, function),
    );
    walk_parameter_initializers(adapter, &context, &function.parameters);
    match function.syntax.body() {
        FunctionLikeBody::Expression(body) => walk_expression_descendants(adapter, &context, body),
        FunctionLikeBody::Statements(body) => {
            walk_statement_list(adapter, &context, body);
        }
    }
}

pub(crate) fn walk_expression_descendants<'ast, A>(
    adapter: &mut A,
    context: &A::Context,
    expression: &'ast Expression,
) where
    A: DescendantAdapter<'ast> + ?Sized,
{
    adapter.expression(context, expression);
    match &expression.kind {
        ExpressionKind::Identifier { .. } => adapter.identifier(context, expression),
        ExpressionKind::This
        | ExpressionKind::Literal(_)
        | ExpressionKind::RegularExpression(_)
        | ExpressionKind::Missing => {}
        ExpressionKind::Template(template) => {
            for span in &template.spans {
                walk_expression_descendants(adapter, context, &span.expression);
            }
        }
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
        } => {
            walk_expression_descendants(adapter, context, callee);
            for argument in arguments {
                walk_expression_descendants(adapter, context, argument);
            }
        }
        ExpressionKind::New {
            callee,
            type_arguments,
            arguments,
            ..
        } => {
            walk_expression_descendants(adapter, context, callee);
            for argument in type_arguments {
                adapter.type_node(context, argument);
            }
            for argument in arguments {
                walk_expression_descendants(adapter, context, argument);
            }
        }
        ExpressionKind::Member { object, .. }
        | ExpressionKind::NonNull(object)
        | ExpressionKind::Unary {
            operand: object, ..
        }
        | ExpressionKind::Parenthesized(object) => {
            walk_expression_descendants(adapter, context, object);
        }
        ExpressionKind::As { expression, ty } => {
            walk_expression_descendants(adapter, context, expression);
            adapter.type_node(context, ty);
        }
        ExpressionKind::ElementAccess { object, index } => {
            walk_expression_descendants(adapter, context, object);
            walk_expression_descendants(adapter, context, index);
        }
        ExpressionKind::FunctionLike(function) => {
            adapter.function_like(context, expression, function);
        }
        ExpressionKind::Binary { left, right, .. } => {
            walk_expression_descendants(adapter, context, left);
            walk_expression_descendants(adapter, context, right);
        }
        ExpressionKind::Assignment { left, right, .. } => {
            walk_expression_descendants(adapter, context, left);
            let context =
                adapter.expression_edge(context, ExpressionEdge::AssignmentRight(expression));
            walk_expression_descendants(adapter, &context, right);
        }
    }
}

fn walk_parameter_initializers<'ast, A>(
    adapter: &mut A,
    context: &A::Context,
    parameters: &'ast [Parameter],
) where
    A: DescendantAdapter<'ast> + ?Sized,
{
    for parameter in parameters {
        if let Some(initializer) = &parameter.initializer {
            walk_expression_descendants(adapter, context, initializer);
        }
    }
}

pub(crate) fn walk_statement_list<'ast, A>(
    adapter: &mut A,
    context: &A::Context,
    statements: &'ast [Statement],
) -> A::Context
where
    A: DescendantAdapter<'ast> + ?Sized,
{
    statements
        .iter()
        .enumerate()
        .fold(context.clone(), |context, (index, statement)| {
            walk_nested_statement(adapter, &context, statement, statements.get(index + 1))
        })
}

fn walk_nested_statement<'ast, A>(
    adapter: &mut A,
    context: &A::Context,
    statement: &'ast Statement,
    next_statement: Option<&'ast Statement>,
) -> A::Context
where
    A: DescendantAdapter<'ast> + ?Sized,
{
    let parent = context;
    let local = adapter.context(parent, DescendantContainer::Statement(statement));
    let nested = if adapter.nested_statement(&local, statement, next_statement)
        == NestedStatement::Descend
    {
        walk_statement_descendants(adapter, &local, statement)
    } else {
        local.clone()
    };
    adapter.fold_context(parent, &nested)
}

struct ClosureAdapter<'visit, C, S, E> {
    container: &'visit mut C,
    statement: &'visit mut S,
    expression: &'visit mut E,
}

impl<'ast, C, S, E> DescendantAdapter<'ast> for ClosureAdapter<'_, C, S, E>
where
    C: FnMut(DescendantContainer<'ast>) -> bool,
    S: FnMut(&'ast Statement),
    E: FnMut(&'ast Expression),
{
    type Context = bool;

    fn context(&mut self, context: &bool, container: DescendantContainer<'ast>) -> bool {
        *context && (self.container)(container)
    }

    fn nested_statement(
        &mut self,
        context: &bool,
        statement: &'ast Statement,
        _next_statement: Option<&'ast Statement>,
    ) -> NestedStatement {
        if !context {
            return NestedStatement::Handled;
        }
        (self.statement)(statement);
        NestedStatement::Descend
    }

    fn function_like(
        &mut self,
        context: &bool,
        expression: &'ast Expression,
        function: &'ast FunctionLikeExpression,
    ) {
        if *context {
            walk_function_like_descendants(self, context, expression, function);
        }
    }

    fn expression(&mut self, context: &bool, expression: &'ast Expression) {
        if *context {
            (self.expression)(expression);
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
        let mut ignore = |_| {};
        let mut adapter = ClosureAdapter {
            container: descend,
            statement: visit,
            expression: &mut ignore,
        };
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
    Statement(&'ast Statement),
    Statements(&'ast [Statement]),
}

pub(crate) fn contains_matching_expression<'ast>(
    root: ExpressionRoot<'ast>,
    traversal: ExpressionTraversal,
    mut predicate: impl FnMut(&'ast Expression) -> bool,
) -> bool {
    let found = std::cell::Cell::new(false);
    let mut container = |container| {
        !found.get()
            && (traversal == ExpressionTraversal::All
                || !matches!(
                    container,
                    DescendantContainer::Function(_, _)
                        | DescendantContainer::Class(_, _)
                        | DescendantContainer::FunctionLike(_, _)
                ))
    };
    let mut statement = |_| {};
    let mut expression = |candidate| {
        if !found.get() && predicate(candidate) {
            found.set(true);
        }
    };
    let mut finder = ClosureAdapter {
        container: &mut container,
        statement: &mut statement,
        expression: &mut expression,
    };
    match root {
        ExpressionRoot::Expression(expression) => {
            walk_expression_descendants(&mut finder, &true, expression)
        }
        ExpressionRoot::Statement(statement) => {
            walk_statement_descendants(&mut finder, &true, statement);
        }
        ExpressionRoot::Statements(statements) => {
            for statement in statements {
                walk_statement_descendants(&mut finder, &true, statement);
            }
        }
    }
    found.get()
}
